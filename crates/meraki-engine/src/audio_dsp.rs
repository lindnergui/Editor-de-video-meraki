use crate::audio_dsp_chain::{
    apply_echo, apply_noise_reduction, apply_reverb, apply_settings_to_clip,
    apply_voice_leveler, attach_clip_audio_chain, effective_muted, resolve_clip_dsp, ClipAudioDsp,
};
use crate::audio_meter::AudioMeterAggregator;
use crate::pipeline::PipelineError;
use crate::protocol::{TimelineRenderClip, TrackAudioMix};
use gstreamer as gst;
use gstreamer_editing_services as ges;
use meraki_core::{
    db_to_linear, eq_band_index, AudioEffectKind, EqBand, TrackAudioSettings, TrackId,
    MAX_EQ_GAIN_DB, MAX_TRACK_GAIN_DB, MIN_EQ_GAIN_DB, MIN_TRACK_GAIN_DB,
};
use std::collections::HashMap;

mod live;
pub(crate) use live::checked_db;

/// Controlador de cadeia DSP de áudio por trilha com atualização ao vivo sem reconstruir o GES.
pub(crate) struct AudioDspController {
    settings: HashMap<TrackId, TrackAudioSettings>,
    pending_effects: Vec<(TrackId, ges::Effect)>,
    live_handles: HashMap<TrackId, Vec<ClipAudioDsp>>,
    next_meter_seq: u64,
}

impl AudioDspController {
    pub(crate) fn settings_snapshot(&self) -> Vec<TrackAudioMix> {
        self.settings.iter().map(|(&track_id, settings)| TrackAudioMix {
            track_id, settings: settings.clone(),
        }).collect()
    }
    pub(crate) fn new() -> Self {
        Self {
            settings: HashMap::new(),
            pending_effects: Vec::new(),
            live_handles: HashMap::new(),
            next_meter_seq: 0,
        }
    }

    pub(crate) fn prepare_sync(&mut self, audio_mix: Vec<TrackAudioMix>) -> Result<(), PipelineError> {
        self.clear_handles();
        self.settings.clear();
        for mix in audio_mix {
            mix.settings
                .validate()
                .map_err(|err| PipelineError::TimelineSetup { step: "audio_mix", reason: err.to_string() })?;
            self.settings.insert(mix.track_id, mix.settings);
        }
        Ok(())
    }

    pub(crate) fn clear_handles(&mut self) {
        self.pending_effects.clear();
        self.live_handles.clear();
    }

    /// Anexa keyframes de volume (`InterpolationControlSource`) e a cadeia DSP ao clipe GES.
    pub(crate) fn attach_clip_audio(
        &mut self,
        ges_clip: &ges::Clip,
        clip: &TimelineRenderClip,
        meter: &mut AudioMeterAggregator,
    ) {
        self.settings.entry(clip.track_id).or_default();
        let meter_name = gst::ElementFactory::find("level").map(|_| {
            self.next_meter_seq = self.next_meter_seq.wrapping_add(1);
            format!("meraki_trk_level_{}", self.next_meter_seq)
        });
        if let Some(effect) = attach_clip_audio_chain(ges_clip, clip, meter_name.clone()) {
            if let Some(name) = meter_name {
                meter.register_track_level(name, clip.track_id);
            }
            self.pending_effects.push((clip.track_id, effect));
        }
    }

    /// Resolve elementos GStreamer internos após `timeline.commit()` e aplica estado atual.
    pub(crate) fn finalize_after_commit(&mut self) {
        let pending = std::mem::take(&mut self.pending_effects);
        let any_solo = self.any_track_solo();
        for (track_id, effect) in pending {
            let dsp = resolve_clip_dsp(&effect);
            let settings = self.settings.get(&track_id).cloned().unwrap_or_default();
            apply_settings_to_clip(&dsp, &settings, any_solo);
            self.live_handles.entry(track_id).or_default().push(dsp);
        }
    }

    pub(crate) fn set_track_gain(&mut self, track_id: TrackId, gain_db: f32) -> Result<(), PipelineError> {
        if !gain_db.is_finite() {
            return Err(PipelineError::InvalidParameter { name: "gain_db", reason: "ganho deve ser finito" });
        }
        self.finalize_after_commit();
        let clamped = gain_db.clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB);
        self.settings.entry(track_id).or_default().gain_db = clamped;
        let linear = db_to_linear(clamped);
        if let Some(clips) = self.live_handles.get(&track_id) {
            for clip in clips {
                if let Some(ref vol) = clip.volume {
                    vol.set_gain(linear);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_track_mute(&mut self, track_id: TrackId, muted: bool) -> Result<(), PipelineError> {
        self.finalize_after_commit();
        self.settings.entry(track_id).or_default().muted = muted;
        let any_solo = self.any_track_solo();
        if let (Some(settings), Some(clips)) = (self.settings.get(&track_id), self.live_handles.get(&track_id)) {
            let eff_mute = effective_muted(settings, any_solo);
            for clip in clips {
                if let Some(ref vol) = clip.volume {
                    vol.set_mute(eff_mute);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_track_solo(&mut self, track_id: TrackId, solo: bool) -> Result<(), PipelineError> {
        self.finalize_after_commit();
        self.settings.entry(track_id).or_default().solo = solo;
        let any_solo = self.any_track_solo();
        for (tid, clips) in &self.live_handles {
            if let Some(settings) = self.settings.get(tid) {
                let eff_mute = effective_muted(settings, any_solo);
                for clip in clips {
                    if let Some(ref vol) = clip.volume {
                        vol.set_mute(eff_mute);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn update_eq_band(
        &mut self,
        track_id: TrackId,
        band: EqBand,
        gain_db: f32,
    ) -> Result<(), PipelineError> {
        if !gain_db.is_finite() {
            return Err(PipelineError::InvalidParameter { name: "eq_gain_db", reason: "ganho EQ deve ser finito" });
        }
        self.finalize_after_commit();
        let idx = eq_band_index(band);
        let clamped = gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB);
        self.settings.entry(track_id).or_default().eq_gains_db[idx] = clamped;
        if let Some(clips) = self.live_handles.get(&track_id) {
            for clip in clips {
                if let Some(ref eq) = clip.eq {
                    eq.set_gain(idx, clamped);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_noise_reduction(
        &mut self,
        track_id: TrackId,
        enabled: bool,
        strength: f32,
    ) -> Result<(), PipelineError> {
        if !strength.is_finite() {
            return Err(PipelineError::InvalidParameter {
                name: "noise_reduction_strength",
                reason: "intensidade deve ser finita",
            });
        }
        self.finalize_after_commit();
        let clamped = strength.clamp(0.0, 1.0);
        let entry = self.settings.entry(track_id).or_default();
        entry.noise_reduction_enabled = enabled;
        entry.noise_reduction_strength = clamped;
        if let Some(clips) = self.live_handles.get(&track_id) {
            for clip in clips {
                if let Some(ref nr) = clip.noise_reduction {
                    apply_noise_reduction(nr, enabled, clamped);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_voice_leveler(&mut self, track_id: TrackId, enabled: bool) -> Result<(), PipelineError> {
        self.finalize_after_commit();
        self.settings.entry(track_id).or_default().voice_leveler_enabled = enabled;
        if let Some(clips) = self.live_handles.get(&track_id) {
            for clip in clips {
                if let Some(ref lvl) = clip.voice_leveler {
                    apply_voice_leveler(lvl, enabled);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn toggle_track_audio_effect(
        &mut self,
        track_id: TrackId,
        effect: AudioEffectKind,
        enabled: bool,
    ) -> Result<(), PipelineError> {
        self.finalize_after_commit();
        self.settings.entry(track_id).or_default().set_effect(effect, enabled);
        if let Some(clips) = self.live_handles.get(&track_id) {
            for clip in clips {
                match effect {
                    AudioEffectKind::Echo => {
                        if let Some(ref echo) = clip.echo {
                            apply_echo(echo, enabled);
                        }
                    }
                    AudioEffectKind::Reverb => {
                        if let Some(ref reverb) = clip.reverb {
                            apply_reverb(reverb, enabled);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn any_track_solo(&self) -> bool {
        self.settings.values().any(|s| s.solo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discoverer::tests::{write_test_wav_1s_stereo_48khz, TempFileGuard};
    use gstreamer::prelude::*;
    use gstreamer_editing_services::prelude::*;
    use meraki_core::{ClipId, ClipTransform, TrackKind, VolumeKeyframe};

    #[test]
    fn effective_muted_obeys_solo_and_mute_rules() {
        let normal = TrackAudioSettings::default();
        let muted = TrackAudioSettings { muted: true, ..TrackAudioSettings::default() };
        let soloed = TrackAudioSettings { solo: true, ..TrackAudioSettings::default() };
        let solo_and_muted = TrackAudioSettings { solo: true, muted: true, ..TrackAudioSettings::default() };

        assert!(!effective_muted(&normal, false));
        assert!(effective_muted(&muted, false));
        assert!(effective_muted(&normal, true));
        assert!(!effective_muted(&soloed, true));
        assert!(effective_muted(&solo_and_muted, true));
    }

    #[test]
    fn live_dsp_handles_resolve_and_update_eq_and_solo_without_rebuilding_timeline() {
        crate::ensure_media_initialized().unwrap();
        let wav = TempFileGuard::new("dsp_live", "wav");
        write_test_wav_1s_stereo_48khz(&wav.0);
        let uri = crate::discoverer::path_to_file_uri(&wav.0).unwrap();

        let timeline = ges::Timeline::new();
        let audio_track = ges::AudioTrack::new();
        timeline.add_track(&audio_track).unwrap();
        let layer1 = timeline.append_layer();
        let layer2 = timeline.append_layer();

        let asset = ges::UriClipAsset::request_sync(&uri).unwrap();
        let track_a = TrackId::new();
        let track_b = TrackId::new();

        let clip_a = TimelineRenderClip {
            clip_id: ClipId::new(),
            track_id: track_a,
            path: wav.0.clone(),
            proxy_path: None,
            track_kind: TrackKind::Audio,
            timeline_start_ns: 0,
            source_in_ns: 0,
            duration_ns: 1_000_000_000,
            transform: ClipTransform::default(),
            title: None,
            effects: Vec::new(),
            transition_in: None,
            transition_out: None,
            keyframes: Vec::new(),
            volume_keyframes: vec![VolumeKeyframe::new(0, -6.0), VolumeKeyframe::new(1_000_000_000, 0.0)],
        };
        let clip_b = TimelineRenderClip { track_id: track_b, clip_id: ClipId::new(), ..clip_a.clone() };

        let ges_a = layer1
            .add_asset(&asset, gst::ClockTime::ZERO, gst::ClockTime::ZERO, gst::ClockTime::SECOND, ges::TrackType::AUDIO)
            .unwrap();
        let ges_b = layer2
            .add_asset(&asset, gst::ClockTime::ZERO, gst::ClockTime::ZERO, gst::ClockTime::SECOND, ges::TrackType::AUDIO)
            .unwrap();

        let mut dsp = AudioDspController::new();
        let mut meter = AudioMeterAggregator::new();
        dsp.prepare_sync(vec![
            TrackAudioMix { track_id: track_a, settings: TrackAudioSettings::default() },
            TrackAudioMix { track_id: track_b, settings: TrackAudioSettings::default() },
        ])
        .unwrap();
        dsp.attach_clip_audio(&ges_a, &clip_a, &mut meter);
        dsp.attach_clip_audio(&ges_b, &clip_b, &mut meter);
        assert!(timeline.commit());
        dsp.finalize_after_commit();

        assert_eq!(dsp.live_handles[&track_a].len(), 1);
        let eq = dsp.live_handles[&track_a][0].eq.clone().unwrap();
        assert!(dsp.live_handles[&track_a][0].volume.is_some());

        dsp.update_eq_band(track_a, EqBand::Low, 5.5).unwrap();
        dsp.update_eq_band(track_a, EqBand::High, -4.0).unwrap();
        assert!((eq.bands[0].property::<f64>("gain") - 5.5).abs() < 1e-4);
        assert!((eq.bands[3].property::<f64>("gain") - (-4.0)).abs() < 1e-4);

        dsp.set_track_solo(track_a, true).unwrap();
        assert!(!dsp.live_handles[&track_a][0].volume.as_ref().unwrap().element.property::<bool>("mute"));
        assert!(dsp.live_handles[&track_b][0].volume.as_ref().unwrap().element.property::<bool>("mute"));

        dsp.set_track_solo(track_a, false).unwrap();
        assert!(!dsp.live_handles[&track_b][0].volume.as_ref().unwrap().element.property::<bool>("mute"));
    }
}
