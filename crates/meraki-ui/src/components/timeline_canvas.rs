use crate::components::timeline_cache::{hash_timeline_static, DualCanvasCache};
use crate::components::timeline_interaction::TimelineInteraction;
use crate::components::timeline_math::{display_tracks_ordered, TimelineTool, TimelineViewState};
use crate::components::timeline_render::{
    build_track_headers, draw_playhead, draw_static_timeline, StaticTimelineSpec,
};
pub use crate::components::timeline_render::{compute_effect_badges, truncate_clip_title, EffectBadge};
pub use crate::components::timeline_transitions::{compute_transition_visuals, hit_test_transition, TransitionVisual};
use crate::components::timeline_tools::{build_toolbar, draw_hover_guide, wheel_zoom_factor};
use crate::playback::PlaybackState;
use crate::theme;
use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry};
use iced::widget::{container, row, vertical_rule};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme};
use meraki_core::{AppLanguage, ClipId, KeymapProfile, Project, TrackId};
use uuid::Uuid;

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
    ScrubEmpty(u64),
    SelectClip(Option<ClipId>),
    SelectTransition(Option<Uuid>),
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
    lang: AppLanguage,
) -> Element<'a, TimelineCanvasMessage> {
    view_with_options(project, playback, view_state, profile, lang, false)
}

/// Barra inferior configurável permitindo destacar `T1` no topo (aba Efeitos).
pub fn view_with_options<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
    profile: KeymapProfile,
    lang: AppLanguage,
    overlay_first: bool,
) -> Element<'a, TimelineCanvasMessage> {
    let tracks = display_tracks_ordered(&project.timeline, overlay_first);
    let canvas_widget = Canvas::new(TimelineProgram { project, playback, view_state, overlay_first, lang })
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        row![
            build_toolbar(view_state, profile, lang),
            vertical_rule(1).style(theme::divider),
            build_track_headers(&tracks, overlay_first, lang),
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
    lang: AppLanguage,
}

#[derive(Default)]
struct DragState {
    interaction: TimelineInteraction,
    modifiers: keyboard::Modifiers,
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

        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(local) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                let (captured, msg) = state.interaction.on_press(
                    local,
                    self.project,
                    self.playback.playhead_ns,
                    self.overlay_first,
                    self.view_state,
                );
                let status = if captured { canvas::event::Status::Captured } else { canvas::event::Status::Ignored };
                (status, msg)
            }
            mouse::Event::CursorMoved { .. } => {
                let inside = cursor.position_in(bounds);
                let raw_local = cursor.position().map_or(Point::ORIGIN, |p| {
                    Point::new((p.x - bounds.x).max(0.0), p.y - bounds.y)
                });
                let (captured, msg) = state.interaction.on_move(
                    inside,
                    raw_local,
                    self.project,
                    self.playback.playhead_ns,
                    self.overlay_first,
                    self.view_state,
                );
                let status = if captured { canvas::event::Status::Captured } else { canvas::event::Status::Ignored };
                (status, msg)
            }
            mouse::Event::CursorLeft => {
                state.interaction.hover_x = None;
                if state.interaction.active.is_none() {
                    state.interaction.hide_hover_guide = false;
                }
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
                if state.interaction.active.is_none() {
                    return (canvas::event::Status::Ignored, None);
                }
                (canvas::event::Status::Captured, state.interaction.on_release())
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
        let drag_preview = state.interaction.active_drag_span(&self.project.timeline);
        let trim_limit = state.interaction.active_trim_limit();
        let static_key = hash_timeline_static(
            self.project,
            self.overlay_first,
            self.playback.selected_clip_id,
            self.playback.selected_transition_id,
            px,
            scroll,
            drag_preview,
            trim_limit,
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
                        selected_transition_id: self.playback.selected_transition_id,
                        drag_preview,
                        trim_limit,
                        px,
                        scroll,
                        bounds_size: bounds.size(),
                        lang: self.lang,
                    },
                );
            },
            |frame| {
                draw_hover_guide(frame, state.interaction.visible_hover_x(), bounds.height);
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
        state.interaction.mouse_interaction(
            cursor.position_in(bounds),
            &self.project.timeline,
            self.overlay_first,
            self.view_state,
        )
    }
}
