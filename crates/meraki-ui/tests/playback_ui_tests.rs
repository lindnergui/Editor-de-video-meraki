use bytes::Bytes;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::image;
use meraki_core::{ClipTransform, MediaMetadata, PreviewQuality, TrackKind};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent, VideoFrame};
use meraki_ui::playback::{format_timecode, map_keyboard_shortcut};
use meraki_ui::views::files::FilesMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::path::PathBuf;

#[test]
fn starts_on_files_tab_with_default_project_name() {
    let (engine, _worker) = create_engine_channel();
    let app = MerakiApp::new(engine);
    assert_eq!(app.active_tab(), WorkspaceTab::Files);
    assert_eq!(app.project_name(), "Projeto sem título");
    assert_eq!(app.project().child_folders(None).len(), 3);
    assert!(!app.is_playing());
    assert_eq!(app.playhead_ns(), 0);
    assert_eq!(app.playback_rate(), 1.0);
    assert_eq!(app.master_volume(), 1.0);
    assert_eq!(app.preview_quality(), PreviewQuality::Proxy720p);
    assert!(app.current_frame().is_none());
    assert!(app.selected_clip_id().is_none());
}

#[test]
fn select_tab_and_export_navigate_cleanly() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    for tab in WorkspaceTab::ALL {
        let _ = app.update(Message::SelectTab(tab));
        assert_eq!(app.active_tab(), tab);
    }
    let _ = app.update(Message::SelectTab(WorkspaceTab::Edit));
    let _ = app.update(Message::ExportClicked);
    assert_eq!(app.active_tab(), WorkspaceTab::Delivery);
}

#[test]
fn request_shutdown_sends_command_to_engine_channel() {
    let (engine, worker) = create_engine_channel();
    let app = MerakiApp::new(engine);
    app.request_shutdown().unwrap();
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::Shutdown)));
}

#[test]
fn media_discovered_frame_ready_and_playhead_tick_update_state() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let asset_id = app.import_media(PathBuf::from("cena.mp4"), None, true).unwrap();
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::ImportMedia { asset_id: id, .. }) if id == asset_id));

    let _ = app.update(Message::EngineEvent(EngineEvent::EngineReady));
    let _ = app.update(Message::EngineEvent(EngineEvent::PlayheadTick(123_456)));
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 6_000_000_000,
        width: Some(1920),
        height: Some(1080),
        has_audio: true,
        ..MediaMetadata::default()
    })));
    let _ = app.update(Message::EngineEvent(EngineEvent::FrameReady(VideoFrame {
        width: 2,
        height: 2,
        rgba_pixels: Bytes::from_static(&[255; 16]),
        pts_ns: 123_456,
    })));
    let _ = app.update(Message::EngineEvent(EngineEvent::Error("falha".to_string())));

    let asset = app.project().asset(asset_id).unwrap();
    assert_eq!(asset.metadata.duration_ns, 6_000_000_000);
    assert_eq!((asset.metadata.width, asset.metadata.height), (Some(1920), Some(1080)));
    assert!(app.is_engine_ready());
    assert_eq!(app.playhead_ns(), 123_456);
    assert_eq!(app.project().timeline.playhead_ns, 123_456);
    assert!(matches!(app.current_frame(), Some(image::Handle::Rgba { width: 2, height: 2, .. })));
    assert_eq!(app.last_error(), Some("falha"));
}

#[test]
fn send_to_timeline_respects_in_out_routes_v1_a1_and_syncs_all_mutations() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let v_id = app.import_media(PathBuf::from("video.mp4"), None, false).unwrap();
    let a_id = app.import_media(PathBuf::from("audio.wav"), None, false).unwrap();
    while worker.try_recv_command().is_some() {}

    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: v_id,
        duration_ns: 10_000_000_000,
        ..MediaMetadata::default()
    })));
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: a_id,
        duration_ns: 4_000_000_000,
        has_audio: true,
        ..MediaMetadata::default()
    })));

    // Marca In/Out no vídeo via UI e envia para V1
    let _ = app.update(Message::Files(FilesMessage::SelectAsset(v_id)));
    let _ = app.update(Message::Files(FilesMessage::PreviewScrubChanged(0.2)));
    let _ = app.update(Message::Files(FilesMessage::MarkIn));
    let _ = app.update(Message::Files(FilesMessage::PreviewScrubChanged(0.6)));
    let _ = app.update(Message::Files(FilesMessage::MarkOut));
    let _ = app.update(Message::Files(FilesMessage::SendToTimeline(v_id)));

    let c1 = app.selected_clip_id().unwrap();
    let (v1_id, clip1) = app.project().timeline.clip(c1).unwrap();
    assert_eq!(app.project().timeline.track(v1_id).unwrap().name, "V1");
    assert!(clip1.source_in_ns.abs_diff(2_000_000_000) <= 1_000);
    assert!(clip1.duration_ns.abs_diff(4_000_000_000) <= 2_000);

    match worker.try_recv_command() {
        Some(EngineCommand::SyncTimeline { clips, preview_quality, .. }) => {
            assert_eq!(clips.len(), 1);
            assert_eq!(clips[0].track_kind, TrackKind::Video);
            assert_eq!(preview_quality, PreviewQuality::Proxy720p);
        }
        other => panic!("esperado SyncTimeline após enviar vídeo, recebido {other:?}"),
    }

    // Envia áudio para A1
    let _ = app.update(Message::Files(FilesMessage::SendToTimeline(a_id)));
    let c2 = app.selected_clip_id().unwrap();
    assert_eq!(app.project().timeline.track(app.project().timeline.clip(c2).unwrap().0).unwrap().name, "A1");
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 2));

    // Testa move, trim, split, delete, undo e redo sincronizando a cada etapa
    app.move_clip(c1, v1_id, 500_000_000).unwrap();
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    app.trim_clip(c1, 500_000_000, 1_000_000_000, 3_000_000_000, Some(10_000_000_000)).unwrap();
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let right_id = app.split_clip(c1, 2_000_000_000).unwrap();
    assert!(app.project().timeline.clip(right_id).is_some());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 3));

    app.update_clip_transform(c1, ClipTransform { opacity: 0.8, ..ClipTransform::default() }).unwrap();
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { clip_id, .. }) if clip_id == c1));

    // Seleciona e deleta via PlaybackMessage::DeleteSelectedClip
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(Some(right_id))));
    let _ = app.update(Message::Playback(PlaybackMessage::DeleteSelectedClip));
    assert!(app.project().timeline.clip(right_id).is_none());
    assert!(app.selected_clip_id().is_none());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 2));

    // Ctrl+Z (Undo) restaura o clipe removido e ressincroniza
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert!(app.project().timeline.clip(right_id).is_some());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 3));

    // Ctrl+Shift+Z (Redo) remove novamente e ressincroniza
    let _ = app.update(Message::Playback(PlaybackMessage::Redo));
    assert!(app.project().timeline.clip(right_id).is_none());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 2));
}

#[test]
fn keyboard_shortcuts_and_transport_commands_behave_accurately() {
    assert!(matches!(
        map_keyboard_shortcut(Key::Named(Named::Space), Modifiers::empty()),
        Some(PlaybackMessage::TogglePlayPause)
    ));
    assert!(matches!(
        map_keyboard_shortcut(Key::Character(" ".into()), Modifiers::empty()),
        Some(PlaybackMessage::TogglePlayPause)
    ));
    assert!(map_keyboard_shortcut(Key::Named(Named::Space), Modifiers::CTRL).is_none());
    assert!(matches!(
        map_keyboard_shortcut(Key::Character("z".into()), Modifiers::CTRL),
        Some(PlaybackMessage::Undo)
    ));
    assert!(matches!(
        map_keyboard_shortcut(Key::Character("Z".into()), Modifiers::CTRL | Modifiers::SHIFT),
        Some(PlaybackMessage::Redo)
    ));
    assert!(matches!(
        map_keyboard_shortcut(Key::Named(Named::Delete), Modifiers::empty()),
        Some(PlaybackMessage::DeleteSelectedClip)
    ));
    assert!(matches!(
        map_keyboard_shortcut(Key::Named(Named::Backspace), Modifiers::empty()),
        Some(PlaybackMessage::DeleteSelectedClip)
    ));
    assert!(map_keyboard_shortcut(Key::Named(Named::Backspace), Modifiers::ALT).is_none());

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::Playback(PlaybackMessage::TogglePlayPause));
    assert!(app.is_playing());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::Play)));

    let _ = app.update(Message::Playback(PlaybackMessage::TogglePlayPause));
    assert!(!app.is_playing());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::Pause)));

    let _ = app.update(Message::Playback(PlaybackMessage::SetPlaybackRate(1.5)));
    assert_eq!(app.playback_rate(), 1.5);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SetPlaybackRate(r)) if (r - 1.5).abs() < 1e-6));

    let _ = app.update(Message::Playback(PlaybackMessage::SetMasterVolume(0.75)));
    assert_eq!(app.master_volume(), 0.75);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateAudioLive {
        master_volume_db: Some(db), ..
    }) if (db - 20.0 * 0.75_f64.log10()).abs() < 1e-6));

    let _ = app.update(Message::Playback(PlaybackMessage::SetPreviewQuality(PreviewQuality::Full1080p)));
    assert_eq!(app.preview_quality(), PreviewQuality::Full1080p);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SetPreviewQuality(PreviewQuality::Full1080p))));

    let _ = app.update(Message::Playback(PlaybackMessage::Seek(1_500_000_000)));
    assert_eq!(app.playhead_ns(), 1_500_000_000);
    assert_eq!(worker.take_pending_seek(), Some(1_500_000_000));
    assert_eq!(format_timecode(3_661_500_000_000, 30), "01:01:01:15");
}
