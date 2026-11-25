use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::image;
use meraki_core::{AssetId, Clip, ClipId, MediaKind, PreviewQuality, Project, TrackId, TrackKind};
use meraki_engine::{AudioMeterFrame, EngineCommand, EngineEvent, EngineHandle, TimelineRenderClip, TrackAudioMix};
use tracing::{debug, error, info, warn};

/// Duração padrão aplicada caso o ativo ainda não tenha metadados descobertos.
pub const DEFAULT_ASSET_CLIP_DURATION_NS: u64 = 5_000_000_000;

/// Estado de reprodução e controle de preview em tempo real mantido na UI.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub current_frame: Option<image::Handle>,
    pub is_playing: bool,
    pub playhead_ns: u64,
    pub playback_rate: f64,
    pub master_volume: f32,
    pub preview_quality: PreviewQuality,
    pub selected_clip_id: Option<ClipId>,
    pub audio_meter: AudioMeterFrame,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self::with_preview_quality(PreviewQuality::default())
    }
}

impl PlaybackState {
    pub fn with_preview_quality(preview_quality: PreviewQuality) -> Self {
        Self {
            current_frame: None,
            is_playing: false,
            playhead_ns: 0,
            playback_rate: 1.0,
            master_volume: 1.0,
            preview_quality,
            selected_clip_id: None,
            audio_meter: AudioMeterFrame {
                master_peak_db: -60.0,
                master_rms_db: -60.0,
                tracks: Vec::new(),
            },
        }
    }
}

/// Comandos e eventos de interação relacionados ao player e transporte da timeline.
#[derive(Debug, Clone)]
pub enum PlaybackMessage {
    TogglePlayPause,
    Play,
    Pause,
    Seek(u64),
    SetPlaybackRate(f64),
    SetMasterVolume(f32),
    SetPreviewQuality(PreviewQuality),
    SelectClip(Option<ClipId>),
    DeleteSelectedClip,
    Undo,
    Redo,
}

/// Converte a timeline atual do `Project` na lista de clipes leves para sincronização GES.
pub fn build_render_clips(project: &Project, quality: PreviewQuality) -> Vec<TimelineRenderClip> {
    let mut render_clips = Vec::new();
    for track in &project.timeline.tracks {
        for clip in &track.clips {
            let path = if clip.title.is_some() {
                std::path::PathBuf::new()
            } else {
                let Some(asset) = project.asset(clip.asset_id) else {
                    continue;
                };
                if quality == PreviewQuality::Proxy720p {
                    asset
                        .proxy_path
                        .as_ref()
                        .filter(|proxy| proxy.is_file())
                        .cloned()
                        .unwrap_or_else(|| asset.path.clone())
                } else {
                    asset.path.clone()
                }
            };
            render_clips.push(TimelineRenderClip {
                clip_id: clip.id,
                track_id: track.id,
                path,
                track_kind: track.kind,
                timeline_start_ns: clip.timeline_start_ns,
                source_in_ns: clip.source_in_ns,
                duration_ns: clip.duration_ns,
                transform: clip.transform,
                title: clip.title.clone(),
                effects: clip.effects.clone(),
                transition_in: clip.transition_in,
                transition_out: clip.transition_out,
                keyframes: clip.keyframes.clone(),
                volume_keyframes: clip.volume_keyframes.clone(),
            });
        }
    }
    render_clips
}

/// Constrói a configuração de mixagem por trilha para sincronização com o motor de áudio.
pub fn build_audio_mix(project: &Project) -> Vec<TrackAudioMix> {
    project
        .timeline
        .tracks
        .iter()
        .map(|track| TrackAudioMix { track_id: track.id, settings: track.audio.clone() })
        .collect()
}

/// Cria um `Clip` pronto para inserção em `V1` (vídeo/imagem) ou `A1` (áudio) respeitando In/Out.
pub fn build_clip_for_asset(project: &Project, asset_id: AssetId) -> Option<(TrackId, Clip)> {
    let asset = project.asset(asset_id)?;
    let (target_kind, preferred_name) = match asset.kind {
        MediaKind::Audio => (TrackKind::Audio, "A1"),
        MediaKind::Video | MediaKind::Image => (TrackKind::Video, "V1"),
    };

    let track = project
        .timeline
        .tracks
        .iter()
        .find(|t| t.kind == target_kind && t.name == preferred_name)
        .or_else(|| project.timeline.tracks.iter().find(|t| t.kind == target_kind))?;

    let source_in_ns = asset.in_point_ns;
    let raw_end = asset
        .out_point_ns
        .filter(|&out| out > source_in_ns)
        .unwrap_or(asset.metadata.duration_ns);
    let duration_ns = raw_end
        .checked_sub(source_in_ns)
        .filter(|&d| d > 0)
        .unwrap_or(DEFAULT_ASSET_CLIP_DURATION_NS);

    let timeline_start_ns = track
        .clips
        .iter()
        .filter_map(|clip| clip.timeline_end_ns().ok())
        .max()
        .unwrap_or(0);

    Some((
        track.id,
        Clip::new(asset.id, timeline_start_ns, source_in_ns, duration_ns),
    ))
}

/// Constrói um `Clip` pronto para inserção em uma trilha específica (`A1`, `A2` ou `A3`).
pub fn build_clip_for_asset_on_track(
    project: &Project,
    asset_id: AssetId,
    track_id: TrackId,
) -> Option<Clip> {
    let asset = project.asset(asset_id)?;
    let track = project.timeline.track(track_id)?;

    let source_in_ns = asset.in_point_ns;
    let raw_end = asset
        .out_point_ns
        .filter(|&out| out > source_in_ns)
        .unwrap_or(asset.metadata.duration_ns);
    let duration_ns = raw_end
        .checked_sub(source_in_ns)
        .filter(|&d| d > 0)
        .unwrap_or(DEFAULT_ASSET_CLIP_DURATION_NS);
    let timeline_start_ns = track
        .clips
        .iter()
        .filter_map(|clip| clip.timeline_end_ns().ok())
        .max()
        .unwrap_or(0);

    Some(Clip::new(asset.id, timeline_start_ns, source_in_ns, duration_ns))
}

/// Traduz atalhos globais de teclado para ações de transporte e histórico da timeline.
pub fn map_keyboard_shortcut(key: Key, modifiers: Modifiers) -> Option<PlaybackMessage> {
    let has_any_mod =
        modifiers.control() || modifiers.command() || modifiers.alt() || modifiers.shift();

    if !has_any_mod {
        return match key.as_ref() {
            Key::Named(Named::Space) => Some(PlaybackMessage::TogglePlayPause),
            Key::Character(" ") => Some(PlaybackMessage::TogglePlayPause),
            Key::Named(Named::Delete | Named::Backspace) => Some(PlaybackMessage::DeleteSelectedClip),
            _ => None,
        };
    }

    if (modifiers.control() || modifiers.command()) && !modifiers.alt() {
        if let Key::Character(ch) = key.as_ref() {
            if ch.eq_ignore_ascii_case("z") {
                return Some(if modifiers.shift() {
                    PlaybackMessage::Redo
                } else {
                    PlaybackMessage::Undo
                });
            }
        }
    }

    None
}

/// Formata um instante em nanossegundos como timecode `HH:MM:SS:FF`.
pub fn format_timecode(ns: u64, fps: u64) -> String {
    let fps = fps.clamp(1, 240);
    let total_secs = ns / 1_000_000_000;
    let remainder_ns = ns % 1_000_000_000;
    let frames = (remainder_ns * fps) / 1_000_000_000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}:{frames:02}")
}

/// Formata um instante em nanossegundos como `MM:SS:FF` para a barra de transporte do player.
pub fn format_timecode_mm_ss_ff(ns: u64, fps: u64) -> String {
    let fps = fps.clamp(1, 240);
    let total_secs = ns / 1_000_000_000;
    let remainder_ns = ns % 1_000_000_000;
    let frames = (remainder_ns * fps) / 1_000_000_000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    format!("{minutes:02}:{seconds:02}:{frames:02}")
}

/// Aplica comandos de transporte e parâmetros de preview no estado e despacha ao motor.
pub fn apply_transport_or_selection(
    playback: &mut PlaybackState,
    project: &mut Project,
    engine: &EngineHandle,
    message: PlaybackMessage,
) -> Result<(), String> {
    match message {
        PlaybackMessage::TogglePlayPause => {
            playback.is_playing = !playback.is_playing;
            let cmd = if playback.is_playing { EngineCommand::Play } else { EngineCommand::Pause };
            engine.send(cmd).map_err(|err| err.to_string())
        }
        PlaybackMessage::Play => {
            playback.is_playing = true;
            engine.send(EngineCommand::Play).map_err(|err| err.to_string())
        }
        PlaybackMessage::Pause => {
            playback.is_playing = false;
            engine.send(EngineCommand::Pause).map_err(|err| err.to_string())
        }
        PlaybackMessage::Seek(pos_ns) => {
            playback.playhead_ns = pos_ns;
            project.timeline.playhead_ns = pos_ns;
            engine.request_seek(pos_ns);
            Ok(())
        }
        PlaybackMessage::SetPlaybackRate(rate) => {
            if rate.is_finite() && (0.05..=16.0).contains(&rate.abs()) {
                playback.playback_rate = rate;
                engine.send(EngineCommand::SetPlaybackRate(rate)).map_err(|err| err.to_string())
            } else {
                Ok(())
            }
        }
        PlaybackMessage::SetMasterVolume(volume) => {
            if volume.is_finite() {
                let clamped = volume.clamp(0.0, 1.0);
                playback.master_volume = clamped;
                engine.send(EngineCommand::SetMasterVolume(clamped)).map_err(|err| err.to_string())
            } else {
                Ok(())
            }
        }
        PlaybackMessage::SetPreviewQuality(quality) => {
            playback.preview_quality = quality;
            project.preview_quality = quality;
            let uses_proxy = project.assets.iter().any(|a| a.proxy_path.as_ref().is_some_and(|p| p.is_file()));
            let cmd = if uses_proxy {
                EngineCommand::SyncTimeline {
                    clips: build_render_clips(project, quality),
                    audio_mix: build_audio_mix(project),
                    preview_quality: quality,
                }
            } else {
                EngineCommand::SetPreviewQuality(quality)
            };
            engine.send(cmd).map_err(|err| err.to_string())
        }
        PlaybackMessage::SelectClip(clip_id) => {
            playback.selected_clip_id = clip_id;
            Ok(())
        }
        PlaybackMessage::DeleteSelectedClip | PlaybackMessage::Undo | PlaybackMessage::Redo => Ok(()),
    }
}

/// Processa eventos assíncronos do motor atualizando frame, playhead, metadados e estado.
pub fn apply_engine_event(
    playback: &mut PlaybackState,
    project: &mut Project,
    engine_ready: &mut bool,
    last_error: &mut Option<String>,
    event: EngineEvent,
) {
    match event {
        EngineEvent::EngineReady => {
            *engine_ready = true;
            info!("motor multimídia pronto");
        }
        EngineEvent::MediaDiscovered(metadata) => {
            let asset_id = metadata.asset_id;
            if project.apply_discovered_metadata(metadata) {
                info!(%asset_id, "metadados de mídia aplicados ao projeto");
            } else {
                warn!(%asset_id, "metadados recebidos para ativo inexistente no projeto");
            }
        }
        EngineEvent::ThumbnailReady(thumb) => {
            debug!(
                asset_id = %thumb.asset_id,
                width = thumb.width,
                height = thumb.height,
                "miniatura de mídia recebida"
            );
        }
        EngineEvent::ProxyProgress { asset_id, progress, proxy_path } => {
            if let (Some(path), Some(asset)) = (proxy_path, project.asset_mut(asset_id)) {
                asset.proxy_path = Some(path);
            }
            debug!(%asset_id, progress, "progresso de proxy atualizado");
        }
        EngineEvent::FrameReady(frame) => {
            playback.current_frame = Some(image::Handle::from_rgba(
                frame.width,
                frame.height,
                frame.rgba_pixels.clone(),
            ));
        }
        EngineEvent::PlayheadTick(position_ns) => {
            playback.playhead_ns = position_ns;
            project.timeline.playhead_ns = position_ns;
        }
        EngineEvent::Error(message) => {
            error!(error = %message, "erro recebido do motor multimídia");
            *last_error = Some(message);
        }
        EngineEvent::AudioLevels(frame) => {
            playback.audio_meter = frame;
        }
        EngineEvent::ExportProgress { .. }
        | EngineEvent::ExportFinished { .. }
        | EngineEvent::ExportFailed { .. }
        | EngineEvent::ExportCancelled { .. } => {
            debug!("evento assíncrono do motor processado");
        }
    }
}
