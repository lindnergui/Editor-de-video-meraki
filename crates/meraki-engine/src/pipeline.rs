use crate::audio_dsp::AudioDspController;
use crate::clip_prepare;
pub use crate::clip_prepare::PipelineError;
use crate::playback_graph::PlaybackGraph;
use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip, TrackAudioMix};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use meraki_core::PreviewQuality;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

/// Controla a reprodução e mantém o cache de mídia entre reconstruções do pipeline.
pub(crate) struct PlaybackEngine {
    graph: PlaybackGraph,
    worker: Arc<EngineWorkerChannel>,
    asset_cache: HashMap<String, ges::UriClipAsset>,
    playback_rate: f64,
    master_volume: f32,
    is_playing: bool,
    last_position_ns: u64,
    pending_seek_ns: Option<u64>,
}

impl PlaybackEngine {
    pub(crate) fn new(worker: Arc<EngineWorkerChannel>) -> Result<Self, PipelineError> {
        let graph = PlaybackGraph::new(&[], Vec::new(), PreviewQuality::default(), &worker)?;
        Ok(Self {
            graph,
            worker,
            asset_cache: HashMap::new(),
            playback_rate: 1.0,
            master_volume: 1.0,
            is_playing: false,
            last_position_ns: 0,
            pending_seek_ns: None,
        })
    }

    pub(crate) fn audio_dsp_mut(&mut self) -> &mut AudioDspController {
        &mut self.graph.audio_dsp
    }

    pub(crate) async fn sync_timeline(
        &mut self,
        clips: Vec<TimelineRenderClip>,
        audio_mix: Vec<TrackAudioMix>,
        preview_quality: PreviewQuality,
    ) -> Result<(), PipelineError> {
        let prepared = clip_prepare::prepare_clips(&mut self.asset_cache, clips).await?;
        // Cada pipeline GES contém seu próprio playsink e streamsynchronizer.
        // Reiniciar o mesmo pipeline durante o preroll pode recriar pads ainda presentes.
        let next = PlaybackGraph::new(&prepared, audio_mix, preview_quality, &self.worker)?;
        next.set_master_volume(self.master_volume);

        let was_playing = self.is_playing;
        let resume_pos_ns = self.position_for_seek();
        self.graph.set_state(gst::State::Null)?;
        self.graph = next;
        self.is_playing = false;
        self.pending_seek_ns = None;

        if !self.graph.has_clips {
            self.last_position_ns = 0;
            return Ok(());
        }

        if was_playing {
            self.play()?;
        } else {
            self.pause()?;
        }
        if resume_pos_ns > 0 || self.playback_rate != 1.0 {
            self.queue_seek(resume_pos_ns);
        }
        Ok(())
    }

    pub(crate) fn play(&mut self) -> Result<(), PipelineError> {
        if self.graph.has_clips {
            self.graph.set_state(gst::State::Playing)?;
        }
        self.is_playing = true;
        Ok(())
    }

    pub(crate) fn pause(&mut self) -> Result<(), PipelineError> {
        if self.graph.has_clips {
            self.graph.set_state(gst::State::Paused)?;
        }
        self.is_playing = false;
        Ok(())
    }

    pub(crate) fn set_playback_rate(&mut self, rate: f64) -> Result<(), PipelineError> {
        if !rate.is_finite() || rate.abs() < 0.05 || rate.abs() > 16.0 {
            return Err(PipelineError::InvalidParameter {
                name: "playback_rate",
                reason: "taxa deve ser finita com magnitude entre 0.05 e 16.0",
            });
        }
        self.playback_rate = rate;
        if self.graph.has_clips {
            let position_ns = self.position_for_seek();
            self.queue_seek(position_ns);
        }
        Ok(())
    }

    pub(crate) fn set_master_volume(&mut self, volume: f32) -> Result<(), PipelineError> {
        if !volume.is_finite() {
            return Err(PipelineError::InvalidParameter {
                name: "master_volume",
                reason: "volume deve ser um número finito",
            });
        }
        self.master_volume = volume.clamp(0.0, 1.0);
        self.graph.set_master_volume(self.master_volume);
        Ok(())
    }

    pub(crate) fn set_preview_quality(&mut self, quality: PreviewQuality) {
        self.graph.video_sink.set_preview_quality(quality);
    }

    pub(crate) fn queue_seek(&mut self, position_ns: u64) {
        self.last_position_ns = position_ns;
        self.pending_seek_ns = self.graph.has_clips.then_some(position_ns);
    }

    pub(crate) fn flush_pending_seek(&mut self) -> Result<(), PipelineError> {
        let Some(position_ns) = self.pending_seek_ns else {
            return Ok(());
        };
        if !self.can_seek() {
            return Ok(());
        }
        self.pending_seek_ns = None;

        let target = gst::ClockTime::from_nseconds(position_ns);
        let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT;
        let (start, stop) = if self.playback_rate >= 0.0 {
            (target, gst::ClockTime::NONE)
        } else {
            (gst::ClockTime::ZERO, Some(target))
        };

        self.graph
            .pipeline
            .seek(
                self.playback_rate,
                flags,
                gst::SeekType::Set,
                Some(start),
                gst::SeekType::Set,
                stop,
            )
            .map_err(|err| PipelineError::SeekFailed {
                position_ns,
                reason: err.to_string(),
            })
    }

    fn can_seek(&self) -> bool {
        let (result, current, pending) = self.graph.pipeline.state(Some(gst::ClockTime::ZERO));
        // O estado atual pode continuar Paused durante um seek FLUSH em preroll.
        result == Ok(gst::StateChangeSuccess::Success)
            && current >= gst::State::Paused
            && pending == gst::State::VoidPending
    }

    fn position_for_seek(&mut self) -> u64 {
        self.pending_seek_ns
            .or_else(|| self.query_position_ns())
            .unwrap_or(self.last_position_ns)
    }

    pub(crate) fn query_position_ns(&mut self) -> Option<u64> {
        if self.pending_seek_ns.is_some() || !self.can_seek() {
            return None;
        }
        let pos = self
            .graph
            .pipeline
            .query_position::<gst::ClockTime>()?
            .nseconds();
        self.last_position_ns = pos;
        Some(pos)
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub(crate) fn poll_bus(&mut self, worker: &EngineWorkerChannel) {
        let Some(bus) = self.graph.pipeline.bus() else {
            return;
        };
        while let Some(msg) = bus.pop() {
            match msg.view() {
                gst::MessageView::Error(err) => {
                    self.is_playing = false;
                    self.pending_seek_ns = None;
                    let src = err.src().map(|s| s.path_string()).unwrap_or_default();
                    let detail = err.debug().map(|d| d.to_string()).unwrap_or_default();
                    worker.emit_critical(EngineEvent::Error(format!(
                        "erro no pipeline GES ({src}): {} ({detail})",
                        err.error()
                    )));
                }
                gst::MessageView::Eos(_) => {
                    if let Err(err) = self.pause() {
                        worker.emit_critical(EngineEvent::Error(err.to_string()));
                    }
                    debug!("fim da timeline alcançado (EOS)");
                }
                gst::MessageView::Element(element_msg) => {
                    self.graph
                        .audio_meter
                        .handle_element_message(element_msg, worker);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if let Err(err) = self.graph.set_state(gst::State::Null) {
            warn!(error = %err, "falha ao encerrar pipeline GES de preview");
        }
    }
}
