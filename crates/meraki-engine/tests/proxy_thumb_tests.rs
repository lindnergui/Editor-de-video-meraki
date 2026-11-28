use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::AssetId;
use meraki_engine::{spawn_engine, EngineCommand, EngineEvent, ProxyMode, ThumbnailFrame};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct FixtureGuard {
    dir: PathBuf,
    cleanup_proxies: Vec<PathBuf>,
}

impl FixtureGuard {
    fn new(stem: &str) -> Self {
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("meraki_proxy_thumb_{stem}_{}_{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("deve criar diretório temporário");
        Self { dir, cleanup_proxies: Vec::new() }
    }
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        for path in &self.cleanup_proxies {
            let _ = fs::remove_file(path);
        }
    }
}

fn run_gst_fixture(desc: &str) {
    let _ = gst::init();
    let pipeline = gst::parse::launch(desc).expect("pipeline de fixture deve montar");
    pipeline.set_state(gst::State::Playing).expect("deve iniciar fixture");
    let bus = pipeline.bus().expect("bus de fixture");
    let mut got_eos = false;
    for msg in bus.iter_timed(gst::ClockTime::from_seconds(25)) {
        match msg.view() {
            gst::MessageView::Eos(_) => {
                got_eos = true;
                break;
            }
            gst::MessageView::Error(err) => {
                let _ = pipeline.set_state(gst::State::Null);
                panic!("erro criando fixture GStreamer: {} ({:?})", err.error(), err.debug());
            }
            _ => {}
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    assert!(got_eos, "timeout criando fixture GStreamer");
}

fn write_sample_1080p_mp4(path: &Path) {
    let aenc = if gst::ElementFactory::find("voaacenc").is_some() {
        "voaacenc ! aacparse"
    } else {
        "mfaacenc ! aacparse"
    };
    let loc = path.to_string_lossy().replace('\\', "/");
    let desc = format!(
        "videotestsrc num-buffers=75 ! video/x-raw,format=I420,width=1920,height=1080,framerate=30/1 ! x264enc speed-preset=ultrafast key-int-max=15 ! h264parse ! mux.video_0 \
         audiotestsrc num-buffers=110 ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! {aenc} ! mux.audio_0 \
         mp4mux name=mux ! filesink location=\"{loc}\""
    );
    run_gst_fixture(&desc);
}

fn write_sample_png(path: &Path, width: u32, height: u32) {
    let loc = path.to_string_lossy().replace('\\', "/");
    let desc = format!(
        "videotestsrc num-buffers=1 ! video/x-raw,width={width},height={height} ! videoconvert ! pngenc ! filesink location=\"{loc}\""
    );
    run_gst_fixture(&desc);
}

fn write_stereo_wav(path: &Path) {
    let total_frames = 24_000_u32;
    let data_bytes = total_frames * 4;
    let riff_size = 36 + data_bytes;
    let mut f = File::create(path).expect("deve criar WAV");
    f.write_all(b"RIFF").unwrap();
    f.write_all(&riff_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16_u32.to_le_bytes()).unwrap();
    f.write_all(&1_u16.to_le_bytes()).unwrap();
    f.write_all(&2_u16.to_le_bytes()).unwrap();
    f.write_all(&48_000_u32.to_le_bytes()).unwrap();
    f.write_all(&(48_000_u32 * 4).to_le_bytes()).unwrap();
    f.write_all(&4_u16.to_le_bytes()).unwrap();
    f.write_all(&16_u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_bytes.to_le_bytes()).unwrap();
    f.write_all(&vec![0_u8; data_bytes as usize]).unwrap();
}

fn assert_valid_thumb(thumb: &ThumbnailFrame, expected_id: AssetId) {
    assert_eq!(thumb.asset_id, expected_id);
    assert!(thumb.width > 0 && thumb.width <= 320);
    assert!(thumb.height > 0 && thumb.height <= 180);
    assert_eq!(thumb.rgba_pixels.len(), (thumb.width * thumb.height * 4) as usize);
}

#[test]
fn engine_extracts_rgba_thumbnails_generates_720p_proxy_and_reuses_disk_cache() {
    let _ = gst::init();
    let mut guard = FixtureGuard::new("suite");
    let mp4_path = guard.dir.join("clip_1080p.mp4");
    let png_path = guard.dir.join("frame.png");
    let wav_path = guard.dir.join("audio.wav");

    write_sample_1080p_mp4(&mp4_path);
    write_sample_png(&png_path, 640, 360);
    write_stereo_wav(&wav_path);

    let handle = spawn_engine();
    let ready = handle.events().recv_timeout(Duration::from_secs(30)).expect("EngineReady");
    assert!(matches!(ready, EngineEvent::EngineReady));
    handle.send(EngineCommand::SetProxyMode(ProxyMode::Always)).unwrap();

    // 1. Vídeo 1080p no modo Always gera miniatura e proxy 1280 × 720.
    let id1 = AssetId::new();
    handle
        .send(EngineCommand::ImportMedia {
            asset_id: id1,
            path: mp4_path.clone(),
            generate_proxy: true,
        })
        .unwrap();

    let (mut saw_meta1, mut thumb1, mut proxy1) = (false, None, None);
    let deadline1 = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline1 && !(saw_meta1 && thumb1.is_some() && proxy1.is_some()) {
        if let Ok(ev) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match ev {
                EngineEvent::MediaDiscovered(m) if m.asset_id == id1 => {
                    assert_eq!((m.width, m.height), (Some(1920), Some(1080)));
                    saw_meta1 = true;
                }
                EngineEvent::ThumbnailReady(t) if t.asset_id == id1 => thumb1 = Some(t),
                EngineEvent::ProxyReady { asset_id, proxy_path: p } if asset_id == id1 =>
                {
                    proxy1 = Some(p);
                }
                EngineEvent::Error(err) => panic!("erro inesperado no import 1: {err}"),
                EngineEvent::ProxyFailed { reason, .. } => panic!("proxy 1080p: {reason}"),
                _ => {}
            }
        }
    }
    assert!(saw_meta1, "MediaDiscovered esperado para vídeo 1080p");
    assert_valid_thumb(thumb1.as_ref().expect("ThumbnailReady esperado para vídeo"), id1);
    let proxy1_path = proxy1.expect("ProxyReady esperado");
    guard.cleanup_proxies.push(proxy1_path.clone());
    assert!(proxy1_path.is_file());
    let proxy1_len = fs::metadata(&proxy1_path).unwrap().len();
    assert!(proxy1_len > 1024);

    // 2. Valida dimensões do proxy gerado e reutilização do cache ao reimportar o mesmo vídeo
    let id2 = AssetId::new();
    handle
        .send(EngineCommand::ImportMedia {
            asset_id: id2,
            path: mp4_path,
            generate_proxy: true,
        })
        .unwrap();
    let verify_id = AssetId::new();
    handle
        .send(EngineCommand::ImportMedia {
            asset_id: verify_id,
            path: proxy1_path.clone(),
            generate_proxy: false,
        })
        .unwrap();

    let (mut saw_meta2, mut thumb2, mut proxy2, mut verified_proxy_dims) = (false, None, None, false);
    let deadline2 = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline2
        && !(saw_meta2 && thumb2.is_some() && proxy2.is_some() && verified_proxy_dims)
    {
        if let Ok(ev) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match ev {
                EngineEvent::MediaDiscovered(m) if m.asset_id == id2 => saw_meta2 = true,
                EngineEvent::MediaDiscovered(m) if m.asset_id == verify_id => {
                    assert_eq!((m.width, m.height), (Some(1280), Some(720)));
                    verified_proxy_dims = true;
                }
                EngineEvent::ThumbnailReady(t) if t.asset_id == id2 => thumb2 = Some(t),
                EngineEvent::ProxyReady { asset_id, proxy_path: p } if asset_id == id2 =>
                {
                    proxy2 = Some(p);
                }
                EngineEvent::Error(err) => panic!("erro inesperado no cache hit: {err}"),
                EngineEvent::ProxyFailed { reason, .. } => panic!("proxy do cache: {reason}"),
                _ => {}
            }
        }
    }
    assert!(saw_meta2 && verified_proxy_dims);
    assert_valid_thumb(thumb2.as_ref().expect("ThumbnailReady no reimport"), id2);
    let proxy2_path = proxy2.expect("cache hit de proxy deve emitir ProxyReady");
    assert_eq!(proxy2_path, proxy1_path);
    assert_eq!(fs::metadata(&proxy2_path).unwrap().len(), proxy1_len);

    // 3. Imagem PNG + Áudio WAV (áudio não gera erro nem trava)
    let id_png = AssetId::new();
    let id_wav = AssetId::new();
    handle
        .send(EngineCommand::ImportMedia { asset_id: id_png, path: png_path, generate_proxy: true })
        .unwrap();
    handle
        .send(EngineCommand::ImportMedia { asset_id: id_wav, path: wav_path, generate_proxy: true })
        .unwrap();

    let (mut saw_png, mut thumb_png, mut saw_wav) = (false, None, false);
    let deadline3 = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline3 && !(saw_png && thumb_png.is_some() && saw_wav) {
        if let Ok(ev) = handle.events().recv_timeout(Duration::from_millis(50)) {
            match ev {
                EngineEvent::MediaDiscovered(m) if m.asset_id == id_png => saw_png = true,
                EngineEvent::ThumbnailReady(t) if t.asset_id == id_png => thumb_png = Some(t),
                EngineEvent::MediaDiscovered(m) if m.asset_id == id_wav => saw_wav = true,
                EngineEvent::Error(err) => panic!("erro inesperado com PNG/WAV: {err}"),
                _ => {}
            }
        }
    }
    assert!(saw_png && saw_wav);
    assert_valid_thumb(thumb_png.as_ref().expect("ThumbnailReady para PNG"), id_png);

    handle.send(EngineCommand::Shutdown).unwrap();
}
