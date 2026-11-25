use crate::audio::{AudioEffectKind, TrackAudioSettings, VoicePreset, VolumeKeyframe};
use crate::timeline::{Clip, ClipId, Timeline, TimelineError, TrackId, TrackKind};

const MIXER_TRACK_NAMES: [&str; 3] = ["A1 Voz", "A2 Música", "A3 SFX"];

pub(crate) fn validate_volume_keyframes(clip: &Clip) -> Result<(), TimelineError> {
    for kf in &clip.volume_keyframes {
        kf.validate(clip.duration_ns)?;
    }
    Ok(())
}

/// Particiona `volume_keyframes` em `(esquerdo, direito)` ao dividir um clipe em `split_offset_ns`.
pub(crate) fn split_volume_keyframes(
    keyframes: &[VolumeKeyframe],
    split_offset_ns: u64,
) -> (Vec<VolumeKeyframe>, Vec<VolumeKeyframe>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for &kf in keyframes {
        if kf.time_offset_ns <= split_offset_ns {
            left.push(kf);
        }
        if kf.time_offset_ns >= split_offset_ns {
            right.push(VolumeKeyframe {
                time_offset_ns: kf.time_offset_ns - split_offset_ns,
                ..kf
            });
        }
    }
    (left, right)
}

impl Timeline {
    /// Garante de forma idempotente as trilhas reais usadas pelo Mixer (`A1`/`A1 Voz`, `A2 Música`, `A3 SFX`).
    pub fn ensure_mixer_audio_tracks(&mut self) -> Vec<TrackId> {
        let current_audio_count =
            self.tracks.iter().filter(|t| t.kind == TrackKind::Audio).count();
        for name in MIXER_TRACK_NAMES.iter().skip(current_audio_count) {
            let _ = self.add_track(*name, TrackKind::Audio);
        }
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect()
    }

    /// Atualiza toda a configuração de áudio da trilha mantendo campos legados sincronizados.
    pub fn update_track_audio(
        &mut self,
        track_id: TrackId,
        settings: TrackAudioSettings,
    ) -> Result<TrackAudioSettings, TimelineError> {
        settings.validate()?;
        let track = self.track_mut(track_id).ok_or(TimelineError::TrackNotFound(track_id))?;
        let previous = track.audio.clone();
        track.muted = settings.muted;
        track.solo = settings.solo;
        track.gain_db = settings.gain_db;
        track.audio = settings;
        Ok(previous)
    }

    /// Aplica um `VoicePreset` sobre a trilha retornando as configurações anteriores.
    pub fn apply_voice_preset(
        &mut self,
        track_id: TrackId,
        preset: VoicePreset,
    ) -> Result<TrackAudioSettings, TimelineError> {
        let current = self
            .track(track_id)
            .ok_or(TimelineError::TrackNotFound(track_id))?
            .audio
            .clone();
        let mut next = current;
        preset.apply_to(&mut next);
        self.update_track_audio(track_id, next)
    }

    /// Habilita ou desabilita um efeito de áudio (`Reverb`/`Echo`) na trilha.
    /// Retorna `(configurações_anteriores, estado_atual_habilitado)`.
    pub fn toggle_track_audio_effect(
        &mut self,
        track_id: TrackId,
        effect: AudioEffectKind,
    ) -> Result<(TrackAudioSettings, bool), TimelineError> {
        let mut next = self
            .track(track_id)
            .ok_or(TimelineError::TrackNotFound(track_id))?
            .audio
            .clone();
        let enable = !next.has_effect(effect);
        next.set_effect(effect, enable);
        let prev = self.update_track_audio(track_id, next)?;
        Ok((prev, enable))
    }

    /// Insere ou atualiza um keyframe de automação de volume mantendo ordenação por `time_offset_ns`.
    pub fn upsert_volume_keyframe(
        &mut self,
        clip_id: ClipId,
        keyframe: VolumeKeyframe,
    ) -> Result<Option<VolumeKeyframe>, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        keyframe.validate(clip.duration_ns)?;
        let prev = if let Some(slot) =
            clip.volume_keyframes.iter_mut().find(|k| k.time_offset_ns == keyframe.time_offset_ns)
        {
            Some(std::mem::replace(slot, keyframe))
        } else {
            clip.volume_keyframes.push(keyframe);
            None
        };
        clip.volume_keyframes.sort_by_key(|k| k.time_offset_ns);
        Ok(prev)
    }

    /// Remove um keyframe de automação de volume exato em `time_offset_ns`.
    pub fn remove_volume_keyframe(
        &mut self,
        clip_id: ClipId,
        time_offset_ns: u64,
    ) -> Result<VolumeKeyframe, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        let Some(idx) =
            clip.volume_keyframes.iter().position(|k| k.time_offset_ns == time_offset_ns)
        else {
            return Err(TimelineError::VolumeKeyframeNotFound { clip_id, time_offset_ns });
        };
        Ok(clip.volume_keyframes.remove(idx))
    }
}
