use meraki_core::{
    clip_source_bounds, AddAssetCommand, AddClipCommand, AppLanguage, AppPreferences, AssetId,
    Clip, HistoryError, HistoryStack, KeymapProfile, MediaAsset, MediaKind, MoveClipCommand,
    Project, ProxyMode, RemoveClipCommand, RemoveTransitionCommand, SplitClipCommand, Timeline,
    TimelineError, TimelineTransition, TransitionKind, TransitionPlacement, TrimClipCommand,
    UpdateTransitionDurationCommand, UpsertTransitionCommand, DEFAULT_TRANSITION_DURATION_NS,
};
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn preferences_language_and_proxy_mode_roundtrip_and_v03_backwards_compat() {
    let defaults = AppPreferences::default();
    assert_eq!(defaults.language, AppLanguage::PtBr);
    assert_eq!(defaults.proxy_mode, ProxyMode::AutoAbove1080p);

    let custom = AppPreferences {
        keymap: KeymapProfile::DaVinciResolve,
        onboarding_completed: true,
        recent_projects: Vec::new(),
        language: AppLanguage::EnUs,
        proxy_mode: ProxyMode::Always,
    };
    let json = custom.to_json().unwrap();
    let restored = AppPreferences::from_json(&json).unwrap();
    assert_eq!(restored, custom);

    // JSON da v0.3.0 sem `language` nem `proxy_mode`
    let legacy_json = r#"{"keymap":"CapCut","onboarding_completed":true,"recent_projects":[]}"#;
    let legacy = AppPreferences::from_json(legacy_json).unwrap();
    assert_eq!(legacy.keymap, KeymapProfile::CapCut);
    assert!(legacy.onboarding_completed);
    assert_eq!(legacy.language, AppLanguage::PtBr);
    assert_eq!(legacy.proxy_mode, ProxyMode::AutoAbove1080p);

    // `ProxyMode::Off` também serializa/desserializa corretamente
    let off_prefs = AppPreferences { proxy_mode: ProxyMode::Off, ..custom };
    let off_restored = AppPreferences::from_json(&off_prefs.to_json().unwrap()).unwrap();
    assert_eq!(off_restored.proxy_mode, ProxyMode::Off);

    // Projeto .meraki legado (v0.3.0) sem `transitions` no `Timeline` nem `media_duration_ns` nos clipes
    let asset_id = AssetId::new();
    let clip_id = Uuid::new_v4();
    let track_id = Uuid::new_v4();
    let legacy_project_json = format!(
        r#"{{
            "id": "{}",
            "name": "Projeto v0.3",
            "folders": [],
            "assets": [],
            "timeline": {{
                "tracks": [{{
                    "id": "{track_id}",
                    "name": "V1",
                    "kind": "Video",
                    "muted": false,
                    "solo": false,
                    "gain_db": 0.0,
                    "clips": [{{
                        "id": "{clip_id}",
                        "asset_id": "{asset_id}",
                        "timeline_start_ns": 0,
                        "source_in_ns": 500,
                        "duration_ns": 2000
                    }}]
                }}],
                "playhead_ns": 0
            }},
            "preview_quality": "Proxy720p",
            "export_preset": "YouTube"
        }}"#,
        Uuid::new_v4()
    );
    let legacy_project = Project::from_json(&legacy_project_json).unwrap();
    assert!(legacy_project.timeline.transitions.is_empty());
    assert_eq!(legacy_project.timeline.tracks[0].clips[0].media_duration_ns, 0);
}

#[test]
fn positional_transitions_between_start_end_cleanup_and_undo_redo() {
    let mut project = Project::new("Transições Posicionais");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new("Cena", PathBuf::from("cena.mp4"), MediaKind::Video, None, 10_000_000_000);
    let asset_id = asset.id;
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();

    // Dois clipes contíguos [0..2s) e [2s..2.6s) na trilha V1
    let c1 = Clip::new(asset_id, 0, 0, 2_000_000_000);
    let c2 = Clip::new(asset_id, 2_000_000_000, 2_000_000_000, 600_000_000);
    let c1_id = c1.id;
    let c2_id = c2.id;
    history.execute(Box::new(AddClipCommand::new(v1, c1)), &mut project).unwrap();
    history.execute(Box::new(AddClipCommand::new(v1, c2)), &mut project).unwrap();

    // Detecta junção contígua próxima à agulha (2.0s)
    assert_eq!(
        project.timeline.find_adjacent_pair_near(v1, 2_050_000_000, 100_000_000),
        Some((c1_id.0, c2_id.0, 2_000_000_000))
    );
    assert_eq!(
        project.timeline.find_adjacent_pair_near(v1, 1_000_000_000, 100_000_000),
        None
    );

    // BetweenClips: 0.5s é clampado para metade do menor clipe envolvente (600ms / 2 = 300ms)
    let between = TimelineTransition::with_default_duration(
        v1,
        TransitionKind::Crossfade,
        TransitionPlacement::BetweenClips { left_clip_id: c1_id.0, right_clip_id: c2_id.0 },
    );
    assert_eq!(between.duration_ns, DEFAULT_TRANSITION_DURATION_NS);
    let between_id = between.id;
    history.execute(Box::new(UpsertTransitionCommand::new(between)), &mut project).unwrap();
    assert_eq!(project.timeline.transition(between_id).unwrap().duration_ns, 300_000_000);

    // Substituir o mesmo placement atualiza sem duplicar
    let between_zoom = TimelineTransition::new(
        v1,
        TransitionKind::Zoom,
        TransitionPlacement::BetweenClips { left_clip_id: c1_id.0, right_clip_id: c2_id.0 },
        250_000_000,
    );
    let between_zoom_id = between_zoom.id;
    history.execute(Box::new(UpsertTransitionCommand::new(between_zoom)), &mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 1);
    assert_eq!(project.timeline.transition(between_zoom_id).unwrap().kind, TransitionKind::Zoom);

    // ClipStart e ClipEnd
    let start_tr = TimelineTransition::with_default_duration(
        v1,
        TransitionKind::Blur,
        TransitionPlacement::ClipStart { clip_id: c1_id.0 },
    );
    let start_id = start_tr.id;
    let end_tr = TimelineTransition::new(
        v1,
        TransitionKind::Crossfade,
        TransitionPlacement::ClipEnd { clip_id: c2_id.0 },
        500_000_000,
    );
    let end_id = end_tr.id;
    history.execute(Box::new(UpsertTransitionCommand::new(start_tr)), &mut project).unwrap();
    history.execute(Box::new(UpsertTransitionCommand::new(end_tr)), &mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 3);
    assert_eq!(project.timeline.transition(start_id).unwrap().duration_ns, 500_000_000);
    assert_eq!(project.timeline.transition(end_id).unwrap().duration_ns, 300_000_000);

    // UpdateTransitionDurationCommand + Undo/Redo
    history.execute(Box::new(UpdateTransitionDurationCommand::new(start_id, 200_000_000)), &mut project).unwrap();
    assert_eq!(project.timeline.transition(start_id).unwrap().duration_ns, 200_000_000);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.transition(start_id).unwrap().duration_ns, 500_000_000);
    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.transition(start_id).unwrap().duration_ns, 200_000_000);

    // Mover c2 para 2.5s separa a junção -> cleanup_orphaned_transitions remove apenas BetweenClips!
    history.execute(Box::new(MoveClipCommand::new(c2_id, v1, 2_500_000_000)), &mut project).unwrap();
    assert!(project.timeline.transition(between_zoom_id).is_none());
    assert_eq!(project.timeline.transitions.len(), 2);

    // Undo restaura o clipe na posição original e também a transição BetweenClips removida
    history.undo(&mut project).unwrap();
    assert!(project.timeline.transition(between_zoom_id).is_some());
    assert_eq!(project.timeline.transitions.len(), 3);

    // Excluir c2 limpa BetweenClips + ClipEnd de c2; Undo restaura tudo; RemoveTransitionCommand remove explícito
    history.execute(Box::new(RemoveClipCommand::new(c2_id)), &mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 1);
    assert!(project.timeline.transition(start_id).is_some());
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 3);

    history.execute(Box::new(RemoveTransitionCommand::new(end_id)), &mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 2);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 3);
}

#[test]
fn non_destructive_split_and_reextend_trim_preserve_media_duration_and_bounds() {
    let mut project = Project::new("Não destrutivo");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let total_media_ns = 10_000_000_000_u64;
    let asset = MediaAsset::new("Bruto", PathBuf::from("bruto.mp4"), MediaKind::Video, None, total_media_ns);
    let asset_id = asset.id;
    history.execute(Box::new(AddAssetCommand::new(asset)), &mut project).unwrap();

    // Clipe inserido cobrindo [2s..8s) da mídia original (6s de duração na timeline)
    let clip = Clip::new(asset_id, 0, 2_000_000_000, 6_000_000_000);
    let left_id = clip.id;
    history.execute(Box::new(AddClipCommand::new(v1, clip)), &mut project).unwrap();

    let added = project.timeline.clip(left_id).unwrap().1;
    assert_eq!(added.media_duration_ns, total_media_ns);
    assert_eq!(clip_source_bounds(added), (2_000_000_000, 2_000_000_000));

    // Corta no instante 3.0s da timeline => esquerdo [0..3s) src=[2s..5s), direito [3s..6s) src=[5s..8s)
    let split = SplitClipCommand::new(left_id, 3_000_000_000);
    let right_id = split.right_clip_id();
    history.execute(Box::new(split), &mut project).unwrap();

    let left = project.timeline.clip(left_id).unwrap().1;
    let right = project.timeline.clip(right_id).unwrap().1;
    assert_eq!(left.media_duration_ns, total_media_ns);
    assert_eq!(right.media_duration_ns, total_media_ns);
    assert_eq!(clip_source_bounds(left), (2_000_000_000, 5_000_000_000));
    assert_eq!(clip_source_bounds(right), (5_000_000_000, 2_000_000_000));

    // Remove o clipe esquerdo para liberar espaço à esquerda na timeline e re-estende o clipe direito até source_in=0
    history.execute(Box::new(RemoveClipCommand::new(left_id)), &mut project).unwrap();
    history
        .execute(Box::new(TrimClipCommand::new(right_id, 0, 0, 10_000_000_000, None)), &mut project)
        .unwrap();

    let extended = project.timeline.clip(right_id).unwrap().1;
    assert_eq!(extended.timeline_start_ns, 0);
    assert_eq!(extended.source_in_ns, 0);
    assert_eq!(extended.duration_ns, total_media_ns);
    assert_eq!(clip_source_bounds(extended), (0, 0));

    // Tentar estender além de media_duration_ns deve ser rejeitado mesmo passando None em max_source_duration_ns
    assert!(matches!(
        history.execute(Box::new(TrimClipCommand::new(right_id, 0, 0, 11_000_000_000, None)), &mut project),
        Err(HistoryError::Timeline(TimelineError::SourceOutOfBounds { .. }))
    ));

    // Undo restaura o trecho pós-split [3s..6s) com source_in = 5s e [0..3s) no clipe esquerdo
    history.undo(&mut project).unwrap();
    history.undo(&mut project).unwrap();
    let restored_right = project.timeline.clip(right_id).unwrap().1;
    assert_eq!(restored_right.source_in_ns, 5_000_000_000);
    assert_eq!(restored_right.duration_ns, 3_000_000_000);
    assert!(project.timeline.clip(left_id).is_some());

    // Também funciona quando Timeline::trim_clip é chamado diretamente após SplitClipCommand
    history.undo(&mut project).unwrap();
    let split2 = SplitClipCommand::new(left_id, 2_000_000_000);
    let right2_id = split2.right_clip_id();
    history.execute(Box::new(split2), &mut project).unwrap();
    // Move right2 para longe e re-estende esquerda de volta até media_duration_ns
    history.execute(Box::new(MoveClipCommand::new(right2_id, v1, 20_000_000_000)), &mut project).unwrap();
    history
        .execute(Box::new(TrimClipCommand::new(left_id, 0, 0, 10_000_000_000, None)), &mut project)
        .unwrap();
    let left_extended = project.timeline.clip(left_id).unwrap().1;
    assert_eq!((left_extended.source_in_ns, left_extended.duration_ns), (0, total_media_ns));
    let _ = Timeline::new();
}
