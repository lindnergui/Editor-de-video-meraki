use crate::audio_dsp::AudioDspController;
use crate::audio_meter::AudioMeterAggregator;
use crate::clip_prepare::{PipelineError, PreparedClip, PreparedSource};
use crate::effects_ges;
use crate::protocol::{EngineWorkerChannel, TimelineRenderClip, TrackAudioMix};
use crate::timeline_sync::ActiveVideo;
use crate::transition_builder::{attach_curves, fragment_style, TransitionPlan};
use gstreamer as gst;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipId, PreviewQuality, TrackId, TrackKind};
use std::collections::HashMap;

pub(crate) struct GraphContents {
    pub videos: HashMap<ClipId, TimelineVideo>,
    pub audio_dsp: AudioDspController,
    pub audio_meter: AudioMeterAggregator,
    pub populated: bool,
}

pub(crate) struct TimelineVideo {
    pub primary: ActiveVideo,
    fragments: Vec<ActiveVideo>,
}

impl TimelineVideo {
    pub(crate) fn supports_update(
        &self,
        previous: &TimelineRenderClip,
        next: &TimelineRenderClip,
    ) -> bool {
        self.primary.supports_update(previous, next)
            && (self.fragments.is_empty() || {
                let previous = fragment_style(previous);
                let next = fragment_style(next);
                self.fragments
                    .iter()
                    .all(|video| video.supports_update(&previous, &next))
            })
    }

    pub(crate) fn apply_live(
        &self,
        previous: &TimelineRenderClip,
        next: &TimelineRenderClip,
    ) -> Result<(), PipelineError> {
        self.primary.apply_live(previous, next)?;
        if !self.fragments.is_empty() {
            let previous = fragment_style(previous);
            let next = fragment_style(next);
            for fragment in &self.fragments {
                fragment.apply_live(&previous, &next)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn clip_track_types(item: &PreparedClip, audio_only: bool) -> ges::TrackType {
    match (&item.source, audio_only) {
        (PreparedSource::Media { track_types, .. }, false) => *track_types,
        (PreparedSource::Media { track_types, .. }, true)
            if item.render.track_kind != TrackKind::Overlay =>
        {
            *track_types & ges::TrackType::AUDIO
        }
        (PreparedSource::Title, false) => ges::TrackType::VIDEO,
        _ => ges::TrackType::empty(),
    }
}

pub(crate) fn populate(
    timeline: &ges::Timeline,
    clips: &[PreparedClip],
    transitions: &[meraki_core::TimelineTransition],
    quality: PreviewQuality,
    audio_only: bool,
    audio_mix: Vec<TrackAudioMix>,
    worker: &EngineWorkerChannel,
) -> Result<GraphContents, PipelineError> {
    let plan = if audio_only {
        TransitionPlan::default()
    } else {
        TransitionPlan::new(clips, transitions)
    };
    let mut audio_dsp = AudioDspController::new();
    let mut audio_meter = AudioMeterAggregator::new();
    let mut meter_tracks: Vec<_> = audio_mix.iter().map(|mix| mix.track_id).collect();
    for item in clips {
        if !meter_tracks.contains(&item.render.track_id) {
            meter_tracks.push(item.render.track_id);
        }
    }
    audio_meter.reset_for_sync(&meter_tracks);
    audio_dsp.prepare_sync(audio_mix)?;

    let mut layers = HashMap::<TrackId, TrackLayers>::new();
    let mut videos = HashMap::new();
    let mut populated = false;
    for item in clips {
        let mask = clip_track_types(item, audio_only);
        if mask.is_empty() {
            continue;
        }
        let clip = &item.render;
        let slots = layers.entry(clip.track_id).or_insert_with(|| TrackLayers {
            fragments: plan
                .fragments
                .contains_key(&clip.track_id)
                .then(|| timeline.append_layer()),
            clips: Vec::new(),
        });
        let layer = slots.layer_for(timeline, clip);
        let Some(ges_clip) = add_clip(&layer, item, clip, mask, worker)? else {
            continue;
        };
        populated = true;
        if mask.contains(ges::TrackType::VIDEO) {
            let source = ActiveVideo::new(&ges_clip, clip, natural_size(item), quality, worker)?;
            if let Some(curves) = plan.curves.get(&clip.clip_id) {
                attach_curves(&ges_clip, clip.duration_ns, curves)?;
            }
            videos.insert(
                clip.clip_id,
                TimelineVideo {
                    primary: source,
                    fragments: Vec::new(),
                },
            );
        }
        if mask.contains(ges::TrackType::AUDIO) {
            audio_dsp.attach_clip_audio(&ges_clip, clip, &mut audio_meter);
        }
    }
    for (track, fragments) in plan.fragments {
        let Some(layer) = layers
            .get(&track)
            .and_then(|slots| slots.fragments.as_ref())
        else {
            continue;
        };
        for fragment in fragments {
            let origin = &clips[fragment.origin];
            let timing = fragment.render_clip(origin);
            let Some(clip) = add_clip(layer, origin, &timing, ges::TrackType::VIDEO, worker)?
            else {
                continue;
            };
            let style = fragment_style(&origin.render);
            let video = ActiveVideo::new(&clip, &style, natural_size(origin), quality, worker)?;
            attach_curves(&clip, timing.duration_ns, &[fragment.alpha()])?;
            if let Some(active) = videos.get_mut(&origin.render.clip_id) {
                active.fragments.push(video);
            }
        }
    }
    Ok(GraphContents {
        videos,
        audio_dsp,
        audio_meter,
        populated,
    })
}

fn add_clip(
    layer: &ges::Layer,
    item: &PreparedClip,
    clip: &TimelineRenderClip,
    mask: ges::TrackType,
    worker: &EngineWorkerChannel,
) -> Result<Option<ges::Clip>, PipelineError> {
    match &item.source {
        PreparedSource::Title => Ok(effects_ges::add_title_clip(layer, clip, worker)),
        PreparedSource::Media {
            asset, is_image, ..
        } => {
            let inpoint = if *is_image {
                gst::ClockTime::ZERO
            } else {
                gst::ClockTime::from_nseconds(clip.source_in_ns)
            };
            layer
                .add_asset(
                    asset,
                    gst::ClockTime::from_nseconds(clip.timeline_start_ns),
                    inpoint,
                    gst::ClockTime::from_nseconds(clip.duration_ns),
                    mask,
                )
                .map(Some)
                .map_err(|err| PipelineError::AddAssetClip {
                    path: clip.path.clone(),
                    reason: err.to_string(),
                })
        }
    }
}

fn natural_size(item: &PreparedClip) -> Option<(i32, i32)> {
    match &item.source {
        PreparedSource::Title => None,
        PreparedSource::Media { natural_size, .. } => *natural_size,
    }
}

struct TrackLayers {
    fragments: Option<ges::Layer>,
    clips: Vec<LayerSlot>,
}

struct LayerSlot {
    layer: ges::Layer,
    occupied: Vec<(u64, u64)>,
}

impl TrackLayers {
    fn layer_for(&mut self, timeline: &ges::Timeline, clip: &TimelineRenderClip) -> ges::Layer {
        let start = clip.timeline_start_ns;
        let end = start.saturating_add(clip.duration_ns);
        if let Some(slot) = self
            .clips
            .iter_mut()
            .find(|slot| !slot.occupied.iter().any(|&(a, b)| a < end && start < b))
        {
            slot.occupied.push((start, end));
            return slot.layer.clone();
        }
        let layer = timeline.append_layer();
        self.clips.push(LayerSlot {
            layer: layer.clone(),
            occupied: vec![(start, end)],
        });
        layer
    }
}
