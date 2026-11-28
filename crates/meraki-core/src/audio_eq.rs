use crate::audio::{eq_band_index, TrackAudioSettings, VoicePreset, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB};
use crate::project::EqBand;
use serde::{Deserialize, Serialize};

pub const AUTO_EQ_DAMPING_FACTOR: f64 = 0.55;
pub const AUTO_EQ_MIN_CORRECTION_DB: f64 = -4.5;
pub const AUTO_EQ_MAX_CORRECTION_DB: f64 = 4.5;

/// Tipo de filtro paramétrico associado a cada banda do equalizador de 4 bandas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EqFilterType {
    LowShelf,
    Peak,
    HighShelf,
}

/// Especificação musical explícita de uma banda do equalizador paramétrico.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqBandSpec {
    pub label: &'static str,
    pub freq_hz: f64,
    pub bandwidth_hz: f64,
    pub filter_type: EqFilterType,
}

/// Especificação padrão das 4 bandas musicais do Meraki.
pub const EQ_BAND_SPECS: [EqBandSpec; 4] = [
    EqBandSpec {
        label: "Graves",
        freq_hz: 90.0,
        bandwidth_hz: 140.0,
        filter_type: EqFilterType::LowShelf,
    },
    EqBandSpec {
        label: "Médios-Graves",
        freq_hz: 320.0,
        bandwidth_hz: 300.0,
        filter_type: EqFilterType::Peak,
    },
    EqBandSpec {
        label: "Médios",
        freq_hz: 2_200.0,
        bandwidth_hz: 1_800.0,
        filter_type: EqFilterType::Peak,
    },
    EqBandSpec {
        label: "Agudos",
        freq_hz: 8_500.0,
        bandwidth_hz: 5_000.0,
        filter_type: EqFilterType::HighShelf,
    },
];

/// Larguras de banda (Hz) correspondentes às 4 faixas musicais do equalizador.
pub const EQ_BAND_WIDTHS_HZ: [f64; 4] = [140.0, 300.0, 1_800.0, 5_000.0];

/// Retorna a especificação musical (`EqBandSpec`) de uma banda específica.
pub const fn eq_band_spec(band: EqBand) -> EqBandSpec {
    EQ_BAND_SPECS[eq_band_index(band)]
}

/// Garante ganho finito e seguro de banda EQ entre `-12.0 dB` e `+12.0 dB`.
pub fn clamp_eq_gain_db(gain_db: f32) -> f32 {
    if gain_db.is_finite() {
        let clamped = gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB);
        if clamped == 0.0 { 0.0 } else { clamped }
    } else {
        0.0
    }
}

/// Aplica clamping seguro em todas as 4 bandas de EQ (`[-12.0, +12.0] dB`).
pub fn clamp_eq_gains_db(gains_db: [f32; 4]) -> [f32; 4] {
    gains_db.map(clamp_eq_gain_db)
}

/// Presets reais de áudio com curvas musicais suaves e melhorias opcionais de voz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioPreset {
    YouTubeVoice,
    Music,
    Podcast,
    ClearVoice,
    Cinema,
    Radio,
    Flat,
}

impl AudioPreset {
    pub const ALL: [AudioPreset; 7] = [
        Self::YouTubeVoice,
        Self::Music,
        Self::Podcast,
        Self::ClearVoice,
        Self::Cinema,
        Self::Radio,
        Self::Flat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::YouTubeVoice => "Fala / YouTube",
            Self::Music => "Música",
            Self::Podcast => "Podcast",
            Self::ClearVoice => "Voz Clara",
            Self::Cinema => "Cinema",
            Self::Radio => "Voz de Rádio",
            Self::Flat => "Padrão / Plano",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::YouTubeVoice => "Limpa graves abafados, destaca a clareza da fala e ativa nivelador.",
            Self::Music => "Graves firmes e brilho suave para trilhas musicais.",
            Self::Podcast => "Presença vocal quente com nivelador automático ativo.",
            Self::ClearVoice => "Foco máximo em inteligibilidade para locuções e tutoriais.",
            Self::Cinema => "Extensão encorpada nos graves e definição cinematográfica nos agudos.",
            Self::Radio => "Timbre clássico focado em médios característico de transmissão.",
            Self::Flat => "Curva neutra em 0 dB sem alterações de equalização.",
        }
    }

    /// Ganhos em dB das 4 bandas (`[Graves, Médios-Graves, Médios, Agudos]`).
    pub fn eq_gains_db(self) -> [f32; 4] {
        match self {
            Self::YouTubeVoice => [-2.5, -1.5, 3.0, 1.8],
            Self::Music => [2.2, -0.8, 0.8, 2.0],
            Self::Podcast => [1.2, -1.0, 2.8, 1.5],
            Self::ClearVoice => [-4.0, -2.0, 3.8, 2.2],
            Self::Cinema => [3.0, -0.5, 1.2, 2.4],
            Self::Radio => [-6.0, 2.5, 4.0, -3.5],
            Self::Flat => [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Sugestão opcional para ativação/desativação do nivelador de voz.
    pub fn voice_leveler(self) -> Option<bool> {
        match self {
            Self::YouTubeVoice | Self::Podcast => Some(true),
            Self::Music | Self::Cinema | Self::Flat => Some(false),
            Self::ClearVoice | Self::Radio => None,
        }
    }

    /// Sugestão opcional para ativação/desativação da redução de ruído.
    pub fn noise_reduction(self) -> Option<bool> {
        match self {
            Self::ClearVoice => Some(true),
            Self::Music | Self::Cinema | Self::Flat => Some(false),
            Self::YouTubeVoice | Self::Podcast | Self::Radio => None,
        }
    }

    /// Mapeia para o `VoicePreset` legado equivalente quando houver correspondência direta.
    pub fn to_legacy_voice_preset(self) -> Option<VoicePreset> {
        match self {
            Self::Podcast => Some(VoicePreset::Podcast),
            Self::ClearVoice => Some(VoicePreset::ClearVoice),
            Self::Radio => Some(VoicePreset::Radio),
            Self::YouTubeVoice | Self::Music | Self::Cinema | Self::Flat => None,
        }
    }

    /// Converte um `VoicePreset` legado para o `AudioPreset` correspondente.
    pub fn from_voice_preset(preset: VoicePreset) -> Self {
        match preset {
            VoicePreset::Podcast => Self::Podcast,
            VoicePreset::ClearVoice => Self::ClearVoice,
            VoicePreset::EnhanceVoice => Self::YouTubeVoice,
            VoicePreset::Radio => Self::Radio,
        }
    }

    /// Aplica a curva do preset e flags opcionais preservando fader, Mute, Solo e efeitos da trilha.
    pub fn apply_to(self, settings: &mut TrackAudioSettings) {
        settings.eq_gains_db = clamp_eq_gains_db(self.eq_gains_db());
        if let Some(leveler) = self.voice_leveler() {
            settings.voice_leveler_enabled = leveler;
        }
        if let Some(nr) = self.noise_reduction() {
            settings.noise_reduction_enabled = nr;
        }
        settings.audio_preset = Some(self);
        settings.voice_preset = self.to_legacy_voice_preset();
    }
}

/// Alvo tonal usado pelo algoritmo de Auto-EQ suave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutoEqTarget {
    SpeechYouTube,
    Music,
}

impl AutoEqTarget {
    /// Perfil alvo de energia relativa (média zero) para as 4 bandas em dB.
    pub const fn target_profile_db(self) -> [f64; 4] {
        match self {
            Self::SpeechYouTube => [-2.0, -1.0, 2.2, 0.8],
            Self::Music => [1.0, -1.0, -0.8, 0.8],
        }
    }
}

/// Energia medida nas 4 bandas espectrais (em dB) para análise de Auto-EQ.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpectrumBands {
    pub bands_db: [f64; 4],
}

impl SpectrumBands {
    pub const fn new(bands_db: [f64; 4]) -> Self {
        Self { bands_db }
    }
}

/// Calcula uma correção suave e agradável em dB (`[-4.5, +4.5]`) comparando o equilíbrio tonal
/// normalizado das 4 faixas medidas com o perfil alvo (`factor = 0.55`).
pub fn compute_auto_eq(measured: SpectrumBands, target: AutoEqTarget) -> [f64; 4] {
    if !measured.bands_db.iter().all(|v| v.is_finite()) {
        return [0.0; 4];
    }
    let mean_db = measured.bands_db.iter().sum::<f64>() / 4.0;
    let target_profile = target.target_profile_db();
    let mut correction = [0.0; 4];
    for i in 0..4 {
        let normalized = measured.bands_db[i] - mean_db;
        let delta = target_profile[i] - normalized;
        let damped = (delta * AUTO_EQ_DAMPING_FACTOR)
            .clamp(AUTO_EQ_MIN_CORRECTION_DB, AUTO_EQ_MAX_CORRECTION_DB);
        correction[i] = if damped == 0.0 { 0.0 } else { damped };
    }
    correction
}

/// Converte a saída de `compute_auto_eq` em ganhos `f32` com clamping seguro para `TrackAudioSettings`.
pub fn auto_eq_gains_f32(correction_db: [f64; 4]) -> [f32; 4] {
    correction_db.map(|db| clamp_eq_gain_db(db as f32))
}
