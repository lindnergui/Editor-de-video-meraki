use crate::app::{MerakiApp, Message};
use crate::app_delivery::DeliveryState;
use crate::app_exit_guard::PendingExitAction;
use crate::app_preferences::{load_preferences_from, save_preferences_to};
use crate::app_project_io::{short_file_label, ProjectIoMessage, ProjectIoState};
use crate::components::delete_project_modal;
use crate::components::settings_menu::{self, SettingsMenuContext, SettingsMenuMessage};
use crate::navigation::WorkspaceTab;
use crate::playback::PlaybackState;
use crate::project_storage::{
    create_managed_project_in, internal_projects_dir, scan_internal_projects_from,
    sync_project_file_name_on_disk,
};
use crate::views::audio::AudioState;
use crate::views::edit::EditState;
use crate::views::effects::EffectsState;
use crate::views::files::FilesState;
use crate::views::keymap_welcome::{self, KeymapWelcomeMessage};
use crate::views::project_hub::{self, ProjectHubMessage, ProjectHubState};
use iced::{Element, Task};
use meraki_core::{HistoryStack, KeymapProfile, Project};
use meraki_engine::{EngineCommand, EngineHandle};
use std::path::{Path, PathBuf};

/// Destino ao concluir ou voltar da tela de boas-vindas de atalhos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeReturnTarget {
    ProjectHub,
    Editor,
}

/// Telas principais de alto nível da aplicação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppScreen {
    #[default]
    ProjectHub,
    KeymapWelcome {
        return_to: WelcomeReturnTarget,
        can_go_back: bool,
    },
    Editor,
}

/// Mensagens de transição entre o Painel de Projetos, Atalhos e Editor.
#[derive(Debug, Clone)]
pub enum ScreenMessage {
    OpenProjectHub,
    OpenKeymapWelcome(WelcomeReturnTarget),
    SettingsMenu(SettingsMenuMessage),
    ProjectHub(ProjectHubMessage),
    KeymapWelcome(KeymapWelcomeMessage),
}

/// Estado local das telas iniciais v0.2.0.
#[derive(Debug, Clone)]
pub(crate) struct ScreenState {
    pub screen: AppScreen,
    pub hub: ProjectHubState,
    pub welcome_selected: KeymapProfile,
    pub settings_menu_open: bool,
}

impl ScreenState {
    pub fn new(initial_profile: KeymapProfile, onboarding_completed: bool) -> Self {
        let screen = if onboarding_completed {
            AppScreen::ProjectHub
        } else {
            AppScreen::KeymapWelcome {
                return_to: WelcomeReturnTarget::ProjectHub,
                can_go_back: false,
            }
        };
        Self {
            screen,
            hub: ProjectHubState::default(),
            welcome_selected: initial_profile,
            settings_menu_open: false,
        }
    }
}

impl MerakiApp {
    /// Constrói a aplicação com caminhos explícitos de preferências e pasta interna de projetos.
    pub fn with_storage_paths(
        engine: EngineHandle,
        project: Project,
        prefs_path: PathBuf,
        projects_dir: PathBuf,
    ) -> Self {
        let playback = PlaybackState::with_preview_quality(project.preview_quality);
        let mut preferences = load_preferences_from(&prefs_path);
        if scan_internal_projects_from(&projects_dir, &mut preferences) > 0 {
            let _ = save_preferences_to(&prefs_path, &preferences);
        }
        let initial_keymap = preferences.keymap;
        let onboarding_completed = preferences.onboarding_completed;
        Self {
            engine,
            project,
            history: HistoryStack::new(),
            active_tab: WorkspaceTab::Files,
            files: FilesState::default(),
            edit: EditState::default(),
            effects: EffectsState::default(),
            audio: AudioState::default(),
            delivery: DeliveryState::default(),
            project_io: ProjectIoState::default(),
            preferences,
            preferences_path: prefs_path,
            projects_dir,
            pending_exit: None,
            screens: ScreenState::new(initial_keymap, onboarding_completed),
            playback,
            engine_ready: false,
            last_error: None,
        }
    }

    /// Constrói a aplicação carregando preferências de `prefs_path` e projetos em `<pai>/projects`.
    pub fn with_preferences_path(
        engine: EngineHandle,
        project: Project,
        prefs_path: PathBuf,
    ) -> Self {
        let projects_dir = prefs_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join("projects"))
            .unwrap_or_else(internal_projects_dir);
        Self::with_storage_paths(engine, project, prefs_path, projects_dir)
    }

    pub fn set_projects_dir(&mut self, dir: PathBuf) {
        self.projects_dir = dir;
    }

    pub fn projects_dir(&self) -> &Path {
        &self.projects_dir
    }

    pub fn screen(&self) -> AppScreen {
        self.screens.screen
    }

    pub fn hub_state(&self) -> &ProjectHubState {
        &self.screens.hub
    }

    pub fn welcome_selected(&self) -> KeymapProfile {
        self.screens.welcome_selected
    }

    pub fn is_settings_menu_open(&self) -> bool {
        self.screens.settings_menu_open
    }

    /// Cria um novo projeto gerenciado na pasta interna e entra no editor pronto para edição.
    pub fn reset_to_new_project(&mut self) {
        if self.playback.is_playing {
            let _ = self.engine.send(EngineCommand::Pause);
        }

        let default_name = crate::i18n::strings(self.preferences.language).status_untitled_project;
        let (project, managed_path, create_err) =
            match create_managed_project_in(&self.projects_dir, default_name) {
                Ok((proj, path)) => (proj, Some(path), None),
                Err(err) => {
                    let mut fallback = Project::new(default_name);
                    fallback.timeline.ensure_mixer_audio_tracks();
                    (fallback, None, Some(err))
                }
            };

        self.project = project;
        self.history.clear();
        self.active_tab = WorkspaceTab::Files;
        self.files = FilesState::default();
        self.edit = EditState::default();
        self.effects = EffectsState::default();
        self.audio = AudioState::default();
        self.delivery = DeliveryState::default();
        self.project_io = ProjectIoState {
            project_file_path: managed_path.clone(),
            last_saved_label: managed_path
                .as_deref()
                .map(|p| format!("Salvo: {}", short_file_label(p))),
            ..ProjectIoState::default()
        };
        self.playback = PlaybackState::with_preview_quality(self.project.preview_quality);
        self.pending_exit = None;
        self.last_error = create_err;
        self.screens.settings_menu_open = false;
        self.screens.screen = AppScreen::Editor;

        if let Some(path) = managed_path {
            self.record_recent_project(self.project.name.clone(), path);
        }
        let _ = self.sync_timeline_to_engine();
        self.apply_proxy_preference();
    }

    pub(crate) fn handle_screen_message(&mut self, msg: ScreenMessage) -> Task<Message> {
        match msg {
            ScreenMessage::OpenProjectHub => self.request_exit_action(PendingExitAction::ReturnToProjectHub),
            ScreenMessage::OpenKeymapWelcome(return_to) => {
                self.screens.settings_menu_open = false;
                self.screens.welcome_selected = self.preferences.keymap;
                self.screens.screen = AppScreen::KeymapWelcome { return_to, can_go_back: true };
                Task::none()
            }
            ScreenMessage::SettingsMenu(menu_msg) => self.handle_settings_menu_message(menu_msg),
            ScreenMessage::ProjectHub(hub_msg) => self.handle_project_hub_message(hub_msg),
            ScreenMessage::KeymapWelcome(kw_msg) => { self.handle_keymap_welcome_message(kw_msg); Task::none() }
        }
    }

    fn handle_settings_menu_message(&mut self, msg: SettingsMenuMessage) -> Task<Message> {
        match msg {
            SettingsMenuMessage::Toggle => self.screens.settings_menu_open = !self.screens.settings_menu_open,
            SettingsMenuMessage::Close => self.screens.settings_menu_open = false,
            SettingsMenuMessage::SaveNow => {
                self.screens.settings_menu_open = false;
                return self.handle_project_io_message(ProjectIoMessage::Save);
            }
            SettingsMenuMessage::OpenExternal => {
                self.screens.settings_menu_open = false;
                return self.handle_project_io_message(ProjectIoMessage::Open);
            }
            SettingsMenuMessage::ExportCopy => {
                self.screens.settings_menu_open = false;
                return self.handle_project_io_message(ProjectIoMessage::SaveAs);
            }
            SettingsMenuMessage::OpenShortcuts => {
                self.screens.settings_menu_open = false;
                let return_to = match self.screens.screen {
                    AppScreen::Editor => WelcomeReturnTarget::Editor,
                    AppScreen::ProjectHub | AppScreen::KeymapWelcome { .. } => WelcomeReturnTarget::ProjectHub,
                };
                self.screens.welcome_selected = self.preferences.keymap;
                self.screens.screen = AppScreen::KeymapWelcome { return_to, can_go_back: true };
            }
        }
        Task::none()
    }

    fn handle_project_hub_message(&mut self, msg: ProjectHubMessage) -> Task<Message> {
        match msg {
            ProjectHubMessage::SearchChanged(q) => self.screens.hub.search_query = q,
            ProjectHubMessage::NewProjectClicked => self.reset_to_new_project(),
            ProjectHubMessage::ToggleSettings => self.screens.settings_menu_open = !self.screens.settings_menu_open,
            ProjectHubMessage::OpenFileDialog => {
                self.screens.settings_menu_open = false;
                return self.handle_project_io_message(ProjectIoMessage::Open);
            }
            ProjectHubMessage::OpenShortcuts => {
                self.screens.settings_menu_open = false;
                self.screens.welcome_selected = self.preferences.keymap;
                self.screens.screen = AppScreen::KeymapWelcome { return_to: WelcomeReturnTarget::ProjectHub, can_go_back: true };
            }
            ProjectHubMessage::OpenRecent(path) => {
                if let Err(err) = self.open_project_from_path(&path) { self.last_error = Some(err); }
            }
            ProjectHubMessage::StartRename { path, current_name } => {
                self.screens.hub.renaming_path = Some(path);
                self.screens.hub.rename_draft = current_name;
            }
            ProjectHubMessage::RenameDraftChanged(draft) => self.screens.hub.rename_draft = draft,
            ProjectHubMessage::CommitRename => self.commit_hub_rename(),
            ProjectHubMessage::CancelRename => {
                self.screens.hub.renaming_path = None;
                self.screens.hub.rename_draft.clear();
            }
            ProjectHubMessage::RemoveRecent(path) => {
                if self.preferences.remove_recent(&path) {
                    if let Err(err) = save_preferences_to(&self.preferences_path, &self.preferences) {
                        self.last_error = Some(err);
                    }
                }
                if self.screens.hub.renaming_path.as_ref() == Some(&path) {
                    self.screens.hub.renaming_path = None;
                    self.screens.hub.rename_draft.clear();
                }
            }
            ProjectHubMessage::RequestDeleteProject(path) => self.request_delete_project(path),
            ProjectHubMessage::ConfirmDeleteProject => self.confirm_delete_project(),
            ProjectHubMessage::CancelDeleteProject => self.cancel_delete_project(),
        }
        Task::none()
    }

    fn commit_hub_rename(&mut self) {
        let Some(path) = self.screens.hub.renaming_path.take() else {
            return;
        };
        let trimmed = self.screens.hub.rename_draft.trim().to_string();
        self.screens.hub.rename_draft.clear();
        if trimmed.is_empty() {
            return;
        }
        if self.preferences.rename_recent(&path, trimmed.clone()) {
            if let Err(err) = save_preferences_to(&self.preferences_path, &self.preferences) {
                self.last_error = Some(err);
            }
        }
        if let Err(err) = sync_project_file_name_on_disk(&path, &trimmed) {
            self.last_error = Some(err);
        }
        if self.project_io.project_file_path.as_ref() == Some(&path) {
            let _ = self.project.rename(trimmed);
        }
    }

    fn handle_keymap_welcome_message(&mut self, msg: KeymapWelcomeMessage) {
        let (return_to, can_go_back) = match self.screens.screen {
            AppScreen::KeymapWelcome {
                return_to,
                can_go_back,
            } => (return_to, can_go_back),
            AppScreen::ProjectHub => (WelcomeReturnTarget::ProjectHub, true),
            AppScreen::Editor => (WelcomeReturnTarget::Editor, true),
        };
        match msg {
            KeymapWelcomeMessage::SelectProfile(profile) => {
                self.screens.welcome_selected = profile;
            }
            KeymapWelcomeMessage::Back => {
                if can_go_back {
                    self.screens.screen = match return_to {
                        WelcomeReturnTarget::ProjectHub => AppScreen::ProjectHub,
                        WelcomeReturnTarget::Editor => AppScreen::Editor,
                    };
                }
            }
            KeymapWelcomeMessage::Continue => {
                let chosen = self.screens.welcome_selected;
                if let Err(err) = self.complete_onboarding(chosen) {
                    self.last_error = Some(err);
                }
                self.screens.screen = match return_to {
                    WelcomeReturnTarget::ProjectHub => AppScreen::ProjectHub,
                    WelcomeReturnTarget::Editor => AppScreen::Editor,
                };
            }
        }
    }

    pub(crate) fn render_current_screen(&self) -> Element<'_, Message> {
        let lang = self.preferences.language;
        match self.screens.screen {
            AppScreen::ProjectHub => {
                let hub = project_hub::view(
                    &self.screens.hub,
                    &self.preferences.recent_projects,
                    self.screens.settings_menu_open,
                    self.last_error.as_deref(),
                    lang,
                )
                .map(|m| Message::Screen(ScreenMessage::ProjectHub(m)));
                let with_delete = delete_project_modal::overlay(
                    hub,
                    self.screens.hub.pending_delete_project.as_ref(),
                    lang,
                );
                settings_menu::overlay(
                    with_delete,
                    self.screens.settings_menu_open,
                    SettingsMenuContext {
                        in_editor: false,
                        is_dirty: false,
                        profile: self.preferences.keymap,
                        language: lang,
                        proxy_mode: self.preferences.proxy_mode,
                        status_notice: self.project_io.status_notice.as_deref(),
                    },
                )
            }
            AppScreen::KeymapWelcome { can_go_back, .. } => {
                keymap_welcome::view(self.screens.welcome_selected, can_go_back, lang)
                    .map(|m| Message::Screen(ScreenMessage::KeymapWelcome(m)))
            }
            AppScreen::Editor => self.view_editor(),
        }
    }
}
