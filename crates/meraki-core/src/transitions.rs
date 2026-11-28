use crate::timeline::{ClipId, Timeline, TimelineError, TrackId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Duração padrão aplicada a novas transições posicionais (`0.5s`).
pub const DEFAULT_TRANSITION_DURATION_NS: u64 = 500_000_000;

/// Tipo de transição de entrada, saída ou junção suportado pelo editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionKind {
    #[serde(alias = "Fade")]
    Crossfade,
    Blur,
    Zoom,
}

impl TransitionKind {
    /// Alias de compatibilidade com a v0.1–v0.3 para `Crossfade`.
    #[allow(non_upper_case_globals)]
    pub const Fade: Self = Self::Crossfade;
}

/// Ponto exato na timeline onde uma transição posicional atua.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionPlacement {
    BetweenClips { left_clip_id: Uuid, right_clip_id: Uuid },
    ClipStart { clip_id: Uuid },
    ClipEnd { clip_id: Uuid },
}

/// Transição posicional independente aplicada em borda ou corte entre dois clipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelineTransition {
    pub id: Uuid,
    pub track_id: Uuid,
    pub kind: TransitionKind,
    pub placement: TransitionPlacement,
    pub duration_ns: u64,
}

impl TimelineTransition {
    pub fn new(
        track_id: impl Into<TrackId>,
        kind: TransitionKind,
        placement: TransitionPlacement,
        duration_ns: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            track_id: track_id.into().0,
            kind,
            placement,
            duration_ns,
        }
    }

    pub fn with_default_duration(
        track_id: impl Into<TrackId>,
        kind: TransitionKind,
        placement: TransitionPlacement,
    ) -> Self {
        Self::new(track_id, kind, placement, DEFAULT_TRANSITION_DURATION_NS)
    }
}

impl Timeline {
    /// Busca uma transição posicional pelo seu identificador.
    pub fn transition(&self, id: Uuid) -> Option<&TimelineTransition> {
        self.transitions.iter().find(|tr| tr.id == id)
    }

    /// Detecta dois clipes contíguos na trilha cujo corte esteja dentro de `tolerance_ns` de `playhead_ns`.
    pub fn find_adjacent_pair_near(
        &self,
        track_id: impl Into<TrackId>,
        playhead_ns: u64,
        tolerance_ns: u64,
    ) -> Option<(Uuid, Uuid, u64)> {
        let track = self.track(track_id.into())?;
        let mut best: Option<(u64, Uuid, Uuid, u64)> = None;
        for pair in track.clips.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            let Ok(left_end) = left.timeline_end_ns() else {
                continue;
            };
            if left_end != right.timeline_start_ns {
                continue;
            }
            let dist = left_end.abs_diff(playhead_ns);
            if dist <= tolerance_ns && best.is_none_or(|(best_dist, ..)| dist < best_dist) {
                best = Some((dist, left.id.0, right.id.0, left_end));
            }
        }
        best.map(|(_, l, r, seam)| (l, r, seam))
    }

    /// Insere ou substitui uma transição no mesmo `placement`, limitando sua duração à metade dos clipes envolvidos.
    pub fn upsert_transition(
        &mut self,
        mut transition: TimelineTransition,
    ) -> Result<Option<TimelineTransition>, TimelineError> {
        if transition.duration_ns == 0 {
            return Err(TimelineError::InvalidTransition {
                reason: "duração da transição deve ser maior que zero",
            });
        }
        let max_dur = self.validate_placement_max_duration(transition.track_id, transition.placement)?;
        transition.duration_ns = transition.duration_ns.min(max_dur);

        let replaced = if let Some(idx) = self.transitions.iter().position(|existing| {
            existing.id == transition.id
                || (existing.track_id == transition.track_id && existing.placement == transition.placement)
        }) {
            Some(std::mem::replace(&mut self.transitions[idx], transition))
        } else {
            self.transitions.push(transition);
            None
        };
        Ok(replaced)
    }

    /// Atualiza a duração de uma transição existente respeitando os limites dos clipes envolvidos.
    pub fn update_transition_duration(
        &mut self,
        transition_id: Uuid,
        new_duration_ns: u64,
    ) -> Result<u64, TimelineError> {
        if new_duration_ns == 0 {
            return Err(TimelineError::InvalidTransition {
                reason: "duração da transição deve ser maior que zero",
            });
        }
        let current = self
            .transition(transition_id)
            .copied()
            .ok_or(TimelineError::TransitionNotFound(transition_id))?;
        let max_dur = self.validate_placement_max_duration(current.track_id, current.placement)?;
        let clamped = new_duration_ns.min(max_dur);
        let Some(slot) = self.transitions.iter_mut().find(|tr| tr.id == transition_id) else {
            return Err(TimelineError::TransitionNotFound(transition_id));
        };
        let prev = slot.duration_ns;
        slot.duration_ns = clamped;
        Ok(prev)
    }

    /// Remove uma transição posicional pelo seu `id`.
    pub fn remove_transition(&mut self, transition_id: Uuid) -> Result<TimelineTransition, TimelineError> {
        let idx = self
            .transitions
            .iter()
            .position(|tr| tr.id == transition_id)
            .ok_or(TimelineError::TransitionNotFound(transition_id))?;
        Ok(self.transitions.remove(idx))
    }

    /// Remove transições órfãs (clipes removidos/separados ou duração zerada) e reajusta limites restantes.
    pub fn cleanup_orphaned_transitions(&mut self) -> Vec<TimelineTransition> {
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(self.transitions.len());
        let existing = std::mem::take(&mut self.transitions);
        for mut tr in existing {
            match self.validate_placement_max_duration(tr.track_id, tr.placement) {
                Ok(max_dur) => {
                    tr.duration_ns = tr.duration_ns.min(max_dur).max(1);
                    kept.push(tr);
                }
                Err(_) => removed.push(tr),
            }
        }
        self.transitions = kept;
        removed
    }

    fn validate_placement_max_duration(
        &self,
        track_id: Uuid,
        placement: TransitionPlacement,
    ) -> Result<u64, TimelineError> {
        let track = self.track(TrackId(track_id)).ok_or(TimelineError::TrackNotFound(TrackId(track_id)))?;
        let find_clip = |id: Uuid| {
            track
                .clips
                .iter()
                .find(|c| c.id == ClipId(id))
                .ok_or(TimelineError::ClipNotFound(ClipId(id)))
        };
        let max_dur = match placement {
            TransitionPlacement::BetweenClips { left_clip_id, right_clip_id } => {
                if left_clip_id == right_clip_id {
                    return Err(TimelineError::InvalidTransition {
                        reason: "transição entre clipes exige dois clipes distintos",
                    });
                }
                let left = find_clip(left_clip_id)?;
                let right = find_clip(right_clip_id)?;
                let left_end = left.timeline_end_ns()?;
                if left_end != right.timeline_start_ns {
                    return Err(TimelineError::InvalidTransition {
                        reason: "os dois clipes precisam estar contíguos na mesma trilha",
                    });
                }
                (left.duration_ns / 2).min(right.duration_ns / 2)
            }
            TransitionPlacement::ClipStart { clip_id } | TransitionPlacement::ClipEnd { clip_id } => {
                find_clip(clip_id)?.duration_ns / 2
            }
        };
        if max_dur == 0 {
            return Err(TimelineError::InvalidTransition {
                reason: "duração do clipe insuficiente para comportar a transição",
            });
        }
        Ok(max_dur)
    }
}
