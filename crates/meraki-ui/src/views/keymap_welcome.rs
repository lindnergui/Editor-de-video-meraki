use crate::i18n;
use crate::{theme, window_icon};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, horizontal_space, row, text};
use iced::{Background, Border, Element, Length};
use meraki_core::{AppLanguage, KeymapProfile, ShortcutAction};

/// Ações emitidas pela tela de boas-vindas e seleção de perfil de atalhos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapWelcomeMessage {
    SelectProfile(KeymapProfile),
    Back,
    Continue,
}

fn profile_card_style(selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if selected {
                theme::BG_SURFACE
            } else {
                theme::BG_PANEL
            })),
            text_color: theme::TEXT_PRIMARY,
            border: Border {
                color: if selected {
                    theme::ACCENT
                } else if hovered {
                    theme::ACCENT_HOVER
                } else {
                    theme::BORDER_SUBTLE
                },
                width: if selected { 2.0 } else { 1.0 },
                radius: theme::RADIUS_PANEL.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Renderiza a tela centralizada 2x2 de escolha do perfil de atalhos sobre `#111218`.
pub fn view<'a>(selected: KeymapProfile, can_go_back: bool, lang: AppLanguage) -> Element<'a, KeymapWelcomeMessage> {
    let s = i18n::strings(lang);
    let header = column![
        row![
            window_icon::brand_icon(32.0),
            text(s.welcome_title)
                .size(26)
                .font(theme::FONT_BOLD)
                .color(theme::TEXT_PRIMARY),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
        text(s.welcome_subtitle)
            .size(14)
            .color(theme::TEXT_SECONDARY)
            .align_x(Horizontal::Center),
    ]
    .spacing(10)
    .align_x(Horizontal::Center);

    let profiles = KeymapProfile::all();
    let row_top = row![
        profile_card(profiles[0], profiles[0] == selected, lang),
        profile_card(profiles[1], profiles[1] == selected, lang),
    ]
    .spacing(14)
    .width(Length::Fill);

    let row_bottom = row![
        profile_card(profiles[2], profiles[2] == selected, lang),
        profile_card(profiles[3], profiles[3] == selected, lang),
    ]
    .spacing(14)
    .width(Length::Fill);

    let grid = column![row_top, row_bottom].spacing(14).width(Length::Fill);

    let continue_label = if can_go_back {
        s.welcome_continue
    } else {
        s.welcome_continue_projects
    };
    let continue_btn = button(
        text(continue_label)
            .size(13)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_ON_ACCENT),
    )
    .padding([10, 24])
    .style(theme::primary_button)
    .on_press(KeymapWelcomeMessage::Continue);

    let footer = if can_go_back {
        let back_btn = button(
            text(s.welcome_back)
                .size(13)
                .font(theme::FONT_MEDIUM)
                .color(theme::TEXT_PRIMARY),
        )
        .padding([10, 20])
        .style(theme::secondary_button)
        .on_press(KeymapWelcomeMessage::Back);
        row![back_btn, horizontal_space(), continue_btn]
    } else {
        row![horizontal_space(), continue_btn]
    }
    .width(Length::Fill)
    .align_y(Vertical::Center);

    let card_shell = container(
        column![header, grid, footer]
            .spacing(26)
            .align_x(Horizontal::Center)
            .width(Length::Fill),
    )
    .max_width(920.0)
    .width(Length::Fill);

    container(card_shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding([28, 32])
        .style(theme::base_canvas)
        .into()
}

fn profile_card<'a>(profile: KeymapProfile, selected: bool, lang: AppLanguage) -> Element<'a, KeymapWelcomeMessage> {
    let s = i18n::strings(lang);
    let badge: Element<'a, KeymapWelcomeMessage> = if selected {
        container(
            text(s.welcome_selected)
                .size(10)
                .font(theme::FONT_BOLD)
                .color(theme::TEXT_ON_ACCENT),
        )
        .padding([3, 8])
        .style(theme::badge_accent)
        .into()
    } else {
        container(text(s.welcome_click_select).size(10).color(theme::TEXT_SECONDARY)).into()
    };

    let head = row![
        text(profile.display_name())
            .size(16)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
        badge,
    ]
    .align_y(Vertical::Center);

    let key_rows = column![
        shortcut_row(s.welcome_action_cut, profile.shortcut_label(ShortcutAction::SplitAtPlayhead)),
        shortcut_row(s.welcome_action_select, profile.shortcut_label(ShortcutAction::ToolSelect)),
        shortcut_row(s.welcome_action_snap, profile.shortcut_label(ShortcutAction::ToggleSnap)),
        shortcut_row(s.welcome_action_play_pause, profile.shortcut_label(ShortcutAction::PlayPause)),
        shortcut_row(s.welcome_action_redo, profile.shortcut_label(ShortcutAction::Redo)),
    ]
    .spacing(6);

    button(
        column![
            head,
            text(i18n::keymap_subtitle(lang, profile)).size(12).color(theme::TEXT_SECONDARY),
            key_rows,
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .width(Length::FillPortion(1))
    .padding(18)
    .style(profile_card_style(selected))
    .on_press(KeymapWelcomeMessage::SelectProfile(profile))
    .into()
}

fn shortcut_row<'a>(action_label: &'a str, key_label: &'a str) -> Element<'a, KeymapWelcomeMessage> {
    row![
        text(action_label).size(12).color(theme::TEXT_SECONDARY),
        horizontal_space(),
        container(
            text(key_label)
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY)
                .wrapping(text::Wrapping::None),
        )
        .width(Length::Shrink)
        .padding([3, 8])
        .style(theme::badge),
    ]
    .align_y(Vertical::Center)
    .into()
}
