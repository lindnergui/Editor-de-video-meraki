use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::{
    ClipEffect, ClipId, ClipTransform, EffectParameters, FilmParams, PreviewQuality, TitleOverlay,
    TrackId, TrackKind, VideoEffectKind, ZoomEasing, ZoomParams,
};
use meraki_engine::{
    spawn_engine, EngineCommand, EngineEvent, EngineHandle, TimelineRenderClip, VideoFrame,
};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};
use uuid::Uuid;

static GST_INIT: Once = Once::new();
static ENGINE_TEST: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    let guard = ENGINE_TEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    GST_INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();
        gst::init().unwrap();
    });
    guard
}

struct Fixture(PathBuf);

impl Fixture {
    fn make(extension: &str, desc: &str) -> Self {
        let fixture =
            Self(std::env::temp_dir().join(format!("meraki-v03-{}.{}", Uuid::new_v4(), extension)));
        let pipeline = gst::parse::launch(desc)
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        pipeline
            .by_name("out")
            .unwrap()
            .set_property("location", fixture.0.to_string_lossy().as_ref());
        pipeline.set_state(gst::State::Playing).unwrap();
        let message = pipeline.bus().unwrap().timed_pop_filtered(
            gst::ClockTime::from_seconds(10),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(
            message
                .as_ref()
                .is_some_and(|msg| matches!(msg.view(), gst::MessageView::Eos(_))),
            "{message:?}"
        );
        fixture
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn clip(path: &Path, kind: TrackKind) -> TimelineRenderClip {
    TimelineRenderClip {
        clip_id: ClipId::new(),
        track_id: TrackId::new(),
        path: path.to_path_buf(),
        proxy_path: None,
        track_kind: kind,
        timeline_start_ns: 0,
        source_in_ns: 0,
        duration_ns: 3_000_000_000,
        transform: ClipTransform::default(),
        title: None,
        effects: Vec::new(),
        transition_in: None,
        transition_out: None,
        keyframes: Vec::new(),
        volume_keyframes: Vec::new(),
    }
}

fn wait_event<T>(
    engine: &EngineHandle,
    label: &str,
    mut take: impl FnMut(EngineEvent) -> Option<T>,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let event = engine
            .events()
            .recv_deadline(deadline)
            .unwrap_or_else(|error| panic!("{label}: {error}"));
        if let EngineEvent::Error(error) = event {
            panic!("{label}: {error}");
        }
        if let Some(value) = take(event) {
            return value;
        }
    }
}

fn frame(engine: &EngineHandle, label: &str) -> VideoFrame {
    wait_event(engine, label, |event| match event {
        EngineEvent::FrameReady(frame) => Some(frame),
        _ => None,
    })
}

fn start(clips: Vec<TimelineRenderClip>) -> EngineHandle {
    let engine = spawn_engine();
    wait_event(&engine, "motor pronto", |event| {
        matches!(event, EngineEvent::EngineReady).then_some(())
    });
    engine
        .send(EngineCommand::SyncTimeline {
            transitions: Vec::new(),
            clips,
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
    frame(&engine, "preroll inicial");
    engine
}

fn seek_to(engine: &EngineHandle, position: u64) -> VideoFrame {
    engine.request_seek(position);
    wait_event(engine, "quadro após seek", |event| match event {
        EngineEvent::FrameReady(frame) if frame.pts_ns == position => Some(frame),
        _ => None,
    })
}

fn shutdown(engine: EngineHandle) {
    engine.send(EngineCommand::Shutdown).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match engine.events().recv_deadline(deadline) {
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            Err(error) => panic!("encerramento do motor: {error}"),
        }
    }
}

#[test]
fn paused_live_edits_refresh_current_frame_and_coalesce_partial_patches() {
    let _serial = setup();
    let png = Fixture::make("png",
        "videotestsrc num-buffers=1 pattern=smpte ! video/x-raw,width=320,height=180 ! pngenc ! filesink name=out");
    let mut media = clip(&png.0, TrackKind::Video);
    let mut film = ClipEffect::with_defaults(VideoEffectKind::FilmStyle);
    let mut zoom = ClipEffect::with_params(EffectParameters::Zoom(ZoomParams {
        start_scale: 1.0,
        end_scale: 1.0,
        easing: ZoomEasing::Constant,
        ..ZoomParams::default()
    }));
    media.effects = vec![film.clone(), zoom.clone()];
    let id = media.clip_id;
    let engine = start(vec![media]);
    let baseline = seek_to(&engine, 600_000_000);
    film.params = EffectParameters::Film(FilmParams {
        intensity: 1.0,
        saturation: 0.0,
        ..FilmParams::default()
    });
    zoom.params = EffectParameters::Zoom(ZoomParams {
        start_scale: 1.2,
        end_scale: 2.0,
        ..ZoomParams::default()
    });
    engine
        .send(EngineCommand::UpdateClipLive {
            clip_id: id,
            transform: None,
            title: None,
            effects: Some(vec![film, zoom]),
        })
        .unwrap();
    for pos_x in 0..15 {
        engine
            .send(EngineCommand::UpdateClipLive {
                clip_id: id,
                transform: Some(ClipTransform {
                    pos_x: pos_x as f32 * 10.0,
                    scale: 1.4,
                    ..ClipTransform::default()
                }),
                title: None,
                effects: None,
            })
            .unwrap();
    }
    let edited = frame(&engine, "refresh da edição atual");
    assert_eq!(
        edited.pts_ns, baseline.pts_ns,
        "o pipeline não deve reprerrolar do zero"
    );
    assert_ne!(edited.rgba_pixels, baseline.rgba_pixels);
    shutdown(engine);
}

#[test]
fn title_live_edit_refreshes_rendered_text_bounds() {
    let _serial = setup();
    let mut title = clip(Path::new(""), TrackKind::Overlay);
    let mut spec = TitleOverlay::default();
    title.title = Some(spec.clone());
    let id = title.clip_id;
    let engine = start(vec![title]);
    let bounds = |event| match event {
        EngineEvent::TitleBounds { clip_id, bounds } if clip_id == id => bounds,
        _ => None,
    };
    let initial = wait_event(&engine, "caixa do textoverlay", bounds);
    engine
        .send(EngineCommand::UpdateClipLive {
            clip_id: id,
            transform: Some(ClipTransform {
                pos_y: 140.0,
                ..ClipTransform::default()
            }),
            title: None,
            effects: None,
        })
        .unwrap();
    let moved = wait_event(&engine, "caixa após mover título", |event| {
        bounds(event).filter(|next| *next != initial)
    });
    assert!(moved.y > initial.y, "{initial:?} -> {moved:?}");
    spec.text = "Meraki — título ao vivo".into();
    spec.font_size *= 2;
    engine
        .send(EngineCommand::UpdateClipLive {
            clip_id: id,
            transform: None,
            title: Some(spec),
            effects: None,
        })
        .unwrap();
    let edited = wait_event(&engine, "caixa após edição", |event| {
        bounds(event).filter(|next| *next != moved)
    });
    assert!(edited.width > moved.width && edited.height > moved.height);
    shutdown(engine);
}

#[test]
fn shuttle_mp4_reaches_zero_then_pauses_accelerates_and_resets() {
    let _serial = setup();
    let h264 = if gst::ElementFactory::find("x264enc").is_some() {
        "x264enc key-int-max=12 tune=zerolatency"
    } else {
        "openh264enc gop-size=12"
    };
    let aac = ["voaacenc", "avenc_aac", "mfaacenc", "mf_aacenc"]
        .into_iter()
        .find(|name| gst::ElementFactory::find(name).is_some())
        .expect("encoder AAC");
    let mp4 = Fixture::make(
        "mp4",
        &format!(
            "mp4mux name=mux ! filesink name=out \
         videotestsrc num-buffers=90 ! video/x-raw,format=I420,width=320,height=180,framerate=30/1 \
         ! {h264} ! h264parse ! queue ! mux.video_0 \
         audiotestsrc num-buffers=150 samplesperbuffer=960 ! audio/x-raw,rate=48000,channels=2 \
         ! audioconvert ! {aac} ! aacparse ! queue ! mux.audio_0"
        ),
    );
    let engine = start(vec![clip(&mp4.0, TrackKind::Video)]);
    seek_to(&engine, 400_000_000);
    for (direction, expected) in [(-1, -1.0), (-1, -2.0)] {
        shuttle(&engine, direction, expected);
    }
    let mut saw_zero = false;
    let mut stopped = false;
    wait_event(&engine, "recuo deve parar em zero", |event| {
        match event {
            EngineEvent::PlayheadTick(0) => saw_zero = true,
            EngineEvent::ShuttleRateChanged { shuttle_rate: 0.0 } => stopped = true,
            _ => {}
        }
        (saw_zero && stopped).then_some(())
    });
    for (direction, expected) in [
        (0, 0.0),
        (1, 1.0),
        (1, 2.0),
        (0, 0.0),
        (1, 1.0),
        (1, 2.0),
        (1, 4.0),
        (0, 0.0),
    ] {
        shuttle(&engine, direction, expected);
    }
    shutdown(engine);
}

fn shuttle(engine: &EngineHandle, direction: i8, expected: f64) {
    engine.send(EngineCommand::Shuttle { direction }).unwrap();
    wait_event(
        engine,
        &format!("taxa de shuttle {expected}"),
        |event| match event {
            EngineEvent::ShuttleRateChanged { shuttle_rate } => {
                assert_eq!(shuttle_rate, expected);
                Some(())
            }
            _ => None,
        },
    );
}
