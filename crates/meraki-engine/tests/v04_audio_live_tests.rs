use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::{
    ClipId, ClipTransform, ExportJobId, ExportPreset, ExportSettings, PreviewQuality,
    TrackAudioSettings, TrackId, TrackKind,
};
use meraki_engine::{
    spawn_engine, EngineCommand, EngineEvent, EngineHandle, TimelineRenderClip, TrackAudioMix,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SECOND: u64 = 1_000_000_000;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("meraki-live-audio-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn tone(&self) -> PathBuf {
        let file = self.0.join("tone.wav");
        let frames = 480_000_u32;
        let size = frames * 4;
        let mut wav = Vec::with_capacity(size as usize + 44);
        wav.extend(b"RIFF");
        wav.extend((36 + size).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(48_000_u32.to_le_bytes());
        wav.extend(192_000_u32.to_le_bytes());
        wav.extend(4_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(size.to_le_bytes());
        for frame in 0..frames {
            let phase = f64::from(frame) * std::f64::consts::TAU * 320.0 / 48_000.0;
            let sample = (phase.sin() * 3276.0) as i16;
            wav.extend(sample.to_le_bytes());
            wav.extend(sample.to_le_bytes());
        }
        fs::write(&file, wav).unwrap();
        file
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wait<T>(
    engine: &EngineHandle,
    label: &str,
    mut take: impl FnMut(EngineEvent) -> Option<T>,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let event = engine
            .events()
            .recv_deadline(deadline)
            .unwrap_or_else(|err| panic!("{label}: {err}"));
        if let EngineEvent::Error(error) | EngineEvent::ExportFailed { error, .. } = &event {
            panic!("{label}: {error}");
        }
        if let Some(value) = take(event) {
            return value;
        }
    }
}

fn measured(
    engine: &EngineHandle,
    track: TrackId,
    label: &str,
    mut matches: impl FnMut(f32, f32) -> bool,
) -> (f32, f32) {
    let started = Instant::now();
    let mut last = None;
    let mut consecutive = 0;
    wait(engine, label, |event| match event {
        EngineEvent::AudioLevels(frame) => {
            let track = frame.tracks.iter().find(|level| level.track_id == track)?;
            let values = (frame.master_rms_db, track.rms_db);
            last = Some(values);
            if matches(values.0, values.1) {
                consecutive += 1;
                if consecutive == 1 {
                    eprintln!(
                        "{label}: primeira medição compatível após {:?}",
                        started.elapsed()
                    );
                }
            } else {
                consecutive = 0;
            }
            (consecutive >= 2).then_some(values)
        }
        EngineEvent::PlayheadTick(0) => panic!("{label}: seek inesperado, última amostra {last:?}"),
        _ => None,
    })
}

fn clip(path: PathBuf, track: TrackId) -> TimelineRenderClip {
    TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: track,
        path,
        proxy_path: None,
        track_kind: TrackKind::Audio,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 10 * SECOND,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    }
}

fn update(
    engine: &EngineHandle,
    track: Option<(TrackId, f64, bool)>,
    master: Option<f64>,
    mute: Option<bool>,
    eq: Option<(TrackId, [f64; 4])>,
) {
    engine
        .send(EngineCommand::UpdateAudioLive {
            track_volumes: track,
            master_volume_db: master,
            master_muted: mute,
            track_eq: eq,
            track_enhancements: None,
        })
        .unwrap();
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.5
}

#[test]
fn live_gains_and_parametric_eq_change_real_samples_and_export_matches_preview() {
    std::env::set_var("MERAKI_HEADLESS_AUDIO", "1");
    let fixture = Fixture::new();
    let track = TrackId::new();
    let clip = clip(fixture.tone(), track);
    let engine = spawn_engine();
    wait(&engine, "EngineReady", |event| {
        matches!(event, EngineEvent::EngineReady).then_some(())
    });
    engine
        .send(EngineCommand::SyncTimeline {
            clips: vec![clip.clone()],
            transitions: Vec::new(),
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
    engine.send(EngineCommand::Play).unwrap();
    let (baseline, _) = measured(&engine, track, "tom plano", |master, track| {
        close(master, -23.0) && close(track, -23.0)
    });

    let spectrum = wait(&engine, "espectro de 320 Hz", |event| match event {
        EngineEvent::AudioSpectrum(bands) => Some(bands),
        _ => None,
    });
    let [low, low_mid, mid, high] = spectrum.bands_db;
    assert!(
        low_mid > low + 6.0 && low_mid > mid + 12.0 && low_mid > high + 12.0,
        "{spectrum:?}"
    );

    update(&engine, None, Some(-6.0), None, None);
    measured(&engine, track, "fader Master -6 dB", |master, track| {
        close(master, baseline - 6.0) && close(track, baseline)
    });
    update(&engine, Some((track, -8.0, false)), None, None, None);
    measured(&engine, track, "fader de trilha -8 dB", |master, track| {
        close(master, baseline - 14.0) && close(track, baseline - 8.0)
    });

    let gains = [-2.0, 10.0, 2.0, -3.0];
    for i in 0..20 {
        update(
            &engine,
            None,
            None,
            None,
            Some((track, [-2.0, f64::from(i) * 0.5, 2.0, -3.0])),
        );
    }
    update(&engine, None, None, None, Some((track, gains)));
    let (preview_db, _) = measured(
        &engine,
        track,
        "pico paramétrico de 320 Hz",
        |master, track| close(master, baseline - 4.0) && close(track, baseline + 2.0),
    );

    update(&engine, None, None, Some(true), None);
    measured(&engine, track, "mute Master", |master, track| {
        master <= -59.0 && track > -40.0
    });
    update(&engine, None, Some(-6.0), None, None);
    measured(
        &engine,
        track,
        "volume preserva mute Master",
        |master, track| master <= -59.0 && track > -40.0,
    );
    update(&engine, None, None, Some(false), None);
    measured(
        &engine,
        track,
        "restaura ganho Master após mute",
        |master, _| close(master, preview_db),
    );

    engine.send(EngineCommand::Pause).unwrap();
    wait(&engine, "Pause", |event| {
        matches!(event, EngineEvent::ShuttleRateChanged { shuttle_rate: 0.0 }).then_some(())
    });
    engine.send(EngineCommand::RequestAudioSpectrum).unwrap();
    wait(&engine, "espectro recente sob demanda", |event| {
        matches!(event, EngineEvent::AudioSpectrum(_)).then_some(())
    });

    let job = ExportJobId::new();
    engine
        .send(EngineCommand::StartExport {
            job_id: job,
            clips: vec![clip],
            transitions: Vec::new(),
            settings: ExportSettings {
                output_dir: fixture.0.clone(),
                ..ExportSettings::from_preset(ExportPreset::AudioOnly)
            },
            audio_tracks: vec![TrackAudioMix {
                track_id: track,
                settings: TrackAudioSettings {
                    gain_db: -8.0,
                    eq_gains_db: gains.map(|v| v as f32),
                    ..Default::default()
                },
            }],
        })
        .unwrap();
    let output = wait(&engine, "export WAV", |event| match event {
        EngineEvent::ExportFinished {
            job_id,
            output_path,
            ..
        } if job_id == job => Some(output_path),
        _ => None,
    });
    let export_db = exported_rms_db(&output);
    assert!(
        (preview_db - export_db).abs() < 0.75,
        "preview {preview_db:.2} dB, WAV {export_db:.2} dB"
    );
    engine.send(EngineCommand::Shutdown).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match engine.events().recv_deadline(deadline) {
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(err) => panic!("Shutdown: {err}"),
        }
    }
}

fn exported_rms_db(path: &Path) -> f32 {
    let pipeline = gst::parse::launch(
        "filesrc name=source ! wavparse ! audioconvert ! audio/x-raw,format=F32LE ! appsink name=audio sync=false",
    ).unwrap().downcast::<gst::Pipeline>().unwrap();
    pipeline
        .by_name("source")
        .unwrap()
        .set_property("location", path.to_str().unwrap());
    let sink = pipeline
        .by_name("audio")
        .unwrap()
        .downcast::<gstreamer_app::AppSink>()
        .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut energy = 0.0_f64;
    let mut count = 0_u64;
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(3)) {
        let buffer = sample.buffer().unwrap();
        if buffer.pts().is_some_and(|pts| pts.nseconds() < SECOND) {
            continue;
        }
        let map = buffer.map_readable().unwrap();
        for &bytes in map.as_slice().as_chunks::<4>().0 {
            energy += f64::from(f32::from_le_bytes(bytes)).powi(2);
            count += 1;
        }
    }
    let eos = sink.is_eos();
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(eos && count > 0);
    (10.0 * (energy / count as f64).log10()) as f32
}
