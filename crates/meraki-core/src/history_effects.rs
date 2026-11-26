use crate::effects::{
    ClipEffect, ClipTransition, Keyframe, KeyframeProperty, TitleOverlay, TransitionEdge,
    VideoEffectKind,
};
use crate::history::{Command, HistoryError};
use crate::project::Project;
use crate::timeline::{Clip, ClipId, TrackId, TrackKind};

/// Insere um clipe de título na trilha `T1` (criando-a se necessário) com Undo/Redo reversível.
#[derive(Debug)]
pub struct AddTitleClipCommand {
    track_id: Option<TrackId>,
    clip: Clip,
    resolved_track_id: Option<TrackId>,
    created_track: bool,
}

impl AddTitleClipCommand {
    pub fn new(
        track_id: Option<TrackId>,
        timeline_start_ns: u64,
        duration_ns: u64,
        title: TitleOverlay,
    ) -> Self {
        Self {
            track_id,
            clip: Clip::new_title(timeline_start_ns, duration_ns, title),
            resolved_track_id: None,
            created_track: false,
        }
    }

    pub fn clip_id(&self) -> ClipId {
        self.clip.id
    }
}

impl Command for AddTitleClipCommand {
    fn description(&self) -> &str {
        "Adicionar título na timeline"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let had_overlay = project.timeline.tracks.iter().any(|t| t.kind == TrackKind::Overlay);
        let (tid, _) = project.timeline.add_title_clip(self.track_id, self.clip.clone())?;
        self.resolved_track_id = Some(tid);
        self.created_track = self.track_id.is_none() && !had_overlay;
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.timeline.remove_clip(self.clip.id)?;
        if self.created_track {
            if let Some(tid) = self.resolved_track_id {
                let _ = project.timeline.remove_empty_track(tid);
            }
        }
        Ok(())
    }
}

/// Atualiza texto, estilo, fonte ou cor de um clipe de título com Undo/Redo.
#[derive(Debug)]
pub struct UpdateTitleCommand {
    clip_id: ClipId,
    new_title: TitleOverlay,
    previous_title: Option<TitleOverlay>,
}

impl UpdateTitleCommand {
    pub fn new(clip_id: ClipId, new_title: TitleOverlay) -> Self {
        Self { clip_id, new_title, previous_title: None }
    }
}

impl Command for UpdateTitleCommand {
    fn description(&self) -> &str {
        "Atualizar título"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.update_title(self.clip_id, self.new_title.clone())?;
        self.previous_title = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_title
            .as_ref()
            .ok_or(HistoryError::InvalidCommandState("título anterior ausente"))?
            .clone();
        let _ = project.timeline.update_title(self.clip_id, prev)?;
        Ok(())
    }
}

/// Ativa/desativa um efeito de vídeo em um clipe restaurando o estado anterior ao desfazer.
#[derive(Debug)]
pub struct ToggleClipEffectCommand {
    clip_id: ClipId,
    kind: VideoEffectKind,
    intensity: f32,
    previous: Option<Option<ClipEffect>>,
}

impl ToggleClipEffectCommand {
    pub fn new(clip_id: ClipId, kind: VideoEffectKind, intensity: f32) -> Self {
        Self { clip_id, kind, intensity, previous: None }
    }
}

impl Command for ToggleClipEffectCommand {
    fn description(&self) -> &str {
        "Alternar efeito do clipe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev, _) = project.timeline.toggle_clip_effect(self.clip_id, self.kind, self.intensity)?;
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous
            .clone()
            .ok_or(HistoryError::InvalidCommandState("estado anterior do efeito ausente"))?;
        match prev {
            Some(effect) => {
                let _ = project.timeline.set_clip_effect(self.clip_id, effect)?;
            }
            None => {
                let _ = project.timeline.remove_clip_effect(self.clip_id, self.kind)?;
            }
        }
        Ok(())
    }
}

/// Define ou remove transição de entrada/saída de um clipe com Undo/Redo.
#[derive(Debug)]
pub struct SetClipTransitionCommand {
    clip_id: ClipId,
    edge: TransitionEdge,
    transition: Option<ClipTransition>,
    previous: Option<Option<ClipTransition>>,
}

impl SetClipTransitionCommand {
    pub fn new(clip_id: ClipId, edge: TransitionEdge, transition: Option<ClipTransition>) -> Self {
        Self { clip_id, edge, transition, previous: None }
    }
}

impl Command for SetClipTransitionCommand {
    fn description(&self) -> &str {
        "Configurar transição do clipe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.set_clip_transition(self.clip_id, self.edge, self.transition)?;
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("transição anterior ausente"))?;
        let _ = project.timeline.set_clip_transition(self.clip_id, self.edge, prev)?;
        Ok(())
    }
}

/// Adiciona ou atualiza um keyframe em um clipe preservando o valor anterior para Undo.
#[derive(Debug)]
pub struct UpsertKeyframeCommand {
    clip_id: ClipId,
    keyframe: Keyframe,
    previous: Option<Option<Keyframe>>,
}

impl UpsertKeyframeCommand {
    pub fn new(clip_id: ClipId, keyframe: Keyframe) -> Self {
        Self { clip_id, keyframe, previous: None }
    }
}

impl Command for UpsertKeyframeCommand {
    fn description(&self) -> &str {
        "Definir keyframe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.upsert_keyframe(self.clip_id, self.keyframe)?;
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("keyframe anterior ausente"))?;
        match prev {
            Some(old) => {
                let _ = project.timeline.upsert_keyframe(self.clip_id, old)?;
            }
            None => {
                let _ = project.timeline.remove_keyframe(
                    self.clip_id,
                    self.keyframe.property,
                    self.keyframe.time_offset_ns,
                )?;
            }
        }
        Ok(())
    }
}

/// Remove um keyframe de um clipe restaurando-o ao desfazer.
#[derive(Debug)]
pub struct RemoveKeyframeCommand {
    clip_id: ClipId,
    property: KeyframeProperty,
    time_offset_ns: u64,
    removed: Option<Keyframe>,
}

impl RemoveKeyframeCommand {
    pub fn new(clip_id: ClipId, property: KeyframeProperty, time_offset_ns: u64) -> Self {
        Self { clip_id, property, time_offset_ns, removed: None }
    }
}

impl Command for RemoveKeyframeCommand {
    fn description(&self) -> &str {
        "Remover keyframe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = project.timeline.remove_keyframe(self.clip_id, self.property, self.time_offset_ns)?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = self
            .removed
            .ok_or(HistoryError::InvalidCommandState("keyframe removido ausente"))?;
        let _ = project.timeline.upsert_keyframe(self.clip_id, removed)?;
        Ok(())
    }
}
