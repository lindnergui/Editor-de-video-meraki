use crate::theme;
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, gradient, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{
    button, column, container, horizontal_space, row, slider, text, toggler,
};
use iced::{Color, Element, Length, Pixels, Point, Rectangle, Renderer, Theme};
use meraki_core::{EqBand, TrackAudioSettings, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB};

const BANDS: [(EqBand, &str, f32); 4] = [
    (EqBand::Low, "Graves", 0.12),
    (EqBand::LowMid, "Médios-Graves", 0.37),
    (EqBand::Mid, "Médios", 0.63),
    (EqBand::High, "Agudos", 0.88),
];

/// Eventos interativos do Equalizador Visual de 4 bandas e melhorias de voz.
#[derive(Debug, Clone)]
pub enum EqMessage {
    BandDragged { band: EqBand, gain_db: f32 },
    BandReleased { band: EqBand, gain_db: f32 },
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
    let frac = BANDS.get(idx).map_or(0.5, |b| b.2);
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
            let pt = catmull_rom(p0, p1, p2, p3, t, plot);
            out.push(pt);
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
pub fn view<'a>(selected_track_name: &'a str, settings: &TrackAudioSettings) -> Element<'a, EqMessage> {
    let header = row![
        text("EQUALIZADOR VISUAL").size(11).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY),
        container(text(selected_track_name).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT))
            .padding([3, 8])
            .style(theme::badge_accent),
        horizontal_space(),
        button(text("Resetar EQ").size(10).font(theme::FONT_SEMIBOLD))
            .padding([4, 9])
            .style(theme::secondary_button)
            .on_press(EqMessage::ResetEq),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let eq_graph = container(Canvas::new(EqProgram { gains_db: settings.eq_gains_db }).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::preview_viewport);

    let nr_pct = (settings.noise_reduction_strength * 100.0).round() as u32;
    let nr_row = row![
        column![
            text("Redução de Ruído").size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(format!("Intensidade: {nr_pct}%")).size(10).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2)
        .width(118),
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
            text("Nivelador de Voz").size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text("Compressor suave para consistência").size(10).color(theme::TEXT_SECONDARY),
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
            text("MELHORIAS").size(10).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY),
            nr_row,
            leveler_row,
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(theme::card);

    container(column![header, eq_graph, enhancements].spacing(10).width(Length::Fill).height(Length::Fill))
        .width(Length::FillPortion(66))
        .height(Length::Fill)
        .padding(12)
        .style(theme::sidebar)
        .into()
}

struct EqProgram {
    gains_db: [f32; 4],
}

#[derive(Default)]
struct EqDragState {
    dragging: Option<(EqBand, usize)>,
    hovered: Option<(EqBand, usize)>,
}

fn plot_rect(bounds: Rectangle) -> Rectangle {
    let left = 34.0_f32;
    let right = 14.0_f32;
    let top = 14.0_f32;
    let bottom = 26.0_f32;
    Rectangle {
        x: left,
        y: top,
        width: (bounds.width - left - right).max(40.0),
        height: (bounds.height - top - bottom).max(40.0),
    }
}

impl canvas::Program<EqMessage> for EqProgram {
    type State = EqDragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<EqMessage>) {
        let Some(local) = cursor.position_in(bounds) else {
            if let canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
                if let Some((band, _)) = state.dragging.take() {
                    let gain_db = eq_y_to_gain(cursor.position().map_or(bounds.y, |p| p.y - bounds.y), plot_rect(bounds));
                    return (canvas::event::Status::Captured, Some(EqMessage::BandReleased { band, gain_db }));
                }
            }
            return (canvas::event::Status::Ignored, None);
        };

        let plot = plot_rect(bounds);
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(hit) = hit_eq_band(local, self.gains_db, plot) {
                    state.dragging = Some(hit);
                    let gain_db = eq_y_to_gain(local.y, plot);
                    return (
                        canvas::event::Status::Captured,
                        Some(EqMessage::BandDragged { band: hit.0, gain_db }),
                    );
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((band, _)) = state.dragging {
                    let gain_db = eq_y_to_gain(local.y, plot);
                    return (
                        canvas::event::Status::Captured,
                        Some(EqMessage::BandDragged { band, gain_db }),
                    );
                }
                state.hovered = hit_eq_band_circle(local, self.gains_db, plot);
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((band, _)) = state.dragging.take() {
                    let gain_db = eq_y_to_gain(local.y, plot);
                    return (
                        canvas::event::Status::Captured,
                        Some(EqMessage::BandReleased { band, gain_db }),
                    );
                }
            }
            _ => {}
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), theme::BG_BASE);
        let plot = plot_rect(bounds);

        for (db, label) in [(12.0, "+12"), (6.0, "+6"), (0.0, "0 dB"), (-6.0, "-6"), (-12.0, "-12")] {
            let y = eq_gain_to_y(db, plot);
            let width = if db.abs() < f32::EPSILON { 1.4 } else { 1.0 };
            let grid_line = Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y));
            frame.stroke(&grid_line, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(width));
            frame.fill_text(Text {
                content: label.to_string(),
                position: Point::new(4.0, y - 6.0),
                color: theme::TEXT_SECONDARY,
                size: Pixels(9.0),
                ..Text::default()
            });
        }

        for (idx, (_, label, _)) in BANDS.iter().enumerate() {
            let pt = eq_node_position(idx, self.gains_db[idx], plot);
            let vline = Path::line(Point::new(pt.x, plot.y), Point::new(pt.x, plot.y + plot.height));
            frame.stroke(&vline, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));
            frame.fill_text(Text {
                content: (*label).to_string(),
                position: Point::new((pt.x - 24.0).max(plot.x), plot.y + plot.height + 6.0),
                color: theme::TEXT_SECONDARY,
                size: Pixels(10.0),
                ..Text::default()
            });
        }

        let curve_pts = sample_eq_curve_points(self.gains_db, plot, 16);
        let baseline_y = plot.y + plot.height;
        let fill_path = Path::new(|b| {
            b.move_to(Point::new(plot.x, baseline_y));
            for pt in &curve_pts {
                b.line_to(*pt);
            }
            b.line_to(Point::new(plot.x + plot.width, baseline_y));
            b.close();
        });
        let grad = gradient::Linear::new(
            Point::new(plot.x, plot.y),
            Point::new(plot.x, baseline_y),
        )
        .add_stop(0.0, Color { a: 0.40, ..theme::ACCENT })
        .add_stop(1.0, Color { a: 0.03, ..theme::ACCENT });
        frame.fill(&fill_path, grad);

        let curve_path = Path::new(|b| {
            if let Some(first) = curve_pts.first() {
                b.move_to(*first);
                for pt in curve_pts.iter().skip(1) {
                    b.line_to(*pt);
                }
            }
        });
        frame.stroke(&curve_path, Stroke::default().with_color(theme::ACCENT).with_width(2.4));

        for (idx, (band, _, _)) in BANDS.iter().enumerate() {
            let center = eq_node_position(idx, self.gains_db[idx], plot);
            let active = state.dragging.is_some_and(|(b, _)| b == *band)
                || state.hovered.is_some_and(|(b, _)| b == *band);
            if active {
                frame.fill(&Path::circle(center, 11.0), Color { a: 0.25, ..theme::ACCENT_HOVER });
            }
            let dot = Path::circle(center, 6.0);
            frame.fill(&dot, theme::ACCENT);
            frame.stroke(&dot, Stroke::default().with_color(theme::TEXT_PRIMARY).with_width(2.0));
            if active {
                let db = self.gains_db[idx];
                frame.fill_text(Text {
                    content: format!("{db:+.1} dB"),
                    position: Point::new(center.x - 16.0, (center.y - 20.0).max(2.0)),
                    color: theme::TEXT_PRIMARY,
                    size: Pixels(10.0),
                    ..Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if let Some(local) = cursor.position_in(bounds) {
            if hit_eq_band_circle(local, self.gains_db, plot_rect(bounds)).is_some() {
                return mouse::Interaction::Grab;
            }
            if plot_rect(bounds).contains(local) {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::default()
    }
}

fn hit_eq_band_circle(point: Point, gains: [f32; 4], plot: Rectangle) -> Option<(EqBand, usize)> {
    for (idx, (band, _, _)) in BANDS.iter().enumerate() {
        let center = eq_node_position(idx, gains[idx], plot);
        if point.distance(center) <= 13.0 {
            return Some((*band, idx));
        }
    }
    None
}

fn hit_eq_band(point: Point, gains: [f32; 4], plot: Rectangle) -> Option<(EqBand, usize)> {
    if let Some(hit) = hit_eq_band_circle(point, gains, plot) {
        return Some(hit);
    }
    if !plot.contains(point) {
        return None;
    }
    let mut best: Option<(f32, EqBand, usize)> = None;
    for (idx, (band, _, _)) in BANDS.iter().enumerate() {
        let center = eq_node_position(idx, gains[idx], plot);
        let dx = (point.x - center.x).abs();
        if best.is_none_or(|(d, _, _)| dx < d) {
            best = Some((dx, *band, idx));
        }
    }
    best.map(|(_, b, i)| (b, i))
}
