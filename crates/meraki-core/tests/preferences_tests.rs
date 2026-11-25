use meraki_core::{AppPreferences, KeymapProfile, ShortcutAction, MAX_RECENT_PROJECTS};
use std::path::PathBuf;

#[test]
fn all_four_keymap_profiles_expose_expected_metadata_and_shortcut_labels() {
    assert_eq!(KeymapProfile::default(), KeymapProfile::PremierePro);
    let all = KeymapProfile::all();
    assert_eq!(all.len(), 4);

    assert_eq!(KeymapProfile::PremierePro.display_name(), "Adobe Premiere Pro");
    assert_eq!(KeymapProfile::DaVinciResolve.display_name(), "DaVinci Resolve");
    assert_eq!(KeymapProfile::CapCut.display_name(), "CapCut");
    assert_eq!(KeymapProfile::VegasPro.display_name(), "VEGAS Pro");

    for profile in all {
        assert!(!profile.subtitle().trim().is_empty());
        for &action in ShortcutAction::all() {
            assert!(!profile.shortcut_label(action).is_empty());
        }
        assert_eq!(profile.shortcut_label(ShortcutAction::PlayPause), "Space");
        assert_eq!(profile.shortcut_label(ShortcutAction::StepFrameBack), "←");
        assert_eq!(profile.shortcut_label(ShortcutAction::StepFrameForward), "→");
        assert_eq!(profile.shortcut_label(ShortcutAction::Undo), "Ctrl+Z");
        assert_eq!(profile.shortcut_label(ShortcutAction::DeleteClip), "Delete");
        assert_eq!(profile.shortcut_label(ShortcutAction::SaveProject), "Ctrl+S");
        assert_eq!(profile.shortcut_label(ShortcutAction::OpenProject), "Ctrl+O");
    }

    assert_eq!(KeymapProfile::PremierePro.shortcut_label(ShortcutAction::SplitAtPlayhead), "Ctrl+K");
    assert_eq!(KeymapProfile::DaVinciResolve.shortcut_label(ShortcutAction::SplitAtPlayhead), "Ctrl+B");
    assert_eq!(KeymapProfile::CapCut.shortcut_label(ShortcutAction::SplitAtPlayhead), "Ctrl+B");
    assert_eq!(KeymapProfile::VegasPro.shortcut_label(ShortcutAction::SplitAtPlayhead), "S");

    assert_eq!(KeymapProfile::PremierePro.shortcut_label(ShortcutAction::ToolSelect), "V");
    assert_eq!(KeymapProfile::DaVinciResolve.shortcut_label(ShortcutAction::ToolSelect), "A");
    assert_eq!(KeymapProfile::CapCut.shortcut_label(ShortcutAction::ToolSelect), "A");
    assert_eq!(KeymapProfile::VegasPro.shortcut_label(ShortcutAction::ToolSelect), "D");

    assert_eq!(KeymapProfile::PremierePro.shortcut_label(ShortcutAction::ToggleSnap), "S");
    assert_eq!(KeymapProfile::DaVinciResolve.shortcut_label(ShortcutAction::ToggleSnap), "N");
    assert_eq!(KeymapProfile::CapCut.shortcut_label(ShortcutAction::ToggleSnap), "N");
    assert_eq!(KeymapProfile::VegasPro.shortcut_label(ShortcutAction::ToggleSnap), "F8");

    assert_eq!(KeymapProfile::PremierePro.shortcut_label(ShortcutAction::Redo), "Ctrl+Shift+Z");
    assert_eq!(KeymapProfile::DaVinciResolve.shortcut_label(ShortcutAction::Redo), "Ctrl+Shift+Z");
    assert_eq!(KeymapProfile::CapCut.shortcut_label(ShortcutAction::Redo), "Ctrl+Shift+Z");
    assert_eq!(KeymapProfile::VegasPro.shortcut_label(ShortcutAction::Redo), "Ctrl+Y");
}

#[test]
fn upsert_recent_orders_deduplicates_preserves_id_and_enforces_cap() {
    let mut prefs = AppPreferences::default();

    for i in 0..25_u64 {
        prefs.upsert_recent(
            format!("Projeto {i}"),
            PathBuf::from(format!("/projetos/p_{i}.meraki")),
            1_000 + i,
        );
    }

    assert_eq!(prefs.recent_projects.len(), MAX_RECENT_PROJECTS);
    assert_eq!(prefs.recent_projects[0].name, "Projeto 24");
    assert_eq!(prefs.recent_projects[19].name, "Projeto 5");

    let existing_path = PathBuf::from("/projetos/p_10.meraki");
    let existing_id = prefs
        .recent_projects
        .iter()
        .find(|e| e.path == existing_path)
        .map(|e| e.id);
    assert!(existing_id.is_some());

    prefs.upsert_recent("Projeto 10 Atualizado".to_string(), existing_path.clone(), 9_999);
    assert_eq!(prefs.recent_projects.len(), MAX_RECENT_PROJECTS);
    assert_eq!(prefs.recent_projects[0].path, existing_path);
    assert_eq!(prefs.recent_projects[0].name, "Projeto 10 Atualizado");
    assert_eq!(prefs.recent_projects[0].last_opened_unix_secs, 9_999);
    assert_eq!(Some(prefs.recent_projects[0].id), existing_id);
    assert_eq!(
        prefs
            .recent_projects
            .iter()
            .filter(|e| e.path == existing_path)
            .count(),
        1
    );
}

#[test]
fn rename_remove_and_json_roundtrip_preserve_state_and_legacy_defaults() {
    let mut prefs = AppPreferences {
        keymap: KeymapProfile::DaVinciResolve,
        onboarding_completed: true,
        recent_projects: Vec::new(),
    };
    let path_a = PathBuf::from("c:/videos/a.meraki");
    let path_b = PathBuf::from("c:/videos/b.meraki");
    prefs.upsert_recent("Filme A".to_string(), path_a.clone(), 100);
    prefs.upsert_recent("Filme B".to_string(), path_b.clone(), 200);

    assert!(prefs.rename_recent(&path_a, "  Filme A v2 ".to_string()));
    assert!(!prefs.rename_recent(&path_a, "   ".to_string()));
    assert!(!prefs.rename_recent(&PathBuf::from("inexistente.meraki"), "X".to_string()));
    assert_eq!(prefs.recent_projects[1].name, "Filme A v2");

    let json = prefs.to_json().unwrap_or_else(|e| panic!("to_json falhou: {e}"));
    let restored = AppPreferences::from_json(&json).unwrap_or_else(|e| panic!("from_json falhou: {e}"));
    assert_eq!(restored, prefs);

    assert!(prefs.remove_recent(&path_a));
    assert!(!prefs.remove_recent(&path_a));
    assert_eq!(prefs.recent_projects.len(), 1);
    assert_eq!(prefs.recent_projects[0].path, path_b);

    let from_empty = AppPreferences::from_json("{}").unwrap_or_else(|e| panic!("empty json falhou: {e}"));
    assert_eq!(from_empty.keymap, KeymapProfile::PremierePro);
    assert!(!from_empty.onboarding_completed);
    assert!(from_empty.recent_projects.is_empty());

    let partial = r#"{"keymap":"VegasPro"}"#;
    let from_partial = AppPreferences::from_json(partial).unwrap_or_else(|e| panic!("partial json falhou: {e}"));
    assert_eq!(from_partial.keymap, KeymapProfile::VegasPro);
    assert!(!from_partial.onboarding_completed);
    assert!(from_partial.recent_projects.is_empty());
}
