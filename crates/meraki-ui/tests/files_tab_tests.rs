use meraki_core::MediaMetadata;
use meraki_engine::protocol::create_engine_channel;
use meraki_engine::{EngineCommand, EngineEvent};
use meraki_ui::views::files::{
    filtered_assets, folder_asset_count, format_duration_mm_ss, format_file_size,
    format_resolution, FilesMessage, MediaFilter,
};
use meraki_ui::{MerakiApp, Message};
use std::path::PathBuf;

#[test]
fn files_picked_routes_to_default_folders_and_dispatches_import_command() {
    let (engine, worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::Files(FilesMessage::FilesPicked(vec![
        PathBuf::from("entrevista.mp4"),
        PathBuf::from("trilha.wav"),
        PathBuf::from("capa.png"),
    ])));

    assert_eq!(app.project().assets.len(), 3);
    let cam_id = app.project().folders[0].id;
    let sfx_id = app.project().folders[1].id;
    let img_id = app.project().folders[2].id;
    assert_eq!(folder_asset_count(app.project(), cam_id), 1);
    assert_eq!(folder_asset_count(app.project(), sfx_id), 1);
    assert_eq!(folder_asset_count(app.project(), img_id), 1);

    let mut dispatched = 0;
    while let Some(cmd) = worker.try_recv_command() {
        if matches!(cmd, EngineCommand::ImportMedia { .. }) {
            dispatched += 1;
        }
    }
    assert_eq!(dispatched, 3);
}

#[test]
fn filters_search_and_nested_folder_creation_work() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);
    let cam_id = app.project().folders[0].id;

    let _ = app.update(Message::Files(FilesMessage::SelectFolder(Some(cam_id))));
    let _ = app.update(Message::Files(FilesMessage::ToggleNewFolderInput));
    let _ = app.update(Message::Files(FilesMessage::NewFolderNameChanged("Câmera B".to_string())));
    let _ = app.update(Message::Files(FilesMessage::ConfirmNewFolder));

    let sub_id = app.files_state().selected_folder.unwrap();
    assert_ne!(sub_id, cam_id);

    let _ = app.update(Message::Files(FilesMessage::FilesPicked(vec![
        PathBuf::from("take_01.mp4"),
        PathBuf::from("foto_set.png"),
    ])));
    // Subpasta propaga contagem para pasta pai "Brutos Câmera"
    assert_eq!(folder_asset_count(app.project(), cam_id), 2);
    assert_eq!(folder_asset_count(app.project(), sub_id), 2);

    let _ = app.update(Message::Files(FilesMessage::SelectFilter(MediaFilter::Images)));
    let only_images = filtered_assets(app.project(), app.files_state());
    assert_eq!(only_images.len(), 1);
    assert_eq!(only_images[0].name, "foto_set.png");

    let _ = app.update(Message::Files(FilesMessage::SelectFilter(MediaFilter::All)));
    let _ = app.update(Message::Files(FilesMessage::SearchChanged("TAKE".to_string())));
    let searched = filtered_assets(app.project(), app.files_state());
    assert_eq!(searched.len(), 1);
    assert_eq!(searched[0].name, "take_01.mp4");
}

#[test]
fn mark_in_and_mark_out_update_selected_asset_points_and_formatters() {
    let (engine, _worker) = create_engine_channel();
    let mut app = MerakiApp::new(engine);

    let _ = app.update(Message::Files(FilesMessage::FilesPicked(vec![PathBuf::from("clipe.mp4")])));
    let asset_id = app.files_state().selected_asset.unwrap();

    let _ = app.update(Message::EngineEvent(EngineEvent::MediaDiscovered(MediaMetadata {
        asset_id,
        duration_ns: 10_000_000_000,
        width: Some(1920),
        height: Some(1080),
        file_size_bytes: 2_621_440,
        has_audio: true,
        ..MediaMetadata::default()
    })));

    let _ = app.update(Message::Files(FilesMessage::PreviewScrubChanged(0.20)));
    let _ = app.update(Message::Files(FilesMessage::MarkIn));
    let _ = app.update(Message::Files(FilesMessage::PreviewScrubChanged(0.80)));
    let _ = app.update(Message::Files(FilesMessage::MarkOut));

    let asset = app.project().asset(asset_id).unwrap();
    assert!(asset.in_point_ns.abs_diff(2_000_000_000) <= 1_000);
    assert!(asset.out_point_ns.unwrap().abs_diff(8_000_000_000) <= 1_000);
    assert_eq!(format_duration_mm_ss(135_000_000_000), "02:15");
    assert_eq!(format_file_size(asset.metadata.file_size_bytes), "2.5 MB");
    assert_eq!(format_resolution(asset), "1920x1080");
}
