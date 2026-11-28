use super::{
    attach_sink_ghost_pad, make_element, FrameSinkError, LEVEL_INTERVAL_NS,
    MASTER_LEVEL_ELEMENT_NAME,
};
use crate::audio_output::{float_audio_caps, protect_peaks, MIX_RATE};
use crate::audio_spectrum::{self, HIGH_SPECTRUM, LOW_RATE, LOW_SPECTRUM};
use crate::pipeline::PipelineError;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::warn;

const QUEUE_TIME_NS: u64 = 40_000_000;

/// Limita também a fila interna que o playsink insere antes dos controles de saída.
pub(crate) fn limit_playsink_audio_queue(pipeline: &impl IsA<gst::Bin>) {
    pipeline.connect_deep_element_added(|_, parent, child| {
        if parent.name().as_str() == "abin"
            && child.name().as_str() == "aqueue"
            && child
                .factory()
                .is_some_and(|factory| factory.name() == "queue")
        {
            child.set_property("max-size-buffers", 1_u32);
            child.set_property("max-size-time", 10_000_000_u64);
            child.set_property("max-size-bytes", 0_u32);
        }
    });
}

/// Mantém o fader após a fila curta e mede os níveis efetivamente enviados ao dispositivo.
pub(crate) fn build_audio_preview_sink() -> Result<(gst::Bin, gst::Element), PipelineError> {
    let bin = gst::Bin::with_name("meraki-audio-preview-bin");
    let convert = make_element("audioconvert", "meraki-preview-aconv")?;
    let resample = make_element("audioresample", "meraki-preview-aresample")?;
    let caps = make_element("capsfilter", "meraki-preview-float")?;
    caps.set_property("caps", float_audio_caps(MIX_RATE));

    let queue = make_element("queue", "meraki-master-queue")?;
    queue.set_property("max-size-time", QUEUE_TIME_NS);
    queue.set_property("max-size-buffers", 1_u32);
    queue.set_property("max-size-bytes", 0_u32);
    let volume = make_element("volume", "meraki-preview-volume")?;
    let output = make_element("audioconvert", "meraki-preview-output")?;
    let sink =
        audio_preview_output_sink(std::env::var("MERAKI_HEADLESS_AUDIO").as_deref() == Ok("1"))?;

    add_and_link(&bin, [&convert, &resample, &caps])?;
    let mut main = vec![queue, volume.clone()];
    if gst::ElementFactory::find("level").is_some() {
        let level = make_element("level", MASTER_LEVEL_ELEMENT_NAME)?;
        level.set_property("post-messages", true);
        level.set_property("interval", LEVEL_INTERVAL_NS);
        main.push(level);
    } else {
        warn!("plugin 'level' indisponível; medidor dB master desabilitado");
    }
    main.extend([output, sink]);
    add_and_link(&bin, main.iter())?;
    protect_peaks(&volume, 1.0)?;
    if gst::ElementFactory::find("spectrum").is_some() {
        attach_spectrum(&bin, &caps, &main[0])?;
    } else {
        warn!("plugin 'spectrum' indisponível; análise espectral desabilitada");
        caps.link(&main[0])
            .map_err(|err| FrameSinkError::Link(err.to_string()))?;
    }
    attach_sink_ghost_pad(&bin, &convert, "audioconvert")?;
    Ok((bin, volume))
}

fn add_and_link<'a>(
    bin: &gst::Bin,
    elements: impl IntoIterator<Item = &'a gst::Element>,
) -> Result<(), FrameSinkError> {
    let elements: Vec<_> = elements.into_iter().collect();
    bin.add_many(elements.iter().copied())
        .map_err(|err| FrameSinkError::BinAdd(err.to_string()))?;
    gst::Element::link_many(elements).map_err(|err| FrameSinkError::Link(err.to_string()))
}

fn attach_spectrum(
    bin: &gst::Bin,
    input: &gst::Element,
    output: &gst::Element,
) -> Result<(), FrameSinkError> {
    let tee = make_element("tee", "meraki-analysis-split")?;
    let high = make_element("spectrum", HIGH_SPECTRUM)?;
    audio_spectrum::configure(&high);
    add_and_link(bin, [&tee, &high])?;
    input
        .link(&tee)
        .map_err(|err| FrameSinkError::Link(err.to_string()))?;
    high.link(output)
        .map_err(|err| FrameSinkError::Link(err.to_string()))?;

    let queue = make_element("queue", "meraki-analysis-queue")?;
    queue.set_property_from_str("leaky", "downstream");
    queue.set_property("max-size-time", 200_000_000_u64);
    queue.set_property("max-size-buffers", 0_u32);
    queue.set_property("max-size-bytes", 0_u32);
    let resample = make_element("audioresample", "meraki-analysis-resample")?;
    let caps = make_element("capsfilter", "meraki-analysis-caps")?;
    caps.set_property("caps", float_audio_caps(LOW_RATE));
    let low = make_element("spectrum", LOW_SPECTRUM)?;
    audio_spectrum::configure(&low);
    let sink = make_element("fakesink", "meraki-analysis-sink")?;
    sink.set_property("async", false);
    sink.set_property("sync", false);
    // O sink principal encaminha o seek. O tee já entrega o flush resultante
    // aos dois ramos, então o monitor não deve mandar o mesmo seek upstream.
    let sink_pad = sink
        .static_pad("sink")
        .ok_or(FrameSinkError::MissingSinkPad("fakesink"))?;
    let _ = sink_pad.add_probe(gst::PadProbeType::EVENT_UPSTREAM, |_, info| {
        if info
            .event()
            .is_some_and(|event| event.type_() == gst::EventType::Seek)
        {
            gst::PadProbeReturn::Handled
        } else {
            gst::PadProbeReturn::Ok
        }
    });
    add_and_link(bin, [&queue, &resample, &caps, &low, &sink])?;
    tee.link(&queue)
        .map_err(|err| FrameSinkError::Link(err.to_string()))
}

pub(super) fn audio_preview_output_sink(headless: bool) -> Result<gst::Element, FrameSinkError> {
    if !headless {
        match make_element("autoaudiosink", "meraki-preview-asink") {
            Ok(sink) => {
                if let Some(auto) = sink.downcast_ref::<gst::Bin>() {
                    auto.connect_deep_element_added(|_, _, child| configure_latency(child));
                }
                configure_latency(&sink);
                return Ok(sink);
            }
            Err(err) => warn!(error = %err, "autoaudiosink indisponível; usando fakesink em áudio"),
        }
    }
    let sink = make_element("fakesink", "meraki-preview-afakesink")?;
    // O sink de vídeo determina o preroll mesmo se o áudio ainda não tiver buffers.
    sink.set_property("async", false);
    sink.set_property("sync", true);
    configure_latency(&sink);
    Ok(sink)
}

fn configure_latency(sink: &gst::Element) {
    for (name, value) in [
        ("buffer-time", 40_000_u32),
        ("latency-time", 10_000_u32),
        ("processing-deadline", 0_u32),
    ] {
        let Some(spec) = sink.find_property(name) else {
            continue;
        };
        if !spec.flags().contains(glib::ParamFlags::WRITABLE)
            || spec.flags().contains(glib::ParamFlags::CONSTRUCT_ONLY)
        {
            continue;
        }
        if let Some(spec) = spec.downcast_ref::<glib::ParamSpecInt64>() {
            sink.set_property(name, i64::from(value).clamp(spec.minimum(), spec.maximum()));
        } else if let Some(spec) = spec.downcast_ref::<glib::ParamSpecUInt64>() {
            sink.set_property(name, u64::from(value).clamp(spec.minimum(), spec.maximum()));
        } else if let Some(spec) = spec.downcast_ref::<glib::ParamSpecInt>() {
            sink.set_property(name, (value as i32).clamp(spec.minimum(), spec.maximum()));
        } else if let Some(spec) = spec.downcast_ref::<glib::ParamSpecUInt>() {
            sink.set_property(name, value.clamp(spec.minimum(), spec.maximum()));
        }
    }
}
