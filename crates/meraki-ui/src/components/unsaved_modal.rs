use crate::app::Message;
use crate::app_exit_guard::{ExitGuardMessage, PendingExitAction};
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, opaque, row, stack, text,
};
use iced::{Background, Border, Color, Element, Length};
use meraki_core::AppLanguage;

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

fn danger_subtle_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb8(48, 28, 34)
        } else {
            theme::BG_SURFACE
        })),
        text_color: theme::DANGER,
        border: Border {
            color: theme::DANGER,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    }
}

/// Sobrepõe o modal centralizado de confirmação de saída quando há alterações não salvas.
pub fn overlay<'a>(
    base: Element<'a, Message>,
    project_name: &'a str,
    pending: Option<&PendingExitAction>,
    lang: AppLanguage,
) -> Element<'a, Message> {
    if pending.is_none() {
        return base;
    }
    let s = i18n::strings(lang);

    let header = row![
        icons::icon(icons::ALERT_CIRCLE, 18.0, theme::WARNING),
        text(s.unsaved_title)
            .size(16)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_PRIMARY),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let description = text(i18n::format_unsaved_desc(lang, project_name))
        .size(13)
        .color(theme::TEXT_SECONDARY);

    let cancel_btn = button(
        text(s.unsaved_cancel)
            .size(12)
            .font(theme::FONT_MEDIUM)
            .color(theme::TEXT_PRIMARY)
            .wrapping(text::Wrapping::None),
    )
    .padding([8, 12])
    .style(theme::secondary_button)
    .on_press(Message::ExitGuard(ExitGuardMessage::Cancel));

    let discard_btn = button(
        text(s.unsaved_discard)
            .size(12)
            .font(theme::FONT_MEDIUM)
            .color(theme::DANGER)
            .wrapping(text::Wrapping::None),
    )
    .padding([8, 12])
    .style(danger_subtle_button)
    .on_press(Message::ExitGuard(ExitGuardMessage::DiscardAndContinue));

    let save_btn = button(
        text(s.unsaved_save)
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_ON_ACCENT)
            .wrapping(text::Wrapping::None),
    )
    .padding([8, 12])
    .style(theme::primary_button)
    .on_press(Message::ExitGuard(ExitGuardMessage::SaveAndContinue));

    let actions = row![cancel_btn, horizontal_space(), discard_btn, save_btn]
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
