use super::*;
use crate::protocol::AudioEnhancements;

impl AudioDspController {
    pub(crate) fn set_track_volume(
        &mut self,
        id: TrackId,
        gain_db: f64,
        muted: bool,
    ) -> Result<(), PipelineError> {
        let gain = checked_db(
            gain_db,
            "track_volume_db",
            MIN_TRACK_GAIN_DB,
            MAX_TRACK_GAIN_DB,
        )?;
        let any_solo = self.any_track_solo();
        let settings = self.settings.entry(id).or_default();
        settings.gain_db = gain;
        settings.muted = muted;
        let effective_mute = effective_muted(settings, any_solo);
        if let Some(clips) = self.live_handles.get(&id) {
            for fader in clips.iter().filter_map(|clip| clip.volume.as_ref()) {
                fader.set(db_to_linear(gain), effective_mute);
            }
        }
        Ok(())
    }

    pub(crate) fn set_track_eq(
        &mut self,
        id: TrackId,
        gains: [f64; 4],
    ) -> Result<(), PipelineError> {
        let mut validated = [0.0; 4];
        for (slot, gain) in validated.iter_mut().zip(gains) {
            *slot = checked_db(gain, "eq_gain_db", MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB)?;
        }
        let entry = self.settings.entry(id).or_default();
        let previous = std::mem::replace(&mut entry.eq_gains_db, validated);
        if let Some(clips) = self.live_handles.get(&id) {
            for eq in clips.iter().filter_map(|clip| clip.eq.as_ref()) {
                for (index, gain) in validated.into_iter().enumerate() {
                    if previous[index] != gain {
                        eq.set_gain(index, gain);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_track_enhancements(
        &mut self,
        id: TrackId,
        config: AudioEnhancements,
    ) -> Result<(), PipelineError> {
        self.set_noise_reduction(
            id,
            config.noise_reduction_enabled,
            config.noise_reduction_strength,
        )?;
        self.set_voice_leveler(id, config.voice_leveler_enabled)?;
        self.toggle_track_audio_effect(id, AudioEffectKind::Reverb, config.reverb_enabled)?;
        self.toggle_track_audio_effect(id, AudioEffectKind::Echo, config.echo_enabled)
    }
}

pub(crate) fn checked_db(
    gain: f64,
    name: &'static str,
    min: f32,
    max: f32,
) -> Result<f32, PipelineError> {
    if !gain.is_finite() {
        return Err(PipelineError::InvalidParameter {
            name,
            reason: "ganho em dB deve ser finito",
        });
    }
    Ok(gain.clamp(f64::from(min), f64::from(max)) as f32)
}
