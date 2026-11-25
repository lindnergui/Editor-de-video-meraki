use crate::components::timeline_cache::{hash_timeline_static, DualCanvasCache};
use crate::components::timeline_math::{
    apply_signed_delta, collect_snap_targets, compute_left_trim, compute_right_trim,
    display_tracks_ordered, dx_to_delta_ns, hit_test, snap_time_ns, x_to_time_ns, DisplayTrack,
    HitTarget, TimelineTool, TimelineViewState,
};
use crate::components::timeline_render::{
    build_track_headers, draw_playhead, draw_static_timeline, StaticTimelineSpec,
};
use crate::components::timeline_tools::{
    active_drag_span, build_toolbar, draw_hover_guide, finalize_drag, idle_cursor_interaction,
    wheel_zoom_factor, ActiveDrag,
};
use crate::playback::PlaybackState;
use crate::theme;
use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry};
use iced::widget::{container, row, vertical_rule};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme};
use meraki_core::{ClipId, KeymapProfile, MediaKind, Project, TrackId};

/// Mensagens de interação diretas na régua, ferramentas e Canvas da timeline.
#[derive(Debug, Clone)]
pub enum TimelineCanvasMessage {
    SetTool(TimelineTool),
    SplitAtPlayhead,
    ToggleSnap,
    ZoomIn,
    ZoomOut,
    ZoomAnchored { factor: f32, anchor_ns: u64 },
    Scrub(u64),
    SelectClip(Option<ClipId>),
    MoveClip { clip_id: ClipId, track_id: TrackId, new_start_ns: u64 },
    TrimClip { clip_id: ClipId, new_start_ns: u64, new_source_in_ns: u64, new_duration_ns: u64 },
    SplitClip { clip_id: ClipId, split_at_ns: u64 },
}

/// Barra inferior em largura total com ferramentas, cabeçalhos fixos (`V1`, `A1`, `T1`) e `Canvas`.
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
    profile: KeymapProfile,
) -> Element<'a, TimelineCanvasMessage> {
    view_with_options(project, playback, view_state, profile, false)
}

/// Barra inferior configurável permitindo destacar `T1` no topo (aba Efeitos).
pub fn view_with_options<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
    profile: KeymapProfile,
    overlay_first: bool,
) -> Element<'a, TimelineCanvasMessage> {
    let tracks = display_tracks_ordered(&project.timeline, overlay_first);
    let canvas_widget = Canvas::new(TimelineProgram { project, playback, view_state, overlay_first })
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        row![
            build_toolbar(view_state, profile),
            vertical_rule(1).style(theme::divider),
            build_track_headers(&tracks, overlay_first),
            vertical_rule(1).style(theme::divider),
            canvas_widget,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(theme::base_canvas)
    .into()
}

struct TimelineProgram<'a> {
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
    overlay_first: bool,
}

#[derive(Default)]
struct DragState {
    active: Option<ActiveDrag>,
    modifiers: keyboard::Modifiers,
    hover_x: Option<f32>,
    cache: DualCanvasCache,
}

impl canvas::Program<TimelineCanvasMessage> for TimelineProgram<'_> {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<TimelineCanvasMessage>) {
        if let canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) = event {
            state.modifiers = mods;
            return (canvas::event::Status::Ignored, None);
        }
        let canvas::Event::Mouse(mouse_event) = event else {
            return (canvas::event::Status::Ignored, None);
        };

        let tracks = display_tracks_ordered(&self.project.timeline, self.overlay_first);
        let px = self.view_state.px_per_sec;
        let scroll = self.view_state.scroll_x_px;

        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(local) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                self.handle_press(state, local, &tracks, px, scroll)
            }
            mouse::Event::CursorMoved { .. } => {
                state.hover_x = cursor.position_in(bounds).map(|p| p.x);
                let Some(ref mut drag) = state.active else {
                    return (canvas::event::Status::Ignored, None);
                };
                let local = cursor.position().map_or(Point::ORIGIN, |p| {
                    Point::new((p.x - bounds.x).max(0.0), p.y - bounds.y)
                });
                self.handle_drag_move(drag, local, px, scroll)
            }
            mouse::Event::CursorLeft => {
                state.hover_x = None;
                (canvas::event::Status::Ignored, None)
            }
            mouse::Event::WheelScrolled { delta }
                if (state.modifiers.control() || state.modifiers.command())
                    && cursor.position_in(bounds).is_some() =>
            {
                let msg = wheel_zoom_factor(delta).map(|factor| {
                    TimelineCanvasMessage::ZoomAnchored { factor, anchor_ns: self.playback.playhead_ns }
                });
                let status = if msg.is_some() { canvas::event::Status::Captured } else { canvas::event::Status::Ignored };
                (status, msg)
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                let Some(drag) = state.active.take() else {
                    return (canvas::event::Status::Ignored, None);
                };
                (canvas::event::Status::Captured, finalize_drag(drag))
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let px = self.view_state.px_per_sec;
        let scroll = self.view_state.scroll_x_px;
        let drag_preview = active_drag_span(state.active.as_ref(), &self.project.timeline);
        let static_key = hash_timeline_static(
            self.project,
            self.overlay_first,
            self.playback.selected_clip_id,
            px,
            scroll,
            drag_preview,
        );
        state.cache.draw_layers(
            renderer,
            bounds.size(),
            static_key,
            |frame| {
                draw_static_timeline(
                    frame,
                    StaticTimelineSpec {
                        project: self.project,
                        overlay_first: self.overlay_first,
                        selected_clip_id: self.playback.selected_clip_id,
                        drag_preview,
                        px,
                        scroll,
                        bounds_size: bounds.size(),
                    },
                );
            },
            |frame| {
                draw_hover_guide(frame, state.hover_x, bounds.height);
                draw_playhead(frame, self.playback.playhead_ns, px, scroll, bounds.height);
            },
        )
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(ref drag) = state.active {
            return match drag {
                ActiveDrag::Moving { .. } => mouse::Interaction::Grabbing,
                ActiveDrag::Scrubbing | ActiveDrag::TrimmingLeft { .. } | ActiveDrag::TrimmingRight { .. } => {
                    mouse::Interaction::ResizingHorizontally
                }
            };
        }
        let Some(local) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        idle_cursor_interaction(local, &self.project.timeline, self.overlay_first, self.view_state)
    }
}

impl TimelineProgram<'_> {
    fn handle_press(
        &self,
        state: &mut DragState,
        local: Point,
        tracks: &[DisplayTrack<'_>],
        px: f32,
        scroll: f32,
    ) -> (canvas::event::Status, Option<TimelineCanvasMessage>) {
        match hit_test(local, tracks, px, scroll) {
            HitTarget::Ruler { time_ns } => {
                state.active = Some(ActiveDrag::Scrubbing);
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::Scrub(time_ns)))
            }
            HitTarget::ClipLeftHandle { clip_id, .. }
            | HitTarget::ClipRightHandle { clip_id, .. }
            | HitTarget::ClipBody { clip_id, .. }
                if self.view_state.tool == TimelineTool::Razor =>
            {
                let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
                    return (canvas::event::Status::Ignored, None);
                };
                let raw_ns = x_to_time_ns(local.x, px, scroll);
                let targets = collect_snap_targets(&self.project.timeline, self.playback.playhead_ns, Some(clip_id));
                let snapped = snap_time_ns(raw_ns, &targets, px, self.view_state.snap_enabled);
                let min_split = clip.timeline_start_ns.saturating_add(1);
                let max_split = clip.timeline_start_ns.saturating_add(clip.duration_ns).saturating_sub(1);
                if max_split < min_split {
                    return (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))));
                }
                let split_at_ns = snapped.clamp(min_split, max_split);
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SplitClip { clip_id, split_at_ns }))
            }
            HitTarget::ClipBody { track_id, clip_id } => {
                let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
                    return (canvas::event::Status::Ignored, None);
                };
                state.active = Some(ActiveDrag::Moving {
                    track_id,
                    clip_id,
                    initial_x: local.x,
                    initial_start_ns: clip.timeline_start_ns,
                    preview_start_ns: clip.timeline_start_ns,
                });
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::ClipLeftHandle { clip_id, .. } => {
                let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
                    return (canvas::event::Status::Ignored, None);
                };
                state.active = Some(ActiveDrag::TrimmingLeft {
                    clip: clip.clone(),
                    initial_x: local.x,
                    preview_start_ns: clip.timeline_start_ns,
                    preview_source_in_ns: clip.source_in_ns,
                    preview_duration_ns: clip.duration_ns,
                });
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::ClipRightHandle { clip_id, .. } => {
                let Some((_, clip)) = self.project.timeline.clip(clip_id) else {
                    return (canvas::event::Status::Ignored, None);
                };
                let max_source_ns = self
                    .project
                    .asset(clip.asset_id)
                    .filter(|a| a.kind != MediaKind::Image && a.metadata.duration_ns > 0)
                    .map(|a| a.metadata.duration_ns);
                state.active = Some(ActiveDrag::TrimmingRight {
                    clip: clip.clone(),
                    initial_x: local.x,
                    max_source_ns,
                    preview_duration_ns: clip.duration_ns,
                });
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SelectClip(Some(clip_id))))
            }
            HitTarget::Empty => (canvas::event::Status::Captured, Some(TimelineCanvasMessage::SelectClip(None))),
        }
    }

    fn handle_drag_move(
        &self,
        drag: &mut ActiveDrag,
        local: Point,
        px: f32,
        scroll: f32,
    ) -> (canvas::event::Status, Option<TimelineCanvasMessage>) {
        match drag {
            ActiveDrag::Scrubbing => {
                (canvas::event::Status::Captured, Some(TimelineCanvasMessage::Scrub(x_to_time_ns(local.x, px, scroll))))
            }
            ActiveDrag::Moving { clip_id, initial_x, initial_start_ns, preview_start_ns, .. } => {
                let delta_ns = dx_to_delta_ns(local.x - *initial_x, px);
                let raw_start = apply_signed_delta(*initial_start_ns, delta_ns);
                let targets = collect_snap_targets(&self.project.timeline, self.playback.playhead_ns, Some(*clip_id));
                *preview_start_ns = snap_time_ns(raw_start, &targets, px, self.view_state.snap_enabled);
                (canvas::event::Status::Captured, None)
            }
            ActiveDrag::TrimmingLeft { clip, initial_x, preview_start_ns, preview_source_in_ns, preview_duration_ns } => {
                let (s, src, dur) = compute_left_trim(clip, dx_to_delta_ns(local.x - *initial_x, px));
                *preview_start_ns = s;
                *preview_source_in_ns = src;
                *preview_duration_ns = dur;
                (canvas::event::Status::Captured, None)
            }
            ActiveDrag::TrimmingRight { clip, initial_x, max_source_ns, preview_duration_ns } => {
                *preview_duration_ns = compute_right_trim(clip, dx_to_delta_ns(local.x - *initial_x, px), *max_source_ns);
                (canvas::event::Status::Captured, None)
            }
        }
    }
}
