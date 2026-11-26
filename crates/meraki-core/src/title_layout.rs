use crate::{ClipTransform, TitleStyle};

/// Coordenadas de títulos no quadro de referência 1920 × 1080 do editor.
/// O tamanho nominal da fonte incorpora a escala do clipe. GES e a UI usam a
/// mesma âncora. A caixa dos glifos usa as métricas efetivas de textoverlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitlePose {
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_title_pose_and_clips_the_visible_glyph_box() {
        let transform = ClipTransform {
            pos_x: 480.0,
            pos_y: -270.0,
            scale: 1.5,
            ..ClipTransform::default()
        };
        let pose = title_pose(TitleStyle::ModernText, 24, transform);
        assert_eq!((pose.x, pose.y, pose.font_size), (0.75, 0.25, 36.0));
        assert_eq!(
            NormalizedTitleBox::from_pixels([-32.0, 180.0, 352.0, 600.0], [1280, 720]),
            Some(NormalizedTitleBox {
                x: 0.0,
                y: 0.25,
                width: 0.25,
                height: 0.75
            }),
        );
        assert_eq!(NormalizedTitleBox::from_pixels([0.0; 4], [1280, 720]), None);
    }
}

pub fn title_pose(style: TitleStyle, font_size: u32, transform: ClipTransform) -> TitlePose {
    let baseline_y = if style == TitleStyle::QuickCaption {
        0.85
    } else {
        0.5
    };
    TitlePose {
        x: (0.5 + f64::from(transform.pos_x) / 1920.0).clamp(0.0, 1.0),
        y: (baseline_y + f64::from(transform.pos_y) / 1080.0).clamp(0.0, 1.0),
        font_size: (f64::from(font_size) * f64::from(transform.scale)).max(1.0),
    }
}

/// Retângulo visível do texto em coordenadas normalizadas do quadro composto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedTitleBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedTitleBox {
    /// Converte as métricas em pixels do renderizador, limitando a caixa à imagem.
    pub fn from_pixels(rect: [f64; 4], frame: [u32; 2]) -> Option<Self> {
        let [x, y, width, height] = rect;
        let [fw, fh] = frame.map(f64::from);
        if fw == 0.0
            || fh == 0.0
            || width <= 0.0
            || height <= 0.0
            || !rect.into_iter().all(f64::is_finite)
        {
            return None;
        }
        let left = (x / fw).clamp(0.0, 1.0);
        let top = (y / fh).clamp(0.0, 1.0);
        let right = ((x + width) / fw).clamp(0.0, 1.0);
        let bottom = ((y + height) / fh).clamp(0.0, 1.0);
        (right > left && bottom > top).then_some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}
