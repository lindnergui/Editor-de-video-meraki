use crate::app::{MerakiApp, Message};
use crate::playback;
use iced::Task;
use meraki_core::{
    ExportFormat, ExportFrameRate, ExportJob, ExportJobId, ExportJobStatus, ExportPreset,
    ExportQuality, ExportResolution, ExportSettings,
};
use meraki_engine::{EngineCommand, EngineEvent};
use std::path::PathBuf;
use tracing::{error, info, warn};

const END_COVER_MARGIN_NS: u64 = 100_000_000;
const DEFAULT_FILE_STEM: &str = "meu-video-final";

/// Pontos predefinidos de captura de capa na timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoverSlot {
    Start,
    Middle,
    End,
}

impl CoverSlot {
    pub const ALL: [CoverSlot; 3] = [Self::Start, Self::Middle, Self::End];

    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Início",
            Self::Middle => "Meio",
            Self::End => "Fim",
        }
    }

    /// Calcula o timestamp alvo na timeline (em nanossegundos) para a capa escolhida.
    pub fn timestamp_ns(self, duration_ns: u64) -> u64 {
        match self {
            Self::Start => 0,
            Self::Middle => duration_ns / 2,
            Self::End => duration_ns.saturating_sub(END_COVER_MARGIN_NS),
        }
    }
}

/// Estado local da aba Entrega (configurações, busca de presets, fila e histórico).
#[derive(Debug, Clone)]
pub struct DeliveryState {
    pub preset_search: String,
    pub settings: ExportSettings,
    pub queue: Vec<ExportJob>,
    pub history: Vec<ExportJob>,
    pub active_job_id: Option<ExportJobId>,
    pub queue_started: bool,
    pub selected_cover_slot: Option<CoverSlot>,
    pub notice: Option<String>,
}

impl Default for DeliveryState {
    fn default() -> Self {
        let mut settings = ExportSettings::from_preset(ExportPreset::YouTube);
        settings.file_name = format!("{DEFAULT_FILE_STEM}.{}", settings.format.extension());
        settings.output_dir = default_output_dir();
        Self {
            preset_search: String::new(),
            settings,
            queue: Vec::new(),
            history: Vec::new(),
            active_job_id: None,
            queue_started: false,
            selected_cover_slot: None,
            notice: None,
        }
    }
}

/// Mensagens da aba Entrega roteadas por `MerakiApp`.
#[derive(Debug, Clone)]
pub enum DeliveryMessage {
    PresetSearchChanged(String),
    SelectPreset(ExportPreset),
    SelectCoverSlot(CoverSlot),
    FileNameChanged(String),
    PickOutputDirClicked,
    OutputDirPicked(Option<PathBuf>),
    FormatChanged(ExportFormat),
    ResolutionChanged(ExportResolution),
    FrameRateChanged(ExportFrameRate),
    QualityChanged(ExportQuality),
    ExportClicked,
    StartQueue,
    CancelJob(ExportJobId),
    ClearHistory,
}

fn default_output_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    if let Some(base) = home.map(PathBuf::from) {
        let videos = base.join("Videos");
        if videos.is_dir() {
            return videos;
        }
        let movies = base.join("Movies");
        if movies.is_dir() {
            return movies;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Abre o seletor nativo do sistema operacional para escolher a pasta de saída.
pub async fn pick_output_folder(initial_dir: PathBuf) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title("Selecionar pasta de destino");
    if initial_dir.is_dir() {
        dialog = dialog.set_directory(&initial_dir);
    }
    dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
}

/// Preserva o nome-base do arquivo trocando apenas a extensão de mídia pelo formato ativo.
pub fn replace_extension(file_name: &str, ext: &str) -> String {
    const KNOWN_EXTS: [&str; 6] = [".mp4", ".mov", ".wav", ".mp3", ".mkv", ".avi"];
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return format!("{DEFAULT_FILE_STEM}.{ext}");
    }
    let lower = trimmed.to_ascii_lowercase();
    let mut stem = trimmed;
    for known in KNOWN_EXTS {
        if lower.ends_with(known) && trimmed.len() > known.len() {
            stem = &trimmed[..trimmed.len() - known.len()];
            break;
        }
    }
    let stem = stem.trim().trim_end_matches('.');
    if stem.is_empty() {
        format!("{DEFAULT_FILE_STEM}.{ext}")
    } else {
        format!("{stem}.{ext}")
    }
}

/// Formata contagem de bytes em unidade legível (`B`, `KB`, `MB`, `GB`).
pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1_024.0;
    const MB: f64 = 1_048_576.0;
    const GB: f64 = 1_073_741_824.0;
    let b = bytes as f64;
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if b < MB {
        format!("{:.1} KB", b / KB)
    } else if b < GB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.2} GB", b / GB)
    }
}

/// Formata duração em nanossegundos como `HH:MM:SS`.
pub fn format_hms(duration_ns: u64) -> String {
    let total_secs = duration_ns / 1_000_000_000;
    let hours = total_secs / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let seconds = total_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Formata tempo restante estimado (ETA) de exportação.
pub fn format_eta(eta_seconds: Option<u64>) -> String {
    match eta_seconds {
        None => "Calculando...".to_string(),
        Some(secs) if secs >= 3_600 => format!("{}h {:02}m", secs / 3_600, (secs % 3_600) / 60),
        Some(secs) => format!("{:02}:{:02}", secs / 60, secs % 60),
    }
}

impl MerakiApp {
    /// Processa todas as interações da aba Entrega mantendo a UI não bloqueante.
    pub(crate) fn handle_delivery_message(&mut self, msg: DeliveryMessage) -> Task<Message> {
        match msg {
            DeliveryMessage::PresetSearchChanged(query) => self.delivery.preset_search = query,
            DeliveryMessage::SelectPreset(preset) => {
                let mut next = ExportSettings::from_preset(preset);
                next.output_dir = self.delivery.settings.output_dir.clone();
                next.cover_timestamp_ns = self.delivery.settings.cover_timestamp_ns;
                next.file_name =
                    replace_extension(&self.delivery.settings.file_name, next.format.extension());
                self.delivery.settings = next;
                self.project.export_preset = preset;
                self.delivery.notice = None;
            }
            DeliveryMessage::SelectCoverSlot(slot) => {
                let ts = slot.timestamp_ns(self.project.timeline.total_duration_ns());
                self.delivery.selected_cover_slot = Some(slot);
                self.delivery.settings.cover_timestamp_ns = Some(ts);
                self.playback.playhead_ns = ts;
                self.project.timeline.playhead_ns = ts;
                self.engine.request_seek(ts);
            }
            DeliveryMessage::FileNameChanged(name) => {
                self.delivery.settings.file_name = name;
                self.delivery.notice = None;
            }
            DeliveryMessage::PickOutputDirClicked => {
                return Task::perform(
                    pick_output_folder(self.delivery.settings.output_dir.clone()),
                    |dir| Message::Delivery(DeliveryMessage::OutputDirPicked(dir)),
                );
            }
            DeliveryMessage::OutputDirPicked(Some(dir)) => {
                self.delivery.settings.output_dir = dir;
                self.delivery.notice = None;
            }
            DeliveryMessage::OutputDirPicked(None) => {}
            DeliveryMessage::FormatChanged(format) => {
                self.delivery.settings.format = format;
                self.delivery.settings.file_name =
                    replace_extension(&self.delivery.settings.file_name, format.extension());
                self.delivery.notice = None;
            }
            DeliveryMessage::ResolutionChanged(res) => self.delivery.settings.resolution = res,
            DeliveryMessage::FrameRateChanged(fps) => self.delivery.settings.fps = fps,
            DeliveryMessage::QualityChanged(quality) => self.delivery.settings.quality = quality,
            DeliveryMessage::ExportClicked => self.enqueue_and_start_export(),
            DeliveryMessage::StartQueue => {
                self.delivery.queue_started = true;
                if self.delivery.active_job_id.is_none() {
                    self.start_next_export_job();
                }
            }
            DeliveryMessage::CancelJob(job_id) => self.cancel_export_job(job_id),
            DeliveryMessage::ClearHistory => self.delivery.history.clear(),
        }
        Task::none()
    }

    fn enqueue_and_start_export(&mut self) {
        let duration_ns = self.project.timeline.total_duration_ns();
        match ExportJob::new(self.delivery.settings.clone(), duration_ns) {
            Ok(job) => {
                self.delivery.notice = None;
                self.delivery.queue.push(job);
                self.delivery.queue_started = true;
                if self.delivery.active_job_id.is_none() {
                    self.start_next_export_job();
                }
            }
            Err(err) => {
                let message = err.to_string();
                self.delivery.notice = Some(message.clone());
                self.last_error = Some(message);
            }
        }
    }

    fn cancel_export_job(&mut self, job_id: ExportJobId) {
        if self.delivery.active_job_id == Some(job_id) {
            self.delivery.queue_started = false;
            if let Err(err) = self.engine.send(EngineCommand::CancelExport { job_id }) {
                let message = err.to_string();
                self.delivery.notice = Some(message.clone());
                self.last_error = Some(message);
            }
            return;
        }
        if self
            .delivery
            .queue
            .iter()
            .any(|job| job.id == job_id && matches!(job.status, ExportJobStatus::Queued))
        {
            self.move_job_to_history(job_id, |job| job.status = ExportJobStatus::Cancelled);
        }
    }

    fn start_next_export_job(&mut self) {
        let Some(idx) = self
            .delivery
            .queue
            .iter()
            .position(|job| matches!(job.status, ExportJobStatus::Queued))
        else {
            self.delivery.queue_started = false;
            return;
        };

        let (job_id, settings) = {
            let job = &mut self.delivery.queue[idx];
            job.status = ExportJobStatus::Running { progress: 0.0, eta_seconds: None };
            (job.id, job.settings.clone())
        };
        self.delivery.active_job_id = Some(job_id);

        let clips = playback::build_render_clips(&self.project);
        let audio_tracks = playback::build_audio_mix(&self.project);
        if let Err(err) = self.engine.send(EngineCommand::StartExport {
            transitions: self.project.timeline.transitions.clone(),
            job_id,
            settings,
            clips,
            audio_tracks,
        }) {
            let message = err.to_string();
            self.move_job_to_history(job_id, |job| job.status = ExportJobStatus::Failed(message.clone()));
            self.delivery.notice = Some(message.clone());
            self.last_error = Some(message);
        }
    }

    fn move_job_to_history(&mut self, job_id: ExportJobId, update: impl FnOnce(&mut ExportJob)) {
        if self.delivery.active_job_id == Some(job_id) {
            self.delivery.active_job_id = None;
        }
        if let Some(pos) = self.delivery.queue.iter().position(|job| job.id == job_id) {
            let mut job = self.delivery.queue.remove(pos);
            update(&mut job);
            self.delivery.history.insert(0, job);
        }
    }

    /// Trata progresso, conclusão, falha e cancelamento vindos da thread de exportação.
    pub(crate) fn handle_export_engine_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::ExportProgress { job_id, progress, eta_seconds } => {
                if let Some(job) = self.delivery.queue.iter_mut().find(|job| job.id == job_id) {
                    job.status = ExportJobStatus::Running {
                        progress: progress.clamp(0.0, 1.0),
                        eta_seconds,
                    };
                }
            }
            EngineEvent::ExportFinished { job_id, output_path, file_size_bytes } => {
                info!(%job_id, path = %output_path.display(), file_size_bytes, "exportação concluída");
                self.move_job_to_history(job_id, |job| {
                    job.output_path = output_path;
                    job.status = ExportJobStatus::Completed { file_size_bytes };
                });
                if self.delivery.queue_started {
                    self.start_next_export_job();
                }
            }
            EngineEvent::ExportFailed { job_id, error } => {
                error!(%job_id, error = %error, "falha na exportação");
                self.move_job_to_history(job_id, |job| {
                    job.status = ExportJobStatus::Failed(error.clone());
                });
                self.delivery.notice = Some(error.clone());
                self.last_error = Some(error);
                if self.delivery.queue_started {
                    self.start_next_export_job();
                }
            }
            EngineEvent::ExportCancelled { job_id } => {
                warn!(%job_id, "exportação cancelada pelo usuário");
                self.move_job_to_history(job_id, |job| {
                    job.status = ExportJobStatus::Cancelled;
                });
            }
            _ => {}
        }
    }
}
