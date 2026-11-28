use meraki_core::{
    auto_eq_gains_f32, compute_auto_eq, AudioPreset, AutoEqTarget, EqBand, TrackKind,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent, SpectrumBands};
use meraki_ui::components::audio_library::AudioLibraryMessage;
use meraki_ui::components::eq_canvas::EqMessage;
use meraki_ui::components::eq_render::detect_band_double_click;
use meraki_ui::components::mixer::MixerMessage;
use meraki_ui::views::audio::AudioMessage;
use meraki_ui::{MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::time::{Duration, Instant};

fn drain_commands(worker: &meraki_engine::EngineWorkerChannel) -> Vec<EngineCommand> {
    let mut cmds = Vec::new();
    while let Some(c) = worker.try_recv_command() {
        cmds.push(c);
    }
    cmds
}

#[test]
fn eq_double_click_resets_single_band_and_supports_undo() {
    let base = Instant::now();
    let mut last_click = None;
    assert!(!detect_band_double_click(&mut last_click, EqBand::Mid, base));
    assert!(!detect_band_double_click(
        &mut last_click,
        EqBand::Low,
        base + Duration::from_millis(120)
    ));
    assert!(detect_band_double_click(
        &mut last_click,
        EqBand::Low,
        base + Duration::from_millis(280)
    ));
    assert!(!detect_band_double_click(
        &mut last_click,
        EqBand::Low,
        base + Duration::from_millis(400)
    ));
    assert!(!detect_band_double_click(
        &mut last_click,
        EqBand::Low,
        base + Duration::from_millis(900)
    ));

    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    let a1_id = app.active_audio_track_id().unwrap();

    let _ = app.update(Message::Audio(AudioMessage::Library(
        AudioLibraryMessage::ApplyAudioPreset(AudioPreset::YouTubeVoice),
    )));
    let _ = drain_commands(&worker);

    // Duplo-clique apenas na banda Mid reseta somente índice 2 para 0.0 dB
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandReset(EqBand::Mid))));
    assert_eq!(
        app.project().timeline.track(a1_id).unwrap().audio.eq_gains_db,
        [-2.5, -1.5, 0.0, 1.8]
    );
    assert!(app.project_io().is_dirty);
    let cmds = drain_commands(&worker);
    assert!(cmds.iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { track_eq: Some((id, g)), .. }
            if *id == a1_id && *g == [-2.5_f32, -1.5, 0.0, 1.8].map(f64::from)
    )));

    // Undo restaura apenas o ganho original da banda Mid e o preset anterior
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    let restored = &app.project().timeline.track(a1_id).unwrap().audio;
    assert_eq!(restored.eq_gains_db, [-2.5, -1.5, 3.0, 1.8]);
    assert_eq!(restored.audio_preset, Some(AudioPreset::YouTubeVoice));
}

#[test]
fn mixer_double_click_resets_only_target_channel_to_zero_db() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    let audio_ids: Vec<_> = app
        .project()
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .map(|t| t.id)
        .collect();
    let (a1, a2) = (audio_ids[0], audio_ids[1]);

    // Ajusta A1, A2 e Master para valores diferentes de 0 dB
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainChanged {
        track_id: a1,
        gain_db: -6.5,
    })));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainReleased(a1))));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainChanged {
        track_id: a2,
        gain_db: -3.0,
    })));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainReleased(a2))));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::MasterGainChanged(-4.0))));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::MasterGainReleased)));
    let _ = drain_commands(&worker);

    // Duplo-clique em A1 reseta apenas A1
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainReset(a1))));
    assert_eq!(app.project().timeline.track(a1).unwrap().audio.gain_db, 0.0);
    assert_eq!(app.project().timeline.track(a2).unwrap().audio.gain_db, -3.0);
    assert_eq!(app.project().timeline.master_gain_db, -4.0);
    assert!(drain_commands(&worker).iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { track_volumes: Some((id, db, false)), .. }
            if *id == a1 && (*db - 0.0).abs() < 1e-6
    )));

    // Duplo-clique em Master reseta Master para 0 dB
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::MasterGainReset)));
    assert_eq!(app.project().timeline.master_gain_db, 0.0);
    assert_eq!(app.project().timeline.track(a2).unwrap().audio.gain_db, -3.0);
    assert!(drain_commands(&worker).iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { master_volume_db: Some(db), .. } if (*db - 0.0).abs() < 1e-6
    )));

    // Undo restaura Master e depois A1
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert_eq!(app.project().timeline.master_gain_db, -4.0);
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert_eq!(app.project().timeline.track(a1).unwrap().audio.gain_db, -6.5);
}

#[test]
fn dragging_eq_and_mixer_faders_emits_update_audio_live_without_polluting_history() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    let a1 = app.active_audio_track_id().unwrap();
    let _ = drain_commands(&worker);
    assert_eq!(app.history().undo_len(), 0);

    // 1. Arraste do EQ envia UpdateAudioLive sem criar Undo; release cria exatamente 1
    for g in [2.0, 4.0, 5.5] {
        let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandDragged {
            band: EqBand::High,
            gain_db: g,
        })));
    }
    assert_eq!(app.history().undo_len(), 0);
    let eq_live_cmds = drain_commands(&worker);
    assert_eq!(
        eq_live_cmds
            .iter()
            .filter(|c| matches!(c, EngineCommand::UpdateAudioLive { track_eq: Some((id, _)), .. } if *id == a1))
            .count(),
        3
    );

    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandReleased {
        band: EqBand::High,
        gain_db: 5.5,
    })));
    assert_eq!(app.history().undo_len(), 1);
    assert!(app.project_io().is_dirty);

    // 2. Arraste do fader da trilha envia UpdateAudioLive sem Undo; release cria 1 Undo
    for db in [-2.0, -5.0, -7.5] {
        let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainChanged {
            track_id: a1,
            gain_db: db,
        })));
    }
    assert_eq!(app.history().undo_len(), 1);
    let vol_cmds = drain_commands(&worker);
    assert_eq!(
        vol_cmds
            .iter()
            .filter(|c| matches!(c, EngineCommand::UpdateAudioLive { track_volumes: Some((id, _, _)), .. } if *id == a1))
            .count(),
        3
    );
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::TrackGainReleased(a1))));
    assert_eq!(app.history().undo_len(), 2);

    // 3. Arraste do fader Master envia UpdateAudioLive sem Undo; release cria 1 Undo
    for db in [-1.5, -3.0, -4.5] {
        let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::MasterGainChanged(db))));
    }
    assert_eq!(app.history().undo_len(), 2);
    let master_cmds = drain_commands(&worker);
    assert_eq!(
        master_cmds
            .iter()
            .filter(|c| matches!(c, EngineCommand::UpdateAudioLive { master_volume_db: Some(_), .. }))
            .count(),
        3
    );
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::MasterGainReleased)));
    assert_eq!(app.history().undo_len(), 3);
    assert_eq!(app.project().timeline.master_gain_db, -4.5);

    // 4. Slider de Redução de Ruído envia UpdateAudioLive ao vivo e 1 Undo ao soltar
    for s in [0.65, 0.85] {
        let _ = app.update(Message::Audio(AudioMessage::Eq(
            EqMessage::NoiseReductionStrengthChanged(s),
        )));
    }
    assert_eq!(app.history().undo_len(), 3);
    assert_eq!(
        drain_commands(&worker)
            .iter()
            .filter(|c| matches!(c, EngineCommand::UpdateAudioLive { track_enhancements: Some((id, _)), .. } if *id == a1))
            .count(),
        2
    );
    let _ = app.update(Message::Audio(AudioMessage::Eq(
        EqMessage::NoiseReductionStrengthReleased,
    )));
    assert_eq!(app.history().undo_len(), 4);
}

#[test]
fn audio_presets_and_auto_eq_work_with_spectrum_and_notice() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    let a1 = app.active_audio_track_id().unwrap();
    let _ = drain_commands(&worker);

    // Presets Fala / YouTube e Música
    let _ = app.update(Message::Audio(AudioMessage::Library(
        AudioLibraryMessage::ApplyAudioPreset(AudioPreset::YouTubeVoice),
    )));
    let s1 = &app.project().timeline.track(a1).unwrap().audio;
    assert_eq!(s1.audio_preset, Some(AudioPreset::YouTubeVoice));
    assert_eq!(s1.eq_gains_db, AudioPreset::YouTubeVoice.eq_gains_db());
    assert!(s1.voice_leveler_enabled);
    assert!(drain_commands(&worker).iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive {
            track_eq: Some((id, g)),
            track_enhancements: Some((_, enh)),
            ..
        } if *id == a1 && *g == [-2.5_f32, -1.5, 3.0, 1.8].map(f64::from) && enh.voice_leveler_enabled
    )));

    let _ = app.update(Message::Audio(AudioMessage::Library(
        AudioLibraryMessage::ApplyAudioPreset(AudioPreset::Music),
    )));
    let s2 = &app.project().timeline.track(a1).unwrap().audio;
    assert_eq!(s2.audio_preset, Some(AudioPreset::Music));
    assert_eq!(s2.eq_gains_db, AudioPreset::Music.eq_gains_db());
    assert!(!s2.voice_leveler_enabled);
    let _ = drain_commands(&worker);

    // Sem espectro analisado: solicita espectro ao motor e exibe aviso discreto
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::AutoAdjust(
        AutoEqTarget::SpeechYouTube,
    ))));
    assert!(app.audio_state().auto_eq_notice.is_some());
    assert!(drain_commands(&worker)
        .iter()
        .any(|c| matches!(c, EngineCommand::RequestAudioSpectrum)));

    // Recebe AudioSpectrum real do motor e limpa aviso
    let measured = SpectrumBands::new([-8.0, -10.0, -24.0, -22.0]);
    let _ = app.update(Message::EngineEvent(EngineEvent::AudioSpectrum(measured)));
    assert!(app.audio_state().auto_eq_notice.is_none());

    // Aplica Auto Voz
    let expected_voice = auto_eq_gains_f32(compute_auto_eq(measured, AutoEqTarget::SpeechYouTube));
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::AutoAdjust(
        AutoEqTarget::SpeechYouTube,
    ))));
    assert_eq!(app.project().timeline.track(a1).unwrap().audio.eq_gains_db, expected_voice);
    assert!(drain_commands(&worker).iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { track_eq: Some((id, g)), .. }
            if *id == a1 && *g == expected_voice.map(f64::from)
    )));

    // Aplica Auto Música
    let expected_music = auto_eq_gains_f32(compute_auto_eq(measured, AutoEqTarget::Music));
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::AutoAdjust(
        AutoEqTarget::Music,
    ))));
    assert_eq!(app.project().timeline.track(a1).unwrap().audio.eq_gains_db, expected_music);

    // Undo retorna para Auto Voz
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert_eq!(app.project().timeline.track(a1).unwrap().audio.eq_gains_db, expected_voice);
}
