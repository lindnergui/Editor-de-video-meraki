use gstreamer as gst;
use gstreamer_pbutils as gst_pbutils;
use meraki_core::{
    ClipId, ClipTransform, ExportFormat, ExportJobId, ExportPreset, ExportQuality,
    ExportResolution, ExportSettings, TitleOverlay, TitleStyle, TrackId, TrackKind, VoicePreset,
    VolumeKeyframe,
};
use meraki_engine::{
    spawn_engine, EngineCommand, EngineEvent, TimelineRenderClip, TrackAudioMix,
};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(stem: &str) -> Self {
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("meraki_export_{stem}_{}_{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("deve criar diretório temporário");
        Self(dir)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_stereo_sine_wav(path: &Path, duration_secs: f32, freq_hz: f32) {
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;
    let total_frames = (SAMPLE_RATE as f32 * duration_secs).round() as u32;
    let data_bytes = total_frames * 4;
    let riff_size = 36 + data_bytes;

    let mut f = File::create(path).expect("deve criar WAV temporário");
    f.write_all(b"RIFF").unwrap();
    f.write_all(&riff_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16_u32.to_le_bytes()).unwrap();
    f.write_all(&1_u16.to_le_bytes()).unwrap();
    f.write_all(&CHANNELS.to_le_bytes()).unwrap();
    f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
    f.write_all(&(SAMPLE_RATE * 4).to_le_bytes()).unwrap();
    f.write_all(&4_u16.to_le_bytes()).unwrap();
    f.write_all(&16_u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_bytes.to_le_bytes()).unwrap();

    let mut buf = Vec::with_capacity(data_bytes as usize);
    for n in 0..total_frames {
        let s = (2.0 * std::f32::consts::PI * freq_hz * (n as f32 / SAMPLE_RATE as f32)).sin() * 0.5;
        let q = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buf.extend_from_slice(&q.to_le_bytes());
        buf.extend_from_slice(&q.to_le_bytes());
    }
    f.write_all(&buf).unwrap();
}

#[test]
fn exporter_renders_real_wav_mp4_and_mp3_and_supports_cancel_and_error_reporting() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .init();

    let dir = TempDirGuard::new("suite");
    let wav_in = dir.0.join("input.wav");
    write_stereo_sine_wav(&wav_in, 1.0, 440.0);

    let track_v = TrackId::new();
    let track_a = TrackId::new();
    let mut audio_settings = meraki_core::TrackAudioSettings::default();
    VoicePreset::ClearVoice.apply_to(&mut audio_settings);

    let audio_clip = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track_a,
        path: wav_in.clone(),
        proxy_path: None,
        track_kind: TrackKind::Audio,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 1_000_000_000,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: vec![
            VolumeKeyframe::new(0, -12.0),
            VolumeKeyframe::new(500_000_000, 0.0),
        ],
    };

    let handle = spawn_engine();
    let ready = handle.events().recv_timeout(Duration::from_secs(30)).expect("EngineReady");
    assert!(matches!(ready, EngineEvent::EngineReady));

    // 1. Exportação real WAV (AudioOnly)
    let wav_job = ExportJobId::new();
    let mut wav_settings = ExportSettings::from_preset(ExportPreset::AudioOnly);
    wav_settings.output_dir = dir.0.clone();
    wav_settings.file_name = "saida_mix".to_string();

    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: wav_job,
            settings: wav_settings,
            clips: vec![audio_clip.clone()],
            audio_tracks: vec![TrackAudioMix { track_id: track_a, settings: audio_settings.clone() }],
        })
        .unwrap();

    let (wav_out, wav_bytes) = wait_export_finished(&handle, wav_job, "render WAV");
    assert!(wav_out.is_file());
    assert!(wav_bytes > 44);
    assert_eq!(std::fs::metadata(&wav_out).unwrap().len(), wav_bytes);
    let wav_info = discover_rendered_media(&wav_out);
    assert!(wav_info.video_streams().is_empty());
    let wav_audio = wav_info.audio_streams();
    assert_eq!(wav_audio.len(), 1);
    assert_eq!(wav_audio[0].sample_rate(), 48_000);
    assert_eq!(wav_audio[0].channels(), 2);
    assert_eq!(wav_audio[0].depth(), 16);

    // 2. Exportação real MP4 (título + áudio em 720p Leve)
    let title_clip = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track_v,
        path: PathBuf::new(),
        proxy_path: None,
        track_kind: TrackKind::Overlay,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 1_000_000_000,
        transform: ClipTransform::default(),
        title: Some(TitleOverlay::new("Meraki Export", TitleStyle::ModernText)),
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    };
    let mp4_job = ExportJobId::new();
    let mut mp4_settings = ExportSettings::from_preset(ExportPreset::WhatsAppLite);
    mp4_settings.output_dir = dir.0.clone();
    mp4_settings.file_name = "video_teste.mp4".to_string();
    mp4_settings.resolution = ExportResolution::Sd720p;
    mp4_settings.quality = ExportQuality::Low;

    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: mp4_job,
            settings: mp4_settings,
            clips: vec![title_clip.clone(), audio_clip.clone()],
            audio_tracks: vec![TrackAudioMix { track_id: track_a, settings: audio_settings.clone() }],
        })
        .unwrap();

    let (mp4_out, mp4_bytes) = wait_export_finished(&handle, mp4_job, "render MP4 H.264 #1");
    assert!(mp4_out.is_file());
    assert!(mp4_bytes > 0);
    assert_eq!(std::fs::metadata(&mp4_out).unwrap().len(), mp4_bytes);

    // 2b. Segundo render MP4 consecutivo (0,5s) validando recuperação de contexto/encoder no mesmo processo
    let mut short_title = title_clip.clone();
    short_title.duration_ns = 500_000_000;
    let mut short_audio = audio_clip.clone();
    short_audio.duration_ns = 500_000_000;
    let mp4_job_2 = ExportJobId::new();
    let mut mp4_settings_2 = ExportSettings::from_preset(ExportPreset::WhatsAppLite);
    mp4_settings_2.output_dir = dir.0.clone();
    mp4_settings_2.file_name = "video_teste_2.mp4".to_string();
    mp4_settings_2.resolution = ExportResolution::Sd720p;
    mp4_settings_2.quality = ExportQuality::Low;

    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: mp4_job_2,
            settings: mp4_settings_2,
            clips: vec![short_title, short_audio],
            audio_tracks: vec![TrackAudioMix { track_id: track_a, settings: audio_settings }],
        })
        .unwrap();

    let (mp4_out_2, mp4_bytes_2) = wait_export_finished(&handle, mp4_job_2, "render MP4 H.264 #2");
    assert!(mp4_out_2.is_file());
    assert!(mp4_bytes_2 > 0);
    assert_eq!(std::fs::metadata(&mp4_out_2).unwrap().len(), mp4_bytes_2);

    // O render MP3 extrai o áudio do MP4 e ignora um título mais longo na timeline.
    let mp3_job = ExportJobId::new();
    let mut mp3_settings = ExportSettings::from_preset(ExportPreset::AudioOnly);
    mp3_settings.format = ExportFormat::AudioMp3;
    mp3_settings.output_dir = dir.0.clone();
    mp3_settings.file_name = "audio_extraido".to_string();
    let video_with_audio = TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track_v,
        path: mp4_out,
        proxy_path: None,
        track_kind: TrackKind::Video,
        volume_keyframes: Vec::new(),
        ..audio_clip.clone()
    };
    let long_title = TimelineRenderClip { duration_ns: 5_000_000_000, ..title_clip };
    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: mp3_job,
            settings: mp3_settings,
            clips: vec![video_with_audio, long_title],
            audio_tracks: Vec::new(),
        })
        .unwrap();
    let (mp3_out, _) = wait_export_finished(&handle, mp3_job, "render MP3 de mídia A/V");
    let mp3_info = discover_rendered_media(&mp3_out);
    assert_eq!(mp3_info.audio_streams().len(), 1);
    assert!(mp3_info.video_streams().is_empty());
    let mp3_duration = mp3_info.duration().expect("duração do MP3 renderizado");
    assert!(mp3_duration > gst::ClockTime::from_mseconds(800));
    assert!(mp3_duration < gst::ClockTime::from_seconds(2));

    // 3. Cancelamento limpo de job na fila (atrás do primeiro job curto)
    let blocker_job = ExportJobId::new();
    let mut blocker_settings = ExportSettings::from_preset(ExportPreset::AudioOnly);
    blocker_settings.output_dir = dir.0.clone();
    blocker_settings.file_name = "blocker".to_string();
    blocker_settings.format = ExportFormat::AudioWav;

    let cancelled_job = ExportJobId::new();
    let mut cancelled_settings = ExportSettings::from_preset(ExportPreset::AudioOnly);
    cancelled_settings.output_dir = dir.0.clone();
    cancelled_settings.file_name = "cancelado".to_string();

    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: blocker_job,
            settings: blocker_settings,
            clips: vec![audio_clip.clone()],
            audio_tracks: Vec::new(),
        })
        .unwrap();
    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: cancelled_job,
            settings: cancelled_settings,
            clips: vec![audio_clip],
            audio_tracks: Vec::new(),
        })
        .unwrap();
    handle.send(EngineCommand::CancelExport { job_id: cancelled_job }).unwrap();

    let mut saw_cancelled = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && !saw_cancelled {
        if let Ok(EngineEvent::ExportCancelled { job_id }) =
            handle.events().recv_timeout(Duration::from_millis(100))
        {
            if job_id == cancelled_job {
                saw_cancelled = true;
            }
        }
    }
    assert!(saw_cancelled, "deve emitir ExportCancelled para o job cancelado");

    // 4. Timeline vazia deve emitir ExportFailed sem derrubar o motor
    let bad_job = ExportJobId::new();
    let mut bad_settings = ExportSettings::from_preset(ExportPreset::AudioOnly);
    bad_settings.output_dir = dir.0.clone();
    handle
        .send(EngineCommand::StartExport {
            transitions: Vec::new(),
            job_id: bad_job,
            settings: bad_settings,
            clips: Vec::new(),
            audio_tracks: Vec::new(),
        })
        .unwrap();

    let mut saw_failed = false;
    let err_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < err_deadline && !saw_failed {
        if let Ok(EngineEvent::ExportFailed { job_id, .. }) = handle.events().recv_timeout(Duration::from_millis(50)) {
            if job_id == bad_job {
                saw_failed = true;
            }
        }
    }
    assert!(saw_failed, "timeline vazia deve emitir ExportFailed");

    handle.send(EngineCommand::Shutdown).unwrap();
}

fn discover_rendered_media(path: &Path) -> gst_pbutils::DiscovererInfo {
    let uri = glib::filename_to_uri(path, None).expect("URI do arquivo renderizado");
    gst_pbutils::Discoverer::new(gst::ClockTime::from_seconds(5))
        .expect("descobridor para mídia renderizada")
        .discover_uri(&uri)
        .expect("metadados da mídia renderizada")
}

fn wait_export_finished(
    handle: &meraki_engine::EngineHandle,
    target_job: ExportJobId,
    phase: &str,
) -> (PathBuf, u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_progress = None;
    while Instant::now() < deadline {
        if let Ok(event) = handle.events().recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::ExportFinished { job_id, output_path, file_size_bytes } if job_id == target_job => {
                    return (output_path, file_size_bytes);
                }
                EngineEvent::ExportFailed { job_id, error } if job_id == target_job => {
                    panic!("falha inesperada em {phase}: {error}");
                }
                EngineEvent::ExportCancelled { job_id } if job_id == target_job => {
                    panic!("cancelamento inesperado em {phase}");
                }
                EngineEvent::ExportProgress { job_id, progress, .. } if job_id == target_job => {
                    last_progress = Some(progress);
                }
                EngineEvent::Error(error) => panic!("erro do motor durante {phase}: {error}"),
                _ => {}
            }
        }
    }
    panic!("timeout aguardando ExportFinished em {phase} ({target_job}); último progresso: {last_progress:?}");
}
