use crate::frame_sink::LEVEL_INTERVAL_NS;
use crate::protocol::TimelineRenderClip;
use glib::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_controller as gst_ctrl;
use gstreamer_controller::prelude::*;
use gstreamer_editing_services as ges;
use gstreamer_editing_services::prelude::*;
use meraki_core::{
    db_to_linear, AudioEffectKind, TrackAudioSettings, VolumeKeyframe, EQ_BAND_FREQUENCIES_HZ,
    MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB,
};
use tracing::{debug, warn};

const EQ_BAND_WIDTHS_HZ: [f64; 4] = [120.0, 320.0, 1_400.0, 6_000.0];
const NR_NAME: &str = "meraki_nr";
const EQ_NAME: &str = "meraki_eq";
const LEVELER_NAME: &str = "meraki_lvl";
const ECHO_NAME: &str = "meraki_echo";
const REVERB_NAME: &str = "meraki_reverb";
const VOL_NAME: &str = "meraki_trk_vol";

#[derive(Clone)]
pub(crate) struct ClipAudioDsp {
    pub(crate) eq: Option<gst::Element>,
    pub(crate) noise_reduction: Option<gst::Element>,
    pub(crate) voice_leveler: Option<gst::Element>,
    pub(crate) echo: Option<gst::Element>,
    pub(crate) reverb: Option<gst::Element>,
    pub(crate) volume: Option<gst::Element>,
}

pub(crate) fn effective_muted(settings: &TrackAudioSettings, any_solo: bool) -> bool {
    settings.muted || (any_solo && !settings.solo)
}

pub(crate) fn build_dsp_chain_description(meter_name: Option<&str>) -> String {
    let mut parts = vec!["audioconvert".to_string()];
    if gst::ElementFactory::find("audioresample").is_some() {
        parts.push("audioresample".to_string());
    }
    if gst::ElementFactory::find("audiocheblimit").is_some() {
        parts.push(format!("audiocheblimit name={NR_NAME} mode=high-pass poles=4 cutoff=0"));
    } else {
        warn!("plugin 'audiocheblimit' indisponível; redução de ruído desabilitada nesta máquina");
    }
    if gst::ElementFactory::find("equalizer-nbands").is_some() {
        parts.push(format!("equalizer-nbands name={EQ_NAME} num-bands=4"));
    } else {
        warn!("plugin 'equalizer-nbands' indisponível; equalizador 4 bandas desabilitado");
    }
    if gst::ElementFactory::find("audiodynamic").is_some() {
        parts.push(format!(
            "audiodynamic name={LEVELER_NAME} characteristics=soft-knee mode=compressor ratio=1 threshold=1"
        ));
    } else {
        warn!("plugin 'audiodynamic' indisponível; nivelador de voz desabilitado");
    }
    if gst::ElementFactory::find("audioecho").is_some() {
        parts.push(format!(
            "audioecho name={ECHO_NAME} max-delay=500000000 delay=250000000 intensity=0 feedback=0"
        ));
    } else {
        warn!("plugin 'audioecho' indisponível; efeito Eco desabilitado");
    }
    if gst::ElementFactory::find("freeverb").is_some() {
        parts.push(format!(
            "audioconvert ! freeverb name={REVERB_NAME} room-size=0.55 damping=0.4 width=1 level=0 ! audioconvert"
        ));
    } else {
        warn!("plugin 'freeverb' indisponível; efeito Reverb desabilitado");
    }
    if gst::ElementFactory::find("volume").is_some() {
        parts.push(format!("volume name={VOL_NAME} volume=1 mute=false"));
    } else {
        warn!("plugin 'volume' indisponível; fader de trilha ao vivo desabilitado");
    }
    if let Some(name) = meter_name {
        parts.push(format!("level name={name} post-messages=true interval={LEVEL_INTERVAL_NS}"));
    }
    parts.push("audioconvert".to_string());
    parts.join(" ! ")
}

pub(crate) fn resolve_clip_dsp(effect: &ges::Effect) -> ClipAudioDsp {
    let eff_elem: &ges::TrackElement = effect.upcast_ref();
    let bin = eff_elem.nleobject().downcast::<gst::Bin>().ok();
    let find = |elem_name: &str, prop: &str| {
        bin.as_ref()
            .and_then(|b| b.by_name(elem_name))
            .or_else(|| {
                TrackElementExt::lookup_child(eff_elem, &format!("{elem_name}::{prop}"))
                    .map(|(el, _)| el)
            })
    };
    ClipAudioDsp {
        eq: find(EQ_NAME, "num-bands"),
        noise_reduction: find(NR_NAME, "cutoff"),
        voice_leveler: find(LEVELER_NAME, "ratio"),
        echo: find(ECHO_NAME, "intensity"),
        reverb: find(REVERB_NAME, "level"),
        volume: find(VOL_NAME, "volume"),
    }
}

pub(crate) fn apply_settings_to_clip(
    dsp: &ClipAudioDsp,
    settings: &TrackAudioSettings,
    any_solo: bool,
) {
    if let Some(ref nr) = dsp.noise_reduction {
        apply_noise_reduction(nr, settings.noise_reduction_enabled, settings.noise_reduction_strength);
    }
    if let Some(ref eq) = dsp.eq {
        if let Ok(proxy) = eq.clone().dynamic_cast::<gst::ChildProxy>() {
            for (idx, gain_db) in settings.eq_gains_db.iter().enumerate() {
                apply_eq_band_on_proxy(&proxy, idx, *gain_db);
            }
        }
    }
    if let Some(ref lvl) = dsp.voice_leveler {
        apply_voice_leveler(lvl, settings.voice_leveler_enabled);
    }
    if let Some(ref echo) = dsp.echo {
        apply_echo(echo, settings.has_effect(AudioEffectKind::Echo));
    }
    if let Some(ref reverb) = dsp.reverb {
        apply_reverb(reverb, settings.has_effect(AudioEffectKind::Reverb));
    }
    if let Some(ref vol) = dsp.volume {
        vol.set_property("volume", db_to_linear(settings.gain_db));
        vol.set_property("mute", effective_muted(settings, any_solo));
    }
}

pub(crate) fn apply_eq_band_on_proxy(proxy: &gst::ChildProxy, idx: usize, gain_db: f32) {
    let Some(band_obj) = proxy.child_by_index(idx as u32) else {
        return;
    };
    let clamped = f64::from(gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB));
    set_numeric_prop(&band_obj, "freq", EQ_BAND_FREQUENCIES_HZ[idx]);
    set_numeric_prop(&band_obj, "bandwidth", EQ_BAND_WIDTHS_HZ[idx]);
    set_numeric_prop(&band_obj, "gain", clamped);
}

pub(crate) fn apply_noise_reduction(nr: &gst::Element, enabled: bool, strength: f32) {
    let cutoff = if enabled { 80.0_f32 + 180.0_f32 * strength.clamp(0.0, 1.0) } else { 0.0_f32 };
    nr.set_property("cutoff", cutoff);
}

pub(crate) fn apply_voice_leveler(lvl: &gst::Element, enabled: bool) {
    let (ratio, threshold) = if enabled { (0.35_f32, 0.25_f32) } else { (1.0_f32, 1.0_f32) };
    lvl.set_property("ratio", ratio);
    lvl.set_property("threshold", threshold);
}

pub(crate) fn apply_echo(echo: &gst::Element, enabled: bool) {
    let (intensity, feedback) = if enabled { (0.35_f32, 0.30_f32) } else { (0.0_f32, 0.0_f32) };
    echo.set_property("intensity", intensity);
    echo.set_property("feedback", feedback);
}

pub(crate) fn apply_reverb(reverb: &gst::Element, enabled: bool) {
    reverb.set_property("level", if enabled { 0.35_f32 } else { 0.0_f32 });
}

fn set_numeric_prop(obj: &impl ObjectExt, name: &str, value: f64) {
    let Some(pspec) = obj.find_property(name) else {
        return;
    };
    if pspec.value_type() == glib::Type::F64 {
        obj.set_property(name, value);
    } else if pspec.value_type() == glib::Type::F32 {
        obj.set_property(name, value as f32);
    }
}

pub(crate) fn apply_volume_keyframes(
    ges_clip: &ges::Clip,
    duration_ns: u64,
    keyframes: &[VolumeKeyframe],
) {
    if keyframes.is_empty() {
        return;
    }
    let elems = ges_clip.find_track_elements(
        None::<&ges::Track>,
        ges::TrackType::AUDIO,
        ges::AudioSource::static_type(),
    );
    let Some(track_elem) = elems.into_iter().next() else {
        return;
    };

    let mut sorted = keyframes.to_vec();
    sorted.sort_by_key(|k| k.time_offset_ns);
    let mut points: Vec<(u64, f64)> = Vec::with_capacity(sorted.len() + 2);
    if sorted[0].time_offset_ns > 0 {
        points.push((0, db_to_linear(sorted[0].gain_db)));
    }
    for kf in &sorted {
        let t = kf.time_offset_ns.min(duration_ns);
        let gain = db_to_linear(kf.gain_db);
        if let Some(last) = points.last_mut().filter(|(lt, _)| *lt == t) {
            last.1 = gain;
        } else {
            points.push((t, gain));
        }
    }
    if let Some(last) = points.last().copied() {
        if last.0 < duration_ns {
            points.push((duration_ns, last.1));
        }
    }

    let inpoint_ns = track_elem.upcast_ref::<ges::TimelineElement>().inpoint().nseconds();
    let ctrl = gst_ctrl::InterpolationControlSource::new();
    ctrl.set_mode(gst_ctrl::InterpolationMode::CubicMonotonic);
    for (offset_ns, value) in points {
        let _ = ctrl.set(gst::ClockTime::from_nseconds(inpoint_ns.saturating_add(offset_ns)), value);
    }
    if !track_elem.set_control_source(&ctrl, "volume", "direct-absolute") {
        debug!("propriedade 'volume' não suportou binding direct-absolute no AudioSource");
    }
}

pub(crate) fn attach_clip_audio_chain(
    ges_clip: &ges::Clip,
    clip: &TimelineRenderClip,
    meter_name: Option<String>,
) -> Option<ges::Effect> {
    apply_volume_keyframes(ges_clip, clip.duration_ns, &clip.volume_keyframes);
    if gst::ElementFactory::find("audioconvert").is_none() {
        warn!("plugin GStreamer 'audioconvert' ausente; cadeia DSP da trilha ignorada");
        return None;
    }
    let desc = build_dsp_chain_description(meter_name.as_deref());
    match ges::Effect::new(&desc) {
        Ok(effect) => match GESContainerExt::add(ges_clip, &effect) {
            Ok(()) => Some(effect),
            Err(err) => {
                warn!(error = %err, "falha ao anexar cadeia DSP ao clipe GES");
                None
            }
        },
        Err(err) => {
            warn!(error = %err, desc, "falha ao instanciar efeito DSP da trilha");
            None
        }
    }
}
