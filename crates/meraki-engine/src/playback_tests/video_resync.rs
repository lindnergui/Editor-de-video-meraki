use super::{media_render_clip, wait_for_frame};
use crate::discoverer::tests::TempFileGuard;
use crate::{spawn_engine, EngineCommand, EngineEvent, EngineHandle, TimelineRenderClip};
use gstreamer as gst;
use gstreamer::prelude::*;
use meraki_core::{ClipEffect, PreviewQuality, TrackKind, VideoEffectKind};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn mp4_preview_survives_effect_resync_and_scrubbing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let mp4 = TempFileGuard::new("preview_resync", "mp4");
    write_test_mp4(&mp4.0);

    let handle = spawn_engine();
    assert!(matches!(
        handle
            .events()
            .recv_timeout(Duration::from_secs(30))
            .unwrap(),
        EngineEvent::EngineReady
    ));

    let mut clip = media_render_clip(mp4.0.clone(), TrackKind::Video, 3_000_000_000);
    sync_clip(&handle, &clip);
    wait_for_frame(&handle, "preroll do MP4 com áudio");

    handle.request_seek(500_000_000);
    wait_for_position(&handle, 500_000_000);

    clip.effects = vec![ClipEffect::new(VideoEffectKind::Cinematic, 0.7)];
    sync_clip(&handle, &clip);
    wait_for_position(&handle, 500_000_000);

    for index in 0..8 {
        clip.effects = vec![ClipEffect::new(VideoEffectKind::Cinematic, 0.4)];
        sync_clip(&handle, &clip);
        handle.request_seek(800_000_000);

        clip.effects = if index % 2 == 0 {
            Vec::new()
        } else {
            vec![ClipEffect::new(VideoEffectKind::FilmStyle, 0.6)]
        };
        sync_clip(&handle, &clip);
        let latest = 1_000_000_000 + index * 100_000_000;
        for offset in [200_000_000, 100_000_000, 0] {
            handle.request_seek(latest - offset);
        }
        wait_for_position(&handle, latest);
    }

    handle.send(EngineCommand::Shutdown).unwrap();
}

fn sync_clip(handle: &EngineHandle, clip: &TimelineRenderClip) {
    handle
        .send(EngineCommand::SyncTimeline {
            clips: vec![clip.clone()],
            audio_mix: Vec::new(),
            preview_quality: PreviewQuality::Proxy720p,
        })
        .unwrap();
}

fn wait_for_position(handle: &EngineHandle, position_ns: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match handle.events().recv_timeout(Duration::from_millis(100)) {
            Ok(EngineEvent::FrameReady(frame)) if frame.pts_ns == position_ns => return,
            Ok(EngineEvent::Error(err)) => {
                panic!("erro no preview do MP4 em {position_ns}ns: {err}")
            }
            _ => {}
        }
    }
    panic!("nenhum quadro do MP4 em {position_ns}ns");
}

fn write_test_mp4(path: &Path) {
    crate::ensure_media_initialized().unwrap();
    let h264 = if gst::ElementFactory::find("x264enc").is_some() {
        "x264enc key-int-max=1 tune=zerolatency"
    } else {
        "openh264enc gop-size=1"
    };
    let aac = ["voaacenc", "avenc_aac", "mfaacenc", "mf_aacenc", "faac"]
        .into_iter()
        .find(|name| gst::ElementFactory::find(name).is_some())
        .expect("plugin AAC para gerar o MP4 de teste");
    let desc = format!(
        "mp4mux name=mux ! filesink name=output \
         videotestsrc num-buffers=30 ! video/x-raw,format=I420,width=320,height=180,framerate=10/1 \
         ! {h264} ! h264parse ! queue ! mux. \
         audiotestsrc num-buffers=150 samplesperbuffer=960 ! audio/x-raw,rate=48000 \
         ! audioconvert ! {aac} ! aacparse ! queue ! mux."
    );
    let pipeline = gst::parse::launch(&desc)
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
    pipeline
        .by_name("output")
        .unwrap()
        .set_property("location", path.to_str().unwrap());
    pipeline.set_state(gst::State::Playing).unwrap();
    let message = pipeline.bus().unwrap().timed_pop_filtered(
        gst::ClockTime::from_seconds(10),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(
        matches!(
            message.as_ref().map(|msg| msg.view()),
            Some(gst::MessageView::Eos(_))
        ),
        "falha ao gerar MP4 de teste: {message:?}"
    );
}
