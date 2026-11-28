use crate::components::resettable_slider::{resettable_slider, ResettableSliderSpec};
use crate::components::{easing_card, effect_param_controls};
pub use crate::components::effects_params_panel::ZoomPreset;
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input,
};
use iced::{Element, Length};
use meraki_core::{
    AppLanguage, Clip, ClipId, ClipTransform, EffectParameters, Keyframe, KeyframeEasing,
    KeyframeProperty, Project, TitleOverlay, TitleStyle,
};
use uuid::Uuid;

/// Tolerância temporal (`20ms`) para considerar um keyframe alinhado à agulha atual.
pub const KEYFRAME_SNAP_TOLERANCE_NS: u64 = 20_000_000;

/// Rascunhos visuais ativos durante arraste de sliders antes do `on_release`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EffectsInspectorDrafts<'a> {
    pub title_text: Option<&'a str>,
    pub title_font_size: Option<u32>,
    pub transform: Option<ClipTransform>,
    pub effect_params: Option<(ClipId, Uuid, EffectParameters)>,
    pub selected_effect_id: Option<Uuid>,
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
    SelectEffect(Uuid),
    PreviewEffectParams(ClipId, Uuid, EffectParameters),
    CommitEffectParams(ClipId, Uuid),
    UpdateEffectParams(ClipId, Uuid, EffectParameters),
    ToggleEffectById(ClipId, Uuid),
    RemoveEffectById(ClipId, Uuid),
    ResetEffectById(ClipId, Uuid),
    ApplyZoomPreset(ClipId, Uuid, ZoomPreset),
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

/// Painel direito (`280px`, fundo `#181922`) exibindo Texto (`T1`), Efeitos, Transformação e Curvas.
pub fn view<'a>(
    project: &'a Project,
    selected_clip_id: Option<ClipId>,
    playhead_ns: u64,
    active_easing: KeyframeEasing,
    drafts: EffectsInspectorDrafts<'a>,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let header = text(s.inspector_effects_title)
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY)
        .wrapping(text::Wrapping::None);

    let selected = selected_clip_id.and_then(|id| project.timeline.clip(id).map(|(_, clip)| clip));
    let body: Element<'a, EffectsInspectorMessage> = match selected {
        Some(clip) => {
            let offset_ns = clip_local_offset_ns(clip, playhead_ns);
            let mut controls = column![summary_card(project, clip, lang)].spacing(10).width(Length::Fill);

            if let Some(ref title) = clip.title {
                controls = controls.push(title_section(
                    clip.id,
                    title,
                    drafts.title_text,
                    drafts.title_font_size,
                    lang,
                ));
            }
            if clip.title.is_none() || !clip.effects.is_empty() {
                controls = controls.push(effect_param_controls::view(
                    clip,
                    drafts.selected_effect_id,
                    drafts.effect_params,
                    lang,
                ));
            }
            controls = controls.push(transform_section(clip, offset_ns, drafts.transform, lang));
            controls = controls.push(keyframes_section(clip, offset_ns, active_easing, lang));

            scrollable(controls).height(Length::Fill).style(theme::dark_scrollable).into()
        }
        None => container(
            column![
                text(s.inspector_none_selected).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                text(s.inspector_effects_none_desc).size(12).color(theme::TEXT_SECONDARY).align_x(Horizontal::Center),
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

fn summary_card<'a>(
    project: &'a Project,
    clip: &'a Clip,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let name = clip
        .title
        .as_ref()
        .map(|t| t.text.as_str())
        .or_else(|| project.asset(clip.asset_id).map(|a| a.name.as_str()))
        .unwrap_or(s.clip_default_name);
    let active_fx_count = clip.effects.iter().filter(|e| e.enabled).count();
    let subtitle = i18n::format_clip_fx_summary(lang, active_fx_count, clip.keyframes.len());

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
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let input_value = title_draft.unwrap_or(title.text.as_str());
    let base_for_style = title.clone();
    let style_pills = row![
        style_pill(s.title_style_modern, TitleStyle::ModernText, clip_id, &base_for_style),
        style_pill(s.title_style_caption, TitleStyle::QuickCaption, clip_id, &base_for_style),
        style_pill("3D", TitleStyle::Title3D, clip_id, &base_for_style),
    ]
    .spacing(6);

    let effective_size = font_size_draft.unwrap_or(title.font_size).clamp(16, 120);
    let size_val = f32::from(effective_size as u16);
    let default_font_size = TitleOverlay::new("x", title.style).font_size;
    let reset_title = TitleOverlay { font_size: default_font_size, ..title.clone() };
    let title_for_color = title.clone();

    container(
        column![
            text(s.inspector_text_section).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text_input(s.inspector_title_placeholder, input_value)
                .on_input(move |txt| EffectsInspectorMessage::TitleTextChanged(clip_id, txt))
                .size(12)
                .padding([6, 8])
                .style(theme::text_input_style),
            style_pills,
            resettable_slider(ResettableSliderSpec {
                label: s.inspector_font_size,
                value_label: format!("{effective_size} px"),
                reset_tooltip: s.inspector_double_click_reset,
                range: 16.0_f32..=120.0_f32,
                step: 2.0_f32,
                current: size_val,
                on_change: Box::new(move |val| {
                    EffectsInspectorMessage::PreviewTitleFontSize(clip_id, val.round().clamp(16.0, 120.0) as u32)
                }),
                on_release: EffectsInspectorMessage::CommitTitleFontSize(clip_id),
                on_reset: EffectsInspectorMessage::UpdateTitle(clip_id, reset_title),
                trailing: None,
            }),
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
    button(text(label).size(10).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
        .padding([4, 7])
        .style(theme::filter_pill_button(active))
        .on_press(EffectsInspectorMessage::UpdateTitle(clip_id, updated))
        .into()
}

fn transform_section<'a>(
    clip: &'a Clip,
    offset_ns: u64,
    transform_draft: Option<ClipTransform>,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let clip_id = clip.id;
    let tr = transform_draft.unwrap_or(clip.transform);
    let defaults = ClipTransform::default();
    let opacity_pct = (tr.opacity * 100.0).round().clamp(0.0, 100.0);
    let tip = s.inspector_double_click_reset;

    let header = row![
        text(s.inspector_transform).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
        horizontal_space(),
        button(text(s.reset).size(11).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None))
            .padding([3, 8])
            .style(theme::ghost_button)
            .on_press(EffectsInspectorMessage::UpdateTransform(clip_id, defaults)),
    ]
    .align_y(Vertical::Center);

    column![
        header,
        keyframed_slider(
            s.inspector_pos_x, format!("{:.0} px", tr.pos_x), tip, -960.0_f32..=960.0_f32, 1.0_f32, tr.pos_x,
            find_keyframe_near(clip, KeyframeProperty::PosX, offset_ns).is_some(),
            move |pos_x| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_x, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::UpdateTransform(clip_id, ClipTransform { pos_x: defaults.pos_x, ..tr }),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosX },
        ),
        keyframed_slider(
            s.inspector_pos_y, format!("{:.0} px", tr.pos_y), tip, -540.0_f32..=540.0_f32, 1.0_f32, tr.pos_y,
            find_keyframe_near(clip, KeyframeProperty::PosY, offset_ns).is_some(),
            move |pos_y| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { pos_y, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::UpdateTransform(clip_id, ClipTransform { pos_y: defaults.pos_y, ..tr }),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosY },
        ),
        keyframed_slider(
            s.inspector_scale, format!("{:.0}%", tr.scale * 100.0), tip, 0.10_f32..=3.0_f32, 0.05_f32, tr.scale,
            find_keyframe_near(clip, KeyframeProperty::Scale, offset_ns).is_some(),
            move |scale| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { scale, ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::UpdateTransform(clip_id, ClipTransform { scale: defaults.scale, ..tr }),
            EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::Scale },
        ),
        keyframed_slider(
            s.inspector_opacity, format!("{opacity_pct:.0}%"), tip, 0.0_f32..=100.0_f32, 1.0_f32, opacity_pct,
            find_keyframe_near(clip, KeyframeProperty::Opacity, offset_ns).is_some(),
            move |pct| EffectsInspectorMessage::PreviewTransform(clip_id, ClipTransform { opacity: (pct / 100.0).clamp(0.0, 1.0), ..tr }),
            EffectsInspectorMessage::CommitTransform(clip_id),
            EffectsInspectorMessage::UpdateTransform(clip_id, ClipTransform { opacity: defaults.opacity, ..tr }),
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
    reset_tooltip: &'a str,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    current: f32,
    has_kf: bool,
    on_change: impl Fn(f32) -> EffectsInspectorMessage + 'a,
    on_release: EffectsInspectorMessage,
    on_reset: EffectsInspectorMessage,
    on_kf_toggle: EffectsInspectorMessage,
) -> Element<'a, EffectsInspectorMessage> {
    let kf_icon = if has_kf { icons::KEYFRAME_FILLED } else { icons::KEYFRAME_OUTLINE };
    let kf_tint = if has_kf { theme::ACCENT } else { theme::TEXT_SECONDARY };
    let kf_btn = button(icons::icon(kf_icon, 12.0, kf_tint))
        .padding([3, 5])
        .style(theme::ghost_button)
        .on_press(on_kf_toggle)
        .into();

    resettable_slider(ResettableSliderSpec {
        label,
        value_label,
        reset_tooltip,
        range,
        step,
        current,
        on_change: Box::new(on_change),
        on_release,
        on_reset,
        trailing: Some(kf_btn),
    })
}

fn keyframes_section<'a>(
    clip: &'a Clip,
    offset_ns: u64,
    active_easing: KeyframeEasing,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let sec = (offset_ns as f64) / 1_000_000_000.0;
    container(
        column![
            text(s.inspector_motion_keyframes).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            easing_card::easing_selector(active_easing, lang, EffectsInspectorMessage::SetEasing),
            text(i18n::format_keyframe_instant(lang, sec, clip.keyframes.len())).size(10).color(theme::TEXT_SECONDARY),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card)
    .into()
}
