use crate::audio_output::MasterMix;
use crate::clip_prepare;
use crate::encoding_profiles;
use crate::export_pipeline::{build_render_pipeline, detect_active_streams, RenderSpec};
use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip, TrackAudioMix};
use crate::render_monitor::{monitor_render_bus, RenderBusError};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use gstreamer_editing_services as ges;
use meraki_core::{ExportJobId, ExportSettings, TrackKind};
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tracing::{debug, error, info, warn};

struct ExportTask {
    job_id: ExportJobId,
    settings: ExportSettings,
    clips: Vec<TimelineRenderClip>,
    transitions: Vec<meraki_core::TimelineTransition>,
    audio_tracks: Vec<TrackAudioMix>,
    master: MasterMix,
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
        transitions: Vec<meraki_core::TimelineTransition>,
        audio_tracks: Vec<TrackAudioMix>,
        master: MasterMix,
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

        let task = ExportTask { job_id, settings, clips, transitions, audio_tracks, master, cancel_flag };
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
            transitions: self.transitions.clone(),
            audio_tracks: self.audio_tracks.clone(),
            master: self.master,
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

    let audio_only = task.settings.format.is_audio_only();
    let mut clips = task.clips;
    if audio_only {
        clips.retain(|clip| clip.track_kind != TrackKind::Overlay);
    }
    if clips.is_empty() {
        return Err("nenhum clipe compatível para renderizar na timeline".to_string());
    }

    debug!(%job_id, clips = clips.len(), "preparando mídia para exportação");
    let prepared = context
        .block_on(clip_prepare::prepare_clips(asset_cache, clips))
        .map_err(|e| e.to_string())?;
    let tracks = detect_active_streams(&prepared, audio_only);
    if audio_only && tracks.is_empty() {
        return Err("nenhum fluxo de áudio encontrado para exportação Apenas Áudio".to_string());
    }

    const MAX_RENDER_ATTEMPTS: usize = 4;

    for attempt in 0..MAX_RENDER_ATTEMPTS {
        if task.cancel_flag.load(Ordering::Acquire) {
            return Err("exportação cancelada".to_string());
        }

        let resolved = encoding_profiles::build_encoding_profile(
            &task.settings,
            tracks.contains(ges::TrackType::VIDEO),
            tracks.contains(ges::TrackType::AUDIO),
        )
        .map_err(|e| e.to_string())?;
        info!(
            %job_id,
            attempt = attempt + 1,
            path = %output_path.display(),
            ?tracks,
            muxer = ?resolved.muxer,
            video_enc = ?resolved.video_encoder,
            audio_enc = ?resolved.audio_encoder,
            "iniciando renderização GES dedicada"
        );

        let (timeline, pipeline) = build_render_pipeline(RenderSpec {
            settings: &task.settings,
            audio_tracks: task.audio_tracks.clone(),
            master: task.master,
            prepared: &prepared,
            transitions: &task.transitions,
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
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("nenhum encoder compatível concluiu a renderização".to_string())
}
