use iced::window;
use meraki_core::{AppPreferences, AssetId, KeymapProfile, Project};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::EngineCommand;
use meraki_ui::project_storage::{
    create_managed_project_in, delete_managed_project_if_internal, scan_internal_projects_from,
};
use meraki_ui::views::project_hub::ProjectHubMessage;
use meraki_ui::{
    AppScreen, ExitGuardMessage, MerakiApp, Message, PendingExitAction, ProjectIoMessage,
    ScreenMessage,
};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn managed_project_creation_and_save_work_without_dialog() {
    let dir = temp_test_dir("meraki-managed-storage");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (created_proj, created_path) = create_managed_project_in(&projects_dir, "Direto no Disco")
        .unwrap_or_else(|e| panic!("falha create_managed_project_in: {e}"));
    assert!(created_path.is_file());
    assert_eq!(created_path.parent(), Some(projects_dir.as_path()));
    assert_eq!(created_proj.name, "Direto no Disco");
    assert!(created_proj.timeline.tracks.iter().any(|t| t.name == "A1"));

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir.clone(),
    );
    assert!(app.complete_onboarding(KeymapProfile::PremierePro).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));

    // + Novo Projeto cria arquivo na pasta interna e entra limpo no Editor
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    let initial_path = app
        .project_io()
        .project_file_path
        .clone()
        .unwrap_or_else(|| panic!("projeto deveria ter project_file_path interno"));
    assert_eq!(initial_path.parent(), Some(projects_dir.as_path()));
    assert!(initial_path.is_file());
    assert!(!app.project_io().is_dirty);

    // Edita e aciona ProjectIoMessage::Save -> salva direto sem diálogo
    assert!(app.rename_project("Editado Sem Diálogo".to_string()).is_ok());
    assert!(app.project_io().is_dirty);
    let _ = app.update(Message::ProjectIo(ProjectIoMessage::Save));
    assert!(!app.project_io().is_dirty);

    let disk_json = fs::read_to_string(&initial_path)
        .unwrap_or_else(|e| panic!("falha ler projeto salvo: {e}"));
    let disk_proj =
        Project::from_json(&disk_json).unwrap_or_else(|e| panic!("falha parsear JSON: {e}"));
    assert_eq!(disk_proj.name, "Editado Sem Diálogo");

    // Mesmo se project_file_path fosse None, save_current_project cria na pasta interna sem diálogo
    let mut app2_dir = dir.join("fallback-projects");
    let _ = fs::create_dir_all(&app2_dir);
    let (engine2, _worker2) = create_engine_channel();
    let mut app2 = MerakiApp::with_storage_paths(
        engine2,
        Project::new("Sem Caminho Prévio"),
        dir.join("prefs2.json"),
        app2_dir.clone(),
    );
    app2.set_projects_dir(app2_dir.clone());
    let fallback_saved = app2
        .save_current_project()
        .unwrap_or_else(|e| panic!("falha save_current_project: {e}"));
    assert_eq!(fallback_saved.parent(), Some(app2_dir.as_path()));
    assert!(fallback_saved.is_file());

    // delete_managed_project_if_internal remove interno e preserva externo
    assert_eq!(
        delete_managed_project_if_internal(&projects_dir, &created_path),
        Ok(true)
    );
    assert!(!created_path.exists());
    assert_eq!(
        delete_managed_project_if_internal(&projects_dir, &fallback_saved),
        Ok(false)
    );
    assert!(fallback_saved.is_file());
    let _ = fs::remove_dir_all(&mut app2_dir);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_internal_projects_recovers_valid_files_when_preferences_missing() {
    let dir = temp_test_dir("meraki-scan-storage");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (_, p1) = create_managed_project_in(&projects_dir, "Episódio 01")
        .unwrap_or_else(|e| panic!("falha criar p1: {e}"));
    let (_, p2) = create_managed_project_in(&projects_dir, "Trailer Final")
        .unwrap_or_else(|e| panic!("falha criar p2: {e}"));
    fs::write(projects_dir.join("corrompido.meraki"), "{json quebrado").ok();
    fs::write(projects_dir.join("anotacoes.txt"), "ignorar").ok();

    let mut prefs = AppPreferences::default();
    let recovered = scan_internal_projects_from(&projects_dir, &mut prefs);
    assert_eq!(recovered, 2);
    assert!(prefs.recent_projects.iter().any(|r| r.path == p1 && r.name == "Episódio 01"));
    assert!(prefs.recent_projects.iter().any(|r| r.path == p2 && r.name == "Trailer Final"));

    // Startup sem preferences.json recupera automaticamente os projetos internos
    let (engine, _worker) = create_engine_channel();
    let app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir,
    );
    assert_eq!(app.preferences.recent_projects.len(), 2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn exit_guard_blocks_dirty_editor_and_handles_cancel_save_and_discard() {
    let dir = temp_test_dir("meraki-exit-guard");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir.clone(),
    );
    assert!(app.complete_onboarding(KeymapProfile::DaVinciResolve).is_ok());

    let (_, other_path) = create_managed_project_in(&projects_dir, "Outro Projeto")
        .unwrap_or_else(|e| panic!("falha criar outro projeto: {e}"));

    // Entra no editor com projeto limpo (is_dirty == false)
    app.reset_to_new_project();
    assert_eq!(app.screen(), AppScreen::Editor);
    assert!(!app.project_io().is_dirty);

    // 1. Com is_dirty == false: OpenProjectHub volta imediatamente sem modal
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(app.pending_exit().is_none());

    // Entra de novo em um projeto limpo e edita sem salvar
    app.reset_to_new_project();
    let managed_path = app
        .project_io()
        .project_file_path
        .clone()
        .unwrap_or_else(|| panic!("caminho interno esperado"));
    assert!(app.rename_project("Versão Salva 1".to_string()).is_ok());
    assert!(app.save_current_project().is_ok());
    assert!(!app.project_io().is_dirty);

    // Agora faz alteração pendente (is_dirty == true)
    assert!(app.rename_project("Alteração Pendente".to_string()).is_ok());
    assert!(app.project_io().is_dirty);

    // 2. WindowCloseRequested bloqueia e preenche pending_exit sem enviar Shutdown
    let win_id = window::Id::unique();
    let _ = app.update(Message::WindowCloseRequested(win_id));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.pending_exit(), Some(&PendingExitAction::CloseWindow(win_id)));
    while let Some(cmd) = worker.try_recv_command() {
        assert!(!matches!(cmd, EngineCommand::Shutdown));
    }

    // 3. Cancel apenas limpa pending_exit e permanece no Editor
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::Cancel));
    assert!(app.pending_exit().is_none());
    assert_eq!(app.screen(), AppScreen::Editor);
    assert!(app.project_io().is_dirty);

    // 4. OpenProjectHub bloqueia e SaveAndContinue persiste em disco antes de navegar
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.pending_exit(), Some(&PendingExitAction::ReturnToProjectHub));

    let _ = app.update(Message::ExitGuard(ExitGuardMessage::SaveAndContinue));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.project_io().is_dirty);
    assert!(app.pending_exit().is_none());
    let saved_raw = fs::read_to_string(&managed_path).unwrap_or_else(|e| panic!("ler salvo: {e}"));
    let saved_proj = Project::from_json(&saved_raw).unwrap_or_else(|e| panic!("parse salvo: {e}"));
    assert_eq!(saved_proj.name, "Alteração Pendente");

    // 5. Reabre, edita novamente e usa DiscardAndContinue -> volta sem sobrescrever disco
    assert!(app.open_project_from_path(&managed_path).is_ok());
    assert!(app.rename_project("Mudança Descartada".to_string()).is_ok());
    assert!(app.project_io().is_dirty);

    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.pending_exit(), Some(&PendingExitAction::ReturnToProjectHub));
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::DiscardAndContinue));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.project_io().is_dirty);
    assert!(app.pending_exit().is_none());

    let disk_after_discard = fs::read_to_string(&managed_path)
        .unwrap_or_else(|e| panic!("ler após discard: {e}"));
    let proj_after_discard = Project::from_json(&disk_after_discard)
        .unwrap_or_else(|e| panic!("parse após discard: {e}"));
    assert_eq!(proj_after_discard.name, "Alteração Pendente");

    // 6. Troca para outro projeto protegida e fechamento final enviando Shutdown
    assert!(app.open_project_from_path(&managed_path).is_ok());
    assert!(app.rename_project("Rascunho Temporário".to_string()).is_ok());
    let _ = app.request_exit_action(PendingExitAction::OpenProject(other_path.clone()));
    assert_eq!(
        app.pending_exit(),
        Some(&PendingExitAction::OpenProject(other_path))
    );
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::DiscardAndContinue));
    assert_eq!(app.project_name(), "Outro Projeto");

    // Fechamento final sem pendências dispara EngineCommand::Shutdown imediatamente
    let _ = app.update(Message::WindowCloseRequested(window::Id::unique()));
    assert!(app.pending_exit().is_none());
    let mut got_shutdown = false;
    while let Some(cmd) = worker.try_recv_command() {
        if matches!(cmd, EngineCommand::Shutdown) {
            got_shutdown = true;
        }
    }
    assert!(got_shutdown);

    let _ = fs::remove_dir_all(&dir);
}
