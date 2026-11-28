use meraki_core::{
    db_to_linear, eq_band_index, ApplyVoicePresetCommand, AssetId, AudioEffectKind, Clip, EqBand,
    HistoryStack, Project, RemoveVolumeKeyframeCommand, SplitClipCommand, TimelineError,
    ToggleAudioEffectCommand, TrackAudioSettings, TrackKind, UpdateTrackAudioCommand,
    UpsertVolumeKeyframeCommand, VoicePreset, VolumeKeyframe, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB,
    MIN_TRACK_GAIN_DB,
};

#[test]
fn voice_presets_provide_valid_eq_bands_and_preserve_fader_and_effects() {
    for preset in VoicePreset::ALL {
        assert!(!preset.label().is_empty());
        let gains = preset.eq_gains_db();
        for g in gains {
            assert!((MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&g));
        }
        let (nr_enabled, nr_strength) = preset.suggested_noise_reduction();
        assert!(nr_enabled);
        assert!((0.0..=1.0).contains(&nr_strength));
        assert!(preset.suggested_voice_leveler());

        let mut settings = TrackAudioSettings {
            gain_db: -3.5,
            muted: true,
            solo: true,
            effects: vec![AudioEffectKind::Echo],
            ..TrackAudioSettings::default()
        };
        preset.apply_to(&mut settings);
        settings.validate().unwrap();
        assert_eq!(settings.eq_gains_db, gains);
        assert_eq!(settings.voice_preset, Some(preset));
        assert_eq!(settings.gain_db, -3.5);
        assert!(settings.muted);
        assert!(settings.solo);
        assert_eq!(settings.effects, vec![AudioEffectKind::Echo]);
    }
}

#[test]
fn settings_and_volume_keyframes_validate_and_clamp_bounds() {
    let invalid_gain = TrackAudioSettings { gain_db: 15.0, ..TrackAudioSettings::default() };
    assert!(matches!(invalid_gain.validate(), Err(TimelineError::InvalidAudioSetting { field: "gain_db", .. })));

    let nan_eq = TrackAudioSettings { eq_gains_db: [0.0, f32::NAN, 0.0, 0.0], ..TrackAudioSettings::default() };
    assert!(matches!(nan_eq.validate(), Err(TimelineError::InvalidAudioSetting { field: "eq_gains_db", .. })));

    let bad_nr = TrackAudioSettings { noise_reduction_strength: 1.5, ..TrackAudioSettings::default() };
    assert!(matches!(
        bad_nr.validate(),
        Err(TimelineError::InvalidAudioSetting { field: "noise_reduction_strength", .. })
    ));

    let clamped = TrackAudioSettings {
        gain_db: 99.0,
        eq_gains_db: [-25.0, f32::INFINITY, 20.0, -5.0],
        noise_reduction_strength: -3.0,
        effects: vec![AudioEffectKind::Reverb, AudioEffectKind::Reverb, AudioEffectKind::Echo],
        ..TrackAudioSettings::default()
    }
    .clamped();
    clamped.validate().unwrap();
    assert_eq!(clamped.gain_db, 12.0);
    assert_eq!(clamped.eq_gains_db, [-12.0, 0.0, 12.0, -5.0]);
    assert_eq!(clamped.noise_reduction_strength, 0.0);
    assert_eq!(clamped.effects, vec![AudioEffectKind::Reverb, AudioEffectKind::Echo]);

    let vk_oob = VolumeKeyframe::new(2_000_000_000, 0.0);
    assert!(vk_oob.validate(1_000_000_000).is_err());
    let vk_bad_db = VolumeKeyframe::new(500_000_000, -50.0);
    assert!(vk_bad_db.validate(1_000_000_000).is_err());

    let vk_clamped = VolumeKeyframe::clamped(500_000_000, -90.0);
    assert_eq!(vk_clamped.gain_db, -48.0);
    vk_clamped.validate(1_000_000_000).unwrap();

    assert_eq!(eq_band_index(EqBand::Low), 0);
    assert_eq!(eq_band_index(EqBand::LowMid), 1);
    assert_eq!(eq_band_index(EqBand::Mid), 2);
    assert_eq!(eq_band_index(EqBand::High), 3);
    assert_eq!(db_to_linear(MIN_TRACK_GAIN_DB), 0.0);
    assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
    assert!((db_to_linear(-6.0) - 0.501_187).abs() < 1e-3);
}

#[test]
fn ensure_mixer_audio_tracks_is_idempotent_and_preserves_a1() {
    let mut project = Project::new("Mix");
    let ids = project.timeline.ensure_mixer_audio_tracks();
    assert_eq!(ids.len(), 3);
    let names: Vec<_> = project
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, vec!["A1", "A2 Música", "A3 SFX"]);

    let ids_second = project.timeline.ensure_mixer_audio_tracks();
    assert_eq!(ids_second, ids);
}

#[test]
fn split_clip_partitions_volume_keyframes_cleanly() {
    let mut project = Project::new("Split Volume KF");
    let a1_id = project
        .timeline
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .unwrap()
        .id;

    let mut clip = Clip::new(AssetId::new(), 1_000_000_000, 0, 4_000_000_000);
    clip.volume_keyframes = vec![
        VolumeKeyframe::new(500_000_000, -6.0),
        VolumeKeyframe::new(2_000_000_000, 3.0),
        VolumeKeyframe::new(3_500_000_000, -12.0),
    ];
    let left_id = project.timeline.add_clip(a1_id, clip).unwrap();

    let (l, r) = project.timeline.split_clip(left_id, 3_000_000_000).unwrap();
    let (_, left_clip) = project.timeline.clip(l).unwrap();
    let (_, right_clip) = project.timeline.clip(r).unwrap();

    assert_eq!(
        left_clip.volume_keyframes,
        vec![VolumeKeyframe::new(500_000_000, -6.0), VolumeKeyframe::new(2_000_000_000, 3.0)]
    );
    assert_eq!(
        right_clip.volume_keyframes,
        vec![VolumeKeyframe::new(0, 3.0), VolumeKeyframe::new(1_500_000_000, -12.0)]
    );
}

#[test]
fn legacy_project_json_without_audio_fields_deserializes_with_neutral_defaults() {
    let legacy_json = r#"{
        "id": "00000000-0000-0000-0000-000000000001",
        "name": "Legado",
        "folders": [],
        "assets": [],
        "timeline": {
            "playhead_ns": 0,
            "tracks": [{
                "id": "00000000-0000-0000-0000-000000000010",
                "name": "A1",
                "kind": "Audio",
                "muted": false,
                "solo": false,
                "gain_db": 0.0,
                "clips": [{
                    "id": "00000000-0000-0000-0000-000000000020",
                    "asset_id": "00000000-0000-0000-0000-000000000030",
                    "timeline_start_ns": 0,
                    "source_in_ns": 0,
                    "duration_ns": 2000000000
                }]
            }]
        },
        "preview_quality": "Proxy720p",
        "export_preset": "YouTube"
    }"#;

    let project = Project::from_json(legacy_json).unwrap();
    let track = &project.timeline.tracks[0];
    assert_eq!(track.audio, TrackAudioSettings::default());
    assert!(track.clips[0].volume_keyframes.is_empty());
}

#[test]
fn history_stack_supports_undo_redo_across_all_audio_commands_and_split() {
    let mut project = Project::new("Audio History");
    let audio_tracks = project.timeline.ensure_mixer_audio_tracks();
    let a1 = audio_tracks[0];
    let clip_id = project
        .timeline
        .add_clip(a1, Clip::new(AssetId::new(), 0, 0, 4_000_000_000))
        .unwrap();

    let mut history = HistoryStack::new();

    // 1. UpdateTrackAudioCommand
    let custom_mix = TrackAudioSettings {
        gain_db: -4.5,
        muted: true,
        solo: false,
        eq_gains_db: [1.0, -2.0, 3.0, 0.5],
        noise_reduction_enabled: true,
        noise_reduction_strength: 0.7,
        voice_leveler_enabled: true,
        voice_preset: None,
        audio_preset: None,
        effects: vec![],
    };
    history
        .execute(Box::new(UpdateTrackAudioCommand::new(a1, custom_mix.clone())), &mut project)
        .unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio, custom_mix);
    assert!(project.timeline.track(a1).unwrap().muted);
    assert_eq!(project.timeline.track(a1).unwrap().gain_db, -4.5);

    // 2. ApplyVoicePresetCommand
    history
        .execute(Box::new(ApplyVoicePresetCommand::new(a1, VoicePreset::Podcast)), &mut project)
        .unwrap();
    assert_eq!(
        project.timeline.track(a1).unwrap().audio.voice_preset,
        Some(VoicePreset::Podcast)
    );
    assert_eq!(
        project.timeline.track(a1).unwrap().audio.eq_gains_db,
        VoicePreset::Podcast.eq_gains_db()
    );

    // 3. ToggleAudioEffectCommand
    history
        .execute(Box::new(ToggleAudioEffectCommand::new(a1, AudioEffectKind::Reverb)), &mut project)
        .unwrap();
    assert!(project.timeline.track(a1).unwrap().audio.has_effect(AudioEffectKind::Reverb));

    // 4. UpsertVolumeKeyframeCommand (insert + update same timestamp)
    let kf1 = VolumeKeyframe::new(1_000_000_000, -6.0);
    history.execute(Box::new(UpsertVolumeKeyframeCommand::new(clip_id, kf1)), &mut project).unwrap();
    let kf1_updated = VolumeKeyframe::new(1_000_000_000, 3.0);
    history
        .execute(Box::new(UpsertVolumeKeyframeCommand::new(clip_id, kf1_updated)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf1_updated]);

    // 5. RemoveVolumeKeyframeCommand
    history
        .execute(Box::new(RemoveVolumeKeyframeCommand::new(clip_id, 1_000_000_000)), &mut project)
        .unwrap();
    assert!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes.is_empty());

    // Desfaz tudo em ordem inversa verificando cada passo
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf1_updated]);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf1]);

    history.undo(&mut project).unwrap();
    assert!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes.is_empty());

    history.undo(&mut project).unwrap();
    assert!(!project.timeline.track(a1).unwrap().audio.has_effect(AudioEffectKind::Reverb));

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio, custom_mix);

    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio, TrackAudioSettings::default());

    // Refaz todos os 6 comandos
    for _ in 0..6 {
        history.redo(&mut project).unwrap();
    }
    assert!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes.is_empty());
    assert!(project.timeline.track(a1).unwrap().audio.has_effect(AudioEffectKind::Reverb));

    // SplitClipCommand também restaura volume_keyframes no Undo
    history.undo(&mut project).unwrap();
    let split_cmd = SplitClipCommand::new(clip_id, 2_000_000_000);
    let right_id = split_cmd.right_clip_id();
    history.execute(Box::new(split_cmd), &mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf1_updated]);
    assert!(project.timeline.clip(right_id).unwrap().1.volume_keyframes.is_empty());

    history.undo(&mut project).unwrap();
    assert!(project.timeline.clip(right_id).is_none());
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf1_updated]);
}
