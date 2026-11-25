use crate::playback::{format_timecode_mm_ss_ff, PlaybackMessage, PlaybackState};
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, horizontal_space, image, row, slider, text, Stack};
use iced::{ContentFit, Element, Length};
use meraki_core::{PreviewQuality, Project};

/// Ações emitidas pelo player central de vídeo e barra de transporte.
#[derive(Debug, Clone)]
pub enum PlayerMessage {
    Undo,
    Redo,
    StepBack,
    TogglePlayPause,
    StepForward,
    SetRate(f64),
    SetVolume(f32),
    SetQuality(PreviewQuality),
}

const SPEED_OPTIONS: [(f64, &str); 4] = [
    (1.0, "1x"),
    (1.2, "1.2x"),
    (1.5, "1.5x"),
    (2.0, "2x"),
];

/// Renderiza o player central com barra Desfazer/Refazer, visor 16:9 e controles de transporte.
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
) -> Element<'a, PlayerMessage> {
    view_with_overlay(project, playback, can_undo, can_redo, |m| m, None)
}

/// Renderiza o player central aceitando uma camada interativa (`stack`) sobre o visor de vídeo.
pub fn view_with_overlay<'a, M: Clone + 'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
    overlay: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let top_bar = row![
        history_button(icons::UNDO, "Desfazer", can_undo, map_player(PlayerMessage::Undo)),
        history_button(icons::REDO, "Refazer", can_redo, map_player(PlayerMessage::Redo)),
        horizontal_space(),
        container(text("16:9").size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY))
            .padding([4, 9])
            .style(theme::badge),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let viewport = container(build_viewport(playback, overlay))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(8)
        .style(theme::preview_viewport);

    let total_ns = project.timeline.total_duration_ns();
    let timecode = format!(
        "{} / {}",
        format_timecode_mm_ss_ff(playback.playhead_ns, 30),
        format_timecode_mm_ss_ff(total_ns, 30),
    );

    let transport_icon = if playback.is_playing { icons::PAUSE } else { icons::PLAY };
    let play_pause_btn = button(icons::icon(transport_icon, 14.0, theme::TEXT_ON_ACCENT))
        .padding([7, 14])
        .style(theme::primary_button)
        .on_press(map_player(PlayerMessage::TogglePlayPause));

    let step_back_btn = button(icons::icon(icons::STEP_BACK, 14.0, theme::TEXT_PRIMARY))
        .padding([7, 9])
        .style(theme::secondary_button)
        .on_press(map_player(PlayerMessage::StepBack));
    let step_forward_btn = button(icons::icon(icons::STEP_FORWARD, 14.0, theme::TEXT_PRIMARY))
        .padding([7, 9])
        .style(theme::secondary_button)
        .on_press(map_player(PlayerMessage::StepForward));

    let mut speed_row = row![].spacing(4).align_y(Vertical::Center);
    for (rate, label) in SPEED_OPTIONS {
        let active = (playback.playback_rate - rate).abs() < 0.05;
        speed_row = speed_row.push(
            button(text(label).size(11).font(theme::FONT_MEDIUM))
                .padding([5, 7])
                .style(theme::filter_pill_button(active))
                .on_press(map_player(PlayerMessage::SetRate(rate))),
        );
    }

    let volume_ctrl = row![
        icons::icon(icons::VOLUME, 14.0, theme::TEXT_SECONDARY),
        slider(0.0_f32..=1.0_f32, playback.master_volume, move |v| {
            map_player(PlayerMessage::SetVolume(v))
        })
        .step(0.05_f32)
        .width(68)
        .style(theme::preview_slider),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let is_720 = playback.preview_quality == PreviewQuality::Proxy720p;
    let quality_row = row![
        quality_pill("720p", is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Proxy720p))),
        quality_pill("1080p", !is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Full1080p))),
    ]
    .spacing(4)
    .align_y(Vertical::Center);

    let transport_bar = container(
        row![
            text(timecode).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            horizontal_space(),
            step_back_btn,
            play_pause_btn,
            step_forward_btn,
            horizontal_space(),
            speed_row,
            volume_ctrl,
            quality_row,
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 12])
    .style(theme::panel);

    container(
        column![top_bar, viewport, transport_bar]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(12)
    .style(theme::base_canvas)
    .into()
}

/// Renderiza o player em proporção compacta para o topo centro-esquerda da aba Áudio.
pub fn view_compact<'a, M: Clone + 'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
) -> Element<'a, M> {
    let top_bar = row![
        history_button(icons::UNDO, "Desfazer", can_undo, map_player(PlayerMessage::Undo)),
        history_button(icons::REDO, "Refazer", can_redo, map_player(PlayerMessage::Redo)),
        horizontal_space(),
        container(text("16:9").size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY))
            .padding([3, 7])
            .style(theme::badge),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let viewport = container(build_viewport(playback, None))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(6)
        .style(theme::preview_viewport);

    let total_ns = project.timeline.total_duration_ns();
    let timecode = format!(
        "{} / {}",
        format_timecode_mm_ss_ff(playback.playhead_ns, 30),
        format_timecode_mm_ss_ff(total_ns, 30),
    );

    let transport_icon = if playback.is_playing { icons::PAUSE } else { icons::PLAY };
    let play_pause_btn = button(icons::icon(transport_icon, 13.0, theme::TEXT_ON_ACCENT))
        .padding([6, 12])
        .style(theme::primary_button)
        .on_press(map_player(PlayerMessage::TogglePlayPause));
    let step_back_btn = button(icons::icon(icons::STEP_BACK, 13.0, theme::TEXT_PRIMARY))
        .padding([6, 8])
        .style(theme::secondary_button)
        .on_press(map_player(PlayerMessage::StepBack));
    let step_forward_btn = button(icons::icon(icons::STEP_FORWARD, 13.0, theme::TEXT_PRIMARY))
        .padding([6, 8])
        .style(theme::secondary_button)
        .on_press(map_player(PlayerMessage::StepForward));

    let is_720 = playback.preview_quality == PreviewQuality::Proxy720p;
    let row_top = row![
        step_back_btn,
        play_pause_btn,
        step_forward_btn,
        horizontal_space(),
        text(timecode).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let row_bottom = row![
        icons::icon(icons::VOLUME, 13.0, theme::TEXT_SECONDARY),
        slider(0.0_f32..=1.0_f32, playback.master_volume, move |v| {
            map_player(PlayerMessage::SetVolume(v))
        })
        .step(0.05_f32)
        .width(Length::Fill)
        .style(theme::preview_slider),
        quality_pill("720p", is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Proxy720p))),
        quality_pill("1080p", !is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Full1080p))),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let transport_bar = container(column![row_top, row_bottom].spacing(6))
        .width(Length::Fill)
        .padding([8, 10])
        .style(theme::panel);

    container(
        column![top_bar, viewport, transport_bar]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(10)
    .style(theme::base_canvas)
    .into()
}

fn build_viewport<'a, M: Clone + 'a>(
    playback: &'a PlaybackState,
    overlay: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let base: Element<'a, M> = match &playback.current_frame {
        Some(handle) => image(handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(ContentFit::Contain)
            .into(),
        None => container(
            column![
                icons::icon(icons::MEDIA_VIDEO, 28.0, theme::TEXT_SECONDARY),
                text("Adicione um clipe à timeline para começar")
                    .size(12)
                    .font(theme::FONT_MEDIUM)
                    .color(theme::TEXT_SECONDARY),
            ]
            .spacing(8)
            .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into(),
    };
    if let Some(overlay_el) = overlay {
        Stack::with_children([base, overlay_el]).width(Length::Fill).height(Length::Fill).into()
    } else {
        base
    }
}

fn history_button<'a, M: Clone + 'a>(
    svg_bytes: &'static [u8],
    label: &'a str,
    enabled: bool,
    msg: M,
) -> Element<'a, M> {
    let tint = if enabled { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    button(
        row![
            icons::icon(svg_bytes, 13.0, tint),
            text(label).size(11).font(theme::FONT_MEDIUM).color(tint),
        ]
        .spacing(5)
        .align_y(Vertical::Center),
    )
    .padding([5, 9])
    .style(theme::secondary_button)
    .on_press_maybe(enabled.then_some(msg))
    .into()
}

fn quality_pill<'a, M: Clone + 'a>(label: &'a str, active: bool, msg: M) -> Element<'a, M> {
    button(text(label).size(11).font(theme::FONT_SEMIBOLD))
        .padding([5, 8])
        .style(theme::filter_pill_button(active))
        .on_press(msg)
        .into()
}

/// Converte mensagens da barra do player em mensagens unificadas de reprodução/histórico.
pub fn into_playback_message(msg: PlayerMessage, project: &Project, playhead_ns: u64) -> PlaybackMessage {
    match msg {
        PlayerMessage::Undo => PlaybackMessage::Undo,
        PlayerMessage::Redo => PlaybackMessage::Redo,
        PlayerMessage::StepBack => PlaybackMessage::Seek(playhead_ns.saturating_sub(1_000_000_000)),
        PlayerMessage::StepForward => {
            let next = playhead_ns.saturating_add(1_000_000_000);
            let total = project.timeline.total_duration_ns();
            PlaybackMessage::Seek(if total > 0 { next.min(total) } else { next })
        }
        PlayerMessage::TogglePlayPause => PlaybackMessage::TogglePlayPause,
        PlayerMessage::SetRate(r) => PlaybackMessage::SetPlaybackRate(r),
        PlayerMessage::SetVolume(v) => PlaybackMessage::SetMasterVolume(v),
        PlayerMessage::SetQuality(q) => PlaybackMessage::SetPreviewQuality(q),
    }
}
