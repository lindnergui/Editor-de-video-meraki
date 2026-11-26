use crate::effect_params::EffectParameters;
use crate::effects::ClipEffect;
use crate::history::{Command, HistoryError, UpdateClipTransformCommand};
use crate::project::Project;
use crate::timeline::ClipId;
use uuid::Uuid;

/// Alias semântico para atualização reversível de transformação de clipe (`UpdateClipTransformCommand`).
pub type SetClipTransformCommand = UpdateClipTransformCommand;

/// Adiciona um efeito parametrizado a um clipe com remoção reversível no Undo.
#[derive(Debug)]
pub struct AddClipEffectCommand {
    clip_id: ClipId,
    effect: ClipEffect,
}

impl AddClipEffectCommand {
    pub fn new(clip_id: ClipId, effect: ClipEffect) -> Self {
        Self { clip_id, effect }
    }

    pub fn effect_id(&self) -> Uuid {
        self.effect.id
    }
}

impl Command for AddClipEffectCommand {
    fn description(&self) -> &str {
        "Adicionar efeito ao clipe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project
            .timeline
            .add_clip_effect(self.clip_id, self.effect.clone())?;
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project
            .timeline
            .remove_effect_by_id(self.clip_id, self.effect.id)?;
        Ok(())
    }
}

/// Atualiza parâmetros individuais de um efeito específico com suporte a Undo/Redo.
#[derive(Debug)]
pub struct UpdateEffectParamsCommand {
    clip_id: ClipId,
    effect_id: Uuid,
    new_params: EffectParameters,
    previous_effect: Option<ClipEffect>,
}

impl UpdateEffectParamsCommand {
    pub fn new(clip_id: ClipId, effect_id: Uuid, new_params: EffectParameters) -> Self {
        Self {
            clip_id,
            effect_id,
            new_params,
            previous_effect: None,
        }
    }
}

impl Command for UpdateEffectParamsCommand {
    fn description(&self) -> &str {
        "Atualizar parâmetros do efeito"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.timeline.update_effect_params(
            self.clip_id,
            self.effect_id,
            self.new_params,
        )?;
        self.previous_effect = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self
            .previous_effect
            .as_ref()
            .ok_or(HistoryError::InvalidCommandState("parâmetros anteriores ausentes"))?;
        let _ = project
            .timeline
            .update_effect_params(self.clip_id, self.effect_id, prev.params)?;
        Ok(())
    }
}

/// Alterna habilitação (`enabled`) de um efeito identificado por `effect_id` com Undo/Redo.
#[derive(Debug)]
pub struct ToggleEffectCommand {
    clip_id: ClipId,
    effect_id: Uuid,
    previous_enabled: Option<bool>,
}

impl ToggleEffectCommand {
    pub fn new(clip_id: ClipId, effect_id: Uuid) -> Self {
        Self {
            clip_id,
            effect_id,
            previous_enabled: None,
        }
    }
}

impl Command for ToggleEffectCommand {
    fn description(&self) -> &str {
        "Ativar ou desativar efeito"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project
            .timeline
            .toggle_effect_by_id(self.clip_id, self.effect_id)?;
        self.previous_enabled = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let expected_prev = self
            .previous_enabled
            .ok_or(HistoryError::InvalidCommandState("estado anterior do efeito ausente"))?;
        let current = project
            .timeline
            .toggle_effect_by_id(self.clip_id, self.effect_id)?;
        if current == expected_prev {
            let _ = project
                .timeline
                .toggle_effect_by_id(self.clip_id, self.effect_id)?;
        }
        Ok(())
    }
}

/// Remove um efeito específico de um clipe restaurando-o na posição original no Undo.
#[derive(Debug)]
pub struct RemoveEffectCommand {
    clip_id: ClipId,
    effect_id: Uuid,
    removed: Option<(usize, ClipEffect)>,
}

impl RemoveEffectCommand {
    pub fn new(clip_id: ClipId, effect_id: Uuid) -> Self {
        Self {
            clip_id,
            effect_id,
            removed: None,
        }
    }
}

impl Command for RemoveEffectCommand {
    fn description(&self) -> &str {
        "Remover efeito do clipe"
    }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = project
            .timeline
            .remove_effect_by_id(self.clip_id, self.effect_id)?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (idx, effect) = self
            .removed
            .as_ref()
            .ok_or(HistoryError::InvalidCommandState("efeito removido ausente"))?
            .clone();
        project.timeline.insert_effect_at(self.clip_id, idx, effect)?;
        Ok(())
    }
}
