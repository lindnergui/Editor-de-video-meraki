use crate::components::resettable_slider::{resettable_slider, ResettableSliderSpec};
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{button, column, container, horizontal_rule, horizontal_space, row, scrollable, text};
use iced::{Background, Border, Color, Element, Length};
use meraki_core::{
    AppLanguage, ClipId, Project, TimelineTransition, TransitionKind, TransitionPlacement,
    DEFAULT_TRANSITION_DURATION_NS,
};
use uuid::Uuid;

/// Mensagens do inspetor dedicado a uma `TimelineTransition` posicional selecionada.
#[derive(Debug, Clone, Copy)]
pub enum TransitionInspectorMessage {
    SetKind(Uuid, TransitionKind),
    PreviewDuration(Uuid, u64),
    CommitDuration(Uuid),
    SetDuration(Uuid, u64),
    Remove(Uuid),
}

fn danger_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { Color::from_rgb8(215, 58, 58) } else { theme::DANGER })),
        text_color: theme::TEXT_ON_ACCENT,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: theme::RADIUS_CONTROL.into() },
        ..button::Style::default()
    }
}

fn clip_label<'a>(project: &'a Project, clip_id: Uuid, fallback: &'a str) -> &'a str {
    let Some((_, clip)) = project.timeline.clip(ClipId(clip_id)) else {
        return fallback;
    };
    clip.title
        .as_ref()
        .map(|t| t.text.as_str())
        .or_else(|| project.asset(clip.asset_id).map(|a| a.name.as_str()))
        .unwrap_or(fallback)
}

/// Painel direito (`280px`) para inspecionar e ajustar uma transição posicional selecionada.
pub fn view<'a>(
    project: &'a Project,
    transition: &'a TimelineTransition,
    duration_draft: Option<(Uuid, u64)>,
    lang: AppLanguage,
) -> Element<'a, TransitionInspectorMessage> {
    let s = i18n::strings(lang);
    let tr_id = transition.id;
    let header = text(s.transition_inspector_title)
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY)
        .wrapping(text::Wrapping::None);

    let (place_label, target_desc) = match transition.placement {
        TransitionPlacement::BetweenClips { left_clip_id, right_clip_id } => (
            s.transition_place_between,
            format!(
                "{} -> {}",
                clip_label(project, left_clip_id, s.clip_default_name),
                clip_label(project, right_clip_id, s.clip_default_name),
            ),
        ),
        TransitionPlacement::ClipStart { clip_id } => (
            s.transition_place_start,
            clip_label(project, clip_id, s.clip_default_name).to_string(),
        ),
        TransitionPlacement::ClipEnd { clip_id } => (
            s.transition_place_end,
            clip_label(project, clip_id, s.clip_default_name).to_string(),
        ),
    };

    let summary_card = container(
        column![
            row![
                icons::icon(icons::TRANSITION, 13.0, theme::ACCENT),
                text(s.transition_applied_at).size(11).color(theme::TEXT_SECONDARY),
                horizontal_space(),
                container(text(place_label).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT).wrapping(text::Wrapping::None))
                    .padding([2, 7])
                    .style(theme::badge_accent),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
            text(target_desc).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
        ]
        .spacing(6),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card);

    let kind_pills = row![
        kind_pill(s.transition_kind_crossfade, TransitionKind::Crossfade, transition.kind, tr_id),
        kind_pill(s.transition_kind_blur, TransitionKind::Blur, transition.kind, tr_id),
        kind_pill(s.transition_kind_zoom, TransitionKind::Zoom, transition.kind, tr_id),
    ]
    .spacing(6);

    let kind_card = container(
        column![
            text(s.transition_type_label).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            kind_pills,
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card);

    let active_dur_ns = duration_draft
        .filter(|(id, _)| *id == tr_id)
        .map_or(transition.duration_ns, |(_, d)| d);
    let secs = (active_dur_ns as f32 / 1_000_000_000.0).clamp(0.1, 3.0);

    let duration_slider = resettable_slider(ResettableSliderSpec {
        label: s.inspector_transition_duration,
        value_label: format!("{secs:.2}s"),
        reset_tooltip: s.inspector_double_click_reset,
        range: 0.1_f32..=3.0_f32,
        step: 0.05_f32,
        current: secs,
        on_change: Box::new(move |val| {
            let ns = (f64::from(val) * 1_000_000_000.0).round().clamp(100_000_000.0, 3_000_000_000.0) as u64;
            TransitionInspectorMessage::PreviewDuration(tr_id, ns)
        }),
        on_release: TransitionInspectorMessage::CommitDuration(tr_id),
        on_reset: TransitionInspectorMessage::SetDuration(tr_id, DEFAULT_TRANSITION_DURATION_NS),
        trailing: None,
    });

    let remove_btn = button(
        row![
            horizontal_space(),
            icons::icon(icons::TRASH, 13.0, theme::TEXT_ON_ACCENT),
            text(s.transition_remove).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT).wrapping(text::Wrapping::None),
            horizontal_space(),
        ]
        .spacing(6)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 12])
    .style(danger_btn_style)
    .on_press(TransitionInspectorMessage::Remove(tr_id));

    let body = scrollable(column![summary_card, kind_card, duration_slider, remove_btn].spacing(10).width(Length::Fill))
        .height(Length::Fill)
        .style(theme::dark_scrollable);

    container(column![header, horizontal_rule(1).style(theme::divider), body].spacing(12).height(Length::Fill))
        .width(280)
        .height(Length::Fill)
        .padding(14)
        .style(theme::sidebar)
        .into()
}

fn kind_pill<'a>(
    label: &'a str,
    kind: TransitionKind,
    active: TransitionKind,
    tr_id: Uuid,
) -> Element<'a, TransitionInspectorMessage> {
    button(text(label).size(11).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
        .padding([5, 8])
        .style(theme::filter_pill_button(kind == active))
        .on_press(TransitionInspectorMessage::SetKind(tr_id, kind))
        .into()
}
