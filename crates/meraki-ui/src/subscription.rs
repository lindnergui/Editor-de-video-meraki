use crate::app::Message;
use crate::app_precision::PrecisionMessage;
use crate::app_project_io::ProjectIoMessage;
use crate::playback::PlaybackMessage;
use iced::futures::sink::SinkExt;
use iced::futures::Stream;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::{keyboard, stream, Subscription};
use meraki_core::KeymapProfile;
use meraki_engine::{EngineEvent, EngineHandle};
use std::any::TypeId;
use std::thread;
use tracing::error;

struct EngineEventBridge;

/// Assinatura reativa do Iced que encaminha eventos do `EngineHandle` sem bloquear a UI.
pub fn engine_events(engine: &EngineHandle) -> Subscription<EngineEvent> {
    Subscription::run_with_id(TypeId::of::<EngineEventBridge>(), engine_events_stream(engine))
}

/// Traduz atalhos globais de teclado respeitando o `KeymapProfile` ativo.
pub fn map_keyboard_shortcut(
    profile: KeymapProfile,
    key: Key,
    modifiers: Modifiers,
) -> Option<Message> {
    if modifiers.alt() {
        return None;
    }

    let primary = modifiers.control() || modifiers.command();
    if primary {
        if let Key::Character(ch) = key.as_ref() {
            if ch.eq_ignore_ascii_case("s") {
                return Some(Message::ProjectIo(if modifiers.shift() {
                    ProjectIoMessage::SaveAs
                } else {
                    ProjectIoMessage::Save
                }));
            }
            if ch.eq_ignore_ascii_case("o") && !modifiers.shift() {
                return Some(Message::ProjectIo(ProjectIoMessage::Open));
            }
            if ch.eq_ignore_ascii_case("z") {
                return Some(Message::Playback(if modifiers.shift() {
                    PlaybackMessage::Redo
                } else {
                    PlaybackMessage::Undo
                }));
            }
            if ch.eq_ignore_ascii_case("y")
                && !modifiers.shift()
                && profile == KeymapProfile::VegasPro
            {
                return Some(Message::Playback(PlaybackMessage::Redo));
            }
            if ch.eq_ignore_ascii_case("k")
                && !modifiers.shift()
                && profile == KeymapProfile::PremierePro
            {
                return Some(Message::Precision(PrecisionMessage::SplitAtPlayhead));
            }
            if ch.eq_ignore_ascii_case("b")
                && !modifiers.shift()
                && matches!(
                    profile,
                    KeymapProfile::DaVinciResolve | KeymapProfile::CapCut
                )
            {
                return Some(Message::Precision(PrecisionMessage::SplitAtPlayhead));
            }
        }
        return None;
    }

    match key.as_ref() {
        Key::Named(Named::ArrowLeft) => {
            return Some(Message::Precision(if modifiers.shift() {
                PrecisionMessage::StepSeconds(-1)
            } else {
                PrecisionMessage::StepFrames(-1)
            }));
        }
        Key::Named(Named::ArrowRight) => {
            return Some(Message::Precision(if modifiers.shift() {
                PrecisionMessage::StepSeconds(1)
            } else {
                PrecisionMessage::StepFrames(1)
            }));
        }
        _ => {}
    }

    if modifiers.shift() {
        return None;
    }

    match key.as_ref() {
        Key::Named(Named::Space) | Key::Character(" ") => {
            Some(Message::Playback(PlaybackMessage::TogglePlayPause))
        }
        Key::Named(Named::Delete | Named::Backspace) => {
            Some(Message::Playback(PlaybackMessage::DeleteSelectedClip))
        }
        Key::Named(Named::F8) if profile == KeymapProfile::VegasPro => {
            Some(Message::Precision(PrecisionMessage::ToggleSnap))
        }
        Key::Character(ch) => map_unmodified_char_shortcut(profile, ch),
        _ => None,
    }
}

fn map_unmodified_char_shortcut(profile: KeymapProfile, ch: &str) -> Option<Message> {
    let msg = match profile {
        KeymapProfile::PremierePro => {
            if ch.eq_ignore_ascii_case("v") {
                PrecisionMessage::SelectTool
            } else if ch.eq_ignore_ascii_case("s") {
                PrecisionMessage::ToggleSnap
            } else {
                return None;
            }
        }
        KeymapProfile::DaVinciResolve | KeymapProfile::CapCut => {
            if ch.eq_ignore_ascii_case("a") {
                PrecisionMessage::SelectTool
            } else if ch.eq_ignore_ascii_case("n") {
                PrecisionMessage::ToggleSnap
            } else {
                return None;
            }
        }
        KeymapProfile::VegasPro => {
            if ch.eq_ignore_ascii_case("s") {
                PrecisionMessage::SplitAtPlayhead
            } else if ch.eq_ignore_ascii_case("d") {
                PrecisionMessage::SelectTool
            } else {
                return None;
            }
        }
    };
    Some(Message::Precision(msg))
}

fn map_premiere(key: Key, modifiers: Modifiers) -> Option<Message> {
    map_keyboard_shortcut(KeymapProfile::PremierePro, key, modifiers)
}
fn map_davinci(key: Key, modifiers: Modifiers) -> Option<Message> {
    map_keyboard_shortcut(KeymapProfile::DaVinciResolve, key, modifiers)
}
fn map_capcut(key: Key, modifiers: Modifiers) -> Option<Message> {
    map_keyboard_shortcut(KeymapProfile::CapCut, key, modifiers)
}
fn map_vegas(key: Key, modifiers: Modifiers) -> Option<Message> {
    map_keyboard_shortcut(KeymapProfile::VegasPro, key, modifiers)
}

/// Assinatura para interceptar pedidos de fechamento da janela antes do encerramento imediato.
pub fn window_close_requests() -> Subscription<Message> {
    iced::window::close_requests().map(Message::WindowCloseRequested)
}

/// Assinatura global de atalhos de teclado (disparada apenas fora de campos `text_input` focados).
pub fn keyboard_shortcuts(profile: KeymapProfile) -> Subscription<Message> {
    let mapper = match profile {
        KeymapProfile::PremierePro => map_premiere,
        KeymapProfile::DaVinciResolve => map_davinci,
        KeymapProfile::CapCut => map_capcut,
        KeymapProfile::VegasPro => map_vegas,
    };
    keyboard::on_key_press(mapper)
}

/// Constrói o `Stream` assíncrono alimentado por uma thread leitora dedicada do canal do motor.
pub fn engine_events_stream(engine: &EngineHandle) -> impl Stream<Item = EngineEvent> {
    let rx = engine.events().clone();

    stream::channel(64, move |mut output| async move {
        if let Err(err) = thread::Builder::new()
            .name("meraki-ui-events".to_string())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    if let Err(err) = output.try_send(event) {
                        if err.is_disconnected() {
                            break;
                        }
                        let recovered = err.into_inner();
                        if is_critical_event(&recovered)
                            && iced::futures::executor::block_on(output.send(recovered)).is_err()
                        {
                            break;
                        }
                    }
                }
            })
        {
            error!(error = %err, "falha ao iniciar thread ponte de eventos do motor");
        }
    })
}

fn is_critical_event(event: &EngineEvent) -> bool {
    !matches!(
        event,
        EngineEvent::FrameReady(_)
            | EngineEvent::AudioLevels(_)
            | EngineEvent::PlayheadTick(_)
            | EngineEvent::ExportProgress { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::StreamExt;
    use meraki_core::{AssetId, MediaMetadata};
    use meraki_engine::protocol::create_engine_channel;

    #[test]
    fn engine_events_stream_forwards_engine_events_until_worker_disconnects() {
        let (handle, worker) = create_engine_channel();
        let asset_id = AssetId::new();
        worker.emit_critical(EngineEvent::EngineReady);
        worker.emit_critical(EngineEvent::MediaDiscovered(MediaMetadata {
            asset_id,
            duration_ns: 2_500_000_000,
            width: Some(1280),
            height: Some(720),
            has_audio: true,
            ..MediaMetadata::default()
        }));
        drop(worker);

        let collected: Vec<EngineEvent> = iced::futures::executor::block_on(async {
            engine_events_stream(&handle).collect().await
        });

        assert_eq!(collected.len(), 2);
        assert!(matches!(collected[0], EngineEvent::EngineReady));
        assert!(matches!(
            &collected[1],
            EngineEvent::MediaDiscovered(meta)
                if meta.asset_id == asset_id && meta.duration_ns == 2_500_000_000
        ));
    }
}
