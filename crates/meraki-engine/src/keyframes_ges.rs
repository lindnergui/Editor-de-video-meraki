use crate::protocol::TimelineRenderClip;
use glib::prelude::*;
use gstreamer as gst;
use gstreamer_controller as gst_ctrl;
use gstreamer_controller::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{
    ClipTransition, EffectParameters, Keyframe, KeyframeEasing, KeyframeProperty, TransitionKind,
    ZoomEasing, ZoomParams,
};
use std::collections::BTreeSet;
use tracing::debug;

mod clip_transitions;
use clip_transitions::supports_blur;
pub(crate) use clip_transitions::{
    attach_blur_transition, needs_blur, transitions, update_blur_transition,
};

/// Aplica curvas `InterpolationControlSource` para transições e keyframes do clipe.
pub(crate) fn apply_motion(
    track_elem: &ges::TrackElement,
    clip: &TimelineRenderClip,
    (base_w, base_h): (i32, i32),
) {
    let inpoint_ns = track_elem
        .upcast_ref::<ges::TimelineElement>()
        .inpoint()
        .nseconds();
    let (transition_in, transition_out) = transitions(clip);
    let tr_in_fade = transition_in.filter(|t| {
        t.kind == TransitionKind::Fade || (t.kind == TransitionKind::Blur && !supports_blur())
    });
    let tr_out_fade = transition_out.filter(|t| {
        t.kind == TransitionKind::Fade || (t.kind == TransitionKind::Blur && !supports_blur())
    });
    let mut opacity_kfs: Vec<Keyframe> = clip
        .keyframes
        .iter()
        .copied()
        .filter(|k| k.property == KeyframeProperty::Opacity)
        .collect();
    opacity_kfs.sort_by_key(|k| k.time_offset_ns);

    if tr_in_fade.is_some() || tr_out_fade.is_some() || !opacity_kfs.is_empty() {
        let (mode, points) = build_alpha_curve(
            clip.duration_ns,
            f64::from(clip.transform.opacity),
            tr_in_fade,
            tr_out_fade,
            &opacity_kfs,
        );
        bind_curve(track_elem, "alpha", mode, inpoint_ns, &points);
    }

    bind_axis_keyframes(
        track_elem,
        inpoint_ns,
        clip.duration_ns,
        &clip.keyframes,
        KeyframeProperty::PosX,
        "posx",
    );
    bind_axis_keyframes(
        track_elem,
        inpoint_ns,
        clip.duration_ns,
        &clip.keyframes,
        KeyframeProperty::PosY,
        "posy",
    );

    let mut scale_kfs: Vec<Keyframe> = clip
        .keyframes
        .iter()
        .copied()
        .filter(|k| k.property == KeyframeProperty::Scale)
        .collect();
    scale_kfs.sort_by_key(|k| k.time_offset_ns);
    let zoom_in = transition_in.filter(|t| t.kind == TransitionKind::Zoom);
    let zoom_out = transition_out.filter(|t| t.kind == TransitionKind::Zoom);
    let zoom = zoom_params(clip);
    if let Some(zoom) = zoom {
        apply_zoom(track_elem, clip, (base_w, base_h), inpoint_ns, zoom);
    }
    if !scale_kfs.is_empty() || (zoom.is_none() && (zoom_in.is_some() || zoom_out.is_some())) {
        apply_scale_or_zoom(
            track_elem, inpoint_ns, clip, base_w, base_h, &scale_kfs, zoom_in, zoom_out,
        );
    }
}

pub(crate) fn zoom_params(clip: &TimelineRenderClip) -> Option<ZoomParams> {
    clip.effects
        .iter()
        .rev()
        .filter(|effect| effect.enabled)
        .find_map(|effect| match effect.params {
            EffectParameters::Zoom(zoom) => Some(zoom),
            _ => None,
        })
}

fn apply_zoom(
    track: &ges::TrackElement,
    clip: &TimelineRenderClip,
    (width, height): (i32, i32),
    inpoint: u64,
    zoom: ZoomParams,
) {
    let (base_scale, x, y) = if clip.title.is_some() {
        (1.0, 0.0, 0.0)
    } else {
        (
            f64::from(clip.transform.scale),
            f64::from(clip.transform.pos_x),
            f64::from(clip.transform.pos_y),
        )
    };
    let sx = f64::from(width) * base_scale;
    let sy = f64::from(height) * base_scale;
    let points = |map: &dyn Fn(f64) -> f64| {
        (0..=16)
            .map(|i| {
                let fraction = f64::from(i) / 16.0;
                let t = match zoom.easing {
                    ZoomEasing::Linear => fraction,
                    ZoomEasing::EaseInOut => fraction * fraction * (3.0 - 2.0 * fraction),
                    ZoomEasing::Constant => 0.0,
                };
                let scale = zoom.start_scale + (zoom.end_scale - zoom.start_scale) * t;
                (clip.duration_ns.saturating_mul(i as u64) / 16, map(scale))
            })
            .collect::<Vec<_>>()
    };
    for (key, prop, size, position, focus) in [
        (KeyframeProperty::Scale, "width", sx, 0.0, 0.0),
        (KeyframeProperty::Scale, "height", sy, 0.0, 0.0),
        (KeyframeProperty::PosX, "posx", sx, x, zoom.focus_x),
        (KeyframeProperty::PosY, "posy", sy, y, zoom.focus_y),
    ] {
        if clip.keyframes.iter().any(|item| item.property == key) {
            continue;
        }
        let values = if key == KeyframeProperty::Scale {
            points(&|scale| (size * scale).max(1.0))
        } else {
            points(&|scale| position + (size - size * scale) * (focus + 1.0) * 0.5)
        };
        bind_curve(
            track,
            prop,
            gst_ctrl::InterpolationMode::Linear,
            inpoint,
            &values,
        );
    }
}

fn bind_axis_keyframes(
    track_elem: &ges::TrackElement,
    inpoint_ns: u64,
    duration_ns: u64,
    keyframes: &[Keyframe],
    prop: KeyframeProperty,
    gst_prop: &str,
) {
    let mut filtered: Vec<Keyframe> = keyframes
        .iter()
        .copied()
        .filter(|k| k.property == prop)
        .collect();
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
    let w_pts: Vec<(u64, f64)> = scale_pts
        .iter()
        .map(|&(t, s)| (t, (f64::from(base_w) * s).max(1.0)))
        .collect();
    let h_pts: Vec<(u64, f64)> = scale_pts
        .iter()
        .map(|&(t, s)| (t, (f64::from(base_h) * s).max(1.0)))
        .collect();
    bind_curve(track_elem, "width", mode, inpoint_ns, &w_pts);
    bind_curve(track_elem, "height", mode, inpoint_ns, &h_pts);
}

pub(crate) fn bind_curve(
    track_elem: &ges::TrackElement,
    prop: &str,
    mode: gst_ctrl::InterpolationMode,
    inpoint_ns: u64,
    points: &[(u64, f64)],
) {
    if points.is_empty() {
        return;
    }
    if track_elem.control_binding(prop).is_some() {
        let _ = track_elem.remove_control_binding(prop);
    }
    let src = gst_ctrl::InterpolationControlSource::new();
    src.set_mode(mode);
    for &(offset_ns, value) in points {
        let _ = src.set(
            gst::ClockTime::from_nseconds(inpoint_ns.saturating_add(offset_ns)),
            value,
        );
    }
    if !track_elem.set_control_source(&src, prop, "direct-absolute") {
        debug!(
            prop,
            "propriedade não suportou binding direct-absolute no elemento GES"
        );
    }
}

fn pick_interpolation_mode(keyframes: &[Keyframe], bounded: bool) -> gst_ctrl::InterpolationMode {
    if keyframes
        .iter()
        .any(|k| k.easing == KeyframeEasing::EaseInOut)
    {
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
            let in_factor = if in_dur > 0 && t < in_dur {
                (t as f64) / (in_dur as f64)
            } else {
                1.0
            };
            let out_factor = if out_dur > 0 && t > out_start {
                ((duration_ns - t) as f64) / (out_dur as f64)
            } else {
                1.0
            };
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
            let ratio =
                ((t - a.time_offset_ns) as f64) / ((b.time_offset_ns - a.time_offset_ns) as f64);
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
            &[Keyframe::new(
                1_000_000_000,
                KeyframeProperty::Opacity,
                0.5,
                KeyframeEasing::EaseInOut,
            )],
        );
        assert_eq!(mode, gst_ctrl::InterpolationMode::CubicMonotonic);
        assert_eq!(pts[0], (0, 0.0));
        assert_eq!(pts[pts.len() - 1], (2_000_000_000, 0.0));
        assert!(pts
            .iter()
            .any(|&(t, v)| t == 1_000_000_000 && (v - 0.5).abs() < 1e-6));
    }
}
