use crate::components::timeline_canvas::TimelineCanvasMessage;
use crate::components::timeline_math::{
    apply_signed_delta, collect_snap_targets, compute_left_trim, compute_right_trim,
    display_tracks_ordered, dx_to_delta_ns, hit_test, snap_time_ns, x_to_time_ns, HitTarget,
    TimelineTool, TimelineViewState,
};
use crate::components::timeline_transitions::hit_test_transition;
use iced::mouse;
use iced::Point;
use meraki_core::{Clip, ClipId, MediaKind, Project, Timeline, TrackId};

/// Movimento horizontal mínimo (px) para diferenciar arraste de clipe de um clique simples.
pub const CLIP_DRAG_THRESHOLD_PX: f32 = 4.0;

#[derive(Debug, Clone)]
pub enum ActiveDrag {
    Scrubbing,
    Moving {
        track_id: TrackId,
        clip_id: ClipId,
        initial_x: f32,
        click_time_ns: u64,
        initial_start_ns: u64,
        preview_start_ns: u64,
        has_dragged: bool,
    },
    TrimmingLeft {
        clip: Clip,
        initial_x: f32,
        min_start_ns: u64,
        preview_start_ns: u64,
        preview_source_in_ns: u64,
        preview_duration_ns: u64,
    },
    TrimmingRight {
        clip: Clip,
        initial_x: f32,
        max_source_ns: Option<u64>,
        max_end_ns: Option<u64>,
        preview_duration_ns: u64,
    },
}

/// Estado puro da interação de ponteiro na timeline (clique, scrub, mover e aparar bordas).
#[derive(Debug, Clone, Default)]
pub struct TimelineInteraction {
    pub active: Option<ActiveDrag>,
    pub hover_x: Option<f32>,
    pub hide_hover_guide: bool,
}

/// Resolve a duração máxima real da mídia original combinando `asset.metadata.duration_ns` e `clip.media_duration_ns`.
pub fn resolve_max_source_ns(project: &Project, clip: &Clip) -> Option<u64> {
    if clip.title.is_some() {
        return None;
    }
    if let Some(asset) = project.asset(clip.asset_id) {
        if asset.kind == MediaKind::Image {
            return None;
        }
        let m = asset.metadata.duration_ns.max(clip.media_duration_ns);
        return (m > 0).then_some(m);
    }
    (clip.media_duration_ns > 0).then_some(clip.media_duration_ns)
}

fn neighbor_bounds(timeline: &Timeline, track_id: TrackId, clip_id: ClipId) -> (u64, Option<u64>) {
    let Some(track) = timeline.track(track_id) else {
        return (0, None);
    };
    let Some(idx) = track.clips.iter().position(|c| c.id == clip_id) else {
        return (0, None);
    };
    let min_start = idx
        .checked_sub(1)
        .and_then(|i| track.clips.get(i))
        .and_then(|c| c.timeline_end_ns().ok())
        .unwrap_or(0);
    let max_end = track.clips.get(idx + 1).map(|c| c.timeline_start_ns);
    (min_start, max_end)
}

impl TimelineInteraction {
    pub fn visible_hover_x(&self) -> Option<f32> {
        if self.hide_hover_guide { None } else { self.hover_x }
    }

    pub fn active_drag_span(&self, timeline: &Timeline) -> Option<(ClipId, u64, u64)> {
        match self.active.as_ref() {
            Some(ActiveDrag::Moving { clip_id, preview_start_ns, has_dragged: true, .. }) => {
                timeline.clip(*clip_id).map(|(_, c)| (*clip_id, *preview_start_ns, c.duration_ns))
            }
            Some(ActiveDrag::TrimmingLeft { clip, preview_start_ns, preview_duration_ns, .. }) => {
                Some((clip.id, *preview_start_ns, *preview_duration_ns))
            }
            Some(ActiveDrag::TrimmingRight { clip, preview_duration_ns, .. }) => {
                Some((clip.id, clip.timeline_start_ns, *preview_duration_ns))
            }
            _ => None,
        }
    }

    /// Indica quando o trim ativo atingiu exatamente o início (`0`) ou o fim (`media_duration_ns`) da mídia.
    pub fn active_trim_limit(&self) -> Option<(ClipId, bool, bool)> {
        match self.active.as_ref() {
            Some(ActiveDrag::TrimmingLeft { clip, preview_source_in_ns, .. }) if *preview_source_in_ns == 0 => {
                Some((clip.id, true, false))
            }
            Some(ActiveDrag::TrimmingRight { clip, max_source_ns: Some(max_src), preview_duration_ns, .. })
                if clip.source_in_ns.saturating_add(*preview_duration_ns) >= *max_src =>
            {
                Some((clip.id, false, true))
            }
            _ => None,
        }
    }

    pub fn on_press(
        &mut self,
        local: Point,
        project: &Project,
        playhead_ns: u64,
        overlay_first: bool,
        view_state: &TimelineViewState,
    ) -> (bool, Option<TimelineCanvasMessage>) {
        let tracks = display_tracks_ordered(&project.timeline, overlay_first);
        let px = view_state.px_per_sec;
        let scroll = view_state.scroll_x_px;

        if view_state.tool == TimelineTool::Select && local.y >= crate::components::timeline_math::RULER_HEIGHT {
            if let Some(tr_id) = hit_test_transition(local, &project.timeline, overlay_first, px, scroll) {
                self.active = None;
                self.hide_hover_guide = false;
                return (true, Some(TimelineCanvasMessage::SelectTransition(Some(tr_id))));
            }
        }

        match hit_test(local, &tracks, px, scroll) {
            HitTarget::Ruler { time_ns } => {
                self.hide_hover_guide = false;
                self.active = Some(ActiveDrag::Scrubbing);
                (true, Some(TimelineCanvasMessage::Scrub(time_ns)))
            }
            HitTarget::ClipLeftHandle { clip_id, .. }
            | HitTarget::ClipRightHandle { clip_id, .. }
            | HitTarget::ClipBody { clip_id, .. }
                if view_state.tool == TimelineTool::Razor =>
            {
                let Some((_, clip)) = project.timeline.clip(clip_id) else {
                    return (false, None);
                };
                let raw_ns = x_to_time_ns(local.x, px, scroll);
                let targets = collect_snap_targets(&project.timeline, playhead_ns, Some(clip_id));
                let snapped = snap_time_ns(raw_ns, &targets, px, view_state.snap_enabled);
                let min_split = clip.timeline_start_ns.saturating_add(1);
                let max_split = clip.timeline_start_ns.saturating_add(clip.duration_ns).saturating_sub(1);
                if max_split < min_split {
                    return (true, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))));
                }
                let split_at_ns = snapped.clamp(min_split, max_split);
                (true, Some(TimelineCanvasMessage::SplitClip { clip_id, split_at_ns }))
            }
            HitTarget::ClipBody { track_id, clip_id } => {
                let Some((_, clip)) = project.timeline.clip(clip_id) else {
                    return (false, None);
                };
                self.hide_hover_guide = false;
                self.active = Some(ActiveDrag::Moving {
                    track_id,
                    clip_id,
                    initial_x: local.x,
                    click_time_ns: x_to_time_ns(local.x, px, scroll),
                    initial_start_ns: clip.timeline_start_ns,
                    preview_start_ns: clip.timeline_start_ns,
                    has_dragged: false,
                });
                (true, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::ClipLeftHandle { track_id, clip_id } => {
                let Some((_, clip)) = project.timeline.clip(clip_id) else {
                    return (false, None);
                };
                let (min_start_ns, _) = neighbor_bounds(&project.timeline, track_id, clip_id);
                self.hide_hover_guide = true;
                self.active = Some(ActiveDrag::TrimmingLeft {
                    clip: clip.clone(),
                    initial_x: local.x,
                    min_start_ns,
                    preview_start_ns: clip.timeline_start_ns,
                    preview_source_in_ns: clip.source_in_ns,
                    preview_duration_ns: clip.duration_ns,
                });
                (true, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::ClipRightHandle { track_id, clip_id } => {
                let Some((_, clip)) = project.timeline.clip(clip_id) else {
                    return (false, None);
                };
                let (_, max_end_ns) = neighbor_bounds(&project.timeline, track_id, clip_id);
                let max_source_ns = resolve_max_source_ns(project, clip);
                self.hide_hover_guide = true;
                self.active = Some(ActiveDrag::TrimmingRight {
                    clip: clip.clone(),
                    initial_x: local.x,
                    max_source_ns,
                    max_end_ns,
                    preview_duration_ns: clip.duration_ns,
                });
                (true, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::Empty => {
                self.hide_hover_guide = false;
                self.active = Some(ActiveDrag::Scrubbing);
                (true, Some(TimelineCanvasMessage::ScrubEmpty(x_to_time_ns(local.x, px, scroll))))
            }
        }
    }

    pub fn on_move(
        &mut self,
        inside_pos: Option<Point>,
        raw_local: Point,
        project: &Project,
        playhead_ns: u64,
        overlay_first: bool,
        view_state: &TimelineViewState,
    ) -> (bool, Option<TimelineCanvasMessage>) {
        self.hover_x = inside_pos.map(|p| p.x);
        if let Some(local) = inside_pos {
            let tracks = display_tracks_ordered(&project.timeline, overlay_first);
            let over_trim = matches!(
                hit_test(local, &tracks, view_state.px_per_sec, view_state.scroll_x_px),
                HitTarget::ClipLeftHandle { .. } | HitTarget::ClipRightHandle { .. }
            ) && view_state.tool == TimelineTool::Select;
            self.hide_hover_guide = over_trim
                || matches!(
                    self.active,
                    Some(ActiveDrag::TrimmingLeft { .. } | ActiveDrag::TrimmingRight { .. })
                );
        } else if self.active.is_none() {
            self.hide_hover_guide = false;
        }

        let px = view_state.px_per_sec;
        let scroll = view_state.scroll_x_px;
        let Some(ref mut drag) = self.active else {
            return (false, None);
        };
        match drag {
            ActiveDrag::Scrubbing => {
                (true, Some(TimelineCanvasMessage::Scrub(x_to_time_ns(raw_local.x, px, scroll))))
            }
            ActiveDrag::Moving {
                clip_id, initial_x, initial_start_ns, preview_start_ns, has_dragged, ..
            } => {
                if !*has_dragged && (raw_local.x - *initial_x).abs() <= CLIP_DRAG_THRESHOLD_PX {
                    return (true, None);
                }
                *has_dragged = true;
                let delta_ns = dx_to_delta_ns(raw_local.x - *initial_x, px);
                let raw_start = apply_signed_delta(*initial_start_ns, delta_ns);
                let targets = collect_snap_targets(&project.timeline, playhead_ns, Some(*clip_id));
                *preview_start_ns = snap_time_ns(raw_start, &targets, px, view_state.snap_enabled);
                (true, None)
            }
            ActiveDrag::TrimmingLeft {
                clip, initial_x, min_start_ns, preview_start_ns, preview_source_in_ns, preview_duration_ns,
            } => {
                self.hide_hover_guide = true;
                let (mut s, mut src, mut dur) = compute_left_trim(clip, dx_to_delta_ns(raw_local.x - *initial_x, px));
                if s < *min_start_ns {
                    let bump = *min_start_ns - s;
                    s = *min_start_ns;
                    src = src.saturating_add(bump);
                    dur = dur.saturating_sub(bump);
                }
                *preview_start_ns = s;
                *preview_source_in_ns = src;
                *preview_duration_ns = dur;
                (true, None)
            }
            ActiveDrag::TrimmingRight { clip, initial_x, max_source_ns, max_end_ns, preview_duration_ns } => {
                self.hide_hover_guide = true;
                let mut dur = compute_right_trim(clip, dx_to_delta_ns(raw_local.x - *initial_x, px), *max_source_ns);
                if let Some(limit_end) = *max_end_ns {
                    let max_by_neighbor = limit_end.saturating_sub(clip.timeline_start_ns);
                    dur = dur.min(max_by_neighbor);
                }
                *preview_duration_ns = dur;
                (true, None)
            }
        }
    }

    pub fn on_release(&mut self) -> Option<TimelineCanvasMessage> {
        let drag = self.active.take()?;
        self.hide_hover_guide = false;
        match drag {
            ActiveDrag::Scrubbing => None,
            ActiveDrag::Moving {
                track_id, clip_id, click_time_ns, initial_start_ns, preview_start_ns, has_dragged, ..
            } => {
                if !has_dragged {
                    Some(TimelineCanvasMessage::Scrub(click_time_ns))
                } else {
                    (preview_start_ns != initial_start_ns).then_some(
                        TimelineCanvasMessage::MoveClip { clip_id, track_id, new_start_ns: preview_start_ns },
                    )
                }
            }
            ActiveDrag::TrimmingLeft { clip, preview_start_ns, preview_source_in_ns, preview_duration_ns, .. } => {
                (preview_duration_ns != clip.duration_ns || preview_start_ns != clip.timeline_start_ns)
                    .then_some(TimelineCanvasMessage::TrimClip {
                        clip_id: clip.id,
                        new_start_ns: preview_start_ns,
                        new_source_in_ns: preview_source_in_ns,
                        new_duration_ns: preview_duration_ns,
                    })
            }
            ActiveDrag::TrimmingRight { clip, preview_duration_ns, .. } => {
                (preview_duration_ns != clip.duration_ns).then_some(TimelineCanvasMessage::TrimClip {
                    clip_id: clip.id,
                    new_start_ns: clip.timeline_start_ns,
                    new_source_in_ns: clip.source_in_ns,
                    new_duration_ns: preview_duration_ns,
                })
            }
        }
    }

    pub fn mouse_interaction(
        &self,
        local: Option<Point>,
        timeline: &Timeline,
        overlay_first: bool,
        view_state: &TimelineViewState,
    ) -> mouse::Interaction {
        if let Some(ref drag) = self.active {
            return match drag {
                ActiveDrag::Moving { .. } => mouse::Interaction::Grabbing,
                ActiveDrag::Scrubbing | ActiveDrag::TrimmingLeft { .. } | ActiveDrag::TrimmingRight { .. } => {
                    mouse::Interaction::ResizingHorizontally
                }
            };
        }
        let Some(pos) = local else {
            return mouse::Interaction::default();
        };
        if view_state.tool == TimelineTool::Select
            && hit_test_transition(pos, timeline, overlay_first, view_state.px_per_sec, view_state.scroll_x_px).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        let tracks = display_tracks_ordered(timeline, overlay_first);
        match hit_test(pos, &tracks, view_state.px_per_sec, view_state.scroll_x_px) {
            HitTarget::Ruler { .. } => mouse::Interaction::Pointer,
            HitTarget::ClipLeftHandle { .. } | HitTarget::ClipRightHandle { .. }
                if view_state.tool == TimelineTool::Select =>
            {
                mouse::Interaction::ResizingHorizontally
            }
            HitTarget::ClipBody { .. } | HitTarget::ClipLeftHandle { .. } | HitTarget::ClipRightHandle { .. } => {
                if view_state.tool == TimelineTool::Razor { mouse::Interaction::Crosshair } else { mouse::Interaction::Grab }
            }
            HitTarget::Empty => mouse::Interaction::default(),
        }
    }
}
