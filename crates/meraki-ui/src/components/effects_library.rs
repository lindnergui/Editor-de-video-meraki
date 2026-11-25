use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, Column,
};
use iced::{Element, Length};
use meraki_core::{Clip, TitleStyle, TransitionKind, VideoEffectKind};

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
    pub transitions_expanded: bool,
    pub titles_expanded: bool,
    pub video_effects_expanded: bool,
}

impl Default for EffectsLibraryState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
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
    ToggleTransition(TransitionKind),
    AddTitle(TitleStyle),
    ToggleVideoEffect(VideoEffectKind),
}

const TRANSITIONS: [(TransitionKind, &str, &str, &str); 3] = [
    (TransitionKind::Fade, "Fade", "Dissolver entrada suave", "fade dissolver transicao"),
    (TransitionKind::Zoom, "Zoom", "Aproximação dinâmica", "zoom escala transicao"),
    (TransitionKind::Blur, "Desfoque", "Transição com desfoque", "desfoque blur transicao"),
];

const TITLES: [(TitleStyle, &str, &str, &str); 3] = [
    (TitleStyle::ModernText, "Texto Moderno", "Título limpo centralizado", "texto moderno titulo"),
    (TitleStyle::QuickCaption, "Legenda Rápida", "Rodapé de fácil leitura", "legenda rapida rodape"),
    (TitleStyle::Title3D, "Título 3D", "Destaque em negrito 3D", "titulo 3d destaque"),
];

const VIDEO_EFFECTS: [(VideoEffectKind, &str, &str, &str); 3] = [
    (VideoEffectKind::Glitch, "Glitch", "Distúrbio digital estilizado", "glitch erro digital efeito"),
    (VideoEffectKind::Cinematic, "Cinemático", "Contraste dramático de cinema", "cinematico cinema contraste"),
    (VideoEffectKind::FilmStyle, "Estilo Filme", "Paleta retrô de película", "estilo filme vintage retro"),
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
) -> Element<'a, EffectsLibraryMessage> {
    let header = text("BIBLIOTECA DE EFEITOS")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let search = row![
        icons::icon(icons::SEARCH, 13.0, theme::TEXT_SECONDARY),
        text_input("Buscar efeitos...", &state.search_query)
            .on_input(EffectsLibraryMessage::SearchChanged)
            .size(12)
            .padding([5, 8])
            .style(theme::text_input_style),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let mut content = Column::new().spacing(10).width(Length::Fill);

    content = content.push(section_header(
        "Transições",
        state.transitions_expanded,
        LibrarySection::Transitions,
    ));
    if state.transitions_expanded {
        for (kind, label, desc, kw) in TRANSITIONS {
            if matches_query(&state.search_query, label, kw) {
                let active = selected_clip
                    .and_then(|c| c.transition_in)
                    .is_some_and(|tr| tr.kind == kind);
                content = content.push(effect_card(
                    icons::TRANSITION,
                    label,
                    desc,
                    active,
                    EffectsLibraryMessage::ToggleTransition(kind),
                ));
            }
        }
    }

    content = content.push(section_header(
        "Títulos & Textos",
        state.titles_expanded,
        LibrarySection::Titles,
    ));
    if state.titles_expanded {
        for (style, label, desc, kw) in TITLES {
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
        "Efeitos de Vídeo",
        state.video_effects_expanded,
        LibrarySection::VideoEffects,
    ));
    if state.video_effects_expanded {
        for (kind, label, desc, kw) in VIDEO_EFFECTS {
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
