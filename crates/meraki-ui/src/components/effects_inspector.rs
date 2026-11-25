use crate::components::easing_card;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, slider, text,
    text_input,
};
use iced::{Element, Length};
use meraki_core::{
    Clip, ClipId, ClipTransform, Keyframe, KeyframeEasing, KeyframeProperty, Project, TitleOverlay,
    TitleStyle,
};

/// Tolerância temporal (`20ms`) para considerar um keyframe alinhado à agulha atual.
pub const KEYFRAME_SNAP_TOLERANCE_NS: u64 = 20_000_000;

/// Rascunhos visuais ativos durante arraste de sliders antes do `on_release`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EffectsInspectorDrafts<'a> {
    pub title_text: Option<&'a str>,
    pub title_font_size: Option<u32>,
    pub transform: Option<ClipTransform>,
}

/// Mensagens emitidas pelo painel direito `INSPETOR DE EFEITOS`.
#[derive(Debug, Clone)]
pub enum EffectsInspectorMessage {
    TitleTextChanged(ClipId, String),
    PreviewTitleFontSize(ClipId, u32),
    CommitTitleFontSize(ClipId),
    UpdateTitle(ClipId, TitleOverlay),
    PreviewTransform(ClipId, ClipTransform),
    CommitTransform(ClipId),
    UpdateTransform(ClipId, ClipTransform),
    ToggleKeyframe { clip_id: ClipId, property: KeyframeProperty },
    SetEasing(KeyframeEasing),
}

/// Converte o timestamp absoluto do `playhead_ns` para deslocamento local seguro dentro do clipe.
pub fn clip_local_offset_ns(clip: &Clip, playhead_ns: u64) -> u64 {
    playhead_ns.saturating_sub(clip.timeline_start_ns).min(clip.duration_ns)
}

/// Procura keyframe de uma propriedade próximo ao deslocamento temporal informado.
pub fn find_keyframe_near(
    clip: &Clip,
    property: KeyframeProperty,
    offset_ns: u64,
) -> Option<Keyframe> {
    clip.keyframes
        .iter()
        .copied()
        .find(|kf| kf.property == property && kf.time_offset_ns.abs_diff(offset_ns) <= KEYFRAME_SNAP_TOLERANCE_NS)
}

/// Painel direito (`280px`, fundo `#181922`) exibindo Texto (`T1`), Transformação com keyframes e Curvas.
pub fn view<'a>(
    project: &'a Project,
    selected_clip_id: Option<ClipId>,
    playhead_ns: u64,
    active_easing: KeyframeEasing,
    drafts: EffectsInspectorDrafts<'a>,
) -> Element<'a, EffectsInspectorMessage> {
    let header = text("INSPETOR DE EFEITOS")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let selected = selected_clip_id.and_then(|id| project.timeline.clip(id).map(|(_, clip)| clip));
    let body: Element<'a, EffectsInspectorMessage> = match selected {
        Some(clip) => {
            let offset_ns = clip_local_offset_ns(clip, playhead_ns);
            let mut controls = column![summary_card(project, clip)].spacing(10).width(Length::Fill);

            if let Some(ref title) = clip.title {
                controls = controls.push(title_section(
                    clip.id,
                    title,
                    drafts.title_text,
                    drafts.title_font_size,
                ));
            }
            controls = controls.push(transform_section(clip, offset_ns, drafts.transform));
            controls = controls.push(keyframes_section(clip, offset_ns, active_easing));

            scrollable(controls).height(Length::Fill).style(theme::dark_scrollable).into()
        }
        None => container(
            column![
                text("Nenhum clipe selecionado")
                    .size(13)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::TEXT_PRIMARY),
                text("Selecione um vídeo na timeline ou adicione um Título na Biblioteca à esquerda.")
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

fn summary_card<'a>(project: &'a Project, clip: &'a Clip) -> Element<'a, EffectsInspectorMessage> {
    let name = clip
        .title
        .as_ref()
        .map(|t| t.text.as_str())
        .or_else(|| project.asset(clip.asset_id).map(|a| a.name.as_str()))
        .unwrap_or("Clipe");
    let active_fx_count = clip.effects.iter().filter(|e| e.enabled).count();
    let subtitle = format!("{active_fx_count} efeito(s) • {} keyframe(s)", clip.keyframes.len());

    container(
        column![
            text(name).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(subtitle).size(11).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card)
    .into()
}

fn title_section<'a>(
    clip_id: ClipId,
    title: &'a TitleOverlay,
    title_draft: Option<&'a str>,
    font_size_draft: Option<u32>,
) -> Element<'a, EffectsInspectorMessage> {
    let input_value = title_draft.unwrap_or(title.text.as_str());
    let base_for_style = title.clone();
    let style_pills = row![
        style_pill("Moderno", TitleStyle::ModernText, clip_id, &base_for_style),
        style_pill("Legenda", TitleStyle::QuickCaption, clip_id, &base_for_style),
        style_pill("3D", TitleStyle::Title3D, clip_id, &base_for_style),
    ]
    .spacing(6);

    let effective_size = font_size_draft.unwrap_or(title.font_size).clamp(16, 120);
    let size_val = f32::from(effective_size as u16);
    let title_for_color = title.clone();

    container(
        column![
            text("Texto").size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text_input("Conteúdo do título...", input_value)
                .on_input(move |s| EffectsInspectorMessage::TitleTextChanged(clip_id, s))
                .size(12)
                .padding([6, 8])
                .style(theme::text_input_style),
            style_pills,
            row![
                text("Tamanho").size(11).color(theme::TEXT_SECONDARY),
                horizontal_space(),
                text(format!("{effective_size} px"))
                    .size(11)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::ACCENT_HOVER),
            ]
            .align_y(Vertical::Center),
            slider(16.0_f32..=120.0_f32, size_val, move |val| {
                EffectsInspectorMessage::PreviewTitleFontSize(
                    clip_id,
                    val.round().clamp(16.0, 120.0) as u32,
                )
            })
            .step(2.0_f32)
            .on_release(EffectsInspectorMessage::CommitTitleFontSize(clip_id))
            .width(Length::Fill)
            .style(theme::preview_slider),
            easing_card::color_swatch_row(title.color_rgba, move |rgba| {
                let mut next = title_for_color.clone();
                next.color_rgba = rgba;
                EffectsInspectorMessage::UpdateTitle(clip_id, next)
            }),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card)
    .into()
}

fn style_pill<'a>(
    label: &'a str,
    style: TitleStyle,
    clip_id: ClipId,
    current: &TitleOverlay,
) -> Element<'a, EffectsInspectorMessage> {
    let active = current.style == style;
    let mut updated = current.clone();
    updated.style = style;
    button(text(label).size(10).font(theme::FONT_MEDIUM))
        .padding([4, 7])
        .style(theme::filter_pill_button(active))
        .on_press(EffectsInspectorMessage::UpdateTitle(clip_id, updated))
        .into()
}

fn transform_section<'a>(
    clip: &'a Clip,
    offset_ns: u64,
    transform_draft: Option<ClipTransform>,
) -> Element<'a, EffectsInspectorMessage> {
    let clip_id = clip.id;
    let tr = transform_draft.unwrap_or(clip.transform);
    let opacity_pct = (tr.opacity * 100.0).round().clamp(0.0, 100.0);

    column![
        text("Transformar").size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
        keyframed_slider(
            "Posição X",
            format!("{:.0} px", tr.pos_x),
            -960.0_f32..=960.0_f32,
            1.0_f32,
            tr.pos_x,
            find_keyframe_near(clip, KeyframeProperty::PosX, offset_ns).is_some(),
            move |pos_x| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_x, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosX },
        ),
        keyframed_slider(
            "Posição Y",
            format!("{:.0} px", tr.pos_y),
            -540.0_f32..=540.0_f32,
            1.0_f32,
            tr.pos_y,
            find_keyframe_near(clip, KeyframeProperty::PosY, offset_ns).is_some(),
            move |pos_y| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_y, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosY },
        ),
        keyframed_slider(
            "Escala",
            format!("{:.0}%", tr.scale * 100.0),
            0.10_f32..=3.0_f32,
            0.05_f32,
            tr.scale,
            find_keyframe_near(clip, KeyframeProperty::Scale, offset_ns).is_some(),
            move |scale| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { scale, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::Scale },
        ),
        keyframed_slider(
            "Opacidade",
            format!("{opacity_pct:.0}%"),
            0.0_f32..=100.0_f32,
            1.0_f32,
            opacity_pct,
            find_keyframe_near(clip, KeyframeProperty::Opacity, offset_ns).is_some(),
            move |pct| {
                let opacity = (pct / 100.0).clamp(0.0, 1.0);
                EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { opacity, ..tr })
            },
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::Opacity },
        ),
    ]
    .spacing(8)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn keyframed_slider<'a>(
    label: &'a str,
    value_label: String,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    current: f32,
    has_kf: bool,
    on_change: impl Fn(f32) -> EffectsInspectorMessage + 'a,
    on_release: EffectsInspectorMessage,
    on_kf_toggle: EffectsInspectorMessage,
) -> Element<'a, EffectsInspectorMessage> {
    let kf_icon = if has_kf { icons::KEYFRAME_FILLED } else { icons::KEYFRAME_OUTLINE };
    let kf_tint = if has_kf { theme::ACCENT } else { theme::TEXT_SECONDARY };
    container(
        column![
            row![
                text(label).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
                horizontal_space(),
                text(value_label).size(11).font(theme::FONT_SEMIBOLD).color(theme::ACCENT_HOVER),
                button(icons::icon(kf_icon, 12.0, kf_tint))
                    .padding([3, 5])
                    .style(theme::ghost_button)
                    .on_press(on_kf_toggle),
            ]
            .spacing(4)
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

fn keyframes_section<'a>(
    clip: &'a Clip,
    offset_ns: u64,
    active_easing: KeyframeEasing,
) -> Element<'a, EffectsInspectorMessage> {
    let sec = (offset_ns as f64) / 1_000_000_000.0;
    container(
        column![
            text("Movimento & Keyframes")
                .size(13)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY),
            easing_card::easing_selector(active_easing, EffectsInspectorMessage::SetEasing),
            text(format!("Instante: {sec:.2}s • {} keyframe(s)", clip.keyframes.len()))
                .size(10)
                .color(theme::TEXT_SECONDARY),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card)
    .into()
}
