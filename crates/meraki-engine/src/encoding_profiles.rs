use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_pbutils as gst_pbutils;
use gstreamer_pbutils::encoding_profile::EncodingProfileBuilder;
use meraki_core::{ExportFormat, ExportSettings};
use thiserror::Error;
use tracing::warn;

const H264_ENCODERS: [&str; 7] = [
    "d3d11h264enc",
    "mfh264enc",
    "nvh264enc",
    "qsvh264enc",
    "vaapih264enc",
    "x264enc",
    "openh264enc",
];
const PRORES_ENCODERS: [&str; 2] = ["avenc_prores_ks", "avenc_prores"];
const AAC_ENCODERS: [&str; 5] = ["voaacenc", "mfaacenc", "mf_aacenc", "faac", "avenc_aac"];
const MP3_ENCODERS: [&str; 2] = ["lamemp3enc", "avenc_mp3"];

/// Erros estruturados ao sondar plugins e montar perfis de codificação GStreamer.
#[derive(Debug, Error)]
pub enum ExportProfileError {
    #[error("plugin GStreamer essencial ausente para {role} (procurados: {candidates}); instale {hint}")]
    MissingEssentialPlugin {
        role: &'static str,
        candidates: &'static str,
        hint: &'static str,
    },
    #[error("nenhum fluxo compatível (vídeo ou áudio) disponível para o formato selecionado")]
    NoStreamsSelected,
}

/// Perfil `EncodingProfile` resolvido juntamente com os nomes das fábricas encontradas.
#[derive(Debug, Clone)]
pub struct ResolvedExportProfile {
    pub profile: gst_pbutils::EncodingProfile,
    pub muxer: Option<&'static str>,
    pub video_encoder: Option<&'static str>,
    pub audio_encoder: Option<&'static str>,
}

/// Constrói o perfil `gst_pbutils::EncodingProfile` verificando disponibilidade real dos plugins.
pub fn build_encoding_profile(
    settings: &ExportSettings,
    include_video: bool,
    include_audio: bool,
) -> Result<ResolvedExportProfile, ExportProfileError> {
    if gst::ElementFactory::find("encodebin").is_none() {
        return Err(ExportProfileError::MissingEssentialPlugin {
            role: "codificação de exportação (encodebin)",
            candidates: "encodebin",
            hint: "gst-plugins-base",
        });
    }

    let (w, h) = settings.resolution.dimensions();
    let fps = settings.fps.as_u32();

    match settings.format {
        ExportFormat::Mp4H264 => build_mp4_h264_profile(w, h, fps, include_video, include_audio),
        ExportFormat::MovProRes => build_mov_prores_profile(w, h, fps, include_video, include_audio),
        ExportFormat::AudioWav => build_audio_wav_profile(include_audio),
        ExportFormat::AudioMp3 => build_audio_mp3_profile(include_audio),
    }
}

/// Atalho para sondagem completa de um `ExportSettings` em testes.
#[cfg(test)]
pub fn probe_encoding_profile(
    settings: &ExportSettings,
) -> Result<ResolvedExportProfile, ExportProfileError> {
    let include_video = !settings.format.is_audio_only();
    build_encoding_profile(settings, include_video, true)
}

fn is_factory_usable(name: &str) -> bool {
    gst::ElementFactory::find(name)
        .is_some_and(|f| f.upcast_ref::<gst::PluginFeature>().rank() > gst::Rank::NONE)
}

fn find_first_factory(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|name| is_factory_usable(name))
}

/// Fábricas selecionadas para transcodificação rápida de proxies 720p.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProxyPlugins {
    pub muxer: &'static str,
    pub video_encoder: &'static str,
    pub audio_encoder: Option<&'static str>,
}

/// Seleciona muxer MP4 e encoders disponíveis para geração de proxy 720p.
pub(crate) fn select_proxy_plugins(include_audio: bool) -> Result<ProxyPlugins, ExportProfileError> {
    let muxer = find_first_factory(&["mp4mux", "qtmux"]).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "muxer MP4 para proxy",
            candidates: "mp4mux, qtmux",
            hint: "gst-plugins-good",
        },
    )?;
    let video_encoder = find_first_factory(&H264_ENCODERS).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "encoder H.264 para proxy",
            candidates: "d3d11h264enc, mfh264enc, nvh264enc, qsvh264enc, vaapih264enc, x264enc, openh264enc",
            hint: "gst-plugins-ugly ou gst-plugins-bad",
        },
    )?;
    let audio_encoder = if include_audio {
        let found = find_first_factory(&AAC_ENCODERS);
        if found.is_none() {
            warn!("encoder AAC ausente; gerando proxy 720p somente com vídeo");
        }
        found
    } else {
        None
    };
    Ok(ProxyPlugins { muxer, video_encoder, audio_encoder })
}

/// Rebaixa em runtime encoder de hardware que falhou na renderização quando existe alternativa utilizável.
pub(crate) fn demote_failing_hardware_encoder(src: Option<&gst::Object>) -> Option<String> {
    let elem = src?.downcast_ref::<gst::Element>()?;
    let factory = elem.factory()?;
    let klass = factory.klass();
    if !klass.contains("Hardware") || !klass.contains("Encoder") {
        return None;
    }

    let name = factory.name().to_string();
    let feature = factory.upcast_ref::<gst::PluginFeature>();
    if feature.rank() <= gst::Rank::NONE {
        return None;
    }

    let has_fallback = H264_ENCODERS
        .iter()
        .copied()
        .any(|cand| cand != name.as_str() && is_factory_usable(cand));
    if !has_fallback {
        return None;
    }

    feature.set_rank(gst::Rank::NONE);
    warn!(encoder = %name, "encoder de hardware falhou na renderização; rebaixando para tentar fallback");
    Some(name)
}

fn video_restriction_caps(width: u32, height: u32, fps: u32, formats: Option<&[&str]>) -> gst::Caps {
    let w = i32::try_from(width.max(16)).unwrap_or(1920);
    let h = i32::try_from(height.max(16)).unwrap_or(1080);
    let fps_num = i32::try_from(fps.clamp(1, 240)).unwrap_or(30);
    let mut builder = gst::Caps::builder("video/x-raw")
        .field("width", w)
        .field("height", h)
        .field("framerate", gst::Fraction::new(fps_num, 1));
    if let Some(fmts) = formats {
        builder = builder.field("format", gst::List::new(fmts));
    }
    builder.build()
}

fn audio_restriction_caps() -> gst::Caps {
    gst::Caps::builder("audio/x-raw")
        .field("rate", 48_000_i32)
        .field("channels", 2_i32)
        .build()
}

fn build_mp4_h264_profile(
    width: u32,
    height: u32,
    fps: u32,
    include_video: bool,
    include_audio: bool,
) -> Result<ResolvedExportProfile, ExportProfileError> {
    if !include_video && !include_audio {
        return Err(ExportProfileError::NoStreamsSelected);
    }
    let muxer = find_first_factory(&["mp4mux", "qtmux"]).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "muxer MP4",
            candidates: "mp4mux, qtmux",
            hint: "gst-plugins-good",
        },
    )?;

    let container_caps = gst::Caps::builder("video/quicktime").field("variant", "iso").build();
    let mut container = gst_pbutils::EncodingContainerProfile::builder(&container_caps)
        .name("meraki-mp4-h264");
    let mut video_encoder = None;
    let mut audio_encoder = None;

    if include_video {
        let enc = find_first_factory(&H264_ENCODERS).ok_or(
            ExportProfileError::MissingEssentialPlugin {
                role: "encoder de vídeo H.264",
                candidates: "d3d11h264enc, mfh264enc, nvh264enc, vaapih264enc, x264enc, openh264enc",
                hint: "gst-plugins-ugly ou gst-plugins-bad",
            },
        )?;
        video_encoder = Some(enc);
        let v_caps = gst::Caps::builder("video/x-h264").build();
        let v_rest = video_restriction_caps(width, height, fps, Some(&["I420", "NV12"]));
        let v_prof = gst_pbutils::EncodingVideoProfile::builder(&v_caps)
            .restriction(&v_rest)
            .presence(0)
            .build();
        container = container.add_profile(v_prof);
    }

    if include_audio {
        let enc = find_first_factory(&AAC_ENCODERS).ok_or(
            ExportProfileError::MissingEssentialPlugin {
                role: "encoder de áudio AAC",
                candidates: "voaacenc, mfaacenc, mf_aacenc, faac, avenc_aac",
                hint: "gst-plugins-bad ou gst-libav",
            },
        )?;
        audio_encoder = Some(enc);
        let a_caps = gst::Caps::builder("audio/mpeg")
            .field("mpegversion", 4_i32)
            .field("stream-format", "raw")
            .build();
        let a_rest = audio_restriction_caps();
        let a_prof = gst_pbutils::EncodingAudioProfile::builder(&a_caps)
            .restriction(&a_rest)
            .presence(0)
            .build();
        container = container.add_profile(a_prof);
    }

    Ok(ResolvedExportProfile {
        profile: container.build().upcast(),
        muxer: Some(muxer),
        video_encoder,
        audio_encoder,
    })
}

fn build_mov_prores_profile(
    width: u32,
    height: u32,
    fps: u32,
    include_video: bool,
    include_audio: bool,
) -> Result<ResolvedExportProfile, ExportProfileError> {
    if !include_video && !include_audio {
        return Err(ExportProfileError::NoStreamsSelected);
    }
    let muxer = find_first_factory(&["qtmux", "mp4mux"]).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "muxer MOV",
            candidates: "qtmux, mp4mux",
            hint: "gst-plugins-good",
        },
    )?;

    let container_caps = gst::Caps::builder("video/quicktime").field("variant", "apple").build();
    let mut container = gst_pbutils::EncodingContainerProfile::builder(&container_caps)
        .name("meraki-mov-prores");
    let mut video_encoder = None;
    let mut audio_encoder = None;

    if include_video {
        let (enc, codec_mime, fmts) = if let Some(prores) = find_first_factory(&PRORES_ENCODERS) {
            (prores, "video/x-prores", None)
        } else {
            let fallback = find_first_factory(&H264_ENCODERS).ok_or(
                ExportProfileError::MissingEssentialPlugin {
                    role: "encoder ProRes/H.264",
                    candidates: "avenc_prores_ks, avenc_prores, x264enc",
                    hint: "gst-libav ou gst-plugins-ugly",
                },
            )?;
            warn!(fallback, "encoder ProRes indisponível; usando fallback H.264 alta taxa para MOV");
            (fallback, "video/x-h264", Some(&["I420", "NV12"][..]))
        };
        video_encoder = Some(enc);
        let v_caps = gst::Caps::builder(codec_mime).build();
        let v_rest = video_restriction_caps(width, height, fps, fmts);
        container = container.add_profile(
            gst_pbutils::EncodingVideoProfile::builder(&v_caps)
                .restriction(&v_rest)
                .presence(0)
                .build(),
        );
    }

    if include_audio {
        audio_encoder = Some("pcm_s16le");
        let pcm_caps = gst::Caps::builder("audio/x-raw")
            .field("format", "S16LE")
            .field("layout", "interleaved")
            .build();
        let a_rest = gst::Caps::builder("audio/x-raw")
            .field("format", "S16LE")
            .field("layout", "interleaved")
            .field("rate", 48_000_i32)
            .field("channels", 2_i32)
            .build();
        container = container.add_profile(
            gst_pbutils::EncodingAudioProfile::builder(&pcm_caps)
                .restriction(&a_rest)
                .presence(0)
                .build(),
        );
    }

    Ok(ResolvedExportProfile {
        profile: container.build().upcast(),
        muxer: Some(muxer),
        video_encoder,
        audio_encoder,
    })
}

fn build_audio_wav_profile(include_audio: bool) -> Result<ResolvedExportProfile, ExportProfileError> {
    if !include_audio {
        return Err(ExportProfileError::NoStreamsSelected);
    }
    let muxer = find_first_factory(&["wavenc"]).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "muxer WAV",
            candidates: "wavenc",
            hint: "gst-plugins-good",
        },
    )?;

    let wav_caps = gst::Caps::builder("audio/x-wav").build();
    let pcm_caps = gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("layout", "interleaved")
        .build();
    let a_rest = gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("layout", "interleaved")
        .field("rate", 48_000_i32)
        .field("channels", 2_i32)
        .build();

    let audio_prof = gst_pbutils::EncodingAudioProfile::builder(&pcm_caps)
        .restriction(&a_rest)
        .presence(0)
        .build();
    let container = gst_pbutils::EncodingContainerProfile::builder(&wav_caps)
        .name("meraki-wav-pcm")
        .add_profile(audio_prof)
        .build();

    Ok(ResolvedExportProfile {
        profile: container.upcast(),
        muxer: Some(muxer),
        video_encoder: None,
        audio_encoder: Some("pcm_s16le"),
    })
}

fn build_audio_mp3_profile(include_audio: bool) -> Result<ResolvedExportProfile, ExportProfileError> {
    if !include_audio {
        return Err(ExportProfileError::NoStreamsSelected);
    }
    let enc = find_first_factory(&MP3_ENCODERS).ok_or(
        ExportProfileError::MissingEssentialPlugin {
            role: "encoder MP3",
            candidates: "lamemp3enc, avenc_mp3",
            hint: "gst-plugins-good ou gst-libav",
        },
    )?;

    let mp3_caps = gst::Caps::builder("audio/mpeg")
        .field("mpegversion", 1_i32)
        .field("layer", 3_i32)
        .build();
    let a_rest = audio_restriction_caps();
    let audio_prof = gst_pbutils::EncodingAudioProfile::builder(&mp3_caps)
        .name("meraki-mp3")
        .restriction(&a_rest)
        .presence(0)
        .build();

    Ok(ResolvedExportProfile {
        profile: audio_prof.upcast(),
        muxer: None,
        video_encoder: None,
        audio_encoder: Some(enc),
    })
}
