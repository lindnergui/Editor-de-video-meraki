use crate::app::Message;
use crate::app_screens::ScreenMessage;
use crate::i18n;
use crate::views::project_hub::ProjectHubMessage;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{button, column, container, horizontal_space, opaque, row, stack, text};
use iced::{Background, Border, Color, Element, Length};
use meraki_core::{AppLanguage, RecentProjectEntry};

fn modal_shell_style(_theme: &iced::Theme) -> container::Style {
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
        background: Some(Background::Color(Color::from_rgba8(10, 11, 16, 0.74))),
        ..container::Style::default()
    }
}

fn danger_solid_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb8(215, 58, 58)
        } else {
            theme::DANGER
        })),
        text_color: theme::TEXT_ON_ACCENT,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    }
}

/// Modal centralizado de confirmação para exclusão definitiva de um projeto no Painel de Projetos.
pub fn overlay<'a>(
    base: Element<'a, Message>,
    pending: Option<&'a RecentProjectEntry>,
    lang: AppLanguage,
) -> Element<'a, Message> {
    let Some(entry) = pending else {
        return base;
    };
    let s = i18n::strings(lang);

    let header = row![
        icons::icon(icons::TRASH, 18.0, theme::DANGER),
        text(s.delete_modal_title)
            .size(16)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_PRIMARY),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let description = text(i18n::format_delete_project_desc(lang, &entry.name))
        .size(13)
        .color(theme::TEXT_SECONDARY);

    let cancel_btn = button(
        text(s.delete_modal_cancel)
            .size(12)
            .font(theme::FONT_MEDIUM)
            .color(theme::TEXT_PRIMARY)
            .wrapping(text::Wrapping::None),
    )
    .padding([8, 14])
    .style(theme::secondary_button)
    .on_press(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::CancelDeleteProject,
    )));

    let confirm_btn = button(
        text(s.delete_modal_confirm)
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_ON_ACCENT)
            .wrapping(text::Wrapping::None),
    )
    .padding([8, 16])
    .style(danger_solid_button)
    .on_press(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ConfirmDeleteProject,
    )));

    let actions = row![cancel_btn, horizontal_space(), confirm_btn]
        .spacing(8)
        .align_y(Vertical::Center);

    let modal_card = container(column![header, description, actions].spacing(18))
        .width(Length::Fixed(420.0))
        .padding(22)
        .style(modal_shell_style);

    let overlay = opaque(
        container(modal_card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(backdrop_style),
    );

    stack![base, overlay].into()
}
