use crate::components::effects_inspector::EffectsInspectorMessage;
use crate::components::effects_params_panel;
use crate::components::tooltip::icon_tooltip;
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::tooltip::Position;
use iced::widget::{button, column, container, horizontal_space, row, text};
use iced::{Background, Border, Element, Length};
use meraki_core::{AppLanguage, Clip, ClipEffect, ClipId, EffectParameters};
use uuid::Uuid;

/// Resolve qual efeito deve aparecer no inspetor individual (o selecionado ou o primeiro do take).
pub fn resolve_selected_effect(clip: &Clip, selected_effect_id: Option<Uuid>) -> Option<&ClipEffect> {
    selected_effect_id
        .and_then(|id| clip.effects.iter().find(|effect| effect.id == id))
        .or_else(|| clip.effects.first())
}

/// Renderiza Área 1 ("Efeitos Aplicados no Take") + Área 2 ("Controles Individuais do Efeito Selecionado").
pub fn view<'a>(
    clip: &'a Clip,
    selected_effect_id: Option<Uuid>,
    draft: Option<(ClipId, Uuid, EffectParameters)>,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let selected = resolve_selected_effect(clip, selected_effect_id);
    let selected_id = selected.map(|effect| effect.id);

    let mut applied_list = column![text(s.inspector_applied_effects)
        .size(12)
        .font(theme::FONT_SEMIBOLD)
        .color(theme::TEXT_PRIMARY)]
    .spacing(6)
    .width(Length::Fill);

    if clip.effects.is_empty() {
        applied_list = applied_list.push(
            text(s.inspector_no_effects)
                .size(11)
                .color(theme::TEXT_SECONDARY),
        );
    } else {
        for effect in &clip.effects {
            applied_list = applied_list.push(applied_effect_row(
                clip.id,
                effect,
                selected_id == Some(effect.id),
                lang,
            ));
        }
    }

    let applied_card = container(applied_list)
        .width(Length::Fill)
        .padding(10)
        .style(theme::card);

    let mut root = column![applied_card].spacing(8).width(Length::Fill);
    if let Some(effect) = selected {
        let params = draft
            .filter(|(cid, eid, _)| *cid == clip.id && *eid == effect.id)
            .map_or(effect.params, |(_, _, p)| p);
        root = root.push(effects_params_panel::view(clip.id, effect, params, lang));
    }
    root.into()
}

fn applied_effect_row<'a>(
    clip_id: ClipId,
    effect: &'a ClipEffect,
    is_selected: bool,
    lang: AppLanguage,
) -> Element<'a, EffectsInspectorMessage> {
    let s = i18n::strings(lang);
    let effect_id = effect.id;
    let (eye_icon, eye_tint, eye_tip) = if effect.enabled {
        (icons::EYE, theme::ACCENT, s.inspector_disable_effect)
    } else {
        (icons::EYE_OFF, theme::TEXT_SECONDARY, s.inspector_enable_effect)
    };
    let toggle_btn = icon_tooltip(
        button(icons::icon(eye_icon, 13.0, eye_tint))
            .padding([3, 5])
            .style(theme::ghost_button)
            .on_press(EffectsInspectorMessage::ToggleEffectById(clip_id, effect_id)),
        eye_tip,
        Position::Top,
    );

    let name_color = if effect.enabled { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    let select_btn = button(
        row![
            text(i18n::clip_effect_label(lang, effect))
                .size(11)
                .font(theme::FONT_MEDIUM)
                .color(name_color),
            horizontal_space(),
        ]
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([4, 6])
    .style(theme::ghost_button)
    .on_press(EffectsInspectorMessage::SelectEffect(effect_id));

    let remove_btn = icon_tooltip(
        button(icons::icon(icons::CLOSE, 11.0, theme::TEXT_SECONDARY))
            .padding([3, 5])
            .style(theme::ghost_button)
            .on_press(EffectsInspectorMessage::RemoveEffectById(clip_id, effect_id)),
        s.inspector_remove_effect,
        Position::Top,
    );

    let (border_color, border_width) = if is_selected {
        (theme::ACCENT, 1.5)
    } else {
        (theme::BORDER_SUBTLE, theme::BORDER_WIDTH)
    };

    container(row![toggle_btn, select_btn, remove_btn].spacing(4).align_y(Vertical::Center))
        .width(Length::Fill)
        .padding([4, 6])
        .style(move |_| container::Style {
            background: Some(Background::Color(theme::BG_SURFACE)),
            text_color: Some(theme::TEXT_PRIMARY),
            border: Border {
                color: border_color,
                width: border_width,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into()
}
