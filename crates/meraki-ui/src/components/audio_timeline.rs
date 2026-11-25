use crate::components::audio_timeline_render::{
    build_track_headers, build_zoom_rail, draw_static_audio, hit_audio_clip, StaticAudioSpec,
};
use crate::components::timeline_cache::{hash_audio_timeline_static, DualCanvasCache};
use crate::components::timeline_math::{time_ns_to_x, x_to_time_ns, TimelineViewState, RULER_HEIGHT};
use crate::components::timeline_render::draw_playhead;
use crate::playback::PlaybackState;
use crate::theme;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry};
use iced::widget::{container, row, vertical_rule};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme};
use meraki_core::{
    Clip, ClipId, Project, Timeline, Track, TrackId, TrackKind, VolumeKeyframe, MAX_VOLUME_KF_DB,
    MIN_VOLUME_KF_DB,
};

pub const COMPACT_TRACK_HEIGHT: f32 = 24.0;
pub const COMPACT_TRACK_GAP: f32 = 4.0;
pub const AUDIO_TRACK_HEIGHT: f32 = 64.0;
pub const AUDIO_TRACK_GAP: f32 = 6.0;

/// Eventos da timeline expandida de áudio (zoom, scrub, seleção e automação de volume).
#[derive(Debug, Clone)]
pub enum AudioTimelineMessage {
    ZoomIn,
    ZoomOut,
    Scrub(u64),
    SelectTrack(TrackId),
    SelectClip(Option<ClipId>),
    UpsertVolumeKeyframe {
        clip_id: ClipId,
        previous_offset_ns: Option<u64>,
        keyframe: VolumeKeyframe,
    },
    RemoveVolumeKeyframe {
        clip_id: ClipId,
        time_offset_ns: u64,
    },
}

/// Linha posicionada na timeline de áudio (`T1`/`V1` compactas no topo, `A1..A3` expandidas).
pub struct AudioTrackRow<'a> {
    pub id: Option<TrackId>,
    pub name: &'a str,
    pub kind: TrackKind,
    pub clips: &'a [Clip],
    pub muted: bool,
    pub solo: bool,
    pub y: f32,
    pub height: f32,
    pub gap: f32,
}

/// Ordena trilhas (`Overlay` e `Video` compactos no topo, `Audio` expandido abaixo).
pub fn track_band_layout(timeline: &Timeline) -> Vec<AudioTrackRow<'_>> {
    let mut ordered: Vec<&Track> = timeline.tracks.iter().collect();
    ordered.sort_by_key(|t| match t.kind {
        TrackKind::Overlay => 0,
        TrackKind::Video => 1,
        TrackKind::Audio => 2,
    });

    let mut rows = Vec::with_capacity(ordered.len());
    let mut y = RULER_HEIGHT + 4.0;
    for track in ordered {
        let (height, gap) = if track.kind == TrackKind::Audio {
            (AUDIO_TRACK_HEIGHT, AUDIO_TRACK_GAP)
        } else {
            (COMPACT_TRACK_HEIGHT, COMPACT_TRACK_GAP)
        };
        rows.push(AudioTrackRow {
            id: Some(track.id),
            name: track.name.as_str(),
            kind: track.kind,
            clips: &track.clips,
            muted: track.audio.muted,
            solo: track.audio.solo,
            y,
            height,
            gap,
        });
        y += height + gap;
    }
    rows
}

/// Converte ganho de volume (`-48..+12` dB) para coordenada vertical no clipe de áudio.
pub fn volume_db_to_y(gain_db: f32, clip_top: f32, clip_height: f32) -> f32 {
    let pad = 7.0_f32;
    let usable = (clip_height - pad * 2.0).max(8.0);
    let safe_db = if gain_db.is_finite() {
        gain_db.clamp(MIN_VOLUME_KF_DB, MAX_VOLUME_KF_DB)
    } else {
        0.0
    };
    let ratio = (MAX_VOLUME_KF_DB - safe_db) / (MAX_VOLUME_KF_DB - MIN_VOLUME_KF_DB);
    clip_top + pad + ratio * usable
}

/// Converte coordenada vertical em pixels dentro do clipe para ganho em dB (`-48..+12`).
pub fn y_to_volume_db(y: f32, clip_top: f32, clip_height: f32) -> f32 {
    let pad = 7.0_f32;
    let usable = (clip_height - pad * 2.0).max(8.0);
    if !y.is_finite() {
        return 0.0;
    }
    let ratio = ((y - (clip_top + pad)) / usable).clamp(0.0, 1.0);
    let raw = MAX_VOLUME_KF_DB - ratio * (MAX_VOLUME_KF_DB - MIN_VOLUME_KF_DB);
    ((raw * 2.0).round() / 2.0).clamp(MIN_VOLUME_KF_DB, MAX_VOLUME_KF_DB)
}

/// Localiza um `VolumeKeyframe` próximo à coordenada clicada dentro dos clipes de áudio.
pub fn hit_test_volume_keyframe(
    point: Point,
    rows: &[AudioTrackRow<'_>],
    px: f32,
    scroll: f32,
) -> Option<(ClipId, VolumeKeyframe, f32, f32)> {
    for row in rows.iter().filter(|r| r.kind == TrackKind::Audio) {
        let clip_top = row.y + 3.0;
        let clip_h = (row.height - 6.0).max(12.0);
        for clip in row.clips {
            for &kf in &clip.volume_keyframes {
                let x =
                    time_ns_to_x(clip.timeline_start_ns.saturating_add(kf.time_offset_ns), px, scroll);
                let y = volume_db_to_y(kf.gain_db, clip_top, clip_h);
                if point.distance(Point::new(x, y)) <= 10.0 {
                    return Some((clip.id, kf, clip_top, clip_h));
                }
            }
        }
    }
    None
}

/// Renderiza a barra inferior em largura total com automação de volume sobre formas de onda.
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
    selected_track_id: Option<TrackId>,
) -> Element<'a, AudioTimelineMessage> {
    let rows = track_band_layout(&project.timeline);
    let canvas_widget = Canvas::new(AudioTimelineProgram { project, playback, view_state })
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        row![
            build_zoom_rail(),
            vertical_rule(1).style(theme::divider),
            build_track_headers(&rows, selected_track_id),
            vertical_rule(1).style(theme::divider),
            canvas_widget,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(theme::base_canvas)
    .into()
}

struct AudioTimelineProgram<'a> {
    project: &'a Project,
    playback: &'a PlaybackState,
    view_state: &'a TimelineViewState,
}

#[derive(Default)]
pub(crate) struct DragState {
    scrubbing: bool,
    active_kf: Option<DraggingVolumeKf>,
    cache: DualCanvasCache,
}

pub(crate) struct DraggingVolumeKf {
    pub(crate) clip_id: ClipId,
    pub(crate) clip_start_ns: u64,
    pub(crate) clip_duration_ns: u64,
    pub(crate) clip_top: f32,
    pub(crate) clip_h: f32,
    pub(crate) previous_offset_ns: Option<u64>,
    pub(crate) preview: VolumeKeyframe,
}

impl canvas::Program<AudioTimelineMessage> for AudioTimelineProgram<'_> {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<AudioTimelineMessage>) {
        let px = self.view_state.px_per_sec;
        let scroll = self.view_state.scroll_x_px;
        let Some(local) = cursor.position_in(bounds) else {
            if let canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
                state.scrubbing = false;
                if let Some(drag) = state.active_kf.take() {
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::UpsertVolumeKeyframe {
                            clip_id: drag.clip_id,
                            previous_offset_ns: drag.previous_offset_ns,
                            keyframe: drag.preview,
                        }),
                    );
                }
            }
            return (canvas::event::Status::Ignored, None);
        };

        let rows = track_band_layout(&self.project.timeline);
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if local.y < RULER_HEIGHT {
                    state.scrubbing = true;
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::Scrub(x_to_time_ns(local.x, px, scroll))),
                    );
                }
                if let Some((clip_id, kf, clip_top, clip_h)) =
                    hit_test_volume_keyframe(local, &rows, px, scroll)
                {
                    if let Some((_, clip)) = self.project.timeline.clip(clip_id) {
                        state.active_kf = Some(DraggingVolumeKf {
                            clip_id,
                            clip_start_ns: clip.timeline_start_ns,
                            clip_duration_ns: clip.duration_ns,
                            clip_top,
                            clip_h,
                            previous_offset_ns: Some(kf.time_offset_ns),
                            preview: kf,
                        });
                        return (
                            canvas::event::Status::Captured,
                            Some(AudioTimelineMessage::SelectClip(Some(clip_id))),
                        );
                    }
                }
                if let Some((clip_id, clip, clip_top, clip_h)) = hit_audio_clip(local, &rows, px, scroll) {
                    let abs_ns = x_to_time_ns(local.x, px, scroll);
                    let offset = abs_ns.saturating_sub(clip.timeline_start_ns).min(clip.duration_ns);
                    let gain_db = y_to_volume_db(local.y, clip_top, clip_h);
                    state.active_kf = Some(DraggingVolumeKf {
                        clip_id,
                        clip_start_ns: clip.timeline_start_ns,
                        clip_duration_ns: clip.duration_ns,
                        clip_top,
                        clip_h,
                        previous_offset_ns: None,
                        preview: VolumeKeyframe::new(offset, gain_db),
                    });
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::SelectClip(Some(clip_id))),
                    );
                }
                return (canvas::event::Status::Captured, Some(AudioTimelineMessage::SelectClip(None)));
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some((clip_id, kf, _, _)) = hit_test_volume_keyframe(local, &rows, px, scroll) {
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::RemoveVolumeKeyframe {
                            clip_id,
                            time_offset_ns: kf.time_offset_ns,
                        }),
                    );
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.scrubbing {
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::Scrub(x_to_time_ns(local.x, px, scroll))),
                    );
                }
                if let Some(ref mut drag) = state.active_kf {
                    let abs_ns = x_to_time_ns(local.x, px, scroll);
                    drag.preview.time_offset_ns =
                        abs_ns.saturating_sub(drag.clip_start_ns).min(drag.clip_duration_ns);
                    drag.preview.gain_db = y_to_volume_db(local.y, drag.clip_top, drag.clip_h);
                    return (canvas::event::Status::Captured, None);
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.scrubbing = false;
                if let Some(drag) = state.active_kf.take() {
                    return (
                        canvas::event::Status::Captured,
                        Some(AudioTimelineMessage::UpsertVolumeKeyframe {
                            clip_id: drag.clip_id,
                            previous_offset_ns: drag.previous_offset_ns,
                            keyframe: drag.preview,
                        }),
                    );
                }
            }
            _ => {}
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let px = self.view_state.px_per_sec;
        let scroll = self.view_state.scroll_x_px;
        let drag_kf = state.active_kf.as_ref().map(|d| {
            (d.clip_id, d.preview.time_offset_ns, d.preview.gain_db.to_bits())
        });
        let static_key = hash_audio_timeline_static(
            self.project,
            self.playback.selected_clip_id,
            px,
            scroll,
            drag_kf,
        );
        state.cache.draw_layers(
            renderer,
            bounds.size(),
            static_key,
            |frame| {
                draw_static_audio(
                    frame,
                    StaticAudioSpec {
                        project: self.project,
                        selected_clip_id: self.playback.selected_clip_id,
                        active_drag: state.active_kf.as_ref(),
                        size: bounds.size(),
                        px,
                        scroll,
                    },
                );
            },
            |frame| draw_playhead(frame, self.playback.playhead_ns, px, scroll, bounds.height),
        )
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.active_kf.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if let Some(local) = cursor.position_in(bounds) {
            if local.y < RULER_HEIGHT {
                return mouse::Interaction::Pointer;
            }
            let rows = track_band_layout(&self.project.timeline);
            if hit_test_volume_keyframe(local, &rows, self.view_state.px_per_sec, self.view_state.scroll_x_px).is_some() {
                return mouse::Interaction::Grab;
            }
            if hit_audio_clip(local, &rows, self.view_state.px_per_sec, self.view_state.scroll_x_px).is_some() {
                return mouse::Interaction::Crosshair;
            }
        }
        mouse::Interaction::default()
    }
}
