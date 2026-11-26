use crate::components::effects_inspector::EffectsInspectorMessage;
use crate::components::effects_params_panel::{
    apply_zoom_preset, default_params_for_effect,
};
use crate::components::inspector::InspectorMessage;
use crate::components::player_overlay::PlayerOverlayMessage;
use crate::MerakiApp;
use meraki_core::{
    AddClipEffectCommand, ClipEffect, ClipId, ClipTransform, EffectParameters,
    RemoveEffectCommand, TitleOverlay, ToggleEffectCommand, TrackKind, TransitionEffectKind,
    TransitionParams, UpdateClipTransformCommand, UpdateEffectParamsCommand, VideoEffectKind,
};
use meraki_engine::EngineCommand;
use uuid::Uuid;

impl MerakiApp {
    pub fn update_clip_transform(
        &mut self,
        clip_id: ClipId,
        transform: ClipTransform,
    ) -> Result<(), String> {
        let prev = self.project.timeline.clip(clip_id).map(|(_, c)| c.transform);
        self.record_timeline_command(Box::new(UpdateClipTransformCommand::new(
            clip_id, transform,
        )))?;

        let spatial_changed = prev.is_none_or(|p| {
            p.scale != transform.scale
                || p.pos_x != transform.pos_x
                || p.pos_y != transform.pos_y
                || p.opacity != transform.opacity
        });
        let speed_changed = prev.is_some_and(|p| (p.speed - transform.speed).abs() > f32::EPSILON);

        if spatial_changed || !speed_changed {
            self.preview_clip(clip_id, Some(transform), None);
        }
        if speed_changed {
            self.dispatch_timeline_sync();
        }
        Ok(())
    }

    pub(crate) fn preview_transform_live(&mut self, clip_id: ClipId, transform: ClipTransform) {
        let only_speed_changed = self.project.timeline.clip(clip_id).is_some_and(|(_, c)| {
            let p = c.transform;
            p.scale == transform.scale
                && p.pos_x == transform.pos_x
                && p.pos_y == transform.pos_y
                && p.opacity == transform.opacity
                && (p.speed - transform.speed).abs() > f32::EPSILON
        });
        if !only_speed_changed {
            self.preview_clip(clip_id, Some(transform), None);
        }
    }

    pub(crate) fn preview_clip(
        &mut self,
        clip_id: ClipId,
        transform: Option<ClipTransform>,
        title: Option<TitleOverlay>,
    ) {
        self.send_live_edit(EngineCommand::UpdateClipLive {
            clip_id,
            transform,
            title,
            effects: None,
        });
    }

    pub(crate) fn preview_title_font(&mut self, clip_id: ClipId, size: u32) {
        if let Some(mut title) = self
            .project
            .timeline
            .clip(clip_id)
            .and_then(|(_, clip)| clip.title.clone())
        {
            title.font_size = size;
            self.preview_clip(clip_id, None, Some(title));
        }
    }

    pub(crate) fn preview_overlay_position(&mut self, clip_id: ClipId, pos_x: f32, pos_y: f32) {
        if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
            self.preview_clip(
                clip_id,
                Some(ClipTransform { pos_x, pos_y, ..clip.transform }),
                None,
            );
        }
    }

    pub(crate) fn handle_overlay_message(&mut self, msg: PlayerOverlayMessage) {
        match msg {
            PlayerOverlayMessage::PreviewPosition { clip_id, pos_x, pos_y } => {
                if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
                    self.effects.transform_draft = Some((clip_id, ClipTransform { pos_x, pos_y, ..clip.transform }));
                }
                self.preview_overlay_position(clip_id, pos_x, pos_y);
            }
            PlayerOverlayMessage::CommitPosition { clip_id, pos_x, pos_y } => {
                self.effects.transform_draft = None;
                if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
                    let _ = self.update_clip_transform(clip_id, ClipTransform { pos_x, pos_y, ..clip.transform });
                }
            }
            PlayerOverlayMessage::PreviewScale { clip_id, scale } => {
                if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
                    let next = ClipTransform { scale, ..clip.transform };
                    self.effects.transform_draft = Some((clip_id, next));
                    self.preview_transform_live(clip_id, next);
                }
            }
            PlayerOverlayMessage::CommitScale { clip_id, scale } => {
                self.effects.transform_draft = None;
                if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
                    let _ = self.update_clip_transform(clip_id, ClipTransform { scale, ..clip.transform });
                }
            }
        }
    }

    pub fn add_clip_effect(&mut self, clip_id: ClipId, effect: ClipEffect) -> Result<Uuid, String> {
        let cmd = AddClipEffectCommand::new(clip_id, effect);
        let id = cmd.effect_id();
        self.execute_timeline_command(Box::new(cmd))?;
        if self.playback.selected_clip_id == Some(clip_id) {
            self.effects.selected_effect_id = Some(id);
        }
        Ok(id)
    }

    pub(crate) fn resolve_effect_target_clip_id(&mut self) -> Option<ClipId> {
        if let Some(id) = self.playback.selected_clip_id {
            if self.project.timeline.clip(id).is_some() {
                return Some(id);
            }
        }
        let playhead = self.playback.playhead_ns;
        for kind in [TrackKind::Video, TrackKind::Overlay] {
            if let Some(clip) = self
                .project
                .timeline
                .tracks
                .iter()
                .filter(|t| t.kind == kind)
                .flat_map(|t| t.clips.iter())
                .find(|c| {
                    c.timeline_start_ns <= playhead
                        && playhead < c.timeline_start_ns.saturating_add(c.duration_ns)
                })
            {
                self.playback.selected_clip_id = Some(clip.id);
                return Some(clip.id);
            }
        }
        None
    }

    pub(crate) fn reconcile_selected_effect(&mut self) {
        let Some(clip) = self
            .playback
            .selected_clip_id
            .and_then(|id| self.project.timeline.clip(id).map(|(_, c)| c))
        else {
            self.effects.selected_effect_id = None;
            return;
        };
        let valid = self
            .effects
            .selected_effect_id
            .is_some_and(|eid| clip.effects.iter().any(|e| e.id == eid));
        if !valid {
            self.effects.selected_effect_id = clip.effects.first().map(|e| e.id);
        }
    }

    pub(crate) fn apply_library_video_effect(&mut self, kind: VideoEffectKind) {
        let Some(clip_id) = self.resolve_effect_target_clip_id() else {
            return;
        };
        if let Some(existing) = self
            .project
            .timeline
            .clip(clip_id)
            .and_then(|(_, c)| c.effects.iter().find(|e| e.kind == kind))
            .map(|e| (e.id, e.enabled))
        {
            self.effects.selected_effect_id = Some(existing.0);
            self.effects.effect_params_draft = None;
            if !existing.1 {
                let _ = self.execute_timeline_command(Box::new(ToggleEffectCommand::new(clip_id, existing.0)));
            }
            return;
        }
        let _ = self.add_clip_effect(clip_id, ClipEffect::with_defaults(kind));
    }

    pub(crate) fn apply_library_transition(&mut self, kind: TransitionEffectKind) {
        let Some(clip_id) = self.resolve_effect_target_clip_id() else {
            return;
        };
        if let Some(existing) = self
            .project
            .timeline
            .clip(clip_id)
            .and_then(|(_, c)| {
                c.effects.iter().find(|e| matches!(e.params, EffectParameters::Transition(p) if p.kind == kind))
            })
            .map(|e| (e.id, e.enabled))
        {
            self.effects.selected_effect_id = Some(existing.0);
            self.effects.effect_params_draft = None;
            if !existing.1 {
                let _ = self.execute_timeline_command(Box::new(ToggleEffectCommand::new(clip_id, existing.0)));
            }
            return;
        }
        let mut effect = ClipEffect::with_params(EffectParameters::Transition(TransitionParams {
            kind,
            ..TransitionParams::default()
        }));
        effect.name = match kind {
            TransitionEffectKind::Fade => "Transição Fade",
            TransitionEffectKind::ZoomTransition => "Transição Zoom",
            TransitionEffectKind::BlurTransition => "Transição Desfoque",
        }
        .to_string();
        let _ = self.add_clip_effect(clip_id, effect);
    }

    pub fn update_effect_params(
        &mut self,
        clip_id: ClipId,
        effect_id: Uuid,
        params: EffectParameters,
    ) -> Result<(), String> {
        self.record_timeline_command(Box::new(UpdateEffectParamsCommand::new(
            clip_id, effect_id, params,
        )))?;
        if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
            self.send_live_edit(EngineCommand::UpdateClipLive {
                clip_id,
                transform: None,
                title: None,
                effects: Some(clip.effects.clone()),
            });
        }
        Ok(())
    }

    pub(crate) fn preview_effect_params(
        &mut self,
        clip_id: ClipId,
        effect_id: Uuid,
        params: EffectParameters,
    ) {
        self.effects.effect_params_draft = Some((clip_id, effect_id, params));
        let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
            return;
        };
        let mut preview_effects = clip.effects.clone();
        if let Some(target) = preview_effects.iter_mut().find(|e| e.id == effect_id) {
            target.params = params;
            target.intensity = params.primary_intensity();
        }
        self.send_live_edit(EngineCommand::UpdateClipLive {
            clip_id,
            transform: None,
            title: None,
            effects: Some(preview_effects),
        });
    }

    pub(crate) fn handle_edit_inspector_message(&mut self, msg: InspectorMessage) {
        match msg {
            InspectorMessage::PreviewTransform(clip_id, transform) => {
                self.edit.transform_draft = Some((clip_id, transform));
                self.preview_transform_live(clip_id, transform);
            }
            InspectorMessage::CommitTransform(clip_id) => {
                if let Some((draft_id, tr)) = self.edit.transform_draft.take() {
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
            InspectorMessage::UpdateTransform(clip_id, transform) => {
                self.edit.transform_draft = None;
                let _ = self.update_clip_transform(clip_id, transform);
            }
        }
    }

    pub(crate) fn handle_effect_params_inspector_message(&mut self, msg: EffectsInspectorMessage) {
        match msg {
            EffectsInspectorMessage::SelectEffect(effect_id) => {
                self.effects.selected_effect_id = Some(effect_id);
                self.effects.effect_params_draft = None;
            }
            EffectsInspectorMessage::PreviewEffectParams(clip_id, effect_id, params) => {
                self.preview_effect_params(clip_id, effect_id, params);
            }
            EffectsInspectorMessage::CommitEffectParams(clip_id, effect_id) => {
                if let Some((cid, eid, params)) = self.effects.effect_params_draft.take() {
                    let changed = cid == clip_id
                        && eid == effect_id
                        && self
                            .project
                            .timeline
                            .clip(clip_id)
                            .and_then(|(_, c)| c.effects.iter().find(|e| e.id == effect_id))
                            .is_some_and(|e| e.params != params);
                    if changed {
                        let _ = self.update_effect_params(clip_id, effect_id, params);
                    }
                }
            }
            EffectsInspectorMessage::UpdateEffectParams(clip_id, effect_id, params) => {
                self.effects.effect_params_draft = None;
                let _ = self.update_effect_params(clip_id, effect_id, params);
            }
            EffectsInspectorMessage::ResetEffectById(clip_id, effect_id) => {
                self.effects.effect_params_draft = None;
                if let Some(effect) = self
                    .project
                    .timeline
                    .clip(clip_id)
                    .and_then(|(_, c)| c.effects.iter().find(|e| e.id == effect_id))
                {
                    let def = default_params_for_effect(effect);
                    let _ = self.update_effect_params(clip_id, effect_id, def);
                }
            }
            EffectsInspectorMessage::ApplyZoomPreset(clip_id, effect_id, preset) => {
                self.effects.effect_params_draft = None;
                if let Some(EffectParameters::Zoom(zoom)) = self
                    .project
                    .timeline
                    .clip(clip_id)
                    .and_then(|(_, c)| c.effects.iter().find(|e| e.id == effect_id))
                    .map(|e| e.params)
                {
                    let next = EffectParameters::Zoom(apply_zoom_preset(zoom, preset));
                    let _ = self.update_effect_params(clip_id, effect_id, next);
                }
            }
            EffectsInspectorMessage::ToggleEffectById(clip_id, effect_id) => {
                self.effects.effect_params_draft = None;
                let _ = self.execute_timeline_command(Box::new(ToggleEffectCommand::new(clip_id, effect_id)));
            }
            EffectsInspectorMessage::RemoveEffectById(clip_id, effect_id) => {
                self.effects.effect_params_draft = None;
                let _ = self.execute_timeline_command(Box::new(RemoveEffectCommand::new(clip_id, effect_id)));
                self.reconcile_selected_effect();
            }
            _ => {}
        }
    }

    fn send_live_edit(&mut self, command: EngineCommand) {
        if let Err(err) = self.engine.send(command) {
            self.last_error = Some(err.to_string());
        }
    }
}
