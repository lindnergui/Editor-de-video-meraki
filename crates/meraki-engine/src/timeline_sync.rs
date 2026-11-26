use crate::clip_prepare::PipelineError;
use crate::effects_ges::configure_title_source;
use crate::keyframes_ges::{
    apply_motion, attach_blur_transition, needs_blur, transitions, update_blur_transition,
    zoom_params,
};
use crate::protocol::{EngineWorkerChannel, TimelineRenderClip};
use crate::video_effects::{self, ActiveEffects};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use gstreamer_video as gst_video;
use meraki_core::{NormalizedTitleBox, PreviewQuality};

pub(crate) struct ActiveVideo {
    source: ges::TrackElement,
    effects: ActiveEffects,
    blur_transition: Option<ges::Effect>,
    base: (i32, i32),
    default_size: (i32, i32),
    text_overlay: Option<gst::Element>,
    pub last_title_bounds: Option<NormalizedTitleBox>,
}

impl ActiveVideo {
    pub(crate) fn new(
        ges_clip: &ges::Clip,
        clip: &TimelineRenderClip,
        natural_size: Option<(i32, i32)>,
        quality: PreviewQuality,
        worker: &EngineWorkerChannel,
    ) -> Result<Self, PipelineError> {
        let mut sources = ges_clip
            .find_track_elements(
                None::<&ges::Track>,
                ges::TrackType::VIDEO,
                ges::VideoSource::static_type(),
            )
            .into_iter();
        let source = sources.next().ok_or(PipelineError::LiveProperty {
            clip_id: clip.clip_id,
            property: "VideoSource",
        })?;
        let text_overlay = if clip.title.is_some() {
            if sources.next().is_some() {
                return Err(PipelineError::LiveProperty {
                    clip_id: clip.clip_id,
                    property: "TitleSource duplicado",
                });
            }
            Some(
                find_single_text_overlay(&source).ok_or(PipelineError::LiveProperty {
                    clip_id: clip.clip_id,
                    property: "esperado um único textoverlay",
                })?,
            )
        } else {
            None
        };
        let (w, h) = quality.max_dimensions();
        let default_size = (
            child_int(&source, "width").unwrap_or(0),
            child_int(&source, "height").unwrap_or(0),
        );
        let base = natural_size.unwrap_or((w as i32, h as i32));
        let effects = video_effects::attach_effects(ges_clip, clip, worker);
        let blur_transition = attach_blur_transition(ges_clip, clip).map_err(|reason| {
            PipelineError::TimelineSetup {
                step: "attach_blur_transition",
                reason,
            }
        })?;
        let result = Self {
            source,
            effects,
            blur_transition,
            base,
            default_size,
            text_overlay,
            last_title_bounds: None,
        };
        result.apply_geometry(clip)?;
        Ok(result)
    }

    pub(crate) fn supports_update(
        &self,
        previous: &TimelineRenderClip,
        next: &TimelineRenderClip,
    ) -> bool {
        if previous.title.is_some() != next.title.is_some()
            || self.blur_transition.is_some() != needs_blur(next)
        {
            return false;
        }
        let signature = |clip: &TimelineRenderClip| {
            clip.effects
                .iter()
                .filter(|effect| effect.enabled)
                .map(|effect| (effect.id, effect.kind))
                .collect::<Vec<_>>()
        };
        signature(previous) == signature(next)
            && next
                .effects
                .iter()
                .filter(|effect| effect.enabled && video_effects::has_element(effect.kind))
                .all(|effect| self.effects.contains_key(&effect.id))
    }

    pub(crate) fn apply_live(
        &self,
        previous: &TimelineRenderClip,
        clip: &TimelineRenderClip,
    ) -> Result<(), PipelineError> {
        let transition_changed = transitions(previous) != transitions(clip);
        if previous.transform != clip.transform
            || previous.title != clip.title
            || zoom_params(previous) != zoom_params(clip)
            || transition_changed
        {
            self.apply_geometry(clip)?;
        }
        if let Some(effect) = self.blur_transition.as_ref().filter(|_| transition_changed) {
            update_blur_transition(effect, clip);
        }
        for effect in &clip.effects {
            if let Some(element) = self.effects.get(&effect.id) {
                if !previous
                    .effects
                    .iter()
                    .any(|old| old.id == effect.id && old.params == effect.params)
                {
                    video_effects::set_parameters(element, effect.params).map_err(|property| {
                        PipelineError::LiveProperty {
                            clip_id: clip.clip_id,
                            property,
                        }
                    })?;
                }
            }
        }
        Ok(())
    }

    fn apply_geometry(&self, clip: &TimelineRenderClip) -> Result<(), PipelineError> {
        let elem: &ges::TimelineElement = self.source.upcast_ref();
        let set = |name, value: glib::Value| {
            elem.set_child_property(name, &value)
                .map_err(|_| PipelineError::LiveProperty {
                    clip_id: clip.clip_id,
                    property: name,
                })
        };
        for name in ["alpha", "posx", "posy", "width", "height"] {
            if self.source.control_binding(name).is_some() {
                let _ = self.source.remove_control_binding(name);
            }
        }
        set("alpha", f64::from(clip.transform.opacity).to_value())?;
        if let Some(title) = &clip.title {
            let source = self.source.downcast_ref::<ges::TitleSource>().ok_or(
                PipelineError::LiveProperty {
                    clip_id: clip.clip_id,
                    property: "TitleSource",
                },
            )?;
            configure_title_source(source, title, clip);
            set("posx", 0_i32.to_value())?;
            set("posy", 0_i32.to_value())?;
            set("width", self.default_size.0.to_value())?;
            set("height", self.default_size.1.to_value())?;
        } else {
            let transform = clip.transform;
            let (width, height) = if transform.scale == 1.0 {
                self.default_size
            } else {
                (
                    (self.base.0 as f32 * transform.scale).round().max(1.0) as i32,
                    (self.base.1 as f32 * transform.scale).round().max(1.0) as i32,
                )
            };
            set("posx", (transform.pos_x.round() as i32).to_value())?;
            set("posy", (transform.pos_y.round() as i32).to_value())?;
            set("width", width.to_value())?;
            set("height", height.to_value())?;
        }
        apply_motion(&self.source, clip, self.base);
        Ok(())
    }

    pub(crate) fn title_bounds(&self, canvas: [u32; 2]) -> Option<NormalizedTitleBox> {
        let overlay = self.text_overlay.as_ref()?;
        let caps = overlay.static_pad("video_sink")?.current_caps()?;
        let input = gst_video::VideoInfo::from_caps(&caps).ok()?;
        let x = f64::from(overlay.property::<i32>("text-x"));
        let y = f64::from(overlay.property::<i32>("text-y"));
        let width = f64::from(overlay.property::<u32>("text-width"));
        let height = f64::from(overlay.property::<u32>("text-height"));
        let scale = |prop, input| {
            child_int(&self.source, prop)
                .filter(|&size| size > 0)
                .map_or(1.0, |size| f64::from(size) / f64::from(input))
        };
        let sx = scale("width", input.width());
        let sy = scale("height", input.height());
        let ox = f64::from(child_int(&self.source, "posx").unwrap_or(0));
        let oy = f64::from(child_int(&self.source, "posy").unwrap_or(0));
        NormalizedTitleBox::from_pixels([ox + x * sx, oy + y * sy, width * sx, height * sy], canvas)
    }
}

fn child_int(source: &ges::TrackElement, name: &str) -> Option<i32> {
    TimelineElementExt::child_property(source, name)?
        .get::<i32>()
        .ok()
}

fn find_single_text_overlay(source: &ges::TrackElement) -> Option<gst::Element> {
    let nle = source.nleobject().downcast::<gst::Bin>().ok()?;
    let mut overlays = nle
        .iterate_recurse()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|element| {
            element
                .factory()
                .is_some_and(|factory| factory.name() == "textoverlay")
        });
    let overlay = overlays.next()?;
    overlays.next().is_none().then_some(overlay)
}
