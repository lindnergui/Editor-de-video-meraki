use meraki_core::{
    AddAssetCommand, AddClipCommand, AddFolderCommand, AssetId, Clip, ClipId, ClipTransform,
    HistoryError, HistoryStack, MediaAsset, MediaFolder, MediaKind, MoveClipCommand,
    PreviewQuality, Project, RemoveClipCommand, RenameProjectCommand, SplitClipCommand, Timeline,
    TimelineError, TrackId, TrackKind, TrimClipCommand, UpdateClipTransformCommand,
};
use std::path::PathBuf;

#[test]
fn add_clip_validates_duration_overflow_track_and_overlap() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let asset = AssetId::new();

    assert!(matches!(tl.add_clip(v1, Clip::new(asset, 0, 0, 0)), Err(TimelineError::ZeroDuration)));
    assert!(matches!(
        tl.add_clip(v1, Clip::new(asset, u64::MAX - 5, 0, 10)),
        Err(TimelineError::TimestampOverflow)
    ));
    assert!(matches!(
        tl.add_clip(TrackId::new(), Clip::new(asset, 0, 0, 100)),
        Err(TimelineError::TrackNotFound(_))
    ));

    tl.add_clip(v1, Clip::new(asset, 100, 0, 200)).unwrap();
    assert!(matches!(
        tl.add_clip(v1, Clip::new(asset, 250, 0, 100)),
        Err(TimelineError::ClipOverlap { .. })
    ));
    assert_eq!(tl.total_duration_ns(), 300);
}

#[test]
fn move_clip_works_across_tracks_and_rejects_overlap_atomically() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let v2 = tl.add_track("V2", TrackKind::Video);
    let asset = AssetId::new();

    let c1 = tl.add_clip(v1, Clip::new(asset, 0, 0, 500)).unwrap();
    let _c2 = tl.add_clip(v2, Clip::new(asset, 200, 0, 400)).unwrap();

    assert!(matches!(tl.move_clip(c1, v2, 100), Err(TimelineError::ClipOverlap { .. })));
    assert_eq!(tl.clip(c1).unwrap().0, v1);

    tl.move_clip(c1, v2, 600).unwrap();
    assert_eq!(tl.clip(c1).unwrap().0, v2);
    assert_eq!(tl.clip(c1).unwrap().1.timeline_start_ns, 600);
}

#[test]
fn trim_clip_validates_source_bounds_and_neighbors() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let asset = AssetId::new();
    let c1 = tl.add_clip(v1, Clip::new(asset, 100, 50, 400)).unwrap();
    tl.add_clip(v1, Clip::new(asset, 700, 0, 200)).unwrap();

    assert!(matches!(tl.trim_clip(c1, 100, 50, 0, None), Err(TimelineError::ZeroDuration)));
    assert!(matches!(
        tl.trim_clip(c1, 100, 200, 500, Some(600)),
        Err(TimelineError::SourceOutOfBounds { .. })
    ));
    assert!(matches!(
        tl.trim_clip(c1, 100, 50, 650, None),
        Err(TimelineError::ClipOverlap { .. })
    ));

    tl.trim_clip(c1, 150, 100, 350, Some(600)).unwrap();
    let updated = tl.clip(c1).unwrap().1;
    assert_eq!((updated.timeline_start_ns, updated.source_in_ns, updated.duration_ns), (150, 100, 350));
}

#[test]
fn split_clip_preserves_source_offsets_and_rejects_boundary_points() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let asset = AssetId::new();
    let c1 = tl.add_clip(v1, Clip::new(asset, 1_000, 500, 2_000)).unwrap();

    assert!(matches!(
        tl.split_clip(c1, 1_000),
        Err(TimelineError::SplitPointOutOfBounds { .. })
    ));
    assert!(matches!(
        tl.split_clip(c1, 3_000),
        Err(TimelineError::SplitPointOutOfBounds { .. })
    ));

    let (left_id, right_id) = tl.split_clip(c1, 1_800).unwrap();
    let left = tl.clip(left_id).unwrap().1;
    let right = tl.clip(right_id).unwrap().1;
    assert_eq!((left.timeline_start_ns, left.source_in_ns, left.duration_ns), (1_000, 500, 800));
    assert_eq!((right.timeline_start_ns, right.source_in_ns, right.duration_ns), (1_800, 1_300, 1_200));
}

#[test]
fn history_executes_undoes_and_redoes_project_and_timeline_mutations() {
    let mut project = Project::new("Original");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new("Cena", PathBuf::from("cena.mp4"), MediaKind::Video, None, 2_000);
    let asset_id = asset.id;

    history.execute(Box::new(RenameProjectCommand::new("Editado")), &mut project).unwrap();
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();

    let clip = Clip::new(asset_id, 0, 0, 1_000);
    let clip_id = clip.id;
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();
    history.execute(Box::new(MoveClipCommand::new(clip_id, v1, 200)), &mut project).unwrap();
    history
        .execute(Box::new(TrimClipCommand::new(clip_id, 250, 50, 800, Some(2_000))), &mut project)
        .unwrap();
    let split_cmd = SplitClipCommand::new(clip_id, 650);
    let right_id = split_cmd.right_clip_id();
    history.execute(Box::new(split_cmd), &mut project).unwrap();

    assert_eq!(project.name, "Editado");
    assert_eq!(project.assets.len(), 1);
    assert_eq!(project.timeline.tracks[0].clips.len(), 2);
    assert!(project.timeline.clip(right_id).is_some());

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.timeline_start_ns, 250);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.duration_ns, 1_000);

    history.redo(&mut project).unwrap();
    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.tracks[0].clips.len(), 2);
    assert!(project.timeline.clip(right_id).is_some());
}

#[test]
fn failed_command_and_stack_bounds_behave_safely() {
    let mut project = Project::new("Base");
    let mut history = HistoryStack::with_max_depth(2);

    assert!(matches!(history.undo(&mut project), Err(HistoryError::NothingToUndo)));
    assert!(matches!(history.redo(&mut project), Err(HistoryError::NothingToRedo)));
    assert!(history.execute(Box::new(RenameProjectCommand::new("   ")), &mut project).is_err());
    assert_eq!(history.undo_len(), 0);

    history.execute(Box::new(RenameProjectCommand::new("Passo 1")), &mut project).unwrap();
    history.execute(Box::new(RenameProjectCommand::new("Passo 2")), &mut project).unwrap();
    history.execute(Box::new(RenameProjectCommand::new("Passo 3")), &mut project).unwrap();
    assert_eq!(history.undo_len(), 2);

    history.undo(&mut project).unwrap();
    assert_eq!(project.name, "Passo 2");
    history
        .execute(Box::new(AddFolderCommand::new(MediaFolder::new("Extra", None))), &mut project)
        .unwrap();
    assert!(!history.can_redo());
}

#[test]
fn clip_transform_validation_rejects_out_of_range_and_non_finite_values() {
    let def = ClipTransform::default();
    assert_eq!(def.scale, 1.0);
    assert_eq!(def.pos_x, 0.0);
    assert_eq!(def.pos_y, 0.0);
    assert_eq!(def.opacity, 1.0);
    assert_eq!(def.speed, 1.0);
    assert!(def.validate().is_ok());

    let invalid_cases = [
        ClipTransform { scale: 0.0, ..def },
        ClipTransform { scale: -1.0, ..def },
        ClipTransform { scale: 10.1, ..def },
        ClipTransform { scale: f32::NAN, ..def },
        ClipTransform { pos_x: f32::INFINITY, ..def },
        ClipTransform { pos_y: f32::NEG_INFINITY, ..def },
        ClipTransform { opacity: -0.01, ..def },
        ClipTransform { opacity: 1.01, ..def },
        ClipTransform { opacity: f32::NAN, ..def },
        ClipTransform { speed: 0.0, ..def },
        ClipTransform { speed: 16.5, ..def },
        ClipTransform { speed: f32::NAN, ..def },
    ];

    for bad in invalid_cases {
        assert!(matches!(bad.validate(), Err(TimelineError::InvalidTransform { .. })));
    }
    assert_eq!(PreviewQuality::Proxy720p.max_dimensions(), (1280, 720));
    assert_eq!(PreviewQuality::Full1080p.max_dimensions(), (1920, 1080));
}

#[test]
fn timeline_update_transform_split_and_serde_backward_compatibility() {
    let mut tl = Timeline::with_default_tracks();
    let v1 = tl.tracks[0].id;
    let asset = AssetId::new();

    let mut invalid_clip = Clip::new(asset, 0, 0, 1_000);
    invalid_clip.transform.scale = 0.0;
    assert!(matches!(tl.add_clip(v1, invalid_clip), Err(TimelineError::InvalidTransform { field: "scale", .. })));

    let clip_id = tl.add_clip(v1, Clip::new(asset, 0, 0, 2_000)).unwrap();
    assert!(matches!(
        tl.update_clip_transform(ClipId::new(), ClipTransform::default()),
        Err(TimelineError::ClipNotFound(_))
    ));
    assert!(matches!(
        tl.update_clip_transform(clip_id, ClipTransform { opacity: 2.0, ..ClipTransform::default() }),
        Err(TimelineError::InvalidTransform { field: "opacity", .. })
    ));

    let custom = ClipTransform { scale: 1.5, pos_x: 40.0, pos_y: -20.0, opacity: 0.75, speed: 1.25 };
    let prev = tl.update_clip_transform(clip_id, custom).unwrap();
    assert_eq!(prev, ClipTransform::default());

    let (left_id, right_id) = tl.split_clip(clip_id, 1_000).unwrap();
    assert_eq!(tl.clip(left_id).unwrap().1.transform, custom);
    assert_eq!(tl.clip(right_id).unwrap().1.transform, custom);

    let json = serde_json::to_string(&tl).unwrap();
    let restored: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(tl, restored);

    let legacy_clip_json = format!(
        r#"{{"id":"{}","asset_id":"{}","timeline_start_ns":0,"source_in_ns":0,"duration_ns":1000}}"#,
        ClipId::new(),
        AssetId::new()
    );
    let legacy: Clip = serde_json::from_str(&legacy_clip_json).unwrap();
    assert_eq!(legacy.transform, ClipTransform::default());
}

#[test]
fn update_clip_transform_command_supports_undo_redo_and_preserves_stack_on_error() {
    let mut project = Project::new("Transform");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new("C", PathBuf::from("c.mp4"), MediaKind::Video, None, 4_000);
    let asset_id = asset.id;
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();

    let clip = Clip::new(asset_id, 0, 0, 2_000);
    let clip_id = clip.id;
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();

    let target = ClipTransform { scale: 0.8, pos_x: -100.0, pos_y: 50.0, opacity: 0.5, speed: 2.0 };
    history.execute(Box::new(UpdateClipTransformCommand::new(clip_id, target)), &mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, target);

    let invalid = ClipTransform { speed: -0.5, ..target };
    assert!(history.execute(Box::new(UpdateClipTransformCommand::new(clip_id, invalid)), &mut project).is_err());
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, target);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, ClipTransform::default());

    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, target);
}

#[test]
fn remove_clip_command_supports_undo_redo_and_missing_clip_error() {
    let mut project = Project::new("Remove");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new("Clip", PathBuf::from("clip.mp4"), MediaKind::Video, None, 5_000);
    let asset_id = asset.id;
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();

    let clip = Clip::new(asset_id, 500, 100, 1_500);
    let clip_id = clip.id;
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();

    assert!(matches!(
        history.execute(Box::new(RemoveClipCommand::new(ClipId::new())), &mut project),
        Err(HistoryError::Timeline(TimelineError::ClipNotFound(_)))
    ));

    history.execute(Box::new(RemoveClipCommand::new(clip_id)), &mut project).unwrap();
    assert!(project.timeline.clip(clip_id).is_none());

    history.undo(&mut project).unwrap();
    let (track_id, restored) = project.timeline.clip(clip_id).unwrap();
    assert_eq!(track_id, v1);
    assert_eq!((restored.timeline_start_ns, restored.source_in_ns, restored.duration_ns), (500, 100, 1_500));

    history.redo(&mut project).unwrap();
    assert!(project.timeline.clip(clip_id).is_none());
}
