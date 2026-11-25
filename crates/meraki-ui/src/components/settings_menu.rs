use crate::app::Message;
use crate::app_screens::ScreenMessage;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, mouse_area, opaque, row, stack,
    text, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding};
use meraki_core::{KeymapProfile, ShortcutAction};
use std::path::Path;

/// Ações do menu suspenso/modal de configurações acessível pela engrenagem na topbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMenuMessage {
    Toggle,
    Close,
    SaveNow,
    OpenExternal,
    ExportCopy,
    OpenShortcuts,
}

fn card_shell_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::BG_PANEL)),
        text_color: Some(theme::TEXT_PRIMARY),
        border: Border {
            color: theme::BORDER_SUBTLE,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    }
}

fn backdrop_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(10, 11, 16, 0.56))),
        ..container::Style::default()
    }
}

fn menu_item_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            theme::BG_SURFACE
        } else {
            Color::TRANSPARENT
        })),
        text_color: theme::TEXT_PRIMARY,
        border: Border {
            color: if hovered {
                theme::BORDER_SUBTLE
            } else {
                Color::TRANSPARENT
            },
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    }
}

fn menu_row<'a>(
    icon_bytes: &'static [u8],
    label: &'a str,
    subtitle: Option<(&'a str, Color)>,
    badge_text: &'a str,
    action: SettingsMenuMessage,
) -> Element<'a, Message> {
    let label_col = if let Some((sub, sub_color)) = subtitle {
        column![
            text(label)
                .size(13)
                .font(theme::FONT_MEDIUM)
                .color(theme::TEXT_PRIMARY),
            text(sub).size(11).color(sub_color),
        ]
        .spacing(2)
    } else {
        column![text(label)
            .size(13)
            .font(theme::FONT_MEDIUM)
            .color(theme::TEXT_PRIMARY)]
    };

    let badge = container(
        text(badge_text)
            .size(10)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_SECONDARY),
    )
    .padding([3, 7])
    .style(theme::badge);

    button(
        row![
            icons::icon(icon_bytes, 14.0, theme::TEXT_SECONDARY),
            label_col,
            horizontal_space(),
            badge,
        ]
        .spacing(10)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([9, 10])
    .style(menu_item_style)
    .on_press(Message::Screen(ScreenMessage::SettingsMenu(action)))
    .into()
}

/// Sobrepõe o painel flutuante de configurações no canto superior direito quando `is_open` é verdadeiro.
pub fn overlay<'a>(
    base: Element<'a, Message>,
    is_open: bool,
    in_editor: bool,
    is_dirty: bool,
    profile: KeymapProfile,
    projects_dir: &Path,
) -> Element<'a, Message> {
    if !is_open {
        return base;
    }

    let header = row![
        row![
            icons::icon(icons::SETTINGS, 15.0, theme::ACCENT),
            text("Configurações")
                .size(14)
                .font(theme::FONT_BOLD)
                .color(theme::TEXT_PRIMARY),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
        horizontal_space(),
        button(text("×").size(14).color(theme::TEXT_SECONDARY))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::Screen(ScreenMessage::SettingsMenu(
                SettingsMenuMessage::Close,
            ))),
    ]
    .align_y(Vertical::Center);

    let project_section_label = text("PROJETO")
        .size(10)
        .font(theme::FONT_SEMIBOLD)
        .color(theme::TEXT_SECONDARY);

    let project_items = if in_editor {
        let save_status = if is_dirty {
            ("Alterações pendentes", theme::ACCENT_HOVER)
        } else {
            ("Salvo", theme::SUCCESS)
        };
        column![
            project_section_label,
            menu_row(
                icons::SAVE,
                "Salvar agora",
                Some(save_status),
                profile.shortcut_label(ShortcutAction::SaveProject),
                SettingsMenuMessage::SaveNow,
            ),
            menu_row(
                icons::FOLDER_OPEN,
                "Abrir arquivo .meraki...",
                None,
                profile.shortcut_label(ShortcutAction::OpenProject),
                SettingsMenuMessage::OpenExternal,
            ),
            menu_row(
                icons::EXPORT,
                "Exportar cópia .meraki...",
                None,
                "Ctrl+Shift+S",
                SettingsMenuMessage::ExportCopy,
            ),
        ]
        .spacing(4)
    } else {
        column![
            project_section_label,
            menu_row(
                icons::FOLDER_OPEN,
                "Abrir arquivo .meraki externo...",
                None,
                profile.shortcut_label(ShortcutAction::OpenProject),
                SettingsMenuMessage::OpenExternal,
            ),
        ]
        .spacing(4)
    };

    let prefs_section = column![
        text("PREFERÊNCIAS")
            .size(10)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_SECONDARY),
        menu_row(
            icons::KEYBOARD,
            "Atalhos de Teclado",
            None,
            profile.display_name(),
            SettingsMenuMessage::OpenShortcuts,
        ),
    ]
    .spacing(4);

    let footer = text(format!("Pasta interna: {}", projects_dir.display()))
        .size(11)
        .color(theme::TEXT_SECONDARY);

    let card = opaque(
        container(
            column![
                header,
                horizontal_rule(1).style(theme::divider),
                project_items,
                horizontal_rule(1).style(theme::divider),
                prefs_section,
                horizontal_rule(1).style(theme::divider),
                footer,
            ]
            .spacing(12),
        )
        .width(Length::Fixed(320.0))
        .padding(16)
        .style(card_shell_style),
    );

    let backdrop = mouse_area(
        container(Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(backdrop_style),
    )
    .on_press(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Close,
    )));

    let card_anchor = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top)
        .padding(Padding {
            top: 58.0,
            right: 16.0,
            bottom: 16.0,
            left: 16.0,
        });

    stack![base, backdrop, card_anchor].into()
}
