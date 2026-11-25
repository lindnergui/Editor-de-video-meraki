use crate::app::Message;
use crate::app_project_io::{ProjectIoMessage, ProjectIoState};
use crate::app_screens::ScreenMessage;
use crate::components::settings_menu::SettingsMenuMessage;
use crate::components::tooltip::icon_tooltip;
use crate::{icons, theme, window_icon};
use iced::alignment::Vertical;
use iced::widget::tooltip::Position;
use iced::widget::{button, container, row, text, text_input};
use iced::{Element, Length};

/// Barra superior minimalista do Editor: marca + Projetos, título editável, Engrenagem e Exportar.
pub fn view<'a>(
    project_name: &'a str,
    project_io: &'a ProjectIoState,
    settings_open: bool,
    is_delivery_active: bool,
) -> Element<'a, Message> {
    let projects_btn = icon_tooltip(
        button(
            row![
                icons::icon(icons::HOME, 13.0, theme::TEXT_SECONDARY),
                text("Projetos")
                    .size(11)
                    .font(theme::FONT_MEDIUM)
                    .color(theme::TEXT_SECONDARY),
            ]
            .spacing(5)
            .align_y(Vertical::Center),
        )
        .padding([5, 9])
        .style(theme::ghost_button)
        .on_press(Message::Screen(ScreenMessage::OpenProjectHub)),
        "Voltar para Projetos",
        Position::Bottom,
    );

    let brand = row![
        window_icon::brand_icon(22.0),
        text("meraki")
            .size(17)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_PRIMARY),
        projects_btn,
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let left_slot = container(brand)
        .align_left(Length::FillPortion(1))
        .center_y(Length::Fill);

    let center_slot = container(project_name_control(project_name, project_io))
        .center_x(Length::FillPortion(1))
        .center_y(Length::Fill);

    let settings_btn = icon_tooltip(
        button(icons::icon(
            icons::SETTINGS,
            15.0,
            if settings_open {
                theme::TEXT_ON_ACCENT
            } else {
                theme::TEXT_PRIMARY
            },
        ))
        .padding([7, 9])
        .style(if settings_open {
            theme::primary_button
        } else {
            theme::secondary_button
        })
        .on_press(Message::Screen(ScreenMessage::SettingsMenu(
            SettingsMenuMessage::Toggle,
        ))),
        "Configurações",
        Position::Bottom,
    );

    let export_content = row![
        icons::icon(icons::EXPORT, 14.0, theme::TEXT_ON_ACCENT),
        text("Exportar")
            .size(13)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_ON_ACCENT),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let export_button = icon_tooltip(
        button(export_content)
            .padding([7, 16])
            .style(if is_delivery_active {
                theme::primary_button
            } else {
                theme::export_button
            })
            .on_press(Message::ExportClicked),
        "Exportar",
        Position::Bottom,
    );

    let right_slot = container(
        row![settings_btn, export_button]
            .spacing(8)
            .align_y(Vertical::Center),
    )
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

    let name_btn = button(
        container(pill_row)
            .padding([5, 14])
            .style(theme::badge),
    )
    .padding(0)
    .style(theme::ghost_button)
    .on_press(Message::ProjectIo(ProjectIoMessage::StartRename));

    icon_tooltip(name_btn, "Renomear Projeto", Position::Bottom)
}
