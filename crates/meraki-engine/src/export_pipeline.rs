use crate::audio_dsp::AudioDspController;
use crate::audio_meter::AudioMeterAggregator;
use crate::clip_prepare::{PreparedClip, PreparedSource};
use crate::discoverer;
use crate::effects_ges;
use crate::protocol::{TimelineRenderClip, TrackAudioMix};
use gstreamer as gst;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ExportSettings, PreviewQuality, TrackKind};
use std::path::Path;

pub(crate) fn detect_active_streams(prepared: &[PreparedClip], audio_only: bool) -> (bool, bool) {
    let mut has_video = false;
    let mut has_audio = false;
    for item in prepared {
        match &item.source {
            PreparedSource::Title(_) if !audio_only => has_video = true,
            PreparedSource::Media { track_types, .. } => {
                if !audio_only && track_types.contains(ges::TrackType::VIDEO) {
                    has_video = true;
                }
                if track_types.contains(ges::TrackType::AUDIO) {
                    has_audio = true;
                }
            }
            _ => {}
        }
    }
    (has_video, has_audio)
}

pub(crate) struct RenderSpec<'a> {
    pub settings: &'a ExportSettings,
    pub audio_tracks: Vec<TrackAudioMix>,
    pub prepared: &'a [PreparedClip],
    pub has_video: bool,
    pub has_audio: bool,
    pub profile: &'a gstreamer_pbutils::EncodingProfile,
    pub output_path: &'a Path,
    pub worker: &'a crate::protocol::EngineWorkerChannel,
}

pub(crate) fn build_render_pipeline(spec: RenderSpec<'_>) -> Result<(ges::Timeline, ges::Pipeline), String> {
    let timeline = ges::Timeline::new();
    if spec.has_video {
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
    if spec.has_audio {
        let audio_track = ges::AudioTrack::new();
        timeline.add_track(&audio_track).map_err(|e| format!("add_audio_track: {e}"))?;
    }
    timeline.set_auto_transition(true);

    let mut audio_dsp = AudioDspController::new();
    let mut meter = AudioMeterAggregator::new();
    audio_dsp.prepare_sync(spec.audio_tracks).map_err(|e| e.to_string())?;
    populate_export_layers(
        &timeline,
        spec.prepared,
        spec.has_video,
        spec.has_audio,
        &mut audio_dsp,
        &mut meter,
        spec.worker,
    )?;

    if !timeline.commit() {
        return Err("falha ao confirmar timeline de exportação (commit)".to_string());
    }
    audio_dsp.finalize_after_commit();

    let pipeline = ges::Pipeline::new();
    pipeline.set_timeline(&timeline).map_err(|e| format!("set_timeline: {e}"))?;
    let output_uri = discoverer::path_to_file_uri(spec.output_path).map_err(|e| e.to_string())?;
    pipeline
        .set_render_settings(&output_uri, spec.profile)
        .map_err(|e| format!("set_render_settings: {e}"))?;
    pipeline
        .set_mode(ges::PipelineFlags::RENDER)
        .map_err(|e| format!("set_mode RENDER: {e}"))?;

    Ok((timeline, pipeline))
}

struct ExportLayerSlot {
    kind: TrackKind,
    layer: ges::Layer,
    occupied: Vec<(u64, u64)>,
}

fn populate_export_layers(
    timeline: &ges::Timeline,
    prepared: &[PreparedClip],
    has_video: bool,
    has_audio: bool,
    audio_dsp: &mut AudioDspController,
    meter: &mut AudioMeterAggregator,
    worker: &crate::protocol::EngineWorkerChannel,
) -> Result<(), String> {
    let mut slots: Vec<ExportLayerSlot> = Vec::new();
    for item in prepared {
        let clip: &TimelineRenderClip = &item.render;
        let end_ns = clip.timeline_start_ns.saturating_add(clip.duration_ns);
        let layer = acquire_export_layer(timeline, &mut slots, clip.track_kind, clip.timeline_start_ns, end_ns);

        match &item.source {
            PreparedSource::Media { asset, track_types, is_image, natural_size } => {
                let mut mask = ges::TrackType::empty();
                if has_video && track_types.contains(ges::TrackType::VIDEO) {
                    mask |= ges::TrackType::VIDEO;
                }
                if has_audio && track_types.contains(ges::TrackType::AUDIO) {
                    mask |= ges::TrackType::AUDIO;
                }
                if mask.is_empty() {
                    continue;
                }
                let inpoint = if *is_image { gst::ClockTime::ZERO } else { gst::ClockTime::from_nseconds(clip.source_in_ns) };
                let ges_clip = layer
                    .add_asset(
                        asset,
                        gst::ClockTime::from_nseconds(clip.timeline_start_ns),
                        inpoint,
                        gst::ClockTime::from_nseconds(clip.duration_ns),
                        mask,
                    )
                    .map_err(|e| format!("add_asset '{}': {e}", clip.path.display()))?;

                if mask.contains(ges::TrackType::VIDEO) {
                    effects_ges::apply_clip_transform(&ges_clip, clip, false, *natural_size, PreviewQuality::Full1080p);
                    effects_ges::apply_clip_effects(&ges_clip, clip, worker);
                    effects_ges::apply_transitions_and_keyframes(&ges_clip, clip, *natural_size, PreviewQuality::Full1080p);
                }
                if mask.contains(ges::TrackType::AUDIO) {
                    audio_dsp.attach_clip_audio(&ges_clip, clip, meter);
                }
            }
            PreparedSource::Title(title) if has_video => {
                if let Some(ges_clip) = effects_ges::add_title_clip(&layer, clip, title, worker) {
                    effects_ges::apply_clip_transform(&ges_clip, clip, true, None, PreviewQuality::Full1080p);
                    effects_ges::apply_clip_effects(&ges_clip, clip, worker);
                    effects_ges::apply_transitions_and_keyframes(&ges_clip, clip, None, PreviewQuality::Full1080p);
                }
            }
            PreparedSource::Title(_) => {}
        }
    }
    Ok(())
}

fn acquire_export_layer(
    timeline: &ges::Timeline,
    slots: &mut Vec<ExportLayerSlot>,
    kind: TrackKind,
    start_ns: u64,
    end_ns: u64,
) -> ges::Layer {
    if let Some(slot) = slots.iter_mut().find(|s| s.kind == kind && !s.occupied.iter().any(|&(a, b)| start_ns < b && a < end_ns)) {
        slot.occupied.push((start_ns, end_ns));
        return slot.layer.clone();
    }
    let layer = timeline.append_layer();
    slots.push(ExportLayerSlot { kind, layer: layer.clone(), occupied: vec![(start_ns, end_ns)] });
    layer
}
