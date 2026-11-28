use crate::audio_output::MIX_RATE;
use crate::protocol::{EngineEvent, EngineWorkerChannel, SpectrumBands};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(crate) const LOW_RATE: i32 = 4_000;
pub(crate) const LOW_SPECTRUM: &str = "meraki-spectrum-low";
pub(crate) const HIGH_SPECTRUM: &str = "meraki-spectrum-high";
const SPECTRUM_BINS: u32 = 32;
const INTERVAL_NS: u64 = 100_000_000;
const EVENT_PERIOD: Duration = Duration::from_millis(72);
const MIN_DB: f64 = -80.0;
const WINDOWS: usize = 4;

pub(crate) fn configure(element: &gst::Element) {
    element.set_property("bands", SPECTRUM_BINS);
    element.set_property("interval", INTERVAL_NS);
    element.set_property("post-messages", true);
    element.set_property("message-magnitude", true);
    element.set_property("threshold", MIN_DB as i32);
}

#[derive(Default)]
pub(crate) struct AudioSpectrumMonitor {
    low: Option<(u64, [f64; 2])>,
    high: Option<(u64, [f64; 2])>,
    recent: VecDeque<[f64; 4]>,
    last_sent: Option<Instant>,
}

impl AudioSpectrumMonitor {
    pub(crate) fn receive(
        &mut self,
        message: &gst::message::Element,
        worker: &EngineWorkerChannel,
    ) {
        let Some(values) = message.structure().filter(|s| s.name() == "spectrum") else {
            return;
        };
        let Some(src) = message.src() else { return };
        let Ok(magnitudes) = values.get::<gst::List>("magnitude") else {
            return;
        };
        let Ok(time) = values.get::<u64>("running-time") else {
            return;
        };
        match src.name().as_str() {
            LOW_SPECTRUM => {
                self.low = Some((
                    time,
                    group_powers(&magnitudes, LOW_RATE, [0.0, 170.0, 840.0]),
                ))
            }
            HIGH_SPECTRUM => {
                self.high = Some((
                    time,
                    group_powers(&magnitudes, MIX_RATE, [840.0, 4325.0, 24_001.0]),
                ))
            }
            _ => return,
        }
        let (Some((low_time, low)), Some((high_time, high))) = (self.low, self.high) else {
            return;
        };
        if low_time.abs_diff(high_time) >= INTERVAL_NS / 2 {
            return;
        }
        self.low = None;
        self.high = None;
        if self.recent.len() == WINDOWS {
            self.recent.pop_front();
        }
        self.recent.push_back([low[0], low[1], high[0], high[1]]);
        let now = Instant::now();
        if self
            .last_sent
            .is_none_or(|previous| now.duration_since(previous) >= EVENT_PERIOD)
        {
            self.publish(worker);
            self.last_sent = Some(now);
        }
    }

    pub(crate) fn publish(&self, worker: &EngineWorkerChannel) {
        if self.recent.is_empty() {
            return;
        }
        let mut average = [0.0; 4];
        for window in &self.recent {
            for (sum, power) in average.iter_mut().zip(window) {
                *sum += power;
            }
        }
        let decibels =
            average.map(|power| (10.0 * (power / self.recent.len() as f64).log10()).max(MIN_DB));
        worker.emit_drop_if_full(EngineEvent::AudioSpectrum(SpectrumBands::new(decibels)));
    }
}

fn group_powers(magnitudes: &gst::List, rate: i32, limits: [f64; 3]) -> [f64; 2] {
    // A FFT real do plugin usa 2 × (bands − 1) amostras, incluindo DC e Nyquist.
    let bin_width = f64::from(rate) / f64::from(2 * (SPECTRUM_BINS - 1));
    let mut powers = [0.0; 2];
    for (index, magnitude) in magnitudes.as_slice().iter().enumerate() {
        let hz = index as f64 * bin_width;
        if let Ok(db) = magnitude.get::<f32>() {
            if db.is_finite() && hz >= limits[0] && hz < limits[2] {
                let band = usize::from(hz >= limits[1]);
                powers[band] += 10.0_f64.powf(f64::from(db) / 10.0);
            }
        }
    }
    powers
}
