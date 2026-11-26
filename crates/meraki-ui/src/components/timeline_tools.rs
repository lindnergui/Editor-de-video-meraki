use crate::components::timeline_canvas::TimelineCanvasMessage;
use crate::components::timeline_math::{TimelineTool, TimelineViewState};
use crate::components::tooltip::{action_tooltip, icon_tooltip};
use crate::{icons, theme};
use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::widget::tooltip::Position;
use iced::widget::{button, column, container, horizontal_rule};
use iced::{Color, Element, Length, Point, Size};
use meraki_core::{KeymapProfile, ShortcutAction};

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
