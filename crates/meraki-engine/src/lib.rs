mod audio_dsp;
mod audio_dsp_chain;
mod audio_eq;
mod audio_fader;
mod audio_live;
mod audio_meter;
mod audio_output;
mod audio_spectrum;
mod clip_prepare;
mod discoverer;
mod effects_ges;
mod effects_pixels;
mod encoding_profiles;
mod export_pipeline;
mod exporter;
mod frame_sink;
mod keyframes_ges;
mod live_commands;
mod pipeline;
mod playback_graph;
#[cfg(test)]
mod playback_tests;
pub mod protocol;
mod proxy;
mod proxy_cache;
mod proxy_manager;
mod render_monitor;
mod thumbnailer;
mod timeline_sync;
mod timeline_builder;
mod transition_builder;
mod video_effects;

pub use protocol::{
    AudioEnhancements, AudioMeterFrame, BridgeError, EngineCommand, EngineEvent, EngineHandle, EngineWorkerChannel,
    ProxyMode, SpectrumBands, ThumbnailFrame, TimelineRenderClip, TimelineTransition, TrackAudioLevel, TrackAudioMix, VideoFrame,
};

use crossbeam_channel::{bounded, Receiver, Sender};
use glib::{MainContext, MainLoop};
use meraki_core::AssetId;
use pipeline::PlaybackEngine;
use proxy_manager::{ProxyJob, ProxyManager, ProxyQueue};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info};

static MEDIA_INIT: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Debug)]
struct DiscoverJob {
    asset_id: AssetId,
    path: PathBuf,
    generate_proxy: bool,
}

pub(crate) fn ensure_media_initialized() -> Result<(), String> {
    MEDIA_INIT
        .get_or_init(|| {
            gstreamer::init().map_err(|err| format!("gstreamer::init falhou: {err}"))?;
            gstreamer_editing_services::init().map_err(|err| format!("ges::init falhou: {err}"))?;
            effects_pixels::register().map_err(|err| format!("falha ao registrar filtro de vídeo Meraki: {err}"))
        })
        .clone()
}

/// Inicia o motor multimídia em uma thread dedicada com `glib::MainLoop` isolado
/// e retorna o handle não bloqueante consumido pela interface (`meraki-ui`).
pub fn spawn_engine() -> EngineHandle {
    let (handle, worker) = protocol::create_engine_channel();

    if let Err(err) = thread::Builder::new()
        .name("meraki-engine".to_string())
        .spawn(move || run_engine_thread(worker))
    {
        error!(error = %err, "falha ao criar thread dedicada do motor multimídia");
    }

    handle
}

fn run_engine_thread(worker: EngineWorkerChannel) {
    let worker = Arc::new(worker);
    if let Err(err_msg) = ensure_media_initialized() {
        let message = format!("falha ao inicializar GStreamer/GES: {err_msg}");
        error!("{message}");
        worker.emit_critical(EngineEvent::Error(message));
        return;
    }

    let (mut proxy_manager, proxy_queue, proxy_notices) = ProxyManager::spawn(Arc::clone(&worker));
    let (discover_tx, discover_rx) = bounded::<DiscoverJob>(256);
    let discover_worker_channel = Arc::clone(&worker);
    let discover_proxy_queue = proxy_queue.clone();
    let discover_join = match thread::Builder::new()
        .name("meraki-discoverer".to_string())
        .spawn(move || run_discoverer_worker(discover_rx, discover_worker_channel, discover_proxy_queue))
    {
        Ok(handle) => Some(handle),
        Err(err) => {
            let message = format!("falha ao criar thread de descoberta de mídia: {err}");
            error!("{message}");
            worker.emit_critical(EngineEvent::Error(message));
            None
        }
    };

    let context = MainContext::new();
    let main_loop = MainLoop::new(Some(&context), false);
    let main_loop_ref = main_loop.clone();
    let worker_poll = Arc::clone(&worker);
    let mut discover_tx_slot = Some(discover_tx);

    let ctx_result = context.with_thread_default(|| {
        let mut playback = match PlaybackEngine::new(Arc::clone(&worker_poll)) {
            Ok(engine) => engine,
            Err(err) => {
                let message = format!("falha ao construir pipeline GES de reprodução: {err}");
                error!("{message}");
                worker_poll.emit_critical(EngineEvent::Error(message));
                let _ = discover_tx_slot.take();
                return;
            }
        };

        let context_ref = context.clone();
        let mut exporter = exporter::ExportManager::spawn(Arc::clone(&worker_poll));
        worker_poll.emit_critical(EngineEvent::EngineReady);
        info!("motor multimídia Meraki inicializado em thread dedicada");

        context_ref.spawn_local(async move {
            let mut last_playhead_tick = Instant::now();
            let mut live_edits = live_commands::LiveEdits::default();
            let mut audio_edits = audio_live::AudioEdits::default();
            loop {
                for _ in 0..64 {
                    match worker_poll.try_recv_command_checked() {
                        Ok(Some(EngineCommand::Shutdown)) | Err(BridgeError::EngineDisconnected) => {
                            info!("encerrando loop principal do motor multimídia");
                            proxy_queue.stop();
                            playback.shutdown();
                            exporter.shutdown();
                            let _ = discover_tx_slot.take();
                            main_loop_ref.quit();
                            return;
                        }
                        Ok(Some(cmd)) => {
                            if let Some(cmd) = audio_edits.collect(cmd).and_then(|cmd| live_edits.collect(cmd)) {
                                audio_edits.flush(&mut playback, &worker_poll);
                                live_edits.flush(&mut playback, &worker_poll).await;
                                handle_command(&mut playback, &exporter, &proxy_queue, &worker_poll, discover_tx_slot.as_ref(), cmd).await;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            error!(error = %err, "erro inesperado no canal de comandos");
                            proxy_queue.stop();
                            playback.shutdown();
                            exporter.shutdown();
                            let _ = discover_tx_slot.take();
                            main_loop_ref.quit();
                            return;
                        }
                    }
                }
                audio_edits.flush(&mut playback, &worker_poll);
                live_edits.flush(&mut playback, &worker_poll).await;
                if let Err(err) = playback.poll_proxies(&proxy_notices).await {
                    worker_poll.emit_critical(EngineEvent::Error(err.to_string()));
                }

                playback.poll_bus(&worker_poll);
                playback.publish_shuttle_rate();

                if let Some(seek_ns) = worker_poll.take_pending_seek() {
                    playback.queue_seek(seek_ns);
                    worker_poll.emit_drop_if_full(EngineEvent::PlayheadTick(seek_ns));
                }
                if let Err(err) = playback.flush_pending_seek() {
                    error!(error = %err, "falha ao executar seek");
                    worker_poll.emit_critical(EngineEvent::Error(err.to_string()));
                }

                if last_playhead_tick.elapsed() >= Duration::from_millis(33) {
                    if playback.is_playing() {
                        if let Some(pos_ns) = playback.query_position_ns() {
                            worker_poll.emit_drop_if_full(EngineEvent::PlayheadTick(pos_ns));
                        }
                    }
                    playback.publish_title_bounds();
                    last_playhead_tick = Instant::now();
                }

                glib::timeout_future(Duration::from_millis(16)).await;
            }
        });

        main_loop.run();
    });

    drop(context);
    proxy_manager.shutdown();
    if let Some(handle) = discover_join {
        let _ = handle.join();
    }

    if let Err(err) = ctx_result {
        let message = format!("falha ao ativar GLib MainContext dedicado: {err}");
        error!("{message}");
        worker.emit_critical(EngineEvent::Error(message));
    }
}

fn run_discoverer_worker(
    job_rx: Receiver<DiscoverJob>,
    worker: Arc<EngineWorkerChannel>,
    proxy_queue: ProxyQueue,
) {
    while let Ok(job) = job_rx.recv() {
        if proxy_queue.is_stopping() { break; }
        match discoverer::discover_media(job.asset_id, &job.path) {
            Ok(metadata) => {
                if proxy_queue.is_stopping() { break; }
                debug!(asset_id = %metadata.asset_id, duration_ns = metadata.duration_ns, "mídia descoberta");
                worker.emit_critical(EngineEvent::MediaDiscovered(metadata.clone()));
                if metadata.width.is_some() && metadata.height.is_some() {
                    if let Some(thumb) = thumbnailer::extract_thumbnail(job.asset_id, &job.path, &metadata) {
                        worker.emit_critical(EngineEvent::ThumbnailReady(thumb));
                    }
                }
                if let Some(dimensions) = metadata.width.zip(metadata.height).filter(|_| job.generate_proxy) {
                    proxy_queue.enqueue(ProxyJob {
                        asset_id: job.asset_id,
                        path: job.path,
                        dimensions,
                        metadata: Some(metadata),
                    });
                }
            }
            Err(err) => {
                error!(asset_id = %job.asset_id, path = %job.path.display(), error = %err, "falha ao descobrir mídia");
                worker.emit_critical(EngineEvent::Error(err.to_string()));
            }
        }
    }
}

async fn handle_command(
    playback: &mut PlaybackEngine,
    exporter: &exporter::ExportManager,
    proxies: &ProxyQueue,
    worker: &EngineWorkerChannel,
    discover_tx: Option<&Sender<DiscoverJob>>,
    command: EngineCommand,
) {
    let result = match command {
        EngineCommand::SyncTimeline { clips, transitions, audio_mix, preview_quality } => {
            playback.sync_timeline(clips, transitions, audio_mix, preview_quality).await
        }
        EngineCommand::SetProxyMode(mode) => {
            playback.set_proxy_mode(mode).await.map(|()| proxies.set_mode(mode))
        }
        EngineCommand::EnsureProxy { asset_id, source_path, width, height } => {
            proxies.enqueue(ProxyJob { asset_id, path: source_path, dimensions: (width, height), metadata: None });
            Ok(())
        }
        EngineCommand::ClearProxyCache => playback.clear_proxy_cache(proxies).await,
        EngineCommand::Play => playback.play(),
        EngineCommand::Shuttle { direction } => playback.shuttle(direction),
        EngineCommand::Pause => playback.pause(),
        EngineCommand::SetPlaybackRate(rate) => playback.set_playback_rate(rate),
        EngineCommand::SetMasterVolume(volume) => playback.set_master_volume(volume),
        EngineCommand::RequestAudioSpectrum => {
            playback.publish_audio_spectrum(worker);
            Ok(())
        }
        EngineCommand::SetPreviewQuality(quality) => {
            playback.set_preview_quality(quality);
            Ok(())
        }
        EngineCommand::ImportMedia { asset_id, path, generate_proxy } => {
            debug!(%asset_id, path = %path.display(), generate_proxy, "despachando análise de mídia");
            dispatch_discover_job(worker, discover_tx, asset_id, path, generate_proxy);
            Ok(())
        }
        EngineCommand::SetTrackGain { track_id, gain_db } => {
            playback.audio_dsp_mut().set_track_gain(track_id, gain_db)
        }
        EngineCommand::SetTrackMute { track_id, muted } => {
            playback.audio_dsp_mut().set_track_mute(track_id, muted)
        }
        EngineCommand::SetTrackSolo { track_id, solo } => {
            playback.audio_dsp_mut().set_track_solo(track_id, solo)
        }
        EngineCommand::UpdateEqBand { track_id, band, gain_db } => {
            playback.audio_dsp_mut().update_eq_band(track_id, band, gain_db)
        }
        EngineCommand::SetNoiseReduction { track_id, enabled, strength } => {
            playback.audio_dsp_mut().set_noise_reduction(track_id, enabled, strength)
        }
        EngineCommand::SetVoiceLeveler { track_id, enabled } => {
            playback.audio_dsp_mut().set_voice_leveler(track_id, enabled)
        }
        EngineCommand::ToggleTrackAudioEffect { track_id, effect, enabled } => {
            playback.audio_dsp_mut().toggle_track_audio_effect(track_id, effect, enabled)
        }
        EngineCommand::StartExport { job_id, settings, clips, transitions, audio_tracks } => {
            exporter.enqueue(job_id, settings, clips, transitions, audio_tracks, playback.master_mix());
            Ok(())
        }
        EngineCommand::CancelExport { job_id } => {
            exporter.cancel(job_id);
            Ok(())
        }
        EngineCommand::Shutdown | EngineCommand::UpdateClipLive { .. } | EngineCommand::UpdateAudioLive { .. } => Ok(()),
    };

    if let Err(err) = result {
        error!(error = %err, "erro ao processar comando no pipeline GES");
        worker.emit_critical(EngineEvent::Error(err.to_string()));
    }
}

fn dispatch_discover_job(
    worker: &EngineWorkerChannel,
    discover_tx: Option<&Sender<DiscoverJob>>,
    asset_id: AssetId,
    path: PathBuf,
    generate_proxy: bool,
) {
    match discover_tx {
        Some(tx) => {
            if let Err(err) = tx.try_send(DiscoverJob { asset_id, path, generate_proxy }) {
                worker.emit_critical(EngineEvent::Error(format!(
                    "fila de descoberta indisponível para {asset_id}: {err}"
                )));
            }
        }
        None => worker.emit_critical(EngineEvent::Error(format!(
            "worker de descoberta encerrado para {asset_id}"
        ))),
    }
}
