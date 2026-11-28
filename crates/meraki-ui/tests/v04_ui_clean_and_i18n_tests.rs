use meraki_core::{AppLanguage, AssetId, KeymapProfile, Project, ProxyMode};
use meraki_engine::protocol::create_engine_channel;
use meraki_ui::app_preferences::load_preferences_from;
use meraki_ui::i18n;
use meraki_ui::navigation::WorkspaceTab;
use meraki_ui::project_storage::{create_managed_project_in, scan_internal_projects_from};
use meraki_ui::views::project_hub::{format_unix_timestamp_localized, ProjectHubMessage};
use meraki_ui::{AppScreen, MerakiApp, Message, ScreenMessage, SettingsMenuMessage};
use std::fs;
use std::path::PathBuf;

fn temp_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", AssetId::new()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn settings_menu_language_toggle_updates_project_hub_and_editor_labels_live() {
    let dir = temp_test_dir("meraki-v04-ui-i18n");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Projeto"),
        prefs_file.clone(),
        projects_dir,
    );
    assert!(app.complete_onboarding(KeymapProfile::DaVinciResolve).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::ProjectHub);

    // 1. Abre o menu de configurações no ProjectHub e alterna para English (EnUs)
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ToggleSettings,
    )));
    assert!(app.is_settings_menu_open());
    let _ = app.update(Message::SetLanguage(AppLanguage::EnUs));
    assert_eq!(app.preferences.language, AppLanguage::EnUs);
    assert_eq!(load_preferences_from(&prefs_file).language, AppLanguage::EnUs);

    let en = i18n::strings(AppLanguage::EnUs);
    assert_eq!(en.hub_title, "Your Projects");
    assert_eq!(en.hub_new_project, "New Project");
    assert_eq!(en.settings_lang_pt, "Português (BR)");
    assert_eq!(en.settings_lang_en, "English");
    assert_eq!(en.settings_section_proxy, "PERFORMANCE");
    assert_eq!(en.settings_proxy_auto, "Auto 4K");
    assert_eq!(en.settings_proxy_mode_label, "Preview Proxies");
    assert_eq!(en.settings_clear_proxy_cache, "Clear proxy cache");
    assert_eq!(en.delete_modal_title, "Permanently delete project?");
    assert_eq!(en.delete_modal_confirm, "Delete project");
    assert_eq!(en.topbar_projects, "Projects");
    assert_eq!(en.topbar_export, "Export");
    assert_eq!(en.tab_files, "Files");
    assert_eq!(en.tab_edit, "Edit");
    assert_eq!(en.tab_effects, "Effects");
    assert_eq!(en.tab_audio, "Audio");
    assert_eq!(en.tab_delivery, "Delivery");
    assert_eq!(en.inspector_title, "INSPECTOR");
    assert_eq!(en.audio_mixer_title, "AUDIO MIXER");
    assert_eq!(en.delivery_export_video, "EXPORT VIDEO");
    assert_eq!(format_unix_timestamp_localized(0, AppLanguage::EnUs), "Recent");

    let _ = app.update(Message::SetProxyMode(ProxyMode::Always));
    let _ = app.view();

    // 2. Entra no Editor com Settings fechado e renderiza todas as 5 abas em EN
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::NewProjectClicked,
    )));
    assert_eq!(app.screen(), AppScreen::Editor);
    for tab in WorkspaceTab::ALL {
        let _ = app.update(Message::SelectTab(tab));
        let _ = app.view();
    }

    // 3. Reabre Settings dentro do Editor e volta para Português (BR)
    let _ = app.update(Message::Screen(ScreenMessage::SettingsMenu(
        SettingsMenuMessage::Toggle,
    )));
    assert!(app.is_settings_menu_open());
    let _ = app.update(Message::SetLanguage(AppLanguage::PtBr));
    assert_eq!(app.preferences.language, AppLanguage::PtBr);
    assert_eq!(load_preferences_from(&prefs_file).language, AppLanguage::PtBr);

    let pt = i18n::strings(AppLanguage::PtBr);
    assert_eq!(pt.hub_title, "Seus Projetos");
    assert_eq!(pt.settings_section_proxy, "DESEMPENHO");
    assert_eq!(pt.settings_proxy_mode_label, "Proxies de Prévia");
    assert_eq!(pt.settings_clear_proxy_cache, "Limpar cache de proxies");
    assert_eq!(pt.delete_modal_title, "Excluir projeto permanentemente?");
    assert_eq!(pt.delete_modal_confirm, "Excluir projeto");
    assert_eq!(pt.topbar_export, "Exportar");
    let _ = app.view();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_hub_trash_modal_cancel_preserves_and_confirm_deletes_permanently() {
    let dir = temp_test_dir("meraki-v04-ui-trash");
    let prefs_file = dir.join("preferences.json");
    let projects_dir = dir.join("projects");

    let (proj_a, path_a) = create_managed_project_in(&projects_dir, "Documentário").unwrap();
    let (_, path_b) = create_managed_project_in(&projects_dir, "Teaser").unwrap();
    let sidecar_autosave = projects_dir.join(format!("{}.autosave.meraki", proj_a.id));
    fs::write(&sidecar_autosave, proj_a.to_json().unwrap()).unwrap();

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::with_storage_paths(
        engine,
        Project::new("Base"),
        prefs_file.clone(),
        projects_dir.clone(),
    );
    assert!(app.complete_onboarding(KeymapProfile::PremierePro).is_ok());
    let _ = app.update(Message::Screen(ScreenMessage::OpenProjectHub));
    assert_eq!(app.screen(), AppScreen::ProjectHub);
    assert_eq!(app.preferences.recent_projects.len(), 2);

    // 1. Clicar lixeira abre o modal de confirmação sem vazar caminhos de disco
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RequestDeleteProject(path_a.clone()),
    )));
    assert_eq!(app.pending_delete_project().map(|p| &p.path), Some(&path_a));
    let desc_pt = i18n::format_delete_project_desc(AppLanguage::PtBr, "Documentário");
    let desc_en = i18n::format_delete_project_desc(AppLanguage::EnUs, "Documentário");
    assert!(desc_pt.contains("Documentário"));
    assert!(desc_pt.contains("Esta ação não pode ser desfeita."));
    assert!(!desc_pt.contains(&projects_dir.to_string_lossy().to_string()));
    assert!(desc_en.contains("Documentário"));
    assert!(desc_en.contains("This action cannot be undone."));
    let _ = app.view();

    // 2. Cancelar preserva projeto na lista e no disco
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::CancelDeleteProject,
    )));
    assert!(app.pending_delete_project().is_none());
    assert!(path_a.is_file());
    assert_eq!(app.preferences.recent_projects.len(), 2);

    // 3. Reabrir modal pela lixeira e confirmar remove da lista e do disco
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::RequestDeleteProject(path_a.clone()),
    )));
    let _ = app.update(Message::Screen(ScreenMessage::ProjectHub(
        ProjectHubMessage::ConfirmDeleteProject,
    )));
    assert!(app.pending_delete_project().is_none());
    assert!(!path_a.exists());
    assert!(!sidecar_autosave.exists());
    assert_eq!(app.preferences.recent_projects.len(), 1);
    assert_eq!(app.preferences.recent_projects[0].path, path_b);
    assert_eq!(load_preferences_from(&prefs_file).recent_projects.len(), 1);
    assert_eq!(scan_internal_projects_from(&projects_dir, &mut app.preferences), 0);
    assert!(path_b.is_file());

    let _ = app.view();
    let _ = fs::remove_dir_all(&dir);
}
