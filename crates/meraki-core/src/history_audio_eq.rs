use crate::audio::TrackAudioSettings;
use crate::audio_eq::AudioPreset;
use crate::history::{Command, HistoryError};
use crate::project::{EqBand, Project};
use crate::timeline::TrackId;

/// Alias semântico para reset reversível de uma única banda do EQ.
pub type ResetEqBand = ResetEqBandCommand;
/// Alias semântico para alteração consolidada das 4 bandas do EQ.
pub type SetTrackEq = SetTrackEqCommand;
/// Alias semântico para aplicação de `AudioPreset` na trilha.
pub type ApplyAudioPreset = ApplyAudioPresetCommand;
/// Alias semântico para ajuste consolidado de fader da trilha.
pub type SetTrackVolume = SetTrackVolumeCommand;
/// Alias semântico para ajuste consolidado do volume Master.
pub type SetMasterVolume = SetMasterVolumeCommand;

/// Reseta uma única banda do equalizador para `0.0 dB` preservando as demais, com Undo/Redo.
#[derive(Debug)]
pub struct ResetEqBandCommand {
    track_id: TrackId,
    band: EqBand,
    previous_settings: Option<TrackAudioSettings>,
}

impl ResetEqBandCommand {
    pub fn new(track_id: TrackId, band: EqBand) -> Self {
        Self { track_id, band, previous_settings: None }
    }
}

impl Command for ResetEqBandCommand {
    fn description(&self) -> &str {
        "Resetar banda do equalizador"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.reset_eq_band(self.track_id, self.band)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("configuração anterior da banda EQ ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Atualiza de forma consolidada todas as 4 bandas de EQ da trilha com Undo/Redo.
#[derive(Debug)]
pub struct SetTrackEqCommand {
    track_id: TrackId,
    eq_gains_db: [f32; 4],
    previous_settings: Option<TrackAudioSettings>,
}

impl SetTrackEqCommand {
    pub fn new(track_id: TrackId, eq_gains_db: [f32; 4]) -> Self {
        Self { track_id, eq_gains_db, previous_settings: None }
    }

    pub fn new_clamped(track_id: TrackId, eq_gains_db: [f32; 4]) -> Self {
        Self::new(track_id, crate::audio_eq::clamp_eq_gains_db(eq_gains_db))
    }
}

impl Command for SetTrackEqCommand {
    fn description(&self) -> &str {
        "Ajustar equalizador da trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.set_track_eq(self.track_id, self.eq_gains_db)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("equalização anterior da trilha ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Aplica um `AudioPreset` completo na trilha preservando fader, Mute, Solo e efeitos.
#[derive(Debug)]
pub struct ApplyAudioPresetCommand {
    track_id: TrackId,
    preset: AudioPreset,
    previous_settings: Option<TrackAudioSettings>,
}

impl ApplyAudioPresetCommand {
    pub fn new(track_id: TrackId, preset: AudioPreset) -> Self {
        Self { track_id, preset, previous_settings: None }
    }
}

impl Command for ApplyAudioPresetCommand {
    fn description(&self) -> &str {
        "Aplicar preset de áudio na trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.apply_audio_preset(self.track_id, self.preset)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("configuração anterior ao preset ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Atualiza o ganho (fader em dB) de uma trilha de forma consolidada com Undo/Redo.
#[derive(Debug)]
pub struct SetTrackVolumeCommand {
    track_id: TrackId,
    gain_db: f32,
    previous_settings: Option<TrackAudioSettings>,
}

impl SetTrackVolumeCommand {
    pub fn new(track_id: TrackId, gain_db: f32) -> Self {
        Self { track_id, gain_db, previous_settings: None }
    }

    pub fn new_clamped(track_id: TrackId, gain_db: f32) -> Self {
        Self::new(track_id, crate::audio::clamp_track_gain_db(gain_db))
    }
}

impl Command for SetTrackVolumeCommand {
    fn description(&self) -> &str {
        "Ajustar volume da trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.set_track_volume(self.track_id, self.gain_db)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("volume anterior da trilha ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Atualiza o volume Master da timeline de forma consolidada com Undo/Redo.
#[derive(Debug)]
pub struct SetMasterVolumeCommand {
    gain_db: f32,
    previous_gain_db: Option<f32>,
}

impl SetMasterVolumeCommand {
    pub fn new(gain_db: f32) -> Self {
        Self { gain_db, previous_gain_db: None }
    }

    pub fn new_clamped(gain_db: f32) -> Self {
        Self::new(crate::audio::clamp_track_gain_db(gain_db))
    }
}

impl Command for SetMasterVolumeCommand {
    fn description(&self) -> &str {
        "Ajustar volume Master"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.set_master_volume(self.gain_db)?;
        self.previous_gain_db = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_gain_db
            .ok_or(HistoryError::InvalidCommandState("volume Master anterior ausente"))?;
        let _ = project.timeline.set_master_volume(prev)?;
        Ok(())
    }
}
