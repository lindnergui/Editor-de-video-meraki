use meraki_core::{AppLanguage, AppPreferences, AssetId, KeymapProfile, MediaMetadata, Project, ProxyMode};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::app_project_io::autosave_path;
use meraki_ui::i18n;
use meraki_ui::project_storage::{
    create_managed_project_in, delete_project_permanently_with_prefs, scan_internal_projects_from,
};
use meraki_ui::views::project_hub::ProjectHubMessage;
use meraki_ui::{AppScreen, MerakiApp, Message, ProxyStatus, ScreenMessage};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn i18n_all_strings_non_empty_and_set_language_persists_live() {
    for lang in [AppLanguage::PtBr, AppLanguage::EnUs] {
        let dict = i18n::strings(lang);
        for (field, value) in dict.all_fields() {
            assert!(!value.trim().is_empty(), "campo i18n vazio em {lang:?}: {field}");
        }
        assert!(!i18n::format_delete_project(lang, "Ep01").trim().is_empty());
        assert!(!i18n::format_shortcuts_badge(lang, "CapCut").trim().is_empty());
        assert!(!i18n::format_proxy_cache_cleared(lang, 5 * 1024 * 1024).trim().is_empty());
    }
    assert_eq!(i18n::strings(AppLanguage::PtBr).topbar_export, "Exportar");
    assert_eq!(i18n::strings(AppLanguage::EnUs).topbar_export, "Export");

    let dir = temp_test_dir("meraki-v04-i18n");
    let prefs_file = dir.join("preferences.json");
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_preferences_path(engine, Project::new("Projeto"), prefs_file.clone());
    assert_eq!(app.preferences.language, AppLanguage::PtBr);

    let _ = app.update(Message::SetLanguage(AppLanguage::EnUs));
    assert_eq!(app.preferences.language, AppLanguage::EnUs);
    let raw = fs::read_to_string(&prefs_file).unwrap();
    let disk = AppPreferences::from_json(&raw).unwrap();
    assert_eq!(disk.language, AppLanguage::EnUs);

    let _ = app.update(Message::SetLanguage(AppLanguage::PtBr));
    assert_eq!(app.preferences.language, AppLanguage::PtBr);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_project_permanently_removes_file_autosave_and_prevents_rescan_reappearance() {
    let dir = temp_test_dir("meraki-v04-delete");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (proj_a, path_a) = create_managed_project_in(&projects_dir, "Episódio 01").unwrap();
    let (_, path_b) = create_managed_project_in(&projects_dir, "Episódio 02").unwrap();
    let sidecar_autosave = projects_dir.join(format!("{}.autosave.meraki", proj_a.id));
    fs::write(&sidecar_autosave, proj_a.to_json().unwrap()).unwrap();
    let global_autosave = autosave_path();
    fs::write(&global_autosave, proj_a.to_json().unwrap()).unwrap();

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(engine, Project::new("Base"), prefs_file.clone(), projects_dir.clone());
    assert!(app.complete_onboarding(KeymapProfile::DaVinciResolve).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.preferences.recent_projects.len(), 2);

    // 1. Abrir e cancelar confirmação mantém arquivo e histórico intactos
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RequestDeleteProject(path_a.clone()),
    )));
    assert_eq!(app.pending_delete_project().map(|p| &p.path), Some(&path_a));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::CancelDeleteProject,
    )));
    assert!(app.pending_delete_project().is_none());
    assert!(path_a.is_file());

    // 2. Abre o projeto A no editor e o exclui definitivamente pelo modal -> apaga disco, autosave e reseta editor
    assert!(app.open_project_from_path(&path_a).is_ok());
    assert_eq!(app.project_name(), "Episódio 01");
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RequestDeleteProject(path_a.clone()),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ConfirmDeleteProject,
    )));
    assert!(app.pending_delete_project().is_none());
    assert!(!path_a.exists());
    assert!(!sidecar_autosave.exists());
    assert!(!global_autosave.exists());
    assert!(app.project_io().project_file_path.is_none());
    assert_ne!(app.project().id, proj_a.id);

    // 3. Revarredura na pasta interna NÃO faz o projeto A reaparecer
    let rescanned = scan_internal_projects_from(&projects_dir, &mut app.preferences);
    assert_eq!(rescanned, 0);
    assert!(!app.preferences.recent_projects.iter().any(|r| r.path == path_a));
    assert_eq!(app.preferences.recent_projects.len(), 1);
    assert!(app.preferences.recent_projects.iter().any(|r| r.path == path_b));

    // 4. Também testa função pública direta com projeto B restante
    delete_project_permanently_with_prefs(&path_b, &prefs_file, &mut app.preferences).unwrap();
    assert!(!path_b.exists());
    assert_eq!(scan_internal_projects_from(&projects_dir, &mut app.preferences), 0);
    assert!(app.preferences.recent_projects.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proxy_status_mode_ensure_and_clear_cache_flow() {
    let dir = temp_test_dir("meraki-v04-proxy");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");
    let video_4k = dir.join("cena_4k.mp4");
    let video_hd = dir.join("cena_1080p.mp4");
    fs::write(&video_4k, b"4k").ok();
    fs::write(&video_hd, b"hd").ok();

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(engine, Project::new("Proxy"), prefs_file.clone(), projects_dir);
    assert_eq!(app.preferences.proxy_mode, ProxyMode::AutoAbove1080p);

    let id_4k = app.import_media(video_4k.clone(), None, true).unwrap();
    let id_hd = app.import_media(video_hd.clone(), None, true).unwrap();
    while worker.try_recv_command().is_some() {}

    // MediaDiscovered 4K dispara EnsureProxy em AutoAbove1080p; 1080p não dispara
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: id_4k,
        width: Some(3840),
        height: Some(2160),
        duration_ns: 4_000_000_000,
        ..MediaMetadata::default()
    })));
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: id_hd,
        width: Some(1920),
        height: Some(1080),
        duration_ns: 4_000_000_000,
        ..MediaMetadata::default()
    })));
    let mut ensured_ids = Vec::new();
    while let Some(cmd) = worker.try_recv_command() {
        if let EngineCommand::EnsureProxy { asset_id, .. } = cmd {
            ensured_ids.push(asset_id);
        }
    }
    assert_eq!(ensured_ids, vec![id_4k]);

    // ProxyGenerating -> ProxyReady -> ProxyFailed
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyGenerating { asset_id: id_4k }));
    assert_eq!(app.proxy_status(id_4k), Some(&ProxyStatus::Generating));

    let proxy_file = dir.join("proxy_4k.mp4");
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyReady {
        asset_id: id_4k,
        proxy_path: proxy_file.clone(),
    }));
    assert_eq!(app.proxy_status(id_4k), Some(&ProxyStatus::Ready));
    assert_eq!(app.project().asset(id_4k).and_then(|a| a.proxy_path.clone()), Some(proxy_file));

    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyFailed {
        asset_id: id_hd,
        reason: "codec".to_string(),
    }));
    assert_eq!(app.proxy_status(id_hd), Some(&ProxyStatus::Failed("codec".to_string())));

    // Mudar para Always salva preferences.json, envia SetProxyMode, EnsureProxy para 1080p e ressincroniza
    while worker.try_recv_command().is_some() {}
    let _ = app.update(Message::SetProxyMode(ProxyMode::Always));
    assert_eq!(app.preferences.proxy_mode, ProxyMode::Always);
    let mut saw_set_mode = false;
    let mut saw_sync = false;
    let mut saw_ensure_hd = false;
    while let Some(cmd) = worker.try_recv_command() {
        match cmd {
            EngineCommand::SetProxyMode(ProxyMode::Always) => saw_set_mode = true,
            EngineCommand::SyncTimeline { .. } => saw_sync = true,
            EngineCommand::EnsureProxy { asset_id, .. } if asset_id == id_hd => saw_ensure_hd = true,
            _ => {}
        }
    }
    assert!(saw_set_mode && saw_sync && saw_ensure_hd);

    // ClearProxyCache envia comando e ProxyCacheCleared exibe feedback discreto
    let _ = app.update(Message::ClearProxyCache);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::ClearProxyCache)));
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyCacheCleared { freed_bytes: 10 * 1024 * 1024 }));
    assert!(app.proxy_status(id_4k).is_none());
    assert!(app.project().asset(id_4k).unwrap().proxy_path.is_none());
    assert!(app.status_notice().is_some_and(|n| n.contains("10.0 MB")));
    assert_eq!(app.screen(), AppScreen::KeymapWelcome { return_to: meraki_ui::WelcomeReturnTarget::ProjectHub, can_go_back: false });

    let _ = fs::remove_dir_all(&dir);
}
