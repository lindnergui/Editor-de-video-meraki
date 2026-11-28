pub mod app;
mod app_actions;
mod app_audio;
mod app_audio_live;
pub mod app_delivery;
pub mod app_exit_guard;
mod app_live;
pub mod app_precision;
pub mod app_preferences;
pub mod app_project_io;
pub mod app_proxy;
pub mod app_screens;
mod app_transitions;
pub mod components;
pub mod fonts;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod navigation;
pub mod playback;
pub mod project_storage;
pub mod subscription;
pub mod theme;
pub mod views;
pub mod window_icon;

pub use app::{MerakiApp, Message};
pub use app_delivery::{CoverSlot, DeliveryMessage, DeliveryState};
pub use app_exit_guard::{ExitGuardMessage, PendingExitAction};
pub use app_precision::PrecisionMessage;
pub use app_project_io::{ProjectIoMessage, ProjectIoState};
pub use app_proxy::{ProxyBannerState, ProxyStatus};
pub use app_screens::{AppScreen, ScreenMessage, WelcomeReturnTarget};
pub use components::settings_menu::SettingsMenuMessage;
pub use navigation::WorkspaceTab;
pub use playback::{PlaybackMessage, PlaybackState};

use iced::{window, Size, Task};
use meraki_engine::EngineHandle;

/// Inicia a janela nativa do Meraki conectada ao canal assíncrono do motor.
pub fn run(engine: EngineHandle) -> iced::Result {
    iced::application(MerakiApp::title, MerakiApp::update, MerakiApp::view)
        .font(fonts::INTER_REGULAR_BYTES)
        .font(fonts::INTER_MEDIUM_BYTES)
        .font(fonts::INTER_SEMIBOLD_BYTES)
        .font(fonts::INTER_BOLD_BYTES)
        .default_font(theme::FONT_REGULAR)
        .subscription(MerakiApp::subscription)
        .theme(MerakiApp::theme)
        .window(window::Settings {
            size: Size::new(1280.0, 800.0),
            min_size: Some(Size::new(1120.0, 680.0)),
            icon: window_icon::app_icon(),
            position: window::Position::Centered,
            exit_on_close_request: false,
            ..window::Settings::default()
        })
        .antialiasing(true)
        .run_with(move || (MerakiApp::new(engine), Task::none()))
}
