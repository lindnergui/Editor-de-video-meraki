pub use crate::history_audio::{
    ApplyVoicePresetCommand, RemoveVolumeKeyframeCommand, ToggleAudioEffectCommand,
    UpdateTrackAudioCommand, UpsertVolumeKeyframeCommand,
};
pub use crate::history_effect_params::{
    AddClipEffectCommand, RemoveEffectCommand, SetClipTransformCommand, ToggleEffectCommand,
    UpdateEffectParamsCommand,
};
pub use crate::history_effects::{
    AddTitleClipCommand, RemoveKeyframeCommand, SetClipTransitionCommand, ToggleClipEffectCommand,
    UpdateTitleCommand, UpsertKeyframeCommand,
};
use crate::project::{MediaAsset, MediaFolder, Project, ProjectError};
use crate::timeline::{Clip, ClipId, ClipTransform, TimelineError, TrackId};
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
        let _ = project.timeline.add_clip(self.track_id, self.clip.clone())?;
        Ok(())
    }
    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let _ = project.timeline.remove_clip(self.clip.id)?;
        Ok(())
    }
}

/// Move um clipe na timeline preservando posição e trilha de origem.
#[derive(Debug)]
pub struct MoveClipCommand {
    clip_id: ClipId,
    target_track_id: TrackId,
    new_start_ns: u64,
    previous: Option<(TrackId, u64)>,
}

impl MoveClipCommand {
    pub fn new(clip_id: ClipId, target_track_id: TrackId, new_start_ns: u64) -> Self {
        Self { clip_id, target_track_id, new_start_ns, previous: None }
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
        project.timeline.move_clip(self.clip_id, self.target_track_id, self.new_start_ns)?;
        self.previous = Some((prev_track, prev_start));
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev_track, prev_start) = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("posição anterior ausente"))?;
        project.timeline.move_clip(self.clip_id, prev_track, prev_start)?;
        Ok(())
    }
}

/// Ajusta entrada, saída e duração de um clipe na timeline.
#[derive(Debug)]
pub struct TrimClipCommand {
    clip_id: ClipId,
    new_timeline_start_ns: u64,
    new_source_in_ns: u64,
    new_duration_ns: u64,
    max_source_duration_ns: Option<u64>,
    previous: Option<(u64, u64, u64)>,
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
        }
    }
}

impl Command for TrimClipCommand {
    fn description(&self) -> &str { "Aparar clipe" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (_, existing) = project.timeline.clip(self.clip_id).ok_or(TimelineError::ClipNotFound(self.clip_id))?;
        let prev = (existing.timeline_start_ns, existing.source_in_ns, existing.duration_ns);
        project.timeline.trim_clip(
            self.clip_id,
            self.new_timeline_start_ns,
            self.new_source_in_ns,
            self.new_duration_ns,
            self.max_source_duration_ns,
        )?;
        self.previous = Some(prev);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (prev_start, prev_in, prev_dur) = self
            .previous
            .ok_or(HistoryError::InvalidCommandState("estado anterior de corte ausente"))?;
        project
            .timeline
            .trim_clip(self.clip_id, prev_start, prev_in, prev_dur, self.max_source_duration_ns)?;
        Ok(())
    }
}

/// Divide um clipe ao meio com suporte determinístico a Undo e Redo.
#[derive(Debug)]
pub struct SplitClipCommand {
    clip_id: ClipId,
    split_at_ns: u64,
    right_id: ClipId,
    original_clip: Option<Clip>,
}

impl SplitClipCommand {
    pub fn new(clip_id: ClipId, split_at_ns: u64) -> Self {
        Self { clip_id, split_at_ns, right_id: ClipId::new(), original_clip: None }
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
        let _ = project.timeline.split_clip_with_right_id(self.clip_id, self.split_at_ns, self.right_id)?;
        self.original_clip = Some(original);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let original =
            self.original_clip.as_ref().ok_or(HistoryError::InvalidCommandState("clipe original ausente"))?.clone();
        project.timeline.unsplit_clip(self.clip_id, self.right_id, original)?;
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

/// Remove um clipe da timeline preservando trilha e dados originais para Undo/Redo.
#[derive(Debug)]
pub struct RemoveClipCommand {
    clip_id: ClipId,
    removed: Option<(TrackId, Clip)>,
}

impl RemoveClipCommand {
    pub fn new(clip_id: ClipId) -> Self {
        Self { clip_id, removed: None }
    }
}

impl Command for RemoveClipCommand {
    fn description(&self) -> &str { "Remover clipe da timeline" }

    fn execute(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let removed = project.timeline.remove_clip(self.clip_id)?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let (track_id, clip) =
            self.removed.as_ref().ok_or(HistoryError::InvalidCommandState("clipe removido ausente"))?.clone();
        let _ = project.timeline.add_clip(track_id, clip)?;
        Ok(())
    }
}
