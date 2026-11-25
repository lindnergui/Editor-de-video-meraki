use meraki_core::{
    ExportError, ExportFormat, ExportFrameRate, ExportJob, ExportJobStatus, ExportPreset,
    ExportQuality, ExportResolution, ExportSettings,
};
use std::path::PathBuf;

#[test]
fn presets_provide_expected_formats_dimensions_fps_and_labels() {
    assert_eq!(ExportResolution::Sd720p.dimensions(), (1280, 720));
    assert_eq!(ExportResolution::FullHd1080p.dimensions(), (1920, 1080));
    assert_eq!(ExportResolution::Uhd4k.dimensions(), (3840, 2160));
    assert_eq!(ExportResolution::Vertical1080x1920.dimensions(), (1080, 1920));
    assert_eq!(ExportResolution::Square1080x1080.dimensions(), (1080, 1080));

    for preset in ExportPreset::ALL {
        assert!(!preset.label().is_empty());
        assert!(!preset.description().is_empty());
        let settings = ExportSettings::from_preset(preset);
        assert_eq!(settings.preset, preset);
        settings.validate().unwrap();
    }

    let yt = ExportSettings::from_preset(ExportPreset::YouTube);
    assert_eq!(yt.format, ExportFormat::Mp4H264);
    assert_eq!(yt.resolution, ExportResolution::FullHd1080p);
    assert_eq!(yt.fps, ExportFrameRate::Fps30);

    let tt = ExportSettings::from_preset(ExportPreset::TikTokReels);
    assert_eq!(tt.resolution.dimensions(), (1080, 1920));
    assert_eq!(tt.fps, ExportFrameRate::Fps60);

    let ig = ExportSettings::from_preset(ExportPreset::Instagram);
    assert_eq!(ig.resolution.dimensions(), (1080, 1080));

    let wa = ExportSettings::from_preset(ExportPreset::WhatsAppLite);
    assert_eq!(wa.resolution, ExportResolution::Sd720p);
    assert_eq!(wa.quality, ExportQuality::Low);

    let prores = ExportSettings::from_preset(ExportPreset::MasterProRes);
    assert_eq!(prores.format, ExportFormat::MovProRes);
    assert_eq!(prores.quality, ExportQuality::Master);

    let audio = ExportSettings::from_preset(ExportPreset::AudioOnly);
    assert!(audio.format.is_audio_only());
    assert_eq!(audio.format, ExportFormat::AudioWav);
}

#[test]
fn file_name_sanitization_normalizes_extension_and_rejects_unsafe_paths() {
    let mut s = ExportSettings::from_preset(ExportPreset::YouTube);
    s.output_dir = PathBuf::from("exports");
    s.file_name = "  meu:video*final  ".to_string();
    assert_eq!(s.sanitized_file_name().unwrap(), "meu_video_final.mp4");
    assert_eq!(s.resolved_output_path().unwrap(), PathBuf::from("exports").join("meu_video_final.mp4"));

    // Trocar formato substitui extensão anterior em vez de concatenar .mp4.wav
    s.file_name = "corte_final.mp4".to_string();
    s.format = ExportFormat::AudioWav;
    assert_eq!(s.sanitized_file_name().unwrap(), "corte_final.wav");

    s.format = ExportFormat::MovProRes;
    assert_eq!(s.sanitized_file_name().unwrap(), "corte_final.mov");

    s.format = ExportFormat::AudioMp3;
    assert_eq!(s.sanitized_file_name().unwrap(), "corte_final.mp3");

    s.file_name = "   ".to_string();
    assert!(matches!(s.sanitized_file_name(), Err(ExportError::EmptyFileName)));

    s.file_name = "../fora.mp4".to_string();
    assert!(matches!(s.sanitized_file_name(), Err(ExportError::InvalidFileName(_))));

    s.file_name = r"sub\arq.mp4".to_string();
    assert!(matches!(s.sanitized_file_name(), Err(ExportError::InvalidFileName(_))));

    s.file_name = "ok".to_string();
    s.output_dir = PathBuf::new();
    assert!(matches!(s.resolved_output_path(), Err(ExportError::EmptyOutputDir)));
}

#[test]
fn estimate_file_size_scales_with_duration_resolution_quality_and_format() {
    let duration_10s = 10_000_000_000_u64;
    let yt = ExportSettings::from_preset(ExportPreset::YouTube);
    assert_eq!(yt.estimate_file_size_bytes(0), 0);

    let size_yt = yt.estimate_file_size_bytes(duration_10s);
    let size_wa = ExportSettings::from_preset(ExportPreset::WhatsAppLite)
        .estimate_file_size_bytes(duration_10s);
    let size_prores = ExportSettings::from_preset(ExportPreset::MasterProRes)
        .estimate_file_size_bytes(duration_10s);
    let size_wav = ExportSettings::from_preset(ExportPreset::AudioOnly)
        .estimate_file_size_bytes(duration_10s);
    let mut mp3 = ExportSettings::from_preset(ExportPreset::AudioOnly);
    mp3.format = ExportFormat::AudioMp3;
    let size_mp3 = mp3.estimate_file_size_bytes(duration_10s);

    assert!(size_wa < size_yt);
    assert!(size_yt < size_prores);
    assert!(size_mp3 < size_wav);
    assert!(yt.estimate_file_size_bytes(duration_10s * 2) > size_yt);
}

#[test]
fn export_settings_and_job_serialize_cleanly_with_legacy_preset_aliases() {
    let mut settings = ExportSettings::from_preset(ExportPreset::TikTokReels);
    settings.cover_timestamp_ns = Some(2_500_000_000);
    let job = ExportJob::new(settings.clone(), 5_000_000_000).unwrap();
    assert!(matches!(job.status, ExportJobStatus::Queued));

    let json = serde_json::to_string(&job).unwrap();
    let decoded: ExportJob = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, job);

    // Aliases JSON legados mantêm retrocompatibilidade
    let legacy_wa: ExportPreset = serde_json::from_str("\"WhatsAppLight\"").unwrap();
    let legacy_prores: ExportPreset = serde_json::from_str("\"ProResHighQuality\"").unwrap();
    assert_eq!(legacy_wa, ExportPreset::WhatsAppLite);
    assert_eq!(legacy_prores, ExportPreset::MasterProRes);

    assert!(matches!(ExportJob::new(settings, 0), Err(ExportError::EmptyTimeline)));
}
