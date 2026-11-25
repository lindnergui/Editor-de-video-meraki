use crate::components::timeline_canvas::TimelineCanvasMessage;
use crate::components::timeline_math::{
    display_tracks_ordered, hit_test, HitTarget, TimelineTool, TimelineViewState,
};
use crate::components::tooltip::{action_tooltip, icon_tooltip};
use crate::{icons, theme};
use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::widget::tooltip::Position;
use iced::widget::{button, column, container, horizontal_rule};
use iced::{Color, Element, Length, Point, Size};
use meraki_core::{Clip, ClipId, KeymapProfile, ShortcutAction, Timeline, TrackId};

pub(crate) enum ActiveDrag {
    Scrubbing,
    Moving {
        track_id: TrackId,
        clip_id: ClipId,
        initial_x: f32,
        initial_start_ns: u64,
        preview_start_ns: u64,
    },
    TrimmingLeft {
        clip: Clip,
        initial_x: f32,
        preview_start_ns: u64,
        preview_source_in_ns: u64,
        preview_duration_ns: u64,
    },
    TrimmingRight {
        clip: Clip,
        initial_x: f32,
        max_source_ns: Option<u64>,
        preview_duration_ns: u64,
    },
}

pub(crate) fn active_drag_span(
    active: Option<&ActiveDrag>,
    timeline: &Timeline,
) -> Option<(ClipId, u64, u64)> {
    match active {
        Some(ActiveDrag::Moving { clip_id, preview_start_ns, .. }) => {
            timeline.clip(*clip_id).map(|(_, c)| (*clip_id, *preview_start_ns, c.duration_ns))
        }
        Some(ActiveDrag::TrimmingLeft { clip, preview_start_ns, preview_duration_ns, .. }) => {
            Some((clip.id, *preview_start_ns, *preview_duration_ns))
        }
        Some(ActiveDrag::TrimmingRight { clip, preview_duration_ns, .. }) => {
            Some((clip.id, clip.timeline_start_ns, *preview_duration_ns))
        }
        Some(ActiveDrag::Scrubbing) | None => None,
    }
}

pub(crate) fn finalize_drag(drag: ActiveDrag) -> Option<TimelineCanvasMessage> {
    match drag {
        ActiveDrag::Scrubbing => None,
        ActiveDrag::Moving { track_id, clip_id, initial_start_ns, preview_start_ns, .. } => {
            (preview_start_ns != initial_start_ns).then_some(TimelineCanvasMessage::MoveClip {
                clip_id,
                track_id,
                new_start_ns: preview_start_ns,
            })
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

/// Renderiza a barra de ferramentas lateral esquerda da timeline com tooltips dinâmicos.
pub(crate) fn build_toolbar<'a>(
    view_state: &TimelineViewState,
    profile: KeymapProfile,
) -> Element<'a, TimelineCanvasMessage> {
    let select_active = view_state.tool == TimelineTool::Select;

    let select_btn = action_tooltip(
        tool_btn(
            icons::TOOL_SELECT,
            select_active,
            TimelineCanvasMessage::SetTool(TimelineTool::Select),
        ),
        "Selecionar",
        profile,
        ShortcutAction::ToolSelect,
        Position::Right,
    );

    let split_btn = action_tooltip(
        tool_btn(
            icons::TOOL_RAZOR,
            false,
            TimelineCanvasMessage::SplitAtPlayhead,
        ),
        "Cortar na Agulha",
        profile,
        ShortcutAction::SplitAtPlayhead,
        Position::Right,
    );

    let snap_btn = action_tooltip(
        tool_btn(
            icons::TOOL_SNAP,
            view_state.snap_enabled,
            TimelineCanvasMessage::ToggleSnap,
        ),
        "Imã / Snap",
        profile,
        ShortcutAction::ToggleSnap,
        Position::Right,
    );

    let zoom_in_btn = icon_tooltip(
        tool_btn(icons::ZOOM_IN, false, TimelineCanvasMessage::ZoomIn),
        "Aumentar Zoom",
        Position::Right,
    );

    let zoom_out_btn = icon_tooltip(
        tool_btn(icons::ZOOM_OUT, false, TimelineCanvasMessage::ZoomOut),
        "Diminuir Zoom",
        Position::Right,
    );

    container(
        column![
            select_btn,
            split_btn,
            snap_btn,
            horizontal_rule(1).style(theme::divider),
            zoom_in_btn,
            zoom_out_btn,
        ]
        .spacing(6)
        .align_x(Horizontal::Center),
    )
    .width(42)
    .height(Length::Fill)
    .padding([8, 4])
    .style(theme::sidebar)
    .into()
}

fn tool_btn<'a>(
    svg_bytes: &'static [u8],
    active: bool,
    msg: TimelineCanvasMessage,
) -> Element<'a, TimelineCanvasMessage> {
    let tint = if active {
        theme::TEXT_ON_ACCENT
    } else {
        theme::TEXT_SECONDARY
    };
    button(icons::icon(svg_bytes, 14.0, tint))
        .padding(7)
        .style(theme::filter_pill_button(active))
        .on_press(msg)
        .into()
}

/// Linha guia vertical discreta exibida sob o cursor para referência visual na timeline.
pub(crate) fn draw_hover_guide(frame: &mut Frame, hover_x: Option<f32>, height: f32) {
    let Some(x) = hover_x.filter(|v| v.is_finite() && *v >= 0.0) else {
        return;
    };
    let line = Path::rectangle(Point::new(x, 0.0), Size::new(1.0, height));
    frame.stroke(
        &line,
        Stroke::default()
            .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.14))
            .with_width(1.0),
    );
}

/// Converte rolagem vertical do mouse (`Ctrl+Scroll`) em fator de zoom (`1.25` ou `0.8`).
pub(crate) fn wheel_zoom_factor(delta: mouse::ScrollDelta) -> Option<f32> {
    let y = match delta {
        mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
    };
    if !y.is_finite() || y == 0.0 {
        return None;
    }
    Some(if y > 0.0 { 1.25 } else { 0.8 })
}

/// Cursor interativo quando não há operação de arraste ativa na timeline.
pub(crate) fn idle_cursor_interaction(
    local: Point,
    timeline: &Timeline,
    overlay_first: bool,
    view_state: &TimelineViewState,
) -> mouse::Interaction {
    let tracks = display_tracks_ordered(timeline, overlay_first);
    match hit_test(local, &tracks, view_state.px_per_sec, view_state.scroll_x_px) {
        HitTarget::Ruler { .. } => mouse::Interaction::Pointer,
        HitTarget::ClipLeftHandle { .. } | HitTarget::ClipRightHandle { .. }
            if view_state.tool == TimelineTool::Select =>
        {
            mouse::Interaction::ResizingHorizontally
        }
        HitTarget::ClipBody { .. }
        | HitTarget::ClipLeftHandle { .. }
        | HitTarget::ClipRightHandle { .. } => {
            if view_state.tool == TimelineTool::Razor {
                mouse::Interaction::Crosshair
            } else {
                mouse::Interaction::Grab
            }
        }
        HitTarget::Empty => mouse::Interaction::default(),
    }
}
