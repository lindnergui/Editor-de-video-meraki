use iced::window;

/// Dimensão oficial em pixels (64x64) do ícone nativo da janela.
pub const ICON_SIZE: u32 = 64;

const SAMPLES_PER_AXIS: usize = 4;

fn smooth_coverage(sdf: f32) -> f32 {
    (0.5 - sdf).clamp(0.0, 1.0)
}

fn rounded_box_sdf(px: f32, py: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = px.abs() - half_w + radius;
    let qy = py.abs() - half_h + radius;
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    let inside = qx.max(qy).min(0.0);
    outside + inside - radius
}

fn point_in_triangle(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let d1 = sign((px, py), a, b);
    let d2 = sign((px, py), b, c);
    let d3 = sign((px, py), c, a);
    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
    !(has_neg && has_pos)
}

fn blend_rgba(dst: &mut [f32; 4], src: [f32; 4]) {
    let a = src[3].clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 1e-6 {
        *dst = [0.0; 4];
        return;
    }
    for i in 0..3 {
        dst[i] = (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn sample_icon_pixel(x: f32, y: f32) -> [f32; 4] {
    let cx = x - 32.0;
    let cy = y - 32.0;

    // Fundo arredondado roxo Meraki (#735BF2 -> #624AE0) com borda sutil (#8670F5).
    let badge_sdf = rounded_box_sdf(cx, cy, 28.0, 28.0, 14.0);
    let badge_alpha = smooth_coverage(badge_sdf);
    if badge_alpha <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let t = (y / 64.0).clamp(0.0, 1.0);
    let bg_r = (0x73 as f32 * (1.0 - t) + 0x62 as f32 * t) / 255.0;
    let bg_g = (0x5B as f32 * (1.0 - t) + 0x4A as f32 * t) / 255.0;
    let bg_b = (0xF2 as f32 * (1.0 - t) + 0xE0 as f32 * t) / 255.0;
    let mut pixel = [bg_r, bg_g, bg_b, badge_alpha];

    // Anel sutil interno para contraste em barras de tarefas claras e escuras.
    let rim_alpha = smooth_coverage((badge_sdf + 1.0).abs() - 0.8) * badge_alpha * 0.45;
    blend_rgba(&mut pixel, [0x86 as f32 / 255.0, 0x70 as f32 / 255.0, 0xF5 as f32 / 255.0, rim_alpha]);

    // Moldura branca estilizada estilo `MERAKI_MARK`.
    let frame_sdf = rounded_box_sdf(cx, cy, 18.0, 14.5, 5.5);
    let stroke_alpha = smooth_coverage(frame_sdf.abs() - 1.9);
    blend_rgba(&mut pixel, [1.0, 1.0, 1.0, stroke_alpha]);

    // Triângulo de play central branco.
    if point_in_triangle(x, y, (27.5, 24.5), (41.0, 32.0), (27.5, 39.5)) {
        blend_rgba(&mut pixel, [1.0, 1.0, 1.0, 1.0]);
    }

    pixel
}

/// Gera buffer RGBA8 64x64 com supersampling 4x4 para o ícone da janela.
pub fn generate_rgba() -> Vec<u8> {
    let size = ICON_SIZE as usize;
    let mut rgba = Vec::with_capacity(size * size * 4);
    let inv_samples = 1.0 / (SAMPLES_PER_AXIS * SAMPLES_PER_AXIS) as f32;

    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0_f32; 4];
            for sy in 0..SAMPLES_PER_AXIS {
                for sx in 0..SAMPLES_PER_AXIS {
                    let fx = px as f32 + (sx as f32 + 0.5) / SAMPLES_PER_AXIS as f32;
                    let fy = py as f32 + (sy as f32 + 0.5) / SAMPLES_PER_AXIS as f32;
                    let s = sample_icon_pixel(fx, fy);
                    acc[0] += s[0] * s[3];
                    acc[1] += s[1] * s[3];
                    acc[2] += s[2] * s[3];
                    acc[3] += s[3];
                }
            }
            let alpha = (acc[3] * inv_samples).clamp(0.0, 1.0);
            let (r, g, b) = if alpha > 1e-5 {
                let norm = 1.0 / (alpha * SAMPLES_PER_AXIS as f32 * SAMPLES_PER_AXIS as f32);
                ((acc[0] * norm).clamp(0.0, 1.0), (acc[1] * norm).clamp(0.0, 1.0), (acc[2] * norm).clamp(0.0, 1.0))
            } else {
                (0.0, 0.0, 0.0)
            };
            rgba.push((r * 255.0).round() as u8);
            rgba.push((g * 255.0).round() as u8);
            rgba.push((b * 255.0).round() as u8);
            rgba.push((alpha * 255.0).round() as u8);
        }
    }
    rgba
}

/// Constrói o `iced::window::Icon` nativo do Meraki (sem `.unwrap()`).
pub fn app_icon() -> Option<window::Icon> {
    window::icon::from_rgba(generate_rgba(), ICON_SIZE, ICON_SIZE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_64x64_rgba_meraki_icon() {
        let rgba = generate_rgba();
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);

        // Canto superior esquerdo deve permanecer transparente.
        assert_eq!(rgba[3], 0);

        // Área roxa interna (#735BF2) ao redor de (16, 16) deve ser opaca e violeta.
        let purple_idx = (16 * ICON_SIZE as usize + 16) * 4;
        assert!(rgba[purple_idx + 2] > rgba[purple_idx + 1]);
        assert_eq!(rgba[purple_idx + 3], 255);

        // Centro do triângulo de play em (32, 32) deve ser branco opaco.
        let center_idx = (32 * ICON_SIZE as usize + 32) * 4;
        assert_eq!(&rgba[center_idx..center_idx + 4], &[255, 255, 255, 255]);

        assert!(app_icon().is_some());
    }
}
