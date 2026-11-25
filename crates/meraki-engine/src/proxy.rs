use crate::discoverer;
use crate::encoding_profiles::{self, ProxyPlugins};
use crate::protocol::{EngineEvent, EngineWorkerChannel};
use crate::proxy_cache::{
    finalize_proxy_files, proxy_cache_dir, proxy_output_path, proxy_target_dimensions,
    source_cache_pointer, source_fingerprint, try_reuse_cached_proxy,
};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use meraki_core::{AssetId, MediaMetadata};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

const MAX_PROXY_ATTEMPTS: usize = 4;

/// Trabalho enfileirado para transcodificação em segundo plano para 720p.
#[derive(Debug, Clone)]
pub(crate) struct ProxyJob {
    pub asset_id: AssetId,
    pub path: PathBuf,
    pub metadata: MediaMetadata,
}

/// Handle não bloqueante para envio de tarefas ao worker dedicado de proxies.
#[derive(Clone)]
pub(crate) struct ProxyQueueHandle {
    tx: Sender<ProxyJob>,
}

impl ProxyQueueHandle {
    pub(crate) fn enqueue(&self, job: ProxyJob) {
        let asset_id = job.asset_id;
        if let Err(err) = self.tx.try_send(job) {
            let reason = match err {
                TrySendError::Full(_) => "fila de proxies cheia",
                TrySendError::Disconnected(_) => "worker de proxies desconectado",
            };
            warn!(%asset_id, reason, "não foi possível enfileirar geração de proxy 720p");
        }
    }
}

/// Gerencia a thread dedicada `meraki-proxy-worker` garantindo encerramento limpo.
pub(crate) struct ProxyManager {
    tx: Option<Sender<ProxyJob>>,
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ProxyManager {
    pub(crate) fn spawn(worker: Arc<EngineWorkerChannel>) -> (Self, ProxyQueueHandle) {
        let (tx, rx) = bounded::<ProxyJob>(128);
        let queue_handle = ProxyQueueHandle { tx: tx.clone() };
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);

        let join = match thread::Builder::new()
            .name("meraki-proxy-worker".to_string())
            .spawn(move || run_proxy_worker(rx, worker_shutdown, worker))
        {
            Ok(handle) => Some(handle),
            Err(err) => {
                error!(error = %err, "falha ao criar thread dedicada de proxies 720p");
                None
            }
        };

        (Self { tx: Some(tx), shutdown, join }, queue_handle)
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        drop(self.tx.take());
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn needs_proxy(path: &Path, metadata: &MediaMetadata) -> bool {
    crate::proxy_cache::needs_proxy(path, metadata)
}

fn run_proxy_worker(rx: Receiver<ProxyJob>, shutdown: Arc<AtomicBool>, worker: Arc<EngineWorkerChannel>) {
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(job) => process_proxy_job(job, &shutdown, &worker),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn process_proxy_job(job: ProxyJob, shutdown: &AtomicBool, worker: &EngineWorkerChannel) {
    let dir = proxy_cache_dir();
    if let Err(err) = fs::create_dir_all(&dir) {
        warn!(asset_id = %job.asset_id, error = %err, "falha ao criar diretório de cache de proxies");
        return;
    }

    let output_path = proxy_output_path(job.asset_id);
    let fingerprint = source_fingerprint(&job.path, job.metadata.file_size_bytes);
    let source_pointer = source_cache_pointer(&dir, &job.path, job.metadata.file_size_bytes);

    if let Some(reused) = try_reuse_cached_proxy(&output_path, &source_pointer, &fingerprint) {
        info!(asset_id = %job.asset_id, path = %reused.display(), "proxy 720p reutilizado do cache em disco");
        worker.emit_critical(EngineEvent::ProxyProgress {
            asset_id: job.asset_id,
            progress: 1.0,
            proxy_path: Some(reused),
        });
        return;
    }

    let Some((src_w, src_h)) = job.metadata.width.zip(job.metadata.height) else {
        return;
    };
    let (target_w, target_h) = proxy_target_dimensions(src_w, src_h);
    let part_path = dir.join(format!("{}_720p.part.mp4", job.asset_id));

    for attempt in 0..MAX_PROXY_ATTEMPTS {
        if shutdown.load(Ordering::Acquire) {
            let _ = fs::remove_file(&part_path);
            return;
        }
        let plugins = match encoding_profiles::select_proxy_plugins(job.metadata.has_audio) {
            Ok(p) => p,
            Err(err) => {
                warn!(asset_id = %job.asset_id, error = %err, "plugins indisponíveis para proxy 720p");
                return;
            }
        };
        let _ = fs::remove_file(&part_path);
        match transcode_proxy_attempt(&job, plugins, target_w, target_h, &part_path, shutdown, worker) {
            Ok(()) => {
                if let Err(err) = finalize_proxy_files(&part_path, &output_path, &source_pointer, &fingerprint) {
                    warn!(asset_id = %job.asset_id, error = %err, "falha ao finalizar arquivo de proxy");
                    let _ = fs::remove_file(&part_path);
                    return;
                }
                info!(
                    asset_id = %job.asset_id,
                    attempt = attempt + 1,
                    encoder = plugins.video_encoder,
                    path = %output_path.display(),
                    "proxy 720p concluído"
                );
                worker.emit_critical(EngineEvent::ProxyProgress {
                    asset_id: job.asset_id,
                    progress: 1.0,
                    proxy_path: Some(output_path),
                });
                return;
            }
            Err(ProxyRunError::RetryHardware(enc)) if attempt + 1 < MAX_PROXY_ATTEMPTS => {
                warn!(asset_id = %job.asset_id, encoder = %enc, "tentando próximo encoder H.264 para proxy 720p");
            }
            Err(ProxyRunError::RetryHardware(reason)) | Err(ProxyRunError::Abort(reason)) => {
                warn!(asset_id = %job.asset_id, error = %reason, "transcodificação de proxy 720p interrompida");
                let _ = fs::remove_file(&part_path);
                return;
            }
        }
    }
}

enum ProxyRunError {
    RetryHardware(String),
    Abort(String),
}

struct ProxyPipelineGuard<'a>(&'a gst::Pipeline);

impl Drop for ProxyPipelineGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn transcode_proxy_attempt(
    job: &ProxyJob,
    plugins: ProxyPlugins,
    target_w: u32,
    target_h: u32,
    part_path: &Path,
    shutdown: &AtomicBool,
    worker: &EngineWorkerChannel,
) -> Result<(), ProxyRunError> {
    let uri = discoverer::path_to_file_uri(&job.path).map_err(|e| ProxyRunError::Abort(e.to_string()))?;
    let part_loc = part_path
        .to_str()
        .ok_or_else(|| ProxyRunError::Abort("caminho temporário de proxy inválido".to_string()))?;

    let pipeline = gst::Pipeline::with_name("meraki-proxy-transcode");
    let _guard = ProxyPipelineGuard(&pipeline);

    let decode = make_elem("uridecodebin", "proxy-decode")?;
    decode.set_property("uri", uri);
    let vqueue = make_elem("queue", "proxy-vqueue")?;
    let vconv = make_elem("videoconvert", "proxy-vconv")?;
    let vscale = make_elem("videoscale", "proxy-vscale")?;
    let vcaps = make_elem("capsfilter", "proxy-vcaps")?;
    let venc = make_elem(plugins.video_encoder, "proxy-venc")?;
    let mux = make_elem(plugins.muxer, "proxy-mux")?;
    let filesink = make_elem("filesink", "proxy-filesink")?;
    filesink.set_property("location", part_loc);

    set_auto_threads(&vconv);
    set_auto_threads(&vscale);
    tune_proxy_encoder(&venc, plugins.video_encoder);
    let target_caps = gst_video::VideoCapsBuilder::for_encoding("video/x-raw")
        .format_list([gst_video::VideoFormat::I420, gst_video::VideoFormat::Nv12])
        .width(i32::try_from(target_w).unwrap_or(1280))
        .height(i32::try_from(target_h).unwrap_or(720))
        .build();
    vcaps.set_property("caps", target_caps);

    pipeline
        .add_many([&decode, &vqueue, &vconv, &vscale, &vcaps, &venc, &mux, &filesink])
        .map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let vparse = gst::ElementFactory::find("h264parse")
        .and_then(|_| make_elem("h264parse", "proxy-h264parse").ok());
    if let Some(ref parse) = vparse {
        pipeline.add(parse).map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        gst::Element::link_many([&vqueue, &vconv, &vscale, &vcaps, &venc, parse])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(parse, &mux, "video_%u")?;
    } else {
        gst::Element::link_many([&vqueue, &vconv, &vscale, &vcaps, &venc])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(&venc, &mux, "video_%u")?;
    }
    mux.link(&filesink).map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let aqueue = if let Some(aenc_name) = plugins.audio_encoder {
        build_audio_branch(&pipeline, &mux, aenc_name)?
    } else {
        None
    };

    let vq_target = vqueue;
    let aq_target = aqueue;
    decode.connect_pad_added(move |_src, pad| {
        let Some(caps) = pad.current_caps().or_else(|| Some(pad.query_caps(None))) else { return; };
        let Some(name) = caps.structure(0).map(|s| s.name()) else { return; };
        if name.starts_with("video/") {
            if let Some(sp) = vq_target.static_pad("sink").filter(|p| !p.is_linked()) {
                let _ = pad.link(&sp);
            }
        } else if name.starts_with("audio/") {
            if let Some(sp) = aq_target.as_ref().and_then(|q| q.static_pad("sink")).filter(|p| !p.is_linked()) {
                let _ = pad.link(&sp);
            }
        }
    });

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| ProxyRunError::Abort(format!("erro ao iniciar proxy: {e:?}")))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| ProxyRunError::Abort("barramento indisponível para proxy".to_string()))?;

    let mut last_emit = Instant::now();
    loop {
        if shutdown.load(Ordering::Acquire) {
            let _ = pipeline.send_event(gst::event::Eos::new());
            return Err(ProxyRunError::Abort("motor encerrado durante transcodificação".to_string()));
        }
        if let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(100))) {
            match msg.view() {
                gst::MessageView::Eos(_) => return Ok(()),
                gst::MessageView::Error(err) => {
                    let detail = err.error().to_string();
                    if let Some(enc) = encoding_profiles::demote_failing_hardware_encoder(err.src()) {
                        return Err(ProxyRunError::RetryHardware(enc));
                    }
                    return Err(ProxyRunError::Abort(detail));
                }
                _ => {}
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(150) {
            let progress = query_progress(&pipeline, job.metadata.duration_ns);
            worker.emit_drop_if_full(EngineEvent::ProxyProgress {
                asset_id: job.asset_id,
                progress,
                proxy_path: None,
            });
            last_emit = Instant::now();
        }
    }
}

fn build_audio_branch(
    pipeline: &gst::Pipeline,
    mux: &gst::Element,
    aenc_name: &'static str,
) -> Result<Option<gst::Element>, ProxyRunError> {
    let aq = make_elem("queue", "proxy-aqueue")?;
    let aconv = make_elem("audioconvert", "proxy-aconv")?;
    let ares = make_elem("audioresample", "proxy-ares")?;
    let acaps = make_elem("capsfilter", "proxy-acaps")?;
    let aenc = make_elem(aenc_name, "proxy-aenc")?;
    let audio_caps = gst::Caps::builder("audio/x-raw")
        .field("rate", 48_000_i32)
        .field("channels", 2_i32)
        .build();
    acaps.set_property("caps", audio_caps);

    pipeline
        .add_many([&aq, &aconv, &ares, &acaps, &aenc])
        .map_err(|e| ProxyRunError::Abort(e.to_string()))?;

    let aparse = gst::ElementFactory::find("aacparse")
        .and_then(|_| make_elem("aacparse", "proxy-aacparse").ok());
    if let Some(ref parse) = aparse {
        pipeline.add(parse).map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        gst::Element::link_many([&aq, &aconv, &ares, &acaps, &aenc, parse])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(parse, mux, "audio_%u")?;
    } else {
        gst::Element::link_many([&aq, &aconv, &ares, &acaps, &aenc])
            .map_err(|e| ProxyRunError::Abort(e.to_string()))?;
        link_to_mux(&aenc, mux, "audio_%u")?;
    }
    Ok(Some(aq))
}

fn make_elem(factory: &'static str, name: &'static str) -> Result<gst::Element, ProxyRunError> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|e| ProxyRunError::Abort(format!("falha ao criar '{factory}': {e}")))
}

fn link_to_mux(src: &gst::Element, mux: &gst::Element, pad_tmpl: &str) -> Result<(), ProxyRunError> {
    let src_pad = src.static_pad("src").ok_or_else(|| ProxyRunError::Abort("pad src ausente".to_string()))?;
    let mux_pad = mux.request_pad_simple(pad_tmpl).ok_or_else(|| ProxyRunError::Abort(format!("pad '{pad_tmpl}' ausente")))?;
    src_pad.link(&mux_pad).map_err(|e| ProxyRunError::Abort(format!("link mux: {e:?}")))?;
    Ok(())
}

fn set_auto_threads(elem: &gst::Element) {
    if elem.find_property("n-threads").is_some() {
        elem.set_property("n-threads", 0_u32);
    }
}

fn set_int_prop(elem: &gst::Element, name: &str, val: u32) {
    if let Some(pspec) = elem.find_property(name) {
        if pspec.value_type() == glib::Type::U32 {
            elem.set_property(name, val);
        } else if pspec.value_type() == glib::Type::I32 {
            elem.set_property(name, i32::try_from(val).unwrap_or(15));
        }
    }
}

fn tune_proxy_encoder(elem: &gst::Element, factory_name: &str) {
    set_int_prop(elem, "key-int-max", 15);
    set_int_prop(elem, "gop-size", 15);
    set_int_prop(elem, "bframes", 0);
    if factory_name != "openh264enc" {
        set_int_prop(elem, "bitrate", 2500);
    }
    if elem.find_property("speed-preset").is_some() {
        elem.set_property_from_str("speed-preset", "veryfast");
    }
    if elem.find_property("tune").is_some() {
        elem.set_property_from_str("tune", "fastdecode");
    }
}

fn query_progress(pipeline: &gst::Pipeline, fallback_duration_ns: u64) -> f32 {
    let total_ns = pipeline
        .query_duration::<gst::ClockTime>()
        .map_or(fallback_duration_ns, gst::ClockTime::nseconds);
    let pos_ns = pipeline.query_position::<gst::ClockTime>().map_or(0, gst::ClockTime::nseconds);
    if total_ns == 0 { 0.0 } else { ((pos_ns as f64) / (total_ns as f64)).clamp(0.0, 0.99) as f32 }
}
