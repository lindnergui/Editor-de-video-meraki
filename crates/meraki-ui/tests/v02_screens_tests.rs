use meraki_core::{AssetId, KeymapProfile, Project};
use meraki_engine::protocol::create_engine_channel;
use meraki_ui::app_preferences::load_preferences_from;
use meraki_ui::views::keymap_welcome::KeymapWelcomeMessage;
use meraki_ui::views::project_hub::{
    build_grid_row_slot_counts, filtered_recent_projects, format_short_path, format_unix_timestamp,
    ProjectHubMessage,
};
use meraki_ui::{
    AppScreen, ExitGuardMessage, MerakiApp, Message, PendingExitAction, ScreenMessage,
    WelcomeReturnTarget,
};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn first_launch_opens_keymap_welcome_first_and_persists_into_project_hub() {
    let dir = temp_test_dir("meraki-v02-first");
    let prefs_file = dir.join("preferences.json");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_preferences_path(
        engine,
        Project::new("Projeto sem título"),
        prefs_file.clone(),
    );

    // 1. Primeiro acesso: começa na tela de escolha de atalhos sem opção Voltar
    assert_eq!(
        app.screen(),
        AppScreen::KeymapWelcome {
            return_to: WelcomeReturnTarget::ProjectHub,
            can_go_back: false,
        }
    );

    // Tentar voltar quando can_go_back == false não sai da tela inicial
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Back,
    )));
    assert_eq!(
        app.screen(),
        AppScreen::KeymapWelcome {
            return_to: WelcomeReturnTarget::ProjectHub,
            can_go_back: false,
        }
    );

    // 2. Escolhe DaVinci Resolve e clica Continuar -> salva preferências e vai para ProjectHub
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::SelectProfile(KeymapProfile::DaVinciResolve),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Continue,
    )));

    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(app.preferences.onboarding_completed);
    assert_eq!(app.preferences.keymap, KeymapProfile::DaVinciResolve);

    let persisted = load_preferences_from(&prefs_file);
    assert!(persisted.onboarding_completed);
    assert_eq!(persisted.keymap, KeymapProfile::DaVinciResolve);

    // 3. Clicar + Novo Projeto no ProjectHub entra diretamente no Editor
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);

    // 4. Segunda abertura com preferences.json já salvo inicia direto no ProjectHub
    let (engine2, _worker2) = create_engine_channel();
    let app2 = MerakiApp::with_preferences_path(
        engine2,
        Project::new("Projeto sem título"),
        prefs_file.clone(),
    );
    assert_eq!(app2.screen(), AppScreen::ProjectHub);
    assert_eq!(app2.preferences.keymap, KeymapProfile::DaVinciResolve);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reopening_shortcuts_from_hub_and_editor_allows_back_and_preserves_project() {
    let dir = temp_test_dir("meraki-v02-return");
    let prefs_file = dir.join("preferences.json");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_preferences_path(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
    );
    assert!(app.complete_onboarding(KeymapProfile::PremierePro).is_ok());

    // Abre Atalhos manualmente a partir do ProjectHub -> can_go_back == true e volta para ProjectHub
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::OpenShortcuts,
    )));
    assert_eq!(
        app.screen(),
        AppScreen::KeymapWelcome {
            return_to: WelcomeReturnTarget::ProjectHub,
            can_go_back: true,
        }
    );
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Back,
    )));
    assert_eq!(app.screen(), AppScreen::ProjectHub);

    // Cria projeto e edita no Editor
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    let asset_id = app
        .import_media(PathBuf::from("c:/inexistente/clip.mp4"), None, false)
        .unwrap_or_else(|e| panic!("import falhou: {e}"));
    assert!(app.send_asset_to_timeline(asset_id).is_ok());
    assert!(app.rename_project("Corte Preso".to_string()).is_ok());
    assert!(app.project_io().is_dirty);

    // Abre Atalhos a partir do Editor e clica Voltar
    let _ = app.update(Message::Screen(ScreenMessage::OpenKeymapWelcome(
        WelcomeReturnTarget::Editor,
    )));
    assert_eq!(
        app.screen(),
        AppScreen::KeymapWelcome {
            return_to: WelcomeReturnTarget::Editor,
            can_go_back: true,
        }
    );
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Back,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.project_name(), "Corte Preso");
    assert_eq!(app.project().timeline.tracks[0].clips.len(), 1);

    // Reabre Atalhos, troca para VegasPro e clica Continuar -> preserva projeto aberto
    let _ = app.update(Message::Screen(ScreenMessage::OpenKeymapWelcome(
        WelcomeReturnTarget::Editor,
    )));
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::SelectProfile(KeymapProfile::VegasPro),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Continue,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.preferences.keymap, KeymapProfile::VegasPro);
    assert_eq!(app.project_name(), "Corte Preso");
    assert_eq!(app.project().timeline.tracks[0].clips.len(), 1);

    // Volta ao ProjectHub com alterações pendentes -> abre guarda e Salvar e Continuar conclui
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.pending_exit(), Some(&PendingExitAction::ReturnToProjectHub));
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::SaveAndContinue));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.project_io().is_dirty);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_hub_search_inline_rename_remove_and_three_column_grid_work() {
    let dir = temp_test_dir("meraki-v02-hub");
    let prefs_file = dir.join("preferences.json");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_preferences_path(
        engine,
        Project::new("Projeto sem título"),
        prefs_file.clone(),
    );
    assert!(app.complete_onboarding(KeymapProfile::CapCut).is_ok());

    assert!(app.rename_project("Alpha".to_string()).is_ok());
    let alpha_path = app
        .save_project_to_path(dir.join("alpha.meraki"))
        .unwrap_or_else(|e| panic!("falha salvar alpha: {e}"));

    app.reset_to_new_project();
    assert!(app.rename_project("Beta".to_string()).is_ok());
    let beta_path = app
        .save_current_project()
        .unwrap_or_else(|e| panic!("falha salvar beta: {e}"));

    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.preferences.recent_projects.len(), 2);

    // Busca no hub
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::SearchChanged("alpha".to_string()),
    )));
    let hits = filtered_recent_projects(
        &app.preferences.recent_projects,
        &app.hub_state().search_query,
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, alpha_path);

    // Renomeação inline atualiza preferences.json e arquivo .meraki no disco
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::StartRename {
            path: alpha_path.clone(),
            current_name: "Alpha".to_string(),
        },
    )));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RenameDraftChanged("Alpha Revisado".to_string()),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::CommitRename,
    )));

    let disk_prefs = load_preferences_from(&prefs_file);
    assert!(disk_prefs
        .recent_projects
        .iter()
        .any(|e| e.path == alpha_path && e.name == "Alpha Revisado"));
    let disk_raw = fs::read_to_string(&alpha_path).unwrap_or_else(|e| panic!("ler alpha: {e}"));
    let disk_proj = Project::from_json(&disk_raw).unwrap_or_else(|e| panic!("parse alpha: {e}"));
    assert_eq!(disk_proj.name, "Alpha Revisado");

    // Remover da lista de recentes sem apagar arquivo .meraki do disco
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RemoveRecent(beta_path.clone()),
    )));
    assert!(beta_path.is_file());
    assert_eq!(app.preferences.recent_projects.len(), 1);
    assert_eq!(load_preferences_from(&prefs_file).recent_projects.len(), 1);

    // Grade 3xN com preenchimento exato da última linha com espaços vazios
    assert_eq!(build_grid_row_slot_counts(0), Vec::<(usize, usize)>::new());
    assert_eq!(build_grid_row_slot_counts(1), vec![(1, 2)]);
    assert_eq!(build_grid_row_slot_counts(2), vec![(2, 1)]);
    assert_eq!(build_grid_row_slot_counts(3), vec![(3, 0)]);
    assert_eq!(build_grid_row_slot_counts(4), vec![(3, 0), (1, 2)]);
    assert_eq!(build_grid_row_slot_counts(5), vec![(3, 0), (2, 1)]);
    assert_eq!(build_grid_row_slot_counts(6), vec![(3, 0), (3, 0)]);
    assert_eq!(build_grid_row_slot_counts(7), vec![(3, 0), (3, 0), (1, 2)]);

    // Formatadores auxiliares
    assert!(format_short_path(&alpha_path).ends_with("alpha.meraki"));
    assert_eq!(format_unix_timestamp(1_700_000_000), "14/11/2023 22:13");

    let _ = fs::remove_dir_all(&dir);
}
