use crate::frame_sink::MASTER_LEVEL_ELEMENT_NAME;
use crate::protocol::{AudioMeterFrame, EngineEvent, EngineWorkerChannel, TrackAudioLevel};
use gstreamer as gst;
use gstreamer::prelude::GstObjectExt;
use meraki_core::TrackId;
use std::collections::HashMap;

pub(crate) const MIN_METER_DB: f32 = -60.0;
pub(crate) const MAX_METER_DB: f32 = 0.0;

/// Agrega mensagens `"level"` (~30 Hz) do elemento GStreamer `level` em `AudioMeterFrame`.
pub(crate) struct AudioMeterAggregator {
    master_peak_db: f32,
    master_rms_db: f32,
    track_order: Vec<TrackId>,
    track_levels: HashMap<TrackId, TrackAudioLevel>,
    level_element_to_track: HashMap<String, TrackId>,
}

impl AudioMeterAggregator {
    pub(crate) fn new() -> Self {
        Self {
            master_peak_db: MIN_METER_DB,
            master_rms_db: MIN_METER_DB,
            track_order: Vec::new(),
            track_levels: HashMap::new(),
            level_element_to_track: HashMap::new(),
        }
    }

    pub(crate) fn reset_for_sync(&mut self, track_ids: &[TrackId]) {
        self.master_peak_db = MIN_METER_DB;
        self.master_rms_db = MIN_METER_DB;
        self.track_order.clear();
        self.track_levels.clear();
        self.level_element_to_track.clear();

        for &track_id in track_ids {
            if !self.track_levels.contains_key(&track_id) {
                self.track_order.push(track_id);
                self.track_levels.insert(
                    track_id,
                    TrackAudioLevel { track_id, peak_db: MIN_METER_DB, rms_db: MIN_METER_DB },
                );
            }
        }
    }

    pub(crate) fn register_track_level(&mut self, element_name: String, track_id: TrackId) {
        if !self.track_levels.contains_key(&track_id) {
            self.track_order.push(track_id);
            self.track_levels.insert(
                track_id,
                TrackAudioLevel { track_id, peak_db: MIN_METER_DB, rms_db: MIN_METER_DB },
            );
        }
        self.level_element_to_track.insert(element_name, track_id);
    }

    pub(crate) fn handle_element_message(
        &mut self,
        msg: &gst::message::Element,
        worker: &EngineWorkerChannel,
    ) {
        let Some(structure) = msg.structure() else {
            return;
        };
        if structure.name() != "level" {
            return;
        }
        let Some((peak_db, rms_db)) = parse_level_structure(structure) else {
            return;
        };
        let Some(src) = msg.src() else {
            return;
        };
        let src_name = src.name();

        if src_name.as_str() == MASTER_LEVEL_ELEMENT_NAME {
            self.master_peak_db = peak_db;
            self.master_rms_db = rms_db;
            worker.emit_drop_if_full(EngineEvent::AudioLevels(self.snapshot()));
        } else if let Some(&track_id) = self.level_element_to_track.get(src_name.as_str()) {
            self.track_levels.insert(track_id, TrackAudioLevel { track_id, peak_db, rms_db });
        }
    }

    pub(crate) fn snapshot(&self) -> AudioMeterFrame {
        let tracks = self
            .track_order
            .iter()
            .filter_map(|id| self.track_levels.get(id).copied())
            .collect();
        AudioMeterFrame {
            master_peak_db: self.master_peak_db,
            master_rms_db: self.master_rms_db,
            tracks,
        }
    }
}

pub(crate) fn parse_level_structure(structure: &gst::StructureRef) -> Option<(f32, f32)> {
    if structure.name() != "level" {
        return None;
    }
    let peak_db = max_channel_db(structure, "peak");
    let rms_db = max_channel_db(structure, "rms");
    Some((peak_db, rms_db))
}

fn max_channel_db(structure: &gst::StructureRef, field: &str) -> f32 {
    let Ok(array) = structure.get::<glib::ValueArray>(field) else {
        return MIN_METER_DB;
    };
    max_channel_db_from_slice(array.as_slice())
}

fn max_channel_db_from_slice(values: &[glib::Value]) -> f32 {
    let mut peak: Option<f32> = None;
    for value in values {
        if let Ok(db) = value.get::<f64>() {
            let clamped = if db.is_nan() {
                continue;
            } else if !db.is_finite() {
                if db.is_sign_negative() {
                    MIN_METER_DB
                } else {
                    continue;
                }
            } else {
                (db as f32).clamp(MIN_METER_DB, MAX_METER_DB)
            };
            peak = Some(peak.map_or(clamped, |cur| cur.max(clamped)));
        }
    }
    peak.unwrap_or(MIN_METER_DB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gstreamer::prelude::*;

    #[test]
    fn parses_multichannel_level_structure_and_clamps_extremes() {
        crate::ensure_media_initialized().unwrap();

        let peaks = glib::ValueArray::new([-18.5_f64, -3.25_f64]);
        let rms = glib::ValueArray::new([-24.0_f64, -12.5_f64]);
        assert!((max_channel_db_from_slice(peaks.as_slice()) - (-3.25)).abs() < 1e-4);
        assert!((max_channel_db_from_slice(rms.as_slice()) - (-12.5)).abs() < 1e-4);

        let pipeline = gst::parse::launch(
            "audiotestsrc wave=sine num-buffers=8 ! audioconvert ! level interval=10000000 post-messages=true ! fakesink",
        )
        .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let mut parsed = None;
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(3)) {
            match msg.view() {
                gst::MessageView::Element(el) => {
                    if let Some(st) = el.structure() {
                        if let Some(vals) = parse_level_structure(st) {
                            parsed = Some(vals);
                            break;
                        }
                    }
                }
                gst::MessageView::Eos(_) | gst::MessageView::Error(_) => break,
                _ => {}
            }
        }
        let _ = pipeline.set_state(gst::State::Null);
        let (real_peak, real_rms) = parsed.expect("elemento level deve emitir mensagem 'level'");
        assert!((-30.0..=0.0).contains(&real_peak));
        assert!((-40.0..=0.0).contains(&real_rms));
    }

    #[test]
    fn falls_back_to_floor_db_when_all_channels_are_silent_or_invalid() {
        crate::ensure_media_initialized().unwrap();
        let silent = glib::ValueArray::new([f64::NEG_INFINITY, -120.0_f64]);
        let invalid = glib::ValueArray::new([f64::NAN, f64::INFINITY]);

        assert_eq!(max_channel_db_from_slice(silent.as_slice()), MIN_METER_DB);
        assert_eq!(max_channel_db_from_slice(invalid.as_slice()), MIN_METER_DB);
    }
}
