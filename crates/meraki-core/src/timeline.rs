pub use crate::audio::{AudioEffectKind, TrackAudioSettings, VoicePreset, VolumeKeyframe};
pub use crate::effects::{
    ClipEffect, ClipTransform, ClipTransition, Keyframe, KeyframeEasing, KeyframeProperty,
    TitleOverlay, TitleStyle, TransitionEdge, TransitionKind, VideoEffectKind,
};
pub use crate::transitions::{
    TimelineTransition, TransitionPlacement, DEFAULT_TRANSITION_DURATION_NS,
};
use crate::project::AssetId;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        impl $name { pub fn new() -> Self { Self(Uuid::new_v4()) } }
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl From<Uuid> for $name { fn from(id: Uuid) -> Self { Self(id) } }
        impl From<$name> for Uuid { fn from(id: $name) -> Self { id.0 } }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
        }
    };
}

define_id!(TrackId, "Identificador único de uma trilha na timeline.");
define_id!(ClipId, "Identificador único de um clipe posicionado na timeline.");

/// Categoria funcional de uma trilha da timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
    Overlay,
}

/// Segmento de um ativo ou título posicionado em uma trilha da timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub asset_id: AssetId,
    pub timeline_start_ns: u64,
    pub source_in_ns: u64,
    pub duration_ns: u64,
    #[serde(default)]
    pub media_duration_ns: u64,
    #[serde(default)]
    pub transform: ClipTransform,
    #[serde(default)]
    pub title: Option<TitleOverlay>,
    #[serde(default)]
    pub effects: Vec<ClipEffect>,
    #[serde(default)]
    pub transition_in: Option<ClipTransition>,
    #[serde(default)]
    pub transition_out: Option<ClipTransition>,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
    #[serde(default)]
    pub volume_keyframes: Vec<VolumeKeyframe>,
}

impl Clip {
    pub fn new(asset_id: AssetId, timeline_start_ns: u64, source_in_ns: u64, duration_ns: u64) -> Self {
        Self {
            id: ClipId::new(), asset_id, timeline_start_ns, source_in_ns, duration_ns, media_duration_ns: 0,
            transform: ClipTransform::default(), title: None, effects: Vec::new(),
            transition_in: None, transition_out: None, keyframes: Vec::new(), volume_keyframes: Vec::new(),
        }
    }

    /// Cria um clipe de título/texto sobreposto para a trilha `T1` (`Overlay`).
    pub fn new_title(timeline_start_ns: u64, duration_ns: u64, title: TitleOverlay) -> Self {
        Self {
            id: ClipId::new(), asset_id: AssetId(Uuid::nil()), timeline_start_ns, source_in_ns: 0, duration_ns, media_duration_ns: 0,
            transform: ClipTransform::default(), title: Some(title), effects: Vec::new(),
            transition_in: None, transition_out: None, keyframes: Vec::new(), volume_keyframes: Vec::new(),
        }
    }

    pub fn with_media_duration(mut self, media_duration_ns: u64) -> Self {
        self.media_duration_ns = media_duration_ns;
        self
    }

    pub fn source_bounds(&self) -> (u64, u64) { clip_source_bounds(self) }

    pub fn timeline_end_ns(&self) -> Result<u64, TimelineError> {
        self.timeline_start_ns.checked_add(self.duration_ns).ok_or(TimelineError::TimestampOverflow)
    }

    pub fn source_out_ns(&self) -> Result<u64, TimelineError> {
        self.source_in_ns.checked_add(self.duration_ns).ok_or(TimelineError::TimestampOverflow)
    }

    pub fn overlaps(&self, other: &Self) -> Result<bool, TimelineError> {
        let self_end = self.timeline_end_ns()?;
        let other_end = other.timeline_end_ns()?;
        Ok(self.timeline_start_ns < other_end && other.timeline_start_ns < self_end)
    }
}

/// Retorna a margem disponível `(antes_de_source_in_ns, depois_de_source_out_ns)` para re-extensão não destrutiva.
pub fn clip_source_bounds(clip: &Clip) -> (u64, u64) {
    let source_out = clip.source_in_ns.saturating_add(clip.duration_ns);
    (clip.source_in_ns, clip.media_duration_ns.saturating_sub(source_out))
}

/// Trilha da timeline contendo clipes ordenados sem sobreposição.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub kind: TrackKind,
    pub muted: bool,
    pub solo: bool,
    pub gain_db: f32,
    #[serde(default)]
    pub audio: TrackAudioSettings,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id: TrackId::new(), name: name.into(), kind,
            muted: false, solo: false, gain_db: 0.0, audio: TrackAudioSettings::default(), clips: Vec::new(),
        }
    }
}

/// Erros de validação pura para operações da timeline.
#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("trilha não encontrada: {0}")]
    TrackNotFound(TrackId),
    #[error("clipe não encontrado: {0}")]
    ClipNotFound(ClipId),
    #[error("clipe duplicado: {0}")]
    DuplicateClip(ClipId),
    #[error("duração do clipe deve ser maior que zero")]
    ZeroDuration,
    #[error("estouro aritmético ao calcular timestamp na timeline")]
    TimestampOverflow,
    #[error("sobreposição detectada na trilha {track_id} no intervalo [{start_ns}, {end_ns})")]
    ClipOverlap { track_id: TrackId, start_ns: u64, end_ns: u64 },
    #[error("corte do clipe {clip_id} excede duração da mídia ({source_end_ns}ns > {max_source_duration_ns}ns)")]
    SourceOutOfBounds { clip_id: ClipId, source_end_ns: u64, max_source_duration_ns: u64 },
    #[error("ponto de divisão ({split_at_ns}ns) fora dos limites do clipe {clip_id} ({start_ns}ns..{end_ns}ns)")]
    SplitPointOutOfBounds { clip_id: ClipId, split_at_ns: u64, start_ns: u64, end_ns: u64 },
    #[error("transformação inválida no campo '{field}': {reason}")]
    InvalidTransform { field: &'static str, reason: &'static str },
    #[error("trilha incompatível {track_id}: esperado {expected:?}, encontrado {actual:?}")]
    InvalidTrackKind { track_id: TrackId, expected: TrackKind, actual: TrackKind },
    #[error("clipe {0} não é um clipe de título")]
    NotATitleClip(ClipId),
    #[error("título inválido no campo '{field}': {reason}")]
    InvalidTitle { field: &'static str, reason: &'static str },
    #[error("efeito inválido no campo '{field}': {reason}")]
    InvalidEffect { field: &'static str, reason: &'static str },
    #[error("efeito {effect_id} não encontrado no clipe {clip_id}")]
    EffectNotFound { clip_id: ClipId, effect_id: Uuid },
    #[error("transição inválida: {reason}")]
    InvalidTransition { reason: &'static str },
    #[error("transição posicional não encontrada: {0}")]
    TransitionNotFound(Uuid),
    #[error("keyframe inválido no campo '{field}': {reason}")]
    InvalidKeyframe { field: &'static str, reason: &'static str },
    #[error("keyframe não encontrado em {clip_id} ({property:?} em {time_offset_ns}ns)")]
    KeyframeNotFound { clip_id: ClipId, property: KeyframeProperty, time_offset_ns: u64 },
    #[error("configuração de áudio inválida no campo '{field}': {reason}")]
    InvalidAudioSetting { field: &'static str, reason: &'static str },
    #[error("keyframe de volume não encontrado no clipe {clip_id} em {time_offset_ns}ns")]
    VolumeKeyframeNotFound { clip_id: ClipId, time_offset_ns: u64 },
}

/// Timeline multipista pura do Meraki.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Timeline {
    pub tracks: Vec<Track>,
    pub playhead_ns: u64,
    #[serde(default)]
    pub master_gain_db: f32,
    #[serde(default)]
    pub transitions: Vec<TimelineTransition>,
}

impl Timeline {
    pub fn new() -> Self { Self::default() }

    pub fn with_default_tracks() -> Self {
        Self { tracks: vec![Track::new("V1", TrackKind::Video), Track::new("A1", TrackKind::Audio)], playhead_ns: 0, master_gain_db: 0.0, transitions: Vec::new() }
    }

    pub fn add_track(&mut self, name: impl Into<String>, kind: TrackKind) -> TrackId {
        let track = Track::new(name, kind);
        let id = track.id;
        self.tracks.push(track);
        id
    }

    pub fn track(&self, track_id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == track_id)
    }
    pub fn track_mut(&mut self, track_id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == track_id)
    }
    pub fn clip(&self, clip_id: ClipId) -> Option<(TrackId, &Clip)> {
        self.tracks.iter().find_map(|t| t.clips.iter().find(|c| c.id == clip_id).map(|c| (t.id, c)))
    }
    pub(crate) fn clip_mut(&mut self, clip_id: ClipId) -> Option<&mut Clip> {
        self.tracks.iter_mut().find_map(|t| t.clips.iter_mut().find(|c| c.id == clip_id))
    }
    pub fn total_duration_ns(&self) -> u64 {
        self.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| c.timeline_end_ns().ok()).max().unwrap_or(0)
    }

    /// Adiciona um clipe à trilha informada validando duração, overflow e sobreposição.
    pub fn add_clip(&mut self, track_id: TrackId, clip: Clip) -> Result<ClipId, TimelineError> {
        validate_clip_timestamps(&clip)?;
        crate::timeline_effects::validate_clip_extras(&clip)?;
        if self.clip(clip.id).is_some() {
            return Err(TimelineError::DuplicateClip(clip.id));
        }
        let track = self.track_mut(track_id).ok_or(TimelineError::TrackNotFound(track_id))?;
        ensure_no_overlap(track, &clip, &[])?;
        let clip_id = clip.id;
        track.clips.push(clip);
        track.clips.sort_by_key(|item| item.timeline_start_ns);
        Ok(clip_id)
    }

    /// Remove um clipe existente retornando a trilha de origem e o clipe removido.
    pub fn remove_clip(&mut self, clip_id: ClipId) -> Result<(TrackId, Clip), TimelineError> {
        for idx_t in 0..self.tracks.len() {
            if let Some(idx) = self.tracks[idx_t].clips.iter().position(|clip| clip.id == clip_id) {
                let track_id = self.tracks[idx_t].id;
                let removed = self.tracks[idx_t].clips.remove(idx);
                let _ = self.cleanup_orphaned_transitions();
                return Ok((track_id, removed));
            }
        }
        Err(TimelineError::ClipNotFound(clip_id))
    }

    /// Move um clipe para outra posição e/ou outra trilha sem corromper o estado em caso de erro.
    pub fn move_clip(&mut self, clip_id: ClipId, target_track_id: TrackId, new_start_ns: u64) -> Result<(), TimelineError> {
        let (source_track_id, existing) = self
            .clip(clip_id)
            .map(|(tid, clip)| (tid, clip.clone()))
            .ok_or(TimelineError::ClipNotFound(clip_id))?;
        let moved = Clip { timeline_start_ns: new_start_ns, ..existing };
        validate_clip_timestamps(&moved)?;
        let target_track = self.track(target_track_id).ok_or(TimelineError::TrackNotFound(target_track_id))?;
        ensure_no_overlap(target_track, &moved, &[clip_id])?;

        if source_track_id == target_track_id {
            if let Some(track) = self.track_mut(target_track_id) {
                if let Some(slot) = track.clips.iter_mut().find(|item| item.id == clip_id) {
                    *slot = moved;
                }
                track.clips.sort_by_key(|item| item.timeline_start_ns);
            }
        } else {
            let _ = self.remove_clip(clip_id)?;
            if let Some(track) = self.track_mut(target_track_id) {
                track.clips.push(moved);
                track.clips.sort_by_key(|item| item.timeline_start_ns);
            }
        }
        let _ = self.cleanup_orphaned_transitions();
        Ok(())
    }

    /// Ajusta entrada, saída e duração de um clipe validando limites da mídia e sobreposições (suporta re-extensão até `media_duration_ns`).
    pub fn trim_clip(
        &mut self,
        clip_id: ClipId,
        new_timeline_start_ns: u64,
        new_source_in_ns: u64,
        new_duration_ns: u64,
        max_source_duration_ns: Option<u64>,
    ) -> Result<(), TimelineError> {
        let (track_id, existing) = self
            .clip(clip_id)
            .map(|(tid, clip)| (tid, clip.clone()))
            .ok_or(TimelineError::ClipNotFound(clip_id))?;
        let effective_max = max_source_duration_ns.or_else(|| (existing.media_duration_ns > 0).then_some(existing.media_duration_ns));
        let trimmed = Clip {
            timeline_start_ns: new_timeline_start_ns,
            source_in_ns: new_source_in_ns,
            duration_ns: new_duration_ns,
            media_duration_ns: existing.media_duration_ns,
            ..existing
        };
        validate_clip_timestamps(&trimmed)?;
        let source_end_ns = trimmed.source_out_ns()?;
        if let Some(max_ns) = effective_max {
            if source_end_ns > max_ns {
                return Err(TimelineError::SourceOutOfBounds { clip_id, source_end_ns, max_source_duration_ns: max_ns });
            }
        }
        let track = self.track_mut(track_id).ok_or(TimelineError::TrackNotFound(track_id))?;
        ensure_no_overlap(track, &trimmed, &[clip_id])?;
        if let Some(slot) = track.clips.iter_mut().find(|item| item.id == clip_id) {
            *slot = trimmed;
        }
        track.clips.sort_by_key(|item| item.timeline_start_ns);
        let _ = self.cleanup_orphaned_transitions();
        Ok(())
    }

    /// Divide um clipe ao meio no instante `split_at_ns`, retornando `(clipe_esquerdo, clipe_direito)`.
    pub fn split_clip(&mut self, clip_id: ClipId, split_at_ns: u64) -> Result<(ClipId, ClipId), TimelineError> {
        self.split_clip_with_right_id(clip_id, split_at_ns, ClipId::new())
    }

    /// Divide um clipe usando um `ClipId` explícito para a metade direita (para Undo/Redo determinístico).
    pub fn split_clip_with_right_id(
        &mut self,
        clip_id: ClipId,
        split_at_ns: u64,
        right_id: ClipId,
    ) -> Result<(ClipId, ClipId), TimelineError> {
        if self.clip(right_id).is_some() {
            return Err(TimelineError::DuplicateClip(right_id));
        }
        let (track_id, original) = self
            .clip(clip_id)
            .map(|(tid, clip)| (tid, clip.clone()))
            .ok_or(TimelineError::ClipNotFound(clip_id))?;
        let (left_clip, right_clip) = crate::timeline_effects::build_split_pair(original, split_at_ns, right_id)?;

        let track = self.track_mut(track_id).ok_or(TimelineError::TrackNotFound(track_id))?;
        if let Some(slot) = track.clips.iter_mut().find(|item| item.id == clip_id) {
            *slot = left_clip;
        }
        track.clips.push(right_clip);
        track.clips.sort_by_key(|item| item.timeline_start_ns);
        let _ = self.cleanup_orphaned_transitions();
        Ok((clip_id, right_id))
    }

    /// Atualiza as propriedades de transformação de um clipe retornando o valor anterior.
    pub fn update_clip_transform(&mut self, clip_id: ClipId, transform: ClipTransform) -> Result<ClipTransform, TimelineError> {
        transform.validate()?;
        let clip = self.clip_mut(clip_id).ok_or(TimelineError::ClipNotFound(clip_id))?;
        Ok(std::mem::replace(&mut clip.transform, transform))
    }

    /// Reverte uma divisão restaurando o clipe original na mesma trilha.
    pub fn unsplit_clip(&mut self, left_id: ClipId, right_id: ClipId, original: Clip) -> Result<(), TimelineError> {
        let (left_track_id, _) = self.clip(left_id).ok_or(TimelineError::ClipNotFound(left_id))?;
        let (right_track_id, _) = self.clip(right_id).ok_or(TimelineError::ClipNotFound(right_id))?;
        if left_track_id != right_track_id { return Err(TimelineError::ClipNotFound(right_id)); }
        validate_clip_timestamps(&original)?;
        crate::timeline_effects::validate_clip_extras(&original)?;
        let track = self.track_mut(left_track_id).ok_or(TimelineError::TrackNotFound(left_track_id))?;
        ensure_no_overlap(track, &original, &[left_id, right_id])?;
        track.clips.retain(|item| item.id != left_id && item.id != right_id);
        track.clips.push(original);
        track.clips.sort_by_key(|item| item.timeline_start_ns);
        Ok(())
    }
}

pub(crate) fn validate_clip_timestamps(clip: &Clip) -> Result<(), TimelineError> {
    if clip.duration_ns == 0 { return Err(TimelineError::ZeroDuration); }
    clip.transform.validate()?;
    let _ = clip.timeline_end_ns()?;
    let _ = clip.source_out_ns()?;
    Ok(())
}

pub(crate) fn ensure_no_overlap(track: &Track, candidate: &Clip, ignore_ids: &[ClipId]) -> Result<(), TimelineError> {
    let cand_end = candidate.timeline_end_ns()?;
    for existing in &track.clips {
        if !ignore_ids.contains(&existing.id) && existing.overlaps(candidate)? {
            return Err(TimelineError::ClipOverlap { track_id: track.id, start_ns: candidate.timeline_start_ns, end_ns: cand_end });
        }
    }
    Ok(())
}
