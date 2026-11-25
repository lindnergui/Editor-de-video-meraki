use crate::app::Message;
use crate::navigation::WorkspaceTab;
use crate::playback::PlaybackState;
use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{column, container, horizontal_space, row, text, Column, Row};
use iced::{Element, Length};

/// Área principal dinâmica para as abas Áudio e Entrega.
pub fn view<'a>(active_tab: WorkspaceTab, _playback: &'a PlaybackState) -> Element<'a, Message> {
    let header = row![
        column![
            text(active_tab.label()).size(18).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(active_tab.subtitle()).size(12).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2),
        horizontal_space(),
        chip(active_tab.label(), true),
    ]
    .align_y(Vertical::Center);

    let body = match active_tab {
        WorkspaceTab::Files | WorkspaceTab::Edit | WorkspaceTab::Effects => {
            container(text("")).width(Length::Fill).height(Length::Fill).into()
        }
        WorkspaceTab::Audio => audio_workspace(),
        WorkspaceTab::Delivery => delivery_workspace(),
    };

    container(column![header, body].spacing(14).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(theme::base_canvas)
        .into()
}

fn audio_workspace<'a>() -> Element<'a, Message> {
    let controls = panel_box(
        column![
            row![
                section_title("Player Compacto & Voz"),
                horizontal_space(),
                chips_row(&[("Redução de Ruído", true), ("Nivelador de Voz", false)]),
            ]
            .align_y(Vertical::Center),
            chips_row(&[("Voz de Podcast", true), ("Voz Clara", false), ("Voz de Rádio", false)]),
            section_title("Equalizador Visual (4 Bandas)"),
            cards_row(&[
                ("Graves", "+1.5 dB"),
                ("Médios-Graves", "0.0 dB"),
                ("Médios", "+2.0 dB"),
                ("Agudos", "+1.0 dB"),
            ]),
        ]
        .spacing(10)
        .into(),
        Length::FillPortion(7),
        Length::Fill,
    );
    let mixer = panel_box(
        column![section_title("Mixer Vertical"), cards_row(&[("M", "Mudo"), ("S", "Solo"), ("Master", "-1.0 dB")])]
            .spacing(10)
            .into(),
        Length::FillPortion(3),
        Length::Fill,
    );
    let audio_timeline = panel_box(
        column![section_title("Timeline de Áudio Expandida"), timeline_track("A1", "Forma de Onda + Linha de Volume")]
            .spacing(8)
            .into(),
        Length::Fill,
        Length::FillPortion(4),
    );

    split_vertical(row![controls, mixer].spacing(12).into(), 6, audio_timeline)
}

fn delivery_workspace<'a>() -> Element<'a, Message> {
    let presets = panel_box(
        column![
            section_title("Predefinições de Destino"),
            cards_row(&[("YouTube", "1080p • H.264"), ("TikTok & Reels", "9:16 • 60 fps"), ("Instagram", "1080x1350")]),
            cards_row(&[("WhatsApp Leve", "720p • Compacto"), ("Alta Qualidade ProRes", "MOV • 422"), ("Apenas Áudio", "WAV / AAC")]),
        ]
        .spacing(10)
        .into(),
        Length::FillPortion(5),
        Length::Fill,
    );
    let summary = cards_panel(
        "Resumo & Capa",
        &[("Duração / Tamanho", "00:00 • Estimado pronto"), ("Escolha de Capa", "Frame atual da timeline")],
        Length::FillPortion(3),
    );
    let settings = cards_panel(
        "Configurações Finais",
        &[("Formato", "MP4 (H.264 + AAC)"), ("Taxa de Quadros", "30 fps")],
        Length::FillPortion(2),
    );
    let queue = panel_box(
        column![section_title("Fila de Tarefas"), surface_card("Nenhuma exportação ativa", "Pronto para iniciar entrega")]
            .spacing(8)
            .into(),
        Length::Fill,
        Length::FillPortion(3),
    );

    split_vertical(row![presets, summary, settings].spacing(12).into(), 7, queue)
}

fn split_vertical<'a>(top: Element<'a, Message>, top_portion: u16, bottom: Element<'a, Message>) -> Element<'a, Message> {
    column![container(top).width(Length::Fill).height(Length::FillPortion(top_portion)), bottom]
        .spacing(12)
        .height(Length::Fill)
        .into()
}

fn panel_box<'a>(content: Element<'a, Message>, width: Length, height: Length) -> Element<'a, Message> {
    container(content).width(width).height(height).padding(14).style(theme::panel).into()
}

fn cards_panel<'a>(title: &'a str, items: &[(&'a str, &'a str)], width: Length) -> Element<'a, Message> {
    let mut col = Column::new().push(section_title(title)).spacing(8);
    for &(label, detail) in items {
        col = col.push(surface_card(label, detail));
    }
    panel_box(col.into(), width, Length::Fill)
}

fn cards_row<'a>(items: &[(&'a str, &'a str)]) -> Element<'a, Message> {
    Row::with_children(items.iter().map(|&(label, detail)| surface_card(label, detail))).spacing(8).into()
}

fn chips_row<'a>(items: &[(&'a str, bool)]) -> Element<'a, Message> {
    Row::with_children(items.iter().map(|&(label, accent)| chip(label, accent))).spacing(8).align_y(Vertical::Center).into()
}

fn section_title<'a>(title: &'a str) -> Element<'a, Message> {
    text(title).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY).into()
}

fn surface_card<'a>(title: impl text::IntoFragment<'a>, detail: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(
        Column::with_children([
            text(title).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY).into(),
            text(detail).size(11).color(theme::TEXT_SECONDARY).into(),
        ])
        .spacing(3),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card)
    .into()
}

fn chip<'a>(label: impl text::IntoFragment<'a>, accent: bool) -> Element<'a, Message> {
    let (style, text_color): (fn(&iced::Theme) -> iced::widget::container::Style, _) =
        if accent { (theme::badge_accent, theme::TEXT_ON_ACCENT) } else { (theme::badge, theme::TEXT_SECONDARY) };
    container(text(label).size(11).font(theme::FONT_MEDIUM).color(text_color)).padding([5, 10]).style(style).into()
}

fn timeline_track<'a>(track_label: &'a str, clip_name: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    row![
        container(text(track_label).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY))
            .width(36)
            .align_x(Horizontal::Center)
            .padding([6, 4])
            .style(theme::badge),
        container(text(clip_name).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY))
            .width(Length::Fill)
            .padding([8, 12])
            .style(theme::timeline_clip),
    ]
    .spacing(8)
    .align_y(Vertical::Center)
    .into()
}
