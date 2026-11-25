use crate::discoverer::tests::{
    write_test_png_via_gst, write_test_wav_1s_stereo_48khz, TempFileGuard,
};
use crate::{protocol, spawn_engine, EngineCommand, EngineEvent, TimelineRenderClip};
use meraki_core::{
    AssetId, ClipEffect, ClipId, ClipTransform, ClipTransition, Keyframe, KeyframeEasing,
    KeyframeProperty, PreviewQuality, TitleOverlay, TitleStyle, TrackId, TrackKind, TransitionKind,
    VideoEffectKind,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod video_resync;

fn media_render_clip(path: PathBuf, track_kind: TrackKind, duration_ns: u64) -> TimelineRenderClip {
    TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: TrackId::new(),
        path,
        track_kind,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    }
}

#[test]
fn atomic_seek_coalesces_to_latest_timestamp() {
    let (handle, worker) = protocol::create_engine_channel();
    handle.request_seek(100);
    handle.request_seek(2_500);
    handle.request_seek(9_999);
    assert_eq!(worker.take_pending_seek(), Some(9_999));
    assert_eq!(worker.take_pending_seek(), None);
}

#[test]
fn drop_if_full_never_blocks_when_event_channel_saturated() {
    let (handle, worker) = protocol::create_engine_channel();
    for tick in 0..512_u64 {
        worker.emit_drop_if_full(EngineEvent::PlayheadTick(tick));
    }
    assert!(handle.events().try_recv().is_ok());
}

#[test]
fn spawn_engine_emits_ready_imports_media_processes_seek_and_shuts_down() {
    let wav = TempFileGuard::new("engine_wav", "wav");
    write_test_wav_1s_stereo_48khz(&wav.0);
    let missing = TempFileGuard::new("engine_missing", "mp4");
    let valid_id = AssetId::new();
    let invalid_id = AssetId::new();

    let handle = spawn_engine();
    let first = handle
        .events()
        .recv_timeout(Duration::from_secs(30))
        .expect("motor deve emitir EngineReady ou Error");
    assert!(matches!(first, EngineEvent::EngineReady));

    handle.request_seek(42_000_000);
    handle
        .send(EngineCommand::ImportMedia {
            asset_id: valid_id,
            path: wav.0.clone(),
            generate_proxy: false,
        })
        .unwrap();
    handle
        .send(EngineCommand::ImportMedia {
            asset_id: invalid_id,
            path: missing.0.clone(),
            generate_proxy: false,
        })
        .unwrap();

    let mut got_tick = false;
    let mut got_discovered = false;
    let mut got_error = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !(got_tick && got_discovered && got_error) {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(200)) {
            match event {
                EngineEvent::PlayheadTick(42_000_000) => got_tick = true,
                EngineEvent::MediaDiscovered(meta) if meta.asset_id == valid_id => {
                    assert_eq!(meta.duration_ns, 1_000_000_000);
                    assert!(meta.has_audio);
                    got_discovered = true;
                }
                EngineEvent::Error(_) => got_error = true,
                _ => {}
            }
        }
    }
    assert!(got_tick && got_discovered && got_error);
    handle.send(EngineCommand::Shutdown).unwrap();
}

#[test]
fn ges_playback_syncs_timeline_renders_rgba_frames_seeks_and_reports_errors() {
    let png = TempFileGuard::new("ges_frame", "png");
    write_test_png_via_gst(&png.0, 1600, 900);
    let wav = TempFileGuard::new("ges_audio", "wav");
    write_test_wav_1s_stereo_48khz(&wav.0);

    let handle = spawn_engine();
    let ready = handle
        .events()
        .recv_timeout(Duration::from_secs(30))
        .expect("motor deve emitir EngineReady");
    assert!(matches!(ready, EngineEvent::EngineReady));

    let clips = vec![
        media_render_clip(png.0.clone(), TrackKind::Video, 3_000_000_000),
        media_render_clip(wav.0.clone(), TrackKind::Audio, 1_000_000_000),
    ];

    handle
        .send(EngineCommand::SyncTimeline {
            clips,
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();

    let mut preroll_frame = None;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && preroll_frame.is_none() {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::FrameReady(frame) => preroll_frame = Some(frame),
                EngineEvent::Error(err) => panic!("erro inesperado durante SyncTimeline: {err}"),
                _ => {}
            }
        }
    }
    let frame = preroll_frame.expect("deve emitir FrameReady no preroll GES");
    assert!(frame.width > 0 && frame.width <= 1280);
    assert!(frame.height > 0 && frame.height <= 720);
    assert_eq!(frame.rgba_pixels.len(), (frame.width * frame.height * 4) as usize);

    handle.send(EngineCommand::SetMasterVolume(0.5)).unwrap();
    handle.send(EngineCommand::SetPlaybackRate(1.5)).unwrap();
    handle.send(EngineCommand::Play).unwrap();

    let mut saw_playing_tick = false;
    let play_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < play_deadline && !saw_playing_tick {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(50)) {
            match event {
                EngineEvent::PlayheadTick(_) => saw_playing_tick = true,
                EngineEvent::Error(err) => panic!("erro inesperado durante Play: {err}"),
                _ => {}
            }
        }
    }
    assert!(saw_playing_tick, "deve emitir PlayheadTick durante reprodução");

    while handle.events().try_recv().is_ok() {}
    handle.send(EngineCommand::Pause).unwrap();
    handle.send(EngineCommand::SetPreviewQuality(PreviewQuality::Full1080p)).unwrap();
    handle.request_seek(1_200_000_000);

    let mut hd_frame = None;
    let hd_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < hd_deadline && hd_frame.is_none() {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(50)) {
            match event {
                EngineEvent::FrameReady(f) => hd_frame = Some(f),
                EngineEvent::Error(err) => panic!("erro inesperado após seek em 1080p: {err}"),
                _ => {}
            }
        }
    }
    let hd = hd_frame.expect("deve emitir FrameReady após seek em 1080p");
    assert!(hd.width > 0 && hd.width <= 1920);
    assert!(hd.height > 0 && hd.height <= 1080);
    assert_eq!(hd.rgba_pixels.len(), (hd.width * hd.height * 4) as usize);

    // Parâmetros inválidos e clipe inexistente devem emitir EngineEvent::Error sem panic.
    let missing = TempFileGuard::new("ges_missing", "mp4");
    handle.send(EngineCommand::SetMasterVolume(f32::NAN)).unwrap();
    handle.send(EngineCommand::SetPlaybackRate(0.0)).unwrap();
    handle
        .send(EngineCommand::SyncTimeline {
            clips: vec![media_render_clip(missing.0.clone(), TrackKind::Video, 1_000_000_000)],
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();

    let mut error_count = 0_u32;
    let err_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < err_deadline && error_count < 3 {
        if let Ok(EngineEvent::Error(_)) = handle.events().recv_timeout(Duration::from_millis(50)) {
            error_count += 1;
        }
    }
    assert!(error_count >= 3);

    handle
        .send(EngineCommand::SyncTimeline {
            clips: Vec::new(),
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
    handle.send(EngineCommand::Shutdown).unwrap();
}

#[test]
fn ges_title_clip_effects_transitions_keyframes_and_asset_cache_sync_cleanly() {
    let png = TempFileGuard::new("ges_fx_frame", "png");
    write_test_png_via_gst(&png.0, 1280, 720);

    let handle = spawn_engine();
    let ready = handle
        .events()
        .recv_timeout(Duration::from_secs(30))
        .expect("motor deve emitir EngineReady");
    assert!(matches!(ready, EngineEvent::EngineReady));

    let mut video_clip = media_render_clip(png.0.clone(), TrackKind::Video, 3_000_000_000);
    video_clip.effects = vec![
        ClipEffect::new(VideoEffectKind::Cinematic, 0.7),
        ClipEffect::new(VideoEffectKind::Glitch, 0.5),
    ];
    video_clip.transition_in = Some(ClipTransition::new(TransitionKind::Fade, 500_000_000));
    video_clip.transition_out = Some(ClipTransition::new(TransitionKind::Blur, 500_000_000));
    video_clip.keyframes = vec![
        Keyframe::new(0, KeyframeProperty::PosX, 0.0, KeyframeEasing::Linear),
        Keyframe::new(2_000_000_000, KeyframeProperty::PosX, 40.0, KeyframeEasing::Linear),
        Keyframe::new(0, KeyframeProperty::Scale, 1.0, KeyframeEasing::EaseInOut),
        Keyframe::new(2_000_000_000, KeyframeProperty::Scale, 1.2, KeyframeEasing::EaseInOut),
        Keyframe::new(1_000_000_000, KeyframeProperty::Opacity, 0.9, KeyframeEasing::EaseInOut),
    ];

    let title_clip = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: TrackId::new(),
        path: PathBuf::new(),
        track_kind: TrackKind::Overlay,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 3_000_000_000,
        transform: ClipTransform::default(),
        title: Some(TitleOverlay::new("Meraki FX", TitleStyle::ModernText)),
        effects: Vec::new(),
        transition_in: Some(ClipTransition::new(TransitionKind::Fade, 400_000_000)),
        transition_out: None,
        keyframes: vec![
            Keyframe::new(0, KeyframeProperty::Opacity, 0.4, KeyframeEasing::Linear),
            Keyframe::new(1_500_000_000, KeyframeProperty::Opacity, 1.0, KeyframeEasing::EaseInOut),
            Keyframe::new(0, KeyframeProperty::PosY, 0.0, KeyframeEasing::Linear),
            Keyframe::new(1_500_000_000, KeyframeProperty::PosY, -20.0, KeyframeEasing::EaseInOut),
        ],
        volume_keyframes: Vec::new(),
    };

    handle
        .send(EngineCommand::SyncTimeline {
            clips: vec![video_clip, title_clip],
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();

    let first_frame = wait_for_frame(&handle, "primeira sincronização TitleClip+FX+Keyframes");
    assert!(first_frame.width > 0 && first_frame.height > 0);

    // Segundo SyncTimeline reutiliza o mesmo arquivo PNG (exercitando o cache de UriClipAsset)
    while handle.events().try_recv().is_ok() {}
    let mut cached_video = media_render_clip(png.0.clone(), TrackKind::Video, 2_500_000_000);
    cached_video.effects = vec![ClipEffect::new(VideoEffectKind::FilmStyle, 0.6)];
    cached_video.transition_in = Some(ClipTransition::new(TransitionKind::Zoom, 500_000_000));

    let cached_title = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: TrackId::new(),
        path: PathBuf::new(),
        track_kind: TrackKind::Overlay,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 2_500_000_000,
        transform: ClipTransform::default(),
        title: Some(TitleOverlay::new("Legenda em Cache", TitleStyle::QuickCaption)),
        effects: Vec::new(),
        transition_in: None,
        transition_out: Some(ClipTransition::new(TransitionKind::Fade, 400_000_000)),
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    };

    handle
        .send(EngineCommand::SyncTimeline {
            clips: vec![cached_video, cached_title],
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();

    let second_frame = wait_for_frame(&handle, "segunda sincronização com cache de UriClipAsset");
    assert!(second_frame.width > 0 && second_frame.height > 0);

    handle.request_seek(1_000_000_000);
    let _seek_frame = wait_for_frame(&handle, "seek em timeline com TitleClip e Keyframes");

    handle.send(EngineCommand::Shutdown).unwrap();
}

#[test]
fn encoding_profiles_resolve_cleanly_for_all_presets_and_formats() {
    crate::ensure_media_initialized().unwrap();
    for preset in meraki_core::ExportPreset::ALL {
        let settings = meraki_core::ExportSettings::from_preset(preset);
        let resolved = crate::encoding_profiles::probe_encoding_profile(&settings)
            .expect("perfil de exportação padrão deve resolver plugins disponíveis");
        assert!(resolved.audio_encoder.is_some());
        if !settings.format.is_audio_only() {
            assert!(resolved.video_encoder.is_some());
            assert!(resolved.muxer.is_some());
        }
    }
    let mut mp3 = meraki_core::ExportSettings::from_preset(meraki_core::ExportPreset::AudioOnly);
    mp3.format = meraki_core::ExportFormat::AudioMp3;
    let resolved_mp3 = crate::encoding_profiles::probe_encoding_profile(&mp3).unwrap();
    assert!(resolved_mp3.audio_encoder.is_some());
}

fn wait_for_frame(handle: &crate::EngineHandle, phase: &str) -> crate::VideoFrame {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::FrameReady(frame) => return frame,
                EngineEvent::Error(err) => panic!("erro inesperado durante {phase}: {err}"),
                _ => {}
            }
        }
    }
    panic!("timeout aguardando FrameReady durante {phase}");
}
