use meraki_core::{
    AddAssetCommand, AddClipCommand, AddTitleClipCommand, AssetId, Clip, ClipEffect, ClipId,
    ClipTransition, HistoryError, HistoryStack, Keyframe, KeyframeEasing, KeyframeProperty,
    MediaAsset, MediaKind, Project, RemoveKeyframeCommand, SetClipTransitionCommand, Timeline,
    TimelineError, TitleOverlay, TitleStyle, ToggleClipEffectCommand, TrackKind, TransitionEdge,
    TransitionKind, UpdateTitleCommand, UpsertKeyframeCommand, VideoEffectKind,
};
use std::path::PathBuf;

fn setup_video_project() -> (Project, HistoryStack, ClipId) {
    let mut project = Project::new("Efeitos");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new("Cena", PathBuf::from("cena.mp4"), MediaKind::Video, None, 10_000_000_000);
    let asset_id = asset.id;
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();
    let clip = Clip::new(asset_id, 0, 0, 5_000_000_000);
    let clip_id = clip.id;
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();
    (project, history, clip_id)
}

#[test]
fn add_and_update_title_commands_manage_overlay_track_and_undo_redo() {
    let mut project = Project::new("Títulos");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;

    let bad_title = TitleOverlay { text: "   ".to_string(), ..TitleOverlay::default() };
    assert!(matches!(
        history.execute(Box::new(AddTitleClipCommand::new(None, 0, 2_000_000_000, bad_title)), &mut project),
        Err(HistoryError::Timeline(TimelineError::InvalidTitle { field: "text", .. }))
    ));
    assert_eq!(project.timeline.tracks.len(), 2);

    let title = TitleOverlay::new("Abertura Meraki", TitleStyle::ModernText);
    assert!(matches!(
        history.execute(Box::new(AddTitleClipCommand::new(Some(v1), 0, 2_000_000_000, title.clone())), &mut project),
        Err(HistoryError::Timeline(TimelineError::InvalidTrackKind { .. }))
    ));

    let add_cmd = AddTitleClipCommand::new(None, 1_000_000_000, 3_000_000_000, title.clone());
    let title_clip_id = add_cmd.clip_id();
    history.execute(Box::new(add_cmd), &mut project).unwrap();

    assert_eq!(project.timeline.tracks.len(), 3);
    assert_eq!(project.timeline.tracks[2].name, "T1");
    assert_eq!(project.timeline.tracks[2].kind, TrackKind::Overlay);
    assert_eq!(project.timeline.clip(title_clip_id).unwrap().1.title.as_ref(), Some(&title));

    let updated_title = TitleOverlay {
        text: "Legenda Rápida".to_string(),
        style: TitleStyle::QuickCaption,
        font_family: "Arial".to_string(),
        font_size: 40,
        color_rgba: [255, 220, 64, 255],
    };
    history.execute(Box::new(UpdateTitleCommand::new(title_clip_id, updated_title.clone())), &mut project).unwrap();
    assert_eq!(project.timeline.clip(title_clip_id).unwrap().1.title.as_ref(), Some(&updated_title));

    let empty_update = TitleOverlay { text: String::new(), ..updated_title.clone() };
    assert!(history.execute(Box::new(UpdateTitleCommand::new(title_clip_id, empty_update)), &mut project).is_err());

    // Desfazer atualização restaura título original
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(title_clip_id).unwrap().1.title.as_ref(), Some(&title));

    // Desfazer inserção remove o clipe e também a trilha T1 criada automaticamente
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.tracks.len(), 2);
    assert!(project.timeline.clip(title_clip_id).is_none());

    // Refazer restaura T1, clipe e texto editado
    history.redo(&mut project).unwrap();
    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.tracks.len(), 3);
    assert_eq!(project.timeline.clip(title_clip_id).unwrap().1.title.as_ref(), Some(&updated_title));
}

#[test]
fn toggle_effect_and_transition_commands_validate_and_undo_redo() {
    let (mut project, mut history, clip_id) = setup_video_project();

    assert!(matches!(
        history.execute(Box::new(ToggleClipEffectCommand::new(clip_id, VideoEffectKind::Glitch, 1.5)), &mut project),
        Err(HistoryError::Timeline(TimelineError::InvalidEffect { field: "intensity", .. }))
    ));

    history.execute(Box::new(ToggleClipEffectCommand::new(clip_id, VideoEffectKind::Cinematic, 0.8)), &mut project).unwrap();
    let fx = &project.timeline.clip(clip_id).unwrap().1.effects;
    assert_eq!(fx.len(), 1);
    assert_eq!(fx[0].kind, VideoEffectKind::Cinematic);
    assert_eq!(fx[0].intensity, 0.8);
    assert!(fx[0].enabled);

    history.execute(Box::new(ToggleClipEffectCommand::new(clip_id, VideoEffectKind::Cinematic, 0.6)), &mut project).unwrap();
    assert!(!project.timeline.clip(clip_id).unwrap().1.effects[0].enabled);

    // Transições inválidas não alteram o estado
    assert!(matches!(
        history.execute(
            Box::new(SetClipTransitionCommand::new(clip_id, TransitionEdge::In, Some(ClipTransition::new(TransitionKind::Fade, 0)))),
            &mut project,
        ),
        Err(HistoryError::Timeline(TimelineError::InvalidTransition { .. }))
    ));
    assert!(matches!(
        history.execute(
            Box::new(SetClipTransitionCommand::new(clip_id, TransitionEdge::Out, Some(ClipTransition::new(TransitionKind::Blur, 6_000_000_000)))),
            &mut project,
        ),
        Err(HistoryError::Timeline(TimelineError::InvalidTransition { .. }))
    ));

    let fade_in = ClipTransition::new(TransitionKind::Fade, 500_000_000);
    let zoom_out = ClipTransition::new(TransitionKind::Zoom, 800_000_000);
    history.execute(Box::new(SetClipTransitionCommand::new(clip_id, TransitionEdge::In, Some(fade_in))), &mut project).unwrap();
    history.execute(Box::new(SetClipTransitionCommand::new(clip_id, TransitionEdge::Out, Some(zoom_out))), &mut project).unwrap();

    let clip = project.timeline.clip(clip_id).unwrap().1;
    assert_eq!(clip.transition_in, Some(fade_in));
    assert_eq!(clip.transition_out, Some(zoom_out));

    // Desfazer tudo até antes do primeiro efeito
    history.undo(&mut project).unwrap();
    history.undo(&mut project).unwrap();
    history.undo(&mut project).unwrap();
    history.undo(&mut project).unwrap();

    let restored = project.timeline.clip(clip_id).unwrap().1;
    assert!(restored.effects.is_empty());
    assert!(restored.transition_in.is_none());
    assert!(restored.transition_out.is_none());

    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.effects.len(), 1);
}

#[test]
fn keyframe_commands_sort_update_validate_and_undo_redo() {
    let (mut project, mut history, clip_id) = setup_video_project();

    let invalid_kfs = [
        Keyframe::new(6_000_000_000, KeyframeProperty::PosX, 10.0, KeyframeEasing::Linear),
        Keyframe::new(1_000_000_000, KeyframeProperty::Scale, 0.0, KeyframeEasing::Linear),
        Keyframe::new(1_000_000_000, KeyframeProperty::Opacity, 1.2, KeyframeEasing::EaseInOut),
    ];
    for bad in invalid_kfs {
        assert!(matches!(
            history.execute(Box::new(UpsertKeyframeCommand::new(clip_id, bad)), &mut project),
            Err(HistoryError::Timeline(TimelineError::InvalidKeyframe { .. }))
        ));
    }

    let k_late = Keyframe::new(3_000_000_000, KeyframeProperty::Opacity, 0.25, KeyframeEasing::EaseInOut);
    let k_early = Keyframe::new(500_000_000, KeyframeProperty::Scale, 1.4, KeyframeEasing::Linear);
    let k_early_updated = Keyframe::new(500_000_000, KeyframeProperty::Scale, 1.8, KeyframeEasing::EaseInOut);

    history.execute(Box::new(UpsertKeyframeCommand::new(clip_id, k_late)), &mut project).unwrap();
    history.execute(Box::new(UpsertKeyframeCommand::new(clip_id, k_early)), &mut project).unwrap();
    history.execute(Box::new(UpsertKeyframeCommand::new(clip_id, k_early_updated)), &mut project).unwrap();

    assert_eq!(project.timeline.clip(clip_id).unwrap().1.keyframes, vec![k_early_updated, k_late]);

    assert!(matches!(
        history.execute(
            Box::new(RemoveKeyframeCommand::new(clip_id, KeyframeProperty::PosX, 123)),
            &mut project
        ),
        Err(HistoryError::Timeline(TimelineError::KeyframeNotFound { .. }))
    ));

    history
        .execute(Box::new(RemoveKeyframeCommand::new(clip_id, KeyframeProperty::Opacity, 3_000_000_000)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.keyframes, vec![k_early_updated]);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.keyframes, vec![k_early_updated, k_late]);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.keyframes, vec![k_early, k_late]);

    history.redo(&mut project).unwrap();
    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.keyframes, vec![k_early_updated]);
}

#[test]
fn split_clip_and_json_roundtrip_preserve_effects_transitions_keyframes_and_legacy_clips() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let mut clip = Clip::new(AssetId::new(), 0, 0, 4_000_000_000);
    clip.effects.push(ClipEffect::new(VideoEffectKind::FilmStyle, 0.7));
    clip.transition_in = Some(ClipTransition::new(TransitionKind::Fade, 500_000_000));
    clip.transition_out = Some(ClipTransition::new(TransitionKind::Blur, 600_000_000));
    clip.keyframes = vec![
        Keyframe::new(1_000_000_000, KeyframeProperty::PosX, -40.0, KeyframeEasing::Linear),
        Keyframe::new(3_000_000_000, KeyframeProperty::PosX, 80.0, KeyframeEasing::EaseInOut),
    ];
    let original = clip.clone();
    let id = tl.add_clip(v1, clip).unwrap();

    let (left_id, right_id) = tl.split_clip(id, 2_000_000_000).unwrap();
    let left = tl.clip(left_id).unwrap().1;
    let right = tl.clip(right_id).unwrap().1;
    assert_eq!(left.effects.len(), 1);
    assert_eq!(right.effects.len(), 1);
    assert!(left.transition_in.is_some() && left.transition_out.is_none());
    assert!(right.transition_in.is_none() && right.transition_out.is_some());
    assert_eq!(left.keyframes.len(), 1);
    assert_eq!(right.keyframes.len(), 1);
    assert_eq!(right.keyframes[0].time_offset_ns, 1_000_000_000);

    tl.unsplit_clip(left_id, right_id, original.clone()).unwrap();
    let restored = tl.clip(id).unwrap().1;
    assert_eq!(restored, &original);

    // JSON round-trip completo
    let json = serde_json::to_string(&tl).unwrap();
    let parsed: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, tl);

    // Compatibilidade retroativa de JSON legado (sem campos novos e keyframe com time_ns/Ease)
    let legacy_clip_json = format!(
        r#"{{"id":"{}","asset_id":"{}","timeline_start_ns":0,"source_in_ns":0,"duration_ns":1000}}"#,
        ClipId::new(),
        AssetId::new()
    );
    let legacy_clip: Clip = serde_json::from_str(&legacy_clip_json).unwrap();
    assert!(legacy_clip.title.is_none());
    assert!(legacy_clip.effects.is_empty());
    assert!(legacy_clip.transition_in.is_none());
    assert!(legacy_clip.transition_out.is_none());
    assert!(legacy_clip.keyframes.is_empty());

    let legacy_kf_json = r#"{"time_ns":250,"property":"Opacity","value":0.5,"easing":"Ease"}"#;
    let legacy_kf: Keyframe = serde_json::from_str(legacy_kf_json).unwrap();
    assert_eq!(legacy_kf, Keyframe::new(250, KeyframeProperty::Opacity, 0.5, KeyframeEasing::EaseInOut));
}
