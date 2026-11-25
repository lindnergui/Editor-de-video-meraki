use crate::effects::{
    ClipEffect, ClipTransition, Keyframe, KeyframeProperty, TitleOverlay, TransitionEdge,
    VideoEffectKind,
};
use crate::timeline::{
    ensure_no_overlap, validate_clip_timestamps, Clip, ClipId, Timeline, TimelineError, TrackId,
    TrackKind,
};

/// Valida título, efeitos, transições e keyframes de um clipe.
pub(crate) fn validate_clip_extras(clip: &Clip) -> Result<(), TimelineError> {
    if let Some(ref title) = clip.title {
        title.validate()?;
    }
    for effect in &clip.effects {
        effect.validate()?;
    }
    if let Some(tr_in) = clip.transition_in {
        tr_in.validate(clip.duration_ns)?;
    }
    if let Some(tr_out) = clip.transition_out {
        tr_out.validate(clip.duration_ns)?;
    }
    for kf in &clip.keyframes {
        kf.validate(clip.duration_ns)?;
    }
    crate::timeline_audio::validate_volume_keyframes(clip)?;
    Ok(())
}

/// Constrói o par `(clipe_esquerdo, clipe_direito)` particionando transições, efeitos e keyframes.
pub(crate) fn build_split_pair(
    original: Clip,
    split_at_ns: u64,
    right_id: ClipId,
) -> Result<(Clip, Clip), TimelineError> {
    let start_ns = original.timeline_start_ns;
    let end_ns = original.timeline_end_ns()?;
    if split_at_ns <= start_ns || split_at_ns >= end_ns {
        return Err(TimelineError::SplitPointOutOfBounds {
            clip_id: original.id,
            split_at_ns,
            start_ns,
            end_ns,
        });
    }

    let left_duration = split_at_ns - start_ns;
    let right_duration = end_ns - split_at_ns;
    let right_source_in =
        original.source_in_ns.checked_add(left_duration).ok_or(TimelineError::TimestampOverflow)?;

    let left_in = original
        .transition_in
        .map(|tr| ClipTransition { duration_ns: tr.duration_ns.min(left_duration), ..tr });
    let right_out = original
        .transition_out
        .map(|tr| ClipTransition { duration_ns: tr.duration_ns.min(right_duration), ..tr });

    let mut left_kf = Vec::new();
    let mut right_kf = Vec::new();
    for &kf in &original.keyframes {
        if kf.time_offset_ns <= left_duration {
            left_kf.push(kf);
        }
        if kf.time_offset_ns >= left_duration {
            right_kf.push(Keyframe { time_offset_ns: kf.time_offset_ns - left_duration, ..kf });
        }
    }

    let (left_vol_kf, right_vol_kf) =
        crate::timeline_audio::split_volume_keyframes(&original.volume_keyframes, left_duration);

    let right_clip = Clip {
        id: right_id,
        asset_id: original.asset_id,
        timeline_start_ns: split_at_ns,
        source_in_ns: right_source_in,
        duration_ns: right_duration,
        transform: original.transform,
        title: original.title.clone(),
        effects: original.effects.clone(),
        transition_in: None,
        transition_out: right_out,
        keyframes: right_kf,
        volume_keyframes: right_vol_kf,
    };
    let left_clip = Clip {
        duration_ns: left_duration,
        transition_in: left_in,
        transition_out: None,
        keyframes: left_kf,
        volume_keyframes: left_vol_kf,
        ..original
    };
    Ok((left_clip, right_clip))
}

impl Timeline {
    /// Remove uma trilha vazia (usado pelo Undo ao desfazer criação automática de `T1`).
    pub(crate) fn remove_empty_track(&mut self, track_id: TrackId) -> bool {
        let Some(idx) = self.tracks.iter().position(|t| t.id == track_id && t.clips.is_empty())
        else {
            return false;
        };
        self.tracks.remove(idx);
        true
    }

    /// Retorna a trilha de sobreposição `T1` existente ou cria uma nova automaticamente.
    pub fn ensure_overlay_track(&mut self) -> TrackId {
        if let Some(existing) = self.tracks.iter().find(|t| t.kind == TrackKind::Overlay) {
            return existing.id;
        }
        self.add_track("T1", TrackKind::Overlay)
    }

    /// Insere um clipe de título em uma trilha `Overlay` (criando `T1` quando `track_id` for `None`).
    pub fn add_title_clip(
        &mut self,
        track_id: Option<TrackId>,
        clip: Clip,
    ) -> Result<(TrackId, ClipId), TimelineError> {
        if clip.title.is_none() {
            return Err(TimelineError::NotATitleClip(clip.id));
        }
        validate_clip_timestamps(&clip)?;
        validate_clip_extras(&clip)?;
        if self.clip(clip.id).is_some() {
            return Err(TimelineError::DuplicateClip(clip.id));
        }

        let resolved_track_id = match track_id {
            Some(tid) => {
                let track = self.track(tid).ok_or(TimelineError::TrackNotFound(tid))?;
                if track.kind != TrackKind::Overlay {
                    return Err(TimelineError::InvalidTrackKind {
                        track_id: tid,
                        expected: TrackKind::Overlay,
                        actual: track.kind,
                    });
                }
                ensure_no_overlap(track, &clip, &[])?;
                tid
            }
            None => {
                if let Some(existing) = self.tracks.iter().find(|t| t.kind == TrackKind::Overlay) {
                    ensure_no_overlap(existing, &clip, &[])?;
                    existing.id
                } else {
                    self.add_track("T1", TrackKind::Overlay)
                }
            }
        };

        let clip_id = self.add_clip(resolved_track_id, clip)?;
        Ok((resolved_track_id, clip_id))
    }

    /// Atualiza texto, estilo, fonte ou cor de um clipe de título retornando o valor anterior.
    pub fn update_title(
        &mut self,
        clip_id: ClipId,
        title: TitleOverlay,
    ) -> Result<TitleOverlay, TimelineError> {
        title.validate()?;
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        let slot = clip.title.as_mut().ok_or(TimelineError::NotATitleClip(clip_id))?;
        Ok(std::mem::replace(slot, title))
    }

    /// Alterna um efeito de vídeo no clipe (ou cria habilitado se ainda não existir).
    pub fn toggle_clip_effect(
        &mut self,
        clip_id: ClipId,
        kind: VideoEffectKind,
        intensity: f32,
    ) -> Result<(Option<ClipEffect>, ClipEffect), TimelineError> {
        ClipEffect::new(kind, intensity).validate()?;
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        if let Some(existing) = clip.effects.iter_mut().find(|e| e.kind == kind) {
            let prev = *existing;
            existing.enabled = !existing.enabled;
            existing.intensity = intensity;
            Ok((Some(prev), *existing))
        } else {
            let added = ClipEffect::new(kind, intensity);
            clip.effects.push(added);
            Ok((None, added))
        }
    }

    /// Define ou atualiza exatamente um efeito de vídeo no clipe (usado pelo Undo/Redo).
    pub fn set_clip_effect(
        &mut self,
        clip_id: ClipId,
        effect: ClipEffect,
    ) -> Result<Option<ClipEffect>, TimelineError> {
        effect.validate()?;
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        if let Some(existing) = clip.effects.iter_mut().find(|e| e.kind == effect.kind) {
            let prev = *existing;
            *existing = effect;
            Ok(Some(prev))
        } else {
            clip.effects.push(effect);
            Ok(None)
        }
    }

    /// Remove o efeito do tipo informado de um clipe, retornando-o caso existisse.
    pub fn remove_clip_effect(
        &mut self,
        clip_id: ClipId,
        kind: VideoEffectKind,
    ) -> Result<Option<ClipEffect>, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        if let Some(idx) = clip.effects.iter().position(|e| e.kind == kind) {
            return Ok(Some(clip.effects.remove(idx)));
        }
        Ok(None)
    }

    /// Configura ou remove a transição de entrada/saída do clipe retornando a anterior.
    pub fn set_clip_transition(
        &mut self,
        clip_id: ClipId,
        edge: TransitionEdge,
        transition: Option<ClipTransition>,
    ) -> Result<Option<ClipTransition>, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        if let Some(tr) = transition {
            tr.validate(clip.duration_ns)?;
        }
        let slot = match edge {
            TransitionEdge::In => &mut clip.transition_in,
            TransitionEdge::Out => &mut clip.transition_out,
        };
        Ok(std::mem::replace(slot, transition))
    }

    /// Insere ou atualiza um keyframe na chave `(property, time_offset_ns)` mantendo ordenação.
    pub fn upsert_keyframe(
        &mut self,
        clip_id: ClipId,
        keyframe: Keyframe,
    ) -> Result<Option<Keyframe>, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        keyframe.validate(clip.duration_ns)?;
        let prev = if let Some(slot) = clip
            .keyframes
            .iter_mut()
            .find(|k| k.property == keyframe.property && k.time_offset_ns == keyframe.time_offset_ns)
        {
            Some(std::mem::replace(slot, keyframe))
        } else {
            clip.keyframes.push(keyframe);
            None
        };
        clip.keyframes.sort_by_key(|k| (k.time_offset_ns, k.property.sort_rank()));
        Ok(prev)
    }

    /// Remove um keyframe exato por propriedade e deslocamento temporal.
    pub fn remove_keyframe(
        &mut self,
        clip_id: ClipId,
        property: KeyframeProperty,
        time_offset_ns: u64,
    ) -> Result<Keyframe, TimelineError> {
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        let Some(idx) = clip
            .keyframes
            .iter()
            .position(|k| k.property == property && k.time_offset_ns == time_offset_ns)
        else {
            return Err(TimelineError::KeyframeNotFound { clip_id, property, time_offset_ns });
        };
        Ok(clip.keyframes.remove(idx))
    }
}
