use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, horizontal_rule, horizontal_space, row, scrollable, slider, text};
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
            let asset_name = project
                .asset(clip.asset_id)
                .map_or("Clipe", |asset| asset.name.as_str());

            let section_bar = row![
                text("Transformar").size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                horizontal_space(),
                button(text("Resetar").size(11).color(theme::TEXT_SECONDARY))
                    .padding([4, 8])
                    .style(theme::ghost_button)
                    .on_press(InspectorMessage::UpdateTransform(clip_id, ClipTransform::default())),
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
                slider_control(
                    "Escala",
                    format!("{:.0}%", tr.scale * 100.0),
                    0.10_f32..=3.0_f32,
                    0.05_f32,
                    tr.scale,
                    move |scale| InspectorMessage::PreviewTransform(clip_id, ClipTransform { scale, ..tr }),
                    InspectorMessage::CommitTransform(clip_id),
                ),
                slider_control(
                    "Posição X",
                    format!("{:.0} px", tr.pos_x),
                    -960.0_f32..=960.0_f32,
                    1.0_f32,
                    tr.pos_x,
                    move |pos_x| InspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_x, ..tr }),
                    InspectorMessage::CommitTransform(clip_id),
                ),
                slider_control(
                    "Posição Y",
                    format!("{:.0} px", tr.pos_y),
                    -540.0_f32..=540.0_f32,
                    1.0_f32,
                    tr.pos_y,
                    move |pos_y| InspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_y, ..tr }),
                    InspectorMessage::CommitTransform(clip_id),
                ),
                slider_control(
                    "Opacidade",
                    format!("{opacity_pct:.0}%"),
                    0.0_f32..=100.0_f32,
                    1.0_f32,
                    opacity_pct,
                    move |pct| {
                        let opacity = (pct / 100.0).clamp(0.0, 1.0);
                        InspectorMessage::PreviewTransform(clip_id, ClipTransform { opacity, ..tr })
                    },
                    InspectorMessage::CommitTransform(clip_id),
                ),
                slider_control(
                    "Velocidade",
                    format!("{:.2}x", tr.speed),
                    0.25_f32..=4.0_f32,
                    0.05_f32,
                    tr.speed,
                    move |speed| InspectorMessage::PreviewTransform(clip_id, ClipTransform { speed, ..tr }),
                    InspectorMessage::CommitTransform(clip_id),
                ),
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

fn slider_control<'a>(
    label: &'a str,
    value_label: String,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    current: f32,
    on_change: impl Fn(f32) -> InspectorMessage + 'a,
    on_release: InspectorMessage,
) -> Element<'a, InspectorMessage> {
    container(
        column![
            row![
                text(label).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
                horizontal_space(),
                text(value_label).size(11).font(theme::FONT_SEMIBOLD).color(theme::ACCENT_HOVER),
            ]
            .align_y(Vertical::Center),
            slider(range, current, on_change)
                .step(step)
                .on_release(on_release)
                .width(Length::Fill)
                .style(theme::preview_slider),
        ]
        .spacing(6),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(theme::card)
    .into()
}
