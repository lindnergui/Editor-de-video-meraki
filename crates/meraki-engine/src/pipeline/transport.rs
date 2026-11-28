use super::{PipelineError, PlaybackEngine};
use crate::protocol::EngineEvent;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::{Duration, Instant};

const REVERSE_INTERVAL: Duration = Duration::from_millis(55);
const REFRESH_INTERVAL: Duration = Duration::from_millis(32);

pub(super) struct ReverseClock {
    rate: f64,
    last_step: Instant,
}

impl PlaybackEngine {
    pub(crate) fn play(&mut self) -> Result<(), PipelineError> {
        let position = self.position_for_seek();
        let was_reversing = self.stop_reverse();
        if self.graph.has_clips {
            self.graph.set_state(gst::State::Playing)?;
        }
        self.is_playing = self.graph.has_clips;
        if was_reversing || std::mem::take(&mut self.rate_needs_seek) {
            self.queue_seek(position);
        }
        self.emit_shuttle_rate();
        Ok(())
    }

    pub(crate) fn pause(&mut self) -> Result<(), PipelineError> {
        if self.graph.has_clips {
            self.graph.set_state(gst::State::Paused)?;
        }
        self.is_playing = false;
        self.stop_reverse();
        self.emit_shuttle_rate();
        Ok(())
    }

    pub(crate) fn shuttle(&mut self, direction: i8) -> Result<(), PipelineError> {
        match direction {
            -1 => {
                let rate = self
                    .reverse
                    .as_ref()
                    .map_or(1.0, |reverse| (reverse.rate * 2.0).min(4.0));
                self.start_reverse(rate)
            }
            0 => {
                self.rate_needs_seek |= self.playback_rate != 1.0;
                self.playback_rate = 1.0;
                self.pause()
            }
            1 => {
                let position = self.position_for_seek();
                let rate = if self.is_playing {
                    if self.playback_rate < 2.0 {
                        2.0
                    } else {
                        4.0
                    }
                } else {
                    1.0
                };
                self.playback_rate = rate;
                self.queue_seek(position);
                self.play()
            }
            _ => Err(PipelineError::InvalidParameter {
                name: "direction",
                reason: "use -1, 0 ou 1",
            }),
        }
    }

    pub(crate) fn set_playback_rate(&mut self, rate: f64) -> Result<(), PipelineError> {
        if !rate.is_finite() || !(0.05..=16.0).contains(&rate.abs()) {
            return Err(PipelineError::InvalidParameter {
                name: "playback_rate",
                reason: "taxa deve ser finita com magnitude entre 0.05 e 16.0",
            });
        }
        if rate < 0.0 {
            return self.start_reverse(-rate);
        }
        let position = self.position_for_seek();
        self.stop_reverse();
        self.playback_rate = rate;
        self.queue_seek(position);
        self.emit_shuttle_rate();
        Ok(())
    }

    fn start_reverse(&mut self, rate: f64) -> Result<(), PipelineError> {
        if !self.graph.has_clips {
            self.emit_shuttle_rate();
            return Ok(());
        }
        let position = self.position_for_seek();
        self.graph.set_state(gst::State::Paused)?;
        self.is_playing = false;
        self.last_position_ns = position;
        self.playback_rate = 1.0;
        self.graph.mute_master(true);
        self.reverse = Some(ReverseClock {
            rate,
            last_step: Instant::now(),
        });
        self.emit_shuttle_rate();
        Ok(())
    }

    pub(super) fn stop_reverse(&mut self) -> bool {
        let stopped = self.reverse.take().is_some();
        if stopped {
            self.graph.set_master(self.master, false);
        }
        stopped
    }

    pub(super) fn emit_shuttle_rate(&mut self) {
        self.pending_shuttle_rate = Some(match &self.reverse {
            Some(reverse) => -reverse.rate,
            None if self.is_playing => self.playback_rate,
            None => 0.0,
        });
        self.publish_shuttle_rate();
    }

    pub(crate) fn publish_shuttle_rate(&mut self) {
        if let Some(shuttle_rate) = self.pending_shuttle_rate {
            if self
                .worker
                .try_emit(EngineEvent::ShuttleRateChanged { shuttle_rate })
            {
                self.pending_shuttle_rate = None;
            }
        }
    }

    /// Prioriza o seek da UI. O relógio de reverso e o refresh usam o mesmo
    /// ponto de despacho, depois de o preroll anterior terminar.
    pub(crate) fn flush_pending_seek(&mut self) -> Result<(), PipelineError> {
        if self.pending_seek_ns.is_none() && self.reverse.is_none() && !self.refresh_pending {
            return Ok(());
        }
        if !self.can_seek() {
            return Ok(());
        }
        let now = Instant::now();
        let key_unit = gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT;
        if let Some(position) = self.pending_seek_ns.take() {
            self.rate_needs_seek = false;
            if let Some(reverse) = &mut self.reverse {
                reverse.last_step = now;
            }
            self.refresh_pending = false;
            return self.seek_positive(position, self.playback_rate, key_unit);
        }
        if let Some(reverse) = &mut self.reverse {
            let elapsed = now.duration_since(reverse.last_step);
            if elapsed < REVERSE_INTERVAL {
                return Ok(());
            }
            let distance = (elapsed.as_nanos() as f64 * reverse.rate) as u64;
            let position = self.last_position_ns.saturating_sub(distance);
            reverse.last_step = now;
            let result = self.seek_positive(position, 1.0, key_unit);
            if position == 0 || result.is_err() {
                self.stop_reverse();
                self.emit_shuttle_rate();
            }
            result?;
            self.last_position_ns = position;
            self.refresh_pending = false;
            self.worker
                .emit_drop_if_full(EngineEvent::PlayheadTick(position));
            return Ok(());
        }
        if self.refresh_pending
            && self
                .last_refresh
                .is_none_or(|last| now.duration_since(last) >= REFRESH_INTERVAL)
        {
            self.refresh_pending = false;
            if !self.is_playing {
                self.last_refresh = Some(now);
                let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
                self.seek_positive(self.last_position_ns, 1.0, flags)?;
                // O refresh usa um segmento 1x. Mantém a seleção de taxa para o próximo Play.
                self.rate_needs_seek = self.playback_rate != 1.0;
            }
        }
        Ok(())
    }

    fn seek_positive(
        &self,
        position_ns: u64,
        rate: f64,
        flags: gst::SeekFlags,
    ) -> Result<(), PipelineError> {
        let target = gst::ClockTime::from_nseconds(position_ns);
        let pipeline = &self.graph.pipeline;
        let result = if rate == 1.0 {
            pipeline.seek_simple(flags, target)
        } else {
            pipeline.seek(
                rate,
                flags,
                gst::SeekType::Set,
                Some(target),
                gst::SeekType::Set,
                gst::ClockTime::NONE,
            )
        };
        result.map_err(|err| PipelineError::SeekFailed {
            position_ns,
            reason: err.to_string(),
        })
    }
}
