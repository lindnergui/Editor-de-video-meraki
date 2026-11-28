use crate::app_delivery::{DeliveryMessage, DeliveryState};
use crate::components::{delivery_config, delivery_presets, delivery_queue, delivery_summary};
use crate::playback::PlaybackState;
use crate::theme;
use iced::widget::{column, container, horizontal_rule, row, vertical_rule};
use iced::{Element, Length};
use meraki_core::{AppLanguage, Project};

/// Renderiza a aba Entrega completa:
/// Topo: Predefinições (`~36%`) | Resumo & Capa (`~38%`) | Configurações (`280px`)
/// Base: Fila de Render & Exportados Recentemente (`~24%` da altura).
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    state: &'a DeliveryState,
    lang: AppLanguage,
) -> Element<'a, DeliveryMessage> {
    let presets_panel = delivery_presets::view(&state.preset_search, state.settings.preset, lang);
    let summary_panel = delivery_summary::view(project, playback, state, lang);
    let config_panel = delivery_config::view(project, state, lang);

    let top_row = container(
        row![
            presets_panel,
            vertical_rule(1).style(theme::divider),
            summary_panel,
            vertical_rule(1).style(theme::divider),
            config_panel,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(76));

    let bottom_queue = delivery_queue::view(state, lang);

    column![top_row, horizontal_rule(1).style(theme::divider), bottom_queue]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
