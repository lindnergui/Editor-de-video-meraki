use crate::components::audio_timeline::track_band_layout;
use crate::components::timeline_math::display_tracks_ordered;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Renderer, Size};
use meraki_core::{ClipId, Project};
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Cache estático para régua, clipes, ondas e keyframes, combinado com desenho direto
/// leve da agulha (`playhead_ns`) para evitar re-tesselar a timeline a cada tick a 30 Hz.
#[derive(Default)]
pub(crate) struct DualCanvasCache {
    static_cache: canvas::Cache,
    last_static_key: Cell<Option<u64>>,
}

impl DualCanvasCache {
    pub(crate) fn draw_layers(
        &self,
        renderer: &Renderer,
        size: Size,
        static_key: u64,
        draw_static: impl FnOnce(&mut Frame),
        draw_playhead: impl FnOnce(&mut Frame),
    ) -> Vec<Geometry> {
        if self.last_static_key.get() != Some(static_key) {
            self.static_cache.clear();
            self.last_static_key.set(Some(static_key));
        }
        let static_geom = self.static_cache.draw(renderer, size, draw_static);
        let mut playhead_frame = Frame::new(renderer, size);
        draw_playhead(&mut playhead_frame);
        vec![static_geom, playhead_frame.into_geometry()]
    }
}

/// Gera chave de cache do conteúdo estático da timeline principal (exclui `playhead_ns`).
pub fn hash_timeline_static(
    project: &Project,
    overlay_first: bool,
    selected_clip_id: Option<ClipId>,
    px_per_sec: f32,
    scroll_x_px: f32,
    drag_preview: Option<(ClipId, u64, u64)>,
) -> u64 {
    let mut h = DefaultHasher::new();
    overlay_first.hash(&mut h);
    selected_clip_id.hash(&mut h);
    px_per_sec.to_bits().hash(&mut h);
    scroll_x_px.to_bits().hash(&mut h);
    drag_preview.hash(&mut h);

    for track in display_tracks_ordered(&project.timeline, overlay_first) {
        track.id.hash(&mut h);
        track.name.hash(&mut h);
        track.kind.hash(&mut h);
        track.clips.len().hash(&mut h);
        for clip in track.clips {
            clip.id.hash(&mut h);
            clip.asset_id.hash(&mut h);
            clip.timeline_start_ns.hash(&mut h);
            clip.source_in_ns.hash(&mut h);
            clip.duration_ns.hash(&mut h);
            if let Some(asset) = project.asset(clip.asset_id) {
                asset.name.hash(&mut h);
            }
            if let Some(ref title) = clip.title {
                title.text.hash(&mut h);
            }
            clip.effects.len().hash(&mut h);
            for fx in &clip.effects {
                fx.id.hash(&mut h);
                fx.enabled.hash(&mut h);
                fx.short_badge_label().hash(&mut h);
            }
            clip.transition_in.is_some().hash(&mut h);
            clip.transition_out.is_some().hash(&mut h);
            clip.keyframes.len().hash(&mut h);
            for kf in &clip.keyframes {
                kf.time_offset_ns.hash(&mut h);
                kf.property.hash(&mut h);
                kf.easing.hash(&mut h);
                kf.value.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Gera chave de cache do conteúdo estático da timeline de áudio (exclui `playhead_ns`).
pub fn hash_audio_timeline_static(
    project: &Project,
    selected_clip_id: Option<ClipId>,
    px_per_sec: f32,
    scroll_x_px: f32,
    drag_kf: Option<(ClipId, u64, u32)>,
) -> u64 {
    let mut h = DefaultHasher::new();
    selected_clip_id.hash(&mut h);
    px_per_sec.to_bits().hash(&mut h);
    scroll_x_px.to_bits().hash(&mut h);
    drag_kf.hash(&mut h);

    for row in track_band_layout(&project.timeline) {
        row.id.hash(&mut h);
        row.name.hash(&mut h);
        row.kind.hash(&mut h);
        row.muted.hash(&mut h);
        row.solo.hash(&mut h);
        row.clips.len().hash(&mut h);
        for clip in row.clips {
            clip.id.hash(&mut h);
            clip.timeline_start_ns.hash(&mut h);
            clip.duration_ns.hash(&mut h);
            if let Some(asset) = project.asset(clip.asset_id) {
                asset.name.hash(&mut h);
            }
            clip.volume_keyframes.len().hash(&mut h);
            for kf in &clip.volume_keyframes {
                kf.time_offset_ns.hash(&mut h);
                kf.gain_db.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use meraki_core::{Clip, VolumeKeyframe};

    #[test]
    fn playhead_ticks_keep_static_hashes_stable_while_structural_edits_invalidate() {
        let mut project = Project::new("Cache Teste");
        let v1 = project.timeline.tracks[0].id;
        let a1 = project.timeline.tracks[1].id;
        let asset_id = meraki_core::AssetId::new();
        let c1 = project
            .timeline
            .add_clip(v1, Clip::new(asset_id, 0, 0, 3_000_000_000))
            .expect("add v1");
        let _c2 = project
            .timeline
            .add_clip(a1, Clip::new(asset_id, 0, 0, 3_000_000_000))
            .expect("add a1");

        let h_main_0 = hash_timeline_static(&project, false, Some(c1), 25.0, 0.0, None);
        let h_audio_0 = hash_audio_timeline_static(&project, Some(c1), 25.0, 0.0, None);

        // Alterar apenas o playhead não invalida o cache estático
        project.timeline.playhead_ns = 1_750_000_000;
        assert_eq!(hash_timeline_static(&project, false, Some(c1), 25.0, 0.0, None), h_main_0);
        assert_eq!(hash_audio_timeline_static(&project, Some(c1), 25.0, 0.0, None), h_audio_0);

        // Alterar zoom, seleção, preview de arraste ou volume keyframe invalida corretamente
        assert_ne!(hash_timeline_static(&project, false, Some(c1), 50.0, 0.0, None), h_main_0);
        assert_ne!(hash_timeline_static(&project, false, None, 25.0, 0.0, None), h_main_0);
        assert_ne!(
            hash_timeline_static(&project, false, Some(c1), 25.0, 0.0, Some((c1, 500, 3_000_000_000))),
            h_main_0
        );
        assert_ne!(hash_audio_timeline_static(&project, Some(c1), 25.0, 12.0, None), h_audio_0);
        assert_ne!(
            hash_audio_timeline_static(
                &project,
                Some(c1),
                25.0,
                0.0,
                Some((c1, VolumeKeyframe::new(100, -6.0).time_offset_ns, (-6.0_f32).to_bits()))
            ),
            h_audio_0
        );
    }
}
