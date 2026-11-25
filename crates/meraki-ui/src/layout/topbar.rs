use crate::app::Message;
use crate::app_project_io::{ProjectIoMessage, ProjectIoState};
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{button, container, row, text, text_input};
use iced::{Element, Length};

/// Barra superior minimalista com identidade, ações rápidas Abrir/Salvar, título editável e Exportar.
pub fn view<'a>(
    project_name: &'a str,
    project_io: &'a ProjectIoState,
    is_delivery_active: bool,
) -> Element<'a, Message> {
    let brand = row![
        icons::icon(icons::MERAKI_MARK, 20.0, theme::ACCENT),
        text("meraki")
            .size(17)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_PRIMARY),
        topbar_file_button(icons::FOLDER_OPEN, "Abrir", ProjectIoMessage::Open),
        topbar_file_button(icons::SAVE, "Salvar", ProjectIoMessage::Save),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let left_slot = container(brand)
        .align_left(Length::FillPortion(1))
        .center_y(Length::Fill);

    let center_slot = container(project_name_control(project_name, project_io))
        .center_x(Length::FillPortion(1))
        .center_y(Length::Fill);

    let export_content = row![
        icons::icon(icons::EXPORT, 14.0, theme::TEXT_ON_ACCENT),
        text("Exportar")
            .size(13)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_ON_ACCENT),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let export_button = button(export_content)
        .padding([7, 16])
        .style(if is_delivery_active {
            theme::primary_button
        } else {
            theme::export_button
        })
        .on_press(Message::ExportClicked);

    let right_slot = container(export_button)
        .align_right(Length::FillPortion(1))
        .center_y(Length::Fill);

    container(
        row![left_slot, center_slot, right_slot]
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(theme::TOPBAR_HEIGHT))
    .padding([0, 16])
    .style(theme::topbar)
    .into()
}

fn topbar_file_button<'a>(
    icon_bytes: &'static [u8],
    label: &'a str,
    msg: ProjectIoMessage,
) -> Element<'a, Message> {
    button(
        row![
            icons::icon(icon_bytes, 13.0, theme::TEXT_SECONDARY),
            text(label)
                .size(11)
                .font(theme::FONT_MEDIUM)
                .color(theme::TEXT_SECONDARY),
        ]
        .spacing(5)
        .align_y(Vertical::Center),
    )
    .padding([5, 9])
    .style(theme::ghost_button)
    .on_press(Message::ProjectIo(msg))
    .into()
}

fn project_name_control<'a>(
    project_name: &'a str,
    project_io: &'a ProjectIoState,
) -> Element<'a, Message> {
    if project_io.editing_name {
        let input = text_input("Nome do projeto...", &project_io.name_draft)
            .on_input(|val| Message::ProjectIo(ProjectIoMessage::NameDraftChanged(val)))
            .on_submit(Message::ProjectIo(ProjectIoMessage::CommitRename))
            .width(Length::Fixed(220.0))
            .padding([5, 10])
            .size(13)
            .style(theme::text_input_style);

        let confirm_btn = button(
            text("OK")
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_ON_ACCENT),
        )
        .padding([5, 10])
        .style(theme::primary_button)
        .on_press(Message::ProjectIo(ProjectIoMessage::CommitRename));

        let cancel_btn = button(text("×").size(12).color(theme::TEXT_SECONDARY))
            .padding([4, 8])
            .style(theme::ghost_button)
            .on_press(Message::ProjectIo(ProjectIoMessage::CancelRename));

        return row![input, confirm_btn, cancel_btn]
            .spacing(4)
            .align_y(Vertical::Center)
            .into();
    }

    let mut pill_row = row![
        text(project_name)
            .size(13)
            .font(theme::FONT_MEDIUM)
            .color(theme::TEXT_PRIMARY),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    if project_io.is_dirty {
        pill_row = pill_row.push(text("●").size(10).color(theme::ACCENT));
    }

    button(
        container(pill_row)
            .padding([5, 14])
            .style(theme::badge),
    )
    .padding(0)
    .style(theme::ghost_button)
    .on_press(Message::ProjectIo(ProjectIoMessage::StartRename))
    .into()
}
