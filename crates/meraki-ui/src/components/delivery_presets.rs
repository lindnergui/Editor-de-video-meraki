use crate::app_delivery::DeliveryMessage;
use crate::components::effects_library::matches_query;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input, Column, Row,
};
use iced::{Element, Length};
use meraki_core::ExportPreset;

/// Metadados visuais de cada cartão de predefinição na aba Entrega.
#[derive(Debug, Clone, Copy)]
pub struct PresetCardInfo {
    pub preset: ExportPreset,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub tag: &'static str,
    pub icon: &'static [u8],
}

/// Catálogo fixo das 6 predefinições oficiais de exportação em grade 3x2.
pub const PRESET_CARDS: [PresetCardInfo; 6] = [
    PresetCardInfo {
        preset: ExportPreset::YouTube,
        title: "YouTube",
        subtitle: "1080p ou 4K, otimizado para web",
        tag: "16:9",
        icon: icons::PRESET_YOUTUBE,
    },
    PresetCardInfo {
        preset: ExportPreset::TikTokReels,
        title: "TikTok & Reels",
        subtitle: "9:16 vertical, alta taxa de bits",
        tag: "9:16",
        icon: icons::PRESET_VERTICAL,
    },
    PresetCardInfo {
        preset: ExportPreset::Instagram,
        title: "Instagram",
        subtitle: "1:1 ou 4:5",
        tag: "1:1",
        icon: icons::PRESET_INSTAGRAM,
    },
    PresetCardInfo {
        preset: ExportPreset::WhatsAppLite,
        title: "WhatsApp (Leve)",
        subtitle: "Arquivo leve, fácil compartilhamento",
        tag: "720p",
        icon: icons::PRESET_WHATSAPP,
    },
    PresetCardInfo {
        preset: ExportPreset::MasterProRes,
        title: "Alta Qualidade (ProRes)",
        subtitle: "Mestre / MOV / ProRes",
        tag: "MOV",
        icon: icons::PRESET_PRORES,
    },
    PresetCardInfo {
        preset: ExportPreset::AudioOnly,
        title: "Apenas Áudio (MP3/WAV)",
        subtitle: "Somente trilha de áudio",
        tag: "ÁUDIO",
        icon: icons::PRESET_AUDIO,
    },
];

fn matches_preset(info: &PresetCardInfo, query: &str) -> bool {
    matches_query(query, info.title, info.subtitle)
        || matches_query(query, info.tag, info.preset.label())
        || matches_query(query, info.preset.description(), "")
}

/// Filtra predefinições por título, subtítulo, tag ou descrição do domínio.
pub fn filtered_presets(query: &str) -> Vec<ExportPreset> {
    PRESET_CARDS
        .iter()
        .filter(|info| matches_preset(info, query))
        .map(|info| info.preset)
        .collect()
}

/// Painel esquerdo (~36% da largura, `#181922`): busca + grade 3x2 de predefinições.
pub fn view<'a>(
    search_query: &'a str,
    active_preset: ExportPreset,
) -> Element<'a, DeliveryMessage> {
    let header = text("PREDEFINIÇÕES DE EXPORTAÇÃO")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let search = row![
        icons::icon(icons::SEARCH, 13.0, theme::TEXT_SECONDARY),
        text_input("Buscar predefinições...", search_query)
            .on_input(DeliveryMessage::PresetSearchChanged)
            .size(12)
            .padding([6, 8])
            .style(theme::text_input_style),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let visible: Vec<&PresetCardInfo> = PRESET_CARDS
        .iter()
        .filter(|info| matches_preset(info, search_query))
        .collect();

    let grid_content: Element<'a, DeliveryMessage> = if visible.is_empty() {
        container(
            text("Nenhuma predefinição encontrada.")
                .size(12)
                .color(theme::TEXT_SECONDARY),
        )
        .width(Length::Fill)
        .padding(16)
        .style(theme::card)
        .into()
    } else {
        let mut grid = Column::new().spacing(10).width(Length::Fill);
        for pair in visible.chunks(2) {
            let mut r = Row::new().spacing(10).width(Length::Fill);
            for info in pair {
                r = r.push(preset_card(info, info.preset == active_preset));
            }
            if pair.len() == 1 {
                r = r.push(horizontal_space().width(Length::Fill));
            }
            grid = grid.push(r);
        }
        grid.into()
    };

    let list_scroll = scrollable(grid_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::dark_scrollable);

    container(
        column![header, search, list_scroll]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::FillPortion(36))
    .height(Length::Fill)
    .padding(14)
    .style(theme::topbar)
    .into()
}

fn preset_card<'a>(info: &PresetCardInfo, selected: bool) -> Element<'a, DeliveryMessage> {
    let icon_color = if selected { theme::ACCENT_HOVER } else { theme::TEXT_SECONDARY };
    let tag_style: fn(&iced::Theme) -> iced::widget::container::Style =
        if selected { theme::badge_accent } else { theme::badge };
    let tag_text_color = if selected { theme::TEXT_ON_ACCENT } else { theme::TEXT_SECONDARY };

    let top_row = row![
        container(icons::icon(info.icon, 20.0, icon_color))
            .width(34)
            .height(34)
            .center_x(34)
            .center_y(34)
            .style(theme::badge),
        horizontal_space(),
        container(text(info.tag).size(10).font(theme::FONT_SEMIBOLD).color(tag_text_color))
            .padding([3, 7])
            .style(tag_style),
    ]
    .align_y(Vertical::Center);

    let content = column![
        top_row,
        text(info.title)
            .size(13)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
        text(info.subtitle)
            .size(11)
            .color(theme::TEXT_SECONDARY),
    ]
    .spacing(6)
    .width(Length::Fill);

    button(content)
        .width(Length::Fill)
        .padding(12)
        .style(theme::media_card_button(selected))
        .on_press(DeliveryMessage::SelectPreset(info.preset))
        .into()
}
