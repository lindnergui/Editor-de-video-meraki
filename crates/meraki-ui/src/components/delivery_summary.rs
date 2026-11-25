use crate::app_delivery::{format_file_size, format_hms, CoverSlot, DeliveryMessage, DeliveryState};
use crate::playback::PlaybackState;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, image, progress_bar, row, text, Stack,
};
use iced::{Background, Border, Color, ContentFit, Element, Length, Theme};
use meraki_core::Project;

/// Converte o tamanho estimado em bytes para proporção visual (0.0..=1.0) em escala logarítmica.
pub fn estimated_size_bar_ratio(bytes: u64) -> f32 {
    if bytes == 0 {
        return 0.0;
    }
    let ratio = ((bytes as f64).log10() - 5.0) / 4.5;
    ratio.clamp(0.06, 1.0) as f32
}

/// Painel central (~38% da largura, `#181922`): prévia 16:9, resumo técnico e seleção de capa.
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    state: &'a DeliveryState,
) -> Element<'a, DeliveryMessage> {
    let header = text("VISUALIZAÇÃO DO VÍDEO & INFO")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let duration_ns = project.timeline.total_duration_ns();
    let duration_label = format_hms(duration_ns);
    let viewport = build_preview_box(playback, duration_label.clone(), duration_ns == 0);

    let settings = &state.settings;
    let estimated_bytes = settings.estimate_file_size_bytes(duration_ns);
    let resolution_label = if settings.format.is_audio_only() {
        "Somente Áudio"
    } else {
        settings.resolution.label()
    };
    let fps_label = if settings.format.is_audio_only() {
        "—"
    } else {
        settings.fps.label()
    };

    let info_card = container(
        column![
            row![
                metric_item("Duração", duration_label),
                metric_item("Resolução", resolution_label.to_string()),
            ]
            .spacing(12)
            .width(Length::Fill),
            row![
                metric_item("Taxa de Quadros", fps_label.to_string()),
                metric_item("Tamanho Estimado", format_file_size(estimated_bytes)),
            ]
            .spacing(12)
            .width(Length::Fill),
            column![
                progress_bar(0.0..=1.0, estimated_size_bar_ratio(estimated_bytes))
                    .height(6.0)
                    .style(size_bar_style),
                row![
                    text(settings.format.label()).size(10).color(theme::TEXT_SECONDARY),
                    horizontal_space(),
                    text(format!("Qualidade: {}", settings.quality.label()))
                        .size(10)
                        .color(theme::TEXT_SECONDARY),
                ],
            ]
            .spacing(4),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .padding(12)
    .style(theme::card);

    let cover_header = row![
        text("Escolher Capa (Thumbnail)")
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
        text(
            settings
                .cover_timestamp_ns
                .map_or_else(|| "Nenhuma definida".to_string(), format_hms)
        )
        .size(11)
        .color(theme::TEXT_SECONDARY),
    ]
    .align_y(Vertical::Center);

    let mut cover_row = row![].spacing(8).width(Length::Fill);
    for slot in CoverSlot::ALL {
        let selected = state.selected_cover_slot == Some(slot);
        cover_row = cover_row.push(cover_slot_card(slot, duration_ns, selected));
    }

    container(
        column![header, viewport, info_card, cover_header, cover_row]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::FillPortion(38))
    .height(Length::Fill)
    .padding(14)
    .style(theme::topbar)
    .into()
}

fn build_preview_box<'a>(
    playback: &'a PlaybackState,
    duration_label: String,
    is_empty: bool,
) -> Element<'a, DeliveryMessage> {
    let media: Element<'a, DeliveryMessage> = if let Some(frame) = &playback.current_frame {
        container(
            image(frame.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let placeholder = if is_empty {
            "Timeline sem clipes para pré-visualização"
        } else {
            "Clique em uma capa para pré-visualizar o quadro"
        };
        container(
            column![
                icons::icon(icons::MEDIA_VIDEO, 32.0, theme::TEXT_SECONDARY),
                text(placeholder).size(12).color(theme::TEXT_SECONDARY),
            ]
            .spacing(8)
            .align_x(iced::alignment::Horizontal::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    };

    let duration_badge = container(
        container(
            text(duration_label)
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY),
        )
        .padding([4, 8])
        .style(theme::badge),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(10)
    .align_right(Length::Fill)
    .align_bottom(Length::Fill);

    container(
        Stack::new()
            .push(media)
            .push(duration_badge)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(5))
    .padding(4)
    .style(theme::preview_viewport)
    .into()
}

fn metric_item<'a>(label: &'a str, value: String) -> Element<'a, DeliveryMessage> {
    column![
        text(label).size(10).color(theme::TEXT_SECONDARY),
        text(value)
            .size(13)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
    ]
    .spacing(2)
    .width(Length::Fill)
    .into()
}

fn cover_slot_card<'a>(
    slot: CoverSlot,
    duration_ns: u64,
    selected: bool,
) -> Element<'a, DeliveryMessage> {
    let ts_label = format_hms(slot.timestamp_ns(duration_ns));
    let icon_color = if selected { theme::ACCENT_HOVER } else { theme::TEXT_SECONDARY };
    let content = column![
        container(icons::icon(icons::MEDIA_IMAGE, 16.0, icon_color))
            .width(Length::Fill)
            .height(26)
            .center_x(Length::Fill)
            .center_y(26)
            .style(theme::preview_viewport),
        row![
            text(slot.label())
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY),
            horizontal_space(),
            text(ts_label).size(10).color(theme::TEXT_SECONDARY),
        ]
        .align_y(Vertical::Center),
    ]
    .spacing(6);

    button(content)
        .width(Length::Fill)
        .padding(8)
        .style(theme::media_card_button(selected))
        .on_press_maybe((duration_ns > 0).then_some(DeliveryMessage::SelectCoverSlot(slot)))
        .into()
}

fn size_bar_style(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(theme::BG_BASE),
        bar: Background::Color(theme::ACCENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
    }
}
