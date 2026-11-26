use crate::components::timeline_canvas::TimelineCanvasMessage;
use crate::components::timeline_math::{
    display_tracks_ordered, format_ruler_label, ruler_step_secs, time_ns_to_x, x_to_time_ns,
    DisplayTrack, CLIP_RADIUS, RULER_HEIGHT, TRACK_HEIGHT, TRACK_STRIDE,
};
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::border;
use iced::widget::canvas::{Frame, Path, Stroke, Text};
use iced::widget::{container, horizontal_rule, row, text, Column};
use iced::{Color, Element, Length, Pixels, Point, Size};
use meraki_core::{Clip, ClipId, Project, TrackKind};

pub(crate) fn build_track_headers<'a>(
    tracks: &[DisplayTrack<'a>],
    highlight_overlay: bool,
) -> Element<'a, TimelineCanvasMessage> {
    let mut col = Column::new().width(Length::Fill);
    col = col.push(
        container(text("TRILHAS").size(10).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY))
            .width(Length::Fill)
            .height(RULER_HEIGHT)
            .center_x(Length::Fill)
            .center_y(RULER_HEIGHT)
            .style(theme::sidebar),
    );
    col = col.push(horizontal_rule(1).style(theme::divider));

    for track in tracks {
        let is_featured = highlight_overlay && track.kind == TrackKind::Overlay;
        let track_icon = match track.kind {
            TrackKind::Video => icons::MEDIA_VIDEO,
            TrackKind::Audio => icons::TAB_AUDIO,
            TrackKind::Overlay => icons::TRACK_TEXT,
        };
        let icon_color = if is_featured { theme::ACCENT_HOVER } else { theme::TEXT_SECONDARY };
        let label_color = if is_featured { theme::ACCENT_HOVER } else { theme::TEXT_PRIMARY };
        let card_style = if is_featured { theme::badge_accent } else { theme::card };
        let (icon_tint, text_tint) = if is_featured {
            (theme::TEXT_ON_ACCENT, theme::TEXT_ON_ACCENT)
        } else {
            (icon_color, label_color)
        };
        let row_box = container(
            row![
                icons::icon(track_icon, 13.0, icon_tint),
                text(track.name).size(11).font(theme::FONT_SEMIBOLD).color(text_tint),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .height(TRACK_HEIGHT)
        .padding([0, 8])
        .center_y(TRACK_HEIGHT)
        .style(card_style);
        col = col.push(row_box).push(container("").height(6));
    }

    container(col)
        .width(72)
        .height(Length::Fill)
        .style(theme::sidebar)
        .into()
}

pub(crate) fn draw_ruler(frame: &mut Frame, width: f32, px: f32, scroll: f32) {
    frame.fill_rectangle(Point::ORIGIN, Size::new(width, RULER_HEIGHT), theme::BG_PANEL);
    let bottom = Path::line(Point::new(0.0, RULER_HEIGHT), Point::new(width, RULER_HEIGHT));
    frame.stroke(&bottom, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));

    let step_secs = ruler_step_secs(px);
    let start_sec = (x_to_time_ns(0.0, px, scroll) / 1_000_000_000 / step_secs) * step_secs;
    let end_sec = (x_to_time_ns(width + 80.0, px, scroll) / 1_000_000_000).saturating_add(step_secs);

    let mut sec = start_sec;
    while sec <= end_sec {
        let x = time_ns_to_x(sec.saturating_mul(1_000_000_000), px, scroll);
        if x >= -2.0 && x <= width + 2.0 {
            let tick = Path::line(Point::new(x, RULER_HEIGHT - 9.0), Point::new(x, RULER_HEIGHT));
            frame.stroke(&tick, Stroke::default().with_color(theme::TEXT_SECONDARY).with_width(1.0));
            frame.fill_text(Text {
                content: format_ruler_label(sec),
                position: Point::new(x + 6.0, 5.0),
                color: theme::TEXT_SECONDARY,
                size: Pixels(10.0),
                ..Text::default()
            });
        }
        sec = sec.saturating_add(step_secs);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_clip_box(
    frame: &mut Frame,
    kind: TrackKind,
    clip: &Clip,
    label: &str,
    selected: bool,
    start_ns: u64,
    duration_ns: u64,
    y: f32,
    bounds_width: f32,
    px: f32,
    scroll: f32,
) {
    let left_x = time_ns_to_x(start_ns, px, scroll);
    let right_x = time_ns_to_x(start_ns.saturating_add(duration_ns), px, scroll).max(left_x + 12.0);
    if right_x < 0.0 || left_x > bounds_width {
        return;
    }

    let top_left = Point::new(left_x, y + 3.0);
    let size = Size::new(right_x - left_x, TRACK_HEIGHT - 6.0);
    let clip_path = Path::rounded_rectangle(top_left, size, border::Radius::from(CLIP_RADIUS));
    frame.fill(&clip_path, theme::BG_SURFACE);

    if kind == TrackKind::Overlay {
        let strip = Path::rounded_rectangle(
            Point::new(left_x + 4.0, top_left.y + 1.0),
            Size::new((size.width - 8.0).max(2.0), 2.5),
            border::Radius::from(1.2),
        );
        frame.fill(&strip, theme::ACCENT);
    }

    let (border_color, border_w) = if selected { (theme::ACCENT, 2.0) } else { (theme::BORDER_SUBTLE, 1.0) };
    frame.stroke(&clip_path, Stroke::default().with_color(border_color).with_width(border_w));

    if kind == TrackKind::Audio {
        draw_waveform_bars(frame, top_left, size);
    }

    // Alças laterais brancas sólidas para aparar nas extremidades do clipe
    let handle_h = (size.height - 10.0).max(8.0);
    let left_handle = Path::rounded_rectangle(
        Point::new(left_x + 2.0, top_left.y + 5.0),
        Size::new(3.0, handle_h),
        border::Radius::from(1.5),
    );
    let right_handle = Path::rounded_rectangle(
        Point::new(right_x - 5.0, top_left.y + 5.0),
        Size::new(3.0, handle_h),
        border::Radius::from(1.5),
    );
    frame.fill(&left_handle, Color::WHITE);
    frame.fill(&right_handle, Color::WHITE);

    if size.width > 34.0 {
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(left_x + 10.0, top_left.y + 6.0),
            color: theme::TEXT_PRIMARY,
            size: Pixels(11.0),
            ..Text::default()
        });
    }

    draw_effect_badges(frame, clip, top_left, size);
    draw_keyframe_markers(frame, clip, start_ns, duration_ns, left_x, right_x, top_left.y + size.height, px, scroll, selected);
}

/// Badge compacto de efeito renderizado dentro de um clipe na timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectBadge {
    pub label: String,
    pub is_accent: bool,
}

fn badge_width_px(label: &str) -> f32 {
    (label.chars().count() as f32) * 5.8 + 10.0
}

/// Seleciona os badges `FX` + etiquetas curtas (ou `+N`) que cabem na largura visual disponível.
pub fn compute_effect_badges(clip: &Clip, available_width: f32) -> Vec<EffectBadge> {
    let active: Vec<&str> = clip
        .effects
        .iter()
        .filter(|e| e.enabled)
        .map(|e| e.short_badge_label())
        .collect();
    if active.is_empty() || available_width < badge_width_px("FX") {
        return Vec::new();
    }

    let gap = 4.0_f32;
    let mut badges = vec![EffectBadge {
        label: "FX".to_string(),
        is_accent: true,
    }];
    let mut used = badge_width_px("FX");

    for (idx, label) in active.iter().enumerate() {
        let w = badge_width_px(label);
        let remaining = active.len() - idx - 1;
        let overflow_reserve = if remaining > 0 {
            gap + badge_width_px(&format!("+{remaining}"))
        } else {
            0.0
        };
        if used + gap + w + overflow_reserve <= available_width {
            badges.push(EffectBadge {
                label: (*label).to_string(),
                is_accent: false,
            });
            used += gap + w;
        } else {
            let hidden = active.len() - idx;
            let overflow_label = format!("+{hidden}");
            if used + gap + badge_width_px(&overflow_label) <= available_width {
                badges.push(EffectBadge {
                    label: overflow_label,
                    is_accent: false,
                });
            }
            break;
        }
    }
    badges
}

fn draw_effect_badges(frame: &mut Frame, clip: &Clip, top_left: Point, size: Size) {
    let avail = size.width - 20.0;
    let badges = compute_effect_badges(clip, avail);
    if badges.is_empty() {
        return;
    }

    let badge_y = top_left.y + 21.0;
    let badge_h = 14.0_f32;
    let mut cursor_x = top_left.x + 10.0;

    for badge in badges {
        let w = badge_width_px(&badge.label);
        let pill = Path::rounded_rectangle(
            Point::new(cursor_x, badge_y),
            Size::new(w, badge_h),
            border::Radius::from(3.5),
        );
        if badge.is_accent {
            frame.fill(&pill, theme::ACCENT);
        } else {
            frame.fill(&pill, theme::BG_PANEL);
            frame.stroke(&pill, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));
        }
        frame.fill_text(Text {
            content: badge.label,
            position: Point::new(cursor_x + 5.0, badge_y + 1.5),
            color: if badge.is_accent { theme::TEXT_ON_ACCENT } else { theme::TEXT_PRIMARY },
            size: Pixels(9.5),
            ..Text::default()
        });
        cursor_x += w + 4.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_keyframe_markers(
    frame: &mut Frame,
    clip: &Clip,
    start_ns: u64,
    duration_ns: u64,
    left_x: f32,
    right_x: f32,
    bottom_y: f32,
    px: f32,
    scroll: f32,
    selected: bool,
) {
    if clip.keyframes.is_empty() {
        return;
    }
    let mut drawn_offsets = Vec::new();
    for kf in &clip.keyframes {
        if kf.time_offset_ns > duration_ns || drawn_offsets.contains(&kf.time_offset_ns) {
            continue;
        }
        drawn_offsets.push(kf.time_offset_ns);

        let raw_x = time_ns_to_x(start_ns.saturating_add(kf.time_offset_ns), px, scroll);
        let min_x = left_x + 7.0;
        let max_x = (right_x - 7.0).max(min_x);
        let cx = raw_x.clamp(min_x, max_x);
        let cy = bottom_y - 7.0;
        let r = 4.0_f32;
        let diamond = Path::new(|b| {
            b.move_to(Point::new(cx, cy - r));
            b.line_to(Point::new(cx + r, cy));
            b.line_to(Point::new(cx, cy + r));
            b.line_to(Point::new(cx - r, cy));
            b.close();
        });
        let (fill, border) = if selected {
            (theme::ACCENT, Color::WHITE)
        } else {
            (Color::WHITE, theme::ACCENT)
        };
        frame.fill(&diamond, fill);
        frame.stroke(&diamond, Stroke::default().with_color(border).with_width(1.0));
    }
}

fn draw_waveform_bars(frame: &mut Frame, top_left: Point, size: Size) {
    let start_x = top_left.x + 9.0;
    let end_x = top_left.x + size.width - 9.0;
    let mid_y = top_left.y + size.height * 0.68;
    let mut x = start_x;
    let mut idx = 0_usize;
    let wave_color = Color { a: 0.50, ..theme::TEXT_PRIMARY };
    while x + 3.0 < end_x {
        let factor = [0.25_f32, 0.55, 0.85, 0.4, 0.7, 0.95, 0.35, 0.65][idx % 8];
        let bar_h = (size.height * 0.38 * factor).max(3.0);
        frame.fill_rectangle(Point::new(x, mid_y - bar_h * 0.5), Size::new(2.0, bar_h), wave_color);
        x += 5.0;
        idx += 1;
    }
}

pub(crate) fn draw_playhead(frame: &mut Frame, playhead_ns: u64, px: f32, scroll: f32, height: f32) {
    let x = time_ns_to_x(playhead_ns, px, scroll);
    if x < -6.0 || x > 10_000.0 {
        return;
    }
    let line = Path::line(Point::new(x, 16.0), Point::new(x, height));
    frame.stroke(&line, Stroke::default().with_color(theme::ACCENT).with_width(2.0));
    let head = Path::rounded_rectangle(Point::new(x - 5.5, 4.0), Size::new(11.0, 14.0), border::Radius::from(4.0));
    frame.fill(&head, theme::ACCENT);
}

pub(crate) struct StaticTimelineSpec<'a> {
    pub project: &'a Project,
    pub overlay_first: bool,
    pub selected_clip_id: Option<ClipId>,
    pub drag_preview: Option<(ClipId, u64, u64)>,
    pub px: f32,
    pub scroll: f32,
    pub bounds_size: Size,
}

pub(crate) fn draw_static_timeline(frame: &mut Frame, spec: StaticTimelineSpec<'_>) {
    frame.fill_rectangle(Point::ORIGIN, spec.bounds_size, theme::BG_BASE);
    draw_ruler(frame, spec.bounds_size.width, spec.px, spec.scroll);

    let tracks = display_tracks_ordered(&spec.project.timeline, spec.overlay_first);
    for (idx, track) in tracks.iter().enumerate() {
        let y = RULER_HEIGHT + (idx as f32) * TRACK_STRIDE;
        if y > spec.bounds_size.height {
            break;
        }
        frame.fill_rectangle(
            Point::new(0.0, y),
            Size::new(spec.bounds_size.width, TRACK_HEIGHT),
            Color { a: 0.35, ..theme::BG_SURFACE },
        );
        let lane_line = Path::line(
            Point::new(0.0, y + TRACK_HEIGHT),
            Point::new(spec.bounds_size.width, y + TRACK_HEIGHT),
        );
        frame.stroke(&lane_line, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));

        for clip in track.clips {
            let (start_ns, duration_ns) = match spec.drag_preview {
                Some((id, s, d)) if id == clip.id => (s, d),
                _ => (clip.timeline_start_ns, clip.duration_ns),
            };
            let label = clip
                .title
                .as_ref()
                .map(|t| t.text.as_str())
                .or_else(|| spec.project.asset(clip.asset_id).map(|a| a.name.as_str()))
                .unwrap_or("Clipe");
            let selected = spec.selected_clip_id == Some(clip.id);
            draw_clip_box(
                frame,
                track.kind,
                clip,
                label,
                selected,
                start_ns,
                duration_ns,
                y,
                spec.bounds_size.width,
                spec.px,
                spec.scroll,
            );
        }
    }
}
