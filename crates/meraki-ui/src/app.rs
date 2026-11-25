use crate::app_delivery::{DeliveryMessage, DeliveryState};
use crate::app_exit_guard::{ExitGuardMessage, PendingExitAction};
use crate::app_precision::PrecisionMessage;
use crate::app_preferences::preferences_path;
use crate::app_project_io::{ProjectIoMessage, ProjectIoState};
use crate::app_screens::{AppScreen, ScreenMessage, ScreenState};
use crate::layout::{sidebar, topbar};
use crate::navigation::WorkspaceTab;
use crate::playback::{self, PlaybackMessage, PlaybackState};
use crate::project_storage::internal_projects_dir;
use crate::views::audio::{self, AudioMessage, AudioState};
use crate::views::delivery;
use crate::views::edit::{self, EditMessage, EditState};
use crate::views::effects::{self, EffectsMessage, EffectsState};
use crate::views::files::{self, FilesMessage, FilesState};
use crate::{subscription, theme};
use iced::widget::{column, container, horizontal_rule, image, row, vertical_rule};
use iced::{Element, Length, Subscription, Task, Theme};
use meraki_core::{
    AddAssetCommand, AddClipCommand, AppPreferences, AssetId, ClipId, ClipTransform, Command,
    FolderId, HistoryError, HistoryStack, MediaAsset, MoveClipCommand, PreviewQuality, Project,
    RemoveClipCommand, SplitClipCommand, TrackId, TrimClipCommand, UpdateClipTransformCommand,
};
use meraki_engine::{BridgeError, EngineCommand, EngineEvent, EngineHandle};
use std::path::PathBuf;
use tracing::info;

/// Mensagens da interface principal do Meraki.
#[derive(Debug, Clone)]
pub enum Message {
    /// Alterna para uma das 5 abas fixas da barra lateral esquerda.
    SelectTab(WorkspaceTab),
    /// Ação do botão Exportar na barra superior.
    ExportClicked,
    /// Interações da aba Arquivos.
    Files(FilesMessage),
    /// Interações da aba Edição (gaveta, player, inspetor e timeline canvas).
    Edit(EditMessage),
    /// Interações da aba Efeitos (biblioteca, overlay do player, inspetor e timeline).
    Effects(EffectsMessage),
    /// Interações da aba Áudio (biblioteca/presets, EQ visual, mixer vertical e automação).
    Audio(AudioMessage),
    /// Interações da aba Entrega (presets, resumo, configuração e fila de exportação).
    Delivery(DeliveryMessage),
    /// Ações de abertura, salvamento e renomeação do arquivo `.meraki`.
    ProjectIo(ProjectIoMessage),
    /// Edição precisa na agulha, ferramentas e navegação quadro a quadro.
    Precision(PrecisionMessage),
    /// Navegação entre telas v0.2.0 (`ProjectHub`, `KeymapWelcome` e `Editor`).
    Screen(ScreenMessage),
    /// Pedido de fechamento nativo da janela interceptado pela guarda de alterações.
    WindowCloseRequested(iced::window::Id),
    /// Ação do modal de confirmação ao sair com alterações não salvas.
    ExitGuard(ExitGuardMessage),
    /// Comandos de reprodução, atalhos de teclado e edição na timeline.
    Playback(PlaybackMessage),
    /// Evento assíncrono emitido pelo motor multimídia.
    EngineEvent(EngineEvent),
}

/// Estado raiz da aplicação gráfica do Meraki conectado ao motor via canal não bloqueante.
pub struct MerakiApp {
    pub(crate) engine: EngineHandle,
    pub(crate) project: Project,
    pub(crate) history: HistoryStack,
    pub(crate) active_tab: WorkspaceTab,
    pub(crate) files: FilesState,
    pub(crate) edit: EditState,
    pub(crate) effects: EffectsState,
    pub(crate) audio: AudioState,
    pub(crate) delivery: DeliveryState,
    pub(crate) project_io: ProjectIoState,
    pub preferences: AppPreferences,
    pub(crate) preferences_path: PathBuf,
    pub(crate) projects_dir: PathBuf,
    pub(crate) pending_exit: Option<PendingExitAction>,
    pub(crate) screens: ScreenState,
    pub(crate) playback: PlaybackState,
    pub(crate) engine_ready: bool,
    pub(crate) last_error: Option<String>,
}

impl MerakiApp {
    pub fn new(engine: EngineHandle) -> Self {
        Self::with_project(engine, Project::new("Projeto sem título"))
    }

    pub fn with_project(engine: EngineHandle, project: Project) -> Self {
        Self::with_storage_paths(engine, project, preferences_path(), internal_projects_dir())
    }

    pub fn project(&self) -> &Project { &self.project }
    pub fn project_name(&self) -> &str { &self.project.name }
    pub fn history(&self) -> &HistoryStack { &self.history }
    pub fn engine(&self) -> &EngineHandle { &self.engine }
    pub fn active_tab(&self) -> WorkspaceTab { self.active_tab }
    pub fn files_state(&self) -> &FilesState { &self.files }
    pub fn edit_state(&self) -> &EditState { &self.edit }
    pub fn effects_state(&self) -> &EffectsState { &self.effects }
    pub fn audio_state(&self) -> &AudioState { &self.audio }
    pub fn delivery_state(&self) -> &DeliveryState { &self.delivery }
    pub fn playback(&self) -> &PlaybackState { &self.playback }
    pub fn current_frame(&self) -> Option<&image::Handle> { self.playback.current_frame.as_ref() }
    pub fn is_playing(&self) -> bool { self.playback.is_playing }
    pub fn playhead_ns(&self) -> u64 { self.playback.playhead_ns }
    pub fn playback_rate(&self) -> f64 { self.playback.playback_rate }
    pub fn master_volume(&self) -> f32 { self.playback.master_volume }
    pub fn preview_quality(&self) -> PreviewQuality { self.playback.preview_quality }
    pub fn selected_clip_id(&self) -> Option<ClipId> { self.playback.selected_clip_id }
    pub fn is_engine_ready(&self) -> bool { self.engine_ready }
    pub fn last_error(&self) -> Option<&str> { self.last_error.as_deref() }

    /// Converte o `Project`/`Timeline` atual em `EngineCommand::SyncTimeline` e despacha ao motor.
    pub fn sync_timeline_to_engine(&self) -> Result<(), BridgeError> {
        let clips = playback::build_render_clips(&self.project, self.playback.preview_quality);
        let audio_mix = playback::build_audio_mix(&self.project);
        self.engine.send(EngineCommand::SyncTimeline {
            clips,
            audio_mix,
            preview_quality: self.playback.preview_quality,
        })
    }

    /// Registra uma mídia no `Project` e despacha inspeção assíncrona ao motor sem bloquear a UI.
    pub fn import_media(
        &mut self,
        path: PathBuf,
        folder_id: Option<FolderId>,
        generate_proxy: bool,
    ) -> Result<AssetId, String> {
        let name = path
            .file_name()
            .and_then(|file| file.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Mídia sem nome")
            .to_string();
        let kind = files::infer_media_kind(&path);
        let asset = MediaAsset::new(name, path.clone(), kind, folder_id, 0);
        let asset_id = asset.id;

        if let Err(err) = self.history.execute(Box::new(AddAssetCommand::new(asset)), &mut self.project) {
            let message = err.to_string();
            self.last_error = Some(message.clone());
            return Err(message);
        }
        self.mark_project_dirty();

        if let Err(err) = self.engine.send(EngineCommand::ImportMedia { asset_id, path, generate_proxy }) {
            let message = err.to_string();
            self.last_error = Some(message.clone());
            return Err(message);
        }

        Ok(asset_id)
    }

    /// Envia um ativo para a timeline (`V1` ou `A1`) respeitando In/Out e sincroniza com o motor.
    pub fn send_asset_to_timeline(&mut self, asset_id: AssetId) -> Result<ClipId, String> {
        let Some((track_id, clip)) = playback::build_clip_for_asset(&self.project, asset_id) else {
            let msg = format!("não foi possível criar clipe na timeline para ativo {asset_id}");
            self.last_error = Some(msg.clone());
            return Err(msg);
        };
        let clip_id = clip.id;
        self.execute_timeline_command(Box::new(AddClipCommand::new(track_id, clip)))?;
        self.playback.selected_clip_id = Some(clip_id);
        Ok(clip_id)
    }

    pub fn move_clip(&mut self, clip_id: ClipId, target_track_id: TrackId, new_start_ns: u64) -> Result<(), String> {
        self.execute_timeline_command(Box::new(MoveClipCommand::new(clip_id, target_track_id, new_start_ns)))
    }

    pub fn trim_clip(
        &mut self,
        clip_id: ClipId,
        new_start_ns: u64,
        new_source_in_ns: u64,
        new_duration_ns: u64,
        max_source_ns: Option<u64>,
    ) -> Result<(), String> {
        self.execute_timeline_command(Box::new(TrimClipCommand::new(
            clip_id,
            new_start_ns,
            new_source_in_ns,
            new_duration_ns,
            max_source_ns,
        )))
    }

    pub fn split_clip(&mut self, clip_id: ClipId, split_at_ns: u64) -> Result<ClipId, String> {
        let cmd = SplitClipCommand::new(clip_id, split_at_ns);
        let right_id = cmd.right_clip_id();
        self.execute_timeline_command(Box::new(cmd))?;
        Ok(right_id)
    }

    pub fn update_clip_transform(&mut self, clip_id: ClipId, transform: ClipTransform) -> Result<(), String> {
        self.execute_timeline_command(Box::new(UpdateClipTransformCommand::new(clip_id, transform)))
    }

    pub fn remove_clip(&mut self, clip_id: ClipId) -> Result<(), String> {
        self.execute_timeline_command(Box::new(RemoveClipCommand::new(clip_id)))
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo(&mut self.project) {
            Ok(()) => {
                self.clear_inspector_drafts();
                self.prune_selected_clip();
                self.mark_project_dirty();
                self.dispatch_timeline_sync();
                true
            }
            Err(HistoryError::NothingToUndo) => false,
            Err(err) => {
                self.last_error = Some(err.to_string());
                false
            }
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.history.redo(&mut self.project) {
            Ok(()) => {
                self.clear_inspector_drafts();
                self.prune_selected_clip();
                self.mark_project_dirty();
                self.dispatch_timeline_sync();
                true
            }
            Err(HistoryError::NothingToRedo) => false,
            Err(err) => {
                self.last_error = Some(err.to_string());
                false
            }
        }
    }

    pub(crate) fn clear_inspector_drafts(&mut self) {
        self.edit.transform_draft = None;
        self.effects.title_draft = None;
        self.effects.title_font_size_draft = None;
        self.effects.transform_draft = None;
    }

    pub fn request_shutdown(&self) -> Result<(), BridgeError> { self.engine.send(EngineCommand::Shutdown) }
    pub fn title(&self) -> String {
        if self.project_io.is_dirty { format!("meraki — {} *", self.project.name) } else { format!("meraki — {}", self.project.name) }
    }
    pub fn theme(&self) -> Theme { theme::meraki_theme() }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            subscription::engine_events(&self.engine).map(Message::EngineEvent),
            subscription::window_close_requests(),
        ];
        if self.screens.screen == AppScreen::Editor {
            subs.push(subscription::keyboard_shortcuts(self.preferences.keymap));
            subs.push(
                iced::time::every(std::time::Duration::from_secs(60))
                    .map(|_| Message::ProjectIo(ProjectIoMessage::AutoSaveTick)),
            );
        }
        Subscription::batch(subs)
    }

    /// Processa eventos de interação da interface sem bloquear a thread de UI.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(tab) => {
                self.active_tab = tab;
                if tab == WorkspaceTab::Audio {
                    self.ensure_audio_tab_ready();
                }
                Task::none()
            }
            Message::ExportClicked => {
                info!(project = %self.project.name, "botão Exportar acionado; abrindo aba Entrega");
                self.active_tab = WorkspaceTab::Delivery;
                Task::none()
            }
            Message::Files(msg) => self.handle_files_message(msg),
            Message::Edit(msg) => {
                self.handle_edit_message(msg);
                Task::none()
            }
            Message::Effects(msg) => {
                self.handle_effects_message(msg);
                Task::none()
            }
            Message::Audio(msg) => self.handle_audio_message(msg),
            Message::Delivery(msg) => self.handle_delivery_message(msg),
            Message::ProjectIo(msg) => self.handle_project_io_message(msg),
            Message::Precision(msg) => {
                self.handle_precision_message(msg);
                Task::none()
            }
            Message::Screen(msg) => self.handle_screen_message(msg),
            Message::WindowCloseRequested(id) => {
                self.request_exit_action(PendingExitAction::CloseWindow(id))
            }
            Message::ExitGuard(msg) => self.handle_exit_guard_message(msg),
            Message::Playback(msg) => {
                self.handle_playback_message(msg);
                Task::none()
            }
            Message::EngineEvent(event) => {
                self.handle_engine_event(event);
                Task::none()
            }
        }
    }

    pub(crate) fn execute_timeline_command(&mut self, command: Box<dyn Command>) -> Result<(), String> {
        if let Err(err) = self.history.execute(command, &mut self.project) {
            let message = err.to_string();
            self.last_error = Some(message.clone());
            return Err(message);
        }
        self.prune_selected_clip();
        self.mark_project_dirty();
        self.dispatch_timeline_sync();
        Ok(())
    }

    fn dispatch_timeline_sync(&mut self) {
        if let Err(err) = self.sync_timeline_to_engine() {
            self.last_error = Some(err.to_string());
        }
    }

    fn prune_selected_clip(&mut self) {
        if let Some(selected) = self.playback.selected_clip_id {
            if self.project.timeline.clip(selected).is_none() {
                self.playback.selected_clip_id = None;
            }
        }
    }

    fn handle_engine_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::ExportProgress { .. }
            | EngineEvent::ExportFinished { .. }
            | EngineEvent::ExportFailed { .. }
            | EngineEvent::ExportCancelled { .. } => self.handle_export_engine_event(event),
            other => {
                self.handle_media_visual_event(&other);
                playback::apply_engine_event(
                    &mut self.playback,
                    &mut self.project,
                    &mut self.engine_ready,
                    &mut self.last_error,
                    other,
                );
            }
        }
    }

    /// Renderiza a tela ativa (`ProjectHub`, `KeymapWelcome` ou `Editor`).
    pub fn view(&self) -> Element<'_, Message> {
        self.render_current_screen()
    }

    pub(crate) fn view_editor(&self) -> Element<'_, Message> {
        let is_delivery = self.active_tab == WorkspaceTab::Delivery;
        let profile = self.preferences.keymap;
        let top = topbar::view(
            &self.project.name,
            &self.project_io,
            self.screens.settings_menu_open,
            is_delivery,
        );
        let left_nav = sidebar::view(self.active_tab);
        let main_area = match self.active_tab {
            WorkspaceTab::Files => files::view(&self.project, &self.files, &self.project_io).map(Message::Files),
            WorkspaceTab::Edit => edit::view(&self.project, &self.playback, &self.history, &self.edit, &self.project_io, profile).map(Message::Edit),
            WorkspaceTab::Effects => effects::view(&self.project, &self.playback, &self.history, &self.effects, profile).map(Message::Effects),
            WorkspaceTab::Audio => audio::view(&self.project, &self.playback, &self.history, &self.audio, profile).map(Message::Audio),
            WorkspaceTab::Delivery => delivery::view(&self.project, &self.playback, &self.delivery).map(Message::Delivery),
        };
        let body = row![left_nav, vertical_rule(1).style(theme::divider), main_area]
            .width(Length::Fill)
            .height(Length::Fill);
        let root = column![top, horizontal_rule(1).style(theme::divider), body]
            .width(Length::Fill)
            .height(Length::Fill);
        let editor = container(root).width(Length::Fill).height(Length::Fill).style(theme::base_canvas).into();
        self.wrap_with_editor_overlays(editor)
    }
}
