use crate::components::audio_library::{self, AudioLibraryMessage};
use crate::components::audio_timeline::{self, AudioTimelineMessage};
use crate::components::eq_canvas::{self, EqMessage};
use crate::components::mixer::{self, MixerMessage};
use crate::components::player::{self, PlayerMessage};
use crate::components::timeline_math::TimelineViewState;
use crate::playback::PlaybackState;
use crate::theme;
use iced::widget::{column, container, horizontal_rule, row, vertical_rule};
use iced::{Element, Length};
use meraki_core::{HistoryStack, Project, Track, TrackAudioSettings, TrackId, TrackKind};
use std::path::PathBuf;

/// Estado local da aba Áudio (busca, trilha selecionada e snapshots de arraste).
#[derive(Debug, Clone, Default)]
pub struct AudioState {
    pub search_query: String,
    pub selected_track_id: Option<TrackId>,
    pub timeline: TimelineViewState,
    pub eq_drag_origin: Option<(TrackId, TrackAudioSettings)>,
    pub gain_drag_origin: Option<(TrackId, TrackAudioSettings)>,
    pub nr_drag_origin: Option<(TrackId, TrackAudioSettings)>,
}

/// Mensagens da aba Áudio roteadas por `MerakiApp`.
#[derive(Debug, Clone)]
pub enum AudioMessage {
    Library(AudioLibraryMessage),
    AudioFilesPicked(Vec<PathBuf>),
    Player(PlayerMessage),
    Eq(EqMessage),
    Mixer(MixerMessage),
    Timeline(AudioTimelineMessage),
}

/// Abre o seletor nativo filtrado para arquivos de áudio de forma assíncrona.
pub async fn pick_audio_files() -> Vec<PathBuf> {
    let handles = rfd::AsyncFileDialog::new()
        .set_title("Importar Áudio")
        .add_filter("Áudios", &["wav", "mp3", "aac", "flac", "ogg", "m4a"])
        .pick_files()
        .await;
    handles.unwrap_or_default().into_iter().map(|h| h.path().to_path_buf()).collect()
}

/// Retorna a trilha de áudio selecionada ou a primeira trilha `Audio` disponível no projeto.
pub fn resolve_selected_audio_track(
    project: &Project,
    selected_id: Option<TrackId>,
) -> Option<&Track> {
    selected_id
        .and_then(|id| project.timeline.track(id))
        .filter(|t| t.kind == TrackKind::Audio)
        .or_else(|| project.timeline.tracks.iter().find(|t| t.kind == TrackKind::Audio))
}

/// Renderiza a aba Áudio completa:
/// Topo: Biblioteca (`240px`) | Player Compacto (`~34%`) | EQ + Melhorias (`~66%`) | Mixer (`250px`)
/// Base: Timeline expandida de áudio (`~36%` da altura).
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    history: &'a HistoryStack,
    state: &'a AudioState,
) -> Element<'a, AudioMessage> {
    let audio_tracks: Vec<&Track> =
        project.timeline.tracks.iter().filter(|t| t.kind == TrackKind::Audio).collect();
    let selected_track = resolve_selected_audio_track(project, state.selected_track_id);
    let selected_id = selected_track.map(|t| t.id);
    let selected_name = selected_track.map_or("A1", |t| t.name.as_str());
    let active_settings = selected_track.map(|t| t.audio.clone()).unwrap_or_default();

    let library_panel =
        audio_library::view(project, &state.search_query, selected_name, &active_settings)
            .map(AudioMessage::Library);

    let player_panel = container(player::view_compact(
        project,
        playback,
        history.can_undo(),
        history.can_redo(),
        AudioMessage::Player,
    ))
    .width(Length::FillPortion(34))
    .height(Length::Fill);

    let eq_panel = eq_canvas::view(selected_name, &active_settings).map(AudioMessage::Eq);
    let center_split = row![
        player_panel,
        vertical_rule(1).style(theme::divider),
        eq_panel,
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let mixer_panel = mixer::view(&audio_tracks, selected_id, playback).map(AudioMessage::Mixer);

    let top_row = container(
        row![
            library_panel,
            vertical_rule(1).style(theme::divider),
            center_split,
            vertical_rule(1).style(theme::divider),
            mixer_panel,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(64));

    let bottom_timeline = container(
        audio_timeline::view(project, playback, &state.timeline, selected_id)
            .map(AudioMessage::Timeline),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(36));

    column![top_row, horizontal_rule(1).style(theme::divider), bottom_timeline]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
