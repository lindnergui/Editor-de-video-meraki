use meraki_core::{
    auto_eq_gains_f32, clamp_eq_gain_db, clamp_eq_gains_db, clamp_track_gain_db, compute_auto_eq,
    eq_band_spec, ApplyAudioPreset, ApplyAudioPresetCommand, AudioEffectKind, AudioPreset,
    AutoEqTarget, EqBand, EqFilterType, HistoryStack, Project, ResetEqBand, ResetEqBandCommand,
    SetMasterVolume, SetMasterVolumeCommand, SetTrackEq, SetTrackEqCommand, SetTrackVolume,
    SetTrackVolumeCommand, SpectrumBands, TimelineError, TrackAudioSettings,
    AUTO_EQ_MAX_CORRECTION_DB, AUTO_EQ_MIN_CORRECTION_DB, EQ_BAND_FREQUENCIES_HZ, EQ_BAND_SPECS,
    EQ_BAND_WIDTHS_HZ, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB,
};

#[test]
fn eq_band_specs_match_musical_frequencies_bandwidths_and_filters() {
    assert_eq!(EQ_BAND_SPECS.len(), 4);
    assert_eq!(EQ_BAND_FREQUENCIES_HZ, [90.0, 320.0, 2_200.0, 8_500.0]);
    assert_eq!(EQ_BAND_WIDTHS_HZ, [140.0, 300.0, 1_800.0, 5_000.0]);

    let low = eq_band_spec(EqBand::Low);
    assert_eq!(low.label, "Graves");
    assert_eq!(low.freq_hz, 90.0);
    assert_eq!(low.bandwidth_hz, 140.0);
    assert_eq!(low.filter_type, EqFilterType::LowShelf);

    let low_mid = eq_band_spec(EqBand::LowMid);
    assert_eq!(low_mid.label, "Médios-Graves");
    assert_eq!(low_mid.freq_hz, 320.0);
    assert_eq!(low_mid.bandwidth_hz, 300.0);
    assert_eq!(low_mid.filter_type, EqFilterType::Peak);

    let mid = eq_band_spec(EqBand::Mid);
    assert_eq!(mid.label, "Médios");
    assert_eq!(mid.freq_hz, 2_200.0);
    assert_eq!(mid.bandwidth_hz, 1_800.0);
    assert_eq!(mid.filter_type, EqFilterType::Peak);

    let high = eq_band_spec(EqBand::High);
    assert_eq!(high.label, "Agudos");
    assert_eq!(high.freq_hz, 8_500.0);
    assert_eq!(high.bandwidth_hz, 5_000.0);
    assert_eq!(high.filter_type, EqFilterType::HighShelf);
}

#[test]
fn audio_presets_expose_curves_flags_and_preserve_mixer_state() {
    assert_eq!(AudioPreset::ALL.len(), 7);
    assert_eq!(AudioPreset::YouTubeVoice.label(), "Fala / YouTube");
    assert_eq!(AudioPreset::YouTubeVoice.eq_gains_db(), [-2.5, -1.5, 3.0, 1.8]);
    assert_eq!(AudioPreset::YouTubeVoice.voice_leveler(), Some(true));

    assert_eq!(AudioPreset::Music.label(), "Música");
    assert_eq!(AudioPreset::Music.eq_gains_db(), [2.2, -0.8, 0.8, 2.0]);
    assert_eq!(AudioPreset::Music.voice_leveler(), Some(false));

    assert_eq!(AudioPreset::Podcast.label(), "Podcast");
    assert_eq!(AudioPreset::Podcast.eq_gains_db(), [1.2, -1.0, 2.8, 1.5]);
    assert_eq!(AudioPreset::Podcast.voice_leveler(), Some(true));

    assert_eq!(AudioPreset::ClearVoice.label(), "Voz Clara");
    assert_eq!(AudioPreset::ClearVoice.eq_gains_db(), [-4.0, -2.0, 3.8, 2.2]);

    assert_eq!(AudioPreset::Cinema.label(), "Cinema");
    assert_eq!(AudioPreset::Cinema.eq_gains_db(), [3.0, -0.5, 1.2, 2.4]);

    assert_eq!(AudioPreset::Radio.label(), "Voz de Rádio");
    assert_eq!(AudioPreset::Radio.eq_gains_db(), [-6.0, 2.5, 4.0, -3.5]);

    assert_eq!(AudioPreset::Flat.label(), "Padrão / Plano");
    assert_eq!(AudioPreset::Flat.eq_gains_db(), [0.0, 0.0, 0.0, 0.0]);

    for preset in AudioPreset::ALL {
        assert!(!preset.label().is_empty());
        assert!(!preset.description().is_empty());
        for g in preset.eq_gains_db() {
            assert!((MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&g));
        }

        let mut settings = TrackAudioSettings {
            gain_db: -4.0,
            muted: true,
            solo: true,
            noise_reduction_enabled: true,
            voice_leveler_enabled: false,
            effects: vec![AudioEffectKind::Reverb],
            ..TrackAudioSettings::default()
        };
        preset.apply_to(&mut settings);
        settings.validate().unwrap();
        assert_eq!(settings.eq_gains_db, preset.eq_gains_db());
        assert_eq!(settings.audio_preset, Some(preset));
        assert_eq!(settings.gain_db, -4.0);
        assert!(settings.muted);
        assert!(settings.solo);
        assert_eq!(settings.effects, vec![AudioEffectKind::Reverb]);
    }
}

#[test]
fn clamping_guards_eq_and_track_gains_safely() {
    assert_eq!(clamp_eq_gain_db(-24.0), -12.0);
    assert_eq!(clamp_eq_gain_db(18.5), 12.0);
    assert_eq!(clamp_eq_gain_db(f32::NAN), 0.0);
    assert_eq!(clamp_eq_gain_db(f32::INFINITY), 0.0);
    assert_eq!(
        clamp_eq_gains_db([-30.0, f32::NEG_INFINITY, 3.25, 42.0]),
        [-12.0, 0.0, 3.25, 12.0]
    );
    assert_eq!(clamp_track_gain_db(-99.0), -60.0);
    assert_eq!(clamp_track_gain_db(24.0), 12.0);
    assert_eq!(clamp_track_gain_db(f32::NAN), 0.0);
}

#[test]
fn compute_auto_eq_tames_muddy_bass_and_keeps_balanced_audio_flat() {
    // 1. Áudio abafado com excesso de graves/médios-graves e pouca definição em médios
    let muddy_speech = SpectrumBands::new([-8.0, -10.0, -24.0, -22.0]);
    let speech_corr = compute_auto_eq(muddy_speech, AutoEqTarget::SpeechYouTube);
    for gain in speech_corr {
        assert!(
            (AUTO_EQ_MIN_CORRECTION_DB..=AUTO_EQ_MAX_CORRECTION_DB).contains(&gain),
            "correção fora de [-4.5, +4.5]: {gain}"
        );
    }
    assert!(speech_corr[0] < 0.0, "deve atenuar graves em excesso: {speech_corr:?}");
    assert!(speech_corr[1] < 0.0, "deve atenuar médios-graves abafados: {speech_corr:?}");
    assert!(speech_corr[2] > 0.0, "deve realçar presença vocal nos médios: {speech_corr:?}");

    // 2. Independência de volume absoluto (somando +18 dB em todas as bandas dá o mesmo resultado)
    let louder_muddy = SpectrumBands::new([10.0, 8.0, -6.0, -4.0]);
    let louder_corr = compute_auto_eq(louder_muddy, AutoEqTarget::SpeechYouTube);
    for i in 0..4 {
        assert!((speech_corr[i] - louder_corr[i]).abs() < 1e-9);
    }

    // 3. Áudio já equilibrado no perfil-alvo gera correção próxima de zero
    let balanced_speech = SpectrumBands::new([-20.0, -19.0, -15.8, -17.2]);
    let near_zero = compute_auto_eq(balanced_speech, AutoEqTarget::SpeechYouTube);
    for gain in near_zero {
        assert!(gain.abs() < 1e-9, "esperava correção ~0, veio {gain}");
    }

    // 4. Alvo Music em espectro totalmente plano cria curva em V suave dentro de [-4.5, +4.5]
    let flat_music = SpectrumBands::new([-14.0, -14.0, -14.0, -14.0]);
    let music_corr = compute_auto_eq(flat_music, AutoEqTarget::Music);
    assert!(music_corr[0] > 0.0 && music_corr[3] > 0.0);
    assert!(music_corr[1] < 0.0 && music_corr[2] < 0.0);
    for gain in music_corr {
        assert!((AUTO_EQ_MIN_CORRECTION_DB..=AUTO_EQ_MAX_CORRECTION_DB).contains(&gain));
    }
    let gains_f32 = auto_eq_gains_f32(music_corr);
    assert_eq!(clamp_eq_gains_db(gains_f32), gains_f32);

    // 5. Valores não finitos retornam neutro sem NaN
    assert_eq!(
        compute_auto_eq(SpectrumBands::new([f64::NAN, -10.0, -10.0, -10.0]), AutoEqTarget::Music),
        [0.0; 4]
    );
}

#[test]
fn history_stack_supports_undo_redo_for_band_reset_eq_preset_and_faders() {
    let mut project = Project::new("EQ History");
    let audio_tracks = project.timeline.ensure_mixer_audio_tracks();
    let a1 = audio_tracks[0];
    let mut history = HistoryStack::new();

    // 1. ApplyAudioPreset (alias + struct)
    history
        .execute(Box::new(ApplyAudioPreset::new(a1, AudioPreset::YouTubeVoice)), &mut project)
        .unwrap();
    let s1 = &project.timeline.track(a1).unwrap().audio;
    assert_eq!(s1.audio_preset, Some(AudioPreset::YouTubeVoice));
    assert_eq!(s1.eq_gains_db, [-2.5, -1.5, 3.0, 1.8]);
    assert!(s1.voice_leveler_enabled);

    // 2. ResetEqBand em uma única banda (apenas Band::Mid volta a 0.0)
    history
        .execute(Box::new(ResetEqBand::new(a1, EqBand::Mid)), &mut project)
        .unwrap();
    let s2 = &project.timeline.track(a1).unwrap().audio;
    assert_eq!(s2.eq_gains_db, [-2.5, -1.5, 0.0, 1.8]);
    assert_eq!(s2.audio_preset, None);

    // Undo restaura exatamente a banda resetada e o preset anterior
    history.undo(&mut project).unwrap();
    let s_undo = &project.timeline.track(a1).unwrap().audio;
    assert_eq!(s_undo.eq_gains_db, [-2.5, -1.5, 3.0, 1.8]);
    assert_eq!(s_undo.audio_preset, Some(AudioPreset::YouTubeVoice));

    // Redo refaz o reset individual
    history.redo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.eq_gains_db, [-2.5, -1.5, 0.0, 1.8]);

    // 3. SetTrackEq consolidado (inclusive aplicando saída de compute_auto_eq)
    let auto_gains = auto_eq_gains_f32(compute_auto_eq(
        SpectrumBands::new([-9.0, -11.0, -20.0, -18.0]),
        AutoEqTarget::SpeechYouTube,
    ));
    history
        .execute(Box::new(SetTrackEqCommand::new(a1, auto_gains)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.eq_gains_db, auto_gains);

    // Validação rejeita ganho fora de [-12, +12]
    assert!(matches!(
        history.execute(Box::new(SetTrackEq::new(a1, [15.0, 0.0, 0.0, 0.0])), &mut project),
        Err(meraki_core::HistoryError::Timeline(TimelineError::InvalidAudioSetting {
            field: "eq_gains_db",
            ..
        }))
    ));
    assert_eq!(project.timeline.track(a1).unwrap().audio.eq_gains_db, auto_gains);

    // 4. SetTrackVolume e SetMasterVolume com persistência JSON + Undo/Redo
    history
        .execute(Box::new(SetTrackVolume::new(a1, -4.5)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.gain_db, -4.5);
    assert_eq!(project.timeline.track(a1).unwrap().gain_db, -4.5);

    history
        .execute(Box::new(SetMasterVolume::new(-2.0)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.master_gain_db, -2.0);

    let json = project.to_json().unwrap();
    let restored = Project::from_json(&json).unwrap();
    assert_eq!(restored.timeline.master_gain_db, -2.0);
    assert_eq!(
        restored.timeline.track(a1).unwrap().audio.eq_gains_db,
        auto_gains
    );

    // Undo Master + TrackVolume + SetTrackEq + ResetEqBand + ApplyAudioPreset
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.master_gain_db, 0.0);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.gain_db, 0.0);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.eq_gains_db, [-2.5, -1.5, 0.0, 1.8]);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio.eq_gains_db, [-2.5, -1.5, 3.0, 1.8]);
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(a1).unwrap().audio, TrackAudioSettings::default());

    // Garante que comandos não-clamped inválidos de volume retornam erro sem corromper
    assert!(matches!(
        history.execute(Box::new(SetTrackVolumeCommand::new(a1, 20.0)), &mut project),
        Err(meraki_core::HistoryError::Timeline(TimelineError::InvalidAudioSetting { .. }))
    ));
    assert!(matches!(
        history.execute(Box::new(SetMasterVolumeCommand::new(20.0)), &mut project),
        Err(meraki_core::HistoryError::Timeline(TimelineError::InvalidAudioSetting {
            field: "master_gain_db",
            ..
        }))
    ));
    assert!(matches!(
        history.execute(Box::new(ResetEqBandCommand::new(a1, EqBand::Low)), &mut project),
        Ok(())
    ));
    assert!(matches!(
        history.execute(Box::new(ApplyAudioPresetCommand::new(a1, AudioPreset::Flat)), &mut project),
        Ok(())
    ));
}
