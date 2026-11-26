use crate::effect_params::{ClipEffectSerde, EffectParameters};
use crate::timeline::TimelineError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Transformações espaciais, opacidade e velocidade aplicadas a um clipe na timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipTransform {
    pub scale: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub opacity: f32,
    pub speed: f32,
}

impl Default for ClipTransform {
    fn default() -> Self {
        Self { scale: 1.0, pos_x: 0.0, pos_y: 0.0, opacity: 1.0, speed: 1.0 }
    }
}

impl ClipTransform {
    /// Valida limites seguros de escala, posição, opacidade e velocidade do clipe.
    pub fn validate(&self) -> Result<(), TimelineError> {
        if !self.scale.is_finite() || self.scale <= 0.0 || self.scale > 10.0 {
            return Err(TimelineError::InvalidTransform {
                field: "scale",
                reason: "deve ser finito e estar no intervalo (0.0, 10.0]",
            });
        }
        if !self.pos_x.is_finite() {
            return Err(TimelineError::InvalidTransform {
                field: "pos_x",
                reason: "deve ser um número finito",
            });
        }
        if !self.pos_y.is_finite() {
            return Err(TimelineError::InvalidTransform {
                field: "pos_y",
                reason: "deve ser um número finito",
            });
        }
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err(TimelineError::InvalidTransform {
                field: "opacity",
                reason: "deve ser finito e estar no intervalo [0.0, 1.0]",
            });
        }
        if !self.speed.is_finite() || self.speed <= 0.0 || self.speed > 16.0 {
            return Err(TimelineError::InvalidTransform {
                field: "speed",
                reason: "deve ser finito e estar no intervalo (0.0, 16.0]",
            });
        }
        Ok(())
    }
}

/// Tipo de transição de entrada ou saída suportado pelo editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionKind {
    Fade,
    Zoom,
    Blur,
}

/// Extremidade de um clipe na qual a transição é aplicada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionEdge {
    In,
    Out,
}

/// Transição configurada na entrada ou saída de um clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipTransition {
    pub kind: TransitionKind,
    pub duration_ns: u64,
}

impl ClipTransition {
    pub fn new(kind: TransitionKind, duration_ns: u64) -> Self {
        Self { kind, duration_ns }
    }

    /// Garante que a duração da transição seja positiva e caiba dentro do clipe.
    pub fn validate(&self, clip_duration_ns: u64) -> Result<(), TimelineError> {
        if self.duration_ns == 0 {
            return Err(TimelineError::InvalidTransition {
                reason: "duração da transição deve ser maior que zero",
            });
        }
        if self.duration_ns > clip_duration_ns {
            return Err(TimelineError::InvalidTransition {
                reason: "duração da transição excede a duração do clipe",
            });
        }
        Ok(())
    }
}

/// Categorias de efeitos de vídeo aplicáveis a um clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VideoEffectKind {
    Glitch,
    Cinematic,
    #[serde(alias = "Film")]
    FilmStyle,
    Zoom,
    Blur,
    Transition,
}

pub type EffectKind = VideoEffectKind;

impl VideoEffectKind {
    /// Nome legível padrão do efeito na interface e no inspetor.
    pub const fn default_name(self) -> &'static str {
        match self {
            Self::Glitch => "Glitch",
            Self::Cinematic => "Cinemático",
            Self::FilmStyle => "Estilo Filme",
            Self::Zoom => "Zoom",
            Self::Blur => "Desfoque",
            Self::Transition => "Transição",
        }
    }

    /// Rótulo curto padrão para badges sobre o clipe na timeline.
    pub const fn short_badge_label(self) -> &'static str {
        match self {
            Self::Glitch => "Glitch",
            Self::Cinematic => "Cinema",
            Self::FilmStyle => "Filme",
            Self::Zoom => "Zoom",
            Self::Blur => "Blur",
            Self::Transition => "Fade",
        }
    }
}

/// Instância de efeito de vídeo anexada a um clipe com parâmetros editáveis individuais.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ClipEffectSerde")]
pub struct ClipEffect {
    pub id: Uuid,
    pub kind: VideoEffectKind,
    pub name: String,
    pub intensity: f32,
    pub enabled: bool,
    pub params: EffectParameters,
}

pub type VideoEffect = ClipEffect;

impl ClipEffect {
    /// Cria um efeito com intensidade inicial e parâmetros sincronizados (compatível com v0.1/v0.2).
    pub fn new(kind: VideoEffectKind, intensity: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            name: kind.default_name().to_string(),
            intensity,
            enabled: true,
            params: EffectParameters::default_for_with_intensity(kind, intensity),
        }
    }

    /// Cria um efeito usando os parâmetros padrão oficiais do seu tipo.
    pub fn with_defaults(kind: VideoEffectKind) -> Self {
        let params = EffectParameters::default_for(kind);
        Self::with_params(params)
    }

    /// Cria um efeito a partir de parâmetros específicos já configurados.
    pub fn with_params(params: EffectParameters) -> Self {
        let kind = params.kind();
        Self {
            id: Uuid::new_v4(),
            kind,
            name: kind.default_name().to_string(),
            intensity: params.primary_intensity(),
            enabled: true,
            params,
        }
    }

    /// Rótulo curto para exibição em badges na timeline.
    pub const fn short_badge_label(&self) -> &'static str {
        self.params.short_badge_label()
    }

    /// Valor padrão de um parâmetro individual do efeito para reset seletivo.
    pub fn default_param_value(&self, field: &str) -> Option<f64> {
        self.params.default_value(field)
    }

    /// Valida consistência de tipo, intensidade normalizada e todos os parâmetros individuais.
    pub fn validate(&self) -> Result<(), TimelineError> {
        if !self.intensity.is_finite() || !(0.0..=1.0).contains(&self.intensity) {
            return Err(TimelineError::InvalidEffect {
                field: "intensity",
                reason: "deve ser finito e estar no intervalo [0.0, 1.0]",
            });
        }
        if self.params.kind() != self.kind {
            return Err(TimelineError::InvalidEffect {
                field: "params",
                reason: "parâmetros não correspondem ao tipo do efeito",
            });
        }
        self.params.validate()
    }
}

/// Presets visuais de títulos e legendas inseridos na trilha `T1` (Overlay).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TitleStyle {
    #[default]
    ModernText,
    QuickCaption,
    Title3D,
}

/// Dados tipográficos e estéticos de um clipe de título/texto sobreposto.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TitleOverlay {
    pub text: String,
    pub style: TitleStyle,
    pub font_family: String,
    pub font_size: u32,
    pub color_rgba: [u8; 4],
}

impl TitleOverlay {
    /// Cria um título com tipografia e cor padrão coerentes com o preset visual.
    pub fn new(text: impl Into<String>, style: TitleStyle) -> Self {
        let (font_family, font_size, color_rgba) = match style {
            TitleStyle::ModernText => ("Sans", 48, [255, 255, 255, 255]),
            TitleStyle::QuickCaption => ("Sans", 32, [255, 238, 128, 255]),
            TitleStyle::Title3D => ("Sans Bold", 64, [115, 91, 242, 255]),
        };
        Self {
            text: text.into(),
            style,
            font_family: font_family.to_string(),
            font_size,
            color_rgba,
        }
    }

    /// Valida texto não vazio, família tipográfica válida e tamanho seguro da fonte.
    pub fn validate(&self) -> Result<(), TimelineError> {
        if self.text.trim().is_empty() {
            return Err(TimelineError::InvalidTitle {
                field: "text",
                reason: "o texto do título não pode estar vazio",
            });
        }
        if self.font_family.trim().is_empty() {
            return Err(TimelineError::InvalidTitle {
                field: "font_family",
                reason: "a família da fonte não pode estar vazia",
            });
        }
        if self.font_size == 0 || self.font_size > 400 {
            return Err(TimelineError::InvalidTitle {
                field: "font_size",
                reason: "tamanho da fonte deve estar no intervalo [1, 400]",
            });
        }
        Ok(())
    }
}

impl Default for TitleOverlay {
    fn default() -> Self {
        Self::new("Novo Título", TitleStyle::ModernText)
    }
}

/// Curva de interpolação simplificada para keyframes (Linear vs Suave).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum KeyframeEasing {
    #[default]
    Linear,
    #[serde(alias = "Ease")]
    EaseInOut,
}

/// Propriedade animável de um clipe via keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyframeProperty {
    PosX,
    PosY,
    Scale,
    Opacity,
}

impl KeyframeProperty {
    pub(crate) fn sort_rank(self) -> u8 {
        match self {
            Self::PosX => 0,
            Self::PosY => 1,
            Self::Scale => 2,
            Self::Opacity => 3,
        }
    }
}

/// Ponto de animação de parâmetro relativo ao início do clipe na timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    #[serde(alias = "time_ns")]
    pub time_offset_ns: u64,
    pub property: KeyframeProperty,
    pub value: f32,
    #[serde(default)]
    pub easing: KeyframeEasing,
}

impl Keyframe {
    pub fn new(
        time_offset_ns: u64,
        property: KeyframeProperty,
        value: f32,
        easing: KeyframeEasing,
    ) -> Self {
        Self { time_offset_ns, property, value, easing }
    }

    /// Valida instante dentro da duração do clipe e limites seguros por propriedade.
    pub fn validate(&self, clip_duration_ns: u64) -> Result<(), TimelineError> {
        if self.time_offset_ns > clip_duration_ns {
            return Err(TimelineError::InvalidKeyframe {
                field: "time_offset_ns",
                reason: "instante do keyframe excede a duração do clipe",
            });
        }
        if !self.value.is_finite() {
            return Err(TimelineError::InvalidKeyframe {
                field: "value",
                reason: "deve ser um número finito",
            });
        }
        match self.property {
            KeyframeProperty::Scale if self.value <= 0.0 || self.value > 10.0 => {
                Err(TimelineError::InvalidKeyframe {
                    field: "value",
                    reason: "escala em keyframe deve estar no intervalo (0.0, 10.0]",
                })
            }
            KeyframeProperty::Opacity if !(0.0..=1.0).contains(&self.value) => {
                Err(TimelineError::InvalidKeyframe {
                    field: "value",
                    reason: "opacidade em keyframe deve estar no intervalo [0.0, 1.0]",
                })
            }
            KeyframeProperty::PosX | KeyframeProperty::PosY | KeyframeProperty::Scale | KeyframeProperty::Opacity => {
                Ok(())
            }
        }
    }
}
