use crate::app::MerakiApp;
use crate::components::player::{self, PlayerMessage};
use crate::components::timeline_math::TimelineTool;
use crate::playback::{self, PlaybackMessage};
use meraki_core::{ClipId, TrackKind};

/// Mensagens de edição precisa na timeline e navegação quadro a quadro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecisionMessage {
    SplitAtPlayhead,
    SelectTool,
    ToggleSnap,
    StepFrames(i64),
    StepSeconds(i64),
}

const TRACK_SPLIT_PRIORITY: [TrackKind; 3] = [
    TrackKind::Video,
    TrackKind::Overlay,
    TrackKind::Audio,
];

fn clip_strictly_contains(clip_start: u64, clip_end: Option<u64>, target_ns: u64) -> bool {
    target_ns > clip_start && clip_end.is_some_and(|end| target_ns < end)
}

impl MerakiApp {
    /// Taxa de quadros ativa do projeto (`24`, `30` ou `60`; padrão `30` fps).
    pub fn project_fps(&self) -> u64 {
        u64::from(self.delivery.settings.fps.as_u32()).max(1)
    }

    /// Corta exatamente em `self.playback.playhead_ns` o clipe selecionado ou o primeiro clipe
    /// que cruze a agulha na prioridade `Video -> Overlay -> Audio`.
    /// Não faz nada (sem erro fatal) quando nenhum clipe cruza estritamente a agulha.
    pub fn split_at_playhead(&mut self) -> Option<ClipId> {
        let split_at_ns = self.playback.playhead_ns;

        let selected_target = self.playback.selected_clip_id.and_then(|clip_id| {
            let (_, clip) = self.project.timeline.clip(clip_id)?;
            clip_strictly_contains(
                clip.timeline_start_ns,
                clip.timeline_end_ns().ok(),
                split_at_ns,
            )
            .then_some(clip_id)
        });

        let target_clip_id = selected_target.or_else(|| {
            TRACK_SPLIT_PRIORITY.into_iter().find_map(|kind| {
                self.project
                    .timeline
                    .tracks
                    .iter()
                    .filter(|track| track.kind == kind)
                    .flat_map(|track| track.clips.iter())
                    .find(|clip| {
                        clip_strictly_contains(
                            clip.timeline_start_ns,
                            clip.timeline_end_ns().ok(),
                            split_at_ns,
                        )
                    })
                    .map(|clip| clip.id)
            })
        })?;

        match self.split_clip(target_clip_id, split_at_ns) {
            Ok(right_id) => {
                self.playback.selected_clip_id = Some(right_id);
                Some(right_id)
            }
            Err(_) => None,
        }
    }

    /// Desloca a agulha exatamente `delta_frames` quadros dentro de `[0, duration_ns]`
    /// e solicita `request_seek` imediato ao motor.
    pub fn step_playhead_frames(&mut self, delta_frames: i64) -> u64 {
        let fps = self.project_fps();
        let frame_ns = 1_000_000_000_u64 / fps;
        let max_ns = self.project.timeline.total_duration_ns() as i128;
        let delta_ns = (delta_frames as i128).saturating_mul(frame_ns as i128);
        let candidate = (self.playback.playhead_ns as i128).saturating_add(delta_ns);
        let new_ns = candidate.clamp(0, max_ns) as u64;

        self.playback.playhead_ns = new_ns;
        self.project.timeline.playhead_ns = new_ns;
        self.engine.request_seek(new_ns);
        new_ns
    }

    pub(crate) fn handle_precision_message(&mut self, message: PrecisionMessage) {
        match message {
            PrecisionMessage::SplitAtPlayhead => {
                let _ = self.split_at_playhead();
            }
            PrecisionMessage::SelectTool => {
                self.edit.timeline.tool = TimelineTool::Select;
                self.effects.timeline.tool = TimelineTool::Select;
            }
            PrecisionMessage::ToggleSnap => {
                let next = !self.edit.timeline.snap_enabled;
                self.edit.timeline.snap_enabled = next;
                self.effects.timeline.snap_enabled = next;
            }
            PrecisionMessage::StepFrames(delta_frames) => {
                let _ = self.step_playhead_frames(delta_frames);
            }
            PrecisionMessage::StepSeconds(delta_secs) => {
                let frames = delta_secs.saturating_mul(self.project_fps() as i64);
                let _ = self.step_playhead_frames(frames);
            }
        }
    }

    pub(crate) fn handle_player_message(&mut self, msg: PlayerMessage) {
        match msg {
            PlayerMessage::StepBack => {
                let _ = self.step_playhead_frames(-1);
            }
            PlayerMessage::StepForward => {
                let _ = self.step_playhead_frames(1);
            }
            other => {
                let pb = player::into_playback_message(other, &self.project, self.playback.playhead_ns);
                self.handle_playback_message(pb);
            }
        }
    }

    pub(crate) fn handle_playback_message(&mut self, message: PlaybackMessage) {
        match message {
            PlaybackMessage::DeleteSelectedClip => {
                if let Some(tr_id) = self.playback.selected_transition_id {
                    let _ = self.remove_transition(tr_id);
                } else if let Some(clip_id) = self.playback.selected_clip_id {
                    let _ = self.remove_clip(clip_id);
                }
            }
            PlaybackMessage::Undo => {
                let _ = self.undo();
            }
            PlaybackMessage::Redo => {
                let _ = self.redo();
            }
            other => {
                if let Err(err) = playback::apply_transport_or_selection(
                    &mut self.playback,
                    &mut self.project,
                    &self.engine,
                    other,
                ) {
                    self.last_error = Some(err);
                }
            }
        }
    }
}
