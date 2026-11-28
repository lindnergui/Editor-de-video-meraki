use crate::discoverer;
use crate::encoding_profiles::select_proxy_plugins;
use crate::protocol::{EngineEvent, EngineWorkerChannel};
use crate::proxy::{transcode, ProxyRunError};
use crate::proxy_cache;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use meraki_core::{AssetId, MediaMetadata, ProxyMode};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use thiserror::Error;
use tracing::warn;

pub(crate) struct ProxyJob {
    pub asset_id: AssetId,
    pub path: PathBuf,
    pub dimensions: (u32, u32),
    pub metadata: Option<MediaMetadata>,
}

enum Request {
    Generate(ProxyJob),
    Clear(Vec<PathBuf>),
}

pub(crate) enum ProxyNotice {
    Event(EngineEvent),
    Ready {
        asset_id: AssetId,
        source: PathBuf,
        proxy: PathBuf,
    },
    Cleared {
        freed_bytes: u64,
        error: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct ProxyQueue {
    tx: Sender<Request>,
    notices: Sender<ProxyNotice>,
    mode: Arc<AtomicU8>,
    shutdown: Arc<AtomicBool>,
}

impl ProxyQueue {
    pub(crate) fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    pub(crate) fn set_mode(&self, mode: ProxyMode) {
        let value = match mode {
            ProxyMode::Off => 0,
            ProxyMode::AutoAbove1080p => 1,
            ProxyMode::Always => 2,
        };
        self.mode.store(value, Ordering::Release);
    }

    fn mode(&self) -> ProxyMode {
        match self.mode.load(Ordering::Acquire) {
            0 => ProxyMode::Off,
            1 => ProxyMode::AutoAbove1080p,
            _ => ProxyMode::Always,
        }
    }

    pub(crate) fn enqueue(&self, job: ProxyJob) {
        if !eligible(self.mode(), &job.path, job.dimensions) {
            return;
        }
        let asset_id = job.asset_id;
        if let Err(err) = self.tx.try_send(Request::Generate(job)) {
            notify_failure(&self.notices, asset_id, &queue_error(err));
        }
    }

    pub(crate) fn clear(&self, originals: Vec<PathBuf>) -> Result<(), ProxyError> {
        self.tx
            .try_send(Request::Clear(originals))
            .map_err(queue_error)
    }
}

#[derive(Debug, Error)]
pub(crate) enum ProxyError {
    #[error("fila de proxies cheia")]
    QueueFull,
    #[error("worker de proxies desconectado")]
    Disconnected,
    #[error("proxy cancelado")]
    Cancelled,
    #[error("arquivo original mudou durante a geração do proxy")]
    SourceChanged,
    #[error("falha no cache de proxies: {0}")]
    Cache(#[from] std::io::Error),
    #[error(transparent)]
    Discover(#[from] discoverer::DiscovererError),
    #[error(transparent)]
    Plugins(#[from] crate::encoding_profiles::ExportProfileError),
    #[error("transcodificação de proxy: {0}")]
    Transcode(String),
}

fn queue_error(error: TrySendError<Request>) -> ProxyError {
    match error {
        TrySendError::Full(_) => ProxyError::QueueFull,
        TrySendError::Disconnected(_) => ProxyError::Disconnected,
    }
}

pub(crate) struct ProxyManager {
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ProxyManager {
    pub(crate) fn spawn(
        worker: Arc<EngineWorkerChannel>,
    ) -> (Self, ProxyQueue, Receiver<ProxyNotice>) {
        let (tx, rx) = bounded(128);
        let (notices, received) = unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));
        let queue = ProxyQueue {
            tx,
            notices,
            mode: Arc::new(AtomicU8::new(1)),
            shutdown: Arc::clone(&shutdown),
        };
        let worker_queue = queue.clone();
        let cancel = Arc::clone(&shutdown);

        let join = thread::Builder::new()
            .name("meraki-proxy-worker".to_string())
            .spawn(move || {
                lower_thread_priority();
                while !cancel.load(Ordering::Acquire) {
                    match rx.recv_timeout(Duration::from_millis(40)) {
                        Ok(Request::Generate(job)) => {
                            let stopped = || {
                                cancel.load(Ordering::Acquire)
                                    || worker_queue.mode() == ProxyMode::Off
                            };
                            if stopped()
                                || !eligible(worker_queue.mode(), &job.path, job.dimensions)
                            {
                                continue;
                            }
                            let asset_id = job.asset_id;
                            let source = job.path.clone();
                            match ensure(job, &stopped, &worker_queue.notices, &worker) {
                                Ok(proxy) => {
                                    let _ = worker_queue.notices.send(ProxyNotice::Ready {
                                        asset_id,
                                        source,
                                        proxy,
                                    });
                                }
                                Err(ProxyError::Cancelled) if cancel.load(Ordering::Acquire) => {
                                    break
                                }
                                Err(err) => notify_failure(&worker_queue.notices, asset_id, &err),
                            }
                        }
                        Ok(Request::Clear(originals)) => {
                            let (freed_bytes, error) = proxy_cache::clear(&originals);
                            let _ = worker_queue
                                .notices
                                .send(ProxyNotice::Cleared { freed_bytes, error });
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        let join = match join {
            Ok(join) => Some(join),
            Err(error) => {
                warn!(%error, "falha ao iniciar worker de proxies");
                None
            }
        };
        (Self { shutdown, join }, queue, received)
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ProxyManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn eligible(mode: ProxyMode, path: &Path, (width, height): (u32, u32)) -> bool {
    width > 0
        && height > 0
        && !discoverer::is_static_image_extension(path)
        && match mode {
            ProxyMode::Off => false,
            ProxyMode::AutoAbove1080p => width > 1920 || height > 1080,
            ProxyMode::Always => true,
        }
}

fn ensure(
    job: ProxyJob,
    stopped: &dyn Fn() -> bool,
    notices: &Sender<ProxyNotice>,
    worker: &EngineWorkerChannel,
) -> Result<PathBuf, ProxyError> {
    let name = proxy_cache::fingerprint(&job.path)?;
    if let Some(cached) = proxy_cache::locate(&name) {
        return Ok(cached);
    }
    let _ = notices.send(ProxyNotice::Event(EngineEvent::ProxyGenerating {
        asset_id: job.asset_id,
    }));
    let metadata = match job.metadata {
        Some(metadata) => metadata,
        None => discoverer::discover_media(job.asset_id, &job.path)?,
    };
    let dimensions = metadata
        .width
        .zip(metadata.height)
        .ok_or_else(|| ProxyError::Transcode("a mídia não tem fluxo de vídeo".into()))?;
    let target = proxy_cache::target_dimensions(dimensions.0, dimensions.1);
    let (output, partial) = proxy_cache::reserve_output(&name)?;

    for attempt in 0..4 {
        if stopped() {
            return Err(ProxyError::Cancelled);
        }
        let plugins = select_proxy_plugins(metadata.has_audio)?;
        match transcode(
            &job.path,
            &metadata,
            plugins,
            target,
            partial.path(),
            stopped,
            worker,
        ) {
            Ok(()) => {
                if stopped() {
                    return Err(ProxyError::Cancelled);
                }
                if proxy_cache::fingerprint(&job.path)? != name {
                    return Err(ProxyError::SourceChanged);
                }
                return partial.publish(output, &name).map_err(ProxyError::from);
            }
            Err(ProxyRunError::RetryHardware(encoder)) if attempt < 3 => {
                warn!(%encoder, "tentando encoder alternativo para o proxy");
            }
            Err(ProxyRunError::RetryHardware(reason) | ProxyRunError::Abort(reason)) => {
                return Err(ProxyError::Transcode(reason));
            }
            Err(ProxyRunError::Cancelled) => return Err(ProxyError::Cancelled),
        }
    }
    Err(ProxyError::Transcode(
        "tentativas de codificação esgotadas".into(),
    ))
}

fn notify_failure(notices: &Sender<ProxyNotice>, asset_id: AssetId, err: &ProxyError) {
    warn!(%asset_id, error = %err, "falha na geração do proxy");
    let _ = notices.send(ProxyNotice::Event(EngineEvent::ProxyFailed {
        asset_id,
        reason: err.to_string(),
    }));
}

#[cfg(windows)]
fn lower_thread_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
    };
    // O pseudohandle pertence à thread chamadora e não precisa ser fechado.
    if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } == 0 {
        warn!("não foi possível reduzir a prioridade da thread de proxy");
    }
}

#[cfg(target_os = "linux")]
fn lower_thread_priority() {
    // No Linux, PRIO_PROCESS com identificador 0 ajusta o nice da thread chamadora.
    if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) } != 0 {
        warn!("não foi possível reduzir a prioridade da thread de proxy");
    }
}

#[cfg(target_os = "macos")]
fn lower_thread_priority() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(
            qos_class: libc::c_uint,
            relative_priority: libc::c_int,
        ) -> libc::c_int;
    }
    // QOS_CLASS_UTILITY é a classe Darwin para trabalho de segundo plano com progresso visível.
    if unsafe { pthread_set_qos_class_self_np(0x11, 0) } != 0 {
        warn!("não foi possível reduzir a prioridade da thread de proxy");
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn lower_thread_priority() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_proxy_threshold_is_strict_in_both_dimensions() {
        let path = Path::new("clip.mp4");
        assert!(!eligible(ProxyMode::AutoAbove1080p, path, (1920, 1080)));
        assert!(eligible(ProxyMode::AutoAbove1080p, path, (1922, 1080)));
        assert!(eligible(ProxyMode::AutoAbove1080p, path, (1080, 1920)));
        assert!(eligible(ProxyMode::Always, path, (640, 480)));
        assert!(!eligible(
            ProxyMode::Always,
            Path::new("imagem.png"),
            (3840, 2160)
        ));
        assert!(!eligible(ProxyMode::Off, path, (3840, 2160)));
    }
}
