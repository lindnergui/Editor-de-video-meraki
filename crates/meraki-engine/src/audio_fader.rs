use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_controller as ctrl;
use gstreamer_controller::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const RAMP_NS: u64 = 5_000_000;
const MUTED: u64 = 1 << 63;

#[derive(Clone)]
pub(crate) struct AudioFader {
    pub(crate) element: gst::Element,
    target: Arc<AtomicU64>,
    ramped: bool,
}

impl AudioFader {
    pub(crate) fn new(element: gst::Element) -> Self {
        let target = Arc::new(AtomicU64::new(1.0_f64.to_bits()));
        let ramped = install_ramp(&element, Arc::clone(&target)).is_some();
        Self {
            element,
            target,
            ramped,
        }
    }

    pub(crate) fn set(&self, gain: f64, muted: bool) {
        self.target.store(
            (gain.to_bits() & !MUTED) | if muted { MUTED } else { 0 },
            Ordering::Release,
        );
        if !self.ramped || self.element.current_state() < gst::State::Playing {
            self.element.set_property("volume", gain);
            self.element.set_property("mute", muted);
        }
    }

    pub(crate) fn set_gain(&self, gain: f64) {
        self.set(gain, self.target.load(Ordering::Acquire) & MUTED != 0);
    }

    pub(crate) fn set_mute(&self, muted: bool) {
        self.set(
            f64::from_bits(self.target.load(Ordering::Acquire) & !MUTED),
            muted,
        );
    }
}

#[derive(Default)]
struct Envelope {
    target: Option<u64>,
    ends: Option<gst::ClockTime>,
}

fn install_ramp(element: &gst::Element, target: Arc<AtomicU64>) -> Option<()> {
    let sink = element.static_pad("sink")?;
    let source = ctrl::InterpolationControlSource::new();
    source.set_mode(ctrl::InterpolationMode::Linear);
    let binding = ctrl::DirectControlBinding::new_absolute(element, "volume", &source);
    if element.add_control_binding(&binding).is_err() {
        return None;
    }
    let state = Mutex::new(Envelope::default());
    let weak = element.downgrade();
    let probe = sink.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let (Some(element), Some(buffer), Ok(mut envelope)) =
            (weak.upgrade(), info.buffer(), state.lock())
        else {
            return gst::PadProbeReturn::Ok;
        };
        let target = target.load(Ordering::Acquire);
        let reset = buffer.flags().contains(gst::BufferFlags::DISCONT) || envelope.target.is_none();
        if !reset && envelope.target == Some(target) && envelope.ends.is_none() {
            return gst::PadProbeReturn::Ok;
        }
        let Some(time) = pad
            .sticky_event::<gst::event::Segment>(0)
            .and_then(|segment| {
                segment
                    .segment()
                    .downcast_ref::<gst::ClockTime>()?
                    .to_stream_time(buffer.pts())
            })
        else {
            return gst::PadProbeReturn::Ok;
        };
        let muted = target & MUTED != 0;
        let gain = f64::from_bits(target & !MUTED);
        if reset {
            source.unset_all();
            let _ = source.set(time, if muted { 0.0 } else { gain });
            element.set_property("mute", muted);
            envelope.ends = None;
        } else if envelope.target != Some(target) {
            let current = ControlSourceExt::value(&source, time).unwrap_or(gain);
            let end = time.saturating_add(gst::ClockTime::from_nseconds(RAMP_NS));
            source.unset_all();
            let _ = source.set(time, current);
            let _ = source.set(end, if muted { 0.0 } else { gain });
            element.set_property("mute", false);
            envelope.ends = Some(end);
        } else if envelope.ends.is_some_and(|end| time >= end) {
            element.set_property("mute", muted);
            envelope.ends = None;
        }
        envelope.target = Some(target);
        gst::PadProbeReturn::Ok
    });
    if probe.is_none() {
        let _ = element.remove_control_binding(&binding);
        return None;
    }
    Some(())
}
