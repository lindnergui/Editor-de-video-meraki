use meraki_core::{
    AudioEffectKind, ClipId, ClipTransform, EqBand, PreviewQuality, TrackAudioSettings, TrackId,
    TrackKind, VoicePreset, VolumeKeyframe,
};
use meraki_engine::{
    spawn_engine, EngineCommand, EngineEvent, TimelineRenderClip, TrackAudioMix,
};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempWavGuard(PathBuf);

impl TempWavGuard {
    fn new(stem: &str) -> Self {
        let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        Self(std::env::temp_dir().join(format!("meraki_dsp_{stem}_{pid}_{id}.wav")))
    }
}

impl Drop for TempWavGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Gera WAV PCM 16-bit estéreo 48 kHz em Rust puro para teste real do grafo de áudio.
fn write_stereo_sine_wav(path: &Path, duration_secs: f32, freq_hz: f32, amplitude: f32) {
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;
    const BITS_PER_SAMPLE: u16 = 16;
    let total_frames = (SAMPLE_RATE as f32 * duration_secs).round() as u32;
    let data_bytes = total_frames * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
    let riff_size = 36 + data_bytes;

    let mut file = File::create(path).expect("deve criar WAV temporário");
    file.write_all(b"RIFF").unwrap();
    file.write_all(&riff_size.to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&CHANNELS.to_le_bytes()).unwrap();
    file.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&BITS_PER_SAMPLE.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_bytes.to_le_bytes()).unwrap();

    let mut pcm = Vec::with_capacity(data_bytes as usize);
    for n in 0..total_frames {
        let t = n as f32 / SAMPLE_RATE as f32;
        let sample = (2.0 * std::f32::consts::PI * freq_hz * t).sin() * amplitude;
        let quantized = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        pcm.extend_from_slice(&quantized.to_le_bytes());
        pcm.extend_from_slice(&quantized.to_le_bytes());
    }
    file.write_all(&pcm).unwrap();
}

#[test]
fn engine_applies_live_dsp_eq_solo_mute_volume_keyframes_and_streams_db_levels() {
    let wav_a = TempWavGuard::new("voice");
    let wav_b = TempWavGuard::new("music");
    write_stereo_sine_wav(&wav_a.0, 2.0, 440.0, 0.7);
    write_stereo_sine_wav(&wav_b.0, 2.0, 660.0, 0.5);

    let track_a = TrackId::new();
    let track_b = TrackId::new();

    let mut settings_a = TrackAudioSettings::default();
    VoicePreset::Podcast.apply_to(&mut settings_a);
    let settings_b = TrackAudioSettings { gain_db: -6.0, ..TrackAudioSettings::default() };

    let clip_a = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track_a,
        path: wav_a.0.clone(),
        proxy_path: None,
        track_kind: TrackKind::Audio,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 2_000_000_000,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: vec![
            VolumeKeyframe::new(0, -12.0),
            VolumeKeyframe::new(500_000_000, 0.0),
            VolumeKeyframe::new(1_500_000_000, -6.0),
        ],
    };
    let clip_b = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track_b,
        path: wav_b.0.clone(),
        proxy_path: None,
        volume_keyframes: Vec::new(),
        ..clip_a.clone()
    };

    let handle = spawn_engine();
    let ready = handle
        .events()
        .recv_timeout(Duration::from_secs(30))
        .expect("motor deve emitir EngineReady");
    assert!(matches!(ready, EngineEvent::EngineReady));

    handle
        .send(EngineCommand::SyncTimeline {
            transitions: Vec::new(),
            clips: vec![clip_a, clip_b],
            audio_mix: vec![
                TrackAudioMix { track_id: track_a, settings: settings_a },
                TrackAudioMix { track_id: track_b, settings: settings_b },
            ],
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
    handle.send(EngineCommand::Play).unwrap();

    // Verifica que AudioLevels (~30 Hz) chegam com sinal não silencioso durante reprodução
    let mut non_silent_frame = None;
    let first_deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < first_deadline && non_silent_frame.is_none() {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::AudioLevels(frame) => {
                    assert!(frame.master_peak_db.is_finite());
                    assert!(frame.master_rms_db.is_finite());
                    assert!((-60.0..=0.0).contains(&frame.master_peak_db));
                    assert!((-60.0..=0.0).contains(&frame.master_rms_db));
                    if frame.master_peak_db > -50.0 {
                        non_silent_frame = Some(frame);
                    }
                }
                EngineEvent::Error(err) => panic!("erro inesperado na reprodução DSP: {err}"),
                _ => {}
            }
        }
    }
    let meter = non_silent_frame.expect("deve receber AudioLevels não silencioso durante Play");
    assert_eq!(meter.tracks.len(), 2);

    // Mutações AO VIVO sem reconstruir pipeline GES (4 bandas EQ, ganho, NR, leveler, efeitos, mute, solo)
    handle.send(EngineCommand::UpdateEqBand { track_id: track_a, band: EqBand::Low, gain_db: 4.0 }).unwrap();
    handle.send(EngineCommand::UpdateEqBand { track_id: track_a, band: EqBand::LowMid, gain_db: -2.0 }).unwrap();
    handle.send(EngineCommand::UpdateEqBand { track_id: track_a, band: EqBand::Mid, gain_db: 3.0 }).unwrap();
    handle.send(EngineCommand::UpdateEqBand { track_id: track_a, band: EqBand::High, gain_db: 2.5 }).unwrap();
    handle.send(EngineCommand::SetTrackGain { track_id: track_a, gain_db: -3.0 }).unwrap();
    handle.send(EngineCommand::SetNoiseReduction { track_id: track_a, enabled: true, strength: 0.6 }).unwrap();
    handle.send(EngineCommand::SetVoiceLeveler { track_id: track_a, enabled: true }).unwrap();
    handle.send(EngineCommand::ToggleTrackAudioEffect { track_id: track_a, effect: AudioEffectKind::Echo, enabled: true }).unwrap();
    handle.send(EngineCommand::ToggleTrackAudioEffect { track_id: track_a, effect: AudioEffectKind::Reverb, enabled: true }).unwrap();
    handle.send(EngineCommand::SetTrackMute { track_id: track_a, muted: true }).unwrap();
    handle.send(EngineCommand::SetTrackMute { track_id: track_a, muted: false }).unwrap();
    handle.send(EngineCommand::SetTrackSolo { track_id: track_b, solo: true }).unwrap();
    handle.send(EngineCommand::SetTrackSolo { track_id: track_b, solo: false }).unwrap();
    handle.send(EngineCommand::ToggleTrackAudioEffect { track_id: track_a, effect: AudioEffectKind::Echo, enabled: false }).unwrap();
    handle.send(EngineCommand::ToggleTrackAudioEffect { track_id: track_a, effect: AudioEffectKind::Reverb, enabled: false }).unwrap();

    let mut post_live_levels = 0_u32;
    let live_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < live_deadline && post_live_levels < 3 {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::AudioLevels(frame) => {
                    assert!(frame.master_peak_db.is_finite());
                    post_live_levels += 1;
                }
                EngineEvent::Error(err) => panic!("erro inesperado após comandos DSP ao vivo: {err}"),
                _ => {}
            }
        }
    }
    assert!(post_live_levels >= 1, "motor deve continuar emitindo AudioLevels após updates ao vivo");

    // Parâmetro não finito deve gerar EngineEvent::Error sem derrubar o motor
    handle.send(EngineCommand::UpdateEqBand { track_id: track_a, band: EqBand::Low, gain_db: f32::NAN }).unwrap();
    let mut got_validation_err = false;
    let err_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < err_deadline && !got_validation_err {
        if let Ok(EngineEvent::Error(_)) = handle.events().recv_timeout(Duration::from_millis(100)) {
            got_validation_err = true;
        }
    }
    assert!(got_validation_err, "EQ NaN deve reportar EngineEvent::Error sem pânico");

    handle.send(EngineCommand::Shutdown).unwrap();
}
