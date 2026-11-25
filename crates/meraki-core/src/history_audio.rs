use crate::audio::{AudioEffectKind, TrackAudioSettings, VoicePreset, VolumeKeyframe};
use crate::history::{Command, HistoryError};
use crate::project::Project;
use crate::timeline::{ClipId, TrackId};

/// Atualiza toda a configuração DSP/mixagem de uma trilha com Undo/Redo reversível.
#[derive(Debug)]
pub struct UpdateTrackAudioCommand {
    track_id: TrackId,
    new_settings: TrackAudioSettings,
    previous_settings: Option<TrackAudioSettings>,
}

impl UpdateTrackAudioCommand {
    pub fn new(track_id: TrackId, new_settings: TrackAudioSettings) -> Self {
        Self { track_id, new_settings, previous_settings: None }
    }
}

impl Command for UpdateTrackAudioCommand {
    fn description(&self) -> &str {
        "Atualizar mixagem de áudio da trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.update_track_audio(self.track_id, self.new_settings.clone())?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("configuração anterior da trilha ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Aplica um preset de voz (`Podcast`, `ClearVoice`, `EnhanceVoice`, `Radio`) com Undo/Redo.
#[derive(Debug)]
pub struct ApplyVoicePresetCommand {
    track_id: TrackId,
    preset: VoicePreset,
    previous_settings: Option<TrackAudioSettings>,
}

impl ApplyVoicePresetCommand {
    pub fn new(track_id: TrackId, preset: VoicePreset) -> Self {
        Self { track_id, preset, previous_settings: None }
    }
}

impl Command for ApplyVoicePresetCommand {
    fn description(&self) -> &str {
        "Aplicar preset de voz na trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.apply_voice_preset(self.track_id, self.preset)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("configuração anterior de preset ausente"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Alterna um efeito de áudio (`Reverb` / `Echo`) na trilha com Undo/Redo.
#[derive(Debug)]
pub struct ToggleAudioEffectCommand {
    track_id: TrackId,
    effect: AudioEffectKind,
    previous_settings: Option<TrackAudioSettings>,
}

impl ToggleAudioEffectCommand {
    pub fn new(track_id: TrackId, effect: AudioEffectKind) -> Self {
        Self { track_id, effect, previous_settings: None }
    }
}

impl Command for ToggleAudioEffectCommand {
    fn description(&self) -> &str {
        "Alternar efeito de áudio da trilha"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev, _) = project.timeline.toggle_track_audio_effect(self.track_id, self.effect)?;
        self.previous_settings = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_settings
            .clone()
            .ok_or(HistoryError::InvalidCommandState("efeitos anteriores de áudio ausentes"))?;
        let _ = project.timeline.update_track_audio(self.track_id, prev)?;
        Ok(())
    }
}

/// Insere ou atualiza um keyframe na curva de automação de volume do clipe com Undo/Redo.
#[derive(Debug)]
pub struct UpsertVolumeKeyframeCommand {
    clip_id: ClipId,
    keyframe: VolumeKeyframe,
    previous: Option<Option<VolumeKeyframe>>,
}

impl UpsertVolumeKeyframeCommand {
    pub fn new(clip_id: ClipId, keyframe: VolumeKeyframe) -> Self {
        Self { clip_id, keyframe, previous: None }
    }
}

impl Command for UpsertVolumeKeyframeCommand {
    fn description(&self) -> &str {
        "Adicionar ou atualizar keyframe de volume"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.upsert_volume_keyframe(self.clip_id, self.keyframe)?;
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("estado anterior do keyframe de volume ausente"))?;
        match prev {
            Some(old) => {
                let _ = project.timeline.upsert_volume_keyframe(self.clip_id, old)?;
            }
            None => {
                let _ = project
                    .timeline
                    .remove_volume_keyframe(self.clip_id, self.keyframe.time_offset_ns)?;
            }
        }
        Ok(())
    }
}

/// Remove um keyframe de automação de volume do clipe restaurando-o no Undo.
#[derive(Debug)]
pub struct RemoveVolumeKeyframeCommand {
    clip_id: ClipId,
    time_offset_ns: u64,
    removed: Option<VolumeKeyframe>,
}

impl RemoveVolumeKeyframeCommand {
    pub fn new(clip_id: ClipId, time_offset_ns: u64) -> Self {
        Self { clip_id, time_offset_ns, removed: None }
    }
}

impl Command for RemoveVolumeKeyframeCommand {
    fn description(&self) -> &str {
        "Remover keyframe de volume"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = project.timeline.remove_volume_keyframe(self.clip_id, self.time_offset_ns)?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let kf = self
            .removed
            .ok_or(HistoryError::InvalidCommandState("keyframe de volume removido ausente"))?;
        let _ = project.timeline.upsert_volume_keyframe(self.clip_id, kf)?;
        Ok(())
    }
}
