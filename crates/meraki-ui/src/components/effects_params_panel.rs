use crate::components::effects_inspector::EffectsInspectorMessage;
use crate::components::resettable_slider::{resettable_slider, ResettableSliderSpec};
use crate::theme;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, horizontal_space, row, text, toggler};
use iced::{Element, Length};
use meraki_core::{
    BlurParams, CinematicParams, ClipEffect, ClipId, EffectParameters, FilmParams, GlitchParams,
    TransitionParams, ZoomEasing, ZoomParams,
};
use std::ops::RangeInclusive;
use uuid::Uuid;

/// Presets rápidos de 1 clique para o efeito de Zoom no inspetor individual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomPreset {
    ZoomIn,
    ZoomOut,
    Fixed,
}

/// Aplica um preset rápido de Zoom preservando foco e demais ajustes do efeito.
pub fn apply_zoom_preset(current: ZoomParams, preset: ZoomPreset) -> ZoomParams {
    match preset {
        ZoomPreset::ZoomIn => ZoomParams { start_scale: 1.0, end_scale: 1.25, easing: ZoomEasing::EaseInOut, ..current },
        ZoomPreset::ZoomOut => ZoomParams { start_scale: 1.25, end_scale: 1.0, easing: ZoomEasing::EaseInOut, ..current },
        ZoomPreset::Fixed => ZoomParams { start_scale: 1.25, end_scale: 1.25, easing: ZoomEasing::Constant, ..current },
    }
}

/// Identifica qual preset rápido melhor representa os parâmetros atuais de Zoom.
pub fn active_zoom_preset(p: ZoomParams) -> ZoomPreset {
    if p.easing == ZoomEasing::Constant || (p.start_scale - p.end_scale).abs() < 0.01 {
        ZoomPreset::Fixed
    } else if p.end_scale >= p.start_scale {
        ZoomPreset::ZoomIn
    } else {
        ZoomPreset::ZoomOut
    }
}

/// Retorna os parâmetros padrão de um efeito preservando o subtipo de transição quando aplicável.
pub fn default_params_for_effect(effect: &ClipEffect) -> EffectParameters {
    match effect.params {
        EffectParameters::Transition(p) => {
            EffectParameters::Transition(TransitionParams { kind: p.kind, ..TransitionParams::default() })
        }
        _ => EffectParameters::default_for(effect.kind),
    }
}

/// Área 2 do Inspetor: controles individuais exclusivos do efeito atualmente selecionado.
pub fn view<'a>(
    clip_id: ClipId,
    effect: &'a ClipEffect,
    params: EffectParameters,
) -> Element<'a, EffectsInspectorMessage> {
    let effect_id = effect.id;
    let mut header = row![
        text(effect.name.as_str())
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    if !effect.enabled {
        header = header.push(
            container(text("Inativo").size(9).font(theme::FONT_MEDIUM).color(theme::TEXT_SECONDARY))
                .padding([2, 6])
                .style(theme::badge),
        );
    }
    header = header.push(horizontal_space());
    header = header.push(
        button(text("Resetar efeito").size(10).color(theme::TEXT_SECONDARY))
            .padding([3, 7])
            .style(theme::ghost_button)
            .on_press(EffectsInspectorMessage::ResetEffectById(clip_id, effect_id)),
    );

    let mut body = column![header].spacing(8);
    body = match params {
        EffectParameters::Zoom(p) => append_zoom_controls(body, clip_id, effect_id, p),
        EffectParameters::Film(p) => append_film_controls(body, clip_id, effect_id, p),
        EffectParameters::Cinematic(p) => append_cinematic_controls(body, clip_id, effect_id, p),
        EffectParameters::Glitch(p) => append_glitch_controls(body, clip_id, effect_id, p),
        EffectParameters::Blur(p) => append_blur_controls(body, clip_id, effect_id, p),
        EffectParameters::Transition(p) => append_transition_controls(body, clip_id, effect_id, p),
    };

    container(body)
        .width(Length::Fill)
        .padding(10)
        .style(theme::card)
        .into()
}

fn append_zoom_controls<'a>(
    mut col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: ZoomParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let active = active_zoom_preset(p);
    let presets = row![
        zoom_preset_pill("Zoom In", ZoomPreset::ZoomIn, active, clip_id, effect_id),
        zoom_preset_pill("Zoom Out", ZoomPreset::ZoomOut, active, clip_id, effect_id),
        zoom_preset_pill("Fixo", ZoomPreset::Fixed, active, clip_id, effect_id),
    ]
    .spacing(6);
    col = col.push(presets);

    let def = ZoomParams::default();
    col = col.push(param_slider(
        clip_id,
        effect_id,
        "Zoom Inicial (%)",
        format!("{:.0}%", p.start_scale * 100.0),
        0.5..=3.0,
        0.05,
        p.start_scale as f32,
        move |v| EffectParameters::Zoom(ZoomParams { start_scale: f64::from(v), ..p }),
        EffectParameters::Zoom(ZoomParams { start_scale: def.start_scale, ..p }),
    ));
    col = col.push(param_slider(
        clip_id,
        effect_id,
        "Zoom Final (%)",
        format!("{:.0}%", p.end_scale * 100.0),
        0.5..=3.0,
        0.05,
        p.end_scale as f32,
        move |v| EffectParameters::Zoom(ZoomParams { end_scale: f64::from(v), ..p }),
        EffectParameters::Zoom(ZoomParams { end_scale: def.end_scale, ..p }),
    ));
    col = col.push(param_slider(
        clip_id,
        effect_id,
        "Foco X",
        format!("{:.2}", p.focus_x),
        -1.0..=1.0,
        0.05,
        p.focus_x as f32,
        move |v| EffectParameters::Zoom(ZoomParams { focus_x: f64::from(v), ..p }),
        EffectParameters::Zoom(ZoomParams { focus_x: def.focus_x, ..p }),
    ));
    col = col.push(param_slider(
        clip_id,
        effect_id,
        "Foco Y",
        format!("{:.2}", p.focus_y),
        -1.0..=1.0,
        0.05,
        p.focus_y as f32,
        move |v| EffectParameters::Zoom(ZoomParams { focus_y: f64::from(v), ..p }),
        EffectParameters::Zoom(ZoomParams { focus_y: def.focus_y, ..p }),
    ));

    let easing_row = row![
        text("Suavização").size(11).color(theme::TEXT_SECONDARY),
        horizontal_space(),
        easing_pill("Suave", ZoomEasing::EaseInOut, p, clip_id, effect_id),
        easing_pill("Linear", ZoomEasing::Linear, p, clip_id, effect_id),
    ]
    .spacing(6)
    .align_y(Vertical::Center);
    col.push(easing_row)
}

fn append_film_controls<'a>(
    mut col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: FilmParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let def = FilmParams::default();
    col = col.push(param_slider(
        clip_id, effect_id, "Intensidade", format!("{:.0}%", p.intensity * 100.0),
        0.0..=1.0, 0.02, p.intensity as f32,
        move |v| EffectParameters::Film(FilmParams { intensity: f64::from(v), ..p }),
        EffectParameters::Film(FilmParams { intensity: def.intensity, ..p }),
    ));
    col = col.push(param_slider(
        clip_id, effect_id, "Contraste", format!("{:.2}", p.contrast),
        0.5..=2.0, 0.02, p.contrast as f32,
        move |v| EffectParameters::Film(FilmParams { contrast: f64::from(v), ..p }),
        EffectParameters::Film(FilmParams { contrast: def.contrast, ..p }),
    ));
    col = col.push(param_slider(
        clip_id, effect_id, "Saturação", format!("{:.2}", p.saturation),
        0.0..=2.0, 0.02, p.saturation as f32,
        move |v| EffectParameters::Film(FilmParams { saturation: f64::from(v), ..p }),
        EffectParameters::Film(FilmParams { saturation: def.saturation, ..p }),
    ));
    col.push(param_slider(
        clip_id, effect_id, "Vinheta", format!("{:.0}%", p.vignette * 100.0),
        0.0..=1.0, 0.02, p.vignette as f32,
        move |v| EffectParameters::Film(FilmParams { vignette: f64::from(v), ..p }),
        EffectParameters::Film(FilmParams { vignette: def.vignette, ..p }),
    ))
}

fn append_cinematic_controls<'a>(
    mut col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: CinematicParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let def = CinematicParams::default();
    col = col.push(param_slider(
        clip_id, effect_id, "Intensidade", format!("{:.0}%", p.intensity * 100.0),
        0.0..=1.0, 0.02, p.intensity as f32,
        move |v| EffectParameters::Cinematic(CinematicParams { intensity: f64::from(v), ..p }),
        EffectParameters::Cinematic(CinematicParams { intensity: def.intensity, ..p }),
    ));
    col = col.push(param_slider(
        clip_id, effect_id, "Contraste", format!("{:.2}", p.contrast),
        0.5..=2.0, 0.02, p.contrast as f32,
        move |v| EffectParameters::Cinematic(CinematicParams { contrast: f64::from(v), ..p }),
        EffectParameters::Cinematic(CinematicParams { contrast: def.contrast, ..p }),
    ));
    col = col.push(param_slider(
        clip_id, effect_id, "Saturação", format!("{:.2}", p.saturation),
        0.0..=2.0, 0.02, p.saturation as f32,
        move |v| EffectParameters::Cinematic(CinematicParams { saturation: f64::from(v), ..p }),
        EffectParameters::Cinematic(CinematicParams { saturation: def.saturation, ..p }),
    ));
    col.push(param_slider(
        clip_id, effect_id, "Brilho", format!("{:+.2}", p.brightness),
        -0.5..=0.5, 0.02, p.brightness as f32,
        move |v| EffectParameters::Cinematic(CinematicParams { brightness: f64::from(v), ..p }),
        EffectParameters::Cinematic(CinematicParams { brightness: def.brightness, ..p }),
    ))
}

fn append_glitch_controls<'a>(
    mut col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: GlitchParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let def = GlitchParams::default();
    col = col.push(param_slider(
        clip_id, effect_id, "Intensidade", format!("{:.0}%", p.intensity * 100.0),
        0.0..=1.0, 0.02, p.intensity as f32,
        move |v| EffectParameters::Glitch(GlitchParams { intensity: f64::from(v), ..p }),
        EffectParameters::Glitch(GlitchParams { intensity: def.intensity, ..p }),
    ));
    col = col.push(param_slider(
        clip_id, effect_id, "Deslocamento RGB", format!("{:.0}%", p.rgb_shift * 100.0),
        0.0..=1.0, 0.02, p.rgb_shift as f32,
        move |v| EffectParameters::Glitch(GlitchParams { rgb_shift: f64::from(v), ..p }),
        EffectParameters::Glitch(GlitchParams { rgb_shift: def.rgb_shift, ..p }),
    ));
    col.push(param_slider(
        clip_id, effect_id, "Frequência", format!("{:.0}%", p.frequency * 100.0),
        0.0..=1.0, 0.02, p.frequency as f32,
        move |v| EffectParameters::Glitch(GlitchParams { frequency: f64::from(v), ..p }),
        EffectParameters::Glitch(GlitchParams { frequency: def.frequency, ..p }),
    ))
}

fn append_blur_controls<'a>(
    col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: BlurParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let def = BlurParams::default().radius;
    col.push(param_slider(
        clip_id, effect_id, "Raio / Intensidade", format!("{:.0}%", p.radius * 100.0),
        0.0..=1.0, 0.02, p.radius as f32,
        move |v| EffectParameters::Blur(BlurParams { radius: f64::from(v) }),
        EffectParameters::Blur(BlurParams { radius: def }),
    ))
}

fn append_transition_controls<'a>(
    mut col: iced::widget::Column<'a, EffectsInspectorMessage>,
    clip_id: ClipId,
    effect_id: Uuid,
    p: TransitionParams,
) -> iced::widget::Column<'a, EffectsInspectorMessage> {
    let def_ms = TransitionParams::default().duration_ms;
    let secs = p.duration_ms as f32 / 1000.0;
    col = col.push(param_slider(
        clip_id,
        effect_id,
        "Duração (s)",
        format!("{secs:.2}s"),
        0.1..=3.0,
        0.05,
        secs.clamp(0.1, 3.0),
        move |v| EffectParameters::Transition(TransitionParams {
            duration_ms: (v * 1000.0).round().clamp(100.0, 3000.0) as u64,
            ..p
        }),
        EffectParameters::Transition(TransitionParams { duration_ms: def_ms, ..p }),
    ));

    let in_row = edge_toggle_row("Entrada", p.apply_in, move |val| {
        EffectsInspectorMessage::UpdateEffectParams(
            clip_id,
            effect_id,
            EffectParameters::Transition(TransitionParams { apply_in: val, ..p }),
        )
    });
    let out_row = edge_toggle_row("Saída", p.apply_out, move |val| {
        EffectsInspectorMessage::UpdateEffectParams(
            clip_id,
            effect_id,
            EffectParameters::Transition(TransitionParams { apply_out: val, ..p }),
        )
    });
    col.push(in_row).push(out_row)
}

fn edge_toggle_row<'a>(
    label: &'a str,
    enabled: bool,
    on_toggle: impl Fn(bool) -> EffectsInspectorMessage + 'a,
) -> Element<'a, EffectsInspectorMessage> {
    row![
        text(label).size(12).color(theme::TEXT_PRIMARY),
        horizontal_space(),
        toggler(enabled).on_toggle(on_toggle).size(16).style(theme::accent_toggler),
    ]
    .align_y(Vertical::Center)
    .into()
}

fn zoom_preset_pill<'a>(
    label: &'a str,
    preset: ZoomPreset,
    active: ZoomPreset,
    clip_id: ClipId,
    effect_id: Uuid,
) -> Element<'a, EffectsInspectorMessage> {
    button(text(label).size(10).font(theme::FONT_MEDIUM))
        .padding([4, 8])
        .style(theme::filter_pill_button(active == preset))
        .on_press(EffectsInspectorMessage::ApplyZoomPreset(clip_id, effect_id, preset))
        .into()
}

fn easing_pill<'a>(
    label: &'a str,
    easing: ZoomEasing,
    current: ZoomParams,
    clip_id: ClipId,
    effect_id: Uuid,
) -> Element<'a, EffectsInspectorMessage> {
    let next = EffectParameters::Zoom(ZoomParams { easing, ..current });
    button(text(label).size(10).font(theme::FONT_MEDIUM))
        .padding([4, 8])
        .style(theme::filter_pill_button(current.easing == easing))
        .on_press(EffectsInspectorMessage::UpdateEffectParams(clip_id, effect_id, next))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn param_slider<'a>(
    clip_id: ClipId,
    effect_id: Uuid,
    label: &'a str,
    value_label: String,
    range: RangeInclusive<f32>,
    step: f32,
    current: f32,
    build: impl Fn(f32) -> EffectParameters + 'a,
    reset_params: EffectParameters,
) -> Element<'a, EffectsInspectorMessage> {
    resettable_slider(ResettableSliderSpec {
        label,
        value_label,
        range,
        step,
        current,
        on_change: Box::new(move |val| {
            EffectsInspectorMessage::PreviewEffectParams(clip_id, effect_id, build(val))
        }),
        on_release: EffectsInspectorMessage::CommitEffectParams(clip_id, effect_id),
        on_reset: EffectsInspectorMessage::UpdateEffectParams(clip_id, effect_id, reset_params),
        trailing: None,
    })
}
