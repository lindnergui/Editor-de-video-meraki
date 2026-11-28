use crate::components::effects_library::TransitionPlacementMode;
use crate::views::transition_inspector::TransitionInspectorMessage;
use crate::MerakiApp;
use meraki_core::{
    ClipId, RemoveTransitionCommand, TimelineTransition, TrackId, TrackKind, TransitionKind,
    TransitionPlacement, UpdateTransitionDurationCommand, UpsertTransitionCommand,
};
use uuid::Uuid;

const CUT_SNAP_TOLERANCE_NS: u64 = 500_000_000;

impl MerakiApp {
    pub fn selected_transition_id(&self) -> Option<Uuid> {
        self.playback.selected_transition_id
    }

    pub(crate) fn select_transition(&mut self, transition_id: Option<Uuid>) {
        self.clear_inspector_drafts();
        self.playback.selected_transition_id = transition_id;
        if transition_id.is_some() {
            self.playback.selected_clip_id = None;
            self.effects.selected_effect_id = None;
        }
    }

    /// Aplica uma transição posicional respeitando o modo escolhido (`BetweenClips`, `ClipStart`, `ClipEnd`).
    pub fn apply_positional_transition(
        &mut self,
        kind: TransitionKind,
        mode: TransitionPlacementMode,
    ) -> Result<Uuid, String> {
        let Some((track_id, placement)) = self.resolve_transition_placement(mode) else {
            return Err("nenhum clipe disponível para aplicar transição".to_string());
        };
        let mut tr = TimelineTransition::with_default_duration(track_id, kind, placement);
        if let Some(existing) = self
            .project
            .timeline
            .transitions
            .iter()
            .find(|t| t.track_id == track_id.0 && t.placement == placement)
        {
            tr.id = existing.id;
            tr.duration_ns = existing.duration_ns;
        }
        let id = tr.id;
        self.execute_timeline_command(Box::new(UpsertTransitionCommand::new(tr)))?;
        self.select_transition(Some(id));
        Ok(id)
    }

    pub fn remove_transition(&mut self, transition_id: Uuid) -> Result<(), String> {
        self.execute_timeline_command(Box::new(RemoveTransitionCommand::new(transition_id)))?;
        if self.playback.selected_transition_id == Some(transition_id) {
            self.playback.selected_transition_id = None;
        }
        self.edit.transition_duration_draft = None;
        self.effects.transition_duration_draft = None;
        Ok(())
    }

    pub fn update_transition_duration(&mut self, transition_id: Uuid, duration_ns: u64) -> Result<(), String> {
        self.execute_timeline_command(Box::new(UpdateTransitionDurationCommand::new(transition_id, duration_ns)))
    }

    pub fn update_transition_kind(&mut self, transition_id: Uuid, kind: TransitionKind) -> Result<(), String> {
        let Some(existing) = self.project.timeline.transition(transition_id).copied() else {
            return Err("transição não encontrada".to_string());
        };
        if existing.kind == kind {
            return Ok(());
        }
        let updated = TimelineTransition { kind, ..existing };
        self.execute_timeline_command(Box::new(UpsertTransitionCommand::new(updated)))
    }

    pub(crate) fn handle_transition_inspector_message(&mut self, msg: TransitionInspectorMessage) {
        match msg {
            TransitionInspectorMessage::SetKind(id, kind) => {
                self.edit.transition_duration_draft = None;
                self.effects.transition_duration_draft = None;
                let _ = self.update_transition_kind(id, kind);
            }
            TransitionInspectorMessage::PreviewDuration(id, dur_ns) => {
                self.edit.transition_duration_draft = Some((id, dur_ns));
                self.effects.transition_duration_draft = Some((id, dur_ns));
            }
            TransitionInspectorMessage::CommitDuration(id) => {
                let draft = self.effects.transition_duration_draft.take().or(self.edit.transition_duration_draft.take());
                self.edit.transition_duration_draft = None;
                if let Some((draft_id, dur_ns)) = draft.filter(|(did, _)| *did == id) {
                    let _ = self.update_transition_duration(draft_id, dur_ns);
                }
            }
            TransitionInspectorMessage::SetDuration(id, dur_ns) => {
                self.edit.transition_duration_draft = None;
                self.effects.transition_duration_draft = None;
                let _ = self.update_transition_duration(id, dur_ns);
            }
            TransitionInspectorMessage::Remove(id) => {
                let _ = self.remove_transition(id);
            }
        }
    }

    fn resolve_transition_placement(&self, mode: TransitionPlacementMode) -> Option<(TrackId, TransitionPlacement)> {
        if mode == TransitionPlacementMode::BetweenClips {
            if let Some(found) = self.find_between_clips_target() {
                return Some(found);
            }
        }
        let (track_id, clip_id) = self.resolve_single_clip_for_transition()?;
        let placement = match mode {
            TransitionPlacementMode::BetweenClips | TransitionPlacementMode::ClipStart => {
                TransitionPlacement::ClipStart { clip_id: clip_id.0 }
            }
            TransitionPlacementMode::ClipEnd => TransitionPlacement::ClipEnd { clip_id: clip_id.0 },
        };
        Some((track_id, placement))
    }

    fn find_between_clips_target(&self) -> Option<(TrackId, TransitionPlacement)> {
        let tl = &self.project.timeline;
        for kind in [TrackKind::Video, TrackKind::Overlay] {
            for track in tl.tracks.iter().filter(|t| t.kind == kind) {
                if let Some((l, r, _)) = tl.find_adjacent_pair_near(track.id, self.playback.playhead_ns, CUT_SNAP_TOLERANCE_NS) {
                    return Some((track.id, TransitionPlacement::BetweenClips { left_clip_id: l, right_clip_id: r }));
                }
            }
        }
        if let Some(sel) = self.playback.selected_clip_id {
            if let Some((tid, _)) = tl.clip(sel) {
                if let Some(track) = tl.track(tid) {
                    for pair in track.clips.windows(2) {
                        if (pair[0].id == sel || pair[1].id == sel)
                            && pair[0].timeline_end_ns().ok() == Some(pair[1].timeline_start_ns)
                        {
                            return Some((tid, TransitionPlacement::BetweenClips { left_clip_id: pair[0].id.0, right_clip_id: pair[1].id.0 }));
                        }
                    }
                }
            }
        }
        for kind in [TrackKind::Video, TrackKind::Overlay] {
            for track in tl.tracks.iter().filter(|t| t.kind == kind) {
                if let Some((l, r, _)) = tl.find_adjacent_pair_near(track.id, self.playback.playhead_ns, u64::MAX) {
                    return Some((track.id, TransitionPlacement::BetweenClips { left_clip_id: l, right_clip_id: r }));
                }
            }
        }
        None
    }

    fn resolve_single_clip_for_transition(&self) -> Option<(TrackId, ClipId)> {
        if let Some(sel) = self.playback.selected_clip_id {
            if let Some((tid, _)) = self.project.timeline.clip(sel) {
                return Some((tid, sel));
            }
        }
        let ph = self.playback.playhead_ns;
        for kind in [TrackKind::Video, TrackKind::Overlay] {
            for track in self.project.timeline.tracks.iter().filter(|t| t.kind == kind) {
                if let Some(clip) = track.clips.iter().find(|c| c.timeline_start_ns <= ph && ph <= c.timeline_start_ns.saturating_add(c.duration_ns)) {
                    return Some((track.id, clip.id));
                }
            }
        }
        for kind in [TrackKind::Video, TrackKind::Overlay] {
            if let Some((tid, clip)) = self.project.timeline.tracks.iter().filter(|t| t.kind == kind).find_map(|t| t.clips.first().map(|c| (t.id, c))) {
                return Some((tid, clip.id));
            }
        }
        None
    }
}
