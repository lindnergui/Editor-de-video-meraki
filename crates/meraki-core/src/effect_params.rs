use crate::effects::{ClipEffect, VideoEffectKind};
use crate::timeline::TimelineError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Curva de interpolação aplicada ao efeito de Zoom dinâmico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ZoomEasing {
    Linear,
    #[default]
    EaseInOut,
    Constant,
}

/// Parâmetros editáveis do efeito de Zoom em um clipe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoomParams {
    pub start_scale: f64,
    pub end_scale: f64,
    pub focus_x: f64,
    pub focus_y: f64,
    pub easing: ZoomEasing,
}

impl Default for ZoomParams {
    fn default() -> Self {
        Self {
            start_scale: 1.0,
            end_scale: 1.25,
            focus_x: 0.0,
            focus_y: 0.0,
            easing: ZoomEasing::EaseInOut,
        }
    }
}

/// Parâmetros editáveis do efeito Filme (gradação e vinheta).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmParams {
    pub intensity: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub vignette: f64,
}

impl Default for FilmParams {
    fn default() -> Self {
        Self {
            intensity: 0.65,
            contrast: 1.12,
            saturation: 0.85,
            vignette: 0.35,
        }
    }
}

/// Parâmetros editáveis do efeito Cinemático.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CinematicParams {
    pub intensity: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub brightness: f64,
}

impl Default for CinematicParams {
    fn default() -> Self {
        Self {
            intensity: 0.7,
            contrast: 1.25,
            saturation: 0.82,
            brightness: -0.06,
        }
    }
}

/// Parâmetros editáveis do efeito Glitch.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlitchParams {
    pub intensity: f64,
    pub rgb_shift: f64,
    pub frequency: f64,
}

impl Default for GlitchParams {
    fn default() -> Self {
        Self {
            intensity: 0.6,
            rgb_shift: 0.4,
            frequency: 0.5,
        }
    }
}

/// Parâmetros editáveis do efeito Desfoque (Blur).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlurParams {
    pub radius: f64,
}

impl Default for BlurParams {
    fn default() -> Self {
        Self { radius: 0.35 }
    }
}

/// Subtipo de efeito de transição configurável no inspetor de efeitos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TransitionEffectKind {
    #[default]
    Fade,
    ZoomTransition,
    BlurTransition,
}

/// Parâmetros editáveis de um efeito de transição anexado ao clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionParams {
    pub kind: TransitionEffectKind,
    pub duration_ms: u64,
    pub apply_in: bool,
    pub apply_out: bool,
}

impl Default for TransitionParams {
    fn default() -> Self {
        Self {
            kind: TransitionEffectKind::Fade,
            duration_ms: 500,
            apply_in: true,
            apply_out: true,
        }
    }
}

/// Conjunto tipado de parâmetros individuais de cada efeito aplicável a um clipe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EffectParameters {
    Zoom(ZoomParams),
    Film(FilmParams),
    Cinematic(CinematicParams),
    Glitch(GlitchParams),
    Blur(BlurParams),
    Transition(TransitionParams),
}

fn validate_unit_f64(value: f64, field: &'static str) -> Result<(), TimelineError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(TimelineError::InvalidEffect {
            field,
            reason: "deve ser finito e estar no intervalo [0.0, 1.0]",
        });
    }
    Ok(())
}

fn validate_scale_f64(value: f64, field: &'static str) -> Result<(), TimelineError> {
    if !value.is_finite() || value <= 0.0 || value > 10.0 {
        return Err(TimelineError::InvalidEffect {
            field,
            reason: "deve ser finito e estar no intervalo (0.0, 10.0]",
        });
    }
    Ok(())
}

fn validate_range_f64(
    value: f64,
    min: f64,
    max: f64,
    field: &'static str,
    reason: &'static str,
) -> Result<(), TimelineError> {
    if !value.is_finite() || value < min || value > max {
        return Err(TimelineError::InvalidEffect { field, reason });
    }
    Ok(())
}

impl EffectParameters {
    /// Retorna os parâmetros padrão para o tipo de efeito informado.
    pub fn default_for(kind: VideoEffectKind) -> Self {
        match kind {
            VideoEffectKind::Zoom => Self::Zoom(ZoomParams::default()),
            VideoEffectKind::FilmStyle => Self::Film(FilmParams::default()),
            VideoEffectKind::Cinematic => Self::Cinematic(CinematicParams::default()),
            VideoEffectKind::Glitch => Self::Glitch(GlitchParams::default()),
            VideoEffectKind::Blur => Self::Blur(BlurParams::default()),
            VideoEffectKind::Transition => Self::Transition(TransitionParams::default()),
        }
    }

    /// Retorna os parâmetros padrão ajustando o parâmetro principal para `intensity` (`0.0..=1.0`).
    pub fn default_for_with_intensity(kind: VideoEffectKind, intensity: f32) -> Self {
        let mut params = Self::default_for(kind);
        params.set_primary_intensity(intensity);
        params
    }

    /// Tipo base do efeito correspondente a estes parâmetros.
    pub const fn kind(&self) -> VideoEffectKind {
        match self {
            Self::Zoom(_) => VideoEffectKind::Zoom,
            Self::Film(_) => VideoEffectKind::FilmStyle,
            Self::Cinematic(_) => VideoEffectKind::Cinematic,
            Self::Glitch(_) => VideoEffectKind::Glitch,
            Self::Blur(_) => VideoEffectKind::Blur,
            Self::Transition(_) => VideoEffectKind::Transition,
        }
    }

    /// Intensidade normalizada (`0.0..=1.0`) usada pelos filtros que operam com fator escalar.
    pub fn primary_intensity(&self) -> f32 {
        match self {
            Self::Zoom(p) => ((p.end_scale - 1.0) / 1.0).clamp(0.0, 1.0) as f32,
            Self::Film(p) => p.intensity.clamp(0.0, 1.0) as f32,
            Self::Cinematic(p) => p.intensity.clamp(0.0, 1.0) as f32,
            Self::Glitch(p) => p.intensity.clamp(0.0, 1.0) as f32,
            Self::Blur(p) => p.radius.clamp(0.0, 1.0) as f32,
            Self::Transition(_) => 1.0,
        }
    }

    /// Atualiza o parâmetro principal de intensidade mantendo os demais parâmetros intactos.
    pub fn set_primary_intensity(&mut self, intensity: f32) {
        let clamped = f64::from(intensity.clamp(0.0, 1.0));
        match self {
            Self::Zoom(p) => p.end_scale = 1.0 + clamped,
            Self::Film(p) => p.intensity = clamped,
            Self::Cinematic(p) => p.intensity = clamped,
            Self::Glitch(p) => p.intensity = clamped,
            Self::Blur(p) => p.radius = clamped,
            Self::Transition(_) => {}
        }
    }

    /// Retorna o valor numérico padrão de um campo individual para reset no inspetor.
    pub fn default_value(&self, field: &str) -> Option<f64> {
        match self {
            Self::Zoom(_) => match field {
                "start_scale" => Some(ZoomParams::default().start_scale),
                "end_scale" => Some(ZoomParams::default().end_scale),
                "focus_x" => Some(ZoomParams::default().focus_x),
                "focus_y" => Some(ZoomParams::default().focus_y),
                _ => None,
            },
            Self::Film(_) => match field {
                "intensity" => Some(FilmParams::default().intensity),
                "contrast" => Some(FilmParams::default().contrast),
                "saturation" => Some(FilmParams::default().saturation),
                "vignette" => Some(FilmParams::default().vignette),
                _ => None,
            },
            Self::Cinematic(_) => match field {
                "intensity" => Some(CinematicParams::default().intensity),
                "contrast" => Some(CinematicParams::default().contrast),
                "saturation" => Some(CinematicParams::default().saturation),
                "brightness" => Some(CinematicParams::default().brightness),
                _ => None,
            },
            Self::Glitch(_) => match field {
                "intensity" => Some(GlitchParams::default().intensity),
                "rgb_shift" => Some(GlitchParams::default().rgb_shift),
                "frequency" => Some(GlitchParams::default().frequency),
                _ => None,
            },
            Self::Blur(_) => match field {
                "radius" | "intensity" => Some(BlurParams::default().radius),
                _ => None,
            },
            Self::Transition(_) => match field {
                "duration_ms" => Some(TransitionParams::default().duration_ms as f64),
                _ => None,
            },
        }
    }

    /// Rótulo curto para badges exibidos sobre clipes na timeline.
    pub const fn short_badge_label(&self) -> &'static str {
        match self {
            Self::Zoom(_) => "Zoom",
            Self::Film(_) => "Filme",
            Self::Cinematic(_) => "Cinema",
            Self::Glitch(_) => "Glitch",
            Self::Blur(_) => "Blur",
            Self::Transition(p) => match p.kind {
                TransitionEffectKind::Fade => "Fade",
                TransitionEffectKind::ZoomTransition => "Zoom",
                TransitionEffectKind::BlurTransition => "Blur",
            },
        }
    }

    /// Valida intervalos seguros e finitude de todos os campos dos parâmetros.
    pub fn validate(&self) -> Result<(), TimelineError> {
        match self {
            Self::Zoom(p) => {
                validate_scale_f64(p.start_scale, "start_scale")?;
                validate_scale_f64(p.end_scale, "end_scale")?;
                validate_range_f64(p.focus_x, -1.0, 1.0, "focus_x", "deve estar em [-1.0, 1.0]")?;
                validate_range_f64(p.focus_y, -1.0, 1.0, "focus_y", "deve estar em [-1.0, 1.0]")
            }
            Self::Film(p) => {
                validate_unit_f64(p.intensity, "intensity")?;
                validate_range_f64(p.contrast, 0.0, 3.0, "contrast", "deve estar em [0.0, 3.0]")?;
                validate_range_f64(p.saturation, 0.0, 3.0, "saturation", "deve estar em [0.0, 3.0]")?;
                validate_unit_f64(p.vignette, "vignette")
            }
            Self::Cinematic(p) => {
                validate_unit_f64(p.intensity, "intensity")?;
                validate_range_f64(p.contrast, 0.0, 3.0, "contrast", "deve estar em [0.0, 3.0]")?;
                validate_range_f64(p.saturation, 0.0, 3.0, "saturation", "deve estar em [0.0, 3.0]")?;
                validate_range_f64(p.brightness, -1.0, 1.0, "brightness", "deve estar em [-1.0, 1.0]")
            }
            Self::Glitch(p) => {
                validate_unit_f64(p.intensity, "intensity")?;
                validate_unit_f64(p.rgb_shift, "rgb_shift")?;
                validate_unit_f64(p.frequency, "frequency")
            }
            Self::Blur(p) => validate_unit_f64(p.radius, "radius"),
            Self::Transition(p) => {
                if p.duration_ms == 0 || p.duration_ms > 60_000 {
                    return Err(TimelineError::InvalidEffect {
                        field: "duration_ms",
                        reason: "deve estar no intervalo [1, 60000]",
                    });
                }
                Ok(())
            }
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub(crate) struct ClipEffectSerde {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub kind: VideoEffectKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub intensity: Option<f32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub params: Option<EffectParameters>,
}

impl From<ClipEffectSerde> for ClipEffect {
    fn from(raw: ClipEffectSerde) -> Self {
        let params = raw
            .params
            .filter(|p| p.kind() == raw.kind)
            .unwrap_or_else(|| match raw.intensity {
                Some(val) => EffectParameters::default_for_with_intensity(raw.kind, val),
                None => EffectParameters::default_for(raw.kind),
            });
        let intensity = raw.intensity.unwrap_or_else(|| params.primary_intensity());
        let name = raw
            .name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| raw.kind.default_name().to_string());
        Self {
            id: raw.id.unwrap_or_else(Uuid::new_v4),
            kind: raw.kind,
            name,
            intensity,
            enabled: raw.enabled,
            params,
        }
    }
}
