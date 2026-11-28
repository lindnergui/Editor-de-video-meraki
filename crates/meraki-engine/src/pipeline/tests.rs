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
                    proxy_path: None,
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
                    .sync_timeline(
                        vec![title],
                        Vec::new(),
                        Vec::new(),
                        PreviewQuality::default(),
                    )
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

#[test]
fn live_audio_patch_keeps_the_active_pipeline_and_seek_slot() {
    use crate::audio_live::AudioEdits;
    use crate::discoverer::tests::{write_test_wav_1s_stereo_48khz, TempFileGuard};
    use crate::protocol::EngineCommand;

    crate::ensure_media_initialized().unwrap();
    let wav = TempFileGuard::new("audio_graph_identity", "wav");
    write_test_wav_1s_stereo_48khz(&wav.0);
    let mut pcm = std::fs::read(&wav.0).unwrap();
    for (index, frame) in pcm[44..].as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let phase = std::f64::consts::TAU * 320.0 * index as f64 / 48_000.0;
        let sample = (3276.0 * phase.sin()) as i16;
        frame[..2].copy_from_slice(&sample.to_le_bytes());
        frame[2..].copy_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(&wav.0, pcm).unwrap();
    let (_handle, worker) = create_engine_channel();
    let worker = Arc::new(worker);
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            context.block_on(async {
                let mut playback = PlaybackEngine::new(Arc::clone(&worker)).unwrap();
                let id = TrackId::new();
                let clip = TimelineRenderClip {
                    clip_id: ClipId::new(),
                    track_id: id,
                    path: wav.0.clone(),
                    proxy_path: None,
                    track_kind: TrackKind::Audio,
                    timeline_start_ns: 0,
                    source_in_ns: 0,
                    duration_ns: 1_000_000_000,
                    transform: ClipTransform::default(),
                    title: None,
                    effects: Vec::new(),
                    keyframes: Vec::new(),
                    transition_in: None,
                    transition_out: None,
                    volume_keyframes: Vec::new(),
                };
                playback
                    .sync_timeline(
                        vec![clip],
                        Vec::new(),
                        Vec::new(),
                        PreviewQuality::default(),
                    )
                    .await
                    .unwrap();
                let pipeline = playback.graph.pipeline.clone();
                let (state, current, _) = pipeline.state(Some(gst::ClockTime::from_seconds(5)));
                assert_eq!(state, Ok(gst::StateChangeSuccess::Success));
                assert_eq!(current, gst::State::Paused);
                let samples = observe_headless_output(&pipeline);
                playback.play().unwrap();
                if let Some(samples) = &samples {
                    receive_peak(samples, |peak| peak > 0.085);
                }
                let mut live = AudioEdits::default();
                assert!(live
                    .collect(EngineCommand::UpdateAudioLive {
                        track_volumes: Some((id, -3.0, false)),
                        master_volume_db: Some(-6.0),
                        master_muted: None,
                        track_eq: Some((id, [1.0, -2.0, 3.0, 4.0])),
                        track_enhancements: None,
                    })
                    .is_none());
                let started = Instant::now();
                live.flush(&mut playback, &worker);
                if let Some(samples) = &samples {
                    let changed = receive_peak(samples, |peak| (0.01..0.045).contains(&peak));
                    eprintln!(
                        "PCM na saída headless mudou em {:?}",
                        changed.duration_since(started)
                    );
                }
                assert_eq!(playback.graph.pipeline, pipeline);
                assert_eq!(pipeline.current_state(), gst::State::Playing);
                assert!(playback.pending_seek_ns.is_none() && !playback.refresh_pending);
                playback.shutdown();
            })
        })
        .unwrap();
}

fn observe_headless_output(
    pipeline: &ges::Pipeline,
) -> Option<crossbeam_channel::Receiver<(Instant, f32)>> {
    let sink = pipeline.by_name("meraki-preview-afakesink")?;
    let (tx, rx) = crossbeam_channel::bounded(64);
    sink.set_property("signal-handoffs", true);
    sink.connect("handoff", false, move |values| {
        let buffer = values.get(1)?.get::<gst::Buffer>().ok()?;
        let map = buffer.map_readable().ok()?;
        let peak = map
            .as_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes).abs())
            .fold(0.0_f32, f32::max);
        let _ = tx.try_send((Instant::now(), peak));
        None
    });
    Some(rx)
}

fn receive_peak(
    samples: &crossbeam_channel::Receiver<(Instant, f32)>,
    matches: impl Fn(f32) -> bool,
) -> Instant {
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let (time, peak) = samples.recv_deadline(deadline).unwrap();
        if matches(peak) {
            return time;
        }
    }
}
