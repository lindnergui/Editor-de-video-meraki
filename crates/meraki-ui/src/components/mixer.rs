use crate::components::tooltip::icon_tooltip;
use crate::playback::PlaybackState;
use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::border;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::tooltip::Position;
use iced::widget::{
    button, column, container, horizontal_rule, row, text, vertical_rule, vertical_slider,
};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use meraki_core::{Track, TrackId, TrackKind, MAX_TRACK_GAIN_DB};

pub const MIXER_MIN_GAIN_DB: f32 = -48.0;
const METER_MIN_DB: f32 = -60.0;
const METER_SEGMENTS: usize = 16;

/// Ações emitidas pelo Mixer vertical de áudio e medidores Master/trilha.
#[derive(Debug, Clone)]
pub enum MixerMessage {
    SelectTrack(TrackId),
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
    TrackGainChanged { track_id: TrackId, gain_db: f32 },
    TrackGainReleased(TrackId),
    MasterVolumeChanged(f32),
}

/// Retorna a cor do segmento do medidor vertical (`verde <-12`, `amarelo -12..-3`, `vermelho >-3 dB`).
pub fn db_meter_segment_color(db: f32) -> Color {
    if db > -3.0 {
        theme::DANGER
    } else if db >= -12.0 {
        theme::WARNING
    } else {
        theme::SUCCESS
    }
}

/// Renderiza o Mixer vertical à direita (`250px`, `#181922`) com trilhas reais + canal `MASTER`.
pub fn view<'a>(
    audio_tracks: &[&'a Track],
    selected_track_id: Option<TrackId>,
    playback: &'a PlaybackState,
) -> Element<'a, MixerMessage> {
    let header = text("MIXER DE ÁUDIO")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let mut strips = row![].spacing(6).width(Length::Fill).height(Length::Fill);
    for track in audio_tracks {
        let level = playback
            .audio_meter
            .tracks
            .iter()
            .find(|l| l.track_id == track.id)
            .map_or((METER_MIN_DB, METER_MIN_DB), |l| (l.peak_db, l.rms_db));
        strips = strips.push(track_strip(
            track,
            selected_track_id == Some(track.id),
            level.0,
            level.1,
        ));
    }

    strips = strips.push(vertical_rule(1).style(theme::divider));
    strips = strips.push(master_strip(playback));

    container(
        column![header, horizontal_rule(1).style(theme::divider), strips]
            .spacing(10)
            .height(Length::Fill),
    )
    .width(250)
    .height(Length::Fill)
    .padding(12)
    .style(theme::sidebar)
    .into()
}

fn track_strip<'a>(
    track: &'a Track,
    selected: bool,
    peak_db: f32,
    rms_db: f32,
) -> Element<'a, MixerMessage> {
    let id = track.id;
    let short_name = concise_track_label(&track.name, track.kind);
    let label_btn = button(
        container(
            text(short_name)
                .size(10)
                .font(theme::FONT_SEMIBOLD)
                .color(if selected { theme::TEXT_ON_ACCENT } else { theme::TEXT_PRIMARY }),
        )
        .width(Length::Fill)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .padding([4, 3])
    .style(theme::filter_pill_button(selected))
    .on_press(MixerMessage::SelectTrack(id));

    let mute_btn = icon_tooltip(
        button(text("M").size(9).font(theme::FONT_BOLD))
            .padding([3, 5])
            .style(theme::mixer_toggle_button(track.audio.muted, theme::DANGER))
            .on_press(MixerMessage::ToggleMute(id)),
        "Silenciar trilha (Mute)",
        Position::Top,
    );
    let solo_btn = icon_tooltip(
        button(text("S").size(9).font(theme::FONT_BOLD))
            .padding([3, 5])
            .style(theme::mixer_toggle_button(track.audio.solo, theme::WARNING))
            .on_press(MixerMessage::ToggleSolo(id)),
        "Ouvir apenas esta trilha (Solo)",
        Position::Top,
    );

    let fader_value = track.audio.gain_db.clamp(MIXER_MIN_GAIN_DB, MAX_TRACK_GAIN_DB);
    let fader = vertical_slider(MIXER_MIN_GAIN_DB..=MAX_TRACK_GAIN_DB, fader_value, move |val| {
        MixerMessage::TrackGainChanged { track_id: id, gain_db: val }
    })
    .step(0.5_f32)
    .on_release(MixerMessage::TrackGainReleased(id))
    .width(18)
    .height(Length::Fill)
    .style(theme::preview_slider);

    let meter = Canvas::new(DbMeterProgram { peak_db, rms_db })
        .width(10)
        .height(Length::Fill);

    let fader_area = row![fader, meter]
        .spacing(4)
        .height(Length::Fill)
        .align_y(Vertical::Center);

    container(
        column![
            label_btn,
            row![mute_btn, solo_btn].spacing(3).align_y(Vertical::Center),
            fader_area,
            text(format_db_readout(fader_value))
                .size(9)
                .font(theme::FONT_MEDIUM)
                .color(theme::TEXT_SECONDARY),
        ]
        .spacing(6)
        .align_x(Horizontal::Center)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([6, 4])
    .style(theme::card)
    .into()
}

fn master_strip<'a>(playback: &'a PlaybackState) -> Element<'a, MixerMessage> {
    let fader = vertical_slider(0.0_f32..=1.0_f32, playback.master_volume, MixerMessage::MasterVolumeChanged)
        .step(0.02_f32)
        .width(18)
        .height(Length::Fill)
        .style(theme::preview_slider);

    let meter = Canvas::new(DbMeterProgram {
        peak_db: playback.audio_meter.master_peak_db,
        rms_db: playback.audio_meter.master_rms_db,
    })
    .width(12)
    .height(Length::Fill);

    let readout = if playback.audio_meter.master_peak_db <= -59.5 {
        format!("{:.0}%", playback.master_volume * 100.0)
    } else {
        format!("{:.1}dB", playback.audio_meter.master_peak_db)
    };

    container(
        column![
            container(text("MASTER").size(9).font(theme::FONT_BOLD).color(theme::TEXT_ON_ACCENT))
                .width(Length::Fill)
                .padding([4, 3])
                .center_x(Length::Fill)
                .style(theme::badge_accent),
            container(text("OUT").size(9).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY))
                .padding([3, 6])
                .style(theme::badge),
            row![fader, meter].spacing(4).height(Length::Fill).align_y(Vertical::Center),
            text(readout).size(9).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
        ]
        .spacing(6)
        .align_x(Horizontal::Center)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([6, 4])
    .style(theme::card)
    .into()
}

fn concise_track_label(name: &str, kind: TrackKind) -> &str {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return match kind {
            TrackKind::Audio => "A",
            TrackKind::Video => "V",
            TrackKind::Overlay => "T",
        };
    }
    trimmed.split_whitespace().next().unwrap_or(trimmed)
}

fn format_db_readout(db: f32) -> String {
    if db <= MIXER_MIN_GAIN_DB + 0.1 {
        "-∞".to_string()
    } else if db > 0.0 {
        format!("+{db:.0}dB")
    } else {
        format!("{db:.0}dB")
    }
}

struct DbMeterProgram {
    peak_db: f32,
    rms_db: f32,
}

impl canvas::Program<MixerMessage> for DbMeterProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let bg = Path::rounded_rectangle(Point::ORIGIN, bounds.size(), border::Radius::from(3.0));
        frame.fill(&bg, theme::BG_BASE);
        frame.stroke(&bg, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));

        let pad = 2.0_f32;
        let inner_w = (bounds.width - pad * 2.0).max(2.0);
        let inner_h = (bounds.height - pad * 2.0).max(4.0);
        let gap = 1.5_f32;
        let seg_h = ((inner_h - gap * (METER_SEGMENTS as f32 - 1.0)) / METER_SEGMENTS as f32).max(1.5);
        let active_level = self.peak_db.max(self.rms_db);

        for idx in 0..METER_SEGMENTS {
            let frac = (idx as f32 + 0.5) / (METER_SEGMENTS as f32);
            let seg_db = METER_MIN_DB + frac * (0.0 - METER_MIN_DB);
            let base_color = db_meter_segment_color(seg_db);
            let lit = active_level >= seg_db && active_level > METER_MIN_DB + 0.5;
            let color = if lit { base_color } else { Color { a: 0.16, ..base_color } };

            let y = pad + inner_h - (idx as f32 + 1.0) * seg_h - (idx as f32) * gap;
            frame.fill_rectangle(Point::new(pad, y), Size::new(inner_w, seg_h), color);
        }

        vec![frame.into_geometry()]
    }
}
