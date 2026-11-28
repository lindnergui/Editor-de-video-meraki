use iced::Rectangle;
use meraki_core::{
    AudioEffectKind, EqBand, MediaMetadata, TrackKind, VoicePreset, VolumeKeyframe,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{AudioMeterFrame, EngineCommand, EngineEvent, TrackAudioLevel};
use meraki_ui::components::audio_library::AudioLibraryMessage;
use meraki_ui::components::audio_timeline::{
    track_band_layout, volume_db_to_y, y_to_volume_db, AudioTimelineMessage,
};
use meraki_ui::components::eq_canvas::{
    eq_gain_to_y, eq_y_to_gain, sample_eq_curve_points, EqMessage,
};
use meraki_ui::components::mixer::{db_meter_segment_color, MixerMessage};
use meraki_ui::views::audio::{resolve_selected_audio_track, AudioMessage};
use meraki_ui::{theme, MerakiApp, Message, PlaybackMessage, WorkspaceTab};
use std::path::PathBuf;

fn drain_commands(worker: &meraki_engine::EngineWorkerChannel) -> Vec<EngineCommand> {
    let mut cmds = Vec::new();
    while let Some(c) = worker.try_recv_command() {
        cmds.push(c);
    }
    cmds
}

#[test]
fn opening_audio_tab_ensures_mixer_tracks_and_switches_active_track() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    assert_eq!(app.active_tab(), WorkspaceTab::Audio);

    let audio_tracks: Vec<_> = app
        .project()
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .collect();
    assert_eq!(audio_tracks.len(), 3);
    assert_eq!(audio_tracks[0].name, "A1");
    assert_eq!(audio_tracks[1].name, "A2 Música");
    assert_eq!(audio_tracks[2].name, "A3 SFX");

    let a2_id = audio_tracks[1].id;
    assert_eq!(app.audio_state().selected_track_id, Some(audio_tracks[0].id));

    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::SelectTrack(a2_id))));
    assert_eq!(app.audio_state().selected_track_id, Some(a2_id));
    assert_eq!(
        resolve_selected_audio_track(app.project(), app.audio_state().selected_track_id)
            .map(|t| t.id),
        Some(a2_id)
    );
}

#[test]
fn voice_preset_eq_drag_enhancements_effects_and_mute_solo_sync_and_undo_redo() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));
    let a1_id = app.active_audio_track_id().unwrap();
    let a2_id = app.project().timeline.tracks.iter().find(|t| t.name == "A2 Música").unwrap().id;
    let _ = drain_commands(&worker);

    // 1. Aplica VoicePreset::Podcast na trilha selecionada e confere comandos live no motor
    let _ = app.update(Message::Audio(AudioMessage::Library(AudioLibraryMessage::ApplyVoicePreset(
        VoicePreset::Podcast,
    ))));
    let s1 = app.project().timeline.track(a1_id).unwrap().audio.clone();
    assert_eq!(s1.voice_preset, Some(VoicePreset::Podcast));
    assert_eq!(s1.eq_gains_db, VoicePreset::Podcast.eq_gains_db());
    assert!(s1.noise_reduction_enabled);
    assert!(s1.voice_leveler_enabled);

    let cmds = drain_commands(&worker);
    assert!(matches!(cmds.as_slice(), [EngineCommand::UpdateAudioLive {
        track_eq: Some((id, gains)), track_enhancements: Some((_, config)), ..
    }] if *id == a1_id && *gains == s1.eq_gains_db.map(f64::from)
        && config.noise_reduction_enabled && config.voice_leveler_enabled));

    // 2. Arrasta nó de EQ ao vivo e registra 1 único comando no release
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandDragged {
        band: EqBand::Low,
        gain_db: 7.5,
    })));
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandDragged {
        band: EqBand::Low,
        gain_db: 8.0,
    })));
    assert!(drain_commands(&worker)
        .iter()
        .any(|c| matches!(c, EngineCommand::UpdateAudioLive { track_eq: Some((id, gains)), .. } if *id == a1_id && gains[0] == 8.0)));

    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::BandReleased {
        band: EqBand::Low,
        gain_db: 8.5,
    })));
    assert_eq!(app.project().timeline.track(a1_id).unwrap().audio.eq_gains_db[0], 8.5);
    assert_eq!(app.project().timeline.track(a1_id).unwrap().audio.voice_preset, None);
    let _ = drain_commands(&worker);

    // 3. Desfaz drag do EQ (volta direto ao estado exato do preset Podcast!)
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert_eq!(app.project().timeline.track(a1_id).unwrap().audio.eq_gains_db, VoicePreset::Podcast.eq_gains_db());
    assert_eq!(app.project().timeline.track(a1_id).unwrap().audio.voice_preset, Some(VoicePreset::Podcast));
    assert!(matches!(drain_commands(&worker).last(), Some(EngineCommand::SyncTimeline { .. })));

    // 4. Toggles de Melhorias, Reverb/Eco, Reset EQ, Mute/Solo e Ganho
    let _ = app.update(Message::Audio(AudioMessage::Library(AudioLibraryMessage::ToggleAudioEffect(
        AudioEffectKind::Reverb,
    ))));
    let _ = app.update(Message::Audio(AudioMessage::Library(AudioLibraryMessage::ToggleAudioEffect(
        AudioEffectKind::Echo,
    ))));
    assert!(app.project().timeline.track(a1_id).unwrap().audio.has_effect(AudioEffectKind::Reverb));
    assert!(app.project().timeline.track(a1_id).unwrap().audio.has_effect(AudioEffectKind::Echo));

    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::NoiseReductionToggled(false))));
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::VoiceLevelerToggled(false))));
    let _ = app.update(Message::Audio(AudioMessage::Eq(EqMessage::ResetEq)));
    assert_eq!(app.project().timeline.track(a1_id).unwrap().audio.eq_gains_db, [0.0; 4]);

    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::ToggleMute(a2_id))));
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::ToggleSolo(a1_id))));
    assert!(app.project().timeline.track(a2_id).unwrap().audio.muted);
    assert!(app.project().timeline.track(a1_id).unwrap().audio.solo);

    let live_cmds = drain_commands(&worker);
    assert!(live_cmds.iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { track_enhancements: Some((_, config)), .. } if config.reverb_enabled
    )));
    assert!(live_cmds.iter().any(|c| matches!(
        c,
        EngineCommand::UpdateAudioLive { track_enhancements: Some((_, config)), .. } if config.echo_enabled
    )));
    assert!(live_cmds.iter().any(|c| matches!(c, EngineCommand::UpdateAudioLive { track_volumes: Some((id, _, true)), .. } if *id == a2_id)));
    assert!(live_cmds.iter().any(|c| matches!(c, EngineCommand::SetTrackSolo { track_id, solo: true } if *track_id == a1_id)));

    // 5. Recebe medição dB do motor e atualiza Mixer
    let meter_frame = AudioMeterFrame {
        master_peak_db: -4.2,
        master_rms_db: -9.5,
        tracks: vec![TrackAudioLevel { track_id: a1_id, peak_db: -6.0, rms_db: -12.0 }],
    };
    let _ = app.update(Message::EngineEvent(EngineEvent::AudioLevels(meter_frame.clone())));
    assert_eq!(app.playback().audio_meter, meter_frame);
}

#[test]
fn audio_library_inserts_into_selected_track_and_volume_keyframes_sync_and_undo_redo() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let audio_id = app.import_media(PathBuf::from("trilha.wav"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id: audio_id,
        duration_ns: 6_000_000_000,
        has_audio: true,
        ..MediaMetadata::default()
    })));
    let _ = app.update(Message::SelectTab(WorkspaceTab::Audio));

    // Seleciona trilha A2 Música e envia ativo para ela
    let a2_id = app.project().timeline.tracks.iter().find(|t| t.name == "A2 Música").unwrap().id;
    let _ = app.update(Message::Audio(AudioMessage::Mixer(MixerMessage::SelectTrack(a2_id))));
    let _ = drain_commands(&worker);

    let _ = app.update(Message::Audio(AudioMessage::Library(
        AudioLibraryMessage::AddAudioToSelectedTrack(audio_id),
    )));
    let clip_id = app.selected_clip_id().unwrap();
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().0, a2_id);
    assert!(matches!(drain_commands(&worker).last(), Some(EngineCommand::SyncTimeline { clips, .. }) if clips.len() == 1));

    // Cria keyframe de automação de volume na timeline de áudio
    let kf = VolumeKeyframe::new(1_500_000_000, -14.0);
    let _ = app.update(Message::Audio(AudioMessage::Timeline(
        AudioTimelineMessage::UpsertVolumeKeyframe {
            clip_id,
            previous_offset_ns: None,
            keyframe: kf,
        },
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![kf]);
    let cmds = drain_commands(&worker);
    assert!(matches!(
        cmds.last(),
        Some(EngineCommand::SyncTimeline { clips, .. }) if clips[0].volume_keyframes == vec![kf]
    ));

    // Move o keyframe existente para outro instante
    let moved_kf = VolumeKeyframe::new(2_500_000_000, -24.0);
    let _ = app.update(Message::Audio(AudioMessage::Timeline(
        AudioTimelineMessage::UpsertVolumeKeyframe {
            clip_id,
            previous_offset_ns: Some(1_500_000_000),
            keyframe: moved_kf,
        },
    )));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![moved_kf]);
    let _ = drain_commands(&worker);

    // Clique direito remove o keyframe
    let _ = app.update(Message::Audio(AudioMessage::Timeline(
        AudioTimelineMessage::RemoveVolumeKeyframe { clip_id, time_offset_ns: 2_500_000_000 },
    )));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.volume_keyframes.is_empty());

    // Undo restaura
    let _ = app.update(Message::Playback(PlaybackMessage::Undo));
    assert_eq!(app.project().timeline.clip(clip_id).unwrap().1.volume_keyframes, vec![moved_kf]);
    let _ = app.update(Message::Playback(PlaybackMessage::Redo));
    assert!(app.project().timeline.clip(clip_id).unwrap().1.volume_keyframes.is_empty());

    // Verifica layout de trilhas expandidas vs compactas na timeline de áudio
    let layout = track_band_layout(&app.project().timeline);
    assert_eq!(layout[0].kind, TrackKind::Video);
    assert!(layout[1].height > layout[0].height);
}

#[test]
fn pure_geometry_helpers_for_eq_meter_and_volume_line_stay_within_bounds() {
    let plot = Rectangle { x: 30.0, y: 10.0, width: 300.0, height: 180.0 };
    let y0 = eq_gain_to_y(0.0, plot);
    assert!((y0 - 100.0).abs() < 1e-4);
    assert!((eq_y_to_gain(eq_gain_to_y(-6.5, plot), plot) - (-6.5)).abs() <= 0.1);
    let curve = sample_eq_curve_points([3.0, -4.0, 6.0, -2.0], plot, 10);
    assert!(curve.iter().all(|pt| pt.y >= plot.y && pt.y <= plot.y + plot.height));

    assert_eq!(db_meter_segment_color(-20.0), theme::SUCCESS);
    assert_eq!(db_meter_segment_color(-6.0), theme::WARNING);
    assert_eq!(db_meter_segment_color(-1.5), theme::DANGER);

    let y_vol = volume_db_to_y(-12.0, 40.0, 58.0);
    assert!((y_to_volume_db(y_vol, 40.0, 58.0) - (-12.0)).abs() <= 0.5);
}
