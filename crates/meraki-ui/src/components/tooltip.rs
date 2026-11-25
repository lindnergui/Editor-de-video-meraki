use crate::theme;
use iced::widget::tooltip::Position;
use iced::widget::{container, text, tooltip};
use iced::{Background, Border, Element};
use meraki_core::{KeymapProfile, ShortcutAction};

/// Formata o rótulo `"<Nome> (<Atalho>)"` respeitando o `KeymapProfile` ativo.
pub fn format_action_tooltip(name: &str, profile: KeymapProfile, action: ShortcutAction) -> String {
    format!("{name} ({})", profile.shortcut_label(action))
}

fn tooltip_container_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::BG_SURFACE)),
        text_color: Some(theme::TEXT_PRIMARY),
        border: Border {
            color: theme::BORDER_SUBTLE,
            width: theme::BORDER_WIDTH,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    }
}

/// Envolve um botão/ícone com tooltip estilizado no padrão Meraki (`#212330`, borda `#2A2D3D`).
pub fn icon_tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
    position: Position,
) -> Element<'a, Message> {
    let tip = container(
        text(label.into())
            .size(12)
            .font(theme::FONT_REGULAR)
            .color(theme::TEXT_PRIMARY),
    )
    .padding([6, 10])
    .style(tooltip_container_style);

    tooltip(content, tip, position).gap(6.0).into()
}

/// Tooltip para ações com atalho dinâmico no perfil atual (`"<Nome> (<Atalho>)"`).
pub fn action_tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    name: &str,
    profile: KeymapProfile,
    action: ShortcutAction,
    position: Position,
) -> Element<'a, Message> {
    icon_tooltip(content, format_action_tooltip(name, profile, action), position)
}
