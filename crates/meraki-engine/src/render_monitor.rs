use crate::encoding_profiles;
use crate::protocol::{EngineEvent, EngineWorkerChannel};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::ExportJobId;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const STALL_WARNING_INTERVAL: Duration = Duration::from_secs(10);
const PREROLL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub(crate) enum RenderBusError {
    #[error("{detail}")]
    HardwareEncoderFailed { encoder: String, detail: String },
    #[error("{0}")]
    Fatal(String),
}

enum RenderOutcome {
    Finished,
    Cancelled,
}

pub(crate) fn monitor_render_bus(
    job_id: ExportJobId,
    pipeline: &ges::Pipeline,
    timeline: &ges::Timeline,
    output_path: &Path,
    cancel_flag: &AtomicBool,
    worker: &EngineWorkerChannel,
) -> Result<(), RenderBusError> {
    let bus = pipeline.bus().ok_or_else(|| {
        RenderBusError::Fatal("barramento GStreamer indisponível para renderização".to_string())
    })?;

    debug!(%job_id, "solicitando estado Playing para renderização");
    let start_result = pipeline.set_state(gst::State::Playing);
    let result = match start_result {
        Ok(_) => wait_for_render(job_id, pipeline, timeline, &bus, cancel_flag, worker),
        Err(err) => {
            let (_, current, pending) = pipeline.state(Some(gst::ClockTime::ZERO));
            warn!(%job_id, ?current, ?pending, error = %err, "falha ao iniciar render");
            let mut failure = RenderBusError::Fatal(format!(
                "erro ao iniciar render (current={current:?}, pending={pending:?}): {err}"
            ));
            while let Some(message) = bus.pop() {
                match message.view() {
                    gst::MessageView::Warning(warning) => log_warning(job_id, pipeline, warning),
                    gst::MessageView::Error(error) => {
                        failure = render_error(job_id, pipeline, error);
                        break;
                    }
                    _ => {}
                }
            }
            Err(failure)
        }
    };

    if let Err(err) = pipeline.set_state(gst::State::Null) {
        let (_, current, pending) = pipeline.state(Some(gst::ClockTime::ZERO));
        warn!(%job_id, ?current, ?pending, error = %err, "falha ao encerrar render");
        if matches!(result, Ok(RenderOutcome::Finished)) {
            return Err(RenderBusError::Fatal(format!(
                "erro ao encerrar render: {err}"
            )));
        }
    }
    if start_result.is_ok() && !matches!(result, Ok(RenderOutcome::Finished)) {
        if let Err(err) = fs::remove_file(output_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(%job_id, error = %err, path = %output_path.display(), "falha ao remover arquivo parcial");
            }
        }
    }

    match result? {
        RenderOutcome::Finished => {
            let file_size_bytes = fs::metadata(output_path)
                .map_err(|err| {
                    RenderBusError::Fatal(format!("falha ao ler arquivo renderizado: {err}"))
                })?
                .len();
            worker.emit_drop_if_full(EngineEvent::ExportProgress {
                job_id,
                progress: 1.0,
                eta_seconds: Some(0),
            });
            worker.emit_critical(EngineEvent::ExportFinished {
                job_id,
                output_path: output_path.to_path_buf(),
                file_size_bytes,
            });
        }
        RenderOutcome::Cancelled => worker.emit_critical(EngineEvent::ExportCancelled { job_id }),
    }
    Ok(())
}

fn wait_for_render(
    job_id: ExportJobId,
    pipeline: &ges::Pipeline,
    timeline: &ges::Timeline,
    bus: &gst::Bus,
    cancel_flag: &AtomicBool,
    worker: &EngineWorkerChannel,
) -> Result<RenderOutcome, RenderBusError> {
    let started = Instant::now();
    let mut last_activity = started;
    let mut last_warning = started;
    let mut last_report = started;
    let mut furthest_position = 0;
    let mut current = gst::State::Null;
    let total_ns = timeline.duration().nseconds();

    loop {
        if cancel_flag.load(Ordering::Acquire) {
            return Ok(RenderOutcome::Cancelled);
        }

        if let Some(message) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(100))) {
            match message.view() {
                gst::MessageView::Eos(_) => return Ok(RenderOutcome::Finished),
                gst::MessageView::Error(error) => {
                    return Err(render_error(job_id, pipeline, error))
                }
                gst::MessageView::Warning(warning) => log_warning(job_id, pipeline, warning),
                gst::MessageView::StateChanged(state)
                    if state.src() == Some(pipeline.upcast_ref()) =>
                {
                    last_activity = Instant::now();
                    current = state.current();
                    debug!(%job_id, ?current, pending = ?state.pending(), "estado do render mudou");
                }
                _ => {}
            }
        }

        if last_report.elapsed() < PROGRESS_INTERVAL {
            continue;
        }
        last_report = Instant::now();
        let position_ns = if current == gst::State::Playing {
            pipeline
                .query_position::<gst::ClockTime>()
                .map_or(furthest_position, gst::ClockTime::nseconds)
                .min(total_ns)
        } else {
            furthest_position
        };
        if position_ns > furthest_position {
            furthest_position = position_ns;
            last_activity = last_report;
        }

        let idle_for = last_report.duration_since(last_activity);
        if idle_for >= STALL_WARNING_INTERVAL && last_warning.elapsed() >= STALL_WARNING_INTERVAL {
            last_warning = last_report;
            let (_, current, pending) = pipeline.state(Some(gst::ClockTime::ZERO));
            warn!(%job_id, ?current, ?pending, position_ns, total_ns, ?idle_for, "render sem avanço ou EOS");
            if current < gst::State::Playing && idle_for >= PREROLL_TIMEOUT {
                return Err(RenderBusError::Fatal(format!(
                    "timeout no preroll de renderização (current={current:?}, pending={pending:?}, posição={position_ns}ns)"
                )));
            }
        }

        let progress = if total_ns > 0 {
            ((position_ns as f64) / (total_ns as f64)).clamp(0.0, 0.99) as f32
        } else {
            0.0
        };
        let eta_seconds = (progress >= 0.02).then(|| {
            let elapsed = started.elapsed().as_secs_f64();
            let remaining = elapsed * ((1.0 - f64::from(progress)) / f64::from(progress));
            remaining.ceil().max(0.0) as u64
        });
        worker.emit_drop_if_full(EngineEvent::ExportProgress {
            job_id,
            progress,
            eta_seconds,
        });
    }
}

fn log_warning(job_id: ExportJobId, pipeline: &ges::Pipeline, warning: &gst::message::Warning) {
    let (_, current, pending) = pipeline.state(Some(gst::ClockTime::ZERO));
    let source = warning
        .src()
        .map(|src| src.path_string())
        .unwrap_or_default();
    warn!(%job_id, ?current, ?pending, %source, warning = %warning.error(), detail = ?warning.debug(), "aviso do barramento de renderização");
}

fn render_error(
    job_id: ExportJobId,
    pipeline: &ges::Pipeline,
    error: &gst::message::Error,
) -> RenderBusError {
    let (_, current, pending) = pipeline.state(Some(gst::ClockTime::ZERO));
    let source = error.src().map(|src| src.path_string()).unwrap_or_default();
    let detail = error.debug().unwrap_or_default();
    let cause = error.error();
    warn!(%job_id, ?current, ?pending, %source, %detail, error = %cause, "erro do barramento de renderização");
    let message = format!(
        "erro no render ({source}, current={current:?}, pending={pending:?}): {cause} ({detail})"
    );
    match encoding_profiles::demote_failing_hardware_encoder(error.src()) {
        Some(encoder) => RenderBusError::HardwareEncoderFailed {
            encoder,
            detail: message,
        },
        None => RenderBusError::Fatal(message),
    }
}
