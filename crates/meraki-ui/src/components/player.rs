use crate::components::tooltip::{action_tooltip, icon_tooltip};
use crate::playback::{format_timecode_mm_ss_ff, PlaybackMessage, PlaybackState};
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::tooltip::Position;
use iced::widget::{button, column, container, horizontal_space, image, row, slider, text, Stack};
use iced::{ContentFit, Element, Length};
use meraki_core::{KeymapProfile, PreviewQuality, Project, ShortcutAction};

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

const SPEED_OPTIONS: [(f64, &str); 4] = [(1.0, "1x"), (1.2, "1.2x"), (1.5, "1.5x"), (2.0, "2x")];

/// Formata o badge discreto de taxa quando o shuttle J/K/L estiver ativo em taxa diferente de `1x`.
pub fn format_shuttle_badge(shuttle_rate: f64) -> Option<String> {
    if !shuttle_rate.is_finite() || shuttle_rate == 0.0 || (shuttle_rate - 1.0).abs() < 0.05 {
        return None;
    }
    let rounded = shuttle_rate.round();
    if (shuttle_rate - rounded).abs() < 0.05 {
        Some(format!("{}x", rounded as i64))
    } else {
        Some(format!("{shuttle_rate:.1}x"))
    }
}

fn shuttle_pill<'a, M: 'a>(shuttle_rate: f64, compact: bool) -> Option<Element<'a, M>> {
    let label = format_shuttle_badge(shuttle_rate)?;
    Some(
        container(text(label).size(if compact { 10 } else { 11 }).font(theme::FONT_BOLD).color(theme::TEXT_ON_ACCENT))
            .padding(if compact { [2, 6] } else { [3, 8] })
            .style(theme::badge_accent)
            .into(),
    )
}

fn timecode_text(project: &Project, playback: &PlaybackState) -> String {
    format!(
        "{} / {}",
        format_timecode_mm_ss_ff(playback.playhead_ns, 30),
        format_timecode_mm_ss_ff(project.timeline.total_duration_ns(), 30),
    )
}

/// Renderiza o player central com barra Desfazer/Refazer, visor 16:9 e controles de transporte.
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
    profile: KeymapProfile,
) -> Element<'a, PlayerMessage> {
    view_with_overlay(project, playback, can_undo, can_redo, profile, |m| m, None)
}

/// Renderiza o player central aceitando uma camada interativa (`stack`) sobre o visor de vídeo.
pub fn view_with_overlay<'a, M: Clone + 'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
    profile: KeymapProfile,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
    overlay: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let top_bar = history_row(can_undo, can_redo, profile, map_player, false);
    let viewport = container(build_viewport(playback, overlay))
        .width(Length::Fill).height(Length::Fill)
        .center_x(Length::Fill).center_y(Length::Fill)
        .padding(8).style(theme::preview_viewport);

    let mut time_row = row![text(timecode_text(project, playback)).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY)]
        .spacing(8).align_y(Vertical::Center);
    if let Some(badge) = shuttle_pill(playback.shuttle_rate, false) {
        time_row = time_row.push(badge);
    }

    let (step_back_btn, play_pause_btn, step_forward_btn) =
        transport_buttons(playback.is_playing, profile, map_player, false);

    let mut speed_row = row![].spacing(4).align_y(Vertical::Center);
    for (rate, label) in SPEED_OPTIONS {
        let active = (playback.playback_rate - rate).abs() < 0.05;
        let btn = button(text(label).size(11).font(theme::FONT_MEDIUM))
            .padding([5, 7]).style(theme::filter_pill_button(active))
            .on_press(map_player(PlayerMessage::SetRate(rate)));
        speed_row = speed_row.push(icon_tooltip(btn, "Velocidade de Reprodução", Position::Top));
    }

    let volume_ctrl = row![
        icons::icon(icons::VOLUME, 14.0, theme::TEXT_SECONDARY),
        slider(0.0_f32..=1.0_f32, playback.master_volume, move |v| map_player(PlayerMessage::SetVolume(v)))
            .step(0.05_f32).width(68).style(theme::preview_slider),
    ]
    .spacing(6).align_y(Vertical::Center);

    let quality_row = quality_controls(playback.preview_quality, map_player);
    let transport_bar = container(
        row![time_row, horizontal_space(), step_back_btn, play_pause_btn, step_forward_btn, horizontal_space(), speed_row, volume_ctrl, quality_row]
            .spacing(8).align_y(Vertical::Center),
    )
    .width(Length::Fill).padding([8, 12]).style(theme::panel);

    container(column![top_bar, viewport, transport_bar].spacing(10).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill).height(Length::Fill).padding(12).style(theme::base_canvas).into()
}

/// Renderiza o player em proporção compacta para o topo centro-esquerda da aba Áudio.
pub fn view_compact<'a, M: Clone + 'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    can_undo: bool,
    can_redo: bool,
    profile: KeymapProfile,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
) -> Element<'a, M> {
    let top_bar = history_row(can_undo, can_redo, profile, map_player, true);
    let viewport = container(build_viewport(playback, None))
        .width(Length::Fill).height(Length::Fill)
        .center_x(Length::Fill).center_y(Length::Fill)
        .padding(6).style(theme::preview_viewport);

    let (step_back_btn, play_pause_btn, step_forward_btn) =
        transport_buttons(playback.is_playing, profile, map_player, true);

    let mut row_top = row![
        step_back_btn, play_pause_btn, step_forward_btn, horizontal_space(),
        text(timecode_text(project, playback)).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
    ]
    .spacing(6).align_y(Vertical::Center);
    if let Some(badge) = shuttle_pill(playback.shuttle_rate, true) {
        row_top = row_top.push(badge);
    }

    let is_720 = playback.preview_quality == PreviewQuality::Proxy720p;
    let row_bottom = row![
        icons::icon(icons::VOLUME, 13.0, theme::TEXT_SECONDARY),
        slider(0.0_f32..=1.0_f32, playback.master_volume, move |v| map_player(PlayerMessage::SetVolume(v)))
            .step(0.05_f32).width(Length::Fill).style(theme::preview_slider),
        quality_pill("720p", "Prévia 720p (Mais fluida)", is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Proxy720p))),
        quality_pill("1080p", "Prévia 1080p (Qualidade total)", !is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Full1080p))),
    ]
    .spacing(6).align_y(Vertical::Center);

    let transport_bar = container(column![row_top, row_bottom].spacing(6))
        .width(Length::Fill).padding([8, 10]).style(theme::panel);

    container(column![top_bar, viewport, transport_bar].spacing(8).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill).height(Length::Fill).padding(10).style(theme::base_canvas).into()
}

fn history_row<'a, M: Clone + 'a>(
    can_undo: bool,
    can_redo: bool,
    profile: KeymapProfile,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
    compact: bool,
) -> Element<'a, M> {
    let undo_btn = action_tooltip(
        history_button(icons::UNDO, "Desfazer", can_undo, map_player(PlayerMessage::Undo)),
        "Desfazer", profile, ShortcutAction::Undo, Position::Bottom,
    );
    let redo_btn = action_tooltip(
        history_button(icons::REDO, "Refazer", can_redo, map_player(PlayerMessage::Redo)),
        "Refazer", profile, ShortcutAction::Redo, Position::Bottom,
    );
    let badge = container(text("16:9").size(if compact { 10 } else { 11 }).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY))
        .padding(if compact { [3, 7] } else { [4, 9] }).style(theme::badge);
    row![undo_btn, redo_btn, horizontal_space(), badge].spacing(if compact { 6 } else { 8 }).align_y(Vertical::Center).into()
}

fn transport_buttons<'a, M: Clone + 'a>(
    is_playing: bool,
    profile: KeymapProfile,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
    compact: bool,
) -> (Element<'a, M>, Element<'a, M>, Element<'a, M>) {
    let icon_sz = if compact { 13.0 } else { 14.0 };
    let transport_icon = if is_playing { icons::PAUSE } else { icons::PLAY };

    let step_back_tip = format!(
        "Voltar 1 Quadro ({}) • Recuar ({})",
        profile.shortcut_label(ShortcutAction::StepFrameBack),
        profile.shortcut_label(ShortcutAction::ShuttleReverse),
    );
    let play_tip = format!(
        "Reproduzir / Pausar ({} / {})",
        profile.shortcut_label(ShortcutAction::PlayPause),
        profile.shortcut_label(ShortcutAction::ShuttlePause),
    );
    let step_fwd_tip = format!(
        "Avançar 1 Quadro ({}) • Avançar ({})",
        profile.shortcut_label(ShortcutAction::StepFrameForward),
        profile.shortcut_label(ShortcutAction::ShuttleForward),
    );

    let step_back_btn = icon_tooltip(
        button(icons::icon(icons::STEP_BACK, icon_sz, theme::TEXT_PRIMARY))
            .padding(if compact { [6, 8] } else { [7, 9] }).style(theme::secondary_button)
            .on_press(map_player(PlayerMessage::StepBack)),
        step_back_tip, Position::Top,
    );
    let play_pause_btn = icon_tooltip(
        button(icons::icon(transport_icon, icon_sz, theme::TEXT_ON_ACCENT))
            .padding(if compact { [6, 12] } else { [7, 14] }).style(theme::primary_button)
            .on_press(map_player(PlayerMessage::TogglePlayPause)),
        play_tip, Position::Top,
    );
    let step_forward_btn = icon_tooltip(
        button(icons::icon(icons::STEP_FORWARD, icon_sz, theme::TEXT_PRIMARY))
            .padding(if compact { [6, 8] } else { [7, 9] }).style(theme::secondary_button)
            .on_press(map_player(PlayerMessage::StepForward)),
        step_fwd_tip, Position::Top,
    );
    (step_back_btn, play_pause_btn, step_forward_btn)
}

fn quality_controls<'a, M: Clone + 'a>(
    quality: PreviewQuality,
    map_player: impl Fn(PlayerMessage) -> M + Copy + 'a,
) -> Element<'a, M> {
    let is_720 = quality == PreviewQuality::Proxy720p;
    row![
        quality_pill("720p", "Prévia 720p (Mais fluida)", is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Proxy720p))),
        quality_pill("1080p", "Prévia 1080p (Qualidade total)", !is_720, map_player(PlayerMessage::SetQuality(PreviewQuality::Full1080p))),
    ]
    .spacing(4).align_y(Vertical::Center).into()
}

fn build_viewport<'a, M: Clone + 'a>(
    playback: &'a PlaybackState,
    overlay: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let base: Element<'a, M> = match &playback.current_frame {
        Some(handle) => image(handle.clone()).width(Length::Fill).height(Length::Fill).content_fit(ContentFit::Contain).into(),
        None => container(
            column![
                icons::icon(icons::MEDIA_VIDEO, 28.0, theme::TEXT_SECONDARY),
                text("Adicione um clipe à timeline para começar").size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_SECONDARY),
            ]
            .spacing(8).align_x(Horizontal::Center),
        )
        .width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).into(),
    };
    if let Some(overlay_el) = overlay {
        Stack::with_children([base, overlay_el]).width(Length::Fill).height(Length::Fill).into()
    } else {
        base
    }
}

fn history_button<'a, M: Clone + 'a>(svg_bytes: &'static [u8], label: &'a str, enabled: bool, msg: M) -> Element<'a, M> {
    let tint = if enabled { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    button(row![icons::icon(svg_bytes, 13.0, tint), text(label).size(11).font(theme::FONT_MEDIUM).color(tint)].spacing(5).align_y(Vertical::Center))
        .padding([5, 9]).style(theme::secondary_button).on_press_maybe(enabled.then_some(msg)).into()
}

fn quality_pill<'a, M: Clone + 'a>(label: &'a str, tip: &'a str, active: bool, msg: M) -> Element<'a, M> {
    icon_tooltip(
        button(text(label).size(11).font(theme::FONT_SEMIBOLD)).padding([5, 8]).style(theme::filter_pill_button(active)).on_press(msg),
        tip, Position::Top,
    )
}

/// Converte mensagens da barra do player em mensagens unificadas de reprodução/histórico.
pub fn into_playback_message(msg: PlayerMessage, project: &Project, playhead_ns: u64) -> PlaybackMessage {
    let frame_ns = 1_000_000_000_u64 / 30;
    match msg {
        PlayerMessage::Undo => PlaybackMessage::Undo,
        PlayerMessage::Redo => PlaybackMessage::Redo,
        PlayerMessage::StepBack => PlaybackMessage::Seek(playhead_ns.saturating_sub(frame_ns)),
        PlayerMessage::StepForward => {
            let next = playhead_ns.saturating_add(frame_ns);
            let total = project.timeline.total_duration_ns();
            PlaybackMessage::Seek(if total > 0 { next.min(total) } else { next })
        }
        PlayerMessage::TogglePlayPause => PlaybackMessage::TogglePlayPause,
        PlayerMessage::SetRate(r) => PlaybackMessage::SetPlaybackRate(r),
        PlayerMessage::SetVolume(v) => PlaybackMessage::SetMasterVolume(v),
        PlayerMessage::SetQuality(q) => PlaybackMessage::SetPreviewQuality(q),
    }
}
