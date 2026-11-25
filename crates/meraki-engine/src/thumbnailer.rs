use crate::discoverer;
use crate::protocol::ThumbnailFrame;
use bytes::Bytes;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;
use meraki_core::{AssetId, MediaMetadata};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

const MAX_THUMB_W: u32 = 320;
const MAX_THUMB_H: u32 = 180;
const PREROLL_TIMEOUT: Duration = Duration::from_secs(3);
const SEEK_TIMEOUT: Duration = Duration::from_secs(2);
const SEEK_MIN_DURATION_NS: u64 = 2_000_000_000;

struct PipelineGuard<'a>(&'a gst::Pipeline);

impl Drop for PipelineGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

/// Calcula dimensões proporcionais dentro do teto `320x180` sem ampliar mídias menores.
pub(crate) fn compute_thumbnail_dimensions(
    src_w: Option<u32>,
    src_h: Option<u32>,
) -> Option<(u32, u32)> {
    let (w, h) = (src_w?, src_h?);
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (f64::from(MAX_THUMB_W) / f64::from(w))
        .min(f64::from(MAX_THUMB_H) / f64::from(h))
        .min(1.0);
    let tw = ((f64::from(w) * scale).round() as u32).clamp(2, MAX_THUMB_W);
    let th = ((f64::from(h) * scale).round() as u32).clamp(2, MAX_THUMB_H);
    Some((tw, th))
}

/// Extrai uma miniatura RGBA8 compacta (`<= 320x180`) via pipeline curto isolado.
pub(crate) fn extract_thumbnail(
    asset_id: AssetId,
    path: &Path,
    metadata: &MediaMetadata,
) -> Option<ThumbnailFrame> {
    if metadata.width.is_none() || metadata.height.is_none() {
        return None;
    }
    let uri = match discoverer::path_to_file_uri(path) {
        Ok(uri) => uri,
        Err(err) => {
            warn!(%asset_id, path = %path.display(), error = %err, "URI inválida ao extrair miniatura");
            return None;
        }
    };

    let pipeline = gst::Pipeline::with_name("meraki-thumb-pipeline");
    let _guard = PipelineGuard(&pipeline);

    let decode = make_elem("uridecodebin", "thumb-decode", asset_id)?;
    let vconv = make_elem("videoconvert", "thumb-vconv", asset_id)?;
    let vscale = make_elem("videoscale", "thumb-vscale", asset_id)?;
    let capsfilter = make_elem("capsfilter", "thumb-caps", asset_id)?;
    let appsink_elem = make_elem("appsink", "thumb-appsink", asset_id)?;

    decode.set_property("uri", uri);
    set_auto_threads(&vconv);
    set_auto_threads(&vscale);
    capsfilter.set_property("caps", build_thumb_caps(metadata.width, metadata.height));

    let Ok(appsink) = appsink_elem.clone().dynamic_cast::<gst_app::AppSink>() else {
        warn!(%asset_id, "elemento appsink inválido na extração de miniatura");
        return None;
    };
    appsink.set_property("sync", false);
    appsink.set_max_buffers(1);
    appsink.set_drop(false);
    appsink.set_property("emit-signals", false);

    if let Err(err) = pipeline.add_many([&decode, &vconv, &vscale, &capsfilter, &appsink_elem]) {
        warn!(%asset_id, error = %err, "falha ao montar pipeline de miniatura");
        return None;
    }
    if let Err(err) = gst::Element::link_many([&vconv, &vscale, &capsfilter, &appsink_elem]) {
        warn!(%asset_id, error = %err, "falha ao conectar elementos de miniatura");
        return None;
    }

    let no_video = Arc::new(AtomicBool::new(false));
    let vconv_pad_target = vconv.clone();
    decode.connect_pad_added(move |_src, pad| {
        let Some(caps) = pad.current_caps().or_else(|| Some(pad.query_caps(None))) else {
            return;
        };
        let is_video = caps.structure(0).is_some_and(|s| s.name().starts_with("video/"));
        if !is_video {
            return;
        }
        let Some(sink_pad) = vconv_pad_target.static_pad("sink") else {
            return;
        };
        if !sink_pad.is_linked() {
            let _ = pad.link(&sink_pad);
        }
    });

    let vconv_check = vconv;
    let no_video_flag = Arc::clone(&no_video);
    decode.connect_no_more_pads(move |_src| {
        if vconv_check.static_pad("sink").is_some_and(|p| !p.is_linked()) {
            no_video_flag.store(true, Ordering::Release);
        }
    });

    if let Err(err) = pipeline.set_state(gst::State::Paused) {
        warn!(%asset_id, error = ?err, "falha ao pré-carregar pipeline de miniatura");
        return None;
    }

    let bus = pipeline.bus()?;
    if !wait_async_done(&bus, &no_video, PREROLL_TIMEOUT, asset_id) {
        return None;
    }

    let is_static = metadata.fps_num.is_none() && is_static_image_ext(path);
    if !is_static && metadata.duration_ns > SEEK_MIN_DURATION_NS {
        let seek_ns = metadata.duration_ns / 10;
        let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT;
        if pipeline
            .seek_simple(flags, gst::ClockTime::from_nseconds(seek_ns))
            .is_ok()
        {
            let _ = wait_async_done(&bus, &no_video, SEEK_TIMEOUT, asset_id);
        }
    }

    let sample = appsink
        .try_pull_preroll(gst::ClockTime::from_seconds(2))
        .or_else(|| appsink.try_pull_sample(gst::ClockTime::from_mseconds(500)))?;

    sample_to_thumbnail(asset_id, &sample)
}

fn make_elem(factory: &'static str, name: &'static str, asset_id: AssetId) -> Option<gst::Element> {
    match gst::ElementFactory::make(factory).name(name).build() {
        Ok(elem) => Some(elem),
        Err(err) => {
            warn!(%asset_id, factory, error = %err, "plugin indisponível para miniatura");
            None
        }
    }
}

fn set_auto_threads(elem: &gst::Element) {
    if elem.find_property("n-threads").is_some() {
        elem.set_property("n-threads", 0_u32);
    }
}

fn build_thumb_caps(src_w: Option<u32>, src_h: Option<u32>) -> gst::Caps {
    let builder = gst_video::VideoCapsBuilder::for_encoding("video/x-raw")
        .format(gst_video::VideoFormat::Rgba);
    match compute_thumbnail_dimensions(src_w, src_h) {
        Some((w, h)) => {
            let iw = i32::try_from(w).unwrap_or(320);
            let ih = i32::try_from(h).unwrap_or(180);
            builder.width(iw).height(ih).build()
        }
        None => builder
            .width_range(1..=i32::try_from(MAX_THUMB_W).unwrap_or(320))
            .height_range(1..=i32::try_from(MAX_THUMB_H).unwrap_or(180))
            .build(),
    }
}

fn wait_async_done(
    bus: &gst::Bus,
    no_video: &AtomicBool,
    timeout: Duration,
    asset_id: AssetId,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if no_video.load(Ordering::Acquire) {
            warn!(%asset_id, "arquivo sem fluxo de vídeo ao extrair miniatura");
            return false;
        }
        if let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(50))) {
            match msg.view() {
                gst::MessageView::AsyncDone(_) => return true,
                gst::MessageView::Error(err) => {
                    warn!(%asset_id, error = %err.error(), "erro GStreamer ao extrair miniatura");
                    return false;
                }
                gst::MessageView::Eos(_) => {
                    warn!(%asset_id, "EOS antes do preroll ao extrair miniatura");
                    return false;
                }
                _ => {}
            }
        }
    }
    warn!(%asset_id, "timeout aguardando preroll de miniatura");
    false
}

fn sample_to_thumbnail(asset_id: AssetId, sample: &gst::Sample) -> Option<ThumbnailFrame> {
    let caps = sample.caps()?;
    let info = gst_video::VideoInfo::from_caps(caps).ok()?;
    if info.format() != gst_video::VideoFormat::Rgba || info.width() == 0 || info.height() == 0 {
        return None;
    }
    let buffer = sample.buffer()?;
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
    let mut rgba_pixels = Vec::with_capacity(total_bytes);
    if stride == row_bytes {
        rgba_pixels.extend_from_slice(plane.get(..total_bytes)?);
    } else {
        for row in 0..height as usize {
            let start = row.checked_mul(stride)?;
            let end = start.checked_add(row_bytes)?;
            rgba_pixels.extend_from_slice(plane.get(start..end)?);
        }
    }

    Some(ThumbnailFrame {
        asset_id,
        width,
        height,
        rgba_pixels: Bytes::from(rgba_pixels),
    })
}

fn is_static_image_ext(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg" | "webp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_thumbnail_dimensions_scales_proportionally_and_never_upscales() {
        assert_eq!(compute_thumbnail_dimensions(None, Some(1080)), None);
        assert_eq!(compute_thumbnail_dimensions(Some(0), Some(1080)), None);
        assert_eq!(compute_thumbnail_dimensions(Some(1920), Some(1080)), Some((320, 180)));
        assert_eq!(compute_thumbnail_dimensions(Some(1080), Some(1920)), Some((101, 180)));
        assert_eq!(compute_thumbnail_dimensions(Some(240), Some(120)), Some((240, 120)));
    }
}
