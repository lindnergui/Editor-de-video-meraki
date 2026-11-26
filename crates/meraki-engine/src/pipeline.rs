use crate::audio_dsp::AudioDspController;
use crate::clip_prepare;
pub use crate::clip_prepare::PipelineError;
use crate::live_commands::ClipPatch;
use crate::playback_graph::PlaybackGraph;
use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip, TrackAudioMix};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use meraki_core::{ClipId, PreviewQuality, TrackKind};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

mod transport;
#[cfg(test)]
mod tests;
use transport::ReverseClock;

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
    reverse: Option<ReverseClock>,
    pending_shuttle_rate: Option<f64>,
    clips: Vec<TimelineRenderClip>,
    preview_quality: PreviewQuality,
    refresh_pending: bool,
    last_refresh: Option<Instant>,
    rate_needs_seek: bool,
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
            reverse: None,
            pending_shuttle_rate: None,
            clips: Vec::new(),
            preview_quality: PreviewQuality::default(),
            refresh_pending: false,
            last_refresh: None,
            rate_needs_seek: false,
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
        if self.reverse.is_some() {
            next.mute_master(true);
        }

        let was_playing = self.is_playing;
        let resume_pos_ns = self.position_for_seek();
        self.graph.set_state(gst::State::Null)?;
        self.graph = next;
        self.clips = prepared.into_iter().map(|item| item.render).collect();
        self.preview_quality = preview_quality;
        self.is_playing = false;
        self.pending_seek_ns = None;
        self.refresh_pending = false;
        self.rate_needs_seek = false;

        if !self.graph.has_clips {
            self.last_position_ns = 0;
            if self.stop_reverse() || was_playing {
                self.emit_shuttle_rate();
            }
            return Ok(());
        }

        let state = if was_playing { gst::State::Playing } else { gst::State::Paused };
        self.graph.set_state(state)?;
        self.is_playing = was_playing;
        if resume_pos_ns > 0 || self.playback_rate != 1.0 {
            self.queue_seek(resume_pos_ns);
        }
        Ok(())
    }

    pub(crate) async fn update_clips_live(&mut self, patches: HashMap<ClipId, ClipPatch>) -> Result<(), PipelineError> {
        let mut changes = Vec::with_capacity(patches.len());
        let mut rebuild = false;
        for (clip_id, patch) in patches {
            let Some(index) = self.clips.iter().position(|clip| clip.clip_id == clip_id) else {
                self.worker.emit_critical(EngineEvent::TimelineSyncRequired { clip_id });
                continue;
            };
            let previous = &self.clips[index];
            let mut next = previous.clone();
            patch.apply(&mut next)?;
            if next.title.is_some() && next.track_kind != TrackKind::Overlay {
                return Err(PipelineError::LiveProperty { clip_id, property: "título só é válido em Overlay" });
            }
            if next != *previous {
                rebuild |= self.graph.active_video.get(&clip_id)
                    .map_or(next.title.is_some(), |video| !video.supports_update(previous, &next));
                changes.push((index, next));
            }
        }
        if rebuild {
            let mut snapshot = self.clips.clone();
            for (index, clip) in changes {
                snapshot[index] = clip;
            }
            let mix = self.graph.audio_dsp.settings_snapshot();
            return self.sync_timeline(snapshot, mix, self.preview_quality).await;
        }
        for (index, clip) in changes {
            if let Some(video) = self.graph.active_video.get(&clip.clip_id) {
                video.apply_live(&self.clips[index], &clip)?;
                self.refresh_pending = !self.is_playing;
            }
            self.clips[index] = clip;
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
        if self.reverse.is_some() {
            self.graph.mute_master(true);
        }
        Ok(())
    }

    pub(crate) fn set_preview_quality(&mut self, quality: PreviewQuality) {
        self.preview_quality = quality;
        self.graph.video_sink.set_preview_quality(quality);
    }

    pub(crate) fn queue_seek(&mut self, position_ns: u64) {
        self.last_position_ns = position_ns;
        self.pending_seek_ns = self.graph.has_clips.then_some(position_ns);
    }

    fn can_seek(&self) -> bool {
        let (result, current, pending) = self.graph.pipeline.state(Some(gst::ClockTime::ZERO));
        // O estado atual pode continuar Paused durante um seek FLUSH em preroll.
        result == Ok(gst::StateChangeSuccess::Success)
            && current >= gst::State::Paused
            && pending == gst::State::VoidPending
    }

    fn position_for_seek(&mut self) -> u64 {
        if self.reverse.is_some() {
            return self.last_position_ns;
        }
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
        for _ in 0..128 {
            let Some(msg) = bus.pop() else { break };
            match msg.view() {
                gst::MessageView::Error(err) => {
                    self.is_playing = false;
                    self.pending_seek_ns = None;
                    self.refresh_pending = false;
                    self.stop_reverse();
                    self.emit_shuttle_rate();
                    let src = err.src().map(|s| s.path_string()).unwrap_or_default();
                    let detail = err.debug().map(|d| d.to_string()).unwrap_or_default();
                    worker.emit_critical(EngineEvent::Error(format!(
                        "erro no pipeline GES ({src}): {} ({detail})",
                        err.error()
                    )));
                }
                gst::MessageView::Warning(warning) => {
                    let source = warning.src().map(|src| src.path_string()).unwrap_or_default();
                    warn!(%source, warning = %warning.error(), detail = ?warning.debug(), "aviso do pipeline de preview");
                }
                gst::MessageView::StateChanged(state)
                    if state.src() == Some(self.graph.pipeline.upcast_ref()) =>
                {
                    debug!(current = ?state.current(), pending = ?state.pending(), "estado do pipeline de preview mudou");
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

    pub(crate) fn publish_title_bounds(&mut self) {
        if self.clips.iter().all(|clip| clip.title.is_none()) || !self.can_seek() {
            return;
        }
        let Some(canvas) = self.graph.video_dimensions() else { return };
        let position = self.last_position_ns;
        for clip in self.clips.iter().filter(|clip| clip.title.is_some()) {
            if position < clip.timeline_start_ns || position >= clip.timeline_start_ns.saturating_add(clip.duration_ns) {
                continue;
            }
            let Some(video) = self.graph.active_video.get_mut(&clip.clip_id) else { continue };
            let bounds = video.title_bounds(canvas);
            if video.last_title_bounds != bounds
                && self.worker.try_emit(EngineEvent::TitleBounds { clip_id: clip.clip_id, bounds })
            {
                video.last_title_bounds = bounds;
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if let Err(err) = self.graph.set_state(gst::State::Null) {
            warn!(error = %err, "falha ao encerrar pipeline GES de preview");
        }
    }
}
