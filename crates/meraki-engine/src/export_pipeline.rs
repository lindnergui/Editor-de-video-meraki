use crate::audio_output::{float_audio_caps, protect_peaks, MasterMix, MIX_RATE};
use crate::clip_prepare::PreparedClip;
use crate::discoverer;
use crate::protocol::TrackAudioMix;
use crate::timeline_builder::{self, clip_track_types};
use gstreamer as gst;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ExportSettings, PreviewQuality, TimelineTransition};
use std::path::Path;

pub(crate) fn detect_active_streams(prepared: &[PreparedClip], audio_only: bool) -> ges::TrackType {
    prepared.iter().fold(ges::TrackType::empty(), |tracks, item| {
        tracks | clip_track_types(item, audio_only)
    })
}

pub(crate) struct RenderSpec<'a> {
    pub settings: &'a ExportSettings,
    pub audio_tracks: Vec<TrackAudioMix>,
    pub master: MasterMix,
    pub prepared: &'a [PreparedClip],
    pub transitions: &'a [TimelineTransition],
    pub profile: &'a gstreamer_pbutils::EncodingProfile,
    pub output_path: &'a Path,
    pub worker: &'a crate::protocol::EngineWorkerChannel,
}

pub(crate) fn build_render_pipeline(spec: RenderSpec<'_>) -> Result<(ges::Timeline, ges::Pipeline), String> {
    let audio_only = spec.settings.format.is_audio_only();
    let tracks = detect_active_streams(spec.prepared, audio_only);
    let timeline = ges::Timeline::new();
    if tracks.contains(ges::TrackType::VIDEO) {
        let video_track = ges::VideoTrack::new();
        let (w, h) = spec.settings.resolution.dimensions();
        let fps = spec.settings.fps.as_u32();
        let caps = gst::Caps::builder("video/x-raw")
            .field("width", i32::try_from(w.max(16)).unwrap_or(1920))
            .field("height", i32::try_from(h.max(16)).unwrap_or(1080))
            .field("framerate", gst::Fraction::new(i32::try_from(fps.clamp(1, 240)).unwrap_or(30), 1))
            .build();
        video_track.set_restriction_caps(&caps);
        timeline.add_track(&video_track).map_err(|e| format!("add_video_track: {e}"))?;
    }
    if tracks.contains(ges::TrackType::AUDIO) {
        let audio_track = ges::AudioTrack::new();
        audio_track.set_restriction_caps(&float_audio_caps(MIX_RATE));
        timeline.add_track(&audio_track).map_err(|e| format!("add_audio_track: {e}"))?;
        let gain = if spec.master.muted { 0.0 } else { spec.master.gain as f32 };
        protect_peaks(&audio_track, gain).map_err(|err| err.to_string())?;
    }
    timeline.set_auto_transition(true);

    let mut contents = timeline_builder::populate(
        &timeline, spec.prepared, spec.transitions, PreviewQuality::Full1080p, audio_only, spec.audio_tracks, spec.worker,
    ).map_err(|err| err.to_string())?;
    if !timeline.commit_sync() {
        return Err("falha ao confirmar timeline de exportação (commit)".to_string());
    }
    contents.audio_dsp.finalize_after_commit();

    let pipeline = ges::Pipeline::new();
    pipeline.set_timeline(&timeline).map_err(|e| format!("set_timeline: {e}"))?;
    let output_uri = discoverer::path_to_file_uri(spec.output_path).map_err(|e| e.to_string())?;
    pipeline
        .set_render_settings(&output_uri, spec.profile)
        .map_err(|e| format!("set_render_settings: {e}"))?;
    pipeline
        .set_mode(ges::PipelineFlags::RENDER)
        .map_err(|e| format!("set_mode RENDER: {e}"))?;
    // Algumas versões do GES criam sinks padrão na transição Null -> Ready,
    // mesmo com o playsink fora do pipeline de renderização.
    let unused_preview_sink = |name: &'static str| {
        gst::ElementFactory::make("fakesink")
            .name(name)
            .build()
            .map_err(|err| format!("falha ao configurar sink inativo {name}: {err}"))
    };
    pipeline.preview_set_audio_sink(Some(&unused_preview_sink("meraki-render-unused-audio")?));
    pipeline.preview_set_video_sink(Some(&unused_preview_sink("meraki-render-unused-video")?));

    Ok((timeline, pipeline))
}
