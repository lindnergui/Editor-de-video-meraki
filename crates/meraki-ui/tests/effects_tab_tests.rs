use iced::{Point, Size};
use meraki_core::{
    ClipTransition, KeyframeEasing, KeyframeProperty, MediaMetadata, TitleOverlay, TitleStyle,
    TrackKind, TransitionKind, VideoEffectKind,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::effects_inspector::{
    clip_local_offset_ns, find_keyframe_near, EffectsInspectorMessage,
};
use meraki_ui::components::effects_library::{
    matches_query, EffectsLibraryMessage, LibrarySection,
};
use meraki_ui::components::player::PlayerMessage;
use meraki_ui::components::player_overlay::{
    fitted_video_rect, overlay_box_rect, PlayerOverlayMessage,
};
use meraki_ui::components::timeline_canvas::TimelineCanvasMessage;
use meraki_ui::components::timeline_math::display_tracks_ordered;
use meraki_ui::views::effects::EffectsMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::path::PathBuf;

#[test]
fn effects_library_search_sections_and_title_insertion_at_playhead() {
    assert!(matches_query("título", "Título 3D", "titulo 3d"));
    assert!(matches_query("titulo", "Título 3D", "titulo 3d"));
    assert!(matches_query("GLITCH", "Glitch", "glitch digital"));
    assert!(!matches_query("inexistente", "Fade", "fade"));

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Effects));
    assert_eq!(app.active_tab(), WorkspaceTab::Effects);

    // Busca e colapso/expansão local da biblioteca
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::SearchChanged("fade".to_string()),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleSection(LibrarySection::Transitions),
    )));
    assert_eq!(app.effects_state().library.search_query, "fade");
    assert!(!app.effects_state().library.transitions_expanded);

    // Move agulha para 1.5s e insere Título Moderno em T1
    let _ = app.update(Message::Effects(EffectsMessage::Timeline(
        TimelineCanvasMessage::Scrub(1_500_000_000),
    )));
    assert_eq!(app.playhead_ns(), 1_500_000_000);

    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::AddTitle(TitleStyle::ModernText),
    )));

    let first_title_id = app.selected_clip_id().expect("título inserido deve ser selecionado");
    let (t1_id, first_clip) = app.project().timeline.clip(first_title_id).unwrap();
    assert_eq!(app.project().timeline.track(t1_id).unwrap().kind, TrackKind::Overlay);
    assert_eq!(first_clip.timeline_start_ns, 1_500_000_000);
    assert_eq!(
        first_clip.title.as_ref().map(|t| t.text.as_str()),
        Some("Texto Moderno")
    );

    match worker.try_recv_command() {
        Some(EngineCommand::SyncTimeline { clips, .. }) => {
            assert_eq!(clips.len(), 1);
            assert_eq!(clips[0].track_kind, TrackKind::Overlay);
            assert!(clips[0].title.is_some());
        }
        other => panic!("esperado SyncTimeline com título em Overlay, recebido {other:?}"),
    }

    // Na aba Efeitos, T1 deve aparecer ordenada no topo acima de V1 e A1
    let ordered = display_tracks_ordered(&app.project().timeline, true);
    assert_eq!(ordered[0].kind, TrackKind::Overlay);
    assert_eq!(ordered[1].kind, TrackKind::Video);
    assert_eq!(ordered[2].kind, TrackKind::Audio);

    // Segundo título inserido no mesmo playhead avança automaticamente para evitar sobreposição em T1
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::AddTitle(TitleStyle::QuickCaption),
    )));
    let second_title_id = app.selected_clip_id().unwrap();
    assert_ne!(second_title_id, first_title_id);
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.timeline_start_ns,
        4_500_000_000
    );
    assert!(matches!(
        worker.try_recv_command(),
        Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 2
    ));

    // Edição de texto com estado intermediário vazio seguro e UpdateTitle completo
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::TitleTextChanged(second_title_id, "   ".to_string()),
    )));
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.title.as_ref().unwrap().text,
        "Legenda Rápida"
    );

    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::TitleTextChanged(second_title_id, "O FUTURO".to_string()),
    )));
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.title.as_ref().unwrap().text,
        "O FUTURO"
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { title: Some(title), .. }) if title.text == "O FUTURO"));

    let custom_title = TitleOverlay {
        text: "O FUTURO".to_string(),
        style: TitleStyle::Title3D,
        font_family: "Sans Bold".to_string(),
        font_size: 72,
        color_rgba: [115, 91, 242, 255],
    };
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::UpdateTitle(second_title_id, custom_title.clone()),
    )));
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.title.as_ref(),
        Some(&custom_title)
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { title: Some(title), .. }) if title == custom_title));

    // Desfazer / Refazer sincroniza motor e limpa rascunho de texto
    assert!(app.undo());
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.title.as_ref().unwrap().style,
        TitleStyle::QuickCaption
    );
    assert!(app.effects_state().title_draft.is_none());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    assert!(app.redo());
    assert_eq!(
        app.project().timeline.clip(second_title_id).unwrap().1.title.as_ref(),
        Some(&custom_title)
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
}

#[test]
fn effects_transitions_overlay_drag_and_keyframes_sync_and_undo_redo() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Effects));

    let v_id = app.import_media(PathBuf::from("cena.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: v_id,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(v_id).unwrap();
    while worker.try_recv_command().is_some() {}

    // Alterna efeito de vídeo (Glitch) e transição (Desfoque/Blur) ligando e desligando
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleVideoEffect(VideoEffectKind::Glitch),
    )));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.effects[0].enabled);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleTransition(TransitionKind::Blur),
    )));
    assert_eq!(
        app.project().timeline.clip(clip_id).unwrap().1.transition_in,
        Some(ClipTransition::new(TransitionKind::Blur, 500_000_000))
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleTransition(TransitionKind::Blur),
    )));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.transition_in.is_none());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // Geometria do overlay 16:9 e arraste sobre o visor
    let video_rect = fitted_video_rect(Size::new(800.0, 500.0));
    assert!((video_rect.width / video_rect.height - (16.0 / 9.0)).abs() < 0.01);
    let clip_ref = app.project().timeline.clip(clip_id).unwrap().1;
    let initial_box = overlay_box_rect(video_rect, clip_ref, 0.0, 0.0);
    let shifted_box = overlay_box_rect(video_rect, clip_ref, 240.0, -120.0);
    assert!(shifted_box.x > initial_box.x && shifted_box.y < initial_box.y);

    let _ = app.update(Message::Effects(EffectsMessage::Overlay(
        PlayerOverlayMessage::CommitPosition { clip_id, pos_x: 120.0, pos_y: -60.0 },
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.pos_x, 120.0);
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.pos_y, -60.0);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { transform: Some(tr), .. }) if tr.pos_y == -60.0));

    // Keyframes no timestamp 2.0s com Linear e alternância para EaseInOut
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(2_000_000_000)));
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosX },
    )));
    let clip_after_kf = app.project().timeline.clip(clip_id).unwrap().1;
    let offset = clip_local_offset_ns(clip_after_kf, 2_000_000_000);
    let kf = find_keyframe_near(clip_after_kf, KeyframeProperty::PosX, offset).unwrap();
    assert_eq!(kf.value, 120.0);
    assert_eq!(kf.easing, KeyframeEasing::Linear);
    assert!(matches!(
        worker.try_recv_command(),
        Some(EngineCommand::SyncTimeline { clips, .. }) if clips[0].keyframes.len() == 1
    ));

    // Clicar no card Suave [Ease] converte o keyframe ativo para EaseInOut e define padrão futuro
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::SetEasing(KeyframeEasing::EaseInOut),
    )));
    assert_eq!(app.effects_state().active_easing, KeyframeEasing::EaseInOut);
    let clip_eased = app.project().timeline.clip(clip_id).unwrap().1;
    assert_eq!(
        find_keyframe_near(clip_eased, KeyframeProperty::PosX, offset).unwrap().easing,
        KeyframeEasing::EaseInOut
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // Clicar novamente no losango remove o keyframe na agulha atual
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ToggleKeyframe { clip_id, property: KeyframeProperty::PosX },
    )));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.keyframes.is_empty());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // Player Undo / Redo integrado
    let _ = app.update(Message::Effects(EffectsMessage::Player(PlayerMessage::Undo)));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.keyframes.len(), 1);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    let _ = app.update(Message::Effects(EffectsMessage::Player(PlayerMessage::Redo)));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.keyframes.is_empty());
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
    assert!(Point::new(initial_box.x, initial_box.y).x >= 0.0);
}

#[test]
fn effects_inspector_sliders_preview_live_and_record_only_on_release() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Effects));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::AddTitle(TitleStyle::ModernText),
    )));
    let clip_id = app.selected_clip_id().unwrap();
    while worker.try_recv_command().is_some() {}

    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::PreviewTitleFontSize(clip_id, 64),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::PreviewTitleFontSize(clip_id, 84),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::PreviewTransform(
            clip_id,
            meraki_core::ClipTransform { pos_x: 150.0, ..Default::default() },
        ),
    )));
    for size in [64, 84] {
        assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { title: Some(title), .. }) if title.font_size == size));
    }
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { transform: Some(tr), .. }) if tr.pos_x == 150.0));
    assert_eq!(app.effects_state().title_font_size_draft, Some((clip_id, 84)));
    assert_eq!(
        app.effects_state().transform_draft.map(|(_, tr)| tr.pos_x),
        Some(150.0)
    );

    // O release confirma cada valor no HistoryStack.
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::CommitTitleFontSize(clip_id),
    )));
    assert_eq!(
        app.project().timeline.clip(clip_id).unwrap().1.title.as_ref().unwrap().font_size,
        84
    );
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { title: Some(title), .. }) if title.font_size == 84));
    assert!(worker.try_recv_command().is_none());

    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::CommitTransform(clip_id),
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.transform.pos_x, 150.0);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { transform: Some(tr), .. }) if tr.pos_x == 150.0));
    assert!(worker.try_recv_command().is_none());
}
