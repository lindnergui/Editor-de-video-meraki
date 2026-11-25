use meraki_core::{ExportFrameRate, KeymapProfile, MediaMetadata, ShortcutAction};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::player::PlayerMessage;
use meraki_ui::components::timeline_canvas::TimelineCanvasMessage;
use meraki_ui::components::timeline_math::{
    time_ns_to_x, TimelineViewState, MAX_PX_PER_SEC, MIN_PX_PER_SEC,
};
use meraki_ui::components::tooltip::format_action_tooltip;
use meraki_ui::views::edit::EditMessage;
use meraki_ui::{DeliveryMessage, MerakiApp, Message, PlaybackMessage};
use std::path::PathBuf;

#[test]
fn tooltip_labels_format_dynamically_for_all_four_keymap_profiles() {
    assert_eq!(
        format_action_tooltip("Cortar na Agulha", KeymapProfile::PremierePro, ShortcutAction::SplitAtPlayhead),
        "Cortar na Agulha (Ctrl+K)"
    );
    assert_eq!(
        format_action_tooltip("Cortar na Agulha", KeymapProfile::DaVinciResolve, ShortcutAction::SplitAtPlayhead),
        "Cortar na Agulha (Ctrl+B)"
    );
    assert_eq!(
        format_action_tooltip("Cortar na Agulha", KeymapProfile::CapCut, ShortcutAction::SplitAtPlayhead),
        "Cortar na Agulha (Ctrl+B)"
    );
    assert_eq!(
        format_action_tooltip("Cortar na Agulha", KeymapProfile::VegasPro, ShortcutAction::SplitAtPlayhead),
        "Cortar na Agulha (S)"
    );

    assert_eq!(
        format_action_tooltip("Selecionar", KeymapProfile::PremierePro, ShortcutAction::ToolSelect),
        "Selecionar (V)"
    );
    assert_eq!(
        format_action_tooltip("Selecionar", KeymapProfile::DaVinciResolve, ShortcutAction::ToolSelect),
        "Selecionar (A)"
    );
    assert_eq!(
        format_action_tooltip("Selecionar", KeymapProfile::VegasPro, ShortcutAction::ToolSelect),
        "Selecionar (D)"
    );

    assert_eq!(
        format_action_tooltip("Imã / Snap", KeymapProfile::PremierePro, ShortcutAction::ToggleSnap),
        "Imã / Snap (S)"
    );
    assert_eq!(
        format_action_tooltip("Imã / Snap", KeymapProfile::CapCut, ShortcutAction::ToggleSnap),
        "Imã / Snap (N)"
    );
    assert_eq!(
        format_action_tooltip("Imã / Snap", KeymapProfile::VegasPro, ShortcutAction::ToggleSnap),
        "Imã / Snap (F8)"
    );

    assert_eq!(
        format_action_tooltip("Refazer", KeymapProfile::PremierePro, ShortcutAction::Redo),
        "Refazer (Ctrl+Shift+Z)"
    );
    assert_eq!(
        format_action_tooltip("Refazer", KeymapProfile::VegasPro, ShortcutAction::Redo),
        "Refazer (Ctrl+Y)"
    );
    assert_eq!(
        format_action_tooltip("Voltar 1 Quadro", KeymapProfile::PremierePro, ShortcutAction::StepFrameBack),
        "Voltar 1 Quadro (←)"
    );
    assert_eq!(
        format_action_tooltip("Avançar 1 Quadro", KeymapProfile::PremierePro, ShortcutAction::StepFrameForward),
        "Avançar 1 Quadro (→)"
    );
}

#[test]
fn toolbar_split_button_cuts_at_playhead_and_player_step_moves_exact_one_frame() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let asset_id = app
        .import_media(PathBuf::from("c:/inexistente/clip.mp4"), None, false)
        .unwrap_or_else(|e| panic!("import falhou: {e}"));
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    assert!(app.send_asset_to_timeline(asset_id).is_ok());

    // Avança e volta exatamente 1 quadro via botões do player (30fps -> ~33.33ms, depois 60fps -> ~16.66ms)
    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::StepForward)));
    assert_eq!(app.playhead_ns(), 1_000_000_000 / 30);
    assert_eq!(worker.take_pending_seek(), Some(1_000_000_000 / 30));

    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::StepBack)));
    assert_eq!(app.playhead_ns(), 0);

    let _ = app.update(Message::Delivery(DeliveryMessage::FrameRateChanged(ExportFrameRate::Fps60)));
    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::StepForward)));
    assert_eq!(app.playhead_ns(), 1_000_000_000 / 60);

    // Move para 2.5s e dispara diretamente o botão de corte da barra da timeline
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(2_500_000_000)));
    while worker.try_recv_command().is_some() {}

    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::SplitAtPlayhead)));
    assert_eq!(app.project().timeline.tracks[0].clips.len(), 2);
    assert_eq!(app.project().timeline.tracks[0].clips[0].duration_ns, 2_500_000_000);
    assert_eq!(app.project().timeline.tracks[0].clips[1].timeline_start_ns, 2_500_000_000);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
}

#[test]
fn anchored_zoom_preserves_playhead_screen_x_and_clamps_bounds() {
    let playhead_ns = 10_000_000_000_u64; // 10s
    let mut view = TimelineViewState {
        px_per_sec: 40.0,
        scroll_x_px: 120.0,
        ..TimelineViewState::default()
    };

    let initial_x = time_ns_to_x(playhead_ns, view.px_per_sec, view.scroll_x_px);
    view.zoom_anchored_at(1.25, playhead_ns);
    assert!((view.px_per_sec - 50.0).abs() < 1e-4);
    let zoomed_in_x = time_ns_to_x(playhead_ns, view.px_per_sec, view.scroll_x_px);
    assert!((zoomed_in_x - initial_x).abs() < 1e-3);

    view.zoom_anchored_at(0.8, playhead_ns);
    assert!((view.px_per_sec - 40.0).abs() < 1e-3);
    let zoomed_out_x = time_ns_to_x(playhead_ns, view.px_per_sec, view.scroll_x_px);
    assert!((zoomed_out_x - initial_x).abs() < 1e-3);

    // Fatores extremos respeitam limites e scroll >= 0
    view.zoom_anchored_at(100.0, 500_000_000);
    assert!((view.px_per_sec - MAX_PX_PER_SEC).abs() < 1e-4);
    assert!(view.scroll_x_px >= 0.0);

    view.zoom_anchored_at(0.001, 500_000_000);
    assert!((view.px_per_sec - MIN_PX_PER_SEC).abs() < 1e-4);
    assert!(view.scroll_x_px >= 0.0);

    // Roteamento via EditMessage::Timeline(ZoomAnchored)
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let before_px = app.edit_state().timeline.px_per_sec;
    let _ = app.update(Message::Edit(EditMessage::Timeline(
        TimelineCanvasMessage::ZoomAnchored { factor: 1.25, anchor_ns: 4_000_000_000 },
    )));
    assert!(app.edit_state().timeline.px_per_sec > before_px);
}
