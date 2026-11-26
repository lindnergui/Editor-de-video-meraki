use super::bind_curve;
use crate::protocol::TimelineRenderClip;
use glib::prelude::*;
use gstreamer as gst;
use gstreamer_controller as gst_ctrl;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipTransition, EffectParameters, TransitionEffectKind, TransitionKind};

pub(crate) fn transitions(
    clip: &TimelineRenderClip,
) -> (Option<ClipTransition>, Option<ClipTransition>) {
    let typed = clip
        .effects
        .iter()
        .rev()
        .filter(|effect| effect.enabled)
        .find_map(|effect| match effect.params {
            EffectParameters::Transition(params) => Some(params),
            _ => None,
        });
    let Some(params) = typed else {
        return (clip.transition_in, clip.transition_out);
    };
    let kind = match params.kind {
        TransitionEffectKind::Fade => TransitionKind::Fade,
        TransitionEffectKind::ZoomTransition => TransitionKind::Zoom,
        TransitionEffectKind::BlurTransition => TransitionKind::Blur,
    };
    let transition =
        ClipTransition::new(kind, (params.duration_ms * 1_000_000).min(clip.duration_ns));
    (
        clip.transition_in.or(params.apply_in.then_some(transition)),
        clip.transition_out
            .or(params.apply_out.then_some(transition)),
    )
}

pub(super) fn supports_blur() -> bool {
    gst::ElementFactory::find("gaussianblur").is_some()
}

pub(crate) fn needs_blur(clip: &TimelineRenderClip) -> bool {
    let (start, end) = transitions(clip);
    [start, end]
        .into_iter()
        .flatten()
        .any(|tr| tr.kind == TransitionKind::Blur)
        && supports_blur()
}

pub(crate) fn attach_blur_transition(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
) -> Result<Option<ges::Effect>, String> {
    if !needs_blur(clip) {
        return Ok(None);
    }
    let effect = ges::Effect::new("video videoconvert ! gaussianblur sigma=0.0 ! videoconvert")
        .map_err(|err| err.to_string())?;
    GESContainerExt::add(ges_clip, &effect).map_err(|err| err.to_string())?;
    update_blur_transition(&effect, clip);
    Ok(Some(effect))
}

pub(crate) fn update_blur_transition(effect: &ges::Effect, clip: &TimelineRenderClip) {
    let (tr_in, tr_out) = transitions(clip);
    let blur_in = tr_in.filter(|t| t.kind == TransitionKind::Blur);
    let blur_out = tr_out.filter(|t| t.kind == TransitionKind::Blur);
    let duration_ns = clip.duration_ns;
    let eff_elem: &ges::TrackElement = effect.upcast_ref();
    let eff_inpoint = effect
        .upcast_ref::<ges::TimelineElement>()
        .inpoint()
        .nseconds();
    let mut pts = Vec::new();
    if let Some(bin) = blur_in {
        pts.push((0, 8.0));
        pts.push((bin.duration_ns.min(duration_ns), 0.0));
    } else {
        pts.push((0, 0.0));
    }
    if let Some(bout) = blur_out {
        let out_start = duration_ns.saturating_sub(bout.duration_ns);
        if pts.last().is_none_or(|&(t, _)| t < out_start) {
            pts.push((out_start, 0.0));
        }
        pts.push((duration_ns, 8.0));
    } else if pts.last().is_none_or(|&(t, _)| t < duration_ns) {
        pts.push((duration_ns, 0.0));
    }
    bind_curve(
        eff_elem,
        "sigma",
        gst_ctrl::InterpolationMode::CubicMonotonic,
        eff_inpoint,
        &pts,
    );
}
