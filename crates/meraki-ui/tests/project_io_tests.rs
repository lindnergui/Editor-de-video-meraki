use bytes::Bytes;
use iced::keyboard::key::{Key, Named};
use iced::keyboard::Modifiers;
use meraki_core::{AssetId, PreviewQuality};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent, ThumbnailFrame};
use meraki_ui::app_project_io::{autosave_path, ensure_meraki_extension, map_project_shortcut};
use meraki_ui::{MerakiApp, Message, PlaybackMessage, ProjectIoMessage};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn save_and_open_meraki_roundtrip_restores_project_and_warns_missing_media() {
    let dir = temp_test_dir("meraki-roundtrip");
    let existing_video = dir.join("cena_principal.mp4");
    let missing_video = dir.join("arquivo_movido.mp4");
    fs::write(&existing_video, b"dummy video bytes").ok();

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    app.set_preferences_path(dir.join("prefs.json"));

    let existing_id = app.import_media(existing_video.clone(), None, true).ok();
    let _missing_id = app.import_media(missing_video.clone(), None, false).ok();
    assert!(existing_id.is_some());

    if let Some(asset_id) = existing_id {
        assert!(app.send_asset_to_timeline(asset_id).is_ok());
    }
    assert!(app.rename_project("Documentário Teste".to_string()).is_ok());
    assert!(app.project_io().is_dirty);

    let saved_path = app
        .save_project_to_path(dir.join("meu_projeto"))
        .unwrap_or_else(|e| panic!("falha ao salvar: {e}"));
    assert_eq!(saved_path.extension().and_then(|e| e.to_str()), Some("meraki"));
    assert!(!app.project_io().is_dirty);
    assert_eq!(app.project_io().project_file_path.as_ref(), Some(&saved_path));

    let (engine2, worker2) = create_engine_channel();
    let mut reopened = MerakiApp::new(engine2);
    reopened.set_preferences_path(dir.join("prefs.json"));
    let missing = reopened
        .open_project_from_path(&saved_path)
        .unwrap_or_else(|e| panic!("falha ao abrir: {e}"));

    assert_eq!(reopened.project_name(), "Documentário Teste");
    assert_eq!(reopened.project().assets.len(), 2);
    let clip_count: usize = reopened.project().timeline.tracks.iter().map(|t| t.clips.len()).sum();
    assert_eq!(clip_count, 1);
    assert!(!reopened.history().can_undo());
    assert!(!reopened.project_io().is_dirty);
    assert_eq!(missing, vec![missing_video]);
    assert!(reopened
        .last_error()
        .is_some_and(|err| err.contains("Mídia ausente") && err.contains("arquivo_movido.mp4")));

    let mut synced = false;
    let mut imported_count = 0usize;
    while let Some(cmd) = worker2.try_recv_command() {
        match cmd {
            EngineCommand::SyncTimeline { .. } => synced = true,
            EngineCommand::ImportMedia { path, .. } => {
                assert_eq!(path, existing_video);
                imported_count += 1;
            }
            _ => {}
        }
    }
    assert!(synced);
    assert_eq!(imported_count, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dirty_flag_and_autosave_backup_clear_autosave_dirty_until_next_edit() {
    let dir = temp_test_dir("meraki-dirty");
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    app.set_preferences_path(dir.join("prefs.json"));

    assert!(!app.project_io().is_dirty);
    assert!(app.rename_project("Rascunho Auto".to_string()).is_ok());
    assert!(app.project_io().is_dirty);
    assert!(app.project_io().autosave_dirty);

    let wrote = app.perform_autosave_if_dirty();
    assert_eq!(wrote, Ok(true));
    assert!(autosave_path().is_file());
    assert!(!app.project_io().autosave_dirty);
    assert!(app.project_io().is_dirty);

    assert_eq!(app.perform_autosave_if_dirty(), Ok(false));

    assert!(app.save_project_to_path(dir.join("salvo.meraki")).is_ok());
    assert!(!app.project_io().is_dirty);
    assert!(!app.project_io().autosave_dirty);

    let _ = fs::remove_file(autosave_path());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_shortcuts_and_extension_normalization_work() {
    assert!(matches!(
        map_project_shortcut(Key::Character("s".into()), Modifiers::CTRL),
        Some(ProjectIoMessage::Save)
    ));
    assert!(matches!(
        map_project_shortcut(Key::Character("S".into()), Modifiers::CTRL | Modifiers::SHIFT),
        Some(ProjectIoMessage::SaveAs)
    ));
    assert!(matches!(
        map_project_shortcut(Key::Character("o".into()), Modifiers::CTRL),
        Some(ProjectIoMessage::Open)
    ));
    assert!(map_project_shortcut(Key::Named(Named::Space), Modifiers::empty()).is_none());

    assert_eq!(
        ensure_meraki_extension(PathBuf::from("video.final")),
        PathBuf::from("video.final.meraki")
    );
    assert_eq!(
        ensure_meraki_extension(PathBuf::from("pronto.meraki")),
        PathBuf::from("pronto.meraki")
    );
}

#[test]
fn rename_project_supports_undo_and_redo() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::ProjectIo(ProjectIoMessage::StartRename));
    let _ = app.update(Message::ProjectIo(ProjectIoMessage::NameDraftChanged(
        "Episódio 04".to_string(),
    )));
    let _ = app.update(Message::ProjectIo(ProjectIoMessage::CommitRename));

    assert_eq!(app.project_name(), "Episódio 04");
    assert!(!app.project_io().editing_name);
    assert!(app.project_io().is_dirty);

    assert!(app.undo());
    assert_eq!(app.project_name(), "Projeto sem título");

    assert!(app.redo());
    assert_eq!(app.project_name(), "Episódio 04");
}

#[test]
fn thumbnail_ready_and_proxy_progress_update_cache_and_resync_720p() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let asset_id = app
        .import_media(PathBuf::from("c:\\inexistente\\clip.mp4"), None, true)
        .unwrap_or_else(|e| panic!("falha import: {e}"));
    assert!(app.send_asset_to_timeline(asset_id).is_ok());

    let _ = app.update(Message::Playback(PlaybackMessage::SetPreviewQuality(
        PreviewQuality::Proxy720p,
    )));
    while worker.try_recv_command().is_some() {}

    let _ = app.update(Message::EngineEvent(EngineEvent::ThumbnailReady(ThumbnailFrame {
        asset_id,
        width: 2,
        height: 2,
        rgba_pixels: Bytes::from(vec![255; 16]),
    })));
    assert!(app.project_io().thumbnails.contains_key(&asset_id));

    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyProgress {
        asset_id,
        progress: 0.45,
        proxy_path: None,
    }));
    assert_eq!(app.project_io().proxy_progress.get(&asset_id), Some(&0.45));

    let proxy_path = PathBuf::from("c:\\inexistente\\clip_proxy_720p.mp4");
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyProgress {
        asset_id,
        progress: 1.0,
        proxy_path: Some(proxy_path.clone()),
    }));

    assert_eq!(
        app.project().asset(asset_id).and_then(|a| a.proxy_path.clone()),
        Some(proxy_path)
    );
    assert!(!app.project_io().proxy_progress.contains_key(&asset_id));

    let mut resynced_720p = false;
    while let Some(cmd) = worker.try_recv_command() {
        if matches!(
            cmd,
            EngineCommand::SyncTimeline {
                preview_quality: PreviewQuality::Proxy720p,
                ..
            }
        ) {
            resynced_720p = true;
        }
    }
    assert!(resynced_720p);
}
