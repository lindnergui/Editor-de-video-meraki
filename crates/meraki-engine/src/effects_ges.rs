pub(crate) use crate::keyframes_ges::apply_transitions_and_keyframes;
use crate::keyframes_ges::resolve_base_dimensions;
use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip};
use glib::prelude::*;
use gstreamer as gst;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{
    KeyframeProperty, PreviewQuality, TitleOverlay, TitleStyle, TrackKind, TransitionKind,
    VideoEffectKind,
};
use tracing::warn;

/// Converte `[R, G, B, A]` para inteiro `0xAARRGGBB` esperado pelo `GESTitleSource`.
pub(crate) fn rgba_to_argb_u32(rgba: [u8; 4]) -> u32 {
    u32::from_be_bytes([rgba[3], rgba[0], rgba[1], rgba[2]])
}

/// Cria e configura um `ges::TitleClip` na camada `Overlay` com fallback seguro caso Pango falte.
pub(crate) fn add_title_clip(
    layer: &ges::Layer,
    clip: &TimelineRenderClip,
    title: &TitleOverlay,
    worker: &EngineWorkerChannel,
) -> Option<ges::Clip> {
    if gst::ElementFactory::find("textoverlay").is_none() {
        worker.emit_critical(EngineEvent::Error(
            "Plugin Pango (textoverlay) não encontrado; título ignorado sem interromper a timeline."
                .to_string(),
        ));
        return None;
    }

    let Some(title_clip) = ges::TitleClip::new() else {
        worker.emit_critical(EngineEvent::Error(
            "Falha ao instanciar ges::TitleClip para título na trilha T1.".to_string(),
        ));
        return None;
    };

    let tl_elem: &ges::TimelineElement = title_clip.upcast_ref();
    let _ = tl_elem.set_start(gst::ClockTime::from_nseconds(clip.timeline_start_ns));
    let _ = tl_elem.set_duration(gst::ClockTime::from_nseconds(clip.duration_ns));

    if let Err(err) = layer.add_clip(&title_clip) {
        worker.emit_critical(EngineEvent::Error(format!(
            "Falha ao adicionar TitleClip na camada GES: {err}"
        )));
        return None;
    }

    let ges_clip: ges::Clip = title_clip.upcast();
    if let Some(track_elem) =
        ges_clip.find_track_element(None::<&ges::Track>, ges::TitleSource::static_type())
    {
        if let Ok(src) = track_elem.downcast::<ges::TitleSource>() {
            configure_title_source(&src, title, clip);
        }
    }
    Some(ges_clip)
}

fn configure_title_source(src: &ges::TitleSource, title: &TitleOverlay, clip: &TimelineRenderClip) {
    src.set_text(Some(title.text.as_str()));
    let family = title.font_family.trim();
    let font_desc = if title.style == TitleStyle::Title3D
        && !family.to_ascii_lowercase().contains("bold")
    {
        format!("{family} Bold {}", title.font_size)
    } else {
        format!("{family} {}", title.font_size)
    };
    src.set_font_desc(Some(&font_desc));
    src.set_text_color(rgba_to_argb_u32(title.color_rgba));
    src.set_background_color(0x0000_0000);

    let (base_x, base_y) = match title.style {
        TitleStyle::QuickCaption => (0.5_f64, 0.85_f64),
        TitleStyle::ModernText | TitleStyle::Title3D => (0.5_f64, 0.5_f64),
    };
    let has_pos_kf = clip
        .keyframes
        .iter()
        .any(|k| matches!(k.property, KeyframeProperty::PosX | KeyframeProperty::PosY));
    let (xpos, ypos) = if has_pos_kf {
        (base_x, base_y)
    } else {
        (
            (base_x + f64::from(clip.transform.pos_x) / 1920.0).clamp(0.05, 0.95),
            (base_y + f64::from(clip.transform.pos_y) / 1080.0).clamp(0.05, 0.95),
        )
    };
    src.set_halignment(ges::TextHAlign::Position);
    src.set_valignment(ges::TextVAlign::Position);
    src.set_xpos(xpos);
    src.set_ypos(ypos);
}

/// Aplica transformações estáticas base (`alpha`, `posx`, `posy`, `width`, `height`) em vídeo.
pub(crate) fn apply_clip_transform(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
    is_title: bool,
    natural_size: Option<(i32, i32)>,
    preview_quality: PreviewQuality,
) {
    let elem: &ges::TimelineElement = ges_clip.upcast_ref();
    let _ = elem.set_child_property("alpha", &f64::from(clip.transform.opacity).to_value());

    let has_pos_kf = clip
        .keyframes
        .iter()
        .any(|k| matches!(k.property, KeyframeProperty::PosX | KeyframeProperty::PosY));
    if !is_title || has_pos_kf {
        let _ = elem.set_child_property("posx", &(clip.transform.pos_x.round() as i32).to_value());
        let _ = elem.set_child_property("posy", &(clip.transform.pos_y.round() as i32).to_value());
    }

    let has_scale_kf = clip.keyframes.iter().any(|k| k.property == KeyframeProperty::Scale);
    let has_zoom_tr = matches!(clip.transition_in.map(|t| t.kind), Some(TransitionKind::Zoom))
        || matches!(clip.transition_out.map(|t| t.kind), Some(TransitionKind::Zoom));
    if !has_scale_kf && !has_zoom_tr && (clip.transform.scale - 1.0).abs() > f32::EPSILON {
        let (base_w, base_h) = resolve_base_dimensions(elem, natural_size, preview_quality);
        let scaled_w = ((base_w as f32) * clip.transform.scale).round().max(1.0) as i32;
        let scaled_h = ((base_h as f32) * clip.transform.scale).round().max(1.0) as i32;
        let _ = elem.set_child_property("width", &scaled_w.to_value());
        let _ = elem.set_child_property("height", &scaled_h.to_value());
    }
}

/// Anexa `ges::Effect` validando disponibilidade de cada fábrica GStreamer antes de criar.
pub(crate) fn apply_clip_effects(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
    worker: &EngineWorkerChannel,
) {
    let is_overlay = clip.track_kind == TrackKind::Overlay;
    for effect in clip.effects.iter().filter(|e| e.enabled) {
        let Some(desc) = build_effect_description(effect.kind, effect.intensity, is_overlay) else {
            worker.emit_critical(EngineEvent::Error(format!(
                "Elemento GStreamer indisponível para o efeito {:?}; ignorando efeito.",
                effect.kind
            )));
            continue;
        };
        match ges::Effect::new(&desc) {
            Ok(ges_effect) => {
                if let Err(err) = GESContainerExt::add(ges_clip, &ges_effect) {
                    warn!(error = %err, desc, "falha ao anexar efeito no clipe GES");
                    worker.emit_critical(EngineEvent::Error(format!(
                        "Falha ao anexar efeito {:?}: {err}",
                        effect.kind
                    )));
                }
            }
            Err(err) => {
                worker.emit_critical(EngineEvent::Error(format!(
                    "Falha ao construir efeito {:?}: {err}",
                    effect.kind
                )));
            }
        }
    }
}

fn build_effect_description(kind: VideoEffectKind, intensity: f32, is_overlay: bool) -> Option<String> {
    let amt = f64::from(intensity.clamp(0.0, 1.0));
    let has_vb = gst::ElementFactory::find("videobalance").is_some();
    match kind {
        VideoEffectKind::Cinematic if has_vb => Some(format!(
            "videobalance contrast={:.3} saturation={:.3} brightness={:.3}",
            1.0 + 0.25 * amt,
            1.0 - 0.18 * amt,
            -0.06 * amt
        )),
        VideoEffectKind::FilmStyle if has_vb => Some(format!(
            "videobalance contrast={:.3} saturation={:.3} hue={:.3} brightness={:.3}",
            1.0 + 0.15 * amt,
            1.0 - 0.30 * amt,
            -0.08 * amt,
            0.03 * amt
        )),
        VideoEffectKind::Glitch => {
            if !is_overlay
                && gst::ElementFactory::find("edgetv").is_some()
                && gst::ElementFactory::find("videoconvert").is_some()
            {
                Some("videoconvert ! edgetv ! videoconvert".to_string())
            } else if has_vb {
                Some(format!(
                    "videobalance contrast={:.3} hue={:.3} saturation={:.3}",
                    1.0 + 0.45 * amt,
                    0.35 * amt,
                    1.0 + 0.50 * amt
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_converts_to_argb_for_title_source() {
        assert_eq!(rgba_to_argb_u32([0x73, 0x5B, 0xF2, 0xFF]), 0xFF73_5BF2);
        assert_eq!(rgba_to_argb_u32([0x11, 0x22, 0x33, 0x80]), 0x8011_2233);
    }
}
