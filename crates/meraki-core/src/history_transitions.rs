use crate::history::{Command, HistoryError};
use crate::project::Project;
use crate::transitions::TimelineTransition;
use uuid::Uuid;

/// Insere ou substitui uma transição posicional na timeline com Undo/Redo completo.
#[derive(Debug)]
pub struct UpsertTransitionCommand {
    transition: TimelineTransition,
    applied: Option<TimelineTransition>,
    previous: Option<Option<TimelineTransition>>,
}

impl UpsertTransitionCommand {
    pub fn new(transition: TimelineTransition) -> Self {
        Self { transition, applied: None, previous: None }
    }

    pub fn transition_id(&self) -> Uuid {
        self.transition.id
    }
}

impl Command for UpsertTransitionCommand {
    fn description(&self) -> &str {
        "Configurar transição posicional"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.upsert_transition(self.transition)?;
        self.applied = project.timeline.transition(self.transition.id).copied();
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("estado anterior da transição ausente"))?;
        match prev {
            Some(old) => {
                let _ = project.timeline.upsert_transition(old)?;
            }
            None => {
                let _ = project.timeline.remove_transition(self.transition.id)?;
            }
        }
        Ok(())
    }
}

/// Atualiza a duração de uma transição posicional existente com suporte a Undo/Redo.
#[derive(Debug)]
pub struct UpdateTransitionDurationCommand {
    transition_id: Uuid,
    new_duration_ns: u64,
    previous_duration_ns: Option<u64>,
}

impl UpdateTransitionDurationCommand {
    pub fn new(transition_id: Uuid, new_duration_ns: u64) -> Self {
        Self { transition_id, new_duration_ns, previous_duration_ns: None }
    }
}

impl Command for UpdateTransitionDurationCommand {
    fn description(&self) -> &str {
        "Ajustar duração da transição"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project
            .timeline
            .update_transition_duration(self.transition_id, self.new_duration_ns)?;
        self.previous_duration_ns = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_duration_ns
            .ok_or(HistoryError::InvalidCommandState("duração anterior da transição ausente"))?;
        let _ = project
            .timeline
            .update_transition_duration(self.transition_id, prev)?;
        Ok(())
    }
}

/// Remove uma transição posicional da timeline restaurando-a exatamente ao desfazer.
#[derive(Debug)]
pub struct RemoveTransitionCommand {
    transition_id: Uuid,
    removed: Option<TimelineTransition>,
}

impl RemoveTransitionCommand {
    pub fn new(transition_id: Uuid) -> Self {
        Self { transition_id, removed: None }
    }
}

impl Command for RemoveTransitionCommand {
    fn description(&self) -> &str {
        "Remover transição posicional"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = project.timeline.remove_transition(self.transition_id)?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = self
            .removed
            .ok_or(HistoryError::InvalidCommandState("transição removida ausente"))?;
        let _ = project.timeline.upsert_transition(removed)?;
        Ok(())
    }
}
