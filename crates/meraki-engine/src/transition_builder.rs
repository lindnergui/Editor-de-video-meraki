use crate::clip_prepare::{PipelineError, PreparedClip, PreparedSource};
use crate::keyframes_ges::bind_curve;
use crate::protocol::TimelineRenderClip;
use gstreamer_controller as ctrl;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{
    ClipId, TimelineTransition, TrackId, TransitionKind, TransitionPlacement, VideoEffectKind,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use tracing::warn;

#[derive(Clone, Copy)]
pub(crate) struct Ramp {
    kind: TransitionKind,
    start: u64,
    end: u64,
    from: f64,
    to: f64,
}

impl Ramp {
    fn strength(self, time: u64) -> f64 {
        if !(self.start..=self.end).contains(&time) {
            return 0.0;
        }
        let x = (time - self.start) as f64 / (self.end - self.start) as f64;
        let fraction = if self.kind == TransitionKind::Crossfade {
            x
        } else {
            x * x * (3.0 - 2.0 * x)
        };
        self.from + (self.to - self.from) * fraction
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Fragment {
    pub origin: usize,
    pub start: u64,
    pub duration: u64,
    before_seam: bool,
}

impl Fragment {
    pub(crate) fn render_clip(self, origin: &PreparedClip) -> TimelineRenderClip {
        let mut clip = fragment_style(&origin.render);
        clip.timeline_start_ns = self.start;
        clip.duration_ns = self.duration;
        if let PreparedSource::Media {
            asset,
            is_image: false,
            ..
        } = &origin.source
        {
            let max_inpoint = asset.duration().map_or(u64::MAX, |duration| {
                duration.nseconds().saturating_sub(self.duration)
            });
            let desired = if self.before_seam {
                clip.source_in_ns.saturating_sub(self.duration)
            } else {
                clip.source_in_ns.saturating_add(origin.render.duration_ns)
            };
            clip.source_in_ns = desired.min(max_inpoint);
        }
        clip
    }

    pub(crate) fn alpha(self) -> Ramp {
        let (from, to) = if self.before_seam {
            (1.0, 0.5)
        } else {
            (0.5, 1.0)
        };
        Ramp {
            kind: TransitionKind::Crossfade,
            start: 0,
            end: self.duration,
            from,
            to,
        }
    }
}

// As transições posicionais pertencem à timeline, portanto não são copiadas
// para a amostra auxiliar. Os efeitos de vídeo e a transformação continuam ativos.
pub(crate) fn fragment_style(clip: &TimelineRenderClip) -> TimelineRenderClip {
    let mut style = clip.clone();
    style.transition_in = None;
    style.transition_out = None;
    style
        .effects
        .retain(|effect| effect.kind != VideoEffectKind::Transition);
    style
}

#[derive(Default)]
pub(crate) struct TransitionPlan {
    pub curves: HashMap<ClipId, Vec<Ramp>>,
    pub fragments: HashMap<TrackId, Vec<Fragment>>,
}

impl TransitionPlan {
    pub(crate) fn new(prepared: &[PreparedClip], transitions: &[TimelineTransition]) -> Self {
        let clips: HashMap<_, _> = prepared
            .iter()
            .enumerate()
            .filter(|(_, item)| item.has_video())
            .map(|(index, clip)| (clip.render.clip_id.0, (index, &clip.render)))
            .collect();
        let mut plan = Self::default();
        let mut placements = HashSet::new();
        for transition in transitions.iter().rev() {
            if !placements.insert((transition.track_id, transition.placement)) {
                continue;
            }
            let find = |id| {
                clips
                    .get(&id)
                    .copied()
                    .filter(|(_, clip)| clip.track_id.0 == transition.track_id)
            };
            match transition.placement {
                TransitionPlacement::ClipStart { clip_id }
                | TransitionPlacement::ClipEnd { clip_id } => {
                    if let Some((_, clip)) = find(clip_id) {
                        let start =
                            matches!(transition.placement, TransitionPlacement::ClipStart { .. });
                        let span = transition.duration_ns.min(clip.duration_ns);
                        plan.edge(clip, transition.kind, span, start);
                    }
                }
                TransitionPlacement::BetweenClips {
                    left_clip_id,
                    right_clip_id,
                } => {
                    let (Some((left_index, left)), Some((right_index, right))) =
                        (find(left_clip_id), find(right_clip_id))
                    else {
                        continue;
                    };
                    let seam = right.timeline_start_ns;
                    if left.timeline_start_ns.checked_add(left.duration_ns) != Some(seam) {
                        warn!(transition_id = %transition.id, "transição entre clipes separados ignorada");
                        continue;
                    }
                    let span = transition
                        .duration_ns
                        .min(left.duration_ns)
                        .min(right.duration_ns);
                    let before = span / 2;
                    let after = span - before;
                    if transition.kind == TransitionKind::Crossfade {
                        let fragments = plan.fragments.entry(left.track_id).or_default();
                        if before > 0 {
                            fragments.push(Fragment {
                                origin: right_index,
                                start: seam - before,
                                duration: before,
                                before_seam: true,
                            });
                        }
                        if after > 0 {
                            fragments.push(Fragment {
                                origin: left_index,
                                start: seam,
                                duration: after,
                                before_seam: false,
                            });
                        }
                    } else {
                        plan.edge(left, transition.kind, before, false);
                        plan.edge(right, transition.kind, after, true);
                    }
                }
            }
        }
        plan
    }

    fn edge(&mut self, clip: &TimelineRenderClip, kind: TransitionKind, span: u64, leading: bool) {
        if span == 0 {
            return;
        }
        let (start, end, from, to) = if leading {
            (0, span, 1.0, 0.0)
        } else {
            (clip.duration_ns - span, clip.duration_ns, 0.0, 1.0)
        };
        self.curves.entry(clip.clip_id).or_default().push(Ramp {
            kind,
            start,
            end,
            from,
            to,
        });
    }
}

pub(crate) fn attach_curves(
    clip: &ges::Clip,
    clip_duration: u64,
    ramps: &[Ramp],
) -> Result<(), PipelineError> {
    for kind in [
        TransitionKind::Crossfade,
        TransitionKind::Zoom,
        TransitionKind::Blur,
    ] {
        let ramps: Vec<_> = ramps
            .iter()
            .copied()
            .filter(|ramp| ramp.kind == kind)
            .collect();
        if ramps.is_empty() {
            continue;
        }
        let (description, property, base, delta) = match kind {
            TransitionKind::Crossfade => ("video alpha", "alpha", 1.0, -1.0),
            TransitionKind::Zoom => (
                "video videoconvert ! merakivideoeffects ! videoconvert",
                "zoom",
                0.0,
                1.0,
            ),
            TransitionKind::Blur => (
                "video videoconvert ! gaussianblur ! videoconvert",
                "sigma",
                0.0,
                8.0,
            ),
        };
        let setup_error = |reason| PipelineError::TimelineSetup {
            step: "transição posicional",
            reason,
        };
        let effect = ges::Effect::new(description).map_err(|err| setup_error(err.to_string()))?;
        GESContainerExt::add(clip, &effect).map_err(|err| setup_error(err.to_string()))?;
        let track: &ges::TrackElement = effect.upcast_ref();
        if TrackElementExt::lookup_child(track, property).is_none() {
            return Err(setup_error(format!("controle {property} ausente")));
        }
        let points = points_for_curve(clip_duration, &ramps, base, delta);
        let inpoint = effect
            .upcast_ref::<ges::TimelineElement>()
            .inpoint()
            .nseconds();
        bind_curve(
            track,
            property,
            ctrl::InterpolationMode::Linear,
            inpoint,
            &points,
        );
    }
    Ok(())
}

fn points_for_curve(duration: u64, ramps: &[Ramp], base: f64, delta: f64) -> Vec<(u64, f64)> {
    let mut times = BTreeSet::from([0, duration]);
    for ramp in ramps {
        times.extend((0..=16).map(|i| ramp.start + (ramp.end - ramp.start).saturating_mul(i) / 16));
    }
    times
        .into_iter()
        .map(|time| {
            let strength = ramps
                .iter()
                .map(|ramp| ramp.strength(time))
                .fold(0.0_f64, f64::max);
            (time, base + delta * strength)
        })
        .collect()
}
