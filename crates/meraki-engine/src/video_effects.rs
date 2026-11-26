use crate::protocol::{EngineEvent, EngineWorkerChannel, TimelineRenderClip};
use glib::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{ClipEffect, EffectParameters, VideoEffectKind};
use std::collections::HashMap;
use uuid::Uuid;

pub(crate) type ActiveEffects = HashMap<Uuid, ges::Effect>;

pub(crate) fn has_element(kind: VideoEffectKind) -> bool {
    !matches!(kind, VideoEffectKind::Zoom | VideoEffectKind::Transition)
}

pub(crate) fn attach_effects(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
    worker: &EngineWorkerChannel,
) -> ActiveEffects {
    let mut active = HashMap::new();
    for effect in clip
        .effects
        .iter()
        .filter(|effect| effect.enabled && has_element(effect.kind))
    {
        match attach_effect(ges_clip, effect) {
            Ok(element) => {
                active.insert(effect.id, element);
            }
            Err(reason) => worker.emit_critical(EngineEvent::Error(format!(
                "falha ao configurar efeito {:?}: {reason}",
                effect.kind
            ))),
        }
    }
    active
}

fn attach_effect(clip: &ges::Clip, effect: &ClipEffect) -> Result<ges::Effect, String> {
    let desc = match effect.kind {
        VideoEffectKind::Blur => "video videoconvert ! gaussianblur ! videoconvert".into(),
        VideoEffectKind::FilmStyle | VideoEffectKind::Glitch => format!(
            "video videobalance ! videoconvert ! {} ! videoconvert",
            crate::effects_pixels::FACTORY,
        ),
        _ => "video videobalance".into(),
    };
    let element = ges::Effect::new(&desc).map_err(|err| err.to_string())?;
    GESContainerExt::add(clip, &element).map_err(|err| err.to_string())?;
    if let Err(property) = set_parameters(&element, effect.params) {
        let _ = GESContainerExt::remove(clip, &element);
        return Err(format!("propriedade {property} indisponível"));
    }
    Ok(element)
}

pub(crate) fn set_parameters(
    effect: &ges::Effect,
    params: EffectParameters,
) -> Result<(), &'static str> {
    let balance = |contrast: f64, saturation: f64, brightness, hue| {
        [
            ("contrast", contrast.clamp(0.0, 2.0)),
            ("saturation", saturation.clamp(0.0, 2.0)),
            ("brightness", brightness),
            ("hue", hue),
        ]
    };
    let values = match params {
        EffectParameters::Film(p) => balance(
            1.0 + (p.contrast - 1.0) * p.intensity,
            1.0 + (p.saturation - 1.0) * p.intensity,
            0.0,
            -0.08 * p.intensity,
        ),
        EffectParameters::Cinematic(p) => balance(
            1.0 + (p.contrast - 1.0) * p.intensity,
            1.0 + (p.saturation - 1.0) * p.intensity,
            p.brightness * p.intensity,
            0.0,
        ),
        EffectParameters::Glitch(p) => {
            balance(1.0 + 0.45 * p.intensity, 1.0 + 0.50 * p.intensity, 0.0, 0.0)
        }
        EffectParameters::Blur(p) => return set_value(effect, "sigma", 20.0 * p.radius),
        _ => return Ok(()),
    };
    for (name, value) in values {
        set_value(effect, name, value)?;
    }
    match params {
        EffectParameters::Film(p) => set_value(effect, "vignette", p.vignette * p.intensity)?,
        EffectParameters::Glitch(p) => {
            set_value(effect, "rgb-shift", p.rgb_shift * p.intensity)?;
            set_value(effect, "shift-frequency", p.frequency)?;
        }
        _ => {}
    }
    Ok(())
}

fn set_value(effect: &ges::Effect, name: &'static str, value: f64) -> Result<(), &'static str> {
    TimelineElementExt::set_child_property(effect, name, &value.to_value()).map_err(|_| name)
}
