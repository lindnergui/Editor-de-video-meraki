use crate::app_project_io::{map_project_shortcut, ProjectIoMessage};
use crate::playback::{map_keyboard_shortcut, PlaybackMessage};
use iced::futures::sink::SinkExt;
use iced::futures::Stream;
use iced::{keyboard, stream, Subscription};
use meraki_engine::{EngineEvent, EngineHandle};
use std::any::TypeId;
use std::thread;
use tracing::error;

struct EngineEventBridge;

/// Assinatura reativa do Iced que encaminha eventos do `EngineHandle` sem bloquear a UI.
pub fn engine_events(engine: &EngineHandle) -> Subscription<EngineEvent> {
    Subscription::run_with_id(TypeId::of::<EngineEventBridge>(), engine_events_stream(engine))
}

/// Assinatura global de atalhos de teclado (disparada somente fora de campos `text_input` focados).
pub fn keyboard_shortcuts() -> Subscription<PlaybackMessage> {
    keyboard::on_key_press(map_keyboard_shortcut)
}

/// Assinatura global de atalhos de projeto (`Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+O`).
pub fn project_io_shortcuts() -> Subscription<ProjectIoMessage> {
    keyboard::on_key_press(map_project_shortcut)
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
