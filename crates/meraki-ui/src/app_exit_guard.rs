use crate::app::{MerakiApp, Message};
use crate::app_project_io::{pick_open_project_file, short_file_label, ProjectIoMessage, ProjectIoState};
use crate::app_screens::AppScreen;
use crate::components::settings_menu::{self, SettingsMenuContext};
use crate::components::unsaved_modal;
use crate::i18n;
use crate::navigation::WorkspaceTab;
use crate::playback::PlaybackState;
use crate::project_storage::delete_project_permanently_with_prefs;
use crate::views::{audio::AudioState, edit::EditState, effects::EffectsState, files::FilesState};
use iced::{window, Element, Task};
use meraki_core::{Project, RecentProjectEntry};
use meraki_engine::EngineCommand;
use std::path::PathBuf;
use uuid::Uuid;

/// Ação de saída ou troca de contexto aguardando confirmação quando há edições não salvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingExitAction {
    CloseWindow(window::Id),
    ReturnToProjectHub,
    OpenProject(PathBuf),
    OpenExternalProjectDialog,
}

/// Resposta do usuário ao modal de proteção contra perda de alterações.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitGuardMessage {
    SaveAndContinue,
    DiscardAndContinue,
    Cancel,
}

impl MerakiApp {
    /// Retorna a ação pendente de saída aguardando confirmação no modal, se houver.
    pub fn pending_exit(&self) -> Option<&PendingExitAction> {
        self.pending_exit.as_ref()
    }

    pub fn pending_delete_project(&self) -> Option<&RecentProjectEntry> {
        self.screens.hub.pending_delete_project.as_ref()
    }

    pub(crate) fn request_delete_project(&mut self, path: PathBuf) {
        let entry = self
            .preferences
            .recent_projects
            .iter()
            .find(|item| item.path == path)
            .cloned()
            .unwrap_or_else(|| RecentProjectEntry {
                id: Uuid::new_v4(),
                name: short_file_label(&path),
                path,
                last_opened_unix_secs: 0,
            });
        self.screens.hub.pending_delete_project = Some(entry);
    }

    pub(crate) fn cancel_delete_project(&mut self) {
        self.screens.hub.pending_delete_project = None;
    }

    pub(crate) fn confirm_delete_project(&mut self) {
        let Some(entry) = self.screens.hub.pending_delete_project.take() else {
            return;
        };
        if self.screens.hub.renaming_path.as_ref() == Some(&entry.path) {
            self.screens.hub.renaming_path = None;
            self.screens.hub.rename_draft.clear();
        }
        if let Err(err) =
            delete_project_permanently_with_prefs(&entry.path, &self.preferences_path, &mut self.preferences)
        {
            self.last_error = Some(err);
            return;
        }
        let same_open_project = self.project_io.project_file_path.as_ref() == Some(&entry.path)
            || self.project.id == entry.id;
        if same_open_project {
            self.clear_open_project_after_delete();
        }
    }

    fn clear_open_project_after_delete(&mut self) {
        if self.playback.is_playing {
            let _ = self.engine.send(EngineCommand::Pause);
        }
        let mut clean = Project::new(i18n::strings(self.preferences.language).status_untitled_project);
        clean.timeline.ensure_mixer_audio_tracks();
        let quality = clean.preview_quality;
        self.project = clean;
        self.history.clear();
        self.clear_inspector_drafts();
        self.active_tab = WorkspaceTab::Files;
        self.files = FilesState::default();
        self.edit = EditState::default();
        self.effects = EffectsState::default();
        self.audio = AudioState::default();
        self.project_io = ProjectIoState::default();
        self.playback = PlaybackState::with_preview_quality(quality);
        self.pending_exit = None;
        self.dispatch_timeline_sync();
    }

    /// Solicita fechar janela ou sair do projeto atual, protegendo alterações não salvas no `Editor`.
    pub fn request_exit_action(&mut self, action: PendingExitAction) -> Task<Message> {
        self.screens.settings_menu_open = false;
        if self.screens.screen == AppScreen::Editor && self.project_io.is_dirty {
            self.pending_exit = Some(action);
            Task::none()
        } else {
            self.execute_exit_action(action)
        }
    }

    fn execute_exit_action(&mut self, action: PendingExitAction) -> Task<Message> {
        match action {
            PendingExitAction::CloseWindow(id) => {
                let _ = self.engine.send(EngineCommand::Shutdown);
                window::close(id)
            }
            PendingExitAction::ReturnToProjectHub => {
                if self.playback.is_playing {
                    self.playback.is_playing = false;
                    let _ = self.engine.send(EngineCommand::Pause);
                }
                self.screens.screen = AppScreen::ProjectHub;
                Task::none()
            }
            PendingExitAction::OpenProject(path) => {
                if let Err(err) = self.open_project_from_path(&path) {
                    self.last_error = Some(err);
                }
                Task::none()
            }
            PendingExitAction::OpenExternalProjectDialog => {
                Task::perform(pick_open_project_file(), |path| {
                    Message::ProjectIo(ProjectIoMessage::OpenPathChosen(path))
                })
            }
        }
    }

    pub(crate) fn handle_exit_guard_message(&mut self, msg: ExitGuardMessage) -> Task<Message> {
        match msg {
            ExitGuardMessage::Cancel => {
                self.pending_exit = None;
                Task::none()
            }
            ExitGuardMessage::DiscardAndContinue => {
                let pending = self.pending_exit.take();
                self.project_io.is_dirty = false;
                self.project_io.autosave_dirty = false;
                match pending {
                    Some(action) => self.execute_exit_action(action),
                    None => Task::none(),
                }
            }
            ExitGuardMessage::SaveAndContinue => match self.save_current_project() {
                Ok(_) => {
                    let pending = self.pending_exit.take();
                    match pending {
                        Some(action) => self.execute_exit_action(action),
                        None => Task::none(),
                    }
                }
                Err(err) => {
                    self.last_error = Some(err);
                    self.pending_exit = None;
                    Task::none()
                }
            },
        }
    }

    /// Sobrepõe o menu de configurações (se aberto) e o modal de confirmação (se `pending_exit`).
    pub(crate) fn wrap_with_editor_overlays<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let with_settings = settings_menu::overlay(
            base,
            self.screens.settings_menu_open,
            SettingsMenuContext {
                in_editor: true,
                is_dirty: self.project_io.is_dirty,
                profile: self.preferences.keymap,
                language: self.preferences.language,
                proxy_mode: self.preferences.proxy_mode,
                status_notice: self.project_io.status_notice.as_deref(),
            },
        );
        unsaved_modal::overlay(
            with_settings,
            &self.project.name,
            self.pending_exit.as_ref(),
            self.preferences.language,
        )
    }
}
