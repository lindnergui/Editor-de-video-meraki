use crate::protocol::{EngineEvent, EngineWorkerChannel, VideoFrame};
use bytes::Bytes;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;
use meraki_core::PreviewQuality;
use std::sync::Arc;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum FrameSinkError {
    #[error("falha ao instanciar elemento GStreamer '{factory}': {reason}")]
    CreateElement { factory: &'static str, reason: String },
    #[error("elemento '{0}' não é um GstAppSink válido")]
    InvalidAppSink(&'static str),
    #[error("falha ao adicionar elementos ao bin de vídeo: {0}")]
    BinAdd(String),
    #[error("falha ao conectar pipeline interno do sink de vídeo: {0}")]
    Link(String),
    #[error("pad 'sink' ausente em '{0}'")]
    MissingSinkPad(&'static str),
    #[error("falha ao configurar GhostPad no sink de vídeo: {0}")]
    GhostPad(String),
}

/// Encapsula `videoconvert ! videoscale ! capsfilter ! appsink` para extração segura de RGBA8.
pub(crate) struct PreviewVideoSink {
    bin: gst::Bin,
    capsfilter: gst::Element,
    _appsink: gst_app::AppSink,
}

impl PreviewVideoSink {
    pub(crate) fn new(
        initial_quality: PreviewQuality,
        worker: Arc<EngineWorkerChannel>,
    ) -> Result<Self, FrameSinkError> {
        let bin = gst::Bin::with_name("meraki-video-preview-bin");
        let videoconvert = make_element("videoconvert", "meraki-preview-convert")?;
        let videoscale = make_element("videoscale", "meraki-preview-scale")?;
        let capsfilter = make_element("capsfilter", "meraki-preview-caps")?;
        let appsink_elem = make_element("appsink", "meraki-preview-appsink")?;

        configure_multithreading(&videoconvert);
        configure_multithreading(&videoscale);
        capsfilter.set_property("caps", preview_caps(initial_quality));

        let appsink = appsink_elem
            .clone()
            .dynamic_cast::<gst_app::AppSink>()
            .map_err(|_| FrameSinkError::InvalidAppSink("appsink"))?;

        appsink.set_max_buffers(1);
        appsink.set_drop(true);
        appsink.set_property("sync", true);
        appsink.set_property("emit-signals", false);

        let preroll_worker = Arc::clone(&worker);
        let sample_worker = worker;
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_preroll(move |sink| {
                    let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                    emit_sample_frame(&preroll_worker, &sample);
                    Ok(gst::FlowSuccess::Ok)
                })
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    emit_sample_frame(&sample_worker, &sample);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        bin.add_many([&videoconvert, &videoscale, &capsfilter, &appsink_elem])
            .map_err(|err| FrameSinkError::BinAdd(err.to_string()))?;
        gst::Element::link_many([&videoconvert, &videoscale, &capsfilter, &appsink_elem])
            .map_err(|err| FrameSinkError::Link(err.to_string()))?;
        attach_sink_ghost_pad(&bin, &videoconvert, "videoconvert")?;

        Ok(Self {
            bin,
            capsfilter,
            _appsink: appsink,
        })
    }

    pub(crate) fn bin(&self) -> &gst::Bin {
        &self.bin
    }

    /// Atualiza os limites de resolução (`720p` vs `1080p`) e solicita renegociação upstream.
    pub(crate) fn set_preview_quality(&self, quality: PreviewQuality) {
        self.capsfilter.set_property("caps", preview_caps(quality));
        if let Some(sink_pad) = self.capsfilter.static_pad("sink") {
            let _ = sink_pad.push_event(gst::event::Reconfigure::new());
        }
    }
}

pub(crate) fn preview_caps(quality: PreviewQuality) -> gst::Caps {
    let (max_w, max_h) = quality.max_dimensions();
    let max_w = i32::try_from(max_w).unwrap_or(1920);
    let max_h = i32::try_from(max_h).unwrap_or(1080);
    gst_video::VideoCapsBuilder::for_encoding("video/x-raw")
        .format(gst_video::VideoFormat::Rgba)
        .width_range(1..=max_w)
        .height_range(1..=max_h)
        .build()
}

pub(crate) const MASTER_LEVEL_ELEMENT_NAME: &str = "meraki-master-level";
pub(crate) const LEVEL_INTERVAL_NS: u64 = 33_000_000;

/// Constrói o sink de áudio do preview (`audioconvert ! audioresample ! level ! volume ! sink`).
pub(crate) fn build_audio_preview_sink() -> Result<(gst::Bin, gst::Element), FrameSinkError> {
    let bin = gst::Bin::with_name("meraki-audio-preview-bin");
    let convert = make_element("audioconvert", "meraki-preview-aconv")?;
    let resample = make_element("audioresample", "meraki-preview-aresample")?;
    let volume = make_element("volume", "meraki-preview-volume")?;
    let sink = match make_element("autoaudiosink", "meraki-preview-asink") {
        Ok(elem) => elem,
        Err(err) => {
            warn!(error = %err, "autoaudiosink indisponível; usando fakesink em áudio");
            let fake = make_element("fakesink", "meraki-preview-afakesink")?;
            fake.set_property("sync", true);
            fake
        }
    };

    if gst::ElementFactory::find("level").is_some() {
        match make_element("level", MASTER_LEVEL_ELEMENT_NAME) {
            Ok(level) => {
                level.set_property("post-messages", true);
                level.set_property("interval", LEVEL_INTERVAL_NS);
                bin.add_many([&convert, &resample, &level, &volume, &sink])
                    .map_err(|err| FrameSinkError::BinAdd(err.to_string()))?;
                gst::Element::link_many([&convert, &resample, &level, &volume, &sink])
                    .map_err(|err| FrameSinkError::Link(err.to_string()))?;
                attach_sink_ghost_pad(&bin, &convert, "audioconvert")?;
                return Ok((bin, volume));
            }
            Err(err) => warn!(error = %err, "falha ao criar elemento level; seguindo sem medidor dB master"),
        }
    } else {
        warn!("plugin GStreamer 'level' indisponível; medidor dB master desabilitado");
    }

    bin.add_many([&convert, &resample, &volume, &sink])
        .map_err(|err| FrameSinkError::BinAdd(err.to_string()))?;
    gst::Element::link_many([&convert, &resample, &volume, &sink])
        .map_err(|err| FrameSinkError::Link(err.to_string()))?;
    attach_sink_ghost_pad(&bin, &convert, "audioconvert")?;

    Ok((bin, volume))
}

fn configure_multithreading(elem: &gst::Element) {
    if elem.find_property("n-threads").is_some() {
        elem.set_property("n-threads", 0_u32);
    }
}

fn make_element(
    factory: &'static str,
    name: &'static str,
) -> Result<gst::Element, FrameSinkError> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|err| FrameSinkError::CreateElement {
            factory,
            reason: err.to_string(),
        })
}

fn attach_sink_ghost_pad(
    bin: &gst::Bin,
    target: &gst::Element,
    target_name: &'static str,
) -> Result<(), FrameSinkError> {
    let target_pad = target
        .static_pad("sink")
        .ok_or(FrameSinkError::MissingSinkPad(target_name))?;
    let ghost_pad = gst::GhostPad::with_target(&target_pad)
        .map_err(|err| FrameSinkError::GhostPad(err.to_string()))?;
    bin.add_pad(&ghost_pad)
        .map_err(|err| FrameSinkError::GhostPad(err.to_string()))
}

fn emit_sample_frame(worker: &EngineWorkerChannel, sample: &gst::Sample) {
    if let Some(frame) = sample_to_video_frame(sample) {
        worker.emit_drop_if_full(EngineEvent::FrameReady(frame));
    }
}

fn sample_to_video_frame(sample: &gst::Sample) -> Option<VideoFrame> {
    let caps = sample.caps()?;
    let info = gst_video::VideoInfo::from_caps(caps).ok()?;
    if info.format() != gst_video::VideoFormat::Rgba || info.width() == 0 || info.height() == 0 {
        return None;
    }

    let buffer = sample.buffer()?;
    let pts_ns = buffer.pts().map_or(0, gst::ClockTime::nseconds);
    let mapped = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;

    let width = mapped.width();
    let height = mapped.height();
    let row_bytes = usize::try_from(width).ok()?.checked_mul(4)?;
    let total_bytes = row_bytes.checked_mul(usize::try_from(height).ok()?)?;
    let stride = usize::try_from(*mapped.plane_stride().first()?).ok()?;
    if stride < row_bytes {
        return None;
    }

    let plane = mapped.plane_data(0).ok()?;
    let rgba_pixels = if stride == row_bytes {
        Bytes::copy_from_slice(plane.get(..total_bytes)?)
    } else {
        let mut buf = Vec::with_capacity(total_bytes);
        for row in 0..height as usize {
            let start = row.checked_mul(stride)?;
            let end = start.checked_add(row_bytes)?;
            buf.extend_from_slice(plane.get(start..end)?);
        }
        Bytes::from(buf)
    };

    Some(VideoFrame {
        width,
        height,
        rgba_pixels,
        pts_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::create_engine_channel;

    #[test]
    fn preview_sink_configures_multithreading_and_single_buffer_drop() {
        crate::ensure_media_initialized().expect("deve inicializar GStreamer/GES");
        let (_handle, worker) = create_engine_channel();
        let sink = PreviewVideoSink::new(PreviewQuality::Proxy720p, Arc::new(worker))
            .expect("deve construir PreviewVideoSink");

        for elem_name in ["meraki-preview-convert", "meraki-preview-scale"] {
            let elem = sink.bin().by_name(elem_name).expect("elemento deve estar no bin");
            if elem.find_property("n-threads").is_some() {
                assert_eq!(elem.property::<u32>("n-threads"), 0);
            }
        }
        assert_eq!(sink._appsink.property::<u32>("max-buffers"), 1);
        assert!(sink._appsink.property::<bool>("drop"));
    }
}
