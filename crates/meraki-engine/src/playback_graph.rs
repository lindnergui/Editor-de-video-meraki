use crate::audio_dsp::AudioDspController;
use crate::audio_meter::AudioMeterAggregator;
use crate::clip_prepare::{PipelineError, PreparedClip, PreparedSource};
use crate::effects_ges;
use crate::frame_sink::{build_audio_preview_sink, PreviewVideoSink};
use crate::protocol::{EngineWorkerChannel, TrackAudioMix};
use crate::timeline_sync::ActiveVideo;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipId, PreviewQuality, TrackKind};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

pub(crate) struct PlaybackGraph {
    pub(crate) pipeline: ges::Pipeline,
    pub(crate) video_sink: PreviewVideoSink,
    pub(crate) audio_dsp: AudioDspController,
    pub(crate) audio_meter: AudioMeterAggregator,
    pub(crate) has_clips: bool,
    pub(crate) active_video: HashMap<ClipId, ActiveVideo>,
    video_track: ges::VideoTrack,
    master_volume: gst::Element,
}

impl PlaybackGraph {
    pub(crate) fn new(
        clips: &[PreparedClip],
        audio_mix: Vec<TrackAudioMix>,
        quality: PreviewQuality,
        worker: &Arc<EngineWorkerChannel>,
    ) -> Result<Self, PipelineError> {
        let timeline = ges::Timeline::new();
        let video_track = ges::VideoTrack::new();
        timeline
            .add_track(&video_track)
            .map_err(|err| setup_error("add_video_track", err))?;
        timeline
            .add_track(&ges::AudioTrack::new())
            .map_err(|err| setup_error("add_audio_track", err))?;
        timeline.set_auto_transition(true);

        let mut meter_tracks: Vec<_> = audio_mix.iter().map(|mix| mix.track_id).collect();
        for item in clips {
            if !meter_tracks.contains(&item.render.track_id) {
                meter_tracks.push(item.render.track_id);
            }
        }
        let mut audio_dsp = AudioDspController::new();
        audio_dsp.prepare_sync(audio_mix)?;
        let mut audio_meter = AudioMeterAggregator::new();
        audio_meter.reset_for_sync(&meter_tracks);

        let mut layers = Vec::new();
        let mut active_video = HashMap::new();
        let mut has_clips = false;
        for item in clips {
            let clip = &item.render;
            let layer = acquire_layer(&timeline, &mut layers, item);
            let (ges_clip, track_types, natural_size) = match &item.source {
                PreparedSource::Media {
                    asset,
                    track_types,
                    is_image,
                    natural_size,
                } => {
                    let inpoint = if *is_image {
                        gst::ClockTime::ZERO
                    } else {
                        gst::ClockTime::from_nseconds(clip.source_in_ns)
                    };
                    let ges_clip = layer
                        .add_asset(
                            asset,
                            gst::ClockTime::from_nseconds(clip.timeline_start_ns),
                            inpoint,
                            gst::ClockTime::from_nseconds(clip.duration_ns),
                            *track_types,
                        )
                        .map_err(|err| PipelineError::AddAssetClip {
                            path: clip.path.clone(),
                            reason: err.to_string(),
                        })?;
                    (ges_clip, *track_types, *natural_size)
                }
                PreparedSource::Title => {
                    let Some(ges_clip) = effects_ges::add_title_clip(&layer, clip, worker)
                    else {
                        continue;
                    };
                    (ges_clip, ges::TrackType::VIDEO, None)
                }
            };
            has_clips = true;
            if track_types.contains(ges::TrackType::VIDEO) {
                active_video.insert(clip.clip_id, ActiveVideo::new(&ges_clip, clip, natural_size, quality, worker)?);
            }
            if track_types.contains(ges::TrackType::AUDIO) {
                audio_dsp.attach_clip_audio(&ges_clip, clip, &mut audio_meter);
            }
        }

        // Faça o commit com a timeline em Null, antes de iniciar o preroll.
        if has_clips && !timeline.commit_sync() {
            return Err(PipelineError::CommitFailed);
        }
        audio_dsp.finalize_after_commit();

        let video_sink = PreviewVideoSink::new(quality, Arc::clone(worker))?;
        let (audio_bin, master_volume) = build_audio_preview_sink()?;
        let pipeline = ges::Pipeline::new();
        pipeline
            .set_timeline(&timeline)
            .map_err(|err| setup_error("set_timeline", err))?;
        pipeline
            .set_mode(ges::PipelineFlags::FULL_PREVIEW)
            .map_err(|err| setup_error("set_mode", err))?;
        pipeline.preview_set_video_sink(Some(video_sink.bin()));
        pipeline.preview_set_audio_sink(Some(&audio_bin));

        Ok(Self {
            pipeline,
            video_sink,
            audio_dsp,
            audio_meter,
            has_clips,
            active_video,
            video_track,
            master_volume,
        })
    }

    pub(crate) fn video_dimensions(&self) -> Option<[u32; 2]> {
        let caps = self.video_track.static_pad("src")?.current_caps()?;
        let info = gstreamer_video::VideoInfo::from_caps(&caps).ok()?;
        Some([info.width(), info.height()])
    }

    pub(crate) fn mute_master(&self, muted: bool) {
        self.master_volume.set_property("mute", muted);
    }

    pub(crate) fn set_master_volume(&self, volume: f32) {
        self.master_volume.set_property("volume", f64::from(volume));
        self.master_volume
            .set_property("mute", volume <= f32::EPSILON);
    }

    pub(crate) fn set_state(&self, state: gst::State) -> Result<(), PipelineError> {
        self.pipeline
            .set_state(state)
            .map(|_| ())
            .map_err(|err| PipelineError::StateChange {
                target: format!("{state:?}"),
                reason: err.to_string(),
            })
    }
}

impl Drop for PlaybackGraph {
    fn drop(&mut self) {
        if self.pipeline.current_state() != gst::State::Null {
            if let Err(err) = self.set_state(gst::State::Null) {
                warn!(error = %err, "falha ao liberar pipeline GES de preview");
            }
        }
    }
}

fn setup_error(step: &'static str, err: impl ToString) -> PipelineError {
    PipelineError::TimelineSetup {
        step,
        reason: err.to_string(),
    }
}

struct LayerSlot {
    kind: TrackKind,
    layer: ges::Layer,
    occupied: Vec<(u64, u64)>,
}

fn acquire_layer(
    timeline: &ges::Timeline,
    slots: &mut Vec<LayerSlot>,
    item: &PreparedClip,
) -> ges::Layer {
    let clip = &item.render;
    let start_ns = clip.timeline_start_ns;
    let end_ns = start_ns.saturating_add(clip.duration_ns);
    if let Some(slot) = slots.iter_mut().find(|slot| {
        slot.kind == clip.track_kind
            && !slot
                .occupied
                .iter()
                .any(|&(s, e)| start_ns < e && s < end_ns)
    }) {
        slot.occupied.push((start_ns, end_ns));
        return slot.layer.clone();
    }

    let layer = timeline.append_layer();
    slots.push(LayerSlot {
        kind: clip.track_kind,
        layer: layer.clone(),
        occupied: vec![(start_ns, end_ns)],
    });
    layer
}
