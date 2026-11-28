pub use crate::history_audio::{
    ApplyVoicePresetCommand, RemoveVolumeKeyframeCommand, ToggleAudioEffectCommand,
    UpdateTrackAudioCommand, UpsertVolumeKeyframeCommand,
};
pub use crate::history_audio_eq::{
    ApplyAudioPreset, ApplyAudioPresetCommand, ResetEqBand, ResetEqBandCommand, SetMasterVolume,
    SetMasterVolumeCommand, SetTrackEq, SetTrackEqCommand, SetTrackVolume, SetTrackVolumeCommand,
};
pub use crate::history_effect_params::{
    AddClipEffectCommand, RemoveEffectCommand, SetClipTransformCommand, ToggleEffectCommand,
    UpdateEffectParamsCommand,
};
pub use crate::history_effects::{
    AddTitleClipCommand, RemoveKeyframeCommand, SetClipTransitionCommand, ToggleClipEffectCommand,
    UpdateTitleCommand, UpsertKeyframeCommand,
};
pub use crate::history_timeline::{
    MoveClipCommand, RemoveClipCommand, SplitClipCommand, TrimClipCommand,
    UpdateClipTransformCommand,
};
pub use crate::history_transitions::{
    RemoveTransitionCommand, UpdateTransitionDurationCommand, UpsertTransitionCommand,
};
use crate::project::{MediaAsset, MediaFolder, MediaKind, Project, ProjectError};
use crate::timeline::{Clip, TimelineError, TrackId};
use std::fmt;
use thiserror::Error;

/// Erros produzidos ao executar, desfazer ou refazer comandos no histórico.
#[derive(Debug, Error)]
pub enum HistoryError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Timeline(#[from] TimelineError),
    #[error("não há nenhuma ação para desfazer")]
    NothingToUndo,
    #[error("não há nenhuma ação para refazer")]
    NothingToRedo,
    #[error("estado interno inválido do comando: {0}")]
    InvalidCommandState(&'static str),
}

/// Contrato do padrão Command para mutações reversíveis em `Project` e `Timeline`.
pub trait Command: fmt::Debug {
    fn description(&self) -> &str;
    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError>;
    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError>;
}

/// Pilha de Undo/Redo com profundidade máxima configurável.
#[derive(Debug)]
pub struct HistoryStack {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_depth: usize,
}

impl Default for HistoryStack {
    fn default() -> Self { Self::new() }
}

impl HistoryStack {
    pub fn new() -> Self { Self::with_max_depth(200) }

    pub fn with_max_depth(max_depth: usize) -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new(), max_depth: max_depth.max(1) }
    }

    /// Executa um comando; se falhar, mantém as pilhas intactas.
    pub fn execute(&mut self, mut command: Box<dyn Command>, project: &mut Project) -> Result<(), HistoryError> {
        command.execute(project)?;
        self.redo_stack.clear();
        self.undo_stack.push(command);
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        Ok(())
    }

    /// Desfaz o último comando executado, movendo-o para a pilha de Redo.
    pub fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let mut command = self.undo_stack.pop().ok_or(HistoryError::NothingToUndo)?;
        if let Err(err) = command.undo(project) {
            self.undo_stack.push(command);
            return Err(err);
        }
        self.redo_stack.push(command);
        Ok(())
    }

    /// Refaz o último comando desfeito, movendo-o de volta para a pilha de Undo.
    pub fn redo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let mut command = self.redo_stack.pop().ok_or(HistoryError::NothingToRedo)?;
        if let Err(err) = command.execute(project) {
            self.redo_stack.push(command);
            return Err(err);
        }
        self.undo_stack.push(command);
        Ok(())
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }
    pub fn undo_len(&self) -> usize { self.undo_stack.len() }
    pub fn redo_len(&self) -> usize { self.redo_stack.len() }
    pub fn last_undo_description(&self) -> Option<&str> { self.undo_stack.last().map(|c| c.description()) }
    pub fn last_redo_description(&self) -> Option<&str> { self.redo_stack.last().map(|c| c.description()) }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

/// Renomeia o projeto preservando o nome anterior para Undo.
#[derive(Debug)]
pub struct RenameProjectCommand {
    new_name: String,
    previous_name: Option<String>,
}

impl RenameProjectCommand {
    pub fn new(new_name: impl Into<String>) -> Self {
        Self { new_name: new_name.into(), previous_name: None }
    }
}

impl Command for RenameProjectCommand {
    fn description(&self) -> &str { "Renomear projeto" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = project.name.clone();
        project.rename(self.new_name.clone())?;
        self.previous_name = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let prev = self.previous_name.as_ref().ok_or(HistoryError::InvalidCommandState("nome anterior ausente"))?;
        project.rename(prev.clone())?;
        Ok(())
    }
}

/// Cria uma pasta na biblioteca de mídia do projeto.
#[derive(Debug)]
pub struct AddFolderCommand {
    folder: MediaFolder,
}

impl AddFolderCommand {
    pub fn new(folder: MediaFolder) -> Self { Self { folder } }
}

impl Command for AddFolderCommand {
    fn description(&self) -> &str { "Criar pasta de mídia" }
    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        project.insert_folder(self.folder.clone())?;
        Ok(())
    }
    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.remove_folder(self.folder.id)?;
        Ok(())
    }
}

/// Adiciona um ativo de mídia à biblioteca do projeto.
#[derive(Debug)]
pub struct AddAssetCommand {
    asset: MediaAsset,
}

impl AddAssetCommand {
    pub fn new(asset: MediaAsset) -> Self { Self { asset } }
}

impl Command for AddAssetCommand {
    fn description(&self) -> &str { "Importar mídia" }
    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.add_asset(self.asset.clone())?;
        Ok(())
    }
    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.remove_asset(self.asset.id)?;
        Ok(())
    }
}

/// Insere um clipe em uma trilha da timeline.
#[derive(Debug)]
pub struct AddClipCommand {
    track_id: TrackId,
    clip: Clip,
}

impl AddClipCommand {
    pub fn new(track_id: TrackId, clip: Clip) -> Self { Self { track_id, clip } }
}

impl Command for AddClipCommand {
    fn description(&self) -> &str { "Adicionar clipe na timeline" }
    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let mut clip = self.clip.clone();
        if clip.title.is_none() && clip.media_duration_ns == 0 {
            if let Some(asset) = project.asset(clip.asset_id) {
                let dur = asset.metadata.duration_ns;
                if asset.kind != MediaKind::Image && dur > 0 && dur >= clip.source_in_ns.saturating_add(clip.duration_ns) {
                    clip.media_duration_ns = dur;
                }
            }
        }
        let _ = project.timeline.add_clip(self.track_id, clip)?;
        Ok(())
    }
    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.timeline.remove_clip(self.clip.id)?;
        Ok(())
    }
}
