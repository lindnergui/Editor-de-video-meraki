use crate::project::EqBand;
use crate::timeline::TimelineError;
use serde::{Deserialize, Serialize};

pub const MIN_TRACK_GAIN_DB: f32 = -60.0;
pub const MAX_TRACK_GAIN_DB: f32 = 12.0;
pub const MIN_EQ_GAIN_DB: f32 = -12.0;
pub const MAX_EQ_GAIN_DB: f32 = 12.0;
pub const MIN_VOLUME_KF_DB: f32 = -48.0;
pub const MAX_VOLUME_KF_DB: f32 = 12.0;
pub const EQ_BAND_FREQUENCIES_HZ: [f64; 4] = [100.0, 400.0, 2_000.0, 8_000.0];

/// Presets de voz exibidos na aba Áudio para equalização e melhoria instantânea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoicePreset {
    Podcast,
    ClearVoice,
    EnhanceVoice,
    Radio,
}

impl VoicePreset {
    pub const ALL: [VoicePreset; 4] =
        [Self::Podcast, Self::ClearVoice, Self::EnhanceVoice, Self::Radio];

    pub fn label(self) -> &'static str {
        match self {
            Self::Podcast => "Voz de Podcast",
            Self::ClearVoice => "Voz Clara",
            Self::EnhanceVoice => "Melhorar Voz",
            Self::Radio => "Voz de Rádio",
        }
    }

    /// Ganhos em dB das 4 bandas (`[Graves, Médios-Graves, Médios, Agudos]`), entre `-12.0` e `+12.0`.
    pub fn eq_gains_db(self) -> [f32; 4] {
        match self {
            Self::Podcast => [3.5, 1.5, 2.5, 1.5],
            Self::ClearVoice => [-2.0, -1.0, 3.5, 4.0],
            Self::EnhanceVoice => [1.5, 0.5, 3.0, 2.5],
            Self::Radio => [-6.0, 2.0, 6.0, -4.0],
        }
    }

    /// Sugestão de redução de ruído `(habilitada, intensidade 0.0..=1.0)` para este preset.
    pub fn suggested_noise_reduction(self) -> (bool, f32) {
        match self {
            Self::Podcast => (true, 0.45),
            Self::ClearVoice => (true, 0.65),
            Self::EnhanceVoice => (true, 0.55),
            Self::Radio => (true, 0.35),
        }
    }

    /// Indica se o nivelador automático de voz é recomendado para o preset.
    pub fn suggested_voice_leveler(self) -> bool {
        true
    }

    /// Aplica curvas de EQ e melhorias de voz preservando fader, Mute, Solo e efeitos da trilha.
    pub fn apply_to(self, settings: &mut TrackAudioSettings) {
        let (nr_enabled, nr_strength) = self.suggested_noise_reduction();
        settings.eq_gains_db = self.eq_gains_db();
        settings.noise_reduction_enabled = nr_enabled;
        settings.noise_reduction_strength = nr_strength;
        settings.voice_leveler_enabled = self.suggested_voice_leveler();
        settings.voice_preset = Some(self);
    }
}

/// Efeitos espaciais e criativos aplicáveis em trilhas de áudio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioEffectKind {
    Reverb,
    Echo,
}

impl AudioEffectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reverb => "Reverb",
            Self::Echo => "Eco",
        }
    }
}

/// Ponto de automação da linha de volume de um clipe em dB (`-48.0` a `+12.0`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolumeKeyframe {
    pub time_offset_ns: u64,
    pub gain_db: f32,
}

impl VolumeKeyframe {
    pub fn new(time_offset_ns: u64, gain_db: f32) -> Self {
        Self { time_offset_ns, gain_db }
    }

    pub fn clamped(time_offset_ns: u64, gain_db: f32) -> Self {
        let safe_db = if gain_db.is_finite() {
            gain_db.clamp(MIN_VOLUME_KF_DB, MAX_VOLUME_KF_DB)
        } else {
            0.0
        };
        Self { time_offset_ns, gain_db: safe_db }
    }

    pub fn validate(&self, clip_duration_ns: u64) -> Result<(), TimelineError> {
        if self.time_offset_ns > clip_duration_ns {
            return Err(TimelineError::InvalidAudioSetting {
                field: "volume_keyframe.time_offset_ns",
                reason: "deslocamento excede a duração do clipe",
            });
        }
        if !self.gain_db.is_finite()
            || !(MIN_VOLUME_KF_DB..=MAX_VOLUME_KF_DB).contains(&self.gain_db)
        {
            return Err(TimelineError::InvalidAudioSetting {
                field: "volume_keyframe.gain_db",
                reason: "ganho deve ser finito e estar entre -48.0 e +12.0 dB",
            });
        }
        Ok(())
    }
}

/// Configuração completa de mixagem, equalização e melhorias de voz de uma trilha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackAudioSettings {
    pub gain_db: f32,
    pub muted: bool,
    pub solo: bool,
    pub eq_gains_db: [f32; 4],
    pub noise_reduction_enabled: bool,
    pub noise_reduction_strength: f32,
    pub voice_leveler_enabled: bool,
    pub voice_preset: Option<VoicePreset>,
    pub effects: Vec<AudioEffectKind>,
}

impl Default for TrackAudioSettings {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            muted: false,
            solo: false,
            eq_gains_db: [0.0; 4],
            noise_reduction_enabled: false,
            noise_reduction_strength: 0.5,
            voice_leveler_enabled: false,
            voice_preset: None,
            effects: Vec::new(),
        }
    }
}

impl TrackAudioSettings {
    pub fn validate(&self) -> Result<(), TimelineError> {
        if !self.gain_db.is_finite()
            || !(MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB).contains(&self.gain_db)
        {
            return Err(TimelineError::InvalidAudioSetting {
                field: "gain_db",
                reason: "ganho da trilha deve estar entre -60.0 e +12.0 dB",
            });
        }
        for band_db in self.eq_gains_db {
            if !band_db.is_finite() || !(MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&band_db) {
                return Err(TimelineError::InvalidAudioSetting {
                    field: "eq_gains_db",
                    reason: "ganho de banda do EQ deve estar entre -12.0 e +12.0 dB",
                });
            }
        }
        if !self.noise_reduction_strength.is_finite()
            || !(0.0..=1.0).contains(&self.noise_reduction_strength)
        {
            return Err(TimelineError::InvalidAudioSetting {
                field: "noise_reduction_strength",
                reason: "intensidade da redução de ruído deve estar entre 0.0 e 1.0",
            });
        }
        Ok(())
    }

    pub fn clamped(self) -> Self {
        let mut effects = Vec::with_capacity(self.effects.len());
        for kind in self.effects {
            if !effects.contains(&kind) {
                effects.push(kind);
            }
        }
        Self {
            gain_db: sanitize_clamped(self.gain_db, 0.0, MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB),
            muted: self.muted,
            solo: self.solo,
            eq_gains_db: self
                .eq_gains_db
                .map(|v| sanitize_clamped(v, 0.0, MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB)),
            noise_reduction_enabled: self.noise_reduction_enabled,
            noise_reduction_strength: sanitize_clamped(
                self.noise_reduction_strength,
                0.5,
                0.0,
                1.0,
            ),
            voice_leveler_enabled: self.voice_leveler_enabled,
            voice_preset: self.voice_preset,
            effects,
        }
    }

    pub fn has_effect(&self, kind: AudioEffectKind) -> bool {
        self.effects.contains(&kind)
    }

    pub fn set_effect(&mut self, kind: AudioEffectKind, enabled: bool) {
        if enabled {
            if !self.effects.contains(&kind) {
                self.effects.push(kind);
            }
        } else {
            self.effects.retain(|&e| e != kind);
        }
    }
}

/// Converte uma banda do equalizador (`EqBand`) no índice fixo `0..4`.
pub const fn eq_band_index(band: EqBand) -> usize {
    match band {
        EqBand::Low => 0,
        EqBand::LowMid => 1,
        EqBand::Mid => 2,
        EqBand::High => 3,
    }
}

/// Converte ganho em decibéis para fator de ganho linear compatível com `volume` (`0.0..=10.0`).
pub fn db_to_linear(db: f32) -> f64 {
    if !db.is_finite() || db <= MIN_TRACK_GAIN_DB {
        return 0.0;
    }
    10_f64.powf(f64::from(db) / 20.0).clamp(0.0, 10.0)
}

fn sanitize_clamped(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}
