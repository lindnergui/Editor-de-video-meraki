use iced::Point;
use meraki_core::{
    AppLanguage, AssetId, KeymapProfile, MediaMetadata, TransitionKind, TransitionPlacement,
    DEFAULT_TRANSITION_DURATION_NS,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::effects_library::{EffectsLibraryMessage, TransitionPlacementMode};
use meraki_ui::components::timeline_canvas::{
    compute_transition_visuals, hit_test_transition, truncate_clip_title, TimelineCanvasMessage,
};
use meraki_ui::components::timeline_interaction::TimelineInteraction;
use meraki_ui::components::timeline_math::{time_ns_to_x, TimelineViewState, RULER_HEIGHT, TRACK_HEIGHT};
use meraki_ui::views::edit::EditMessage;
use meraki_ui::views::effects::EffectsMessage;
use meraki_ui::views::transition_inspector::TransitionInspectorMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, ProxyBannerState, WorkspaceTab};
use std::path::PathBuf;

fn dispatch_tl(app: &mut MerakiApp, msg: Option<TimelineCanvasMessage>) {
    if let Some(m) = msg {
        let _ = app.update(Message::Edit(EditMessage::Timeline(m)));
    }
}

#[test]
fn positional_transitions_between_start_end_hittest_duration_and_delete_undo() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.complete_onboarding(KeymapProfile::PremierePro);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Effects));

    let v1_asset = app.import_media(PathBuf::from("take1.mp4"), None, false).unwrap();
    let v2_asset = app.import_media(PathBuf::from("take2.mp4"), None, false).unwrap();
    for id in [v1_asset, v2_asset] {
        let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
            asset_id: id,
            duration_ns: 6_000_000_000,
            ..MediaMetadata::default()
        })));
    }
    let c1 = app.send_asset_to_timeline(v1_asset).unwrap();
    let c2 = app.send_asset_to_timeline(v2_asset).unwrap();
    while worker.try_recv_command().is_some() {}

    // 1. Posiciona agulha perto da junção em 6.0s e aplica BetweenClips (Fade) -> sem poluir Clip.effects
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(5_900_000_000)));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::SetTransitionPlacement(TransitionPlacementMode::BetweenClips),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ApplyPositionalTransition(TransitionKind::Crossfade),
    )));

    assert_eq!(app.project().timeline.transitions.len(), 1);
    let tr_between = app.project().timeline.transitions[0];
    assert_eq!(
        tr_between.placement,
        TransitionPlacement::BetweenClips { left_clip_id: c1.0, right_clip_id: c2.0 }
    );
    assert_eq!(app.selected_transition_id(), Some(tr_between.id));
    assert!(app.project().timeline.clip(c1).unwrap().1.effects.is_empty());
    assert!(app.project().timeline.clip(c2).unwrap().1.effects.is_empty());
    assert!(matches!(
        worker.try_recv_command(),
        Some(EngineCommand::SyncTimeline { transitions, .. }) if transitions.len() == 1
    ));

    // 2. Aplica ClipStart em c1 e ClipEnd em c2
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(Some(c1))));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::SetTransitionPlacement(TransitionPlacementMode::ClipStart),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ApplyPositionalTransition(TransitionKind::Blur),
    )));
    let tr_start = *app.project().timeline.transitions.last().unwrap();
    assert_eq!(tr_start.placement, TransitionPlacement::ClipStart { clip_id: c1.0 });

    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(Some(c2))));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::SetTransitionPlacement(TransitionPlacementMode::ClipEnd),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ApplyPositionalTransition(TransitionKind::Zoom),
    )));
    let tr_end = *app.project().timeline.transitions.last().unwrap();
    assert_eq!(tr_end.placement, TransitionPlacement::ClipEnd { clip_id: c2.0 });
    assert_eq!(app.project().timeline.transitions.len(), 3);

    // 3. Hit-test na timeline seleciona a transição sob o ponto e limpa seleção de clipe
    let vs = TimelineViewState::default();
    let visuals = compute_transition_visuals(&app.project().timeline, false, vs.px_per_sec, vs.scroll_x_px, None);
    let between_vis = visuals.iter().find(|v| v.id == tr_between.id).unwrap();
    let seam_x = time_ns_to_x(6_000_000_000, vs.px_per_sec, vs.scroll_x_px);
    assert!((between_vis.rect.x + between_vis.rect.width * 0.5 - seam_x).abs() < 1.5);
    assert_eq!(
        hit_test_transition(
            Point::new(seam_x, RULER_HEIGHT + TRACK_HEIGHT * 0.5),
            &app.project().timeline, false, vs.px_per_sec, vs.scroll_x_px,
        ),
        Some(tr_between.id)
    );

    let mut interaction = TimelineInteraction::default();
    let (captured, press_msg) = interaction.on_press(
        Point::new(seam_x, RULER_HEIGHT + TRACK_HEIGHT * 0.5),
        app.project(), app.playhead_ns(), false, &vs,
    );
    assert!(captured);
    dispatch_tl(&mut app, press_msg);
    assert_eq!(app.selected_transition_id(), Some(tr_between.id));
    assert!(app.selected_clip_id().is_none());

    // Renderização do app com TransitionInspector ativo em PT-BR e EN-US
    let _ = app.view();
    let _ = app.update(Message::SetLanguage(AppLanguage::EnUs));
    let _ = app.view();

    // 4. Ajuste de tipo, preview/commit de duração e reset via duplo-clique (0.5s)
    let _ = app.update(Message::Effects(EffectsMessage::TransitionInspector(
        TransitionInspectorMessage::SetKind(tr_between.id, TransitionKind::Zoom),
    )));
    assert_eq!(app.project().timeline.transition(tr_between.id).unwrap().kind, TransitionKind::Zoom);

    let _ = app.update(Message::Effects(EffectsMessage::TransitionInspector(
        TransitionInspectorMessage::PreviewDuration(tr_between.id, 1_400_000_000),
    )));
    assert_eq!(app.effects_state().transition_duration_draft, Some((tr_between.id, 1_400_000_000)));
    let _ = app.update(Message::Effects(EffectsMessage::TransitionInspector(
        TransitionInspectorMessage::CommitDuration(tr_between.id),
    )));
    assert_eq!(app.project().timeline.transition(tr_between.id).unwrap().duration_ns, 1_400_000_000);
    assert!(app.effects_state().transition_duration_draft.is_none());

    let _ = app.update(Message::Effects(EffectsMessage::TransitionInspector(
        TransitionInspectorMessage::SetDuration(tr_between.id, DEFAULT_TRANSITION_DURATION_NS),
    )));
    assert_eq!(
        app.project().timeline.transition(tr_between.id).unwrap().duration_ns,
        DEFAULT_TRANSITION_DURATION_NS
    );

    // 5. Pressionar Delete apaga APENAS a transição (preservando os dois clipes) + Undo restaura
    let _ = app.update(Message::Playback(PlaybackMessage::DeleteSelectedClip));
    assert_eq!(app.project().timeline.transitions.len(), 2);
    assert!(app.project().timeline.clip(c1).is_some());
    assert!(app.project().timeline.clip(c2).is_some());
    assert!(app.selected_transition_id().is_none());

    assert!(app.undo());
    assert_eq!(app.project().timeline.transitions.len(), 3);
    assert!(app.project().timeline.transition(tr_between.id).is_some());
}

#[test]
fn clip_title_truncation_fits_narrow_clips_without_overflow() {
    let long_name = "Entrevista_Principal_CameraA_Final_v04.mp4";
    assert_eq!(truncate_clip_title(long_name, 400.0), long_name);

    let medium = truncate_clip_title(long_name, 78.0);
    assert!(medium.ends_with('…'));
    assert!(medium.chars().count() < long_name.chars().count());
    assert!(medium.chars().count() <= 12);

    let utf8_name = "Gravação_Ação_Áudio_Épico.mp4";
    let truncated_utf8 = truncate_clip_title(utf8_name, 45.0);
    assert!(truncated_utf8.ends_with('…'));
    assert!(truncated_utf8.chars().count() <= 7);

    assert_eq!(truncate_clip_title(long_name, 10.0), "");
    assert_eq!(truncate_clip_title("   ", 100.0), "");
}

#[test]
fn nondestructive_trim_expands_split_clip_to_media_bounds_and_proxy_banner_updates() {
    let dir = std::env::temp_dir().join(format!("meraki-v04-trim-proxy-{}", AssetId::new()));
    let _ = std::fs::create_dir_all(&dir);
    let video_path = dir.join("fonte.mp4");
    let _ = std::fs::write(&video_path, b"video");

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let asset_id = app.import_media(video_path, None, true).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 8_000_000_000,
        width: Some(3840),
        height: Some(2160),
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(asset_id).unwrap();

    // 1. Divide o clipe ao meio (4s + 4s) e apaga a segunda metade
    let right_id = app.split_clip(clip_id, 4_000_000_000).unwrap();
    assert!(app.remove_clip(right_id).is_ok());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.duration_ns, 4_000_000_000);

    // 2. Arrasta alça direita além de 8s -> reexpande até exatamente 8s e ativa marcador de limite
    let vs = TimelineViewState::default();
    let mut interaction = TimelineInteraction::default();
    let right_edge_x = time_ns_to_x(4_000_000_000, vs.px_per_sec, vs.scroll_x_px) - 2.0;
    let mid_y = RULER_HEIGHT + TRACK_HEIGHT * 0.5;
    let (_, press_r) = interaction.on_press(Point::new(right_edge_x, mid_y), app.project(), 0, false, &vs);
    dispatch_tl(&mut app, press_r);
    let _ = interaction.on_move(
        Some(Point::new(right_edge_x + 200.0, mid_y)),
        Point::new(right_edge_x + 200.0, mid_y),
        app.project(), 0, false, &vs,
    );
    assert_eq!(interaction.active_trim_limit(), Some((clip_id, false, true)));
    dispatch_tl(&mut app, interaction.on_release());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.duration_ns, 8_000_000_000);

    // 3. Apara borda esquerda para 2s e depois arrasta de volta além de 0s -> volta a start=0, source_in=0, dur=8s
    assert!(app.trim_clip(clip_id, 2_000_000_000, 2_000_000_000, 6_000_000_000, Some(8_000_000_000)).is_ok());
    let left_edge_x = time_ns_to_x(2_000_000_000, vs.px_per_sec, vs.scroll_x_px) + 2.0;
    let (_, press_l) = interaction.on_press(Point::new(left_edge_x, mid_y), app.project(), 0, false, &vs);
    dispatch_tl(&mut app, press_l);
    let _ = interaction.on_move(
        Some(Point::new(left_edge_x - 150.0, mid_y)),
        Point::new(left_edge_x - 150.0, mid_y),
        app.project(), 0, false, &vs,
    );
    assert_eq!(interaction.active_trim_limit(), Some((clip_id, true, false)));
    dispatch_tl(&mut app, interaction.on_release());
    let restored = app.project().timeline.clip(clip_id).unwrap().1;
    assert_eq!((restored.timeline_start_ns, restored.source_in_ns, restored.duration_ns), (0, 0, 8_000_000_000));

    // 4. Proxy banner: Hidden -> Generating -> Ready -> Hidden após ClearCache
    assert_eq!(app.proxy_banner_state(), ProxyBannerState::Hidden);
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyGenerating { asset_id }));
    assert_eq!(app.proxy_banner_state(), ProxyBannerState::Generating { count: 1 });
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyReady {
        asset_id,
        proxy_path: dir.join("proxy.mp4"),
    }));
    assert_eq!(app.proxy_banner_state(), ProxyBannerState::Ready);
    let _ = app.update(Message::EngineEvent(EngineEvent::ProxyCacheCleared { freed_bytes: 2_048_000 }));
    assert_eq!(app.proxy_banner_state(), ProxyBannerState::Hidden);

    let _ = std::fs::remove_dir_all(&dir);
}
