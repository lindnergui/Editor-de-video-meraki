use crate::discoverer;
use crate::encoding_profiles::{self, ProxyPlugins};
use crate::protocol::{EngineEvent, EngineWorkerChannel};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use meraki_core::MediaMetadata;
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) enum ProxyRunError {
    RetryHardware(String),
    Abort(String),
    Cancelled,
}

struct ProxyPipelineGuard<'a>(&'a gst::Pipeline);

impl Drop for ProxyPipelineGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

pub(crate) fn transcode(
    source: &Path,
    metadata: &MediaMetadata,
    plugins: ProxyPlugins,
    (target_w, target_h): (u32, u32),
    part_path: &Path,
    stopped: &dyn Fn() -> bool,
    worker: &EngineWorkerChannel,
) -> Result<(), ProxyRunError> {
    let uri =
        discoverer::path_to_file_uri(source).map_err(|e| ProxyRunError::Abort(e.to_string()))?;
    let part_loc = part_path
        .to_str()
        .ok_or_else(|| ProxyRunError::Abort("caminho temporário de proxy inválido".to_string()))?;

    let pipeline = gst::Pipeline::with_name("meraki-proxy-transcode");
    let _guard = ProxyPipelineGuard(&pipeline);

    let decode = make_elem("uridecodebin", "proxy-decode")?;
    decode.set_property("uri", uri);
    let vqueue = make_elem("queue", "proxy-vqueue")?;
    let vconv = make_elem("videoconvert", "proxy-vconv")?;
    let vscale = make_elem("videoscale", "proxy-vscale")?;
    let vcaps = make_elem("capsfilter", "proxy-vcaps")?;
    let venc = make_elem(plugins.video_encoder, "proxy-venc")?;
    let mux = make_elem(plugins.muxer, "proxy-mux")?;
    let filesink = make_elem("filesink", "proxy-filesink")?;
    filesink.set_property("location", part_loc);

    set_int_prop(&vconv, "n-threads", 2);
    set_int_prop(&vscale, "n-threads", 2);
    tune_proxy_encoder(&venc, plugins.video_encoder);
    let target_caps = gst_video::VideoCapsBuilder::for_encoding("video/x-raw")
        .format_list([gst_video::VideoFormat::I420, gst_video::VideoFormat::Nv12])
        .width(i32::try_from(target_w).unwrap_or(1280))
        .height(i32::try_from(target_h).unwrap_or(720))
        .build();
    vcaps.set_property("caps", target_caps);

    pipeline
        .add_many([
            &decode, &vqueue, &vconv, &vscale, &vcaps, &venc, &mux, &filesink,
        ])
        .map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let vparse = gst::ElementFactory::find("h264parse")
        .and_then(|_| make_elem("h264parse", "proxy-h264parse").ok());
    if let Some(ref parse) = vparse {
        pipeline
            .add(parse)
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        gst::Element::link_many([&vqueue, &vconv, &vscale, &vcaps, &venc, parse])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(parse, &mux, "video_%u")?;
    } else {
        gst::Element::link_many([&vqueue, &vconv, &vscale, &vcaps, &venc])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(&venc, &mux, "video_%u")?;
    }
    mux.link(&filesink)
        .map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let aqueue = if let Some(aenc_name) = plugins.audio_encoder {
        build_audio_branch(&pipeline, &mux, aenc_name)?
    } else {
        None
    };

    let vq_target = vqueue;
    let aq_target = aqueue;
    decode.connect_pad_added(move |_src, pad| {
        let Some(caps) = pad.current_caps().or_else(|| Some(pad.query_caps(None))) else {
            return;
        };
        let Some(name) = caps.structure(0).map(|s| s.name()) else {
            return;
        };
        if name.starts_with("video/") {
            if let Some(sp) = vq_target.static_pad("sink").filter(|p| !p.is_linked()) {
                let _ = pad.link(&sp);
            }
        } else if name.starts_with("audio/") {
            if let Some(sp) = aq_target
                .as_ref()
                .and_then(|q| q.static_pad("sink"))
                .filter(|p| !p.is_linked())
            {
                let _ = pad.link(&sp);
            }
        }
    });

    let bus = pipeline
        .bus()
        .ok_or_else(|| ProxyRunError::Abort("barramento indisponível para proxy".to_string()))?;
    if let Err(error) = pipeline.set_state(gst::State::Playing) {
        return Err(bus
            .pop_filtered(&[gst::MessageType::Error])
            .and_then(|message| match message.view() {
                gst::MessageView::Error(error) => Some(proxy_bus_error(error)),
                _ => None,
            })
            .unwrap_or_else(|| ProxyRunError::Abort(format!("erro ao iniciar proxy: {error}"))));
    }

    let mut last_emit = Instant::now();
    let mut last_advance = last_emit;
    let mut last_position = gst::ClockTime::ZERO;
    loop {
        if stopped() {
            return Err(ProxyRunError::Cancelled);
        }
        if let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(100))) {
            match msg.view() {
                gst::MessageView::Eos(_) => return Ok(()),
                gst::MessageView::Error(err) => return Err(proxy_bus_error(err)),
                _ => {}
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(150) {
            let position = pipeline
                .query_position::<gst::ClockTime>()
                .unwrap_or(gst::ClockTime::ZERO);
            if position > last_position {
                last_position = position;
                last_advance = Instant::now();
            }
            if last_advance.elapsed() >= Duration::from_secs(30) {
                return Err(ProxyRunError::Abort(format!(
                    "30 s sem progresso em {position}"
                )));
            }
            let progress = query_progress(&pipeline, position, metadata.duration_ns);
            worker.emit_drop_if_full(EngineEvent::ProxyProgress {
                asset_id: metadata.asset_id,
                progress,
                proxy_path: None,
            });
            last_emit = Instant::now();
        }
    }
}

fn proxy_bus_error(error: &gst::message::Error) -> ProxyRunError {
    if let Some(encoder) = encoding_profiles::demote_failing_hardware_encoder(error.src()) {
        return ProxyRunError::RetryHardware(encoder);
    }
    ProxyRunError::Abort(format!(
        "{} ({})",
        error.error(),
        error.debug().unwrap_or_default()
    ))
}

fn build_audio_branch(
    pipeline: &gst::Pipeline,
    mux: &gst::Element,
    aenc_name: &'static str,
) -> Result<Option<gst::Element>, ProxyRunError> {
    let aq = make_elem("queue", "proxy-aqueue")?;
    let aconv = make_elem("audioconvert", "proxy-aconv")?;
    let ares = make_elem("audioresample", "proxy-ares")?;
    let acaps = make_elem("capsfilter", "proxy-acaps")?;
    let aenc = make_elem(aenc_name, "proxy-aenc")?;
    let audio_caps = gst::Caps::builder("audio/x-raw")
        .field("rate", 48_000_i32)
        .field("channels", 2_i32)
        .build();
    acaps.set_property("caps", audio_caps);

    pipeline
        .add_many([&aq, &aconv, &ares, &acaps, &aenc])
        .map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let aparse = gst::ElementFactory::find("aacparse")
        .and_then(|_| make_elem("aacparse", "proxy-aacparse").ok());
    if let Some(ref parse) = aparse {
        pipeline
            .add(parse)
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        gst::Element::link_many([&aq, &aconv, &ares, &acaps, &aenc, parse])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(parse, mux, "audio_%u")?;
    } else {
        gst::Element::link_many([&aq, &aconv, &ares, &acaps, &aenc])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(&aenc, mux, "audio_%u")?;
    }
    Ok(Some(aq))
}

fn make_elem(factory: &'static str, name: &'static str) -> Result<gst::Element, ProxyRunError> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|e| ProxyRunError::Abort(format!("falha ao criar '{factory}': {e}")))
}

fn link_to_mux(
    src: &gst::Element,
    mux: &gst::Element,
    pad_tmpl: &str,
) -> Result<(), ProxyRunError> {
    let src_pad = src
        .static_pad("src")
        .ok_or_else(|| ProxyRunError::Abort("pad src ausente".to_string()))?;
    let mux_pad = mux
        .request_pad_simple(pad_tmpl)
        .ok_or_else(|| ProxyRunError::Abort(format!("pad '{pad_tmpl}' ausente")))?;
    src_pad
        .link(&mux_pad)
        .map_err(|e| ProxyRunError::Abort(format!("link mux: {e:?}")))?;
    Ok(())
}

fn set_int_prop(elem: &gst::Element, name: &str, val: u32) {
    if let Some(pspec) = elem.find_property(name) {
        if pspec.value_type() == glib::Type::U32 {
            elem.set_property(name, val);
        } else if pspec.value_type() == glib::Type::I32 {
            elem.set_property(name, i32::try_from(val).unwrap_or(15));
        }
    }
}

fn tune_proxy_encoder(elem: &gst::Element, factory_name: &str) {
    set_int_prop(elem, "key-int-max", 15);
    set_int_prop(elem, "gop-size", 15);
    set_int_prop(elem, "bframes", 0);
    set_int_prop(elem, "threads", 2);
    if factory_name != "openh264enc" {
        set_int_prop(elem, "bitrate", 2500);
    }
    if factory_name == "x264enc" {
        elem.set_property_from_str("speed-preset", "ultrafast");
        elem.set_property_from_str("tune", "zerolatency");
    }
}

fn query_progress(
    pipeline: &gst::Pipeline,
    position: gst::ClockTime,
    fallback_duration_ns: u64,
) -> f32 {
    let total_ns = pipeline
        .query_duration::<gst::ClockTime>()
        .map_or(fallback_duration_ns, gst::ClockTime::nseconds);
    let pos_ns = position.nseconds();
    if total_ns == 0 {
        0.0
    } else {
        ((pos_ns as f64) / (total_ns as f64)).clamp(0.0, 0.99) as f32
    }
}
