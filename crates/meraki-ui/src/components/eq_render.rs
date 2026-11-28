use crate::components::eq_canvas::{
    eq_gain_to_y, eq_node_position, eq_y_to_gain, sample_eq_curve_points, EqMessage, EQ_BANDS,
};
use crate::components::resettable_slider::is_double_click;
use crate::i18n;
use crate::theme;
use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas::{self, gradient, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme};
use meraki_core::{AppLanguage, EqBand};
use meraki_engine::SpectrumBands;
use std::time::Instant;

/// Programa `Canvas` responsável pela renderização e arraste/duplo-clique do Equalizador 4 bandas.
pub(crate) struct EqProgram {
    pub gains_db: [f32; 4],
    pub lang: AppLanguage,
    pub spectrum: Option<SpectrumBands>,
    pub is_playing: bool,
}

#[derive(Default)]
pub(crate) struct EqDragState {
    dragging: Option<(EqBand, usize)>,
    hovered: Option<(EqBand, usize)>,
    last_click: Option<(EqBand, Instant)>,
    drag_moved: bool,
}

/// Detecta duplo-clique esquerdo (< 350 ms) na mesma banda do equalizador.
pub fn detect_band_double_click(
    last_click: &mut Option<(EqBand, Instant)>,
    band: EqBand,
    now: Instant,
) -> bool {
    let matched = match *last_click {
        Some((prev_band, prev_at)) if prev_band == band => is_double_click(Some(prev_at), now),
        _ => false,
    };
    if matched {
        *last_click = None;
        true
    } else {
        *last_click = Some((band, now));
        false
    }
}

fn plot_rect(bounds: Rectangle) -> Rectangle {
    let left = 36.0_f32;
    let right = 16.0_f32;
    let top = 16.0_f32;
    let bottom = 36.0_f32;
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
                state.hovered = None;
                if let Some((band, _)) = state.dragging.take() {
                    if state.drag_moved {
                        state.drag_moved = false;
                        let local_y = cursor.position().map_or(bounds.y, |p| p.y - bounds.y);
                        let gain_db = eq_y_to_gain(local_y, plot_rect(bounds));
                        return (canvas::event::Status::Captured, Some(EqMessage::BandReleased { band, gain_db }));
                    }
                    state.drag_moved = false;
                }
            }
            return (canvas::event::Status::Ignored, None);
        };

        let plot = plot_rect(bounds);
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some((band, idx)) = hit_eq_band_circle(local, self.gains_db, plot) {
                    if detect_band_double_click(&mut state.last_click, band, Instant::now()) {
                        state.dragging = None;
                        state.drag_moved = false;
                        return (canvas::event::Status::Captured, Some(EqMessage::BandReset(band)));
                    }
                    state.dragging = Some((band, idx));
                    state.drag_moved = false;
                    return (canvas::event::Status::Captured, None);
                }
                if let Some((band, idx)) = hit_eq_band(local, self.gains_db, plot) {
                    state.last_click = None;
                    state.dragging = Some((band, idx));
                    state.drag_moved = true;
                    let gain_db = eq_y_to_gain(local.y, plot);
                    return (
                        canvas::event::Status::Captured,
                        Some(EqMessage::BandDragged { band, gain_db }),
                    );
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((band, _)) = state.dragging {
                    state.drag_moved = true;
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
                    let was_moved = std::mem::take(&mut state.drag_moved);
                    if was_moved {
                        let gain_db = eq_y_to_gain(local.y, plot);
                        return (
                            canvas::event::Status::Captured,
                            Some(EqMessage::BandReleased { band, gain_db }),
                        );
                    }
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

        if self.is_playing {
            if let Some(spectrum) = self.spectrum {
                draw_spectrum_backdrop(&mut frame, plot, spectrum.bands_db);
            }
        }

        for (db, label) in [(12.0, "+12"), (6.0, "+6"), (0.0, "0 dB"), (-6.0, "-6"), (-12.0, "-12")] {
            let y = eq_gain_to_y(db, plot);
            let is_zero = db.abs() < f32::EPSILON;
            let stroke_color = if is_zero {
                Color { a: 0.45, ..theme::TEXT_SECONDARY }
            } else {
                theme::BORDER_SUBTLE
            };
            let grid_line = Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y));
            frame.stroke(&grid_line, Stroke::default().with_color(stroke_color).with_width(if is_zero { 1.4 } else { 1.0 }));
            frame.fill_text(Text {
                content: label.to_string(),
                position: Point::new(4.0, y - 5.5),
                color: theme::TEXT_SECONDARY,
                size: Pixels(9.0),
                ..Text::default()
            });
        }

        for (idx, (band, _)) in EQ_BANDS.iter().enumerate() {
            let label = i18n::eq_band_label(self.lang, *band);
            let freq = i18n::eq_band_freq_label(*band);
            let pt = eq_node_position(idx, self.gains_db[idx], plot);
            let vline = Path::line(Point::new(pt.x, plot.y), Point::new(pt.x, plot.y + plot.height));
            frame.stroke(&vline, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));
            frame.fill_text(Text {
                content: label.to_string(),
                position: Point::new(pt.x, plot.y + plot.height + 5.0),
                color: theme::TEXT_PRIMARY,
                size: Pixels(9.5),
                horizontal_alignment: Horizontal::Center,
                ..Text::default()
            });
            frame.fill_text(Text {
                content: freq.to_string(),
                position: Point::new(pt.x, plot.y + plot.height + 18.0),
                color: theme::TEXT_SECONDARY,
                size: Pixels(8.5),
                horizontal_alignment: Horizontal::Center,
                ..Text::default()
            });
        }

        draw_eq_curve_and_nodes(&mut frame, plot, self.gains_db, state);
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

fn draw_spectrum_backdrop(frame: &mut Frame, plot: Rectangle, bands_db: [f64; 4]) {
    let bar_w = (plot.width * 0.16).clamp(18.0, 56.0);
    let baseline_y = plot.y + plot.height;
    for (idx, (_, frac)) in EQ_BANDS.iter().enumerate() {
        let energy_db = bands_db[idx];
        if !energy_db.is_finite() {
            continue;
        }
        let norm = ((energy_db + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
        let bar_h = (norm * plot.height).max(2.0);
        let x = plot.x + *frac * plot.width - bar_w * 0.5;
        let y = baseline_y - bar_h;
        let grad = gradient::Linear::new(Point::new(x, y), Point::new(x, baseline_y))
            .add_stop(0.0, Color { a: 0.24, ..theme::ACCENT })
            .add_stop(1.0, Color { a: 0.04, ..theme::ACCENT });
        let rect = Path::rounded_rectangle(Point::new(x, y), Size::new(bar_w, bar_h), 3.0.into());
        frame.fill(&rect, grad);
    }
}

fn draw_eq_curve_and_nodes(frame: &mut Frame, plot: Rectangle, gains_db: [f32; 4], state: &EqDragState) {
    let curve_pts = sample_eq_curve_points(gains_db, plot, 16);
    let baseline_y = plot.y + plot.height;
    let fill_path = Path::new(|b| {
        b.move_to(Point::new(plot.x, baseline_y));
        for pt in &curve_pts {
            b.line_to(*pt);
        }
        b.line_to(Point::new(plot.x + plot.width, baseline_y));
        b.close();
    });
    let grad = gradient::Linear::new(Point::new(plot.x, plot.y), Point::new(plot.x, baseline_y))
        .add_stop(0.0, Color { a: 0.36, ..theme::ACCENT })
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

    for (idx, (band, _)) in EQ_BANDS.iter().enumerate() {
        let center = eq_node_position(idx, gains_db[idx], plot);
        let active = state.dragging.is_some_and(|(b, _)| b == *band)
            || state.hovered.is_some_and(|(b, _)| b == *band);
        if active {
            frame.fill(&Path::circle(center, 11.0), Color { a: 0.28, ..theme::ACCENT_HOVER });
        }
        let dot = Path::circle(center, 6.0);
        frame.fill(&dot, theme::ACCENT);
        frame.stroke(&dot, Stroke::default().with_color(theme::TEXT_PRIMARY).with_width(2.0));
        if active {
            let db = gains_db[idx];
            frame.fill_text(Text {
                content: format!("{db:+.1} dB"),
                position: Point::new(center.x, (center.y - 18.0).max(2.0)),
                color: theme::TEXT_PRIMARY,
                size: Pixels(10.0),
                horizontal_alignment: Horizontal::Center,
                ..Text::default()
            });
        }
    }
}

fn hit_eq_band_circle(point: Point, gains: [f32; 4], plot: Rectangle) -> Option<(EqBand, usize)> {
    for (idx, (band, _)) in EQ_BANDS.iter().enumerate() {
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
    for (idx, (band, _)) in EQ_BANDS.iter().enumerate() {
        let center = eq_node_position(idx, gains[idx], plot);
        let dx = (point.x - center.x).abs();
        if best.is_none_or(|(d, _, _)| dx < d) {
            best = Some((dx, *band, idx));
        }
    }
    best.map(|(_, b, i)| (b, i))
}
