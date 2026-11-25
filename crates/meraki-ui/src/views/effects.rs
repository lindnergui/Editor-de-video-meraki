use crate::components::effects_inspector::{self, EffectsInspectorDrafts, EffectsInspectorMessage};
use crate::components::effects_library::{
    self, EffectsLibraryMessage, EffectsLibraryState, LibrarySection,
};
use crate::components::player::{self, PlayerMessage};
use crate::components::player_overlay::{self, PlayerOverlayMessage};
use crate::components::timeline_canvas::{self, TimelineCanvasMessage};
use crate::components::timeline_math::TimelineViewState;
use crate::playback::PlaybackState;
use crate::theme;
use iced::widget::{column, container, horizontal_rule, row, vertical_rule};
use iced::{Element, Length};
use meraki_core::{
    ClipId, ClipTransform, HistoryStack, KeyframeEasing, KeyframeProperty, KeymapProfile, Project,
    TrackKind,
};

/// Estado local da aba Efeitos (biblioteca, timeline, texto temporário e easing ativo).
#[derive(Debug, Clone)]
pub struct EffectsState {
    pub library: EffectsLibraryState,
    pub timeline: TimelineViewState,
    pub active_easing: KeyframeEasing,
    pub active_property: KeyframeProperty,
    pub title_draft: Option<(ClipId, String)>,
    pub title_font_size_draft: Option<(ClipId, u32)>,
    pub transform_draft: Option<(ClipId, ClipTransform)>,
}

impl Default for EffectsState {
    fn default() -> Self {
        Self {
            library: EffectsLibraryState::default(),
            timeline: TimelineViewState::default(),
            active_easing: KeyframeEasing::Linear,
            active_property: KeyframeProperty::PosX,
            title_draft: None,
            title_font_size_draft: None,
            transform_draft: None,
        }
    }
}

/// Mensagens da aba Efeitos roteadas por `MerakiApp`.
#[derive(Debug, Clone)]
pub enum EffectsMessage {
    Library(EffectsLibraryMessage),
    Player(PlayerMessage),
    Overlay(PlayerOverlayMessage),
    Inspector(EffectsInspectorMessage),
    Timeline(TimelineCanvasMessage),
}

impl EffectsState {
    /// Consome interações puramente visuais (busca, seções expansíveis e ferramentas da timeline).
    pub fn apply_local(&mut self, msg: &EffectsMessage) -> bool {
        match msg {
            EffectsMessage::Library(EffectsLibraryMessage::SearchChanged(q)) => {
                self.library.search_query = q.clone();
            }
            EffectsMessage::Library(EffectsLibraryMessage::ToggleSection(section)) => match section {
                LibrarySection::Transitions => {
                    self.library.transitions_expanded = !self.library.transitions_expanded;
                }
                LibrarySection::Titles => {
                    self.library.titles_expanded = !self.library.titles_expanded;
                }
                LibrarySection::VideoEffects => {
                    self.library.video_effects_expanded = !self.library.video_effects_expanded;
                }
            },
            EffectsMessage::Timeline(TimelineCanvasMessage::SetTool(tool)) => {
                self.timeline.tool = *tool;
            }
            EffectsMessage::Timeline(TimelineCanvasMessage::ToggleSnap) => {
                self.timeline.snap_enabled = !self.timeline.snap_enabled;
            }
            EffectsMessage::Timeline(TimelineCanvasMessage::ZoomIn) => self.timeline.zoom_in(),
            EffectsMessage::Timeline(TimelineCanvasMessage::ZoomOut) => self.timeline.zoom_out(),
            EffectsMessage::Timeline(TimelineCanvasMessage::ZoomAnchored { factor, anchor_ns }) => {
                self.timeline.zoom_anchored_at(*factor, *anchor_ns);
            }
            _ => return false,
        }
        true
    }
}

/// Calcula uma posição inicial livre na trilha `T1` próxima ao `playhead_ns` atual.
pub fn next_title_start_ns(project: &Project, playhead_ns: u64, duration_ns: u64) -> u64 {
    let Some(track) = project.timeline.tracks.iter().find(|t| t.kind == TrackKind::Overlay) else {
        return playhead_ns;
    };
    let mut candidate = playhead_ns;
    for clip in &track.clips {
        let clip_end = clip.timeline_start_ns.saturating_add(clip.duration_ns);
        let cand_end = candidate.saturating_add(duration_ns);
        if clip.timeline_start_ns < cand_end && candidate < clip_end {
            candidate = clip_end;
        }
    }
    candidate
}

/// Renderiza a aba Efeitos completa: Biblioteca (`240px`) | Player + Overlay | Inspetor (`280px`) + Timeline (`T1` no topo).
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    history: &'a HistoryStack,
    state: &'a EffectsState,
    profile: KeymapProfile,
) -> Element<'a, EffectsMessage> {
    let selected_clip = playback
        .selected_clip_id
        .and_then(|id| project.timeline.clip(id).map(|(_, clip)| clip));

    let library_panel = effects_library::view(&state.library, selected_clip).map(EffectsMessage::Library);
    let overlay = player_overlay::optional_view(selected_clip).map(|el| el.map(EffectsMessage::Overlay));
    let player_panel = player::view_with_overlay(
        project,
        playback,
        history.can_undo(),
        history.can_redo(),
        profile,
        EffectsMessage::Player,
        overlay,
    );

    let drafts = EffectsInspectorDrafts {
        title_text: state
            .title_draft
            .as_ref()
            .and_then(|(id, text)| (Some(*id) == playback.selected_clip_id).then_some(text.as_str())),
        title_font_size: state
            .title_font_size_draft
            .and_then(|(id, size)| (Some(id) == playback.selected_clip_id).then_some(size)),
        transform: state
            .transform_draft
            .and_then(|(id, tr)| (Some(id) == playback.selected_clip_id).then_some(tr)),
    };
    let inspector_panel = effects_inspector::view(
        project,
        playback.selected_clip_id,
        playback.playhead_ns,
        state.active_easing,
        drafts,
    )
    .map(EffectsMessage::Inspector);

    let top_row = container(
        row![
            library_panel,
            vertical_rule(1).style(theme::divider),
            player_panel,
            vertical_rule(1).style(theme::divider),
            inspector_panel,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(66));

    let bottom_timeline = container(
        timeline_canvas::view_with_options(project, playback, &state.timeline, profile, true)
            .map(EffectsMessage::Timeline),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(34));

    column![
        top_row,
        horizontal_rule(1).style(theme::divider),
        bottom_timeline,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
