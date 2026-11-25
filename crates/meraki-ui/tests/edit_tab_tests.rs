use iced::Point;
use meraki_core::{Clip, ClipTransform, MediaMetadata, PreviewQuality, Timeline, TrackKind};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::inspector::InspectorMessage;
use meraki_ui::components::player::PlayerMessage;
use meraki_ui::components::timeline_canvas::TimelineCanvasMessage;
use meraki_ui::components::timeline_math::{
    collect_snap_targets, compute_left_trim, compute_right_trim, display_tracks,
    format_ruler_label, hit_test, ruler_step_secs, snap_time_ns, time_ns_to_x, x_to_time_ns,
    HitTarget, TimelineTool, TimelineViewState, LEFT_PADDING, MIN_CLIP_DURATION_NS, RULER_HEIGHT,
    TRACK_HEIGHT, TRACK_STRIDE,
};
use meraki_ui::playback::format_timecode_mm_ss_ff;
use meraki_ui::views::edit::EditMessage;
use meraki_ui::{MerakiApp, Message};
use std::path::PathBuf;

#[test]
fn time_to_pixel_conversions_zoom_and_formatters_work() {
    assert_eq!(time_ns_to_x(0, 50.0, 0.0), LEFT_PADDING);
    assert_eq!(time_ns_to_x(2_000_000_000, 50.0, 0.0), LEFT_PADDING + 100.0);
    assert_eq!(time_ns_to_x(2_000_000_000, 50.0, 25.0), LEFT_PADDING + 75.0);
    assert_eq!(x_to_time_ns(LEFT_PADDING + 75.0, 50.0, 25.0), 2_000_000_000);
    assert_eq!(x_to_time_ns(0.0, 50.0, 0.0), 0);
    assert_eq!(x_to_time_ns(LEFT_PADDING * 0.5, 50.0, 0.0), 0);
    assert_eq!(x_to_time_ns(-10.0, 50.0, 0.0), 0);

    assert_eq!(ruler_step_secs(25.0), 5);
    assert_eq!(ruler_step_secs(100.0), 1);
    assert_eq!(format_ruler_label(65), "01:05");
    assert_eq!(format_timecode_mm_ss_ff(65_500_000_000, 30), "01:05:15");

    let mut view_state = TimelineViewState::default();
    let initial = view_state.px_per_sec;
    view_state.zoom_in();
    assert!(view_state.px_per_sec > initial);
    view_state.zoom_out();
    assert!((view_state.px_per_sec - initial).abs() < 0.01);
}

#[test]
fn hit_test_detects_ruler_body_side_handles_gap_and_display_tracks() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let a1 = tl.tracks[1].id;
    let c1 = tl.add_clip(v1, Clip::new(meraki_core::AssetId::new(), 2_000_000_000, 0, 4_000_000_000)).unwrap();
    let c2 = tl.add_clip(a1, Clip::new(meraki_core::AssetId::new(), 0, 0, 2_000_000_000)).unwrap();

    let tracks = display_tracks(&tl);
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[2].name, "T1");
    assert_eq!(tracks[2].kind, TrackKind::Overlay);

    let px = 25.0; // c1 spans x=[LEFT_PADDING+50, LEFT_PADDING+150] on V1, c2 spans x=[LEFT_PADDING, LEFT_PADDING+50] on A1
    let v1_mid_y = RULER_HEIGHT + TRACK_HEIGHT * 0.5;
    let a1_mid_y = RULER_HEIGHT + TRACK_STRIDE + TRACK_HEIGHT * 0.5;

    assert_eq!(
        hit_test(Point::new(4.0, 10.0), &tracks, px, 0.0),
        HitTarget::Ruler { time_ns: 0 }
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 50.0, 10.0), &tracks, px, 0.0),
        HitTarget::Ruler { time_ns: 2_000_000_000 }
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 52.0, v1_mid_y), &tracks, px, 0.0),
        HitTarget::ClipLeftHandle { track_id: v1, clip_id: c1 }
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 148.0, v1_mid_y), &tracks, px, 0.0),
        HitTarget::ClipRightHandle { track_id: v1, clip_id: c1 }
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 100.0, v1_mid_y), &tracks, px, 0.0),
        HitTarget::ClipBody { track_id: v1, clip_id: c1 }
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 25.0, a1_mid_y), &tracks, px, 0.0),
        HitTarget::ClipBody { track_id: a1, clip_id: c2 }
    );
    assert_eq!(hit_test(Point::new(4.0, a1_mid_y), &tracks, px, 0.0), HitTarget::Empty);
    // Gap entre V1 e A1 e trilha vazia T1 devem retornar Empty
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 25.0, RULER_HEIGHT + TRACK_HEIGHT + 2.0), &tracks, px, 0.0),
        HitTarget::Empty
    );
    assert_eq!(
        hit_test(Point::new(LEFT_PADDING + 25.0, RULER_HEIGHT + TRACK_STRIDE * 2.0 + 10.0), &tracks, px, 0.0),
        HitTarget::Empty
    );
}

#[test]
fn snap_and_trim_math_clamp_safely() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let c1 = tl.add_clip(v1, Clip::new(meraki_core::AssetId::new(), 2_000_000_000, 500_000_000, 3_000_000_000)).unwrap();
    let targets = collect_snap_targets(&tl, 1_000_000_000, None);

    // A 25 px/s, limiar de 8 px equivale a 320 ms
    assert_eq!(snap_time_ns(1_150_000_000, &targets, 25.0, true), 1_000_000_000);
    assert_eq!(snap_time_ns(1_150_000_000, &targets, 25.0, false), 1_150_000_000);

    let clip = tl.clip(c1).unwrap().1;
    // Extender à esquerda além do source_in_ns=500ms fica limitado em source_in_ns=0
    assert_eq!(
        compute_left_trim(clip, -1_000_000_000),
        (1_500_000_000, 0, 3_500_000_000)
    );
    // Cortar à esquerda além da duração preserva MIN_CLIP_DURATION_NS
    let (_, _, min_dur) = compute_left_trim(clip, 5_000_000_000);
    assert_eq!(min_dur, MIN_CLIP_DURATION_NS);

    // Aparar à direita respeita max_source_ns e duração mínima
    assert_eq!(compute_right_trim(clip, 5_000_000_000, Some(4_000_000_000)), 3_500_000_000);
    assert_eq!(compute_right_trim(clip, -5_000_000_000, None), MIN_CLIP_DURATION_NS);
}

#[test]
fn edit_tab_actions_update_state_history_and_engine_sync() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let cam_id = app.project().folders[0].id;
    let video_id = app.import_media(PathBuf::from("cena.mp4"), Some(cam_id), false).unwrap();
    while worker.try_recv_command().is_some() {}

    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: video_id,
        duration_ns: 8_000_000_000,
        ..MediaMetadata::default()
    })));

    // Busca e filtro na gaveta compacta + inserção do clipe
    let _ = app.update(Message::Edit(EditMessage::SearchChanged("cena".to_string())));
    let _ = app.update(Message::Edit(EditMessage::SelectFolder(Some(cam_id))));
    assert_eq!(app.edit_state().search_query, "cena");
    assert_eq!(app.edit_state().selected_folder, Some(cam_id));

    let _ = app.update(Message::Edit(EditMessage::InsertAsset(video_id)));
    let clip_id = app.selected_clip_id().unwrap();
    let v1_id = app.project().timeline.tracks[0].id;
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 1));

    // Inspetor altera ClipTransform via HistoryStack + sincroniza motor
    let custom_tr = ClipTransform { scale: 1.5, pos_x: 30.0, pos_y: -10.0, opacity: 0.9, speed: 1.25 };
    let _ = app.update(Message::Edit(EditMessage::Inspector(InspectorMessage::UpdateTransform(clip_id, custom_tr))));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, custom_tr);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // Player Undo / Redo e transporte
    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::Undo)));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, ClipTransform::default());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::Redo)));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform, custom_tr);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::StepForward)));
    assert_eq!(app.playhead_ns(), 1_000_000_000 / 30);
    assert_eq!(worker.take_pending_seek(), Some(1_000_000_000 / 30));

    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::StepBack)));
    assert_eq!(app.playhead_ns(), 0);
    assert_eq!(worker.take_pending_seek(), Some(0));

    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::SetRate(2.0))));
    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::SetVolume(0.4))));
    let _ = app.update(Message::Edit(EditMessage::Player(PlayerMessage::SetQuality(PreviewQuality::Full1080p))));
    assert_eq!(app.playback_rate(), 2.0);
    assert_eq!(app.master_volume(), 0.4);
    assert_eq!(app.preview_quality(), PreviewQuality::Full1080p);
    while worker.try_recv_command().is_some() {}

    // Ferramentas e ações diretas do Canvas da timeline
    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::SetTool(TimelineTool::Razor))));
    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::ToggleSnap)));
    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::ZoomIn)));
    assert_eq!(app.edit_state().timeline.tool, TimelineTool::Razor);
    assert!(!app.edit_state().timeline.snap_enabled);
    assert!(app.edit_state().timeline.px_per_sec > 25.0);

    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::MoveClip {
        clip_id,
        track_id: v1_id,
        new_start_ns: 1_000_000_000,
    })));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.timeline_start_ns, 1_000_000_000);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::TrimClip {
        clip_id,
        new_start_ns: 1_000_000_000,
        new_source_in_ns: 500_000_000,
        new_duration_ns: 4_000_000_000,
    })));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.duration_ns, 4_000_000_000);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Edit(EditMessage::Timeline(TimelineCanvasMessage::SplitClip {
        clip_id,
        split_at_ns: 3_000_000_000,
    })));
    assert_eq!(app.project().timeline.tracks[0].clips.len(), 2);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 2));
}

#[test]
fn inspector_sliders_defer_sync_until_commit_on_release() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let video_id = app.import_media(PathBuf::from("cena.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: video_id,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(video_id).unwrap();
    while worker.try_recv_command().is_some() {}

    // Arraste contínuo atualiza apenas rascunho local sem disparar SyncTimeline
    for scale in [1.2, 1.5, 1.8] {
        let draft = ClipTransform { scale, ..ClipTransform::default() };
        let _ = app.update(Message::Edit(EditMessage::Inspector(
            InspectorMessage::PreviewTransform(clip_id, draft),
        )));
    }
    assert!(worker.try_recv_command().is_none());
    assert_eq!(
        app.edit_state().transform_draft.map(|(_, tr)| tr.scale),
        Some(1.8)
    );
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.0);

    // Somente on_release confirma no HistoryStack e envia um único SyncTimeline
    let _ = app.update(Message::Edit(EditMessage::Inspector(
        InspectorMessage::CommitTransform(clip_id),
    )));
    assert!(app.edit_state().transform_draft.is_none());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.8);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
    assert!(worker.try_recv_command().is_none());

    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.scale, 1.0);
}
