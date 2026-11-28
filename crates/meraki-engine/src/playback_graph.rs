use crate::audio_dsp::AudioDspController;
use crate::audio_fader::AudioFader;
use crate::audio_meter::AudioMeterAggregator;
use crate::audio_output::{float_audio_caps, shorten_mixer_blocks, MasterMix, MIX_RATE};
use crate::audio_spectrum::AudioSpectrumMonitor;
use crate::clip_prepare::{PipelineError, PreparedClip};
use crate::frame_sink::{build_audio_preview_sink, limit_playsink_audio_queue, PreviewVideoSink};
use crate::protocol::{EngineWorkerChannel, TrackAudioMix};
use crate::timeline_builder::{self, TimelineVideo};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipId, PreviewQuality, TimelineTransition};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

pub(crate) struct PlaybackGraph {
    pub(crate) pipeline: ges::Pipeline,
    pub(crate) video_sink: PreviewVideoSink,
    pub(crate) audio_dsp: AudioDspController,
    pub(crate) audio_meter: AudioMeterAggregator,
    pub(crate) spectrum: AudioSpectrumMonitor,
    pub(crate) has_clips: bool,
    pub(crate) duration_ns: u64,
    pub(crate) active_video: HashMap<ClipId, TimelineVideo>,
    video_track: ges::VideoTrack,
    master_volume: AudioFader,
}

impl PlaybackGraph {
    pub(crate) fn new(
        clips: &[PreparedClip],
        transitions: &[TimelineTransition],
        audio_mix: Vec<TrackAudioMix>,
        quality: PreviewQuality,
        worker: &Arc<EngineWorkerChannel>,
    ) -> Result<Self, PipelineError> {
        let timeline = ges::Timeline::new();
        let video_track = ges::VideoTrack::new();
        timeline
            .add_track(&video_track)
            .map_err(|err| setup_error("add_video_track", err))?;
        let audio_track = ges::AudioTrack::new();
        audio_track.set_restriction_caps(&float_audio_caps(MIX_RATE));
        timeline
            .add_track(&audio_track)
            .map_err(|err| setup_error("add_audio_track", err))?;
        shorten_mixer_blocks(&audio_track);
        timeline.set_auto_transition(true);

        let contents = timeline_builder::populate(&timeline, clips, transitions, quality, false, audio_mix, worker)?;
        let has_clips = contents.populated;
        let mut audio_dsp = contents.audio_dsp;

        // Faça o commit com a timeline em Null, antes de iniciar o preroll.
        if has_clips && !timeline.commit_sync() {
            return Err(PipelineError::CommitFailed);
        }
        audio_dsp.finalize_after_commit();
        let duration_ns = timeline.duration().nseconds();

        let video_sink = PreviewVideoSink::new(quality, Arc::clone(worker))?;
        let (audio_bin, master_volume) = build_audio_preview_sink()?;
        let pipeline = ges::Pipeline::new();
        limit_playsink_audio_queue(&pipeline);
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
            audio_meter: contents.audio_meter,
            spectrum: AudioSpectrumMonitor::default(),
            has_clips,
            duration_ns,
            active_video: contents.videos,
            video_track,
            master_volume: AudioFader::new(master_volume),
        })
    }

    pub(crate) fn video_dimensions(&self) -> Option<[u32; 2]> {
        let caps = self.video_track.static_pad("src")?.current_caps()?;
        let info = gstreamer_video::VideoInfo::from_caps(&caps).ok()?;
        Some([info.width(), info.height()])
    }

    pub(crate) fn mute_master(&self, muted: bool) {
        self.master_volume.set_mute(muted);
    }

    pub(crate) fn set_master(&self, master: MasterMix, reverse: bool) {
        self.master_volume.set(master.gain, master.muted || reverse);
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
