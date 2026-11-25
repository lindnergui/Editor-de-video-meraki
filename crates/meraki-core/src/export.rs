use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Predefinições oficiais de exportação na aba Entrega.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExportPreset {
    #[default]
    YouTube,
    TikTokReels,
    Instagram,
    #[serde(alias = "WhatsAppLight")]
    WhatsAppLite,
    #[serde(alias = "ProResHighQuality")]
    MasterProRes,
    AudioOnly,
}

impl ExportPreset {
    pub const ALL: [ExportPreset; 6] = [
        Self::YouTube, Self::TikTokReels, Self::Instagram,
        Self::WhatsAppLite, Self::MasterProRes, Self::AudioOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::TikTokReels => "TikTok & Reels",
            Self::Instagram => "Instagram",
            Self::WhatsAppLite => "WhatsApp Leve",
            Self::MasterProRes => "Alta Qualidade ProRes",
            Self::AudioOnly => "Apenas Áudio",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::YouTube => "16:9 Full HD H.264 + AAC pronto para envio",
            Self::TikTokReels => "9:16 Vertical 1080x1920 em 60 fps",
            Self::Instagram => "1:1 Quadrado 1080x1080 balanceado para feed",
            Self::WhatsAppLite => "720p compacto para compartilhamento rápido",
            Self::MasterProRes => "MOV ProRes de altíssima qualidade para pós",
            Self::AudioOnly => "Extração de áudio PCM sem perdas ou MP3",
        }
    }
}

/// Formato de contêiner e codec de destino da renderização.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExportFormat {
    #[default]
    Mp4H264,
    MovProRes,
    AudioWav,
    AudioMp3,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [Self::Mp4H264, Self::MovProRes, Self::AudioWav, Self::AudioMp3];

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp4H264 => "mp4",
            Self::MovProRes => "mov",
            Self::AudioWav => "wav",
            Self::AudioMp3 => "mp3",
        }
    }

    pub const fn is_audio_only(self) -> bool { matches!(self, Self::AudioWav | Self::AudioMp3) }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4H264 => "MP4 (H.264 + AAC)",
            Self::MovProRes => "MOV (Apple ProRes)",
            Self::AudioWav => "WAV (Áudio PCM)",
            Self::AudioMp3 => "MP3 (Áudio MPEG)",
        }
    }
}

/// Resolução de saída para exportação de vídeo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExportResolution {
    Sd720p,
    #[default]
    FullHd1080p,
    Uhd4k,
    Vertical1080x1920,
    Square1080x1080,
}

impl ExportResolution {
    pub const ALL: [ExportResolution; 5] = [
        Self::Sd720p, Self::FullHd1080p, Self::Uhd4k, Self::Vertical1080x1920, Self::Square1080x1080,
    ];

    pub const fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Sd720p => (1280, 720),
            Self::FullHd1080p => (1920, 1080),
            Self::Uhd4k => (3840, 2160),
            Self::Vertical1080x1920 => (1080, 1920),
            Self::Square1080x1080 => (1080, 1080),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sd720p => "720p (1280x720)",
            Self::FullHd1080p => "1080p (1920x1080)",
            Self::Uhd4k => "4K UHD (3840x2160)",
            Self::Vertical1080x1920 => "Vertical 9:16 (1080x1920)",
            Self::Square1080x1080 => "Quadrado 1:1 (1080x1080)",
        }
    }
}

/// Taxa de quadros suportada para renderização de vídeo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExportFrameRate {
    Fps24,
    #[default]
    Fps30,
    Fps60,
}

impl ExportFrameRate {
    pub const ALL: [ExportFrameRate; 3] = [Self::Fps24, Self::Fps30, Self::Fps60];

    pub const fn as_u32(self) -> u32 {
        match self { Self::Fps24 => 24, Self::Fps30 => 30, Self::Fps60 => 60 }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Fps24 => "24 fps", Self::Fps30 => "30 fps", Self::Fps60 => "60 fps" }
    }
}

/// Nível de qualidade e taxa de bits aplicada à codificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExportQuality {
    Low,
    Medium,
    #[default]
    High,
    Master,
}

impl ExportQuality {
    pub const ALL: [ExportQuality; 4] = [Self::Low, Self::Medium, Self::High, Self::Master];

    pub fn label(self) -> &'static str {
        match self { Self::Low => "Leve", Self::Medium => "Média", Self::High => "Alta", Self::Master => "Master" }
    }

    /// Retorna a taxa de bits alvo de vídeo em kbps conforme resolução e nível de qualidade.
    pub const fn video_bitrate_kbps(&self, res: ExportResolution) -> u32 {
        match (self, res) {
            (Self::Low, ExportResolution::Sd720p) => 1_800,
            (Self::Low, ExportResolution::Square1080x1080) => 3_000,
            (Self::Low, ExportResolution::FullHd1080p | ExportResolution::Vertical1080x1920) => 4_000,
            (Self::Low, ExportResolution::Uhd4k) => 14_000,
            (Self::Medium, ExportResolution::Sd720p) => 3_500,
            (Self::Medium, ExportResolution::Square1080x1080) => 6_000,
            (Self::Medium, ExportResolution::FullHd1080p | ExportResolution::Vertical1080x1920) => 8_000,
            (Self::Medium, ExportResolution::Uhd4k) => 28_000,
            (Self::High, ExportResolution::Sd720p) => 6_000,
            (Self::High, ExportResolution::Square1080x1080) => 10_000,
            (Self::High, ExportResolution::FullHd1080p | ExportResolution::Vertical1080x1920) => 14_000,
            (Self::High, ExportResolution::Uhd4k) => 45_000,
            (Self::Master, ExportResolution::Sd720p) => 25_000,
            (Self::Master, ExportResolution::Square1080x1080) => 55_000,
            (Self::Master, ExportResolution::FullHd1080p | ExportResolution::Vertical1080x1920) => 80_000,
            (Self::Master, ExportResolution::Uhd4k) => 180_000,
        }
    }

    /// Retorna a taxa de bits de áudio comprimido em kbps.
    pub const fn audio_bitrate_kbps(&self) -> u32 {
        match self { Self::Low => 128, Self::Medium => 192, Self::High => 256, Self::Master => 320 }
    }
}

/// Erros de validação de configuração e fila de exportação.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("nome do arquivo de exportação não pode ser vazio")]
    EmptyFileName,
    #[error("nome do arquivo de exportação inválido: {0}")]
    InvalidFileName(&'static str),
    #[error("diretório de destino da exportação não pode ser vazio")]
    EmptyOutputDir,
    #[error("não é possível exportar uma timeline vazia")]
    EmptyTimeline,
}

/// Configurações completas de um trabalho de exportação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub file_name: String,
    pub output_dir: PathBuf,
    pub preset: ExportPreset,
    pub format: ExportFormat,
    pub resolution: ExportResolution,
    pub fps: ExportFrameRate,
    pub quality: ExportQuality,
    pub cover_timestamp_ns: Option<u64>,
}

impl Default for ExportSettings {
    fn default() -> Self { Self::from_preset(ExportPreset::YouTube) }
}

impl ExportSettings {
    /// Cria configurações iniciais coerentes a partir de um dos 6 presets oficiais.
    pub fn from_preset(preset: ExportPreset) -> Self {
        use ExportFormat::*;
        use ExportFrameRate::*;
        use ExportQuality::*;
        use ExportResolution::*;
        let (file_name, format, resolution, fps, quality) = match preset {
            ExportPreset::YouTube => ("export_youtube.mp4", Mp4H264, FullHd1080p, Fps30, High),
            ExportPreset::TikTokReels => ("export_tiktok_reels.mp4", Mp4H264, Vertical1080x1920, Fps60, High),
            ExportPreset::Instagram => ("export_instagram.mp4", Mp4H264, Square1080x1080, Fps30, High),
            ExportPreset::WhatsAppLite => ("export_whatsapp.mp4", Mp4H264, Sd720p, Fps30, Low),
            ExportPreset::MasterProRes => ("export_master.mov", MovProRes, FullHd1080p, Fps30, Master),
            ExportPreset::AudioOnly => ("export_audio.wav", AudioWav, FullHd1080p, Fps30, High),
        };
        Self {
            file_name: file_name.to_string(),
            output_dir: PathBuf::from("."),
            preset,
            format,
            resolution,
            fps,
            quality,
            cover_timestamp_ns: None,
        }
    }

    /// Higieniza o nome do arquivo removendo caracteres inválidos e garantindo extensão compatível.
    pub fn sanitized_file_name(&self) -> Result<String, ExportError> {
        let trimmed = self.file_name.trim();
        if trimmed.is_empty() {
            return Err(ExportError::EmptyFileName);
        }
        if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
            return Err(ExportError::InvalidFileName("caminho ou separador de pasta não permitido"));
        }
        let cleaned: String = trimmed
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
                other => other,
            })
            .collect();
        let cleaned = cleaned.trim().trim_matches('.');
        if cleaned.is_empty() {
            return Err(ExportError::EmptyFileName);
        }

        let target_ext = self.format.extension();
        let stem = strip_known_media_extension(cleaned);
        if stem.is_empty() {
            return Err(ExportError::EmptyFileName);
        }
        Ok(format!("{stem}.{target_ext}"))
    }

    pub fn resolved_output_path(&self) -> Result<PathBuf, ExportError> {
        if self.output_dir.as_os_str().is_empty() {
            return Err(ExportError::EmptyOutputDir);
        }
        let name = self.sanitized_file_name()?;
        Ok(self.output_dir.join(name))
    }

    pub fn validate(&self) -> Result<(), ExportError> {
        let _ = self.resolved_output_path()?;
        Ok(())
    }

    /// Estima o tamanho final do arquivo em bytes para a duração informada em nanossegundos.
    pub fn estimate_file_size_bytes(&self, duration_ns: u64) -> u64 {
        if duration_ns == 0 {
            return 0;
        }
        let total_kbps: u64 = match self.format {
            ExportFormat::AudioWav => 1_536,
            ExportFormat::AudioMp3 => u64::from(self.quality.audio_bitrate_kbps()),
            ExportFormat::Mp4H264 => {
                u64::from(self.quality.video_bitrate_kbps(self.resolution))
                    + u64::from(self.quality.audio_bitrate_kbps())
            }
            ExportFormat::MovProRes => {
                u64::from(self.quality.video_bitrate_kbps(self.resolution).max(45_000)) + 1_536
            }
        };
        let duration_secs = (duration_ns as f64) / 1_000_000_000.0;
        let bytes = ((total_kbps as f64) * 1_000.0 / 8.0) * duration_secs * 1.02;
        bytes.round().max(1.0) as u64
    }
}

fn strip_known_media_extension(name: &str) -> &str {
    const KNOWN_EXTS: [&str; 6] = [".mp4", ".mov", ".wav", ".mp3", ".mkv", ".avi"];
    let lower = name.to_ascii_lowercase();
    for ext in KNOWN_EXTS {
        if lower.ends_with(ext) && name.len() > ext.len() {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Identificador único de um trabalho na fila de exportação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExportJobId(pub Uuid);

impl ExportJobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ExportJobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ExportJobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Estado atual de execução de um trabalho de exportação.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExportJobStatus {
    Queued,
    Running { progress: f32, eta_seconds: Option<u64> },
    Completed { file_size_bytes: u64 },
    Failed(String),
    Cancelled,
}

/// Entrada da fila de exportação rastreada pela aplicação.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportJob {
    pub id: ExportJobId,
    pub settings: ExportSettings,
    pub output_path: PathBuf,
    pub duration_ns: u64,
    pub status: ExportJobStatus,
}

impl ExportJob {
    pub fn new(settings: ExportSettings, duration_ns: u64) -> Result<Self, ExportError> {
        if duration_ns == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        let output_path = settings.resolved_output_path()?;
        Ok(Self {
            id: ExportJobId::new(),
            settings,
            output_path,
            duration_ns,
            status: ExportJobStatus::Queued,
        })
    }
}
