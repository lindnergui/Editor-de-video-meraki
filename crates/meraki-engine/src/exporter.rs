use crate::clip_prepare::{self, PreparedSource};
use crate::encoding_profiles;
use crate::export_pipeline::{build_render_pipeline, detect_active_streams, RenderSpec};
use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip, TrackAudioMix};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ExportJobId, ExportSettings, TrackKind};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

struct ExportTask {
    job_id: ExportJobId,
    settings: ExportSettings,
    clips: Vec<TimelineRenderClip>,
    audio_tracks: Vec<TrackAudioMix>,
    cancel_flag: Arc<AtomicBool>,
}

/// Gerencia fila sequencial de exportação em thread dedicada sem bloquear UI nem preview GES.
pub(crate) struct ExportManager {
    tx: Option<Sender<ExportTask>>,
    cancels: Arc<Mutex<HashMap<ExportJobId, Arc<AtomicBool>>>>,
    worker: Arc<EngineWorkerChannel>,
    join: Option<JoinHandle<()>>,
}

impl ExportManager {
    pub(crate) fn spawn(worker: Arc<EngineWorkerChannel>) -> Self {
        let (tx, rx) = bounded::<ExportTask>(64);
        let cancels = Arc::new(Mutex::new(HashMap::new()));
        let thread_cancels = Arc::clone(&cancels);
        let thread_worker = Arc::clone(&worker);

        let join = match thread::Builder::new()
            .name("meraki-exporter".to_string())
            .spawn(move || run_exporter_loop(rx, thread_cancels, thread_worker))
        {
            Ok(handle) => Some(handle),
            Err(err) => {
                error!(error = %err, "falha ao criar thread dedicada de exportação");
                worker.emit_critical(EngineEvent::Error(format!(
                    "falha ao iniciar worker de exportação: {err}"
                )));
                None
            }
        };

        Self { tx: Some(tx), cancels, worker, join }
    }

    pub(crate) fn enqueue(
        &self,
        job_id: ExportJobId,
        settings: ExportSettings,
        clips: Vec<TimelineRenderClip>,
        audio_tracks: Vec<TrackAudioMix>,
    ) {
        let Some(tx) = &self.tx else {
            self.worker.emit_critical(EngineEvent::ExportFailed {
                job_id,
                error: "worker de exportação já foi encerrado".to_string(),
            });
            return;
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.cancels.lock() {
            guard.insert(job_id, Arc::clone(&cancel_flag));
        }

        let task = ExportTask { job_id, settings, clips, audio_tracks, cancel_flag };
        if let Err(err) = tx.try_send(task) {
            if let Ok(mut guard) = self.cancels.lock() {
                guard.remove(&job_id);
            }
            let reason = match err {
                TrySendError::Full(_) => "fila de exportação cheia (máximo 64 tarefas)",
                TrySendError::Disconnected(_) => "canal do worker de exportação desconectado",
            };
            self.worker.emit_critical(EngineEvent::ExportFailed { job_id, error: reason.to_string() });
        }
    }

    pub(crate) fn cancel(&self, job_id: ExportJobId) {
        if let Ok(guard) = self.cancels.lock() {
            if let Some(flag) = guard.get(&job_id) {
                flag.store(true, Ordering::Release);
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if let Ok(guard) = self.cancels.lock() {
            for flag in guard.values() {
                flag.store(true, Ordering::Release);
            }
        }
        drop(self.tx.take());
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

fn run_exporter_loop(
    rx: Receiver<ExportTask>,
    cancels: Arc<Mutex<HashMap<ExportJobId, Arc<AtomicBool>>>>,
    worker: Arc<EngineWorkerChannel>,
) {
    let context = glib::MainContext::new();
    let _ = context.with_thread_default(|| {
        let mut asset_cache = HashMap::new();
        while let Ok(task) = rx.recv() {
            let job_id = task.job_id;
            execute_export_task(&context, &mut asset_cache, task, &worker);
            if let Ok(mut guard) = cancels.lock() {
                guard.remove(&job_id);
            }
        }
    });
}

fn execute_export_task(
    context: &glib::MainContext,
    asset_cache: &mut HashMap<String, ges::UriClipAsset>,
    task: ExportTask,
    worker: &EngineWorkerChannel,
) {
    let job_id = task.job_id;
    if task.cancel_flag.load(Ordering::Acquire) {
        worker.emit_critical(EngineEvent::ExportCancelled { job_id });
        return;
    }

    if let Err(err) = execute_render_inner(context, asset_cache, task.clone(), worker) {
        if task.cancel_flag.load(Ordering::Acquire) {
            worker.emit_critical(EngineEvent::ExportCancelled { job_id });
        } else {
            worker.emit_critical(EngineEvent::ExportFailed { job_id, error: err });
        }
    }
}

impl Clone for ExportTask {
    fn clone(&self) -> Self {
        Self {
            job_id: self.job_id,
            settings: self.settings.clone(),
            clips: self.clips.clone(),
            audio_tracks: self.audio_tracks.clone(),
            cancel_flag: Arc::clone(&self.cancel_flag),
        }
    }
}

fn execute_render_inner(
    context: &glib::MainContext,
    asset_cache: &mut HashMap<String, ges::UriClipAsset>,
    task: ExportTask,
    worker: &EngineWorkerChannel,
) -> Result<(), String> {
    let job_id = task.job_id;
    let output_path = task.settings.resolved_output_path().map_err(|e| e.to_string())?;
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("falha ao criar diretório '{}': {e}", parent.display()))?;
    }

    let clips = if task.settings.format.is_audio_only() {
        task.clips.iter().filter(|c| c.track_kind != TrackKind::Overlay).cloned().collect()
    } else {
        task.clips
    };
    if clips.is_empty() {
        return Err("nenhum clipe compatível para renderizar na timeline".to_string());
    }

    let mut prepared = context
        .block_on(clip_prepare::prepare_clips(asset_cache, clips))
        .map_err(|e| e.to_string())?;
    if task.settings.format.is_audio_only() {
        prepared.retain(|item| {
            matches!(&item.source, PreparedSource::Media { track_types, .. } if track_types.contains(ges::TrackType::AUDIO))
        });
        if prepared.is_empty() {
            return Err("nenhum fluxo de áudio encontrado para exportação Apenas Áudio".to_string());
        }
    }

    let (has_video, has_audio) = detect_active_streams(&prepared, task.settings.format.is_audio_only());
    const MAX_RENDER_ATTEMPTS: usize = 4;

    for attempt in 0..MAX_RENDER_ATTEMPTS {
        if task.cancel_flag.load(Ordering::Acquire) {
            return Err("exportação cancelada".to_string());
        }

        let resolved = encoding_profiles::build_encoding_profile(&task.settings, has_video, has_audio)
            .map_err(|e| e.to_string())?;
        info!(
            %job_id,
            attempt = attempt + 1,
            path = %output_path.display(),
            muxer = ?resolved.muxer,
            video_enc = ?resolved.video_encoder,
            audio_enc = ?resolved.audio_encoder,
            "iniciando renderização GES dedicada"
        );

        let (timeline, pipeline) = build_render_pipeline(RenderSpec {
            settings: &task.settings,
            audio_tracks: task.audio_tracks.clone(),
            prepared: &prepared,
            has_video,
            has_audio,
            profile: &resolved.profile,
            output_path: &output_path,
            worker,
        })?;
        match monitor_render_bus(job_id, &pipeline, &timeline, &output_path, &task.cancel_flag, worker) {
            Ok(()) => return Ok(()),
            Err(RenderBusError::HardwareEncoderFailed { encoder, .. })
                if attempt + 1 < MAX_RENDER_ATTEMPTS =>
            {
                warn!(
                    %job_id,
                    %encoder,
                    attempt = attempt + 1,
                    "reexecutando renderização com encoder alternativo"
                );
            }
            Err(RenderBusError::HardwareEncoderFailed { detail, .. })
            | Err(RenderBusError::Fatal(detail)) => return Err(detail),
        }
    }

    Err("nenhum encoder compatível concluiu a renderização".to_string())
}

enum RenderBusError {
    HardwareEncoderFailed { encoder: String, detail: String },
    Fatal(String),
}

fn monitor_render_bus(
    job_id: ExportJobId,
    pipeline: &ges::Pipeline,
    timeline: &ges::Timeline,
    output_path: &PathBuf,
    cancel_flag: &AtomicBool,
    worker: &EngineWorkerChannel,
) -> Result<(), RenderBusError> {
    if let Err(err) = pipeline.set_state(gst::State::Playing) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(RenderBusError::Fatal(format!("erro ao iniciar render: {err:?}")));
    }
    let Some(bus) = pipeline.bus() else {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(RenderBusError::Fatal(
            "barramento GStreamer indisponível para renderização".to_string(),
        ));
    };

    let started = Instant::now();
    let mut last_progress = Instant::now();
    loop {
        if cancel_flag.load(Ordering::Acquire) {
            let _ = pipeline.send_event(gst::event::Eos::new());
            let _ = pipeline.set_state(gst::State::Null);
            if let Err(err) = fs::remove_file(output_path) {
                warn!(error = %err, path = %output_path.display(), "não foi possível remover arquivo parcial cancelado");
            }
            worker.emit_critical(EngineEvent::ExportCancelled { job_id });
            return Ok(());
        }

        if let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(100))) {
            match msg.view() {
                gst::MessageView::Eos(_) => {
                    let _ = pipeline.set_state(gst::State::Null);
                    let file_size_bytes = fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
                    worker.emit_drop_if_full(EngineEvent::ExportProgress {
                        job_id,
                        progress: 1.0,
                        eta_seconds: Some(0),
                    });
                    worker.emit_critical(EngineEvent::ExportFinished {
                        job_id,
                        output_path: output_path.clone(),
                        file_size_bytes,
                    });
                    return Ok(());
                }
                gst::MessageView::Error(err) => {
                    let _ = pipeline.set_state(gst::State::Null);
                    let _ = fs::remove_file(output_path);
                    let src = err.src().map(|s| s.path_string()).unwrap_or_default();
                    let detail = err.debug().map(|d| d.to_string()).unwrap_or_default();
                    let msg_text = format!("erro no render ({src}): {} ({detail})", err.error());
                    if let Some(encoder) = encoding_profiles::demote_failing_hardware_encoder(err.src()) {
                        return Err(RenderBusError::HardwareEncoderFailed {
                            encoder,
                            detail: msg_text,
                        });
                    }
                    return Err(RenderBusError::Fatal(msg_text));
                }
                _ => {}
            }
        }

        if last_progress.elapsed() >= Duration::from_millis(100) {
            let total_ns = pipeline
                .query_duration::<gst::ClockTime>()
                .map_or_else(|| timeline.duration().nseconds(), gst::ClockTime::nseconds);
            let pos_ns = pipeline.query_position::<gst::ClockTime>().map_or(0, gst::ClockTime::nseconds);
            let progress = if total_ns > 0 {
                ((pos_ns as f64) / (total_ns as f64)).clamp(0.0, 0.99) as f32
            } else {
                0.0
            };
            let eta_seconds = if progress >= 0.02 {
                let elapsed = started.elapsed().as_secs_f64();
                let remaining = elapsed * ((1.0 - f64::from(progress)) / f64::from(progress));
                Some(remaining.ceil().max(0.0) as u64)
            } else {
                None
            };
            worker.emit_drop_if_full(EngineEvent::ExportProgress { job_id, progress, eta_seconds });
            last_progress = Instant::now();
        }
    }
}
