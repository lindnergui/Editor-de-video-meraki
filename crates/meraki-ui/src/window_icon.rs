use iced::widget::{image, image::Handle};
use iced::{window, ContentFit, Element, Length};

/// Ícone oficial da marca Meraki em PNG RGBA 256x256.
pub const BRAND_ICON_PNG: &[u8] =
    include_bytes!("../../../assets/branding/meraki-icon-256.png");

/// Dimensão em pixels usada no fallback programático caso a decodificação PNG falhe.
pub const FALLBACK_ICON_SIZE: u32 = 64;

fn fallback_rgba_64() -> Vec<u8> {
    let size = FALLBACK_ICON_SIZE as usize;
    let mut rgba = Vec::with_capacity(size * size * 4);
    let half = 28.0_f32;
    let radius = 14.0_f32;

    for py in 0..size {
        for px in 0..size {
            let cx = (px as f32 + 0.5) - 32.0;
            let cy = (py as f32 + 0.5) - 32.0;
            let qx = cx.abs() - half + radius;
            let qy = cy.abs() - half + radius;
            let sdf = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let alpha = (0.5 - sdf).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.extend_from_slice(&[0x73, 0x5B, 0xF2, (alpha * 255.0).round() as u8]);
            }
        }
    }
    rgba
}

/// Constrói o `iced::window::Icon` nativo do Meraki a partir do PNG embutido (com fallback seguro).
pub fn app_icon() -> Option<window::Icon> {
    window::icon::from_file_data(BRAND_ICON_PNG, None)
        .ok()
        .or_else(|| {
            window::icon::from_rgba(fallback_rgba_64(), FALLBACK_ICON_SIZE, FALLBACK_ICON_SIZE).ok()
        })
}

/// Renderiza o logotipo oficial do Meraki na interface no tamanho informado.
pub fn brand_icon<'a, M: 'a>(size: f32) -> Element<'a, M> {
    image(Handle::from_bytes(BRAND_ICON_PNG))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .content_fit(ContentFit::Contain)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_png_has_valid_header_and_builds_window_icon() {
        assert!(BRAND_ICON_PNG.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(app_icon().is_some());
    }

    #[test]
    fn fallback_rgba_has_expected_size_and_opaque_center() {
        let rgba = fallback_rgba_64();
        let size = FALLBACK_ICON_SIZE as usize;
        assert_eq!(rgba.len(), size * size * 4);
        assert_eq!(rgba[3], 0);
        let center_idx = (32 * size + 32) * 4;
        assert_eq!(&rgba[center_idx..center_idx + 4], &[0x73, 0x5B, 0xF2, 255]);
    }
}
