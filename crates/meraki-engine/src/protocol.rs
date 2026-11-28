use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use meraki_core::{
    AssetId, AudioEffectKind, ClipEffect, ClipId, ClipTransform, ClipTransition, EqBand,
    ExportJobId, ExportSettings, Keyframe, MediaMetadata, PreviewQuality, TitleOverlay,
    TrackAudioSettings, TrackId, TrackKind, VolumeKeyframe,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

pub use meraki_core::{ProxyMode, SpectrumBands, TimelineTransition};

/// Valor sentinela indicando que não há nenhum pedido de Seek pendente.
const NO_PENDING_SEEK: u64 = u64::MAX;

/// Descrição leve de um clipe da timeline para sincronização no motor GES.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRenderClip {
    pub clip_id: ClipId,
    pub track_id: TrackId,
    /// Mídia original, usada em exportação e como fonte de referência da edição.
    pub path: PathBuf,
    /// Proxy disponível da mídia original. Usado apenas no preview.
    pub proxy_path: Option<PathBuf>,
    pub track_kind: TrackKind,
    pub timeline_start_ns: u64,
    pub source_in_ns: u64,
    pub duration_ns: u64,
    pub transform: ClipTransform,
    pub title: Option<TitleOverlay>,
    pub effects: Vec<ClipEffect>,
    pub transition_in: Option<ClipTransition>,
    pub transition_out: Option<ClipTransition>,
    pub keyframes: Vec<Keyframe>,
    pub volume_keyframes: Vec<VolumeKeyframe>,
}

/// Configuração de mixagem/DSP associada a uma trilha enviada ao motor GES.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackAudioMix {
    pub track_id: TrackId,
    pub settings: TrackAudioSettings,
}

/// Melhorias aplicadas à cadeia DSP de uma trilha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioEnhancements {
    pub noise_reduction_enabled: bool,
    pub noise_reduction_strength: f32,
    pub voice_leveler_enabled: bool,
    pub reverb_enabled: bool,
    pub echo_enabled: bool,
}

impl From<&TrackAudioSettings> for AudioEnhancements {
    fn from(settings: &TrackAudioSettings) -> Self {
        Self {
            noise_reduction_enabled: settings.noise_reduction_enabled,
            noise_reduction_strength: settings.noise_reduction_strength,
            voice_leveler_enabled: settings.voice_leveler_enabled,
            reverb_enabled: settings.has_effect(AudioEffectKind::Reverb),
            echo_enabled: settings.has_effect(AudioEffectKind::Echo),
        }
    }
}

/// Quadro de vídeo decodificado pronto para envio à GPU (wgpu / Iced).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// Pixels em formato RGBA8 compactado (4 bytes por pixel), zero-copy via `Bytes`.
    pub rgba_pixels: Bytes,
    /// Timestamp de apresentação em nanossegundos.
    pub pts_ns: u64,
}

/// Miniatura RGBA8 compacta (até 320x180) gerada na importação de mídia, zero-copy via `Bytes`.
#[derive(Debug, Clone)]
pub struct ThumbnailFrame {
    pub asset_id: AssetId,
    pub width: u32,
    pub height: u32,
    pub rgba_pixels: Bytes,
}

/// Medição instantânea de volume de uma trilha para o Mixer de Áudio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackAudioLevel {
    pub track_id: TrackId,
    /// Pico em decibéis (tipicamente entre -60.0 e 0.0 dB).
    pub peak_db: f32,
    /// Valor RMS em decibéis.
    pub rms_db: f32,
}

/// Snapshot (~30 Hz) com medição em dB do barramento Master e das trilhas ativas.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeterFrame {
    pub master_peak_db: f32,
    pub master_rms_db: f32,
    pub tracks: Vec<TrackAudioLevel>,
}

/// Ordens enviadas da UI (`meraki-ui`) para o Motor (`meraki-engine`).
#[derive(Debug, Clone)]
pub enum EngineCommand {
    /// Solicita análise de metadados e geração de miniatura/proxy em segundo plano.
    ImportMedia {
        asset_id: AssetId,
        path: PathBuf,
        generate_proxy: bool,
    },
    SetProxyMode(ProxyMode),
    EnsureProxy { asset_id: AssetId, source_path: PathBuf, width: u32, height: u32 },
    ClearProxyCache,
    /// Controles básicos de transporte e sincronização da timeline GES.
    SyncTimeline {
        clips: Vec<TimelineRenderClip>,
        transitions: Vec<TimelineTransition>,
        audio_mix: Vec<TrackAudioMix>,
        preview_quality: PreviewQuality,
    },
    /// Campos ausentes preservam o valor atual. `effects` substitui a lista inteira.
    UpdateClipLive {
        clip_id: ClipId,
        transform: Option<ClipTransform>,
        title: Option<TitleOverlay>,
        effects: Option<Vec<ClipEffect>>,
    },
    /// -1 recua, 0 pausa e 1 avança. Pressões sucessivas aceleram na mesma direção.
    Shuttle { direction: i8 },
    Play,
    Pause,
    SetPlaybackRate(f64),
    SetMasterVolume(f32),
    SetPreviewQuality(PreviewQuality),

    /// Ajustes de áudio em tempo real (Aba Áudio).
    UpdateAudioLive {
        /// Identificador da trilha, ganho em dB e mute.
        track_volumes: Option<(TrackId, f64, bool)>,
        master_volume_db: Option<f64>,
        master_muted: Option<bool>,
        /// Ganhos em dB na ordem musical definida por `EQ_BAND_SPECS`.
        track_eq: Option<(TrackId, [f64; 4])>,
        track_enhancements: Option<(TrackId, AudioEnhancements)>,
    },
    /// Solicita a média recente do espectro, inclusive quando o preview está pausado.
    RequestAudioSpectrum,
    SetTrackGain {
        track_id: TrackId,
        gain_db: f32,
    },
    SetTrackMute {
        track_id: TrackId,
        muted: bool,
    },
    SetTrackSolo {
        track_id: TrackId,
        solo: bool,
    },
    UpdateEqBand {
        track_id: TrackId,
        band: EqBand,
        gain_db: f32,
    },
    SetNoiseReduction {
        track_id: TrackId,
        enabled: bool,
        strength: f32,
    },
    SetVoiceLeveler {
        track_id: TrackId,
        enabled: bool,
    },
    ToggleTrackAudioEffect {
        track_id: TrackId,
        effect: AudioEffectKind,
        enabled: bool,
    },

    /// Exportação final (Aba Entrega).
    StartExport {
        job_id: ExportJobId,
        settings: ExportSettings,
        clips: Vec<TimelineRenderClip>,
        transitions: Vec<TimelineTransition>,
        audio_tracks: Vec<TrackAudioMix>,
    },
    CancelExport {
        job_id: ExportJobId,
    },

    /// Encerra o GLib MainLoop e libera recursos do GStreamer com segurança.
    Shutdown,
}

/// Eventos enviados do Motor (`meraki-engine`) de volta para a UI (`meraki-ui`).
#[derive(Debug, Clone)]
pub enum EngineEvent {
    EngineReady,
    MediaDiscovered(MediaMetadata),
    ThumbnailReady(ThumbnailFrame),
    ProxyGenerating { asset_id: AssetId },
    ProxyReady { asset_id: AssetId, proxy_path: PathBuf },
    ProxyFailed { asset_id: AssetId, reason: String },
    ProxyCacheCleared { freed_bytes: u64 },
    ProxyProgress {
        asset_id: AssetId,
        /// Progresso entre 0.0 e 1.0.
        progress: f32,
        proxy_path: Option<PathBuf>,
    },
    /// Novo quadro pronto para renderização no Player.
    FrameReady(VideoFrame),
    /// Posição atual da agulha de reprodução em nanossegundos.
    PlayheadTick(u64),
    ShuttleRateChanged { shuttle_rate: f64 },
    /// O motor ainda não recebeu o snapshot que contém este clipe.
    TimelineSyncRequired { clip_id: ClipId },
    TitleBounds { clip_id: ClipId, bounds: Option<meraki_core::NormalizedTitleBox> },
    /// Níveis de áudio atualizados (~30 Hz) para o Mixer vertical.
    AudioLevels(AudioMeterFrame),
    AudioSpectrum(SpectrumBands),
    ExportProgress {
        job_id: ExportJobId,
        progress: f32,
        eta_seconds: Option<u64>,
    },
    ExportFinished {
        job_id: ExportJobId,
        output_path: PathBuf,
        file_size_bytes: u64,
    },
    ExportFailed {
        job_id: ExportJobId,
        error: String,
    },
    ExportCancelled {
        job_id: ExportJobId,
    },
    Error(String),
}

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("fila de comandos do motor está cheia; ação ignorada para evitar travamento da UI")]
    CommandQueueFull,
    #[error("motor multimídia desconectado")]
    EngineDisconnected,
}

/// Extremidade usada pela UI (`meraki-ui`). Nunca bloqueia a thread principal.
#[derive(Clone)]
pub struct EngineHandle {
    cmd_tx: Sender<EngineCommand>,
    event_rx: Receiver<EngineEvent>,
    pending_seek_ns: Arc<AtomicU64>,
}

/// Extremidade exclusiva da thread do motor (`meraki-engine`).
pub struct EngineWorkerChannel {
    cmd_rx: Receiver<EngineCommand>,
    event_tx: Sender<EngineEvent>,
    pending_seek_ns: Arc<AtomicU64>,
}

/// Cria o par de canais assíncronos entre UI e Motor.
pub fn create_engine_channel() -> (EngineHandle, EngineWorkerChannel) {
    let (cmd_tx, cmd_rx) = bounded(256);
    let (event_tx, event_rx) = bounded(128);
    let pending_seek_ns = Arc::new(AtomicU64::new(NO_PENDING_SEEK));

    let handle = EngineHandle {
        cmd_tx,
        event_rx,
        pending_seek_ns: Arc::clone(&pending_seek_ns),
    };

    let worker = EngineWorkerChannel {
        cmd_rx,
        event_tx,
        pending_seek_ns,
    };

    (handle, worker)
}

impl EngineHandle {
    /// Envia um comando sem bloquear a thread da UI.
    pub fn send(&self, cmd: EngineCommand) -> Result<(), BridgeError> {
        self.cmd_tx.try_send(cmd).map_err(|err| match err {
            TrySendError::Full(_) => BridgeError::CommandQueueFull,
            TrySendError::Disconnected(_) => BridgeError::EngineDisconnected,
        })
    }

    /// Solicita um Seek com coalescência atômica (latest-value wins).
    /// Mesmo com 1000 eventos de mouse/seg durante scrubbing, o motor lê apenas o timestamp mais recente.
    pub fn request_seek(&self, position_ns: u64) {
        let clamped = position_ns.min(NO_PENDING_SEEK - 1);
        self.pending_seek_ns.store(clamped, Ordering::Release);
    }

    /// Receptor usado pela `iced::Subscription` para atualizar a interface.
    pub fn events(&self) -> &Receiver<EngineEvent> {
        &self.event_rx
    }
}

impl EngineWorkerChannel {
    /// Lê o próximo comando disponível sem bloquear o loop do motor.
    pub fn try_recv_command(&self) -> Option<EngineCommand> {
        self.cmd_rx.try_recv().ok()
    }

    /// Lê o próximo comando distinguindo fila vazia de canal desconectado.
    pub fn try_recv_command_checked(&self) -> Result<Option<EngineCommand>, BridgeError> {
        match self.cmd_rx.try_recv() {
            Ok(cmd) => Ok(Some(cmd)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(BridgeError::EngineDisconnected),
        }
    }

    /// Consome o último pedido de Seek pendente, caso exista.
    pub fn take_pending_seek(&self) -> Option<u64> {
        let value = self.pending_seek_ns.swap(NO_PENDING_SEEK, Ordering::AcqRel);
        if value == NO_PENDING_SEEK {
            None
        } else {
            Some(value)
        }
    }

    /// Emite evento de alta frequência (quadro de vídeo / VU meter).
    /// Se a UI estiver atrasada, descarta silenciosamente para nunca travar o pipeline GStreamer.
    pub fn emit_drop_if_full(&self, event: EngineEvent) {
        self.try_emit(event);
    }

    pub(crate) fn try_emit(&self, event: EngineEvent) -> bool {
        self.event_tx.try_send(event).is_ok()
    }

    /// Emite evento crítico de estado (erro, descoberta de mídia, fim de exportação).
    pub fn emit_critical(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event);
    }
}
