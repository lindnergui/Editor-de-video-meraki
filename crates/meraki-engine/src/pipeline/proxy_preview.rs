use super::{PipelineError, PlaybackEngine};
use crate::protocol::EngineEvent;
use crate::proxy_manager::{ProxyNotice, ProxyQueue};
use crossbeam_channel::Receiver;
use meraki_core::ProxyMode;

impl PlaybackEngine {
    async fn rebuild_preview(&mut self) -> Result<(), PipelineError> {
        self.sync_timeline(
            self.clips.clone(),
            self.transitions.clone(),
            self.graph.audio_dsp.settings_snapshot(),
            self.preview_quality,
        )
        .await
    }

    pub(crate) async fn set_proxy_mode(&mut self, mode: ProxyMode) -> Result<(), PipelineError> {
        let previous = self.proxy_mode;
        self.proxy_mode = mode;
        if (previous == ProxyMode::Off) != (mode == ProxyMode::Off) && !self.clips.is_empty() {
            if let Err(err) = self.rebuild_preview().await {
                self.proxy_mode = previous;
                return Err(err);
            }
        }
        Ok(())
    }

    pub(crate) async fn clear_proxy_cache(
        &mut self,
        queue: &ProxyQueue,
    ) -> Result<(), PipelineError> {
        self.cache_clears_pending += 1;
        // Fecha os readers de preview antes de excluir os arquivos, inclusive no Windows.
        if !self.selected_proxies.is_empty() {
            if let Err(err) = self.rebuild_preview().await {
                self.cache_clears_pending -= 1;
                return Err(err);
            }
        }
        let originals = self.clips.iter().map(|clip| clip.path.clone()).collect();
        if let Err(err) = queue.clear(originals) {
            self.cache_clears_pending -= 1;
            self.rebuild_preview().await?;
            return Err(PipelineError::TimelineSetup {
                step: "clear_proxy_cache",
                reason: err.to_string(),
            });
        }
        Ok(())
    }

    pub(crate) async fn poll_proxies(
        &mut self,
        events: &Receiver<ProxyNotice>,
    ) -> Result<(), PipelineError> {
        let mut rebuild = false;
        for notice in events.try_iter().take(64) {
            match notice {
                ProxyNotice::Event(event) => self.worker.emit_critical(event),
                ProxyNotice::Ready {
                    asset_id,
                    source,
                    proxy,
                } => {
                    if self.proxy_mode != ProxyMode::Off {
                        rebuild |= self.clips.iter().any(|clip| {
                            clip.path == source
                                && self.selected_proxies.get(&clip.clip_id) != Some(&proxy)
                        });
                        self.worker.emit_critical(EngineEvent::ProxyReady {
                            asset_id,
                            proxy_path: proxy,
                        });
                    }
                }
                ProxyNotice::Cleared { freed_bytes, error } => {
                    self.cache_clears_pending = self.cache_clears_pending.saturating_sub(1);
                    for clip in &mut self.clips {
                        clip.proxy_path = None;
                    }
                    self.worker
                        .emit_critical(EngineEvent::ProxyCacheCleared { freed_bytes });
                    if let Some(error) = error {
                        self.worker.emit_critical(EngineEvent::Error(format!(
                            "falha ao limpar parte do cache de proxies: {error}"
                        )));
                    }
                    rebuild = false;
                }
            }
        }
        if rebuild && self.cache_clears_pending == 0 {
            self.rebuild_preview().await?;
        }
        Ok(())
    }
}
