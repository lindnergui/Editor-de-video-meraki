use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::{
    AssetId, ClipId, ClipTransform, ExportJobId, ExportResolution, ExportSettings, PreviewQuality,
    TrackId, TrackKind, TransitionKind, TransitionPlacement,
};
use meraki_engine::{
    spawn_engine, EngineCommand, EngineEvent, EngineHandle, TimelineRenderClip, TimelineTransition,
    VideoFrame,
};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};
use uuid::Uuid;

static GST: Once = Once::new();
pub(super) static SERIAL: Mutex<()> = Mutex::new(());
pub(super) const SECOND: u64 = 1_000_000_000;

pub(super) struct Fixtures {
    pub(super) dir: PathBuf,
    env: Vec<(&'static str, Option<OsString>)>,
}

impl Fixtures {
    pub(super) fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("meraki-v04-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let env = ["LOCALAPPDATA", "XDG_CACHE_HOME", "TMPDIR", "TMP", "TEMP"]
            .into_iter()
            .map(|key| {
                let previous = std::env::var_os(key);
                std::env::set_var(key, &dir);
                (key, previous)
            })
            .collect();
        GST.call_once(|| {
            gst::init().unwrap();
            let _ = tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_test_writer()
                .try_init();
        });
        Self { dir, env }
    }

    pub(super) fn video(&self, name: &str, pattern: &str) -> PathBuf {
        let output = self.dir.join(format!("{name}.mp4"));
        let encoder = if gst::ElementFactory::find("x264enc").is_some() {
            "x264enc key-int-max=1 speed-preset=ultrafast tune=zerolatency"
        } else {
            "openh264enc gop-size=1"
        };
        let pipeline = gst::parse::launch(&format!(
            "videotestsrc num-buffers=105 pattern={pattern} ! \
             video/x-raw,format=I420,width=320,height=180,framerate=30/1 ! \
             {encoder} ! h264parse ! mp4mux ! filesink name=file"
        ))
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        pipeline
            .by_name("file")
            .unwrap()
            .set_property("location", output.to_str().unwrap());
        pipeline.set_state(gst::State::Playing).unwrap();
        let message = pipeline.bus().unwrap().timed_pop_filtered(
            gst::ClockTime::from_seconds(15),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(
            matches!(
                message.as_ref().map(|message| message.view()),
                Some(gst::MessageView::Eos(_))
            ),
            "{message:?}"
        );
        output
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        for (key, previous) in &self.env {
            if let Some(previous) = previous {
                std::env::set_var(key, previous);
            } else {
                std::env::remove_var(key);
            }
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn wait<T>(
    engine: &EngineHandle,
    label: &str,
    mut filter: impl FnMut(EngineEvent) -> Option<T>,
) -> T {
    let until = Instant::now() + Duration::from_secs(35);
    loop {
        let event = engine
            .events()
            .recv_deadline(until)
            .unwrap_or_else(|err| panic!("{label}: {err}"));
        if let EngineEvent::Error(reason)
        | EngineEvent::ProxyFailed { reason, .. }
        | EngineEvent::ExportFailed { error: reason, .. } = &event
        {
            panic!("{label}: {reason}");
        }
        if let Some(result) = filter(event) {
            return result;
        }
    }
}

pub(super) fn start_engine() -> EngineHandle {
    let engine = spawn_engine();
    wait(&engine, "EngineReady", |event| {
        matches!(event, EngineEvent::EngineReady).then_some(())
    });
    engine
}

pub(super) fn shutdown(engine: EngineHandle) {
    engine.send(EngineCommand::Shutdown).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        match engine.events().recv_deadline(until) {
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(err) => panic!("Shutdown: {err}"),
        }
    }
}

pub(super) fn ensure(engine: &EngineHandle, source: &Path) -> AssetId {
    let asset_id = AssetId::new();
    engine
        .send(EngineCommand::EnsureProxy {
            asset_id,
            source_path: source.to_path_buf(),
            width: 320,
            height: 180,
        })
        .unwrap();
    asset_id
}

pub(super) fn render_clip(path: &Path, track_id: TrackId, start: u64) -> TimelineRenderClip {
    TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id,
        path: path.to_path_buf(),
        proxy_path: None,
        track_kind: TrackKind::Video,
        timeline_start_ns: start,
        source_in_ns: SECOND / 2,
        duration_ns: 2 * SECOND,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    }
}

pub(super) fn sync(
    engine: &EngineHandle,
    clips: Vec<TimelineRenderClip>,
    transitions: Vec<TimelineTransition>,
) {
    engine
        .send(EngineCommand::SyncTimeline {
            clips,
            transitions,
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
}

pub(super) fn first_frame(engine: &EngineHandle) -> VideoFrame {
    wait(engine, "preroll", |event| match event {
        EngineEvent::FrameReady(frame) => Some(frame),
        _ => None,
    })
}

pub(super) fn seek(engine: &EngineHandle, time: u64) -> VideoFrame {
    engine.request_seek(time);
    wait(engine, &format!("quadro em {time}"), |event| match event {
        EngineEvent::FrameReady(frame) if frame.pts_ns.abs_diff(time) < 2_000_000 => Some(frame),
        _ => None,
    })
}

fn rgb_at(pixels: &[u8], width: usize, height: usize) -> [u8; 3] {
    let i = (height / 2 * width + width / 2) * 4;
    pixels[i..i + 3].try_into().unwrap()
}

pub(super) fn rgb(frame: &VideoFrame) -> [u8; 3] {
    rgb_at(
        &frame.rgba_pixels,
        frame.width as usize,
        frame.height as usize,
    )
}

pub(super) fn placement(
    a: &TimelineRenderClip,
    b: &TimelineRenderClip,
    kind: TransitionKind,
) -> Vec<TimelineTransition> {
    vec![
        TimelineTransition::new(
            a.track_id,
            TransitionKind::Crossfade,
            TransitionPlacement::ClipStart {
                clip_id: a.clip_id.0,
            },
            600_000_000,
        ),
        TimelineTransition::new(
            a.track_id,
            kind,
            TransitionPlacement::BetweenClips {
                left_clip_id: a.clip_id.0,
                right_clip_id: b.clip_id.0,
            },
            800_000_000,
        ),
        TimelineTransition::new(
            b.track_id,
            TransitionKind::Crossfade,
            TransitionPlacement::ClipEnd {
                clip_id: b.clip_id.0,
            },
            600_000_000,
        ),
    ]
}

pub(super) fn export(
    engine: &EngineHandle,
    root: &Path,
    clips: Vec<TimelineRenderClip>,
    transitions: Vec<TimelineTransition>,
) -> PathBuf {
    let job_id = ExportJobId::new();
    engine
        .send(EngineCommand::StartExport {
            job_id,
            clips,
            transitions,
            audio_tracks: Vec::new(),
            settings: ExportSettings {
                output_dir: root.to_path_buf(),
                resolution: ExportResolution::Sd720p,
                ..Default::default()
            },
        })
        .unwrap();
    wait(engine, "ExportFinished", |event| match event {
        EngineEvent::ExportFinished {
            job_id: id,
            output_path,
            ..
        } if id == job_id => Some(output_path),
        _ => None,
    })
}

pub(super) fn read_export(path: &Path) -> Vec<[u8; 3]> {
    let pipeline = gst::parse::launch(
        "uridecodebin name=reader ! videoconvert ! videoscale ! video/x-raw,format=RGBA,width=64,height=36 ! appsink name=sink sync=false",
    ).unwrap().downcast::<gst::Pipeline>().unwrap();
    pipeline
        .by_name("reader")
        .unwrap()
        .set_property("uri", glib::filename_to_uri(path, None).unwrap().as_str());
    let sink = pipeline
        .by_name("sink")
        .unwrap()
        .downcast::<gstreamer_app::AppSink>()
        .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut frames = Vec::new();
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let pixels = sample.buffer().unwrap().map_readable().unwrap();
        frames.push(rgb_at(pixels.as_slice(), 64, 36));
    }
    let eos = sink.is_eos();
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(eos, "a leitura da exportação deve terminar com EOS");
    frames
}
