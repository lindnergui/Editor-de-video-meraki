use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip};
use gstreamer as gst;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{title_pose, KeyframeProperty, TitleOverlay, TitleStyle};

pub(crate) fn rgba_to_argb_u32(rgba: [u8; 4]) -> u32 {
    u32::from_be_bytes([rgba[3], rgba[0], rgba[1], rgba[2]])
}

/// TitleClip já cria seu próprio TitleSource (e um único textoverlay).
pub(crate) fn add_title_clip(
    layer: &ges::Layer,
    clip: &TimelineRenderClip,
    worker: &EngineWorkerChannel,
) -> Option<ges::Clip> {
    let create = || -> Result<ges::Clip, String> {
        if gst::ElementFactory::find("textoverlay").is_none() {
            return Err("plugin Pango (textoverlay) indisponível".into());
        }
        let title = ges::TitleClip::new().ok_or("falha ao instanciar TitleClip GES")?;
        let elem: &ges::TimelineElement = title.upcast_ref();
        let _ = elem.set_start(gst::ClockTime::from_nseconds(clip.timeline_start_ns));
        let _ = elem.set_duration(gst::ClockTime::from_nseconds(clip.duration_ns));
        layer.add_clip(&title).map_err(|err| err.to_string())?;
        Ok(title.upcast())
    };
    match create() {
        Ok(title) => Some(title),
        Err(reason) => {
            worker.emit_critical(EngineEvent::Error(format!(
                "falha ao inserir título GES: {reason}"
            )));
            None
        }
    }
}

pub(crate) fn configure_title_source(
    src: &ges::TitleSource,
    title: &TitleOverlay,
    clip: &TimelineRenderClip,
) {
    let mut transform = clip.transform;
    for keyframe in &clip.keyframes {
        match keyframe.property {
            KeyframeProperty::PosX => transform.pos_x = 0.0,
            KeyframeProperty::PosY => transform.pos_y = 0.0,
            _ => {}
        }
    }
    let pose = title_pose(title.style, title.font_size, transform);
    let family = title.font_family.trim();
    let bold =
        if title.style == TitleStyle::Title3D && !family.to_ascii_lowercase().contains("bold") {
            " Bold"
        } else {
            ""
        };
    src.set_text(Some(&title.text));
    src.set_font_desc(Some(&format!("{family}{bold} {:.2}", pose.font_size)));
    src.set_text_color(rgba_to_argb_u32(title.color_rgba));
    src.set_background_color(0);
    src.set_halignment(ges::TextHAlign::Position);
    src.set_valignment(ges::TextVAlign::Position);
    src.set_xpos(pose.x);
    src.set_ypos(pose.y);
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
