use crate::effects::ClipTransform;
use crate::history::{Command, HistoryError};
use crate::project::{MediaKind, Project};
use crate::timeline::{Clip, ClipId, TimelineError, TimelineTransition, TrackId};

/// Move um clipe na timeline preservando posição, trilha e transições afetadas para Undo.
#[derive(Debug)]
pub struct MoveClipCommand {
    clip_id: ClipId,
    target_track_id: TrackId,
    new_start_ns: u64,
    previous: Option<(TrackId, u64)>,
    previous_transitions: Option<Vec<TimelineTransition>>,
}

impl MoveClipCommand {
    pub fn new(clip_id: ClipId, target_track_id: TrackId, new_start_ns: u64) -> Self {
        Self { clip_id, target_track_id, new_start_ns, previous: None, previous_transitions: None }
    }
}

impl Command for MoveClipCommand {
    fn description(&self) -> &str { "Mover clipe" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev_track, prev_start) = project
            .timeline
            .clip(self.clip_id)
            .map(|(tid, clip)| (tid, clip.timeline_start_ns))
            .ok_or(TimelineError::ClipNotFound(self.clip_id))?;
        let prev_transitions = project.timeline.transitions.clone();
        project.timeline.move_clip(self.clip_id, self.target_track_id, self.new_start_ns)?;
        self.previous = Some((prev_track, prev_start));
        self.previous_transitions = Some(prev_transitions);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev_track, prev_start) =
            self.previous.ok_or(HistoryError::InvalidCommandState("posição anterior ausente"))?;
        project.timeline.move_clip(self.clip_id, prev_track, prev_start)?;
        if let Some(ref transitions) = self.previous_transitions {
            project.timeline.transitions = transitions.clone();
        }
        Ok(())
    }
}

/// Ajusta ou re-estende entrada, saída e duração de um clipe preservando referência original.
#[derive(Debug)]
pub struct TrimClipCommand {
    clip_id: ClipId,
    new_timeline_start_ns: u64,
    new_source_in_ns: u64,
    new_duration_ns: u64,
    max_source_duration_ns: Option<u64>,
    previous: Option<(u64, u64, u64)>,
    previous_transitions: Option<Vec<TimelineTransition>>,
}

impl TrimClipCommand {
    pub fn new(
        clip_id: ClipId,
        new_timeline_start_ns: u64,
        new_source_in_ns: u64,
        new_duration_ns: u64,
        max_source_duration_ns: Option<u64>,
    ) -> Self {
        Self {
            clip_id,
            new_timeline_start_ns,
            new_source_in_ns,
            new_duration_ns,
            max_source_duration_ns,
            previous: None,
            previous_transitions: None,
        }
    }
}

impl Command for TrimClipCommand {
    fn description(&self) -> &str { "Aparar clipe" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let existing = project.timeline.clip(self.clip_id).ok_or(TimelineError::ClipNotFound(self.clip_id))?.1;
        let prev = (existing.timeline_start_ns, existing.source_in_ns, existing.duration_ns);
        let effective_max = self
            .max_source_duration_ns
            .or_else(|| (existing.media_duration_ns > 0).then_some(existing.media_duration_ns))
            .or_else(|| {
                (existing.title.is_none())
                    .then(|| project.asset(existing.asset_id))
                    .flatten()
                    .filter(|a| a.kind != MediaKind::Image && a.metadata.duration_ns > 0)
                    .map(|a| a.metadata.duration_ns)
            });
        let prev_transitions = project.timeline.transitions.clone();
        project.timeline.trim_clip(
            self.clip_id,
            self.new_timeline_start_ns,
            self.new_source_in_ns,
            self.new_duration_ns,
            effective_max,
        )?;
        self.previous = Some(prev);
        self.previous_transitions = Some(prev_transitions);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev_start, prev_in, prev_dur) =
            self.previous.ok_or(HistoryError::InvalidCommandState("estado anterior de corte ausente"))?;
        project
            .timeline
            .trim_clip(self.clip_id, prev_start, prev_in, prev_dur, self.max_source_duration_ns)?;
        if let Some(ref transitions) = self.previous_transitions {
            project.timeline.transitions = transitions.clone();
        }
        Ok(())
    }
}

/// Divide um clipe ao meio preservando transições e referência não destrutiva para Undo e Redo.
#[derive(Debug)]
pub struct SplitClipCommand {
    clip_id: ClipId,
    split_at_ns: u64,
    right_id: ClipId,
    original_clip: Option<Clip>,
    previous_transitions: Option<Vec<TimelineTransition>>,
}

impl SplitClipCommand {
    pub fn new(clip_id: ClipId, split_at_ns: u64) -> Self {
        Self {
            clip_id,
            split_at_ns,
            right_id: ClipId::new(),
            original_clip: None,
            previous_transitions: None,
        }
    }

    pub fn right_clip_id(&self) -> ClipId { self.right_id }
}

impl Command for SplitClipCommand {
    fn description(&self) -> &str { "Dividir clipe" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let original = project
            .timeline
            .clip(self.clip_id)
            .map(|(_, clip)| clip.clone())
            .ok_or(TimelineError::ClipNotFound(self.clip_id))?;
        let prev_transitions = project.timeline.transitions.clone();
        let _ = project.timeline.split_clip_with_right_id(self.clip_id, self.split_at_ns, self.right_id)?;
        self.original_clip = Some(original);
        self.previous_transitions = Some(prev_transitions);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let original =
            self.original_clip.as_ref().ok_or(HistoryError::InvalidCommandState("clipe original ausente"))?.clone();
        project.timeline.unsplit_clip(self.clip_id, self.right_id, original)?;
        if let Some(ref transitions) = self.previous_transitions {
            project.timeline.transitions = transitions.clone();
        }
        Ok(())
    }
}

/// Atualiza escala, posição, opacidade e velocidade de um clipe com suporte a Undo/Redo.
#[derive(Debug)]
pub struct UpdateClipTransformCommand {
    clip_id: ClipId,
    new_transform: ClipTransform,
    previous_transform: Option<ClipTransform>,
}

impl UpdateClipTransformCommand {
    pub fn new(clip_id: ClipId, new_transform: ClipTransform) -> Self {
        Self { clip_id, new_transform, previous_transform: None }
    }
}

impl Command for UpdateClipTransformCommand {
    fn description(&self) -> &str { "Atualizar transformação do clipe" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.update_clip_transform(self.clip_id, self.new_transform)?;
        self.previous_transform = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev =
            self.previous_transform.ok_or(HistoryError::InvalidCommandState("transformação anterior ausente"))?;
        let _ = project.timeline.update_clip_transform(self.clip_id, prev)?;
        Ok(())
    }
}

/// Remove um clipe da timeline preservando trilha, dados e transições associadas para Undo/Redo.
#[derive(Debug)]
pub struct RemoveClipCommand {
    clip_id: ClipId,
    removed: Option<(TrackId, Clip)>,
    previous_transitions: Option<Vec<TimelineTransition>>,
}

impl RemoveClipCommand {
    pub fn new(clip_id: ClipId) -> Self {
        Self { clip_id, removed: None, previous_transitions: None }
    }
}

impl Command for RemoveClipCommand {
    fn description(&self) -> &str { "Remover clipe da timeline" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev_transitions = project.timeline.transitions.clone();
        let removed = project.timeline.remove_clip(self.clip_id)?;
        self.removed = Some(removed);
        self.previous_transitions = Some(prev_transitions);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (track_id, clip) =
            self.removed.as_ref().ok_or(HistoryError::InvalidCommandState("clipe removido ausente"))?.clone();
        let _ = project.timeline.add_clip(track_id, clip)?;
        if let Some(ref transitions) = self.previous_transitions {
            project.timeline.transitions = transitions.clone();
        }
        Ok(())
    }
}
