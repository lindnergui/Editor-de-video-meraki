use meraki_core::{
    AddAssetCommand, AddClipCommand, AddFolderCommand, Clip, ClipId, FolderId, HistoryStack,
    MediaAsset, MediaFolder, MediaKind, MediaMetadata, MoveClipCommand, Project, ProjectError,
    RenameProjectCommand, SplitClipCommand, TimelineError, TrackId, TrackKind, TrimClipCommand,
};
use std::path::PathBuf;

#[test]
fn folders_creation_nesting_and_deletion_rules() {
    let mut project = Project::new("Pastas");
    let root_id = project.folders[0].id;

    assert!(matches!(project.add_folder("   ", None), Err(ProjectError::EmptyName)));
    assert!(matches!(
        project.add_folder("Órfã", Some(FolderId::new())),
        Err(ProjectError::FolderNotFound(_))
    ));

    let sub_id = project.add_folder("Cena A", Some(root_id)).unwrap();
    let nested_id = project.add_folder("Takes", Some(sub_id)).unwrap();
    assert_eq!(project.child_folders(Some(root_id)).len(), 1);
    assert_eq!(project.child_folders(Some(sub_id)).len(), 1);

    // Pasta com subpasta não pode ser excluída
    assert!(matches!(
        project.remove_folder(sub_id),
        Err(ProjectError::FolderNotEmpty(_))
    ));

    // Remove subpasta folha e coloca ativo na pasta pai para testar proteção por ativo
    project.remove_folder(nested_id).unwrap();
    let asset_id = project
        .add_asset(MediaAsset::new(
            "Clip.mp4",
            PathBuf::from("clip.mp4"),
            MediaKind::Video,
            Some(sub_id),
            5_000_000_000,
        ))
        .unwrap();
    assert!(matches!(
        project.remove_folder(sub_id),
        Err(ProjectError::FolderNotEmpty(_))
    ));

    project.remove_asset(asset_id).unwrap();
    let removed = project.remove_folder(sub_id).unwrap();
    assert_eq!(removed.id, sub_id);
    assert!(matches!(
        project.remove_folder(sub_id),
        Err(ProjectError::FolderNotFound(_))
    ));
}

#[test]
fn media_asset_import_filtering_and_metadata_update() {
    let mut project = Project::new("Importação");
    let f_video = project.folders[0].id;
    let f_audio = project.folders[1].id;

    let v = MediaAsset::new("A.mp4", PathBuf::from("a.mp4"), MediaKind::Video, Some(f_video), 0);
    let a = MediaAsset::new("B.wav", PathBuf::from("b.wav"), MediaKind::Audio, Some(f_audio), 4_000);
    let i = MediaAsset::new("C.png", PathBuf::from("c.png"), MediaKind::Image, None, 5_000);

    let v_id = project.add_asset(v.clone()).unwrap();
    assert!(matches!(project.add_asset(v), Err(ProjectError::DuplicateAsset(_))));
    assert!(matches!(
        project.add_asset(MediaAsset::new("", PathBuf::from("x.mp4"), MediaKind::Video, None, 10)),
        Err(ProjectError::EmptyName)
    ));
    assert!(matches!(
        project.add_asset(MediaAsset::new(
            "X",
            PathBuf::from("x.mp4"),
            MediaKind::Video,
            Some(FolderId::new()),
            10,
        )),
        Err(ProjectError::FolderNotFound(_))
    ));

    project.add_asset(a).unwrap();
    project.add_asset(i).unwrap();
    assert_eq!(project.assets_by_kind(MediaKind::Video).len(), 1);
    assert_eq!(project.assets_by_kind(MediaKind::Audio).len(), 1);
    assert_eq!(project.assets_by_kind(MediaKind::Image).len(), 1);
    assert_eq!(project.assets_in_folder(Some(f_video)).len(), 1);
    assert_eq!(project.assets_in_folder(None).len(), 1);

    // Descoberta atualiza duração e ajusta in/out
    assert!(project.apply_discovered_metadata(MediaMetadata {
        asset_id: v_id,
        duration_ns: 10_000,
        width: Some(1920),
        height: Some(1080),
        has_audio: true,
        ..MediaMetadata::default()
    }));
    let updated = project.asset_mut(v_id).unwrap();
    assert_eq!(updated.out_point_ns, Some(10_000));
    updated.set_in_out(2_000, Some(8_000)).unwrap();
    assert!(matches!(
        updated.set_in_out(8_000, Some(8_000)),
        Err(ProjectError::InvalidInOutRange { .. })
    ));
    assert!(matches!(
        updated.set_in_out(1_000, Some(12_000)),
        Err(ProjectError::OutPointExceedsDuration { .. })
    ));
}

#[test]
fn add_clip_handles_ordering_adjacency_and_all_error_cases() {
    let mut project = Project::new("Timeline Add");
    let v1 = project.timeline.tracks[0].id;
    let a1 = project.timeline.tracks[1].id;
    let ov = project.timeline.add_track("T1", TrackKind::Overlay);
    let asset = project
        .add_asset(MediaAsset::new("M", PathBuf::from("m.mp4"), MediaKind::Video, None, 10_000))
        .unwrap();

    // Erros básicos
    assert!(matches!(
        project.timeline.add_clip(v1, Clip::new(asset, 0, 0, 0)),
        Err(TimelineError::ZeroDuration)
    ));
    assert!(matches!(
        project.timeline.add_clip(v1, Clip::new(asset, u64::MAX - 2, 0, 10)),
        Err(TimelineError::TimestampOverflow)
    ));
    assert!(matches!(
        project.timeline.add_clip(TrackId::new(), Clip::new(asset, 0, 0, 100)),
        Err(TimelineError::TrackNotFound(_))
    ));

    // Inserções válidas fora de ordem cronológica -> devem ordenar e permitir adjacência exata
    let c2 = Clip::new(asset, 500, 0, 300);
    let c2_id = c2.id;
    project.timeline.add_clip(v1, c2.clone()).unwrap();
    assert!(matches!(
        project.timeline.add_clip(v1, c2),
        Err(TimelineError::DuplicateClip(id)) if id == c2_id
    ));
    project.timeline.add_clip(v1, Clip::new(asset, 200, 0, 300)).unwrap();
    project.timeline.add_clip(a1, Clip::new(asset, 0, 0, 400)).unwrap();
    project.timeline.add_clip(ov, Clip::new(asset, 100, 0, 900)).unwrap();

    let v1_clips = &project.timeline.track(v1).unwrap().clips;
    assert_eq!(v1_clips[0].timeline_start_ns, 200);
    assert_eq!(v1_clips[1].timeline_start_ns, 500);
    assert_eq!(project.timeline.total_duration_ns(), 1_000);

    // Sobreposição no início, fim e englobamento
    for (start, dur) in [(100, 150), (450, 100), (150, 700)] {
        assert!(matches!(
            project.timeline.add_clip(v1, Clip::new(asset, start, 0, dur)),
            Err(TimelineError::ClipOverlap { .. })
        ));
    }
}

#[test]
fn trim_clip_and_split_clip_validate_bounds_and_preserve_state_on_error() {
    let mut project = Project::new("Trim & Split");
    let v1 = project.timeline.tracks[0].id;
    let asset = project
        .add_asset(MediaAsset::new("M", PathBuf::from("m.mp4"), MediaKind::Video, None, 2_000))
        .unwrap();

    let c1 = project.timeline.add_clip(v1, Clip::new(asset, 100, 200, 800)).unwrap();
    let _c2 = project.timeline.add_clip(v1, Clip::new(asset, 1_100, 0, 400)).unwrap();

    // Trim inválido não muta estado
    assert!(matches!(
        project.timeline.trim_clip(ClipId::new(), 100, 200, 500, None),
        Err(TimelineError::ClipNotFound(_))
    ));
    assert!(matches!(
        project.timeline.trim_clip(c1, 100, 200, 0, None),
        Err(TimelineError::ZeroDuration)
    ));
    assert!(matches!(
        project.timeline.trim_clip(c1, u64::MAX - 5, 0, 20, None),
        Err(TimelineError::TimestampOverflow)
    ));
    assert!(matches!(
        project.timeline.trim_clip(c1, 100, 1_500, 600, Some(2_000)),
        Err(TimelineError::SourceOutOfBounds { .. })
    ));
    assert!(matches!(
        project.timeline.trim_clip(c1, 100, 200, 1_050, Some(2_000)),
        Err(TimelineError::ClipOverlap { .. })
    ));
    assert_eq!(project.timeline.clip(c1).unwrap().1.duration_ns, 800);

    // Trim válido na esquerda e direita: [200..900), source_in = 300, duration = 700
    project.timeline.trim_clip(c1, 200, 300, 700, Some(2_000)).unwrap();

    // Split fora dos limites não altera estado
    assert!(matches!(
        project.timeline.split_clip(ClipId::new(), 500),
        Err(TimelineError::ClipNotFound(_))
    ));
    for invalid_pt in [150, 200, 900, 950] {
        assert!(matches!(
            project.timeline.split_clip(c1, invalid_pt),
            Err(TimelineError::SplitPointOutOfBounds { .. })
        ));
    }
    assert_eq!(project.timeline.track(v1).unwrap().clips.len(), 3 - 1);

    // Dois splits encadeados válidos
    let (left, mid) = project.timeline.split_clip(c1, 450).unwrap();
    let (mid2, right) = project.timeline.split_clip(mid, 700).unwrap();
    let cl = project.timeline.clip(left).unwrap().1;
    let cm = project.timeline.clip(mid2).unwrap().1;
    let cr = project.timeline.clip(right).unwrap().1;

    assert_eq!((cl.timeline_start_ns, cl.source_in_ns, cl.duration_ns), (200, 300, 250));
    assert_eq!((cm.timeline_start_ns, cm.source_in_ns, cm.duration_ns), (450, 550, 250));
    assert_eq!((cr.timeline_start_ns, cr.source_in_ns, cr.duration_ns), (700, 800, 200));
    assert_eq!(cl.duration_ns + cm.duration_ns + cr.duration_ns, 700);
}

#[test]
fn project_json_roundtrip_and_invalid_json_error() {
    let mut project = Project::new("Documentário");
    let root = project.folders[0].id;
    let sub = project.add_folder("Entrevistas", Some(root)).unwrap();
    let mut asset = MediaAsset::new(
        "Cena01.mp4",
        PathBuf::from("cena01.mp4"),
        MediaKind::Video,
        Some(sub),
        12_000_000_000,
    );
    asset.proxy_path = Some(PathBuf::from("proxies/cena01_720p.mp4"));
    asset.set_in_out(1_000_000_000, Some(9_000_000_000)).unwrap();
    let aid = project.add_asset(asset).unwrap();

    let v1 = project.timeline.tracks[0].id;
    let ov = project.timeline.add_track("O1", TrackKind::Overlay);
    project.timeline.add_clip(v1, Clip::new(aid, 0, 1_000_000_000, 5_000_000_000)).unwrap();
    project.timeline.add_clip(ov, Clip::new(aid, 500_000_000, 0, 2_000_000_000)).unwrap();
    project.timeline.playhead_ns = 1_500_000_000;

    let json = project.to_json().unwrap();
    let restored = Project::from_json(&json).unwrap();
    assert_eq!(project, restored);
    assert!(matches!(Project::from_json("{json inválido"), Err(ProjectError::Serialization(_))));
}

#[test]
fn history_stack_supports_multiple_sequential_undos_and_redos() {
    let mut project = Project::new("Inicial");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let extra_folder = MediaFolder::new("B-Roll", None);
    let extra_folder_id = extra_folder.id;
    let asset = MediaAsset::new("A.mp4", PathBuf::from("a.mp4"), MediaKind::Video, Some(extra_folder_id), 10_000);
    let asset_id = asset.id;
    let clip = Clip::new(asset_id, 0, 0, 4_000);
    let clip_id = clip.id;
    let split = SplitClipCommand::new(clip_id, 2_200);
    let right_id = split.right_clip_id();

    history.execute(Box::new(RenameProjectCommand::new("Corte 1")), &mut project).unwrap();
    history.execute(Box::new(AddFolderCommand::new(extra_folder)), &mut project).unwrap();
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();
    history.execute(Box::new(MoveClipCommand::new(clip_id, v1, 1_000)), &mut project).unwrap();
    history.execute(Box::new(TrimClipCommand::new(clip_id, 1_200, 200, 3_000, Some(10_000))), &mut project).unwrap();
    history.execute(Box::new(split), &mut project).unwrap();

    assert_eq!(history.undo_len(), 7);
    assert_eq!(history.redo_len(), 0);
    assert!(project.timeline.clip(right_id).is_some());

    // Desfaz todos os 7 comandos passo a passo
    for remaining in (0..7).rev() {
        history.undo(&mut project).unwrap();
        assert_eq!(history.undo_len(), remaining);
        assert_eq!(history.redo_len(), 7 - remaining);
    }
    assert!(!history.can_undo());
    assert!(history.can_redo());
    assert_eq!(project.name, "Inicial");
    assert!(project.folder(extra_folder_id).is_none());
    assert!(project.assets.is_empty());
    assert!(project.timeline.track(v1).unwrap().clips.is_empty());

    // Refaz todos os 7 comandos passo a passo
    for done in 1..=7 {
        history.redo(&mut project).unwrap();
        assert_eq!(history.undo_len(), done);
        assert_eq!(history.redo_len(), 7 - done);
    }
    assert!(!history.can_redo());
    assert_eq!(project.name, "Corte 1");
    assert_eq!(project.timeline.track(v1).unwrap().clips.len(), 2);
    assert_eq!(project.timeline.clip(right_id).unwrap().1.duration_ns, 2_000);

    // Novo comando após undo limpa redo_stack; comando inválido preserva pilhas
    history.undo(&mut project).unwrap();
    assert!(history.can_redo());
    assert!(history.execute(Box::new(RenameProjectCommand::new("")), &mut project).is_err());
    assert!(history.can_redo());
    history.execute(Box::new(RenameProjectCommand::new("Corte 2")), &mut project).unwrap();
    assert!(!history.can_redo());
}
