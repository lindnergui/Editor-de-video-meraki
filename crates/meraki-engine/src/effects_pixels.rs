use glib::prelude::*;
use gst_video::gst_base;
use gstreamer as gst;
use gstreamer_video as gst_video;

pub(crate) const FACTORY: &str = "merakivideoeffects";

pub(crate) fn register() -> Result<(), glib::BoolError> {
    gst::Element::register(None, FACTORY, gst::Rank::NONE, PixelEffects::static_type())
}

glib::wrapper! {
    pub struct PixelEffects(ObjectSubclass<imp::PixelEffects>)
        @extends gst_video::VideoFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

mod imp {
    use super::*;
    use gst_video::prelude::*;
    use gst_video::subclass::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct PixelEffects {
        vignette: AtomicU64,
        shift: AtomicU64,
        frequency: AtomicU64,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PixelEffects {
        const NAME: &'static str = "GstMerakiPixelEffects";
        type Type = super::PixelEffects;
        type ParentType = gst_video::VideoFilter;
    }

    impl PixelEffects {
        fn param(&self, name: &str) -> Option<&AtomicU64> {
            match name {
                "vignette" => Some(&self.vignette),
                "rgb-shift" => Some(&self.shift),
                "shift-frequency" => Some(&self.frequency),
                _ => None,
            }
        }
    }

    impl ObjectImpl for PixelEffects {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<[glib::ParamSpec; 3]> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                ["vignette", "rgb-shift", "shift-frequency"].map(|name| {
                    glib::ParamSpecDouble::builder(name)
                        .minimum(0.0)
                        .maximum(1.0)
                        .build()
                })
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, spec: &glib::ParamSpec) {
            if let (Some(param), Ok(value)) = (self.param(spec.name()), value.get::<f64>()) {
                param.store(value.to_bits(), Ordering::Relaxed);
            }
        }

        fn property(&self, _id: usize, spec: &glib::ParamSpec) -> glib::Value {
            self.param(spec.name()).map_or(0.0, read).to_value()
        }
    }

    impl GstObjectImpl for PixelEffects {}

    impl ElementImpl for PixelEffects {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static METADATA: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(METADATA.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "Meraki pixel effects",
                    "Filter/Effect/Video",
                    "Vignette and RGB separation",
                    "Meraki Contributors",
                )
            }))
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            TEMPLATES.get_or_init(|| {
                let caps = gst_video::VideoCapsBuilder::new()
                    .format(gst_video::VideoFormat::Rgba)
                    .build();
                [
                    ("sink", gst::PadDirection::Sink),
                    ("src", gst::PadDirection::Src),
                ]
                .into_iter()
                .filter_map(|(name, direction)| {
                    gst::PadTemplate::new(name, direction, gst::PadPresence::Always, &caps).ok()
                })
                .collect()
            })
        }
    }

    impl BaseTransformImpl for PixelEffects {
        const MODE: gst_base::subclass::BaseTransformMode =
            gst_base::subclass::BaseTransformMode::AlwaysInPlace;
        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;
    }

    impl VideoFilterImpl for PixelEffects {
        fn transform_frame_ip(
            &self,
            frame: &mut gst_video::VideoFrameRef<&mut gst::BufferRef>,
        ) -> Result<gst::FlowSuccess, gst::FlowError> {
            let vignette = read(&self.vignette) as f32;
            let shift = read(&self.shift);
            if vignette == 0.0 && shift == 0.0 {
                return Ok(gst::FlowSuccess::Ok);
            }

            let width = frame.width() as usize;
            let height = frame.height() as usize;
            let row_bytes = width.checked_mul(4).ok_or(gst::FlowError::Error)?;
            let stride = frame
                .plane_stride()
                .first()
                .and_then(|&value| usize::try_from(value).ok())
                .ok_or(gst::FlowError::Error)?;
            if stride < row_bytes || width == 0 || height == 0 {
                return Err(gst::FlowError::Error);
            }
            let time = frame
                .buffer()
                .pts()
                .unwrap_or(gst::ClockTime::ZERO)
                .nseconds() as f64
                / 1e9;
            let phase = (time * (0.5 + 11.5 * read(&self.frequency)) * std::f64::consts::TAU).cos();
            let offset = (width as f64 * 0.03 * shift * phase).round() as isize;
            let plane = frame.plane_data_mut(0).map_err(|_| gst::FlowError::Error)?;
            for (y, row) in plane.chunks_mut(stride).take(height).enumerate() {
                let Some(row) = row.get_mut(..row_bytes) else {
                    return Err(gst::FlowError::Error);
                };
                if offset != 0 {
                    shift_channels(row, offset);
                }
                if vignette > 0.0 {
                    shade_edges(row, y, height, vignette);
                }
            }
            Ok(gst::FlowSuccess::Ok)
        }
    }

    fn read(param: &AtomicU64) -> f64 {
        f64::from_bits(param.load(Ordering::Relaxed))
    }

    // Percorre cada canal na direção que mantém seus pixels de origem intactos.
    fn shift_channels(row: &mut [u8], offset: isize) {
        let width = row.len() / 4;
        let (left, right) = if offset > 0 { (2, 0) } else { (0, 2) };
        let shift = offset.unsigned_abs();
        for x in 0..width {
            let tail = width - 1 - x;
            row[x * 4 + left] = row[(x + shift).min(width - 1) * 4 + left];
            row[tail * 4 + right] = row[tail.saturating_sub(shift) * 4 + right];
        }
    }

    fn shade_edges(row: &mut [u8], y: usize, height: usize, amount: f32) {
        let width = (row.len() / 4) as f32;
        let ny = (y as f32 + 0.5) * (2.0 / height as f32) - 1.0;
        let (pixels, _) = row.as_chunks_mut::<4>();
        for (x, pixel) in pixels.iter_mut().enumerate() {
            let nx = (x as f32 + 0.5) * (2.0 / width) - 1.0;
            let radius = (nx * nx + ny * ny) * 0.5;
            let gain = 1.0 - amount * radius * radius;
            for channel in &mut pixel[..3] {
                *channel = (f32::from(*channel) * gain).round() as u8;
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn effects_shift_rgb_and_shade_edges_without_changing_alpha() {
            let mut row = [10, 20, 30, 17, 40, 50, 60, 23, 70, 80, 90, 29];
            shift_channels(&mut row, 1);
            assert_eq!(row, [10, 20, 60, 17, 10, 50, 90, 23, 40, 80, 90, 29]);
            shade_edges(&mut row, 1, 3, 1.0);
            assert!(row[2] < 60 && row[10] < 90);
            assert_eq!([row[3], row[5], row[7], row[11]], [17, 50, 23, 29]);
        }
    }
}
