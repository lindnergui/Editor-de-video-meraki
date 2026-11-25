use crate::protocol::TimelineRenderClip;
use glib::prelude::*;
use gstreamer as gst;
use gstreamer_controller as gst_ctrl;
use gstreamer_controller::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{
    ClipTransition, Keyframe, KeyframeEasing, KeyframeProperty, PreviewQuality, TransitionKind,
};
use std::collections::BTreeSet;
use tracing::debug;

/// Aplica curvas `InterpolationControlSource` para transições e keyframes do clipe.
pub(crate) fn apply_transitions_and_keyframes(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
    natural_size: Option<(i32, i32)>,
    preview_quality: PreviewQuality,
) {
    let elems = ges_clip.find_track_elements(
        None::<&ges::Track>,
        ges::TrackType::VIDEO,
        ges::VideoSource::static_type(),
    );
    let Some(track_elem) = elems.into_iter().next() else {
        return;
    };

    let inpoint_ns = track_elem.upcast_ref::<ges::TimelineElement>().inpoint().nseconds();
    let blur_ready = gst::ElementFactory::find("gaussianblur").is_some()
        && gst::ElementFactory::find("videoconvert").is_some();

    let tr_in_fade = clip.transition_in.filter(|t| {
        t.kind == TransitionKind::Fade || (t.kind == TransitionKind::Blur && !blur_ready)
    });
    let tr_out_fade = clip.transition_out.filter(|t| {
        t.kind == TransitionKind::Fade || (t.kind == TransitionKind::Blur && !blur_ready)
    });
    let mut opacity_kfs: Vec<Keyframe> =
        clip.keyframes.iter().copied().filter(|k| k.property == KeyframeProperty::Opacity).collect();
    opacity_kfs.sort_by_key(|k| k.time_offset_ns);

    if tr_in_fade.is_some() || tr_out_fade.is_some() || !opacity_kfs.is_empty() {
        let (mode, points) = build_alpha_curve(
            clip.duration_ns,
            f64::from(clip.transform.opacity),
            tr_in_fade,
            tr_out_fade,
            &opacity_kfs,
        );
        bind_curve(&track_elem, "alpha", mode, inpoint_ns, &points);
    }

    bind_axis_keyframes(&track_elem, inpoint_ns, clip.duration_ns, &clip.keyframes, KeyframeProperty::PosX, "posx");
    bind_axis_keyframes(&track_elem, inpoint_ns, clip.duration_ns, &clip.keyframes, KeyframeProperty::PosY, "posy");

    let mut scale_kfs: Vec<Keyframe> =
        clip.keyframes.iter().copied().filter(|k| k.property == KeyframeProperty::Scale).collect();
    scale_kfs.sort_by_key(|k| k.time_offset_ns);
    let zoom_in = clip.transition_in.filter(|t| t.kind == TransitionKind::Zoom);
    let zoom_out = clip.transition_out.filter(|t| t.kind == TransitionKind::Zoom);

    if !scale_kfs.is_empty() || zoom_in.is_some() || zoom_out.is_some() {
        let (base_w, base_h) =
            resolve_base_dimensions(ges_clip.upcast_ref(), natural_size, preview_quality);
        apply_scale_or_zoom(&track_elem, inpoint_ns, clip, base_w, base_h, &scale_kfs, zoom_in, zoom_out);
    }

    if blur_ready {
        apply_blur_transition(ges_clip, clip.duration_ns, clip.transition_in, clip.transition_out);
    }
}

pub(crate) fn resolve_base_dimensions(
    elem: &ges::TimelineElement,
    natural_size: Option<(i32, i32)>,
    preview_quality: PreviewQuality,
) -> (i32, i32) {
    let child_w = elem.child_property("width").and_then(|v| v.get::<i32>().ok()).filter(|&w| w > 0);
    let child_h = elem.child_property("height").and_then(|v| v.get::<i32>().ok()).filter(|&h| h > 0);
    if let (Some(w), Some(h)) = (child_w, child_h) {
        return (w, h);
    }
    if let Some((nw, nh)) = natural_size {
        if nw > 0 && nh > 0 {
            return (nw, nh);
        }
    }
    let (pw, ph) = preview_quality.max_dimensions();
    (pw as i32, ph as i32)
}

fn bind_axis_keyframes(
    track_elem: &ges::TrackElement,
    inpoint_ns: u64,
    duration_ns: u64,
    keyframes: &[Keyframe],
    prop: KeyframeProperty,
    gst_prop: &str,
) {
    let mut filtered: Vec<Keyframe> = keyframes.iter().copied().filter(|k| k.property == prop).collect();
    if filtered.is_empty() {
        return;
    }
    filtered.sort_by_key(|k| k.time_offset_ns);
    let mode = pick_interpolation_mode(&filtered, false);
    let mut points: Vec<(u64, f64)> = Vec::with_capacity(filtered.len() + 2);
    if filtered[0].time_offset_ns > 0 {
        points.push((0, f64::from(filtered[0].value)));
    }
    for kf in &filtered {
        points.push((kf.time_offset_ns.min(duration_ns), f64::from(kf.value)));
    }
    if let Some(last) = points.last().copied() {
        if last.0 < duration_ns {
            points.push((duration_ns, last.1));
        }
    }
    bind_curve(track_elem, gst_prop, mode, inpoint_ns, &points);
}

#[allow(clippy::too_many_arguments)]
fn apply_scale_or_zoom(
    track_elem: &ges::TrackElement,
    inpoint_ns: u64,
    clip: &TimelineRenderClip,
    base_w: i32,
    base_h: i32,
    scale_kfs: &[Keyframe],
    zoom_in: Option<ClipTransition>,
    zoom_out: Option<ClipTransition>,
) {
    let (mode, scale_pts) = if !scale_kfs.is_empty() {
        let mut pts: Vec<(u64, f64)> = Vec::with_capacity(scale_kfs.len() + 2);
        if scale_kfs[0].time_offset_ns > 0 {
            pts.push((0, f64::from(scale_kfs[0].value)));
        }
        for kf in scale_kfs {
            pts.push((kf.time_offset_ns.min(clip.duration_ns), f64::from(kf.value)));
        }
        if let Some(last) = pts.last().copied() {
            if last.0 < clip.duration_ns {
                pts.push((clip.duration_ns, last.1));
            }
        }
        (pick_interpolation_mode(scale_kfs, true), pts)
    } else {
        let mut pts = Vec::new();
        let scale_base = f64::from(clip.transform.scale);
        if let Some(zin) = zoom_in {
            pts.push((0, scale_base * 0.85));
            pts.push((zin.duration_ns.min(clip.duration_ns), scale_base));
        } else {
            pts.push((0, scale_base));
        }
        if let Some(zout) = zoom_out {
            let out_start = clip.duration_ns.saturating_sub(zout.duration_ns);
            if pts.last().is_none_or(|&(t, _)| t < out_start) {
                pts.push((out_start, scale_base));
            }
            pts.push((clip.duration_ns, scale_base * 1.15));
        } else if pts.last().is_none_or(|&(t, _)| t < clip.duration_ns) {
            pts.push((clip.duration_ns, scale_base));
        }
        (gst_ctrl::InterpolationMode::CubicMonotonic, pts)
    };
    let w_pts: Vec<(u64, f64)> = scale_pts.iter().map(|&(t, s)| (t, (f64::from(base_w) * s).max(1.0))).collect();
    let h_pts: Vec<(u64, f64)> = scale_pts.iter().map(|&(t, s)| (t, (f64::from(base_h) * s).max(1.0))).collect();
    bind_curve(track_elem, "width", mode, inpoint_ns, &w_pts);
    bind_curve(track_elem, "height", mode, inpoint_ns, &h_pts);
}

fn apply_blur_transition(
    ges_clip: &ges::Clip,
    duration_ns: u64,
    tr_in: Option<ClipTransition>,
    tr_out: Option<ClipTransition>,
) {
    let blur_in = tr_in.filter(|t| t.kind == TransitionKind::Blur);
    let blur_out = tr_out.filter(|t| t.kind == TransitionKind::Blur);
    if blur_in.is_none() && blur_out.is_none() {
        return;
    }
    let Ok(effect) = ges::Effect::new("videoconvert ! gaussianblur sigma=0.0 ! videoconvert") else {
        return;
    };
    if GESContainerExt::add(ges_clip, &effect).is_err() {
        return;
    }
    let eff_elem: &ges::TrackElement = effect.upcast_ref();
    if ges::prelude::TrackElementExt::lookup_child(eff_elem, "sigma").is_none() {
        return;
    }
    let eff_inpoint = effect.upcast_ref::<ges::TimelineElement>().inpoint().nseconds();
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
    bind_curve(eff_elem, "sigma", gst_ctrl::InterpolationMode::CubicMonotonic, eff_inpoint, &pts);
}

fn bind_curve(
    track_elem: &ges::TrackElement,
    prop: &str,
    mode: gst_ctrl::InterpolationMode,
    inpoint_ns: u64,
    points: &[(u64, f64)],
) {
    if points.is_empty() {
        return;
    }
    let src = gst_ctrl::InterpolationControlSource::new();
    src.set_mode(mode);
    for &(offset_ns, value) in points {
        let _ = src.set(gst::ClockTime::from_nseconds(inpoint_ns.saturating_add(offset_ns)), value);
    }
    if !track_elem.set_control_source(&src, prop, "direct-absolute") {
        debug!(prop, "propriedade não suportou binding direct-absolute no elemento GES");
    }
}

fn pick_interpolation_mode(keyframes: &[Keyframe], bounded: bool) -> gst_ctrl::InterpolationMode {
    if keyframes.iter().any(|k| k.easing == KeyframeEasing::EaseInOut) {
        if bounded {
            gst_ctrl::InterpolationMode::CubicMonotonic
        } else {
            gst_ctrl::InterpolationMode::Cubic
        }
    } else {
        gst_ctrl::InterpolationMode::Linear
    }
}

/// Calcula a curva unificada de `alpha` combinando opacidade base, keyframes e fade In/Out.
pub(crate) fn build_alpha_curve(
    duration_ns: u64,
    base_opacity: f64,
    fade_in: Option<ClipTransition>,
    fade_out: Option<ClipTransition>,
    opacity_kfs: &[Keyframe],
) -> (gst_ctrl::InterpolationMode, Vec<(u64, f64)>) {
    let mut times = BTreeSet::new();
    times.insert(0);
    times.insert(duration_ns);
    if let Some(tr) = fade_in {
        times.insert(tr.duration_ns.min(duration_ns));
    }
    if let Some(tr) = fade_out {
        times.insert(duration_ns.saturating_sub(tr.duration_ns));
    }
    for kf in opacity_kfs {
        times.insert(kf.time_offset_ns.min(duration_ns));
    }

    let in_dur = fade_in.map_or(0, |t| t.duration_ns.min(duration_ns));
    let out_dur = fade_out.map_or(0, |t| t.duration_ns.min(duration_ns));
    let out_start = duration_ns.saturating_sub(out_dur);

    let points = times
        .into_iter()
        .map(|t| {
            let kf_alpha = sample_opacity_at(t, base_opacity, opacity_kfs);
            let in_factor = if in_dur > 0 && t < in_dur { (t as f64) / (in_dur as f64) } else { 1.0 };
            let out_factor =
                if out_dur > 0 && t > out_start { ((duration_ns - t) as f64) / (out_dur as f64) } else { 1.0 };
            (t, (kf_alpha * in_factor.min(out_factor)).clamp(0.0, 1.0))
        })
        .collect();

    let mode = if !opacity_kfs.is_empty() {
        pick_interpolation_mode(opacity_kfs, true)
    } else {
        gst_ctrl::InterpolationMode::CubicMonotonic
    };
    (mode, points)
}

fn sample_opacity_at(t: u64, base_opacity: f64, kfs: &[Keyframe]) -> f64 {
    if kfs.is_empty() {
        return base_opacity.clamp(0.0, 1.0);
    }
    if t <= kfs[0].time_offset_ns {
        return f64::from(kfs[0].value);
    }
    for pair in kfs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.time_offset_ns && b.time_offset_ns > a.time_offset_ns {
            let ratio = ((t - a.time_offset_ns) as f64) / ((b.time_offset_ns - a.time_offset_ns) as f64);
            return f64::from(a.value) + (f64::from(b.value) - f64::from(a.value)) * ratio;
        }
    }
    f64::from(kfs[kfs.len() - 1].value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meraki_core::TransitionKind;

    #[test]
    fn alpha_curve_combines_fade_in_out_and_keyframes() {
        let (mode, pts) = build_alpha_curve(
            2_000_000_000,
            1.0,
            Some(ClipTransition::new(TransitionKind::Fade, 500_000_000)),
            Some(ClipTransition::new(TransitionKind::Fade, 500_000_000)),
            &[Keyframe::new(1_000_000_000, KeyframeProperty::Opacity, 0.5, KeyframeEasing::EaseInOut)],
        );
        assert_eq!(mode, gst_ctrl::InterpolationMode::CubicMonotonic);
        assert_eq!(pts[0], (0, 0.0));
        assert_eq!(pts[pts.len() - 1], (2_000_000_000, 0.0));
        assert!(pts.iter().any(|&(t, v)| t == 1_000_000_000 && (v - 0.5).abs() < 1e-6));
    }
}
