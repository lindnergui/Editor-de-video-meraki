use glib::filename_to_uri;
use gstreamer as gst;
use gstreamer_pbutils as gst_pbutils;
use gstreamer_pbutils::prelude::*;
use meraki_core::{AssetId, MediaMetadata};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Duração padrão aplicada a imagens estáticas importadas (5 segundos em nanossegundos).
pub const DEFAULT_IMAGE_DURATION_NS: u64 = 5_000_000_000;

/// Erros estruturados durante inspeção técnica de arquivos de mídia.
#[derive(Debug, Error)]
pub enum DiscovererError {
    #[error("falha de E/S ao acessar '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("caminho '{0}' não corresponde a um arquivo regular")]
    NotAFile(PathBuf),
    #[error("não foi possível converter '{path}' em URI válida: {reason}")]
    InvalidUri { path: PathBuf, reason: String },
    #[error("falha ao criar GstDiscoverer: {0}")]
    InitDiscoverer(String),
    #[error("falha ao inspecionar '{path}': {reason}")]
    DiscoverUri { path: PathBuf, reason: String },
    #[error("descoberta de '{path}' terminou com estado {result}")]
    DiscoveryFailed { path: PathBuf, result: String },
    #[error("nenhum fluxo de áudio ou vídeo reconhecido em '{0}'")]
    NoMediaStreams(PathBuf),
    #[error("imagem inválida em '{path}': {reason}")]
    InvalidImage { path: PathBuf, reason: &'static str },
}

/// Converte um caminho de arquivo em URI `file://` compatível com GLib/GStreamer no Windows.
pub(crate) fn path_to_file_uri(path: &Path) -> Result<String, DiscovererError> {
    let abs_path = to_absolute_path(path).map_err(|source| DiscovererError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let uri = filename_to_uri(&abs_path, None).map_err(|err| DiscovererError::InvalidUri {
        path: abs_path,
        reason: err.to_string(),
    })?;
    Ok(uri.into())
}

/// Extrai metadados reais de mídia via `gstreamer_pbutils::Discoverer` e `std::fs::metadata`.
pub fn discover_media(asset_id: AssetId, path: &Path) -> Result<MediaMetadata, DiscovererError> {
    let fs_meta = fs::metadata(path).map_err(|source| DiscovererError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !fs_meta.is_file() {
        return Err(DiscovererError::NotAFile(path.to_path_buf()));
    }
    let file_size_bytes = fs_meta.len();

    let abs_path = to_absolute_path(path).map_err(|source| DiscovererError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let uri = path_to_file_uri(&abs_path)?;

    let discoverer = gst_pbutils::Discoverer::new(gst::ClockTime::from_seconds(10))
        .map_err(|err| DiscovererError::InitDiscoverer(err.to_string()))?;

    let info = match discoverer.discover_uri(uri.as_str()) {
        Ok(info) => info,
        Err(err) => {
            if is_webp_extension(&abs_path) {
                return discover_webp_fallback(asset_id, &abs_path, file_size_bytes);
            }
            return Err(DiscovererError::DiscoverUri {
                path: abs_path,
                reason: err.to_string(),
            });
        }
    };

    if info.result() != gst_pbutils::DiscovererResult::Ok {
        if is_webp_extension(&abs_path) {
            return discover_webp_fallback(asset_id, &abs_path, file_size_bytes);
        }
        return Err(DiscovererError::DiscoveryFailed {
            path: abs_path,
            result: format!("{:?}", info.result()),
        });
    }

    build_metadata(asset_id, &abs_path, file_size_bytes, &info)
}

fn build_metadata(
    asset_id: AssetId,
    path: &Path,
    file_size_bytes: u64,
    info: &gst_pbutils::DiscovererInfo,
) -> Result<MediaMetadata, DiscovererError> {
    let video_streams = info.video_streams();
    let audio_streams = info.audio_streams();

    if video_streams.is_empty() && audio_streams.is_empty() {
        return Err(DiscovererError::NoMediaStreams(path.to_path_buf()));
    }

    let is_static_image =
        is_static_image_extension(path) || video_streams.first().is_some_and(|v| v.is_image());

    let (width, height, fps_num, fps_den, video_codec) = match video_streams.first() {
        Some(video) => {
            let w = (video.width() > 0).then_some(video.width());
            let h = (video.height() > 0).then_some(video.height());
            let (fn_num, fn_den) = if is_static_image {
                (None, None)
            } else {
                extract_fps(video)
            };
            let codec = video
                .caps()
                .map(|caps| gst_pbutils::pb_utils_get_codec_description(&caps).to_string());
            (w, h, fn_num, fn_den, codec)
        }
        None => (None, None, None, None, None),
    };

    let has_audio = !audio_streams.is_empty();
    let (audio_channels, sample_rate_hz, audio_codec) = match audio_streams.first() {
        Some(audio) => {
            let channels = (audio.channels() > 0).then_some(audio.channels());
            let rate = (audio.sample_rate() > 0).then_some(audio.sample_rate());
            let codec = audio
                .caps()
                .map(|caps| gst_pbutils::pb_utils_get_codec_description(&caps).to_string());
            (channels, rate, codec)
        }
        None => (None, None, None),
    };

    let duration_ns = if is_static_image {
        DEFAULT_IMAGE_DURATION_NS
    } else {
        info.duration().map(|d| d.nseconds()).unwrap_or(0)
    };

    Ok(MediaMetadata {
        asset_id,
        duration_ns,
        width,
        height,
        fps_num,
        fps_den,
        has_audio,
        audio_channels,
        sample_rate_hz,
        video_codec,
        audio_codec,
        file_size_bytes,
    })
}

fn extract_fps(video: &gst_pbutils::DiscovererVideoInfo) -> (Option<u32>, Option<u32>) {
    let fps = video.framerate();
    match (u32::try_from(fps.numer()), u32::try_from(fps.denom())) {
        (Ok(num), Ok(den)) if num > 0 && den > 0 => (Some(num), Some(den)),
        _ => (None, None),
    }
}

fn to_absolute_path(path: &Path) -> std::io::Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(strip_windows_verbatim_prefix(&abs))
}

#[cfg(windows)]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").map_or_else(|| path.to_path_buf(), PathBuf::from)
}

#[cfg(not(windows))]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf { path.to_path_buf() }

fn extension_lower(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
}

fn is_static_image_extension(path: &Path) -> bool {
    matches!(extension_lower(path).as_deref(), Some("png" | "jpg" | "jpeg" | "webp"))
}

fn is_webp_extension(path: &Path) -> bool {
    matches!(extension_lower(path).as_deref(), Some("webp"))
}

fn discover_webp_fallback(asset_id: AssetId, path: &Path, file_size_bytes: u64) -> Result<MediaMetadata, DiscovererError> {
    let mut file = File::open(path).map_err(|source| DiscovererError::Io { path: path.to_path_buf(), source })?;
    let mut header = [0_u8; 64];
    let read_len = file.read(&mut header).map_err(|source| DiscovererError::Io { path: path.to_path_buf(), source })?;
    let (width, height) = parse_webp_dimensions(&header[..read_len])
        .ok_or_else(|| DiscovererError::InvalidImage { path: path.to_path_buf(), reason: "cabeçalho WebP ausente ou corrompido" })?;

    Ok(MediaMetadata {
        asset_id,
        duration_ns: DEFAULT_IMAGE_DURATION_NS,
        width: Some(width),
        height: Some(height),
        fps_num: None,
        fps_den: None,
        has_audio: false,
        audio_channels: None,
        sample_rate_hz: None,
        video_codec: Some("WebP".to_string()),
        audio_codec: None,
        file_size_bytes,
    })
}

fn parse_webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return None;
    }
    let chunk = bytes.get(12..16)?;
    let (w, h) = if chunk == b"VP8X" {
        let w_bytes = bytes.get(24..27)?;
        let h_bytes = bytes.get(27..30)?;
        let w = u32::from_le_bytes([w_bytes[0], w_bytes[1], w_bytes[2], 0]) + 1;
        let h = u32::from_le_bytes([h_bytes[0], h_bytes[1], h_bytes[2], 0]) + 1;
        (w, h)
    } else if chunk == b"VP8 " {
        if bytes.get(23..26) != Some(&[0x9D, 0x01, 0x2A]) {
            return None;
        }
        let w_raw = bytes.get(26..28)?;
        let h_raw = bytes.get(28..30)?;
        let w = u32::from(u16::from_le_bytes([w_raw[0], w_raw[1]]) & 0x3FFF);
        let h = u32::from(u16::from_le_bytes([h_raw[0], h_raw[1]]) & 0x3FFF);
        (w, h)
    } else if chunk == b"VP8L" {
        if *bytes.get(20)? != 0x2F {
            return None;
        }
        let bits_raw = bytes.get(21..25)?;
        let bits = u32::from_le_bytes([bits_raw[0], bits_raw[1], bits_raw[2], bits_raw[3]]);
        ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1)
    } else {
        return None;
    };
    (w > 0 && h > 0).then_some((w, h))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(crate) struct TempFileGuard(pub PathBuf);

    impl TempFileGuard {
        pub(crate) fn new(prefix: &str, ext: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let name = format!("meraki_{prefix}_{}_{nanos}.{ext}", std::process::id());
            Self(std::env::temp_dir().join(name))
        }
    }

    impl Drop for TempFileGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    pub(crate) fn write_test_wav_1s_stereo_48khz(path: &Path) {
        let sample_rate: u32 = 48_000;
        let channels: u16 = 2;
        let bits_per_sample: u16 = 16;
        let num_samples = sample_rate as usize;
        let data_bytes = (num_samples * channels as usize * (bits_per_sample as usize / 8)) as u32;
        let riff_size = 36 + data_bytes;
        let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample / 8);
        let block_align = channels * (bits_per_sample / 8);

        let mut buf = Vec::with_capacity(44 + data_bytes as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVEfmt ");
        buf.extend_from_slice(&16_u32.to_le_bytes());
        buf.extend_from_slice(&1_u16.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_bytes.to_le_bytes());
        buf.resize(44 + data_bytes as usize, 0);
        fs::write(path, buf).unwrap();
    }

    pub(crate) fn write_test_png_via_gst(path: &Path, width: u32, height: u32) {
        crate::ensure_media_initialized().unwrap();
        let desc = format!(
            "videotestsrc num-buffers=1 ! video/x-raw,width={width},height={height} ! pngenc ! filesink name=sink"
        );
        let elem = gst::parse::launch(&desc).unwrap();
        let pipeline = elem.downcast::<gst::Pipeline>().unwrap();
        let sink = pipeline.by_name("sink").unwrap();
        sink.set_property("location", path.to_string_lossy().as_ref());
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(10)) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(err) => panic!("erro ao gerar PNG de teste: {err:?}"),
                _ => {}
            }
        }
        pipeline.set_state(gst::State::Null).unwrap();
    }

    #[test]
    fn discover_media_rejects_missing_and_invalid_files() {
        crate::ensure_media_initialized().unwrap();
        let missing = TempFileGuard::new("missing", "mp4");
        assert!(discover_media(AssetId::new(), &missing.0).is_err());

        let invalid = TempFileGuard::new("invalid", "txt");
        fs::write(&invalid.0, b"nao e uma midia valida").unwrap();
        assert!(discover_media(AssetId::new(), &invalid.0).is_err());
    }

    #[test]
    fn discover_media_extracts_real_wav_metadata() {
        crate::ensure_media_initialized().unwrap();
        let wav = TempFileGuard::new("audio", "wav");
        write_test_wav_1s_stereo_48khz(&wav.0);

        let asset_id = AssetId::new();
        let meta = discover_media(asset_id, &wav.0).unwrap();
        assert_eq!(meta.asset_id, asset_id);
        assert_eq!(meta.duration_ns, 1_000_000_000);
        assert!(meta.has_audio);
        assert_eq!(meta.audio_channels, Some(2));
        assert_eq!(meta.sample_rate_hz, Some(48_000));
        assert_eq!(meta.width, None);
        assert_eq!(meta.file_size_bytes, 44 + 48_000 * 4);
    }

    #[test]
    fn discover_media_extracts_real_png_and_webp_with_5s_default() {
        crate::ensure_media_initialized().unwrap();
        let png = TempFileGuard::new("img", "png");
        write_test_png_via_gst(&png.0, 32, 24);
        let png_meta = discover_media(AssetId::new(), &png.0).unwrap();
        assert_eq!(png_meta.duration_ns, DEFAULT_IMAGE_DURATION_NS);
        assert_eq!((png_meta.width, png_meta.height), (Some(32), Some(24)));
        assert!(!png_meta.has_audio);
        assert!(png_meta.file_size_bytes > 0);

        let webp = TempFileGuard::new("img", "webp");
        let mut header = [0_u8; 30];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&22_u32.to_le_bytes());
        header[8..12].copy_from_slice(b"WEBP");
        header[12..16].copy_from_slice(b"VP8X");
        header[16..20].copy_from_slice(&10_u32.to_le_bytes());
        header[24..27].copy_from_slice(&[31, 0, 0]);
        header[27..30].copy_from_slice(&[23, 0, 0]);
        fs::write(&webp.0, header).unwrap();
        let webp_meta = discover_media(AssetId::new(), &webp.0).unwrap();
        assert_eq!(webp_meta.duration_ns, DEFAULT_IMAGE_DURATION_NS);
        assert_eq!((webp_meta.width, webp_meta.height), (Some(32), Some(24)));
        assert!(!webp_meta.has_audio);

        let bad_webp = TempFileGuard::new("bad", "webp");
        fs::write(&bad_webp.0, b"NOT_A_WEBP").unwrap();
        assert!(discover_media(AssetId::new(), &bad_webp.0).is_err());
    }
}
