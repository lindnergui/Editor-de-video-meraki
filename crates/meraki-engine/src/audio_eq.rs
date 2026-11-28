use glib::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::{EqFilterType, EQ_BAND_SPECS, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB};
use tracing::warn;

#[derive(Clone)]
pub(crate) struct ParametricEq {
    pub(crate) bands: [glib::Object; 4],
}

impl ParametricEq {
    pub(crate) fn configure(element: &gst::Element) -> Option<Self> {
        let result = Self::resolve(element);
        if result.is_none() {
            warn!(element = %element.name(), "falha ao configurar as quatro bandas do equalizador");
        }
        result
    }

    fn resolve(element: &gst::Element) -> Option<Self> {
        let proxy = element.dynamic_cast_ref::<gst::ChildProxy>()?;
        let bands = ["band0", "band1", "band2", "band3"]
            .into_iter()
            .map(|name| proxy.child_by_name(name))
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .ok()?;
        let eq = Self { bands };
        for (band, spec) in eq.bands.iter().zip(EQ_BAND_SPECS) {
            let nick = match spec.filter_type {
                EqFilterType::LowShelf => "low-shelf",
                EqFilterType::Peak => "peak",
                EqFilterType::HighShelf => "high-shelf",
            };
            let value_type = band.find_property("type")?.value_type();
            let value = glib::EnumClass::with_type(value_type)?.to_value_by_nick(nick)?;
            band.set_property("type", value);
            band.set_property("freq", spec.freq_hz);
            band.set_property("bandwidth", spec.bandwidth_hz);
        }
        Some(eq)
    }

    pub(crate) fn set_gain(&self, index: usize, gain_db: f32) {
        if let Some(band) = self.bands.get(index) {
            band.set_property(
                "gain",
                f64::from(gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB)),
            );
        }
    }

    pub(crate) fn set_gains(&self, gains: [f32; 4]) {
        for (index, gain) in gains.into_iter().enumerate() {
            self.set_gain(index, gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_children_receive_exact_band_types_frequencies_widths_and_gains() {
        crate::ensure_media_initialized().unwrap();
        let element = gst::ElementFactory::make("equalizer-nbands")
            .property("num-bands", 4_u32)
            .build()
            .unwrap();
        let eq = ParametricEq::configure(&element).unwrap();
        let gains = [3.5_f32, -4.0, 8.0, -2.25];
        eq.set_gains(gains);
        assert_eq!(element.property::<u32>("num-bands"), 4);
        for (index, (freq, bandwidth, nick)) in [
            (90.0, 140.0, "low-shelf"),
            (320.0, 300.0, "peak"),
            (2200.0, 1800.0, "peak"),
            (8500.0, 5000.0, "high-shelf"),
        ]
        .into_iter()
        .enumerate()
        {
            let child = &eq.bands[index];
            let property = child.property_value("type");
            assert_eq!(
                glib::EnumValue::from_value(&property).unwrap().1.nick(),
                nick
            );
            assert_eq!(child.property::<f64>("freq"), freq);
            assert_eq!(child.property::<f64>("bandwidth"), bandwidth);
            assert_eq!(child.property::<f64>("gain"), f64::from(gains[index]));
        }
    }
}
