use crate::app::{MerakiApp, Message};
use crate::app_exit_guard::PendingExitAction;
use crate::project_storage::managed_project_path_in;
use iced::keyboard::key::Key;
use iced::keyboard::Modifiers;
use iced::widget::image;
use iced::Task;
use meraki_core::{AssetId, MediaKind, PreviewQuality, Project, RenameProjectCommand};
use meraki_engine::{EngineCommand, EngineEvent};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Estado de persistência (`.meraki`), renomeação e miniaturas/proxies em memória.
#[derive(Debug, Clone, Default)]
pub struct ProjectIoState {
    pub project_file_path: Option<PathBuf>,
    pub is_dirty: bool,
    pub last_saved_label: Option<String>,
    pub autosave_dirty: bool,
    pub editing_name: bool,
    pub name_draft: String,
    pub thumbnails: HashMap<AssetId, image::Handle>,
    pub proxy_progress: HashMap<AssetId, f32>,
}

/// Mensagens relacionadas à persistência `.meraki`, renomeação inline e auto-save.
#[derive(Debug, Clone)]
pub enum ProjectIoMessage {
    Save,
    SaveAs,
    Open,
    SavePathChosen(Option<PathBuf>),
    OpenPathChosen(Option<PathBuf>),
    StartRename,
    NameDraftChanged(String),
    CommitRename,
    CancelRename,
    AutoSaveTick,
}

/// Retorna o caminho padrão de backup automático na pasta temporária do sistema.
pub fn autosave_path() -> PathBuf {
    std::env::temp_dir().join("meraki-autosave.meraki")
}

/// Garante que o caminho informado possua a extensão `.meraki`.
pub fn ensure_meraki_extension(path: PathBuf) -> PathBuf {
    let has_meraki = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("meraki"));
    if has_meraki {
        path
    } else {
        let mut os = path.into_os_string();
        os.push(".meraki");
        PathBuf::from(os)
    }
}

/// Traduz atalhos `Ctrl+S`, `Ctrl+Shift+S` e `Ctrl+O` em ações de persistência.
pub fn map_project_shortcut(key: Key, modifiers: Modifiers) -> Option<ProjectIoMessage> {
    if !(modifiers.control() || modifiers.command()) || modifiers.alt() {
        return None;
    }
    if let Key::Character(ch) = key.as_ref() {
        if ch.eq_ignore_ascii_case("s") {
            return Some(if modifiers.shift() {
                ProjectIoMessage::SaveAs
            } else {
                ProjectIoMessage::Save
            });
        }
        if ch.eq_ignore_ascii_case("o") && !modifiers.shift() {
            return Some(ProjectIoMessage::Open);
        }
    }
    None
}

async fn pick_save_project_file(suggested_name: String) -> Option<PathBuf> {
    let safe_stem = if suggested_name.trim().is_empty() {
        "projeto".to_string()
    } else {
        suggested_name.trim().to_string()
    };
    let file = rfd::AsyncFileDialog::new()
        .set_title("Exportar Cópia do Projeto (.meraki)")
        .set_file_name(format!("{safe_stem}.meraki"))
        .add_filter("Projeto Meraki", &["meraki"])
        .save_file()
        .await?;
    Some(ensure_meraki_extension(file.path().to_path_buf()))
}

pub(crate) async fn pick_open_project_file() -> Option<PathBuf> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Abrir Projeto Meraki")
        .add_filter("Projeto Meraki", &["meraki"])
        .pick_file()
        .await?;
    Some(file.path().to_path_buf())
}

pub(crate) fn short_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn format_missing_media_warning(missing: &[PathBuf]) -> String {
    let preview: Vec<String> = missing.iter().take(2).map(|p| short_file_label(p)).collect();
    let suffix = if missing.len() > 2 {
        format!(" e mais {}", missing.len() - 2)
    } else {
        String::new()
    };
    format!(
        "Mídia ausente no disco ({} arquivo(s)): {}{suffix}",
        missing.len(),
        preview.join(", ")
    )
}

impl MerakiApp {
    /// Retorna o estado atual de persistência, renomeação e cache visual do projeto.
    pub fn project_io(&self) -> &ProjectIoState {
        &self.project_io
    }

    /// Marca o projeto com alterações pendentes tanto para salvamento manual quanto auto-save.
    pub fn mark_project_dirty(&mut self) {
        self.project_io.is_dirty = true;
        self.project_io.autosave_dirty = true;
    }

    /// Renomeia o projeto atual usando `RenameProjectCommand` no histórico de Undo/Redo.
    pub fn rename_project(&mut self, new_name: String) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            self.project_io.editing_name = false;
            return Ok(());
        }
        if trimmed == self.project.name {
            self.project_io.editing_name = false;
            return Ok(());
        }

        let cmd = RenameProjectCommand::new(trimmed.to_string());
        self.history
            .execute(Box::new(cmd), &mut self.project)
            .map_err(|err| err.to_string())?;
        self.project_io.editing_name = false;
        self.mark_project_dirty();
        Ok(())
    }

    /// Serializa e grava o projeto em `raw_path`, garantindo extensão `.meraki`.
    pub fn save_project_to_path(&mut self, raw_path: PathBuf) -> Result<PathBuf, String> {
        let path = ensure_meraki_extension(raw_path);
        let json = self.project.to_json().map_err(|err| err.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|err| format!("Falha ao preparar pasta: {err}"))?;
        }
        fs::write(&path, json).map_err(|err| format!("Falha ao salvar projeto: {err}"))?;

        let label = format!("Salvo: {}", short_file_label(&path));
        self.project_io.project_file_path = Some(path.clone());
        self.project_io.is_dirty = false;
        self.project_io.autosave_dirty = false;
        self.project_io.last_saved_label = Some(label);
        self.last_error = None;
        self.record_recent_project(self.project.name.clone(), path.clone());
        info!(path = %path.display(), "projeto salvo com sucesso");
        Ok(path)
    }

    /// Abre um arquivo `.meraki`, valida mídias em disco e re-sincroniza motor e miniaturas.
    pub fn open_project_from_path(&mut self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let raw = fs::read_to_string(path).map_err(|err| format!("Falha ao ler arquivo: {err}"))?;
        let mut loaded = Project::from_json(&raw).map_err(|err| format!("Projeto inválido: {err}"))?;
        loaded.timeline.ensure_mixer_audio_tracks();

        let mut missing_paths = Vec::new();
        let mut import_requests = Vec::new();

        for asset in &mut loaded.assets {
            if let Some(proxy) = &asset.proxy_path {
                if !proxy.is_file() {
                    asset.proxy_path = None;
                }
            }
            if asset.path.is_file() {
                let generate_proxy = matches!(asset.kind, MediaKind::Video) && asset.proxy_path.is_none();
                import_requests.push((asset.id, asset.path.clone(), generate_proxy));
            } else {
                missing_paths.push(asset.path.clone());
            }
        }

        let preview_quality = loaded.preview_quality;
        self.project = loaded;
        self.history.clear();
        self.clear_inspector_drafts();
        self.playback.selected_clip_id = None;
        self.playback.is_playing = false;
        self.playback.playhead_ns = self.project.timeline.playhead_ns;
        self.playback.preview_quality = preview_quality;
        self.playback.current_frame = None;
        self.files.selected_asset = self.project.assets.first().map(|a| a.id);
        self.files.selected_folder = None;
        self.project_io.thumbnails.clear();
        self.project_io.proxy_progress.clear();
        self.project_io.editing_name = false;
        self.project_io.name_draft.clear();
        self.project_io.project_file_path = Some(path.to_path_buf());
        self.project_io.is_dirty = false;
        self.project_io.autosave_dirty = false;
        self.project_io.last_saved_label = Some(format!("Aberto: {}", short_file_label(path)));

        if let Err(err) = self.sync_timeline_to_engine() {
            self.last_error = Some(err.to_string());
        }
        for (asset_id, asset_path, generate_proxy) in import_requests {
            if let Err(err) = self.engine.send(EngineCommand::ImportMedia {
                asset_id,
                path: asset_path,
                generate_proxy,
            }) {
                self.last_error = Some(err.to_string());
                break;
            }
        }

        if missing_paths.is_empty() {
            self.last_error = None;
        } else {
            let warning = format_missing_media_warning(&missing_paths);
            warn!("{warning}");
            self.last_error = Some(warning);
        }

        self.record_recent_project(self.project.name.clone(), path.to_path_buf());
        self.screens.screen = crate::app_screens::AppScreen::Editor;
        info!(path = %path.display(), missing = missing_paths.len(), "projeto carregado");
        Ok(missing_paths)
    }

    /// Grava cópia de recuperação automática quando houver alterações não salvas no ciclo.
    pub fn perform_autosave_if_dirty(&mut self) -> Result<bool, String> {
        if !self.project_io.autosave_dirty {
            return Ok(false);
        }
        let path = autosave_path();
        let json = self.project.to_json().map_err(|err| err.to_string())?;
        fs::write(&path, json).map_err(|err| format!("Falha no auto-save: {err}"))?;
        self.project_io.autosave_dirty = false;
        info!(path = %path.display(), "cópia de auto-save atualizada");
        Ok(true)
    }

    /// Salva o projeto no caminho atual ou, se ausente, cria na pasta interna sem abrir diálogo.
    pub fn save_current_project(&mut self) -> Result<PathBuf, String> {
        let target = self
            .project_io
            .project_file_path
            .clone()
            .unwrap_or_else(|| managed_project_path_in(&self.projects_dir, self.project.id));
        self.save_project_to_path(target)
    }

    pub(crate) fn handle_project_io_message(&mut self, msg: ProjectIoMessage) -> Task<Message> {
        match msg {
            ProjectIoMessage::Save => {
                if let Err(err) = self.save_current_project() {
                    self.last_error = Some(err);
                }
                Task::none()
            }
            ProjectIoMessage::SaveAs => Task::perform(
                pick_save_project_file(self.project.name.clone()),
                |path| Message::ProjectIo(ProjectIoMessage::SavePathChosen(path)),
            ),
            ProjectIoMessage::Open => {
                self.request_exit_action(PendingExitAction::OpenExternalProjectDialog)
            }
            ProjectIoMessage::SavePathChosen(Some(path)) => {
                if let Err(err) = self.save_project_to_path(path) {
                    self.last_error = Some(err);
                }
                Task::none()
            }
            ProjectIoMessage::OpenPathChosen(Some(path)) => {
                if let Err(err) = self.open_project_from_path(&path) {
                    self.last_error = Some(err);
                }
                Task::none()
            }
            ProjectIoMessage::SavePathChosen(None) | ProjectIoMessage::OpenPathChosen(None) => {
                Task::none()
            }
            ProjectIoMessage::StartRename => {
                self.project_io.editing_name = true;
                self.project_io.name_draft = self.project.name.clone();
                Task::none()
            }
            ProjectIoMessage::NameDraftChanged(draft) => {
                self.project_io.name_draft = draft;
                Task::none()
            }
            ProjectIoMessage::CommitRename => {
                let draft = self.project_io.name_draft.clone();
                if let Err(err) = self.rename_project(draft) {
                    self.last_error = Some(err);
                }
                Task::none()
            }
            ProjectIoMessage::CancelRename => {
                self.project_io.editing_name = false;
                self.project_io.name_draft.clear();
                Task::none()
            }
            ProjectIoMessage::AutoSaveTick => {
                if let Err(err) = self.perform_autosave_if_dirty() {
                    error!(error = %err, "falha no ciclo de auto-save");
                    self.last_error = Some(err);
                }
                Task::none()
            }
        }
    }

    pub(crate) fn handle_media_visual_event(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::ThumbnailReady(thumb) => {
                let handle = image::Handle::from_rgba(
                    thumb.width,
                    thumb.height,
                    thumb.rgba_pixels.clone(),
                );
                self.project_io.thumbnails.insert(thumb.asset_id, handle);
            }
            EngineEvent::ProxyProgress {
                asset_id,
                progress,
                proxy_path,
            } => {
                if let Some(path) = proxy_path {
                    if let Some(asset) = self.project.asset_mut(*asset_id) {
                        asset.proxy_path = Some(path.clone());
                    }
                    self.project_io.proxy_progress.remove(asset_id);
                    if self.playback.preview_quality == PreviewQuality::Proxy720p {
                        if let Err(err) = self.sync_timeline_to_engine() {
            self.last_error = Some(err.to_string());
        }
                    }
                } else {
                    self.project_io
                        .proxy_progress
                        .insert(*asset_id, progress.clamp(0.0, 1.0));
                }
            }
            _ => {}
        }
    }
}
