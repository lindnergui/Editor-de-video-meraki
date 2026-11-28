use crate::components::tooltip::icon_tooltip;
use crate::i18n;
use crate::views::project_hub::{
    format_unix_timestamp_localized, ProjectHubMessage, ProjectHubState,
};
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::tooltip::Position;
use iced::widget::{
    button, column, container, horizontal_space, row, text, text_input, vertical_space,
};
use iced::{Background, Border, Element, Length};
use meraki_core::{AppLanguage, RecentProjectEntry};

pub const GRID_COLUMNS: usize = 3;
pub const CARD_SIZE: f32 = 260.0;
pub const GRID_SPACING: f32 = 20.0;
pub const CARD_RADIUS: f32 = 10.0;

fn new_project_card_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            theme::BG_SURFACE
        } else {
            theme::BG_PANEL
        })),
        text_color: theme::TEXT_PRIMARY,
        border: Border {
            color: if hovered {
                theme::ACCENT
            } else {
                theme::BORDER_SUBTLE
            },
            width: if hovered { 1.5 } else { 1.0 },
            radius: CARD_RADIUS.into(),
        },
        ..button::Style::default()
    }
}

fn project_card_shell_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::BG_SURFACE)),
        text_color: Some(theme::TEXT_PRIMARY),
        border: Border {
            color: theme::BORDER_SUBTLE,
            width: 1.0,
            radius: CARD_RADIUS.into(),
        },
        ..container::Style::default()
    }
}

fn preview_tile_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(theme::BG_PANEL)),
        text_color: theme::TEXT_PRIMARY,
        border: Border {
            color: if hovered {
                theme::ACCENT
            } else {
                theme::BORDER_SUBTLE
            },
            width: if hovered { 1.5 } else { 1.0 },
            radius: theme::RADIUS_PANEL.into(),
        },
        ..button::Style::default()
    }
}

fn accent_circle_badge_style(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::ACCENT)),
        border: Border {
            radius: 24.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// Card quadrado 260x260 inicial da grade para criação direta de um novo projeto.
pub fn new_project_square_card<'a>(lang: AppLanguage) -> Element<'a, ProjectHubMessage> {
    let s = i18n::strings(lang);
    let plus_badge = container(icons::icon(icons::PLUS, 22.0, theme::TEXT_ON_ACCENT))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(accent_circle_badge_style);

    let inner = container(
        column![
            plus_badge,
            text(s.hub_new_project)
                .size(15)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY),
            text(s.hub_new_project_sub)
                .size(12)
                .color(theme::TEXT_SECONDARY)
                .align_x(Horizontal::Center),
        ]
        .spacing(10)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill);

    button(inner)
        .width(Length::Fixed(CARD_SIZE))
        .height(Length::Fixed(CARD_SIZE))
        .padding(16)
        .style(new_project_card_style)
        .on_press(ProjectHubMessage::NewProjectClicked)
        .into()
}

/// Card quadrado 260x260 de projeto recente com prévia superior, metadados e ações no rodapé.
pub fn project_square_card<'a>(
    entry: &'a RecentProjectEntry,
    state: &'a ProjectHubState,
    lang: AppLanguage,
) -> Element<'a, ProjectHubMessage> {
    let s = i18n::strings(lang);
    let is_renaming = state.renaming_path.as_ref() == Some(&entry.path);
    let exists = entry.path.is_file();

    let preview_inner: Element<'a, ProjectHubMessage> = if exists {
        container(icons::icon(icons::TAB_EDIT, 28.0, theme::ACCENT_HOVER))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    } else {
        container(
            column![
                icons::icon(icons::TAB_EDIT, 24.0, theme::TEXT_SECONDARY),
                row![
                    icons::icon(icons::ALERT_CIRCLE, 12.0, theme::WARNING),
                    text(s.hub_missing_badge).size(11).color(theme::WARNING),
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            ]
            .spacing(6)
            .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    };

    let preview_tile = button(preview_inner)
        .width(Length::Fill)
        .height(Length::Fixed(104.0))
        .padding(8)
        .style(preview_tile_style)
        .on_press(ProjectHubMessage::OpenRecent(entry.path.clone()));

    let title_area: Element<'a, ProjectHubMessage> = if is_renaming {
        row![
            text_input(s.hub_rename_placeholder, &state.rename_draft)
                .on_input(ProjectHubMessage::RenameDraftChanged)
                .on_submit(ProjectHubMessage::CommitRename)
                .size(12)
                .padding([4, 6])
                .style(theme::text_input_style),
            button(
                text(s.ok)
                    .size(11)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::TEXT_ON_ACCENT)
                    .wrapping(text::Wrapping::None),
            )
                .padding([4, 8])
                .style(theme::primary_button)
                .on_press(ProjectHubMessage::CommitRename),
            button(text("×").size(12).color(theme::TEXT_SECONDARY))
                .padding([3, 7])
                .style(theme::ghost_button)
                .on_press(ProjectHubMessage::CancelRename),
        ]
        .spacing(4)
        .align_y(Vertical::Center)
        .into()
    } else {
        text(entry.name.as_str())
            .size(14)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY)
            .into()
    };

    let metadata = text(format_unix_timestamp_localized(entry.last_opened_unix_secs, lang))
        .size(11)
        .color(theme::TEXT_SECONDARY);

    let open_btn = icon_tooltip(
        button(
            text(s.open)
                .size(12)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_ON_ACCENT)
                .wrapping(text::Wrapping::None),
        )
            .padding([5, 11])
            .style(theme::primary_button)
            .on_press(ProjectHubMessage::OpenRecent(entry.path.clone())),
        s.hub_open_tip,
        Position::Top,
    );

    let rename_btn = icon_tooltip(
        button(icons::icon(icons::PENCIL, 12.0, theme::TEXT_PRIMARY))
            .padding([5, 7])
            .style(theme::secondary_button)
            .on_press(ProjectHubMessage::StartRename {
                path: entry.path.clone(),
                current_name: entry.name.clone(),
            }),
        s.rename,
        Position::Top,
    );

    let remove_btn = icon_tooltip(
        button(icons::icon(icons::TRASH, 12.0, theme::TEXT_SECONDARY))
            .padding([5, 7])
            .style(theme::secondary_button)
            .on_press(ProjectHubMessage::RequestDeleteProject(entry.path.clone())),
        s.hub_delete_tip,
        Position::Top,
    );

    let footer = row![open_btn, horizontal_space(), rename_btn, remove_btn]
        .spacing(6)
        .align_y(Vertical::Center);

    container(
        column![preview_tile, title_area, metadata, vertical_space(), footer]
            .spacing(8)
            .height(Length::Fill),
    )
    .width(Length::Fixed(CARD_SIZE))
    .height(Length::Fixed(CARD_SIZE))
    .padding(12)
    .style(project_card_shell_style)
    .into()
}
