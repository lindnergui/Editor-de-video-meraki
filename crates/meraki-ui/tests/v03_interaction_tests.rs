use iced::keyboard::{Key, Modifiers};
use iced::Point;
use meraki_core::{
    ClipEffect, ClipTransform, EffectKind, EffectParameters, KeymapProfile, MediaMetadata,
    ZoomEasing, ZoomParams,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::effects_inspector::EffectsInspectorMessage;
use meraki_ui::components::inspector::InspectorMessage;
use meraki_ui::components::player::format_shuttle_badge;
use meraki_ui::components::resettable_slider::{is_double_click, DOUBLE_CLICK_WINDOW_MS};
use meraki_ui::components::timeline_canvas::{compute_effect_badges, TimelineCanvasMessage};
use meraki_ui::components::timeline_interaction::{TimelineInteraction, CLIP_DRAG_THRESHOLD_PX};
use meraki_ui::components::timeline_math::{
    time_ns_to_x, TimelineViewState, RULER_HEIGHT, TRACK_HEIGHT,
};
use meraki_ui::subscription::map_keyboard_shortcut;
use meraki_ui::views::edit::EditMessage;
use meraki_ui::views::effects::EffectsMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn dispatch_timeline_msg(app: &mut MerakiApp, msg: Option<TimelineCanvasMessage>) {
    if let Some(m) = msg {
        let _ = app.update(Message::Edit(EditMessage::Timeline(m)));
    }
}

#[test]
fn double_click_timing_and_individual_vs_full_reset_with_undo() {
    let now = Instant::now();
    assert!(!is_double_click(None, now));
    assert!(is_double_click(Some(now - Duration::from_millis(DOUBLE_CLICK_WINDOW_MS - 50)), now));
    assert!(!is_double_click(Some(now - Duration::from_millis(DOUBLE_CLICK_WINDOW_MS)), now));

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let vid = app.import_media(PathBuf::from("clipe.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: vid,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(vid).unwrap();

    // Edita múltiplos campos
    let edited = ClipTransform { scale: 1.8, pos_x: 120.0, pos_y: -40.0, opacity: 0.65, speed: 1.5 };
    let _ = app.update(Message::Edit(EditMessage::Inspector(InspectorMessage::UpdateTransform(clip_id, edited))));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, edited);

    // Duplo-clique individual em Escala reseta APENAS scale preservando demais campos
    let only_scale_reset = ClipTransform { scale: ClipTransform::default().scale, ..edited };
    let _ = app.update(Message::Edit(EditMessage::Inspector(
        InspectorMessage::UpdateTransform(clip_id, only_scale_reset),
    )));
    let after_scale_reset = app.project().timeline.clip(clip_id).unwrap().1.transform;
    assert_eq!(after_scale_reset.scale, 1.0);
    assert_eq!(after_scale_reset.pos_x, 120.0);
    assert_eq!(after_scale_reset.pos_y, -40.0);
    assert_eq!(after_scale_reset.opacity, 0.65);
    assert_eq!(after_scale_reset.speed, 1.5);

    // Undo restaura o reset individual de Escala
    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.8);

    // Botão Resetar geral volta todos os campos para o default e aceita Undo
    let _ = app.update(Message::Edit(EditMessage::Inspector(
        InspectorMessage::UpdateTransform(clip_id, ClipTransform::default()),
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, ClipTransform::default());
    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, edited);

    // Reset individual de parâmetro de efeito (Zoom end_scale) + Undo
    let zoom_id = app.add_clip_effect(clip_id, ClipEffect::with_defaults(EffectKind::Zoom)).unwrap();
    let edited_zoom = EffectParameters::Zoom(ZoomParams {
        start_scale: 1.1,
        end_scale: 1.75,
        focus_x: 0.2,
        focus_y: -0.1,
        easing: ZoomEasing::Linear,
    });
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::UpdateEffectParams(clip_id, zoom_id, edited_zoom),
    )));
    let reset_end_only = EffectParameters::Zoom(ZoomParams {
        end_scale: ZoomParams::default().end_scale,
        start_scale: 1.1,
        focus_x: 0.2,
        focus_y: -0.1,
        easing: ZoomEasing::Linear,
    });
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::UpdateEffectParams(clip_id, zoom_id, reset_end_only),
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.effects[0].params, reset_end_only);
    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.effects[0].params, edited_zoom);
}

#[test]
fn live_drag_dispatches_update_clip_live_without_polluting_history() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let vid = app.import_media(PathBuf::from("cena.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: vid,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(vid).unwrap();
    let zoom_id = app.add_clip_effect(clip_id, ClipEffect::with_defaults(EffectKind::Zoom)).unwrap();
    while worker.try_recv_command().is_some() {}

    // Simula arraste de Escala (3 on_change) -> apenas UpdateClipLive, sem poluir histórico
    let _ = app.update(Message::SelectTab(WorkspaceTab::Edit));
    for scale in [1.15, 1.35, 1.6] {
        let draft = ClipTransform { scale, ..ClipTransform::default() };
        let _ = app.update(Message::Edit(EditMessage::Inspector(
            InspectorMessage::PreviewTransform(clip_id, draft),
        )));
        assert!(matches!(
            worker.try_recv_command(),
            Some(EngineCommand::UpdateClipLive { transform: Some(tr), .. }) if tr.scale == scale
        ));
    }
    assert!(worker.try_recv_command().is_none());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.0);

    // Release consolida exatamente 1 entrada de Undo e marca is_dirty
    let _ = app.update(Message::Edit(EditMessage::Inspector(InspectorMessage::CommitTransform(clip_id))));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.6);
    assert!(app.project_io().is_dirty);
    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.0);

    // Arraste de parâmetro de efeito envia UpdateClipLive e consolida 1 Undo no release
    while worker.try_recv_command().is_some() {}
    for end_scale in [1.3, 1.6, 1.9] {
        let params = EffectParameters::Zoom(ZoomParams { end_scale, ..ZoomParams::default() });
        let _ = app.update(Message::Effects(EffectsMessage::Inspector(
            EffectsInspectorMessage::PreviewEffectParams(clip_id, zoom_id, params),
        )));
        assert!(matches!(
            worker.try_recv_command(),
            Some(EngineCommand::UpdateClipLive { effects: Some(fx), .. })
                if fx[0].params == EffectParameters::Zoom(ZoomParams { end_scale, ..ZoomParams::default() })
        ));
    }
    assert_eq!(
        app.project().timeline.clip(clip_id).unwrap().1.effects[0].params,
        EffectParameters::Zoom(ZoomParams::default())
    );
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::CommitEffectParams(clip_id, zoom_id),
    )));
    assert_eq!(
        app.project().timeline.clip(clip_id).unwrap().1.effects[0].params,
        EffectParameters::Zoom(ZoomParams { end_scale: 1.9, ..ZoomParams::default() })
    );
    assert!(app.undo());
    assert_eq!(
        app.project().timeline.clip(clip_id).unwrap().1.effects[0].params,
        EffectParameters::Zoom(ZoomParams::default())
    );

    // Velocidade: preview não dispara loop UpdateClipLive; release consolida 1 Undo + SyncTimeline
    while worker.try_recv_command().is_some() {}
    let speed_draft = ClipTransform { speed: 2.0, ..ClipTransform::default() };
    let _ = app.update(Message::Edit(EditMessage::Inspector(
        InspectorMessage::PreviewTransform(clip_id, speed_draft),
    )));
    assert!(worker.try_recv_command().is_none());
    let _ = app.update(Message::Edit(EditMessage::Inspector(InspectorMessage::CommitTransform(clip_id))));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.speed, 2.0);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
}

#[test]
fn jkl_shuttle_shortcuts_across_all_four_profiles_and_badge_formatting() {
    for profile in KeymapProfile::all() {
        assert!(matches!(
            map_keyboard_shortcut(*profile, Key::Character("j".into()), Modifiers::empty()),
            Some(Message::Playback(PlaybackMessage::Shuttle(-1)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(*profile, Key::Character("K".into()), Modifiers::empty()),
            Some(Message::Playback(PlaybackMessage::Shuttle(0)))
        ));
        assert!(matches!(
            map_keyboard_shortcut(*profile, Key::Character("l".into()), Modifiers::empty()),
            Some(Message::Playback(PlaybackMessage::Shuttle(1)))
        ));
        assert!(map_keyboard_shortcut(*profile, Key::Character("j".into()), Modifiers::CTRL).is_none());
    }

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::Playback(PlaybackMessage::Shuttle(-1)));
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::Shuttle { direction: -1 })));
    let _ = app.update(Message::EngineEvent(EngineEvent::ShuttleRateChanged { shuttle_rate: -2.0 }));
    assert!(app.is_playing());
    assert_eq!(app.playback().shuttle_rate, -2.0);

    let _ = app.update(Message::Playback(PlaybackMessage::Shuttle(0)));
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::Shuttle { direction: 0 })));
    assert!(!app.is_playing());
    assert_eq!(app.playback().shuttle_rate, 0.0);

    assert_eq!(format_shuttle_badge(0.0), None);
    assert_eq!(format_shuttle_badge(1.0), None);
    assert_eq!(format_shuttle_badge(-1.0), Some("-1x".to_string()));
    assert_eq!(format_shuttle_badge(-2.0), Some("-2x".to_string()));
    assert_eq!(format_shuttle_badge(2.0), Some("2x".to_string()));
    assert_eq!(format_shuttle_badge(4.0), Some("4x".to_string()));
}

#[test]
fn timeline_click_scrubs_inside_clip_while_trim_and_drag_protect_playhead() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let vid = app.import_media(PathBuf::from("v.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: vid,
        duration_ns: 8_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(vid).unwrap();
    let v1_id = app.project().timeline.tracks[0].id;
    let _ = app.move_clip(clip_id, v1_id, 2_000_000_000);
    while worker.try_recv_command().is_some() {}

    let view_state = TimelineViewState::default();
    let px = view_state.px_per_sec;
    let v1_mid_y = RULER_HEIGHT + TRACK_HEIGHT * 0.5;
    let mut interaction = TimelineInteraction::default();

    // 1. Clique simples (<= 4px) dentro do clipe em 5s seleciona e move playhead no release
    let click_5s_x = time_ns_to_x(5_000_000_000, px, 0.0);
    let p_click = Point::new(click_5s_x, v1_mid_y);
    let (_, press_msg) = interaction.on_press(p_click, app.project(), app.playhead_ns(), false, &view_state);
    dispatch_timeline_msg(&mut app, press_msg);
    assert_eq!(app.selected_clip_id(), Some(clip_id));
    let _ = interaction.on_move(
        Some(Point::new(click_5s_x + 2.0, v1_mid_y)),
        Point::new(click_5s_x + 2.0, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    let release_msg = interaction.on_release();
    dispatch_timeline_msg(&mut app, release_msg);
    assert_eq!(app.playhead_ns(), 5_000_000_000);
    assert_eq!(worker.take_pending_seek(), Some(5_000_000_000));

    // 2. Borda de trim (~8px) oculta hover guide e NÃO move a agulha
    let left_edge_x = time_ns_to_x(2_000_000_000, px, 0.0) + 2.0;
    let (_, trim_press) = interaction.on_press(
        Point::new(left_edge_x, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    dispatch_timeline_msg(&mut app, trim_press);
    assert!(interaction.visible_hover_x().is_none());
    let _ = interaction.on_move(
        Some(Point::new(left_edge_x + 25.0, v1_mid_y)),
        Point::new(left_edge_x + 25.0, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    let trim_release = interaction.on_release();
    dispatch_timeline_msg(&mut app, trim_release);
    assert_eq!(app.playhead_ns(), 5_000_000_000);
    assert!(app.project().timeline.clip(clip_id).unwrap().1.timeline_start_ns > 2_000_000_000);

    // 3. Arraste do corpo (> 4px) move o clipe e NÃO move a agulha
    let clip_start_before = app.project().timeline.clip(clip_id).unwrap().1.timeline_start_ns;
    let drag_x = time_ns_to_x(clip_start_before + 2_000_000_000, px, 0.0);
    let (_, move_press) = interaction.on_press(
        Point::new(drag_x, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    dispatch_timeline_msg(&mut app, move_press);
    let move_delta = CLIP_DRAG_THRESHOLD_PX + 21.0;
    let _ = interaction.on_move(
        Some(Point::new(drag_x + move_delta, v1_mid_y)),
        Point::new(drag_x + move_delta, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    let move_release = interaction.on_release();
    dispatch_timeline_msg(&mut app, move_release);
    assert_eq!(app.playhead_ns(), 5_000_000_000);
    assert_ne!(app.project().timeline.clip(clip_id).unwrap().1.timeline_start_ns, clip_start_before);

    // 4. Área vazia move a agulha imediatamente e limpa seleção
    let empty_x = time_ns_to_x(1_000_000_000, px, 0.0);
    let (_, empty_press) = interaction.on_press(
        Point::new(empty_x, v1_mid_y),
        app.project(), app.playhead_ns(), false, &view_state,
    );
    dispatch_timeline_msg(&mut app, empty_press);
    assert_eq!(app.playhead_ns(), 1_000_000_000);
    assert!(app.selected_clip_id().is_none());
    let _ = interaction.on_release();

    // 5. Badges FX na timeline (clipe largo vs estreito com +N)
    let mut sample_clip = app.project().timeline.clip(clip_id).unwrap().1.clone();
    sample_clip.effects = vec![
        ClipEffect::with_defaults(EffectKind::Zoom),
        ClipEffect::with_defaults(EffectKind::FilmStyle),
        ClipEffect::with_defaults(EffectKind::Glitch),
    ];
    let wide_labels: Vec<String> = compute_effect_badges(&sample_clip, 260.0).into_iter().map(|b| b.label).collect();
    assert_eq!(wide_labels, vec!["FX", "Zoom", "Filme", "Glitch"]);

    let narrow_labels: Vec<String> = compute_effect_badges(&sample_clip, 90.0).into_iter().map(|b| b.label).collect();
    assert_eq!(narrow_labels.first().map(String::as_str), Some("FX"));
    assert!(narrow_labels.iter().any(|l| l.starts_with('+')));
}
