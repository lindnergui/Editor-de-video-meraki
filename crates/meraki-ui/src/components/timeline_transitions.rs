use crate::components::timeline_math::{display_tracks_ordered, time_ns_to_x, RULER_HEIGHT, TRACK_HEIGHT, TRACK_STRIDE};
use crate::{i18n, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::border;
use iced::widget::canvas::{gradient, Frame, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Size};
use meraki_core::{AppLanguage, Clip, ClipId, Timeline, TransitionKind, TransitionPlacement};
use uuid::Uuid;

const TRANSITION_ACCENT: Color = theme::ACCENT;
const TRANSITION_SELECTED_BORDER: Color = Color::from_rgb(157.0 / 255.0, 139.0 / 255.0, 248.0 / 255.0);

/// Retângulo e metadados projetados de uma `TimelineTransition` para desenho e hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionVisual {
    pub id: Uuid,
    pub kind: TransitionKind,
    pub placement: TransitionPlacement,
    pub rect: Rectangle,
}

fn effective_span(clip: &Clip, drag_preview: Option<(ClipId, u64, u64)>) -> (u64, u64) {
    match drag_preview {
        Some((id, s, d)) if id == clip.id => (s, d),
        _ => (clip.timeline_start_ns, clip.duration_ns),
    }
}

/// Projeta todas as transições posicionais da timeline em retângulos na tela.
pub fn compute_transition_visuals(
    timeline: &Timeline,
    overlay_first: bool,
    px: f32,
    scroll: f32,
    drag_preview: Option<(ClipId, u64, u64)>,
) -> Vec<TransitionVisual> {
    let tracks = display_tracks_ordered(timeline, overlay_first);
    let mut visuals = Vec::with_capacity(timeline.transitions.len());
    for tr in &timeline.transitions {
        let Some((row_idx, track)) = tracks.iter().enumerate().find(|(_, t)| t.id.is_some_and(|id| id.0 == tr.track_id)) else {
            continue;
        };
        let top = RULER_HEIGHT + (row_idx as f32) * TRACK_STRIDE + 3.0;
        let h = (TRACK_HEIGHT - 6.0).max(8.0);
        let raw_w = ((tr.duration_ns as f64 / 1_000_000_000.0) as f32 * px).max(14.0);
        let find_clip = |cid: Uuid| track.clips.iter().find(|c| c.id == ClipId(cid));

        let rect = match tr.placement {
            TransitionPlacement::BetweenClips { left_clip_id, right_clip_id } => {
                let (Some(left), Some(right)) = (find_clip(left_clip_id), find_clip(right_clip_id)) else {
                    continue;
                };
                let (ls, ld) = effective_span(left, drag_preview);
                let (rs, rd) = effective_span(right, drag_preview);
                let lx = time_ns_to_x(ls, px, scroll);
                let seam_x = time_ns_to_x(ls.saturating_add(ld), px, scroll);
                let rx_end = time_ns_to_x(rs.saturating_add(rd), px, scroll).max(seam_x + 12.0);
                let left_avail = (seam_x - lx).max(8.0);
                let right_avail = (rx_end - seam_x).max(8.0);
                let half_w = (raw_w * 0.5).clamp(8.0, left_avail.min(right_avail));
                Rectangle { x: seam_x - half_w, y: top, width: half_w * 2.0, height: h }
            }
            TransitionPlacement::ClipStart { clip_id } => {
                let Some(clip) = find_clip(clip_id) else { continue; };
                let (s, d) = effective_span(clip, drag_preview);
                let lx = time_ns_to_x(s, px, scroll);
                let rx = time_ns_to_x(s.saturating_add(d), px, scroll).max(lx + 12.0);
                let w = raw_w.min(((rx - lx) * 0.5).max(10.0));
                Rectangle { x: lx, y: top, width: w, height: h }
            }
            TransitionPlacement::ClipEnd { clip_id } => {
                let Some(clip) = find_clip(clip_id) else { continue; };
                let (s, d) = effective_span(clip, drag_preview);
                let lx = time_ns_to_x(s, px, scroll);
                let rx = time_ns_to_x(s.saturating_add(d), px, scroll).max(lx + 12.0);
                let w = raw_w.min(((rx - lx) * 0.5).max(10.0));
                Rectangle { x: rx - w, y: top, width: w, height: h }
            }
        };
        visuals.push(TransitionVisual { id: tr.id, kind: tr.kind, placement: tr.placement, rect });
    }
    visuals
}

/// Retorna o `Uuid` da transição sob `point`, se houver.
pub fn hit_test_transition(
    point: Point,
    timeline: &Timeline,
    overlay_first: bool,
    px: f32,
    scroll: f32,
) -> Option<Uuid> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return None;
    }
    compute_transition_visuals(timeline, overlay_first, px, scroll, None)
        .into_iter()
        .rev()
        .find(|v| v.rect.contains(point))
        .map(|v| v.id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_transitions(
    frame: &mut Frame,
    timeline: &Timeline,
    overlay_first: bool,
    selected_transition_id: Option<Uuid>,
    px: f32,
    scroll: f32,
    bounds_width: f32,
    drag_preview: Option<(ClipId, u64, u64)>,
    lang: AppLanguage,
) {
    let visuals = compute_transition_visuals(timeline, overlay_first, px, scroll, drag_preview);
    for vis in visuals {
        let r = vis.rect;
        if r.x + r.width < 0.0 || r.x > bounds_width {
            continue;
        }
        let selected = selected_transition_id == Some(vis.id);
        let label = i18n::transition_kind_short(lang, vis.kind);
        match vis.placement {
            TransitionPlacement::BetweenClips { .. } => draw_between_pill(frame, r, selected, label),
            TransitionPlacement::ClipStart { .. } => draw_edge_ramp(frame, r, true, selected, label),
            TransitionPlacement::ClipEnd { .. } => draw_edge_ramp(frame, r, false, selected, label),
        }
    }
}

fn draw_between_pill(frame: &mut Frame, r: Rectangle, selected: bool, label: &str) {
    let pill = Path::rounded_rectangle(Point::new(r.x, r.y + 2.0), Size::new(r.width, (r.height - 4.0).max(6.0)), border::Radius::from(5.0));
    let mid_alpha = if selected { 0.72 } else { 0.52 };
    let grad = gradient::Linear::new(Point::new(r.x, r.y), Point::new(r.x + r.width, r.y))
        .add_stop(0.0, Color { a: 0.14, ..TRANSITION_ACCENT })
        .add_stop(0.5, Color { a: mid_alpha, ..TRANSITION_ACCENT })
        .add_stop(1.0, Color { a: 0.14, ..TRANSITION_ACCENT });
    frame.fill(&pill, grad);

    let (b_col, b_w) = if selected {
        (TRANSITION_SELECTED_BORDER, 1.8)
    } else {
        (Color { a: 0.55, ..TRANSITION_ACCENT }, 1.0)
    };
    frame.stroke(&pill, Stroke::default().with_color(b_col).with_width(b_w));

    let seam_x = r.x + r.width * 0.5;
    let seam = Path::line(Point::new(seam_x, r.y + 4.0), Point::new(seam_x, r.y + r.height - 4.0));
    frame.stroke(&seam, Stroke::default().with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.35)).with_width(1.0));

    if r.width >= 30.0 {
        frame.with_clip(r, |clipped| {
            clipped.fill_text(Text {
                content: label.to_string(),
                position: Point::new(r.width * 0.5, r.height * 0.5),
                color: Color::WHITE,
                size: Pixels(9.5),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        });
    }
}

fn draw_edge_ramp(frame: &mut Frame, r: Rectangle, is_start: bool, selected: bool, label: &str) {
    let top = r.y + 1.0;
    let bot = r.y + r.height - 1.0;
    let left = r.x;
    let right = r.x + r.width;
    let (wedge, diag, a0, a1) = if is_start {
        let p = Path::new(|b| {
            b.move_to(Point::new(left, top));
            b.line_to(Point::new(right, top));
            b.line_to(Point::new(left, bot));
            b.close();
        });
        let d = Path::line(Point::new(left, bot), Point::new(right, top));
        (p, d, 0.62, 0.08)
    } else {
        let p = Path::new(|b| {
            b.move_to(Point::new(left, top));
            b.line_to(Point::new(right, top));
            b.line_to(Point::new(right, bot));
            b.close();
        });
        let d = Path::line(Point::new(left, top), Point::new(right, bot));
        (p, d, 0.08, 0.62)
    };

    let grad = gradient::Linear::new(Point::new(left, top), Point::new(right, top))
        .add_stop(0.0, Color { a: a0, ..TRANSITION_ACCENT })
        .add_stop(1.0, Color { a: a1, ..TRANSITION_ACCENT });
    frame.fill(&wedge, grad);

    let (stroke_col, stroke_w) = if selected {
        (TRANSITION_SELECTED_BORDER, 2.0)
    } else {
        (Color { a: 0.72, ..TRANSITION_ACCENT }, 1.2)
    };
    if selected {
        frame.stroke(&wedge, Stroke::default().with_color(stroke_col).with_width(stroke_w));
    } else {
        frame.stroke(&diag, Stroke::default().with_color(stroke_col).with_width(stroke_w));
    }

    if r.width >= 34.0 {
        let text_x = if is_start { 4.0 } else { (r.width - 28.0).max(2.0) };
        frame.with_clip(r, |clipped| {
            clipped.fill_text(Text {
                content: label.to_string(),
                position: Point::new(text_x, 3.0),
                color: Color::WHITE,
                size: Pixels(9.0),
                ..Text::default()
            });
        });
    }
}
