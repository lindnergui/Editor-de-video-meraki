use meraki_core::{
    ExportFormat, ExportFrameRate, ExportJobStatus, ExportPreset, ExportQuality, ExportResolution,
    MediaMetadata,
};
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent, EngineWorkerChannel};
use meraki_ui::components::delivery_presets::filtered_presets;
use meraki_ui::components::delivery_summary::estimated_size_bar_ratio;
use meraki_ui::{
    app_delivery::{format_eta, format_file_size, format_hms},
    CoverSlot, DeliveryMessage, MerakiApp, Message, WorkspaceTab,
};
use std::path::PathBuf;

fn drain_commands(worker: &EngineWorkerChannel) -> Vec<EngineCommand> {
    let mut cmds = Vec::new();
    while let Some(c) = worker.try_recv_command() {
        cmds.push(c);
    }
    cmds
}

fn setup_app_with_video_clip(duration_ns: u64) -> (MerakiApp, EngineWorkerChannel) {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let asset_id = app.import_media(PathBuf::from("clipe.mp4"), None, false).unwrap();
    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns,
        width: Some(1920),
        height: Some(1080),
        has_audio: true,
        ..MediaMetadata::default()
    })));
    app.send_asset_to_timeline(asset_id).unwrap();
    let _ = drain_commands(&worker);
    (app, worker)
}

#[test]
fn topbar_export_button_navigates_directly_to_delivery_tab() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    for origin in [
        WorkspaceTab::Files,
        WorkspaceTab::Edit,
        WorkspaceTab::Effects,
        WorkspaceTab::Audio,
    ] {
        let _ = app.update(Message::SelectTab(origin));
        assert_eq!(app.active_tab(), origin);
        let _ = app.update(Message::ExportClicked);
        assert_eq!(app.active_tab(), WorkspaceTab::Delivery);
    }
}

#[test]
fn presets_search_and_selection_update_settings_and_preserve_output_dir() {
    assert_eq!(filtered_presets("").len(), 6);
    assert_eq!(filtered_presets("vertical"), vec![ExportPreset::TikTokReels]);
    assert_eq!(filtered_presets("prores"), vec![ExportPreset::MasterProRes]);
    assert_eq!(filtered_presets("áudio"), vec![ExportPreset::AudioOnly]);
    assert!(filtered_presets("inexistente").is_empty());

    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let custom_dir = PathBuf::from("exports_personalizados");
    let _ = app.update(Message::Delivery(DeliveryMessage::OutputDirPicked(Some(
        custom_dir.clone(),
    ))));
    let _ = app.update(Message::Delivery(DeliveryMessage::PresetSearchChanged(
        "TikTok".to_string(),
    )));
    assert_eq!(app.delivery_state().preset_search, "TikTok");

    let _ = app.update(Message::Delivery(DeliveryMessage::SelectPreset(
        ExportPreset::TikTokReels,
    )));
    let s = &app.delivery_state().settings;
    assert_eq!(s.preset, ExportPreset::TikTokReels);
    assert_eq!(s.resolution, ExportResolution::Vertical1080x1920);
    assert_eq!(s.fps, ExportFrameRate::Fps60);
    assert_eq!(s.format, ExportFormat::Mp4H264);
    assert_eq!(s.output_dir, custom_dir);
    assert_eq!(app.project().export_preset, ExportPreset::TikTokReels);

    let _ = app.update(Message::Delivery(DeliveryMessage::SelectPreset(
        ExportPreset::MasterProRes,
    )));
    assert_eq!(app.delivery_state().settings.format, ExportFormat::MovProRes);
    assert!(app.delivery_state().settings.file_name.ends_with(".mov"));

    let _ = app.update(Message::Delivery(DeliveryMessage::SelectPreset(
        ExportPreset::AudioOnly,
    )));
    assert_eq!(app.delivery_state().settings.format, ExportFormat::AudioWav);
    assert!(app.delivery_state().settings.file_name.ends_with(".wav"));
}

#[test]
fn resolution_quality_format_and_cover_slots_recalculate_estimates_and_seek() {
    let duration_ns = 12_000_000_000;
    let (mut app, worker) = setup_app_with_video_clip(duration_ns);

    let low_720 = {
        let _ = app.update(Message::Delivery(DeliveryMessage::ResolutionChanged(
            ExportResolution::Sd720p,
        )));
        let _ = app.update(Message::Delivery(DeliveryMessage::QualityChanged(
            ExportQuality::Low,
        )));
        app.delivery_state().settings.estimate_file_size_bytes(duration_ns)
    };
    let high_4k = {
        let _ = app.update(Message::Delivery(DeliveryMessage::ResolutionChanged(
            ExportResolution::Uhd4k,
        )));
        let _ = app.update(Message::Delivery(DeliveryMessage::QualityChanged(
            ExportQuality::High,
        )));
        app.delivery_state().settings.estimate_file_size_bytes(duration_ns)
    };
    assert!(high_4k > low_720);
    assert!(estimated_size_bar_ratio(high_4k) > estimated_size_bar_ratio(low_720));
    assert_eq!(estimated_size_bar_ratio(0), 0.0);

    // Alteração de nome, pasta e formato preserva stem trocando extensão
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged(
        "trailer-final.mp4".to_string(),
    )));
    let _ = app.update(Message::Delivery(DeliveryMessage::FormatChanged(
        ExportFormat::AudioMp3,
    )));
    assert_eq!(app.delivery_state().settings.file_name, "trailer-final.mp3");
    assert_eq!(format_hms(3_723_000_000_000), "01:02:03");
    assert_eq!(format_file_size(1_536), "1.5 KB");
    assert_eq!(format_eta(None), "Calculando...");
    assert_eq!(format_eta(Some(75)), "01:15");

    // Seleção de slots de capa grava cover_timestamp_ns e dispara request_seek
    let _ = app.update(Message::Delivery(DeliveryMessage::SelectCoverSlot(CoverSlot::Middle)));
    assert_eq!(app.delivery_state().selected_cover_slot, Some(CoverSlot::Middle));
    assert_eq!(app.delivery_state().settings.cover_timestamp_ns, Some(6_000_000_000));
    assert_eq!(worker.take_pending_seek(), Some(6_000_000_000));

    let _ = app.update(Message::Delivery(DeliveryMessage::SelectCoverSlot(CoverSlot::End)));
    assert_eq!(
        app.delivery_state().settings.cover_timestamp_ns,
        Some(11_900_000_000)
    );
    assert_eq!(worker.take_pending_seek(), Some(11_900_000_000));
}

#[test]
fn empty_timeline_rejects_export_without_sending_engine_command() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));
    assert!(app.delivery_state().queue.is_empty());
    assert!(app.delivery_state().notice.is_some());
    assert!(app.last_error().is_some());
    assert!(drain_commands(&worker).is_empty());
}

#[test]
fn queue_enqueues_tracks_progress_advances_and_records_history() {
    let (mut app, worker) = setup_app_with_video_clip(8_000_000_000);
    let _ = app.update(Message::Delivery(DeliveryMessage::OutputDirPicked(Some(
        PathBuf::from("saida"),
    ))));
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged(
        "parte1".to_string(),
    )));
    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));

    let job1_id = app.delivery_state().queue[0].id;
    assert_eq!(app.delivery_state().active_job_id, Some(job1_id));
    assert!(matches!(
        app.delivery_state().queue[0].status,
        ExportJobStatus::Running { progress, eta_seconds: None } if progress == 0.0
    ));
    let cmds1 = drain_commands(&worker);
    assert!(matches!(
        &cmds1[..],
        [EngineCommand::StartExport { job_id, .. }] if *job_id == job1_id
    ));

    // Enfileira segundo job enquanto o primeiro ainda executa
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged(
        "parte2".to_string(),
    )));
    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));
    assert_eq!(app.delivery_state().queue.len(), 2);
    let job2_id = app.delivery_state().queue[1].id;
    assert!(matches!(app.delivery_state().queue[1].status, ExportJobStatus::Queued));
    assert!(drain_commands(&worker).is_empty());

    // Progresso atualiza estado do job ativo
    let _ = app.update(Message::EngineEvent(EngineEvent::ExportProgress {
        job_id: job1_id,
        progress: 0.65,
        eta_seconds: Some(9),
    }));
    assert!(matches!(
        app.delivery_state().queue[0].status,
        ExportJobStatus::Running { progress, eta_seconds: Some(9) } if (progress - 0.65).abs() < 1e-4
    ));

    // Conclusão do job 1 registra no histórico e inicia o job 2 automaticamente
    let _ = app.update(Message::EngineEvent(EngineEvent::ExportFinished {
        job_id: job1_id,
        output_path: PathBuf::from("saida/parte1.mp4"),
        file_size_bytes: 2_450_000,
    }));
    assert_eq!(app.delivery_state().history.len(), 1);
    assert_eq!(app.delivery_state().history[0].id, job1_id);
    assert_eq!(
        app.delivery_state().history[0].status,
        ExportJobStatus::Completed { file_size_bytes: 2_450_000 }
    );
    assert_eq!(app.delivery_state().active_job_id, Some(job2_id));
    let cmds2 = drain_commands(&worker);
    assert!(matches!(
        &cmds2[..],
        [EngineCommand::StartExport { job_id, .. }] if *job_id == job2_id
    ));

    // Erro no job 2 registra falha no histórico
    let _ = app.update(Message::EngineEvent(EngineEvent::ExportFailed {
        job_id: job2_id,
        error: "erro simulado".to_string(),
    }));
    assert!(app.delivery_state().queue.is_empty());
    assert_eq!(app.delivery_state().active_job_id, None);
    assert_eq!(app.delivery_state().history.len(), 2);
    assert_eq!(app.delivery_state().history[0].id, job2_id);
    assert_eq!(
        app.delivery_state().history[0].status,
        ExportJobStatus::Failed("erro simulado".to_string())
    );
}

#[test]
fn cancelling_active_and_queued_jobs_dispatches_cancel_and_resumes_on_start_queue() {
    let (mut app, worker) = setup_app_with_video_clip(5_000_000_000);
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged("j1".to_string())));
    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged("j2".to_string())));
    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));
    let _ = app.update(Message::Delivery(DeliveryMessage::FileNameChanged("j3".to_string())));
    let _ = app.update(Message::Delivery(DeliveryMessage::ExportClicked));
    let _ = drain_commands(&worker);

    let j1 = app.delivery_state().queue[0].id;
    let j2 = app.delivery_state().queue[1].id;
    let j3 = app.delivery_state().queue[2].id;

    // Cancela o job ativo -> envia CancelExport e pausa auto-avanço
    let _ = app.update(Message::Delivery(DeliveryMessage::CancelJob(j1)));
    let cancel_cmds = drain_commands(&worker);
    assert!(matches!(
        &cancel_cmds[..],
        [EngineCommand::CancelExport { job_id }] if *job_id == j1
    ));

    let _ = app.update(Message::EngineEvent(EngineEvent::ExportCancelled { job_id: j1 }));
    assert_eq!(app.delivery_state().active_job_id, None);
    assert_eq!(app.delivery_state().history[0].status, ExportJobStatus::Cancelled);
    assert_eq!(app.delivery_state().queue.len(), 2);

    // Cancela item ainda pendente diretamente sem precisar enviar CancelExport ao motor
    let _ = app.update(Message::Delivery(DeliveryMessage::CancelJob(j3)));
    assert!(drain_commands(&worker).is_empty());
    assert_eq!(app.delivery_state().queue.len(), 1);
    assert_eq!(app.delivery_state().history[0].id, j3);

    // Iniciar Fila retoma o job restante
    let _ = app.update(Message::Delivery(DeliveryMessage::StartQueue));
    assert_eq!(app.delivery_state().active_job_id, Some(j2));
    assert!(matches!(
        &drain_commands(&worker)[..],
        [EngineCommand::StartExport { job_id, .. }] if *job_id == j2
    ));
}
