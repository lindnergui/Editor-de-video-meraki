use super::*;
use crate::protocol::create_engine_channel;
use meraki_core::{ClipEffect, ClipTransform, TitleOverlay, TrackId, VideoEffectKind};

#[test]
fn live_properties_preserve_native_pipeline_and_structural_changes_replace_it() {
    crate::ensure_media_initialized().unwrap();
    let (_handle, worker) = create_engine_channel();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            context.block_on(async {
                let mut playback = PlaybackEngine::new(Arc::new(worker)).unwrap();
                let id = ClipId::new();
                let mut effect = ClipEffect::with_defaults(VideoEffectKind::FilmStyle);
                let title = TimelineRenderClip {
                    clip_id: id,
                    track_id: TrackId::new(),
                    track_kind: TrackKind::Overlay,
                    path: Default::default(),
                    timeline_start_ns: 0,
                    source_in_ns: 0,
                    duration_ns: 2_000_000_000,
                    transform: ClipTransform::default(),
                    title: Some(TitleOverlay::default()),
                    effects: vec![effect.clone()],
                    transition_in: None,
                    transition_out: None,
                    keyframes: Vec::new(),
                    volume_keyframes: Vec::new(),
                };
                playback
                    .sync_timeline(vec![title], Vec::new(), PreviewQuality::default())
                    .await
                    .unwrap();
                let original = playback.graph.pipeline.clone();
                let (state, _, _) = original.state(Some(gst::ClockTime::from_seconds(5)));
                assert_eq!(state, Ok(gst::StateChangeSuccess::Success));

                effect.params.set_primary_intensity(0.15);
                playback
                    .update_clips_live(HashMap::from([(
                        id,
                        ClipPatch {
                            transform: Some(ClipTransform {
                                opacity: 0.75,
                                scale: 1.3,
                                ..ClipTransform::default()
                            }),
                            effects: Some(vec![effect]),
                            ..ClipPatch::default()
                        },
                    )]))
                    .await
                    .unwrap();
                assert_eq!(playback.graph.pipeline, original);
                assert!(playback.refresh_pending);

                playback
                    .update_clips_live(HashMap::from([(
                        id,
                        ClipPatch {
                            effects: Some(Vec::new()),
                            ..ClipPatch::default()
                        },
                    )]))
                    .await
                    .unwrap();
                assert_ne!(playback.graph.pipeline, original);
                playback.shutdown();
            })
        })
        .unwrap();
}
