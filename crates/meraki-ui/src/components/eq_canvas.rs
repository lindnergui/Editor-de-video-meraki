use crate::components::eq_render::EqProgram;
use crate::components::tooltip::icon_tooltip;
use crate::i18n;
use crate::theme;
use iced::alignment::Vertical;
use iced::widget::canvas::Canvas;
use iced::widget::tooltip::Position;
use iced::widget::{
    button, column, container, horizontal_space, row, slider, text, toggler,
};
use iced::{Element, Length, Point, Rectangle};
use meraki_core::{
    AppLanguage, AutoEqTarget, EqBand, TrackAudioSettings, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB,
};
use meraki_engine::SpectrumBands;

pub(crate) const EQ_BANDS: [(EqBand, f32); 4] = [
    (EqBand::Low, 0.12),
    (EqBand::LowMid, 0.37),
    (EqBand::Mid, 0.63),
    (EqBand::High, 0.88),
];

/// Eventos interativos do Equalizador Visual de 4 bandas e melhorias de voz.
#[derive(Debug, Clone)]
pub enum EqMessage {
    BandDragged { band: EqBand, gain_db: f32 },
    BandReleased { band: EqBand, gain_db: f32 },
    BandReset(EqBand),
    AutoAdjust(AutoEqTarget),
    ResetEq,
    NoiseReductionToggled(bool),
    NoiseReductionStrengthChanged(f32),
    NoiseReductionStrengthReleased,
    VoiceLevelerToggled(bool),
}

/// Converte ganho de EQ (`-12..+12` dB) para coordenada vertical no gráfico.
pub fn eq_gain_to_y(gain_db: f32, plot: Rectangle) -> f32 {
    let clamped = if gain_db.is_finite() {
        gain_db.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB)
    } else {
        0.0
    };
    let ratio = (MAX_EQ_GAIN_DB - clamped) / (MAX_EQ_GAIN_DB - MIN_EQ_GAIN_DB);
    plot.y + ratio * plot.height
}

/// Converte coordenada vertical em pixels no gráfico para ganho de EQ (`-12..+12` dB).
pub fn eq_y_to_gain(y: f32, plot: Rectangle) -> f32 {
    if !y.is_finite() || plot.height <= 0.0 {
        return 0.0;
    }
    let ratio = ((y - plot.y) / plot.height).clamp(0.0, 1.0);
    let raw = MAX_EQ_GAIN_DB - ratio * (MAX_EQ_GAIN_DB - MIN_EQ_GAIN_DB);
    ((raw * 10.0).round() / 10.0).clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB)
}

/// Calcula a posição `(x, y)` de um dos 4 nós do equalizador no gráfico.
pub fn eq_node_position(idx: usize, gain_db: f32, plot: Rectangle) -> Point {
    let frac = EQ_BANDS.get(idx).map_or(0.5, |b| b.1);
    Point::new(plot.x + frac * plot.width, eq_gain_to_y(gain_db, plot))
}

/// Amostra uma curva suave Catmull-Rom passando exatamente pelos 4 nós das bandas de EQ.
pub fn sample_eq_curve_points(gains: [f32; 4], plot: Rectangle, segments_per_span: usize) -> Vec<Point> {
    let steps = segments_per_span.max(4);
    let nodes: [Point; 4] = [
        eq_node_position(0, gains[0], plot),
        eq_node_position(1, gains[1], plot),
        eq_node_position(2, gains[2], plot),
        eq_node_position(3, gains[3], plot),
    ];
    let left_edge = Point::new(plot.x, nodes[0].y);
    let right_edge = Point::new(plot.x + plot.width, nodes[3].y);
    let ctrl = [left_edge, left_edge, nodes[0], nodes[1], nodes[2], nodes[3], right_edge, right_edge];

    let mut out = Vec::with_capacity(steps * 5 + 1);
    for span in 1..ctrl.len().saturating_sub(2) {
        let (p0, p1, p2, p3) = (ctrl[span - 1], ctrl[span], ctrl[span + 1], ctrl[span + 2]);
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            out.push(catmull_rom(p0, p1, p2, p3, t, plot));
        }
    }
    out.push(right_edge);
    out
}

fn catmull_rom(p0: Point, p1: Point, p2: Point, p3: Point, t: f32, plot: Rectangle) -> Point {
    let (t2, t3) = (t * t, t * t * t);
    let interp = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    Point::new(
        interp(p0.x, p1.x, p2.x, p3.x).clamp(plot.x, plot.x + plot.width),
        interp(p0.y, p1.y, p2.y, p3.y).clamp(plot.y, plot.y + plot.height),
    )
}

/// Painel centro-direita (`#181922`) com Equalizador Visual interativo (`Canvas`) e seção Melhorias.
pub fn view<'a>(
    selected_track_name: &'a str,
    settings: &TrackAudioSettings,
    spectrum: Option<SpectrumBands>,
    is_playing: bool,
    auto_eq_notice: Option<&'a str>,
    lang: AppLanguage,
) -> Element<'a, EqMessage> {
    let s = i18n::strings(lang);
    let auto_voice_btn = icon_tooltip(
        button(
            text(s.audio_auto_voice)
                .size(10)
                .font(theme::FONT_SEMIBOLD)
                .wrapping(text::Wrapping::None),
        )
        .padding([4, 8])
        .style(theme::secondary_button)
        .on_press(EqMessage::AutoAdjust(AutoEqTarget::SpeechYouTube)),
        s.audio_auto_voice_tip,
        Position::Top,
    );
    let auto_music_btn = icon_tooltip(
        button(
            text(s.audio_auto_music)
                .size(10)
                .font(theme::FONT_SEMIBOLD)
                .wrapping(text::Wrapping::None),
        )
        .padding([4, 8])
        .style(theme::secondary_button)
        .on_press(EqMessage::AutoAdjust(AutoEqTarget::Music)),
        s.audio_auto_music_tip,
        Position::Top,
    );
    let reset_eq_btn = icon_tooltip(
        button(
            text(s.audio_reset_eq)
                .size(10)
                .font(theme::FONT_SEMIBOLD)
                .wrapping(text::Wrapping::None),
        )
        .padding([4, 9])
        .style(theme::secondary_button)
        .on_press(EqMessage::ResetEq),
        s.audio_reset_eq_tip,
        Position::Top,
    );

    let header = row![
        text(s.audio_eq_header)
            .size(11)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_SECONDARY)
            .wrapping(text::Wrapping::None),
        container(text(selected_track_name).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT))
            .padding([3, 8])
            .style(theme::badge_accent),
        horizontal_space(),
        auto_voice_btn,
        auto_music_btn,
        reset_eq_btn,
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let eq_graph = icon_tooltip(
        container(
            Canvas::new(EqProgram {
                gains_db: settings.eq_gains_db,
                lang,
                spectrum,
                is_playing,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::preview_viewport),
        s.audio_eq_tip,
        Position::Top,
    );

    let nr_pct = (settings.noise_reduction_strength * 100.0).round() as u32;
    let nr_row = row![
        column![
            text(s.audio_noise_reduction).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(format!("{}: {nr_pct}%", s.fx_intensity)).size(10).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2)
        .width(126),
        slider(0.0_f32..=1.0_f32, settings.noise_reduction_strength, EqMessage::NoiseReductionStrengthChanged)
            .step(0.05_f32)
            .on_release(EqMessage::NoiseReductionStrengthReleased)
            .width(Length::Fill)
            .style(theme::preview_slider),
        toggler(settings.noise_reduction_enabled)
            .on_toggle(EqMessage::NoiseReductionToggled)
            .size(18.0)
            .style(theme::accent_toggler),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let leveler_row = row![
        column![
            text(s.audio_voice_leveler).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(s.audio_leveler_desc).size(10).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2),
        horizontal_space(),
        toggler(settings.voice_leveler_enabled)
            .on_toggle(EqMessage::VoiceLevelerToggled)
            .size(18.0)
            .style(theme::accent_toggler),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let enhancements = container(
        column![
            text(s.audio_enhancements_title).size(10).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY),
            nr_row,
            leveler_row,
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card);

    let mut content = column![header].spacing(8).width(Length::Fill).height(Length::Fill);
    if let Some(notice) = auto_eq_notice {
        content = content.push(
            container(text(notice).size(10).color(theme::WARNING))
                .width(Length::Fill)
                .padding([5, 8])
                .style(theme::card),
        );
    }
    content = content.push(eq_graph).push(enhancements);

    container(content)
        .width(Length::FillPortion(66))
        .height(Length::Fill)
        .padding(12)
        .style(theme::sidebar)
        .into()
}
