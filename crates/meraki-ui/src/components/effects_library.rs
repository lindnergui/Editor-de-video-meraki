use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, Column,
};
use iced::{Element, Length};
use meraki_core::{
    AppLanguage, Clip, EffectParameters, TimelineTransition, TitleStyle, TransitionEffectKind,
    TransitionKind, TransitionPlacement, VideoEffectKind,
};

/// Posicionamento alvo selecionado antes/durante a aplicação de uma transição posicional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionPlacementMode {
    #[default]
    BetweenClips,
    ClipStart,
    ClipEnd,
}

/// Seções expansíveis da biblioteca de efeitos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySection {
    Transitions,
    Titles,
    VideoEffects,
}

/// Estado local da barra esquerda `BIBLIOTECA DE EFEITOS`.
#[derive(Debug, Clone)]
pub struct EffectsLibraryState {
    pub search_query: String,
    pub transition_placement: TransitionPlacementMode,
    pub transitions_expanded: bool,
    pub titles_expanded: bool,
    pub video_effects_expanded: bool,
}

impl Default for EffectsLibraryState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            transition_placement: TransitionPlacementMode::BetweenClips,
            transitions_expanded: true,
            titles_expanded: true,
            video_effects_expanded: true,
        }
    }
}

/// Ações disparadas pela biblioteca de efeitos.
#[derive(Debug, Clone)]
pub enum EffectsLibraryMessage {
    SearchChanged(String),
    ToggleSection(LibrarySection),
    SetTransitionPlacement(TransitionPlacementMode),
    ApplyPositionalTransition(TransitionKind),
    ApplyTransition(TransitionEffectKind),
    ToggleTransition(TransitionKind),
    AddTitle(TitleStyle),
    ToggleVideoEffect(VideoEffectKind),
}

const TRANSITIONS: [(TransitionEffectKind, TransitionKind, &str); 3] = [
    (TransitionEffectKind::Fade, TransitionKind::Fade, "fade dissolver dissolve transicao transition"),
    (TransitionEffectKind::ZoomTransition, TransitionKind::Zoom, "zoom escala scale transicao transition"),
    (TransitionEffectKind::BlurTransition, TransitionKind::Blur, "desfoque blur transicao transition"),
];

const TITLES: [(TitleStyle, &str); 3] = [
    (TitleStyle::ModernText, "texto moderno modern title titulo"),
    (TitleStyle::QuickCaption, "legenda rapida quick caption rodape"),
    (TitleStyle::Title3D, "titulo 3d title headline destaque"),
];

const VIDEO_EFFECTS: [(VideoEffectKind, &str); 5] = [
    (VideoEffectKind::Zoom, "zoom escala scale aproximacao ken burns efeito effect"),
    (VideoEffectKind::FilmStyle, "estilo filme film look vintage retro"),
    (VideoEffectKind::Cinematic, "cinematico cinema cinematic contraste"),
    (VideoEffectKind::Glitch, "glitch erro digital efeito effect"),
    (VideoEffectKind::Blur, "desfoque blur suave gaussian efeito effect"),
];

/// Verifica se um item da biblioteca corresponde ao termo buscado (sem distinção de maiúsculas/acentos).
pub fn matches_query(query: &str, label: &str, keywords: &str) -> bool {
    let normalized = fold_ascii(query);
    let q = normalized.trim();
    if q.is_empty() {
        return true;
    }
    fold_ascii(label).contains(q) || fold_ascii(keywords).contains(q)
}

fn fold_ascii(input: &str) -> String {
    input
        .chars()
        .flat_map(char::to_lowercase)
        .map(|ch| match ch {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Painel esquerdo (`240px`, fundo `#181922`) com busca e 3 seções expansíveis de efeitos.
pub fn view<'a>(
    state: &'a EffectsLibraryState,
    selected_clip: Option<&'a Clip>,
    selected_transition: Option<&'a TimelineTransition>,
    lang: AppLanguage,
) -> Element<'a, EffectsLibraryMessage> {
    let s = i18n::strings(lang);
    let header = text(s.effects_library_title)
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY)
        .wrapping(text::Wrapping::None);

    let search = row![
        icons::icon(icons::SEARCH, 13.0, theme::TEXT_SECONDARY),
        text_input(s.effects_search_placeholder, &state.search_query)
            .on_input(EffectsLibraryMessage::SearchChanged)
            .size(12)
            .padding([5, 8])
            .style(theme::text_input_style),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let mut content = Column::new().spacing(10).width(Length::Fill);

    content = content.push(section_header(
        s.effects_section_transitions,
        state.transitions_expanded,
        LibrarySection::Transitions,
    ));
    if state.transitions_expanded {
        let active_mode = selected_transition.map_or(state.transition_placement, |tr| match tr.placement {
            TransitionPlacement::BetweenClips { .. } => TransitionPlacementMode::BetweenClips,
            TransitionPlacement::ClipStart { .. } => TransitionPlacementMode::ClipStart,
            TransitionPlacement::ClipEnd { .. } => TransitionPlacementMode::ClipEnd,
        });
        let placement_bar = row![
            placement_pill(s.transition_place_between, TransitionPlacementMode::BetweenClips, active_mode),
            placement_pill(s.transition_place_start, TransitionPlacementMode::ClipStart, active_mode),
            placement_pill(s.transition_place_end, TransitionPlacementMode::ClipEnd, active_mode),
        ]
        .spacing(4)
        .width(Length::Fill);
        content = content.push(placement_bar);

        for (effect_kind, legacy_kind, kw) in TRANSITIONS {
            let (label, desc) = i18n::transition_effect_info(lang, effect_kind);
            if matches_query(&state.search_query, label, kw) {
                let active = selected_transition.is_some_and(|tr| tr.kind == legacy_kind)
                    || selected_clip.is_some_and(|c| {
                        c.transition_in.is_some_and(|tr| tr.kind == legacy_kind)
                            || c.effects.iter().any(|ef| {
                                ef.enabled
                                    && matches!(ef.params, EffectParameters::Transition(p) if p.kind == effect_kind)
                            })
                    });
                content = content.push(effect_card(
                    icons::TRANSITION,
                    label,
                    desc,
                    active,
                    EffectsLibraryMessage::ApplyPositionalTransition(legacy_kind),
                ));
            }
        }
    }

    content = content.push(section_header(
        s.effects_section_titles,
        state.titles_expanded,
        LibrarySection::Titles,
    ));
    if state.titles_expanded {
        for (style, kw) in TITLES {
            let (label, desc) = i18n::title_style_info(lang, style);
            if matches_query(&state.search_query, label, kw) {
                let active = selected_clip
                    .and_then(|c| c.title.as_ref())
                    .is_some_and(|t| t.style == style);
                content = content.push(effect_card(
                    icons::TRACK_TEXT,
                    label,
                    desc,
                    active,
                    EffectsLibraryMessage::AddTitle(style),
                ));
            }
        }
    }

    content = content.push(section_header(
        s.effects_section_video,
        state.video_effects_expanded,
        LibrarySection::VideoEffects,
    ));
    if state.video_effects_expanded {
        for (kind, kw) in VIDEO_EFFECTS {
            let (label, desc) = i18n::video_effect_info(lang, kind);
            if matches_query(&state.search_query, label, kw) {
                let active = selected_clip.is_some_and(|c| {
                    c.effects.iter().any(|ef| ef.kind == kind && ef.enabled)
                });
                content = content.push(effect_card(
                    icons::TAB_EFFECTS,
                    label,
                    desc,
                    active,
                    EffectsLibraryMessage::ToggleVideoEffect(kind),
                ));
            }
        }
    }

    container(
        column![
            header,
            search,
            horizontal_rule(1).style(theme::divider),
            scrollable(content).height(Length::Fill).style(theme::dark_scrollable),
        ]
        .spacing(10)
        .height(Length::Fill),
    )
    .width(240)
    .height(Length::Fill)
    .padding(12)
    .style(theme::sidebar)
    .into()
}

fn section_header<'a>(
    title: &'a str,
    expanded: bool,
    section: LibrarySection,
) -> Element<'a, EffectsLibraryMessage> {
    let chevron = if expanded { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT };
    button(
        row![
            icons::icon(chevron, 12.0, theme::TEXT_SECONDARY),
            text(title).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            horizontal_space(),
        ]
        .spacing(6)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([5, 6])
    .style(theme::ghost_button)
    .on_press(EffectsLibraryMessage::ToggleSection(section))
    .into()
}

fn placement_pill<'a>(
    label: &'a str,
    mode: TransitionPlacementMode,
    active: TransitionPlacementMode,
) -> Element<'a, EffectsLibraryMessage> {
    button(text(label).size(10).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
        .padding([4, 6])
        .style(theme::filter_pill_button(mode == active))
        .on_press(EffectsLibraryMessage::SetTransitionPlacement(mode))
        .into()
}

fn effect_card<'a>(
    svg_bytes: &'static [u8],
    label: &'a str,
    detail: &'a str,
    active: bool,
    msg: EffectsLibraryMessage,
) -> Element<'a, EffectsLibraryMessage> {
    let icon_color = if active { theme::ACCENT_HOVER } else { theme::ACCENT };
    button(
        row![
            container(icons::icon(svg_bytes, 14.0, icon_color))
                .padding(6)
                .style(theme::preview_viewport),
            column![
                text(label).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                text(detail).size(10).color(theme::TEXT_SECONDARY),
            ]
            .spacing(2),
        ]
        .spacing(9)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(theme::media_card_button(active))
    .on_press(msg)
    .into()
}
