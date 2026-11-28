use crate::pipeline::PipelineError;
use gstreamer as gst;
use gstreamer::prelude::*;

pub(crate) const MIX_RATE: i32 = 48_000;

pub(crate) fn shorten_mixer_blocks(track: &gstreamer_editing_services::AudioTrack) {
    for element in track.iterate_recurse().into_iter().filter_map(Result::ok) {
        if element
            .factory()
            .is_some_and(|factory| factory.name() == "audiomixer")
            && element.find_property("output-buffer-duration").is_some()
        {
            element.set_property("output-buffer-duration", 5_000_000_u64);
        }
    }
}

pub(crate) fn float_audio_caps(rate: i32) -> gst::Caps {
    gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("layout", "interleaved")
        .field("rate", rate)
        .build()
}

#[derive(Clone, Copy)]
pub(crate) struct MasterMix {
    pub(crate) gain: f64,
    pub(crate) muted: bool,
}

impl Default for MasterMix {
    fn default() -> Self {
        Self {
            gain: 1.0,
            muted: false,
        }
    }
}

/// O mixer e os efeitos mantêm headroom em F32 até este ponto, antes de converter para o dispositivo ou codec.
pub(crate) fn protect_peaks(
    element: &impl IsA<gst::Element>,
    gain: f32,
) -> Result<(), PipelineError> {
    let init_error = || PipelineError::TimelineSetup {
        step: "master_limiter",
        reason: "pad de áudio indisponível para proteção de picos".into(),
    };
    let source = element.static_pad("src").ok_or_else(init_error)?;
    source
        .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if let Some(buffer) = info.buffer_mut() {
                if let Ok(mut map) = buffer.make_mut().map_writable() {
                    let (samples, _) = map.as_mut_slice().as_chunks_mut::<4>();
                    for bytes in samples {
                        let sample = f32::from_le_bytes(*bytes) * gain;
                        *bytes = soft_limit(sample).to_le_bytes();
                    }
                }
            }
            gst::PadProbeReturn::Ok
        })
        .ok_or_else(init_error)?;
    Ok(())
}

fn soft_limit(sample: f32) -> f32 {
    const THRESHOLD: f32 = 0.85;
    const HEADROOM: f32 = 0.14;
    if !sample.is_finite() {
        return 0.0;
    }
    let excess = sample.abs() - THRESHOLD;
    if excess <= 0.0 {
        sample
    } else {
        (THRESHOLD + HEADROOM * (excess / (excess + HEADROOM))).copysign(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_nominal_samples_intact_and_softens_overloads_below_full_scale() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.85), -0.85);
        assert!(soft_limit(4.0) > soft_limit(1.0));
        assert!(soft_limit(50.0) < 0.99);
        assert_eq!(soft_limit(-3.0), -soft_limit(3.0));
    }
}
