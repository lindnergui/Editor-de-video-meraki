use crate::app::{MerakiApp, Message};
use crate::components::effects_inspector::{self, EffectsInspectorMessage};
use crate::components::effects_library::EffectsLibraryMessage;
use crate::components::timeline_canvas::TimelineCanvasMessage;
use crate::playback::PlaybackMessage;
use crate::views::edit::EditMessage;
use crate::views::effects::{self, EffectsMessage};
use crate::views::files::{self, FilesMessage};
use iced::Task;
use meraki_core::{
    AddTitleClipCommand, ClipId, ClipTransition, Keyframe, KeyframeEasing,
    KeyframeProperty, MediaKind, RemoveKeyframeCommand, SetClipTransitionCommand, TitleOverlay,
    TitleStyle, ToggleClipEffectCommand, TransitionEdge, TransitionKind, UpdateTitleCommand,
    UpsertKeyframeCommand, VideoEffectKind,
};

const DEFAULT_TITLE_DURATION_NS: u64 = 3_000_000_000;
const DEFAULT_TRANSITION_DURATION_NS: u64 = 500_000_000;

impl MerakiApp {
    /// Insere um clipe de título em `T1`, seleciona-o e sincroniza com o motor.
    pub fn add_title_clip(
        &mut self,
        start_ns: u64,
        duration_ns: u64,
        title: TitleOverlay,
    ) -> Result<ClipId, String> {
        let cmd = AddTitleClipCommand::new(None, start_ns, duration_ns, title);
        let clip_id = cmd.clip_id();
        self.execute_timeline_command(Box::new(cmd))?;
        self.playback.selected_clip_id = Some(clip_id);
        self.effects.title_draft = None;
        Ok(clip_id)
    }

    pub fn update_title(&mut self, clip_id: ClipId, title: TitleOverlay) -> Result<(), String> {
        self.record_timeline_command(Box::new(UpdateTitleCommand::new(clip_id, title.clone())))?;
        self.preview_clip(clip_id, None, Some(title));
        Ok(())
    }

    pub fn toggle_clip_effect(
        &mut self,
        clip_id: ClipId,
        kind: VideoEffectKind,
        intensity: f32,
    ) -> Result<(), String> {
        self.execute_timeline_command(Box::new(ToggleClipEffectCommand::new(clip_id, kind, intensity)))
    }

    pub fn set_clip_transition(
        &mut self,
        clip_id: ClipId,
        edge: TransitionEdge,
        transition: Option<ClipTransition>,
    ) -> Result<(), String> {
        self.execute_timeline_command(Box::new(SetClipTransitionCommand::new(clip_id, edge, transition)))
    }

    pub fn upsert_keyframe(&mut self, clip_id: ClipId, keyframe: Keyframe) -> Result<(), String> {
        self.execute_timeline_command(Box::new(UpsertKeyframeCommand::new(clip_id, keyframe)))
    }

    pub fn remove_keyframe(
        &mut self,
        clip_id: ClipId,
        property: KeyframeProperty,
        time_offset_ns: u64,
    ) -> Result<(), String> {
        self.execute_timeline_command(Box::new(RemoveKeyframeCommand::new(clip_id, property, time_offset_ns)))
    }

    pub(crate) fn handle_files_message(&mut self, message: FilesMessage) -> Task<Message> {
        match message {
            FilesMessage::ImportClicked => {
                return Task::perform(files::pick_media_files(), |paths| {
                    Message::Files(FilesMessage::FilesPicked(paths))
                });
            }
            FilesMessage::FilesPicked(paths) => {
                for path in paths {
                    let target_folder = self
                        .files
                        .selected_folder
                        .or_else(|| files::default_folder_for(&self.project, &path));
                    if let Ok(asset_id) = self.import_media(path, target_folder, true) {
                        self.files.selected_asset = Some(asset_id);
                        self.files.preview_progress = 0.0;
                    }
                }
            }
            FilesMessage::SearchChanged(query) => self.files.search_query = query,
            FilesMessage::SelectFolder(folder_id) => self.files.selected_folder = folder_id,
            FilesMessage::ToggleNewFolderInput => {
                self.files.creating_folder = !self.files.creating_folder;
                self.files.new_folder_name.clear();
            }
            FilesMessage::NewFolderNameChanged(name) => self.files.new_folder_name = name,
            FilesMessage::ConfirmNewFolder => {
                match files::confirm_new_folder(&mut self.project, &mut self.history, &mut self.files) {
                    Ok(()) => self.mark_project_dirty(),
                    Err(err) => self.last_error = Some(err),
                }
            }
            FilesMessage::SelectFilter(filter) => self.files.active_filter = filter,
            FilesMessage::SelectAsset(asset_id) => {
                self.files.selected_asset = Some(asset_id);
                self.files.preview_progress = self
                    .project
                    .asset(asset_id)
                    .filter(|a| a.metadata.duration_ns > 0)
                    .map_or(0.0, |a| {
                        (a.in_point_ns as f32 / a.metadata.duration_ns as f32).clamp(0.0, 1.0)
                    });
            }
            FilesMessage::PreviewScrubChanged(progress) => {
                self.files.preview_progress = progress.clamp(0.0, 1.0);
            }
            FilesMessage::MarkIn | FilesMessage::MarkOut => {
                if let Some(asset_id) = self.files.selected_asset {
                    let mark_in = matches!(message, FilesMessage::MarkIn);
                    match files::apply_in_out_mark(
                        &mut self.project,
                        asset_id,
                        self.files.preview_progress,
                        mark_in,
                    ) {
                        Ok(()) => self.mark_project_dirty(),
                        Err(err) => self.last_error = Some(err),
                    }
                }
            }
            FilesMessage::SendToTimeline(asset_id) => {
                let _ = self.send_asset_to_timeline(asset_id);
            }
        }
        Task::none()
    }

    pub(crate) fn handle_edit_message(&mut self, message: EditMessage) {
        if self.edit.apply_local(&message) {
            return;
        }
        match message {
            EditMessage::InsertAsset(asset_id) => {
                let _ = self.send_asset_to_timeline(asset_id);
            }
            EditMessage::Player(p) => self.handle_player_message(p),
            EditMessage::Inspector(insp_msg) => self.handle_edit_inspector_message(insp_msg),
            EditMessage::Timeline(tl) => self.handle_timeline_canvas_message(tl),
            EditMessage::SearchChanged(_) | EditMessage::SelectFolder(_) => {}
        }
    }

    pub(crate) fn handle_effects_message(&mut self, message: EffectsMessage) {
        if self.effects.apply_local(&message) {
            return;
        }
        match message {
            EffectsMessage::Library(lib_msg) => self.handle_library_action(lib_msg),
            EffectsMessage::Player(p) => self.handle_player_message(p),
            EffectsMessage::Overlay(overlay_msg) => self.handle_overlay_message(overlay_msg),
            EffectsMessage::Inspector(insp_msg) => self.handle_effects_inspector(insp_msg),
            EffectsMessage::Timeline(tl) => self.handle_timeline_canvas_message(tl),
        }
    }

    fn handle_library_action(&mut self, msg: EffectsLibraryMessage) {
        match msg {
            EffectsLibraryMessage::AddTitle(style) => {
                let label = match style {
                    TitleStyle::ModernText => "Texto Moderno",
                    TitleStyle::QuickCaption => "Legenda Rápida",
                    TitleStyle::Title3D => "Título 3D",
                };
                let start_ns = effects::next_title_start_ns(
                    &self.project,
                    self.playback.playhead_ns,
                    DEFAULT_TITLE_DURATION_NS,
                );
                let title = TitleOverlay::new(label, style);
                if let Ok(clip_id) = self.add_title_clip(start_ns, DEFAULT_TITLE_DURATION_NS, title) {
                    self.effects.title_draft = Some((clip_id, label.to_string()));
                }
            }
            EffectsLibraryMessage::ApplyTransition(kind) => self.apply_library_transition(kind),
            EffectsLibraryMessage::ToggleTransition(kind) => self.toggle_selected_transition(kind),
            EffectsLibraryMessage::ToggleVideoEffect(kind) => self.apply_library_video_effect(kind),
            EffectsLibraryMessage::SearchChanged(_) | EffectsLibraryMessage::ToggleSection(_) => {}
        }
    }

    fn toggle_selected_transition(&mut self, kind: TransitionKind) {
        let Some(clip_id) = self.resolve_effect_target_clip_id() else {
            return;
        };
        let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
            return;
        };
        let next = if clip.transition_in.is_some_and(|t| t.kind == kind) {
            None
        } else {
            let dur = DEFAULT_TRANSITION_DURATION_NS.min(clip.duration_ns.max(1));
            Some(ClipTransition::new(kind, dur))
        };
        let _ = self.set_clip_transition(clip_id, TransitionEdge::In, next);
    }

    fn handle_effects_inspector(&mut self, msg: EffectsInspectorMessage) {
        match msg {
            EffectsInspectorMessage::TitleTextChanged(clip_id, new_text) => {
                self.effects.title_draft = Some((clip_id, new_text.clone()));
                if new_text.trim().is_empty() {
                    return;
                }
                if let Some(mut title) = self
                    .project
                    .timeline
                    .clip(clip_id)
                    .and_then(|(_, c)| c.title.clone())
                {
                    title.text = new_text;
                    let _ = self.update_title(clip_id, title);
                }
            }
            EffectsInspectorMessage::PreviewTitleFontSize(clip_id, size) => {
                self.effects.title_font_size_draft = Some((clip_id, size));
                self.preview_title_font(clip_id, size);
            }
            EffectsInspectorMessage::CommitTitleFontSize(clip_id) => {
                if let Some((draft_id, size)) = self.effects.title_font_size_draft.take() {
                    if draft_id == clip_id {
                        if let Some(mut title) = self
                            .project
                            .timeline
                            .clip(clip_id)
                            .and_then(|(_, c)| c.title.clone())
                        {
                            if title.font_size != size {
                                title.font_size = size;
                                let _ = self.update_title(clip_id, title);
                            }
                        }
                    }
                }
            }
            EffectsInspectorMessage::UpdateTitle(clip_id, title) => {
                self.effects.title_draft = Some((clip_id, title.text.clone()));
                self.effects.title_font_size_draft = None;
                let _ = self.update_title(clip_id, title);
            }
            EffectsInspectorMessage::PreviewTransform(clip_id, transform) => {
                self.effects.transform_draft = Some((clip_id, transform));
                self.preview_transform_live(clip_id, transform);
            }
            EffectsInspectorMessage::CommitTransform(clip_id) => {
                if let Some((draft_id, tr)) = self.effects.transform_draft.take() {
                    let changed = draft_id == clip_id
                        && self
                            .project
                            .timeline
                            .clip(clip_id)
                            .is_some_and(|(_, c)| c.transform != tr);
                    if changed {
                        let _ = self.update_clip_transform(clip_id, tr);
                    }
                }
            }
            EffectsInspectorMessage::UpdateTransform(clip_id, transform) => {
                self.effects.transform_draft = None;
                let _ = self.update_clip_transform(clip_id, transform);
            }
            other @ (EffectsInspectorMessage::SelectEffect(..)
            | EffectsInspectorMessage::PreviewEffectParams(..)
            | EffectsInspectorMessage::CommitEffectParams(..)
            | EffectsInspectorMessage::UpdateEffectParams(..)
            | EffectsInspectorMessage::ToggleEffectById(..)
            | EffectsInspectorMessage::RemoveEffectById(..)
            | EffectsInspectorMessage::ResetEffectById(..)
            | EffectsInspectorMessage::ApplyZoomPreset(..)) => {
                self.handle_effect_params_inspector_message(other);
            }
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property } => {
                self.effects.active_property = property;
                self.toggle_clip_keyframe(clip_id, property);
            }
            EffectsInspectorMessage::SetEasing(easing) => {
                self.effects.active_easing = easing;
                self.apply_easing_to_active_keyframe(easing);
            }
        }
    }

    fn toggle_clip_keyframe(&mut self, clip_id: ClipId, property: KeyframeProperty) {
        let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
            return;
        };
        let offset_ns = effects_inspector::clip_local_offset_ns(clip, self.playback.playhead_ns);
        if let Some(existing) = effects_inspector::find_keyframe_near(clip, property, offset_ns) {
            let _ = self.remove_keyframe(clip_id, property, existing.time_offset_ns);
            return;
        }
        let value = match property {
            KeyframeProperty::PosX => clip.transform.pos_x,
            KeyframeProperty::PosY => clip.transform.pos_y,
            KeyframeProperty::Scale => clip.transform.scale,
            KeyframeProperty::Opacity => clip.transform.opacity,
        };
        let kf = Keyframe::new(offset_ns, property, value, self.effects.active_easing);
        let _ = self.upsert_keyframe(clip_id, kf);
    }

    fn apply_easing_to_active_keyframe(&mut self, easing: KeyframeEasing) {
        let Some(clip_id) = self.playback.selected_clip_id else {
            return;
        };
        let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
            return;
        };
        let offset_ns = effects_inspector::clip_local_offset_ns(clip, self.playback.playhead_ns);
        let candidate = effects_inspector::find_keyframe_near(clip, self.effects.active_property, offset_ns)
            .or_else(|| {
                clip.keyframes.iter().copied().find(|kf| {
                    kf.time_offset_ns.abs_diff(offset_ns) <= effects_inspector::KEYFRAME_SNAP_TOLERANCE_NS
                })
            });
        if let Some(mut kf) = candidate {
            if kf.easing != easing {
                kf.easing = easing;
                let _ = self.upsert_keyframe(clip_id, kf);
            }
        }
    }

    pub(crate) fn handle_timeline_canvas_message(&mut self, msg: TimelineCanvasMessage) {
        match msg {
            TimelineCanvasMessage::Scrub(ns) => self.handle_playback_message(PlaybackMessage::Seek(ns)),
            TimelineCanvasMessage::ScrubEmpty(ns) => {
                self.clear_inspector_drafts();
                self.handle_playback_message(PlaybackMessage::SelectClip(None));
                self.reconcile_selected_effect();
                self.handle_playback_message(PlaybackMessage::Seek(ns));
            }
            TimelineCanvasMessage::SelectClip(id) => {
                self.clear_inspector_drafts();
                self.handle_playback_message(PlaybackMessage::SelectClip(id));
                self.reconcile_selected_effect();
            }
            TimelineCanvasMessage::MoveClip { clip_id, track_id, new_start_ns } => {
                let _ = self.move_clip(clip_id, track_id, new_start_ns);
            }
            TimelineCanvasMessage::TrimClip {
                clip_id,
                new_start_ns,
                new_source_in_ns,
                new_duration_ns,
            } => {
                let max_src = self
                    .project
                    .timeline
                    .clip(clip_id)
                    .and_then(|(_, c)| self.project.asset(c.asset_id))
                    .filter(|a| a.kind != MediaKind::Image && a.metadata.duration_ns > 0)
                    .map(|a| a.metadata.duration_ns);
                let _ = self.trim_clip(clip_id, new_start_ns, new_source_in_ns, new_duration_ns, max_src);
            }
            TimelineCanvasMessage::SplitClip { clip_id, split_at_ns } => {
                let _ = self.split_clip(clip_id, split_at_ns);
            }
            TimelineCanvasMessage::SplitAtPlayhead => {
                let _ = self.split_at_playhead();
            }
            TimelineCanvasMessage::SetTool(_)
            | TimelineCanvasMessage::ToggleSnap
            | TimelineCanvasMessage::ZoomIn
            | TimelineCanvasMessage::ZoomOut
            | TimelineCanvasMessage::ZoomAnchored { .. } => {}
        }
    }
}
