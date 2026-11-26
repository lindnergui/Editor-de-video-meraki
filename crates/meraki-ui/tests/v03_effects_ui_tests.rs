use iced::{Rectangle, Size};
use meraki_core::{
    title_pose, ClipEffect, ClipTransform, EffectParameters, MediaMetadata, NormalizedTitleBox,
    TitleOverlay, TitleStyle, TransitionEffectKind, VideoEffectKind,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::components::effect_param_controls::resolve_selected_effect;
use meraki_ui::components::effects_inspector::{EffectsInspectorMessage, ZoomPreset};
use meraki_ui::components::effects_library::EffectsLibraryMessage;
use meraki_ui::components::player_overlay::{
    fitted_video_rect, normalized_title_overlay_box, overlay_box_rect, resolve_overlay_rect,
    PlayerOverlayMessage,
};
use meraki_ui::views::effects::EffectsMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::path::PathBuf;

fn setup_video_clip_app() -> (
    MerakiApp,
    meraki_engine::protocol::EngineWorkerChannel,
    meraki_core::ClipId,
) {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Effects));
    let asset_id = app.import_media(PathBuf::from("take.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 6_000_000_000,
        ..MediaMetadata::default()
    })));
    let clip_id = app.send_asset_to_timeline(asset_id).unwrap();
    while worker.try_recv_command().is_some() {}
    (app, worker, clip_id)
}

#[test]
fn apply_multiple_effects_select_toggle_and_remove_with_undo_redo() {
    let (mut app, worker, clip_id) = setup_video_clip_app();

    // Sem clipe selecionado, clicar na biblioteca seleciona automaticamente o clipe sob a agulha
    let _ = app.update(Message::Playback(PlaybackMessage::SelectClip(None)));
    let _ = app.update(Message::Playback(PlaybackMessage::Seek(1_000_000_000)));
    assert!(app.selected_clip_id().is_none());

    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleVideoEffect(VideoEffectKind::Zoom),
    )));
    assert_eq!(app.selected_clip_id(), Some(clip_id));
    let zoom_id = app.project().timeline.clip(clip_id).unwrap().1.effects[0].id;
    assert_eq!(app.effects_state().selected_effect_id, Some(zoom_id));
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));

    // Aplica Estilo Filme e Transição Fade no mesmo clipe
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ToggleVideoEffect(VideoEffectKind::FilmStyle),
    )));
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::ApplyTransition(TransitionEffectKind::Fade),
    )));
    while worker.try_recv_command().is_some() {}

    let clip = app.project().timeline.clip(clip_id).unwrap().1;
    assert_eq!(clip.effects.len(), 3);
    let film_id = clip.effects[1].id;
    let fade_id = clip.effects[2].id;
    assert_eq!(app.effects_state().selected_effect_id, Some(fade_id));

    // Alterna seleção no inspetor individual entre Zoom e Estilo Filme
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::SelectEffect(zoom_id),
    )));
    assert_eq!(
        resolve_selected_effect(app.project().timeline.clip(clip_id).unwrap().1, app.effects_state().selected_effect_id)
            .map(|e| e.id),
        Some(zoom_id)
    );
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::SelectEffect(film_id),
    )));
    assert_eq!(
        resolve_selected_effect(app.project().timeline.clip(clip_id).unwrap().1, app.effects_state().selected_effect_id)
            .map(|e| e.id),
        Some(film_id)
    );

    // Desativar / reativar efeito com olho e Undo/Redo
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ToggleEffectById(clip_id, film_id),
    )));
    assert!(!app.project().timeline.clip(clip_id).unwrap().1.effects[1].enabled);
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::SyncTimeline { .. })));
    assert!(app.undo());
    assert!(app.project().timeline.clip(clip_id).unwrap().1.effects[1].enabled);
    assert!(app.redo());
    assert!(!app.project().timeline.clip(clip_id).unwrap().1.effects[1].enabled);

    // Atualizar parâmetros de Transição e resetar mantendo o subtipo
    while worker.try_recv_command().is_some() {}
    if let EffectParameters::Transition(mut tr) = app.project().timeline.clip(clip_id).unwrap().1.effects[2].params {
        tr.duration_ms = 1400;
        tr.apply_out = false;
        let _ = app.update(Message::Effects(EffectsMessage::Inspector(
            EffectsInspectorMessage::UpdateEffectParams(clip_id, fade_id, EffectParameters::Transition(tr)),
        )));
    }
    assert!(matches!(
        app.project().timeline.clip(clip_id).unwrap().1.effects[2].params,
        EffectParameters::Transition(tr) if tr.duration_ms == 1400 && !tr.apply_out
    ));
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ResetEffectById(clip_id, fade_id),
    )));
    assert!(matches!(
        app.project().timeline.clip(clip_id).unwrap().1.effects[2].params,
        EffectParameters::Transition(tr)
            if tr.kind == TransitionEffectKind::Fade && tr.duration_ms == 500 && tr.apply_in && tr.apply_out
    ));

    // Remover efeito individual e reconciliar seleção com Undo/Redo
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::RemoveEffectById(clip_id, film_id),
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.effects.len(), 2);
    assert_eq!(app.effects_state().selected_effect_id, Some(zoom_id));

    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.effects.len(), 3);
    assert!(app.redo());
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.effects.len(), 2);
}

#[test]
fn zoom_presets_and_reset_update_start_and_end_scale_with_undo() {
    let (mut app, worker, clip_id) = setup_video_clip_app();

    let zoom_id = app.add_clip_effect(clip_id, ClipEffect::with_defaults(VideoEffectKind::Zoom)).unwrap();
    while worker.try_recv_command().is_some() {}

    // Zoom Out -> start=125%, end=100%
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ApplyZoomPreset(clip_id, zoom_id, ZoomPreset::ZoomOut),
    )));
    match app.project().timeline.clip(clip_id).unwrap().1.effects[0].params {
        EffectParameters::Zoom(z) => {
            assert_eq!(z.start_scale, 1.25);
            assert_eq!(z.end_scale, 1.0);
        }
        other => panic!("esperado ZoomParams, recebido {other:?}"),
    }
    assert!(matches!(worker.try_recv_command(), Some(EngineCommand::UpdateClipLive { effects: Some(_), .. })));

    // Fixo -> start=125%, end=125%
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ApplyZoomPreset(clip_id, zoom_id, ZoomPreset::Fixed),
    )));
    match app.project().timeline.clip(clip_id).unwrap().1.effects[0].params {
        EffectParameters::Zoom(z) => {
            assert_eq!(z.start_scale, 1.25);
            assert_eq!(z.end_scale, 1.25);
        }
        other => panic!("esperado ZoomParams, recebido {other:?}"),
    }

    // Zoom In -> start=100%, end=125%
    let _ = app.update(Message::Effects(EffectsMessage::Inspector(
        EffectsInspectorMessage::ApplyZoomPreset(clip_id, zoom_id, ZoomPreset::ZoomIn),
    )));
    match app.project().timeline.clip(clip_id).unwrap().1.effects[0].params {
        EffectParameters::Zoom(z) => {
            assert_eq!(z.start_scale, 1.0);
            assert_eq!(z.end_scale, 1.25);
        }
        other => panic!("esperado ZoomParams, recebido {other:?}"),
    }

    // Undo restaura preset anterior (Fixo)
    assert!(app.undo());
    match app.project().timeline.clip(clip_id).unwrap().1.effects[0].params {
        EffectParameters::Zoom(z) => {
            assert_eq!(z.start_scale, 1.25);
            assert_eq!(z.end_scale, 1.25);
        }
        other => panic!("esperado ZoomParams após Undo, recebido {other:?}"),
    }
}

#[test]
fn player_overlay_matches_ges_normalized_title_projection_without_duplicate_text() {
    // 1. Garante ausência total de renderização duplicada de texto no Canvas do overlay
    let overlay_src = include_str!("../src/components/player_overlay.rs");
    assert!(!overlay_src.contains("fill_text"));

    // 2. Projeção do fallback normalizado alinha exatamente com meraki_core::title_pose (ModernText & QuickCaption)
    let title = TitleOverlay::new("Meraki", TitleStyle::ModernText);
    let tr = ClipTransform { pos_x: 320.0, pos_y: -180.0, scale: 1.5, ..ClipTransform::default() };
    let pose = title_pose(title.style, title.font_size, tr);
    let norm = normalized_title_overlay_box(&title, title.font_size, tr);
    assert!(((norm.x + norm.width * 0.5) - pose.x).abs() < 1e-6);
    assert!(((norm.y + norm.height * 0.5) - pose.y).abs() < 1e-6);

    let caption = TitleOverlay::new("Legenda", TitleStyle::QuickCaption);
    let cap_tr = ClipTransform::default();
    let cap_pose = title_pose(caption.style, caption.font_size, cap_tr);
    let cap_norm = normalized_title_overlay_box(&caption, caption.font_size, cap_tr);
    assert!(((cap_norm.y + cap_norm.height * 0.5) - cap_pose.y).abs() < 1e-6);

    // 3. Quando title_bounds do GES está presente, resolve_overlay_rect projeta exatamente e acompanha rascunho ao vivo
    let (mut app, _worker, _) = setup_video_clip_app();
    let _ = app.update(Message::Effects(EffectsMessage::Library(
        EffectsLibraryMessage::AddTitle(TitleStyle::ModernText),
    )));
    let title_id = app.selected_clip_id().unwrap();
    let title_clip = app.project().timeline.clip(title_id).unwrap().1.clone();
    let video_rect: Rectangle = fitted_video_rect(Size::new(960.0, 540.0));
    let ges_bounds = NormalizedTitleBox { x: 0.4, y: 0.45, width: 0.2, height: 0.1 };

    let exact = resolve_overlay_rect(video_rect, &title_clip, title_clip.transform, None, Some(ges_bounds));
    assert!((exact.x - video_rect.width * 0.4).abs() < 0.01);
    assert!((exact.y - video_rect.height * 0.45).abs() < 0.01);
    assert!((exact.width - video_rect.width * 0.2).abs() < 0.01);

    let live_shifted = resolve_overlay_rect(
        video_rect,
        &title_clip,
        ClipTransform { pos_x: 288.0, scale: 2.0, ..title_clip.transform },
        None,
        Some(ges_bounds),
    );
    assert!(live_shifted.x > exact.x);
    assert!((live_shifted.x + live_shifted.width * 0.5) > (exact.x + exact.width * 0.5));
    assert!(live_shifted.width > exact.width);

    let fallback_shifted = overlay_box_rect(video_rect, &title_clip, 192.0, 0.0);
    assert!(fallback_shifted.x > video_rect.width * 0.5 - fallback_shifted.width * 0.5);

    // 4. Redimensionamento pelas alças de canto atualiza scale e aceita Undo
    let _ = app.update(Message::Effects(EffectsMessage::Overlay(
        PlayerOverlayMessage::PreviewScale { clip_id: title_id, scale: 1.65 },
    )));
    assert_eq!(app.effects_state().transform_draft.map(|(_, t)| t.scale), Some(1.65));
    let _ = app.update(Message::Effects(EffectsMessage::Overlay(
        PlayerOverlayMessage::CommitScale { clip_id: title_id, scale: 1.65 },
    )));
    assert_eq!(app.project().timeline.clip(title_id).unwrap().1.transform.scale, 1.65);
    assert!(app.undo());
    assert_eq!(app.project().timeline.clip(title_id).unwrap().1.transform.scale, 1.0);
}
