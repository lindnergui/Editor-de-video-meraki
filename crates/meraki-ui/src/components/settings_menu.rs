use crate::app::Message;
use crate::app_screens::ScreenMessage;
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, mouse_area, opaque, row, stack,
    text, Row, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding};
use meraki_core::{AppLanguage, KeymapProfile, ProxyMode, ShortcutAction};

/// Contexto dinâmico exibido no menu suspenso de configurações.
#[derive(Debug, Clone, Copy)]
pub struct SettingsMenuContext<'a> {
    pub in_editor: bool,
    pub is_dirty: bool,
    pub profile: KeymapProfile,
    pub language: AppLanguage,
    pub proxy_mode: ProxyMode,
    pub status_notice: Option<&'a str>,
}

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
        border: Border { color: theme::BORDER_SUBTLE, width: 1.0, radius: 10.0.into() },
        ..container::Style::default()
    }
}

fn backdrop_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(10, 11, 16, 0.56))),
        ..container::Style::default()
    }
}

fn segmented_shell_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::BG_SURFACE)),
        text_color: Some(theme::TEXT_PRIMARY),
        border: Border { color: theme::BORDER_SUBTLE, width: 1.0, radius: theme::RADIUS_CONTROL.into() },
        ..container::Style::default()
    }
}

fn segmented_btn_style(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if active {
            if hovered { theme::ACCENT_HOVER } else { theme::ACCENT }
        } else if hovered {
            Color::from_rgba8(255, 255, 255, 0.06)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if active { theme::TEXT_ON_ACCENT } else { theme::TEXT_SECONDARY },
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 4.0.into() },
            ..button::Style::default()
        }
    }
}

fn menu_item_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { theme::BG_SURFACE } else { Color::TRANSPARENT })),
        text_color: theme::TEXT_PRIMARY,
        border: Border {
            color: if hovered { theme::BORDER_SUBTLE } else { Color::TRANSPARENT },
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
    let title_text = text(label)
        .size(13)
        .font(theme::FONT_MEDIUM)
        .color(theme::TEXT_PRIMARY)
        .wrapping(text::Wrapping::None);
    let label_col = if let Some((sub, sub_color)) = subtitle {
        column![title_text, text(sub).size(11).color(sub_color).wrapping(text::Wrapping::None)].spacing(2)
    } else {
        column![title_text]
    };

    let badge = container(
        text(badge_text)
            .size(10)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_SECONDARY)
            .wrapping(text::Wrapping::None),
    )
    .width(Length::Shrink)
    .padding([3, 8])
    .style(theme::badge);

    button(
        row![icons::icon(icon_bytes, 14.0, theme::TEXT_SECONDARY), label_col, horizontal_space(), badge]
            .spacing(10)
            .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(menu_item_style)
    .on_press(Message::Screen(ScreenMessage::SettingsMenu(action)))
    .into()
}

fn segmented_control<'a>(items: Vec<(&'a str, bool, Message)>) -> Element<'a, Message> {
    let buttons = Row::with_children(items.into_iter().map(|(label, active, msg)| {
        let font = if active { theme::FONT_SEMIBOLD } else { theme::FONT_MEDIUM };
        button(
            container(text(label).size(11).font(font).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .align_x(Horizontal::Center),
        )
        .width(Length::FillPortion(1))
        .padding([5, 8])
        .style(segmented_btn_style(active))
        .on_press(msg)
        .into()
    }))
    .spacing(3)
    .width(Length::Fill);

    container(buttons).width(Length::Fill).padding(3).style(segmented_shell_style).into()
}

/// Sobrepõe o painel flutuante de configurações no canto superior direito quando `is_open` é verdadeiro.
pub fn overlay<'a>(
    base: Element<'a, Message>,
    is_open: bool,
    ctx: SettingsMenuContext<'a>,
) -> Element<'a, Message> {
    if !is_open {
        return base;
    }
    let s = i18n::strings(ctx.language);

    let header = row![
        row![
            icons::icon(icons::SETTINGS, 15.0, theme::ACCENT),
            text(s.settings_title).size(14).font(theme::FONT_BOLD).color(theme::TEXT_PRIMARY).wrapping(text::Wrapping::None),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
        horizontal_space(),
        button(text("×").size(14).color(theme::TEXT_SECONDARY))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::Screen(ScreenMessage::SettingsMenu(SettingsMenuMessage::Close))),
    ]
    .align_y(Vertical::Center);

    let project_label = text(s.settings_section_project).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY);
    let project_items = if ctx.in_editor {
        let save_status = if ctx.is_dirty { (s.settings_dirty, theme::ACCENT_HOVER) } else { (s.settings_saved, theme::SUCCESS) };
        column![
            project_label,
            menu_row(icons::SAVE, s.settings_save_now, Some(save_status), ctx.profile.shortcut_label(ShortcutAction::SaveProject), SettingsMenuMessage::SaveNow),
            menu_row(icons::FOLDER_OPEN, s.settings_open_external, None, ctx.profile.shortcut_label(ShortcutAction::OpenProject), SettingsMenuMessage::OpenExternal),
            menu_row(icons::EXPORT, s.settings_export_copy, None, "Ctrl+Shift+S", SettingsMenuMessage::ExportCopy),
        ]
        .spacing(4)
    } else {
        column![
            project_label,
            menu_row(icons::FOLDER_OPEN, s.settings_open_external, None, ctx.profile.shortcut_label(ShortcutAction::OpenProject), SettingsMenuMessage::OpenExternal),
        ]
        .spacing(4)
    };

    let lang_toggle = segmented_control(vec![
        (s.settings_lang_pt, ctx.language == AppLanguage::PtBr, Message::SetLanguage(AppLanguage::PtBr)),
        (s.settings_lang_en, ctx.language == AppLanguage::EnUs, Message::SetLanguage(AppLanguage::EnUs)),
    ]);
    let lang_block = column![
        text(s.settings_language_label).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
        lang_toggle,
    ]
    .spacing(6);

    let prefs_section = column![
        text(s.settings_section_prefs).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY),
        container(lang_block).padding([4, 10]),
        menu_row(icons::KEYBOARD, s.settings_shortcuts, None, ctx.profile.display_name(), SettingsMenuMessage::OpenShortcuts),
    ]
    .spacing(6);

    let proxy_toggle = segmented_control(vec![
        (s.settings_proxy_auto, ctx.proxy_mode == ProxyMode::AutoAbove1080p, Message::SetProxyMode(ProxyMode::AutoAbove1080p)),
        (s.settings_proxy_always, ctx.proxy_mode == ProxyMode::Always, Message::SetProxyMode(ProxyMode::Always)),
        (s.settings_proxy_off, ctx.proxy_mode == ProxyMode::Off, Message::SetProxyMode(ProxyMode::Off)),
    ]);
    let clear_cache_btn = button(
        container(text(s.settings_clear_proxy_cache).size(11).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
            .width(Length::Fill)
            .align_x(Horizontal::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(theme::secondary_button)
    .on_press(Message::ClearProxyCache);

    let mut proxy_section = column![
        text(s.settings_section_proxy).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY),
        text(s.settings_proxy_mode_label).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
        proxy_toggle,
        text(s.settings_proxy_hint).size(11).color(theme::TEXT_SECONDARY),
        clear_cache_btn,
    ]
    .spacing(6);
    if let Some(notice) = ctx.status_notice {
        proxy_section = proxy_section.push(text(notice).size(11).color(theme::SUCCESS));
    }

    let card = opaque(
        container(
            column![
                header,
                horizontal_rule(1).style(theme::divider),
                project_items,
                horizontal_rule(1).style(theme::divider),
                prefs_section,
                horizontal_rule(1).style(theme::divider),
                proxy_section,
            ]
            .spacing(10),
        )
        .width(Length::Fixed(360.0))
        .padding(16)
        .style(card_shell_style),
    );

    let backdrop = mouse_area(
        container(Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(backdrop_style),
    )
    .on_press(Message::Screen(ScreenMessage::SettingsMenu(SettingsMenuMessage::Close)));

    let card_anchor = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top)
        .padding(Padding { top: 58.0, right: 16.0, bottom: 16.0, left: 16.0 });

    stack![base, backdrop, card_anchor].into()
}
