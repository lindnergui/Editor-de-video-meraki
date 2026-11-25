use meraki_core::{AssetId, KeymapProfile, Project};
use meraki_engine::protocol::create_engine_channel;
use meraki_ui::views::keymap_welcome::KeymapWelcomeMessage;
use meraki_ui::views::project_hub::ProjectHubMessage;
use meraki_ui::{
    AppScreen, ExitGuardMessage, MerakiApp, Message, PendingExitAction, ScreenMessage,
    SettingsMenuMessage, WelcomeReturnTarget,
};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn settings_menu_opens_closes_and_saves_in_hub_and_editor() {
    let dir = temp_test_dir("meraki-settings-menu");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir,
    );
    assert!(app.complete_onboarding(KeymapProfile::PremierePro).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.is_settings_menu_open());

    // Abre e fecha pelo botão de engrenagem no ProjectHub
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ToggleSettings,
    )));
    assert!(app.is_settings_menu_open());
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Close,
    )));
    assert!(!app.is_settings_menu_open());

    // Entra no Editor, edita e salva pelo menu de configurações
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert!(!app.is_settings_menu_open());

    assert!(app.rename_project("Salvo Via Engrenagem".to_string()).is_ok());
    assert!(app.project_io().is_dirty);

    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Toggle,
    )));
    assert!(app.is_settings_menu_open());

    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::SaveNow,
    )));
    assert!(!app.is_settings_menu_open());
    assert!(!app.project_io().is_dirty);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn settings_menu_shortcuts_preserves_return_target_for_hub_and_editor() {
    let dir = temp_test_dir("meraki-settings-shortcuts");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir,
    );
    assert!(app.complete_onboarding(KeymapProfile::PremierePro).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));

    // A partir do ProjectHub -> abre Atalhos e volta para ProjectHub
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ToggleSettings,
    )));
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::OpenShortcuts,
    )));
    assert!(!app.is_settings_menu_open());
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

    // A partir do Editor -> abre Atalhos, troca para CapCut e volta para Editor
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    assert!(app.rename_project("Com Atalhos CapCut".to_string()).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Toggle,
    )));
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::OpenShortcuts,
    )));
    assert!(!app.is_settings_menu_open());
    assert_eq!(
        app.screen(),
        AppScreen::KeymapWelcome {
            return_to: WelcomeReturnTarget::Editor,
            can_go_back: true,
        }
    );
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::SelectProfile(KeymapProfile::CapCut),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::KeymapWelcome(
        KeymapWelcomeMessage::Continue,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(app.preferences.keymap, KeymapProfile::CapCut);
    assert_eq!(app.project_name(), "Com Atalhos CapCut");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn topbar_projects_button_triggers_unsaved_modal_and_handles_all_three_choices() {
    let dir = temp_test_dir("meraki-unsaved-modal");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto sem título"),
        prefs_file,
        projects_dir,
    );
    assert!(app.complete_onboarding(KeymapProfile::DaVinciResolve).is_ok());

    app.reset_to_new_project();
    let saved_path = app
        .project_io()
        .project_file_path
        .clone()
        .unwrap_or_else(|| panic!("caminho interno esperado"));
    assert!(app.rename_project("Versão Base".to_string()).is_ok());
    assert!(app.save_current_project().is_ok());

    // Altera e deixa o menu de engrenagem aberto, depois clica no botão Projetos
    assert!(app.rename_project("Alteração Pendente".to_string()).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Toggle,
    )));
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert!(!app.is_settings_menu_open());
    assert_eq!(app.screen(), AppScreen::Editor);
    assert_eq!(
        app.pending_exit(),
        Some(&PendingExitAction::ReturnToProjectHub)
    );

    // 1. Cancelar
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::Cancel));
    assert!(app.pending_exit().is_none());
    assert_eq!(app.screen(), AppScreen::Editor);
    assert!(app.project_io().is_dirty);

    // 2. Salvar e Sair
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::SaveAndContinue));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.project_io().is_dirty);
    let raw = fs::read_to_string(&saved_path).unwrap_or_else(|e| panic!("ler salvo: {e}"));
    let proj = Project::from_json(&raw).unwrap_or_else(|e| panic!("parse salvo: {e}"));
    assert_eq!(proj.name, "Alteração Pendente");

    // 3. Reabre, edita e escolhe Sair sem Salvar -> mantém nome salvo anterior
    assert!(app.open_project_from_path(&saved_path).is_ok());
    assert!(app.rename_project("Descartar Isto".to_string()).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(
        app.pending_exit(),
        Some(&PendingExitAction::ReturnToProjectHub)
    );
    let _ = app.update(Message::ExitGuard(ExitGuardMessage::DiscardAndContinue));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert!(!app.project_io().is_dirty);
    let raw2 = fs::read_to_string(&saved_path).unwrap_or_else(|e| panic!("ler pós-descarte: {e}"));
    let proj2 = Project::from_json(&raw2).unwrap_or_else(|e| panic!("parse pós-descarte: {e}"));
    assert_eq!(proj2.name, "Alteração Pendente");

    let _ = fs::remove_dir_all(&dir);
}
