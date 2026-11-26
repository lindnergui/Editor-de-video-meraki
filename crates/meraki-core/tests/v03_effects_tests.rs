use meraki_core::{
    AddAssetCommand, AddClipCommand, AddClipEffectCommand, BlurParams, Clip, ClipEffect, ClipId,
    ClipTransform, EffectKind, EffectParameters, FilmParams, GlitchParams, HistoryError,
    HistoryStack, MediaAsset, MediaKind, Project, RemoveEffectCommand, SetClipTransformCommand,
    TimelineError, ToggleEffectCommand, TransitionEffectKind, TransitionParams,
    UpdateEffectParamsCommand, VideoEffect, ZoomEasing, ZoomParams,
};
use std::path::PathBuf;

fn setup_clip() -> (Project, HistoryStack, ClipId) {
    let mut project = Project::new("v0.3 Efeitos");
    let mut history = HistoryStack::new();
    let v1 = project.timeline.tracks[0].id;
    let asset = MediaAsset::new(
        "Plano A",
        PathBuf::from("plano_a.mp4"),
        MediaKind::Video,
        None,
        8_000_000_000,
    );
    let asset_id = asset.id;
    history
        .execute(Box::new(AddAssetCommand::new(asset)), &mut project)
        .unwrap();
    let clip = Clip::new(asset_id, 0, 0, 5_000_000_000);
    let clip_id = clip.id;
    history
        .execute(Box::new(AddClipCommand::new(v1, clip)), &mut project)
        .unwrap();
    (project, history, clip_id)
}

#[test]
fn effect_defaults_badges_and_reset_values_match_specification() {
    let zoom: VideoEffect = ClipEffect::with_defaults(EffectKind::Zoom);
    assert_eq!(zoom.short_badge_label(), "Zoom");
    assert_eq!(
        zoom.params,
        EffectParameters::Zoom(ZoomParams {
            start_scale: 1.0,
            end_scale: 1.25,
            focus_x: 0.0,
            focus_y: 0.0,
            easing: ZoomEasing::EaseInOut,
        })
    );
    assert_eq!(zoom.default_param_value("start_scale"), Some(1.0));
    assert_eq!(zoom.default_param_value("end_scale"), Some(1.25));
    assert_eq!(zoom.default_param_value("focus_x"), Some(0.0));
    assert_eq!(zoom.default_param_value("focus_y"), Some(0.0));

    let film = ClipEffect::with_defaults(EffectKind::FilmStyle);
    assert_eq!(film.short_badge_label(), "Filme");
    assert_eq!(
        film.params,
        EffectParameters::Film(FilmParams {
            intensity: 0.65,
            contrast: 1.12,
            saturation: 0.85,
            vignette: 0.35,
        })
    );
    assert_eq!(film.default_param_value("vignette"), Some(0.35));

    let glitch = ClipEffect::with_defaults(EffectKind::Glitch);
    assert_eq!(glitch.short_badge_label(), "Glitch");
    assert_eq!(
        glitch.params,
        EffectParameters::Glitch(GlitchParams {
            intensity: 0.6,
            rgb_shift: 0.4,
            frequency: 0.5,
        })
    );
    assert_eq!(glitch.default_param_value("rgb_shift"), Some(0.4));

    let blur = ClipEffect::with_defaults(EffectKind::Blur);
    assert_eq!(blur.short_badge_label(), "Blur");
    assert_eq!(blur.params, EffectParameters::Blur(BlurParams { radius: 0.35 }));
    assert_eq!(blur.default_param_value("radius"), Some(0.35));

    let fade = ClipEffect::with_defaults(EffectKind::Transition);
    assert_eq!(fade.short_badge_label(), "Fade");
    assert_eq!(
        fade.params,
        EffectParameters::Transition(TransitionParams {
            kind: TransitionEffectKind::Fade,
            duration_ms: 500,
            apply_in: true,
            apply_out: true,
        })
    );
    assert_eq!(fade.default_param_value("duration_ms"), Some(500.0));

    let zoom_tr = ClipEffect::with_params(EffectParameters::Transition(TransitionParams {
        kind: TransitionEffectKind::ZoomTransition,
        duration_ms: 750,
        apply_in: true,
        apply_out: false,
    }));
    assert_eq!(zoom_tr.short_badge_label(), "Zoom");

    let cinematic = ClipEffect::with_defaults(EffectKind::Cinematic);
    assert_eq!(cinematic.short_badge_label(), "Cinema");
    assert_eq!(cinematic.default_param_value("contrast"), Some(1.25));
}

#[test]
fn update_effect_params_toggle_remove_and_transform_support_undo_redo_and_validation() {
    let (mut project, mut history, clip_id) = setup_clip();

    let zoom_fx = ClipEffect::with_defaults(EffectKind::Zoom);
    let zoom_id = zoom_fx.id;
    let film_fx = ClipEffect::with_defaults(EffectKind::FilmStyle);
    let film_id = film_fx.id;

    history
        .execute(Box::new(AddClipEffectCommand::new(clip_id, zoom_fx)), &mut project)
        .unwrap();
    history
        .execute(Box::new(AddClipEffectCommand::new(clip_id, film_fx)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.effects.len(), 2);

    // Parâmetro inválido não altera estado nem entra no histórico
    let bad_zoom = EffectParameters::Zoom(ZoomParams {
        start_scale: 0.0,
        ..ZoomParams::default()
    });
    assert!(matches!(
        history.execute(
            Box::new(UpdateEffectParamsCommand::new(clip_id, zoom_id, bad_zoom)),
            &mut project
        ),
        Err(HistoryError::Timeline(TimelineError::InvalidEffect { field: "start_scale", .. }))
    ));

    // Kind incompatível rejeitado
    let wrong_kind = EffectParameters::Blur(BlurParams { radius: 0.5 });
    assert!(matches!(
        history.execute(
            Box::new(UpdateEffectParamsCommand::new(clip_id, zoom_id, wrong_kind)),
            &mut project
        ),
        Err(HistoryError::Timeline(TimelineError::InvalidEffect { field: "params", .. }))
    ));

    // Atualização válida de Zoom e Film
    let edited_zoom = EffectParameters::Zoom(ZoomParams {
        start_scale: 1.05,
        end_scale: 1.45,
        focus_x: -0.25,
        focus_y: 0.15,
        easing: ZoomEasing::Linear,
    });
    history
        .execute(
            Box::new(UpdateEffectParamsCommand::new(clip_id, zoom_id, edited_zoom)),
            &mut project,
        )
        .unwrap();

    let edited_film = EffectParameters::Film(FilmParams {
        intensity: 0.9,
        contrast: 1.3,
        saturation: 0.7,
        vignette: 0.6,
    });
    history
        .execute(
            Box::new(UpdateEffectParamsCommand::new(clip_id, film_id, edited_film)),
            &mut project,
        )
        .unwrap();

    assert_eq!(
        project.timeline.clip(clip_id).unwrap().1.effects[0].params,
        edited_zoom
    );
    assert_eq!(
        project.timeline.clip(clip_id).unwrap().1.effects[1].params,
        edited_film
    );

    // ToggleEffectCommand e RemoveEffectCommand
    history
        .execute(Box::new(ToggleEffectCommand::new(clip_id, film_id)), &mut project)
        .unwrap();
    assert!(!project.timeline.clip(clip_id).unwrap().1.effects[1].enabled);

    history
        .execute(Box::new(RemoveEffectCommand::new(clip_id, zoom_id)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.effects.len(), 1);
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.effects[0].id, film_id);

    // SetClipTransformCommand (alias de UpdateClipTransformCommand)
    let new_tr = ClipTransform {
        scale: 1.3,
        pos_x: 120.0,
        pos_y: -45.0,
        opacity: 0.85,
        speed: 1.25,
    };
    history
        .execute(Box::new(SetClipTransformCommand::new(clip_id, new_tr)), &mut project)
        .unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, new_tr);

    // Undo na ordem inversa: Transform -> Remove -> Toggle -> Edit Film -> Edit Zoom
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.clip(clip_id).unwrap().1.transform, ClipTransform::default());

    history.undo(&mut project).unwrap();
    let fx_after_unremove = &project.timeline.clip(clip_id).unwrap().1.effects;
    assert_eq!(fx_after_unremove.len(), 2);
    assert_eq!(fx_after_unremove[0].id, zoom_id);
    assert_eq!(fx_after_unremove[0].params, edited_zoom);

    history.undo(&mut project).unwrap();
    assert!(project.timeline.clip(clip_id).unwrap().1.effects[1].enabled);

    history.undo(&mut project).unwrap();
    assert_eq!(
        project.timeline.clip(clip_id).unwrap().1.effects[1].params,
        EffectParameters::Film(FilmParams::default())
    );

    history.undo(&mut project).unwrap();
    assert_eq!(
        project.timeline.clip(clip_id).unwrap().1.effects[0].params,
        EffectParameters::Zoom(ZoomParams::default())
    );

    // Redo restaura até a remoção e transformação
    for _ in 0..5 {
        history.redo(&mut project).unwrap();
    }
    let final_clip = project.timeline.clip(clip_id).unwrap().1;
    assert_eq!(final_clip.effects.len(), 1);
    assert!(!final_clip.effects[0].enabled);
    assert_eq!(final_clip.effects[0].params, edited_film);
    assert_eq!(final_clip.transform, new_tr);
}

#[test]
fn v02_legacy_clip_effect_json_deserializes_cleanly_and_roundtrips_v03() {
    let clip_id = ClipId::new();
    let asset_id = meraki_core::AssetId::new();
    let legacy_clip_json = format!(
        r#"{{
            "id": "{clip_id}",
            "asset_id": "{asset_id}",
            "timeline_start_ns": 0,
            "source_in_ns": 0,
            "duration_ns": 4000000000,
            "effects": [
                {{ "kind": "FilmStyle", "intensity": 0.72, "enabled": true }},
                {{ "kind": "Glitch", "intensity": 0.5, "enabled": false }}
            ]
        }}"#
    );

    let parsed: Clip = serde_json::from_str(&legacy_clip_json).unwrap();
    assert_eq!(parsed.effects.len(), 2);

    let film = &parsed.effects[0];
    assert_eq!(film.kind, EffectKind::FilmStyle);
    assert_eq!(film.name, "Estilo Filme");
    assert!((film.intensity - 0.72).abs() < 1e-6);
    assert!(film.enabled);
    assert_eq!(film.short_badge_label(), "Filme");
    match film.params {
        EffectParameters::Film(p) => {
            assert!((p.intensity - 0.72).abs() < 1e-5);
            assert!((p.contrast - 1.12).abs() < 1e-5);
        }
        other => panic!("esperava FilmParams, obteve {other:?}"),
    }

    let glitch = &parsed.effects[1];
    assert_eq!(glitch.kind, EffectKind::Glitch);
    assert!(!glitch.enabled);
    assert_eq!(glitch.short_badge_label(), "Glitch");
    assert!(glitch.validate().is_ok());

    // Round-trip v0.3.0 preserva UUID e todos os parâmetros
    let json_v03 = serde_json::to_string(&parsed).unwrap();
    let reparsed: Clip = serde_json::from_str(&json_v03).unwrap();
    assert_eq!(reparsed, parsed);
}
