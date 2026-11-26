use crate::components::resettable_slider::{resettable_slider, ResettableSliderSpec};
use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, horizontal_rule, horizontal_space, row, scrollable, text};
use iced::{Element, Length};
use meraki_core::{ClipId, ClipTransform, Project};

/// Ações do painel direito Inspetor conectadas ao `UpdateClipTransformCommand`.
#[derive(Debug, Clone)]
pub enum InspectorMessage {
    PreviewTransform(ClipId, ClipTransform),
    CommitTransform(ClipId),
    UpdateTransform(ClipId, ClipTransform),
}

/// Painel direito (`280px`, fundo `#181922`) exibindo os parâmetros de transformação do clipe.
pub fn view<'a>(
    project: &'a Project,
    selected_clip_id: Option<ClipId>,
    transform_draft: Option<(ClipId, ClipTransform)>,
) -> Element<'a, InspectorMessage> {
    let header = text("INSPETOR")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let selected = selected_clip_id.and_then(|id| project.timeline.clip(id).map(|(_, clip)| clip));
    let body: Element<'a, InspectorMessage> = match selected {
        Some(clip) => {
            let clip_id = clip.id;
            let tr = transform_draft
                .filter(|(id, _)| *id == clip_id)
                .map_or(clip.transform, |(_, draft)| draft);
            let defaults = ClipTransform::default();
            let asset_name = project
                .asset(clip.asset_id)
                .map_or("Clipe", |asset| asset.name.as_str());

            let section_bar = row![
                text("Transformar").size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                horizontal_space(),
                button(text("Resetar").size(11).color(theme::TEXT_SECONDARY))
                    .padding([4, 8])
                    .style(theme::ghost_button)
                    .on_press(InspectorMessage::UpdateTransform(clip_id, defaults)),
            ]
            .align_y(Vertical::Center);

            let opacity_pct = (tr.opacity * 100.0).round().clamp(0.0, 100.0);
            let controls = column![
                container(
                    column![
                        text(asset_name).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                        text("Ajustes espaciais e temporais").size(11).color(theme::TEXT_SECONDARY),
                    ]
                    .spacing(2),
                )
                .width(Length::Fill)
                .padding(10)
                .style(theme::card),
                section_bar,
                resettable_slider(ResettableSliderSpec {
                    label: "Escala",
                    value_label: format!("{:.0}%", tr.scale * 100.0),
                    range: 0.10_f32..=3.0_f32,
                    step: 0.05_f32,
                    current: tr.scale,
                    on_change: Box::new(move |scale| {
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { scale, ..tr })
                    }),
                    on_release: InspectorMessage::CommitTransform(clip_id),
                    on_reset: InspectorMessage::UpdateTransform(clip_id, ClipTransform { scale: defaults.scale, ..tr }),
                    trailing: None,
                }),
                resettable_slider(ResettableSliderSpec {
                    label: "Posição X",
                    value_label: format!("{:.0} px", tr.pos_x),
                    range: -960.0_f32..=960.0_f32,
                    step: 1.0_f32,
                    current: tr.pos_x,
                    on_change: Box::new(move |pos_x| {
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_x, ..tr })
                    }),
                    on_release: InspectorMessage::CommitTransform(clip_id),
                    on_reset: InspectorMessage::UpdateTransform(clip_id, ClipTransform { pos_x: defaults.pos_x, ..tr }),
                    trailing: None,
                }),
                resettable_slider(ResettableSliderSpec {
                    label: "Posição Y",
                    value_label: format!("{:.0} px", tr.pos_y),
                    range: -540.0_f32..=540.0_f32,
                    step: 1.0_f32,
                    current: tr.pos_y,
                    on_change: Box::new(move |pos_y| {
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_y, ..tr })
                    }),
                    on_release: InspectorMessage::CommitTransform(clip_id),
                    on_reset: InspectorMessage::UpdateTransform(clip_id, ClipTransform { pos_y: defaults.pos_y, ..tr }),
                    trailing: None,
                }),
                resettable_slider(ResettableSliderSpec {
                    label: "Opacidade",
                    value_label: format!("{opacity_pct:.0}%"),
                    range: 0.0_f32..=100.0_f32,
                    step: 1.0_f32,
                    current: opacity_pct,
                    on_change: Box::new(move |pct| {
                        let opacity = (pct / 100.0).clamp(0.0, 1.0);
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { opacity, ..tr })
                    }),
                    on_release: InspectorMessage::CommitTransform(clip_id),
                    on_reset: InspectorMessage::UpdateTransform(clip_id, ClipTransform { opacity: defaults.opacity, ..tr }),
                    trailing: None,
                }),
                resettable_slider(ResettableSliderSpec {
                    label: "Velocidade",
                    value_label: format!("{:.2}x", tr.speed),
                    range: 0.25_f32..=4.0_f32,
                    step: 0.05_f32,
                    current: tr.speed,
                    on_change: Box::new(move |speed| {
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { speed, ..tr })
                    }),
                    on_release: InspectorMessage::CommitTransform(clip_id),
                    on_reset: InspectorMessage::UpdateTransform(clip_id, ClipTransform { speed: defaults.speed, ..tr }),
                    trailing: None,
                }),
            ]
            .spacing(12)
            .width(Length::Fill);

            scrollable(controls).height(Length::Fill).style(theme::dark_scrollable).into()
        }
        None => container(
            column![
                text("Nenhum clipe selecionado")
                    .size(13)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::TEXT_PRIMARY),
                text("Selecione um clipe na timeline abaixo para editar Escala, Posição, Opacidade e Velocidade.")
                    .size(12)
                    .color(theme::TEXT_SECONDARY)
                    .align_x(Horizontal::Center),
            ]
            .spacing(8)
            .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .padding(16)
        .center_y(Length::Fill)
        .style(theme::card)
        .into(),
    };

    container(
        column![header, horizontal_rule(1).style(theme::divider), body]
            .spacing(12)
            .height(Length::Fill),
    )
    .width(280)
    .height(Length::Fill)
    .padding(14)
    .style(theme::sidebar)
    .into()
}
