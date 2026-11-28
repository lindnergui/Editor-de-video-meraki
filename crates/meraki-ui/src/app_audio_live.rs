use crate::app::MerakiApp;
use crate::i18n;
use meraki_core::{
    auto_eq_gains_f32, compute_auto_eq, db_to_linear, ApplyAudioPresetCommand, AudioPreset,
    AutoEqTarget, EqBand, ResetEqBandCommand, SetMasterVolumeCommand, SetTrackEqCommand,
    TrackId, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB,
};
use meraki_engine::{EngineCommand, EngineEvent};

impl MerakiApp {
    /// Aplica um `AudioPreset` real na trilha selecionada com envio imediato ao motor e Undo.
    pub fn apply_audio_preset(&mut self, track_id: TrackId, preset: AudioPreset) -> Result<(), String> {
        self.audio.auto_eq_notice = None;
        self.execute_audio_command(Box::new(ApplyAudioPresetCommand::new(track_id, preset)))?;
        if let Some(track) = self.project.timeline.track(track_id) {
            let s = track.audio.clone();
            self.send_live_eq_and_voice(track_id, &s);
        }
        Ok(())
    }

    pub(crate) fn handle_eq_band_reset(&mut self, track_id: TrackId, band: EqBand) {
        self.audio.eq_drag_origin = None;
        self.audio.auto_eq_notice = None;
        if self
            .execute_audio_command(Box::new(ResetEqBandCommand::new(track_id, band)))
            .is_ok()
        {
            self.send_live_eq(track_id);
        }
    }

    pub(crate) fn handle_auto_eq(&mut self, track_id: TrackId, target: AutoEqTarget) {
        let Some(bands) = self.playback.audio_spectrum else {
            self.send_engine_command(EngineCommand::RequestAudioSpectrum);
            self.audio.auto_eq_notice =
                Some(i18n::strings(self.preferences.language).audio_auto_eq_need_play);
            return;
        };
        self.audio.auto_eq_notice = None;
        let gains = auto_eq_gains_f32(compute_auto_eq(bands, target));
        if self
            .execute_audio_command(Box::new(SetTrackEqCommand::new(track_id, gains)))
            .is_ok()
        {
            self.send_live_eq(track_id);
        }
    }

    pub(crate) fn handle_track_gain_reset(&mut self, track_id: TrackId) {
        self.audio.gain_drag_origin = None;
        if self
            .execute_audio_command(Box::new(meraki_core::SetTrackVolumeCommand::new(track_id, 0.0)))
            .is_ok()
        {
            self.send_live_volume(track_id);
        }
    }

    pub(crate) fn handle_master_gain_changed(&mut self, gain_db: f32) {
        let clamped = if gain_db.is_finite() {
            gain_db.clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB)
        } else {
            0.0
        };
        if self.audio.master_gain_drag_origin.is_none() {
            self.audio.master_gain_drag_origin = Some(self.project.timeline.master_gain_db);
        }
        self.project.timeline.master_gain_db = clamped;
        self.playback.master_volume = db_to_linear(clamped).clamp(0.0, 1.0) as f32;
        self.send_live_master_gain(clamped);
    }

    pub(crate) fn handle_master_gain_released(&mut self) {
        let Some(orig_db) = self.audio.master_gain_drag_origin.take() else {
            self.send_live_master_gain(self.project.timeline.master_gain_db);
            return;
        };
        let final_db = self.project.timeline.master_gain_db;
        if (orig_db - final_db).abs() <= 1e-4 {
            return;
        }
        let _ = self.project.timeline.set_master_volume(orig_db);
        let _ = self.execute_audio_command(Box::new(SetMasterVolumeCommand::new(final_db)));
        self.playback.master_volume = db_to_linear(final_db).clamp(0.0, 1.0) as f32;
        self.send_live_master_gain(final_db);
    }

    pub(crate) fn handle_master_gain_reset(&mut self) {
        self.audio.master_gain_drag_origin = None;
        if self
            .execute_audio_command(Box::new(SetMasterVolumeCommand::new(0.0)))
            .is_ok()
        {
            self.playback.master_volume = 1.0;
            self.send_live_master_gain(0.0);
        }
    }

    pub(crate) fn sync_audio_after_history_change(&mut self) {
        self.audio.eq_drag_origin = None;
        self.audio.gain_drag_origin = None;
        self.audio.nr_drag_origin = None;
        self.audio.master_gain_drag_origin = None;
        let master_db = self.project.timeline.master_gain_db;
        let new_vol = db_to_linear(master_db).clamp(0.0, 1.0) as f32;
        if (new_vol - self.playback.master_volume).abs() > 1e-4 {
            self.playback.master_volume = new_vol;
            self.send_live_master_gain(master_db);
        }
    }

    pub(crate) fn handle_audio_engine_event(&mut self, event: &EngineEvent) {
        if matches!(event, EngineEvent::AudioSpectrum(_)) {
            self.audio.auto_eq_notice = None;
        }
    }

    fn send_live_master_gain(&mut self, gain_db: f32) {
        self.send_engine_command(EngineCommand::UpdateAudioLive {
            track_volumes: None,
            master_volume_db: Some(f64::from(gain_db)),
            master_muted: None,
            track_eq: None,
            track_enhancements: None,
        });
    }
}
