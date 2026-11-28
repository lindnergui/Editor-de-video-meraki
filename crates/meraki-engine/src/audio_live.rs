use crate::pipeline::{PipelineError, PlaybackEngine};
use crate::protocol::{AudioEnhancements, EngineCommand, EngineEvent, EngineWorkerChannel};
use meraki_core::TrackId;
use std::collections::HashMap;

#[derive(Default)]
struct TrackEdit {
    volume: Option<(f64, bool)>,
    eq: Option<[f64; 4]>,
    enhancements: Option<AudioEnhancements>,
}

#[derive(Default)]
pub(crate) struct AudioEdits {
    tracks: HashMap<TrackId, TrackEdit>,
    master_db: Option<f64>,
    master_muted: Option<bool>,
}

impl AudioEdits {
    /// Consolida por trilha e por grupo de propriedades até uma barreira da fila.
    pub(crate) fn collect(&mut self, command: EngineCommand) -> Option<EngineCommand> {
        let EngineCommand::UpdateAudioLive {
            track_volumes,
            master_volume_db,
            master_muted,
            track_eq,
            track_enhancements,
        } = command
        else {
            return Some(command);
        };

        if let Some((id, db, muted)) = track_volumes {
            self.tracks.entry(id).or_default().volume = Some((db, muted));
        }
        if let Some((id, gains)) = track_eq {
            self.tracks.entry(id).or_default().eq = Some(gains);
        }
        if let Some((id, config)) = track_enhancements {
            self.tracks.entry(id).or_default().enhancements = Some(config);
        }
        self.master_db = master_volume_db.or(self.master_db);
        self.master_muted = master_muted.or(self.master_muted);
        None
    }

    pub(crate) fn flush(&mut self, playback: &mut PlaybackEngine, worker: &EngineWorkerChannel) {
        let report = |result: Result<(), PipelineError>| {
            if let Err(err) = result {
                worker.emit_critical(EngineEvent::Error(err.to_string()));
            }
        };
        if self.master_db.is_some() || self.master_muted.is_some() {
            report(playback.update_master(self.master_db.take(), self.master_muted.take()));
        }
        let dsp = playback.audio_dsp_mut();
        for (id, edit) in self.tracks.drain() {
            if let Some((gain_db, muted)) = edit.volume {
                report(dsp.set_track_volume(id, gain_db, muted));
            }
            if let Some(gains) = edit.eq {
                report(dsp.set_track_eq(id, gains));
            }
            if let Some(config) = edit.enhancements {
                report(dsp.set_track_enhancements(id, config));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_latest_values_per_track_and_preserves_independent_master_mute() {
        let track = TrackId::new();
        let other = TrackId::new();
        let mut edits = AudioEdits::default();
        for index in 0..20 {
            edits.collect(EngineCommand::UpdateAudioLive {
                track_volumes: Some((track, -f64::from(index), false)),
                track_eq: Some((other, [f64::from(index); 4])),
                master_volume_db: Some(-f64::from(index)),
                master_muted: (index == 0).then_some(true),
                track_enhancements: None,
            });
        }
        assert!(matches!(
            edits.collect(EngineCommand::Play),
            Some(EngineCommand::Play)
        ));
        assert_eq!(edits.master_db, Some(-19.0));
        assert_eq!(edits.master_muted, Some(true));
        assert_eq!(edits.tracks[&track].volume, Some((-19.0, false)));
        assert_eq!(edits.tracks[&other].eq, Some([19.0; 4]));
    }
}
