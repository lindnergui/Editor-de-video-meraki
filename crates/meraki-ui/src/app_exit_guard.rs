use crate::app::{MerakiApp, Message};
use crate::app_project_io::{pick_open_project_file, ProjectIoMessage};
use crate::app_screens::AppScreen;
use crate::components::{settings_menu, unsaved_modal};
use iced::{window, Element, Task};
use meraki_engine::EngineCommand;
use std::path::PathBuf;

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
            true,
            self.project_io.is_dirty,
            self.preferences.keymap,
            &self.projects_dir,
        );
        unsaved_modal::overlay(with_settings, &self.project.name, self.pending_exit.as_ref())
    }
}
