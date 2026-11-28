use meraki_core::{AssetId, ClipTransform, TrackId, TransitionKind};
use meraki_engine::{EngineCommand, EngineEvent, ProxyMode};
use std::fs;

#[path = "v04/support.rs"]
mod support;
use support::*;

#[test]
fn proxy_modes_reuse_the_cache_clear_it_and_preserve_original_bytes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixtures::new();
    assert!(std::env::temp_dir().starts_with(&fixture.dir));
    let source = fixture.video("source", "smpte");
    let original = fs::read(&source).unwrap();
    let engine = start_engine();
    ensure(&engine, &source); // AutoAbove1080p preserva este vídeo de 320 × 180.
    engine
        .send(EngineCommand::SetProxyMode(ProxyMode::Off))
        .unwrap();
    ensure(&engine, &source);
    engine.send(EngineCommand::ClearProxyCache).unwrap();
    wait(&engine, "cache vazio", |event| match event {
        EngineEvent::ProxyGenerating { .. } => panic!("modo de proxy incorreto"),
        EngineEvent::ProxyCacheCleared { freed_bytes: 0 } => Some(()),
        _ => None,
    });

    engine
        .send(EngineCommand::SetProxyMode(ProxyMode::Always))
        .unwrap();
    let id = ensure(&engine, &source);
    let mut generated = false;
    let proxy = wait(&engine, "primeiro proxy", |event| match event {
        EngineEvent::ProxyGenerating { asset_id } if asset_id == id => {
            generated = true;
            None
        }
        EngineEvent::ProxyReady {
            asset_id,
            proxy_path,
        } if asset_id == id => Some(proxy_path),
        _ => None,
    });
    assert!(generated);
    assert!(proxy
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("_720p.mp4"));
    let size = fs::metadata(&proxy).unwrap().len();
    let reused = ensure(&engine, &source);
    let cached = wait(&engine, "reuso de proxy", |event| match event {
        EngineEvent::ProxyGenerating { asset_id } if asset_id == reused => {
            panic!("cache recompresso")
        }
        EngineEvent::ProxyReady {
            asset_id,
            proxy_path,
        } if asset_id == reused => Some(proxy_path),
        _ => None,
    });
    assert_eq!(cached, proxy);
    let proxy_probe = AssetId::new();
    engine
        .send(EngineCommand::ImportMedia {
            asset_id: proxy_probe,
            path: proxy.clone(),
            generate_proxy: false,
        })
        .unwrap();
    let dimensions = wait(&engine, "proxy decodificável", |event| match event {
        EngineEvent::MediaDiscovered(meta) if meta.asset_id == proxy_probe => {
            meta.width.zip(meta.height)
        }
        _ => None,
    });
    assert_eq!(dimensions, (320, 180));

    let mut preview = render_clip(&source, TrackId::new(), 0);
    preview.proxy_path = Some(cached);
    sync(&engine, vec![preview], Vec::new());
    first_frame(&engine);
    engine.send(EngineCommand::ClearProxyCache).unwrap();
    let freed = wait(&engine, "limpeza do preview ativo", |event| match event {
        EngineEvent::ProxyCacheCleared { freed_bytes } => Some(freed_bytes),
        _ => None,
    });
    assert_eq!(freed, size);
    assert!(!proxy.exists());
    assert_eq!(fs::read(&source).unwrap(), original);
    let cancelled = ensure(&engine, &source);
    wait(&engine, "nova geração após limpeza", |event| match event {
        EngineEvent::ProxyGenerating { asset_id } if asset_id == cancelled => Some(()),
        _ => None,
    });
    shutdown(engine);
    assert!(fs::read_dir(proxy.parent().unwrap()).unwrap().all(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_none_or(|ext| ext != "part")
    }));
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn blur_and_zoom_peak_on_both_sides_of_the_cut_and_leave_the_middle_unchanged() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixtures::new();
    let path = fixture.video("checkerboard", "checkers-8");
    let track = TrackId::new();
    let a = render_clip(&path, track, 0);
    let mut b = render_clip(&path, track, 2 * SECOND);
    b.source_in_ns = 100_000_000;
    let engine = start_engine();
    let timing = [SECOND, 1_900_000_000, 2_100_000_000, 3 * SECOND];
    sync(&engine, vec![a.clone(), b.clone()], Vec::new());
    first_frame(&engine);
    let baseline = timing.map(|time| seek(&engine, time));
    let delta = |left: &meraki_engine::VideoFrame, right: &meraki_engine::VideoFrame| {
        left.rgba_pixels
            .iter()
            .zip(&right.rgba_pixels)
            .step_by(28)
            .map(|(&a, &b)| u64::from(a.abs_diff(b)))
            .sum::<u64>()
    };
    for kind in [TransitionKind::Zoom, TransitionKind::Blur] {
        let mut transitions = placement(&a, &b, kind);
        transitions.remove(2);
        transitions.remove(0);
        sync(&engine, vec![a.clone(), b.clone()], transitions);
        first_frame(&engine);
        let actual = timing.map(|time| seek(&engine, time));
        let differences = std::array::from_fn::<_, 4, _>(|i| delta(&baseline[i], &actual[i]));
        assert!(
            differences[1] > 150_000 && differences[2] > 150_000,
            "{kind:?}: {differences:?}"
        );
        assert!(
            differences[0] < differences[1] / 10 && differences[3] < differences[2] / 10,
            "{kind:?}: {differences:?}"
        );
    }
    shutdown(engine);
}

fn assert_red(pixel: [u8; 3]) {
    assert!(
        pixel[0] > 175 && pixel[1] < 50 && pixel[2] < 50,
        "{pixel:?}"
    );
}
fn assert_blue(pixel: [u8; 3]) {
    assert!(
        pixel[2] > 175 && pixel[0] < 50 && pixel[1] < 50,
        "{pixel:?}"
    );
}
fn assert_mix(pixel: [u8; 3]) {
    assert!(pixel[0] > 35 && pixel[2] > 35 && pixel[1] < 60, "{pixel:?}");
}
fn assert_dark(pixel: [u8; 3]) {
    assert!(pixel.into_iter().all(|channel| channel < 65), "{pixel:?}");
}

#[test]
fn preview_and_export_crossfade_at_the_cut_and_keep_edge_fades_local() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixtures::new();
    let (red, blue, green) = (
        fixture.video("red", "red"),
        fixture.video("blue", "blue"),
        fixture.video("green", "green"),
    );
    let track = TrackId::new();
    let mut a = render_clip(&red, track, 0);
    let b = render_clip(&blue, track, 2 * SECOND);
    a.proxy_path = Some(green);
    let transitions = placement(&a, &b, TransitionKind::Crossfade);
    let engine = start_engine();
    sync(&engine, vec![a.clone(), b.clone()], transitions.clone());
    first_frame(&engine);
    let green = rgb(&seek(&engine, SECOND));
    assert!(
        green[1] > 175 && green[0] < 50 && green[2] < 50,
        "{green:?}"
    );

    let file = export(
        &engine,
        &fixture.dir,
        vec![a.clone(), b.clone()],
        transitions.clone(),
    );
    let decoded = read_export(&file);
    assert!(
        (120..=121).contains(&decoded.len()),
        "a transição não muda a duração da timeline"
    );
    assert_dark(decoded[0]);
    assert_red(decoded[30]);
    assert_mix(decoded[54]);
    assert_mix(decoded[66]);
    assert_blue(decoded[90]);
    assert_dark(decoded[118]);

    engine
        .send(EngineCommand::SetProxyMode(ProxyMode::Off))
        .unwrap();
    first_frame(&engine);
    assert_dark(rgb(&seek(&engine, 0)));
    assert_red(rgb(&seek(&engine, SECOND)));
    assert_mix(rgb(&seek(&engine, 1_800_000_000)));
    assert_mix(rgb(&seek(&engine, 2_200_000_000)));
    assert_blue(rgb(&seek(&engine, 3 * SECOND)));
    assert_dark(rgb(&seek(&engine, 3_933_333_333)));

    a.source_in_ns = 0;
    sync(&engine, vec![a.clone(), b], transitions);
    first_frame(&engine);
    engine
        .send(EngineCommand::UpdateClipLive {
            clip_id: a.clip_id,
            transform: Some(ClipTransform {
                opacity: 0.8,
                ..ClipTransform::default()
            }),
            title: None,
            effects: None,
        })
        .unwrap();
    assert_red(rgb(&seek(&engine, SECOND)));
    shutdown(engine);
}
