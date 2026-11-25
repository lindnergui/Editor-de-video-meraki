use iced::keyboard::key::{Key, Named};
use iced::keyboard::Modifiers;
use meraki_core::{AssetId, ExportFrameRate, KeymapProfile, MediaMetadata, TitleStyle, TrackKind};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::app_preferences::{load_preferences_from, save_preferences_to};
use meraki_ui::components::effects_library::EffectsLibraryMessage;
use meraki_ui::components::timeline_math::TimelineTool;
use meraki_ui::subscription::map_keyboard_shortcut;
use meraki_ui::views::effects::EffectsMessage;
use meraki_ui::{
    DeliveryMessage, MerakiApp, Message, PlaybackMessage, PrecisionMessage, ProjectIoMessage,
};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn map_keyboard_shortcut_respects_all_four_keymap_profiles_and_common_keys() {
    // Premiere Pro: Ctrl+K, V, S, Ctrl+Shift+Z
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::PremierePro, Key::Character("k".into()), Modifiers::CTRL),
        Some(Message::Precision(PrecisionMessage::SplitAtPlayhead))
    ));
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::PremierePro, Key::Character("v".into()), Modifiers::empty()),
        Some(Message::Precision(PrecisionMessage::SelectTool))
    ));
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::PremierePro, Key::Character("s".into()), Modifiers::empty()),
        Some(Message::Precision(PrecisionMessage::ToggleSnap))
    ));
    assert!(matches!(
        map_keyboard_shortcut(
            KeymapProfile::PremierePro,
            Key::Character("z".into()),
            Modifiers::CTRL | Modifiers::SHIFT
        ),
        Some(Message::Playback(PlaybackMessage::Redo))
    ));

    // DaVinci Resolve & CapCut: Ctrl+B, A, N, Ctrl+Shift+Z
    for profile in [KeymapProfile::DaVinciResolve, KeymapProfile::CapCut] {
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("b".into()), Modifiers::CTRL),
            Some(Message::Precision(PrecisionMessage::SplitAtPlayhead))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("a".into()), Modifiers::empty()),
            Some(Message::Precision(PrecisionMessage::SelectTool))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("n".into()), Modifiers::empty()),
            Some(Message::Precision(PrecisionMessage::ToggleSnap))
        ));
    }

    // VEGAS Pro: S, D, F8, Ctrl+Y
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::VegasPro, Key::Character("s".into()), Modifiers::empty()),
        Some(Message::Precision(PrecisionMessage::SplitAtPlayhead))
    ));
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::VegasPro, Key::Character("d".into()), Modifiers::empty()),
        Some(Message::Precision(PrecisionMessage::SelectTool))
    ));
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::VegasPro, Key::Named(Named::F8), Modifiers::empty()),
        Some(Message::Precision(PrecisionMessage::ToggleSnap))
    ));
    assert!(matches!(
        map_keyboard_shortcut(KeymapProfile::VegasPro, Key::Character("y".into()), Modifiers::CTRL),
        Some(Message::Playback(PlaybackMessage::Redo))
    ));

    // Comuns a todos os perfis
    for &profile in KeymapProfile::all() {
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::Space), Modifiers::empty()),
            Some(Message::Playback(PlaybackMessage::TogglePlayPause))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::ArrowLeft), Modifiers::empty()),
            Some(Message::Precision(PrecisionMessage::StepFrames(-1)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::ArrowRight), Modifiers::empty()),
            Some(Message::Precision(PrecisionMessage::StepFrames(1)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::ArrowLeft), Modifiers::SHIFT),
            Some(Message::Precision(PrecisionMessage::StepSeconds(-1)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::ArrowRight), Modifiers::SHIFT),
            Some(Message::Precision(PrecisionMessage::StepSeconds(1)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("z".into()), Modifiers::CTRL),
            Some(Message::Playback(PlaybackMessage::Undo))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Named(Named::Delete), Modifiers::empty()),
            Some(Message::Playback(PlaybackMessage::DeleteSelectedClip))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("s".into()), Modifiers::CTRL),
            Some(Message::ProjectIo(ProjectIoMessage::Save))
        ));
        assert!(matches!(
            map_keyboard_shortcut(profile, Key::Character("o".into()), Modifiers::CTRL),
            Some(Message::ProjectIo(ProjectIoMessage::Open))
        ));
    }
}

#[test]
fn split_at_playhead_respects_selection_priority_edges_and_undo_redo() {
    let dir = temp_test_dir("meraki-split");
    let video_path = dir.join("clip.mp4");
    let audio_path = dir.join("voz.wav");
    fs::write(&video_path, b"video").ok();
    fs::write(&audio_path, b"audio").ok();

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    app.set_preferences_path(dir.join("prefs.json"));
    let video_id = app.import_media(video_path, None, false).ok();
    let audio_id = app.import_media(audio_path, None, false).ok();
    assert!(video_id.is_some() && audio_id.is_some());

    if let Some(vid) = video_id {
        assert!(app.send_asset_to_timeline(vid).is_ok());
    }
    let a1_clip_id = audio_id.and_then(|aid| app.send_asset_to_timeline(aid).ok());
    assert!(a1_clip_id.is_some());

    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::AddTitle(TitleStyle::ModernText),
    )));

    // 1. Na borda exata (0ns), não corta nem gera erro
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(0)));
    assert!(app.split_at_playhead().is_none());
    assert!(app.last_error().is_none());

    // 2. Com A1 selecionado explicitamente em 1.5s, corta A1 mesmo existindo V1 e T1
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(a1_clip_id)));
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(1_500_000_000)));
    while worker.try_recv_command().is_some() {}
    let right_a1 = app.split_at_playhead();
    assert!(right_a1.is_some());
    assert_eq!(app.project().timeline.tracks[1].clips.len(), 2);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // 3. Sem seleção em 2.5s, prioriza Video (V1) antes de Overlay (T1) e Audio (A1)
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(None)));
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(2_500_000_000)));
    let right_v1 = app.split_at_playhead();
    assert!(right_v1.is_some());
    let count_kind = |app: &MerakiApp, kind: TrackKind| -> usize {
        app.project()
            .timeline
            .tracks
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.clips.len())
            .sum()
    };
    assert_eq!(count_kind(&app, TrackKind::Video), 2);

    // 4. Novo corte exatamente em 2.5s (agora borda em V1) cai automaticamente para Overlay (T1)
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(None)));
    let right_t1 = app.split_at_playhead();
    assert!(right_t1.is_some());
    assert_eq!(count_kind(&app, TrackKind::Overlay), 2);

    // Undo / Redo restaura e reaplica o corte de T1
    assert!(app.undo());
    assert_eq!(count_kind(&app, TrackKind::Overlay), 1);
    assert!(app.redo());
    assert_eq!(count_kind(&app, TrackKind::Overlay), 2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn step_playhead_frames_and_second_jumps_clamp_and_seek_accurately() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let asset_id = app
        .import_media(PathBuf::from("c:/inexistente/clip.mp4"), None, false)
        .unwrap_or_else(|e| panic!("import falhou: {e}"));
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 2_000_000_000,
        ..MediaMetadata::default()
    })));
    assert!(app.send_asset_to_timeline(asset_id).is_ok());
    assert_eq!(app.project_fps(), 30);

    let frame_ns = 1_000_000_000_u64 / 30;
    assert_eq!(app.step_playhead_frames(1), frame_ns);
    assert_eq!(app.playhead_ns(), frame_ns);
    assert_eq!(worker.take_pending_seek(), Some(frame_ns));

    assert_eq!(app.step_playhead_frames(-1), 0);
    assert_eq!(app.step_playhead_frames(-5), 0);

    let _ = app.update(Message::Precision(PrecisionMessage::StepSeconds(1)));
    assert_eq!(app.playhead_ns(), frame_ns * 30);

    let _ = app.update(Message::Precision(PrecisionMessage::StepSeconds(5)));
    assert_eq!(app.playhead_ns(), 2_000_000_000);

    // Atalhos de ferramenta e snap atualizam ambas as timelines
    app.edit_state();
    let _ = app.update(Message::Precision(PrecisionMessage::ToggleSnap));
    assert!(!app.edit_state().timeline.snap_enabled);
    assert!(!app.effects_state().timeline.snap_enabled);
    let _ = app.update(Message::Precision(PrecisionMessage::SelectTool));
    assert_eq!(app.edit_state().timeline.tool, TimelineTool::Select);

    // ExportFrameRate 60 altera o tamanho do passo de frame
    let _ = app.update(Message::Delivery(DeliveryMessage::FrameRateChanged(
        ExportFrameRate::Fps60,
    )));
    assert_eq!(app.project_fps(), 60);
    let step_60 = app.step_playhead_frames(1);
    let _ = app.update(Message::Precision(PrecisionMessage::StepSeconds(-1)));
    assert!(step_60 <= 2_000_000_000);
}

#[test]
fn preferences_persistence_recovers_corrupted_json_and_records_recent_projects() {
    let dir = temp_test_dir("meraki-prefs");
    let prefs_file = dir.join("nested").join("preferences.json");

    let missing_loaded = load_preferences_from(&prefs_file);
    assert_eq!(missing_loaded.keymap, KeymapProfile::PremierePro);

    fs::create_dir_all(dir.join("nested")).ok();
    fs::write(&prefs_file, b"{not valid json").ok();
    let corrupted_loaded = load_preferences_from(&prefs_file);
    assert_eq!(corrupted_loaded.keymap, KeymapProfile::PremierePro);

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    app.set_preferences_path(prefs_file.clone());
    assert!(app.set_keymap_profile(KeymapProfile::VegasPro).is_ok());
    assert!(app.rename_project("Projeto Recente".to_string()).is_ok());

    let project_path = app
        .save_project_to_path(dir.join("projeto_teste.meraki"))
        .unwrap_or_else(|e| panic!("falha ao salvar: {e}"));

    let disk_prefs = load_preferences_from(&prefs_file);
    assert_eq!(disk_prefs.keymap, KeymapProfile::VegasPro);
    assert_eq!(disk_prefs.recent_projects.len(), 1);
    assert_eq!(disk_prefs.recent_projects[0].name, "Projeto Recente");
    assert_eq!(disk_prefs.recent_projects[0].path, project_path);

    let mut manual = disk_prefs.clone();
    manual.onboarding_completed = true;
    assert!(save_preferences_to(&prefs_file, &manual).is_ok());
    assert!(load_preferences_from(&prefs_file).onboarding_completed);

    let _ = fs::remove_dir_all(&dir);
}
