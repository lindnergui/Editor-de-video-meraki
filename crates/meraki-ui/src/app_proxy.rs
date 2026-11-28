use crate::app::MerakiApp;
use crate::app_preferences::save_preferences_to;
use crate::i18n;
use iced::widget::image;
use meraki_core::{AssetId, MediaKind, PreviewQuality, ProxyMode};
use meraki_engine::{EngineCommand, EngineEvent};
use std::path::PathBuf;

/// Estado do ciclo de vida do proxy 720p para um ativo de mídia na UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyStatus {
    Generating,
    Ready,
    Failed(String),
}

/// Estado agregado de proxy exibido de forma discreta perto do player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyBannerState {
    #[default]
    Hidden,
    Generating { count: usize },
    Ready,
}

/// Avalia localmente se um vídeo deve gerar proxy segundo o `ProxyMode` configurado.
pub const fn should_request_proxy(mode: ProxyMode, width: u32, height: u32) -> bool {
    match mode {
        ProxyMode::Off => false,
        ProxyMode::AutoAbove1080p => width > 1920 || height > 1080,
        ProxyMode::Always => true,
    }
}

impl MerakiApp {
    pub fn proxy_status(&self, asset_id: AssetId) -> Option<&ProxyStatus> {
        self.project_io.proxy_status.get(&asset_id)
    }

    pub fn proxy_banner_state(&self) -> ProxyBannerState {
        let generating = self
            .project_io
            .proxy_status
            .values()
            .filter(|st| matches!(st, ProxyStatus::Generating))
            .count();
        if generating > 0 {
            return ProxyBannerState::Generating { count: generating };
        }
        let has_ready = self.preferences.proxy_mode != ProxyMode::Off
            && (self.project.assets.iter().any(|a| a.proxy_path.is_some())
                || self.project_io.proxy_status.values().any(|st| matches!(st, ProxyStatus::Ready)));
        if has_ready {
            ProxyBannerState::Ready
        } else {
            ProxyBannerState::Hidden
        }
    }

    pub fn status_notice(&self) -> Option<&str> {
        self.project_io.status_notice.as_deref()
    }

    /// Atualiza `preferences.proxy_mode`, persiste em `preferences.json`, notifica o motor e ressincroniza.
    pub fn set_proxy_mode(&mut self, mode: ProxyMode) -> Result<(), String> {
        self.preferences.proxy_mode = mode;
        save_preferences_to(&self.preferences_path, &self.preferences)?;
        if let Err(err) = self.engine.send(EngineCommand::SetProxyMode(mode)) {
            self.last_error = Some(err.to_string());
        }
        if mode != ProxyMode::Off {
            self.ensure_proxies_for_loaded_project();
        }
        self.dispatch_timeline_sync();
        Ok(())
    }

    /// Solicita limpeza completa do cache de proxies em disco pelo motor.
    pub fn clear_proxy_cache(&mut self) {
        if let Err(err) = self.engine.send(EngineCommand::ClearProxyCache) {
            self.last_error = Some(err.to_string());
        }
    }

    pub(crate) fn maybe_ensure_proxy_for_asset(
        &mut self,
        asset_id: AssetId,
        source_path: PathBuf,
        width: u32,
        height: u32,
    ) {
        if !should_request_proxy(self.preferences.proxy_mode, width, height) {
            return;
        }
        if let Err(err) = self.engine.send(EngineCommand::EnsureProxy {
            asset_id,
            source_path,
            width,
            height,
        }) {
            self.last_error = Some(err.to_string());
        }
    }

    pub(crate) fn ensure_proxies_for_loaded_project(&mut self) {
        let requests: Vec<(AssetId, PathBuf, u32, u32)> = self
            .project
            .assets
            .iter()
            .filter(|a| {
                matches!(a.kind, MediaKind::Video)
                    && a.proxy_path.is_none()
                    && a.path.is_file()
                    && should_request_proxy(
                        self.preferences.proxy_mode,
                        a.metadata.width.unwrap_or(0),
                        a.metadata.height.unwrap_or(0),
                    )
            })
            .map(|a| {
                (
                    a.id,
                    a.path.clone(),
                    a.metadata.width.unwrap_or(0),
                    a.metadata.height.unwrap_or(0),
                )
            })
            .collect();

        for (asset_id, source_path, width, height) in requests {
            if let Err(err) = self.engine.send(EngineCommand::EnsureProxy {
                asset_id,
                source_path,
                width,
                height,
            }) {
                self.last_error = Some(err.to_string());
                break;
            }
        }
    }

    pub(crate) fn handle_proxy_and_visual_event(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::EngineReady => self.apply_proxy_preference(),
            EngineEvent::MediaDiscovered(meta) => {
                let target = self
                    .project
                    .asset(meta.asset_id)
                    .filter(|a| matches!(a.kind, MediaKind::Video) && a.proxy_path.is_none() && a.path.is_file())
                    .map(|a| (a.id, a.path.clone(), meta.width.unwrap_or(0), meta.height.unwrap_or(0)));
                if let Some((asset_id, path, w, h)) = target {
                    self.maybe_ensure_proxy_for_asset(asset_id, path, w, h);
                }
            }
            EngineEvent::ThumbnailReady(thumb) => {
                let handle = image::Handle::from_rgba(
                    thumb.width,
                    thumb.height,
                    thumb.rgba_pixels.clone(),
                );
                self.project_io.thumbnails.insert(thumb.asset_id, handle);
            }
            EngineEvent::ProxyGenerating { asset_id } => {
                self.project_io.proxy_status.insert(*asset_id, ProxyStatus::Generating);
                self.project_io.proxy_progress.insert(*asset_id, 0.0);
            }
            EngineEvent::ProxyReady { asset_id, proxy_path } => {
                if let Some(asset) = self.project.asset_mut(*asset_id) {
                    asset.proxy_path = Some(proxy_path.clone());
                }
                self.project_io.proxy_status.insert(*asset_id, ProxyStatus::Ready);
                self.project_io.proxy_progress.remove(asset_id);
                if self.playback.preview_quality == PreviewQuality::Proxy720p {
                    self.dispatch_timeline_sync();
                }
            }
            EngineEvent::ProxyFailed { asset_id, reason } => {
                self.project_io.proxy_status.insert(*asset_id, ProxyStatus::Failed(reason.clone()));
                self.project_io.proxy_progress.remove(asset_id);
            }
            EngineEvent::ProxyCacheCleared { freed_bytes } => {
                self.project_io.proxy_progress.clear();
                self.project_io.proxy_status.clear();
                for asset in &mut self.project.assets {
                    asset.proxy_path = None;
                }
                self.project_io.status_notice = Some(i18n::format_proxy_cache_cleared(
                    self.preferences.language,
                    *freed_bytes,
                ));
                if !self.project.timeline.tracks.iter().all(|t| t.clips.is_empty()) {
                    self.dispatch_timeline_sync();
                }
            }
            EngineEvent::ProxyProgress {
                asset_id,
                progress,
                proxy_path,
            } => {
                if let Some(path) = proxy_path {
                    if let Some(asset) = self.project.asset_mut(*asset_id) {
                        asset.proxy_path = Some(path.clone());
                    }
                    self.project_io.proxy_status.insert(*asset_id, ProxyStatus::Ready);
                    self.project_io.proxy_progress.remove(asset_id);
                    if self.playback.preview_quality == PreviewQuality::Proxy720p {
                        self.dispatch_timeline_sync();
                    }
                } else {
                    self.project_io.proxy_status.insert(*asset_id, ProxyStatus::Generating);
                    self.project_io
                        .proxy_progress
                        .insert(*asset_id, progress.clamp(0.0, 1.0));
                }
            }
            _ => {}
        }
    }
}
