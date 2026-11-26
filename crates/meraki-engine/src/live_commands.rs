use crate::clip_prepare::PipelineError;
use crate::pipeline::PlaybackEngine;
use crate::protocol::{EngineCommand, EngineEvent, EngineWorkerChannel, TimelineRenderClip};
use meraki_core::{ClipEffect, ClipId, ClipTransform, TitleOverlay};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ClipPatch {
    pub transform: Option<ClipTransform>,
    pub title: Option<TitleOverlay>,
    pub effects: Option<Vec<ClipEffect>>,
}

#[derive(Default)]
pub(crate) struct LiveEdits(HashMap<ClipId, ClipPatch>);

impl LiveEdits {
    /// Preserva o último valor de cada campo até a próxima barreira da fila.
    pub(crate) fn collect(&mut self, command: EngineCommand) -> Option<EngineCommand> {
        let EngineCommand::UpdateClipLive {
            clip_id,
            transform,
            title,
            effects,
        } = command
        else {
            return Some(command);
        };
        let patch = self.0.entry(clip_id).or_default();
        patch.transform = transform.or(patch.transform.take());
        patch.title = title.or(patch.title.take());
        patch.effects = effects.or(patch.effects.take());
        None
    }

    pub(crate) async fn flush(
        &mut self,
        playback: &mut PlaybackEngine,
        worker: &EngineWorkerChannel,
    ) {
        if !self.0.is_empty() {
            if let Err(err) = playback
                .update_clips_live(std::mem::take(&mut self.0))
                .await
            {
                worker.emit_critical(EngineEvent::Error(err.to_string()));
            }
        }
    }
}

impl ClipPatch {
    pub(crate) fn apply(self, clip: &mut TimelineRenderClip) -> Result<(), PipelineError> {
        let invalid = |source| PipelineError::InvalidTransform {
            clip_id: clip.clip_id,
            source,
        };
        if let Some(transform) = self.transform {
            transform.validate().map_err(invalid)?;
            clip.transform = transform;
        }
        if let Some(title) = self.title {
            title.validate().map_err(invalid)?;
            clip.title = Some(title);
        }
        if let Some(effects) = self.effects {
            for effect in &effects {
                effect.validate().map_err(invalid)?;
            }
            clip.effects = effects;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_latest_transform_without_discarding_title_or_effects() {
        let id = ClipId::new();
        let other = ClipId::new();
        let title = TitleOverlay::default();
        let mut edits = LiveEdits::default();
        edits.collect(EngineCommand::UpdateClipLive {
            clip_id: id,
            transform: None,
            title: Some(title.clone()),
            effects: Some(Vec::new()),
        });
        for i in 0..15 {
            edits.collect(EngineCommand::UpdateClipLive {
                clip_id: id,
                transform: Some(ClipTransform {
                    pos_x: i as f32,
                    ..ClipTransform::default()
                }),
                title: None,
                effects: None,
            });
        }
        edits.collect(EngineCommand::UpdateClipLive {
            clip_id: other,
            transform: Some(ClipTransform::default()),
            title: None,
            effects: None,
        });
        assert!(matches!(
            edits.collect(EngineCommand::Play),
            Some(EngineCommand::Play)
        ));
        assert_eq!(edits.0.len(), 2);
        assert_eq!(edits.0[&id].transform.map(|tr| tr.pos_x), Some(14.0));
        assert_eq!(edits.0[&id].title.as_ref(), Some(&title));
        assert_eq!(edits.0[&id].effects.as_ref().map(Vec::len), Some(0));
    }
}
