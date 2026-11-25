use crate::components::audio_timeline::{
    track_band_layout, volume_db_to_y, AudioTimelineMessage, AudioTrackRow, DraggingVolumeKf,
};
use crate::components::timeline_math::{time_ns_to_x, CLIP_RADIUS, RULER_HEIGHT};
use crate::components::timeline_render::draw_ruler;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::border;
use iced::widget::canvas::{Frame, Path, Stroke, Text};
use iced::widget::{button, column, container, horizontal_rule, row, text, Column};
use iced::{Color, Element, Length, Pixels, Point, Size};
use meraki_core::{Clip, ClipId, Project, TrackId, TrackKind};

pub(crate) fn build_zoom_rail<'a>() -> Element<'a, AudioTimelineMessage> {
    container(
        column![
            button(icons::icon(icons::ZOOM_IN, 14.0, theme::TEXT_PRIMARY))
                .padding(6)
                .style(theme::secondary_button)
                .on_press(AudioTimelineMessage::ZoomIn),
            button(icons::icon(icons::ZOOM_OUT, 14.0, theme::TEXT_PRIMARY))
                .padding(6)
                .style(theme::secondary_button)
                .on_press(AudioTimelineMessage::ZoomOut),
        ]
        .spacing(6)
        .align_x(Horizontal::Center),
    )
    .width(38)
    .height(Length::Fill)
    .padding([8, 4])
    .style(theme::sidebar)
    .into()
}

pub(crate) fn build_track_headers<'a>(
    rows: &[AudioTrackRow<'a>],
    selected_track_id: Option<TrackId>,
) -> Element<'a, AudioTimelineMessage> {
    let mut headers = Column::new().width(Length::Fill);
    headers = headers.push(
        container(text("TRILHAS").size(10).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY))
            .width(Length::Fill)
            .height(RULER_HEIGHT + 4.0)
            .center_x(Length::Fill)
            .center_y(RULER_HEIGHT + 4.0)
            .style(theme::sidebar),
    );
    headers = headers.push(horizontal_rule(1).style(theme::divider));

    for r in rows {
        let is_audio = r.kind == TrackKind::Audio;
        let is_selected = is_audio && r.id.is_some() && r.id == selected_track_id;
        let item: Element<'a, AudioTimelineMessage> = if is_audio {
            let tag = match (r.muted, r.solo) {
                (true, _) => " M",
                (_, true) => " S",
                _ => "",
            };
            let btn = button(
                row![
                    icons::icon(
                        icons::TAB_AUDIO,
                        12.0,
                        if is_selected { theme::TEXT_ON_ACCENT } else { theme::ACCENT },
                    ),
                    text(format!("{}{tag}", r.name))
                        .size(10)
                        .font(theme::FONT_SEMIBOLD)
                        .color(if is_selected { theme::TEXT_ON_ACCENT } else { theme::TEXT_PRIMARY }),
                ]
                .spacing(5)
                .align_y(Vertical::Center),
            )
            .width(Length::Fill)
            .height(r.height)
            .padding([0, 7])
            .style(theme::filter_pill_button(is_selected));
            if let Some(tid) = r.id {
                btn.on_press(AudioTimelineMessage::SelectTrack(tid)).into()
            } else {
                btn.into()
            }
        } else {
            let icon =
                if r.kind == TrackKind::Overlay { icons::TRACK_TEXT } else { icons::MEDIA_VIDEO };
            container(
                row![
                    icons::icon(icon, 11.0, theme::TEXT_SECONDARY),
                    text(r.name).size(10).font(theme::FONT_MEDIUM).color(theme::TEXT_SECONDARY),
                ]
                .spacing(5)
                .align_y(Vertical::Center),
            )
            .width(Length::Fill)
            .height(r.height)
            .padding([0, 7])
            .center_y(r.height)
            .style(theme::card)
            .into()
        };
        headers = headers.push(item).push(container("").height(r.gap));
    }

    container(headers).width(96).height(Length::Fill).style(theme::sidebar).into()
}

#[allow(clippy::too_many_arguments)]
pub(crate) struct StaticAudioSpec<'a> {
    pub project: &'a Project,
    pub selected_clip_id: Option<ClipId>,
    pub active_drag: Option<&'a DraggingVolumeKf>,
    pub size: Size,
    pub px: f32,
    pub scroll: f32,
}

pub(crate) fn draw_static_audio(frame: &mut Frame, spec: StaticAudioSpec<'_>) {
    frame.fill_rectangle(Point::ORIGIN, spec.size, theme::BG_BASE);
    draw_ruler(frame, spec.size.width, spec.px, spec.scroll);
    let rows = track_band_layout(&spec.project.timeline);
    draw_tracks_and_clips(frame, &rows, &spec);
}

pub(crate) fn hit_audio_clip<'a>(
    point: Point,
    rows: &'a [AudioTrackRow<'a>],
    px: f32,
    scroll: f32,
) -> Option<(ClipId, &'a Clip, f32, f32)> {
    for row in rows.iter().filter(|r| r.kind == TrackKind::Audio) {
        let clip_top = row.y + 3.0;
        let clip_h = (row.height - 6.0).max(12.0);
        if point.y < clip_top || point.y > clip_top + clip_h {
            continue;
        }
        for clip in row.clips {
            let left_x = time_ns_to_x(clip.timeline_start_ns, px, scroll);
            let right_x =
                time_ns_to_x(clip.timeline_start_ns.saturating_add(clip.duration_ns), px, scroll)
                    .max(left_x + 12.0);
            if point.x >= left_x && point.x <= right_x {
                return Some((clip.id, clip, clip_top, clip_h));
            }
        }
    }
    None
}

fn draw_tracks_and_clips(frame: &mut Frame, rows: &[AudioTrackRow<'_>], spec: &StaticAudioSpec<'_>) {
    let px = spec.px;
    let scroll = spec.scroll;
    for r in rows {
        if r.y > spec.size.height {
            break;
        }
        let bg_alpha = if r.kind == TrackKind::Audio { 0.40 } else { 0.22 };
        frame.fill_rectangle(
            Point::new(0.0, r.y),
            Size::new(spec.size.width, r.height),
            Color { a: bg_alpha, ..theme::BG_SURFACE },
        );
        for clip in r.clips {
            let left_x = time_ns_to_x(clip.timeline_start_ns, px, scroll);
            let right_x = time_ns_to_x(clip.timeline_start_ns.saturating_add(clip.duration_ns), px, scroll)
                .max(left_x + 12.0);
            if right_x < 0.0 || left_x > spec.size.width {
                continue;
            }
            let clip_top = r.y + 3.0;
            let clip_h = (r.height - 6.0).max(8.0);
            let rect_path = Path::rounded_rectangle(
                Point::new(left_x, clip_top),
                Size::new(right_x - left_x, clip_h),
                border::Radius::from(CLIP_RADIUS),
            );
            frame.fill(&rect_path, theme::BG_SURFACE);
            let selected = spec.selected_clip_id == Some(clip.id);
            let (b_col, b_w) =
                if selected { (theme::ACCENT, 2.0) } else { (theme::BORDER_SUBTLE, 1.0) };
            frame.stroke(&rect_path, Stroke::default().with_color(b_col).with_width(b_w));

            if r.kind == TrackKind::Audio {
                draw_audio_clip_overlay(
                    frame,
                    spec.project,
                    clip,
                    spec.active_drag,
                    left_x,
                    right_x,
                    clip_top,
                    clip_h,
                    px,
                    scroll,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_audio_clip_overlay(
    frame: &mut Frame,
    project: &Project,
    clip: &Clip,
    active_drag: Option<&DraggingVolumeKf>,
    left_x: f32,
    right_x: f32,
    clip_top: f32,
    clip_h: f32,
    px: f32,
    scroll: f32,
) {
    let mut x = left_x + 6.0;
    let mut i = 0_usize;
    let wave_color = Color { a: 0.32, ..theme::TEXT_PRIMARY };
    while x + 2.0 < right_x - 6.0 {
        let amp = [0.3_f32, 0.7, 0.9, 0.45, 0.8, 0.6, 0.95, 0.4][i % 8];
        let bar_h = (clip_h * 0.56 * amp).max(4.0);
        frame.fill_rectangle(
            Point::new(x, clip_top + clip_h * 0.5 - bar_h * 0.5),
            Size::new(2.0, bar_h),
            wave_color,
        );
        x += 5.0;
        i += 1;
    }

    let mut points = clip.volume_keyframes.clone();
    if let Some(drag) = active_drag.filter(|d| d.clip_id == clip.id) {
        if let Some(prev_t) = drag.previous_offset_ns {
            points.retain(|k| k.time_offset_ns != prev_t);
        }
        points.retain(|k| k.time_offset_ns != drag.preview.time_offset_ns);
        points.push(drag.preview);
        points.sort_by_key(|k| k.time_offset_ns);
    }

    let zero_y = volume_db_to_y(0.0, clip_top, clip_h);
    let line_path = Path::new(|b| {
        if points.is_empty() {
            b.move_to(Point::new(left_x + 4.0, zero_y));
            b.line_to(Point::new(right_x - 4.0, zero_y));
        } else {
            let first_y = volume_db_to_y(points[0].gain_db, clip_top, clip_h);
            b.move_to(Point::new(left_x + 2.0, first_y));
            for kf in &points {
                let kx = time_ns_to_x(clip.timeline_start_ns.saturating_add(kf.time_offset_ns), px, scroll)
                    .clamp(left_x, right_x);
                b.line_to(Point::new(kx, volume_db_to_y(kf.gain_db, clip_top, clip_h)));
            }
            let last_y = volume_db_to_y(points[points.len() - 1].gain_db, clip_top, clip_h);
            b.line_to(Point::new(right_x - 2.0, last_y));
        }
    });
    frame.stroke(&line_path, Stroke::default().with_color(theme::ACCENT_HOVER).with_width(2.0));

    for kf in &points {
        let kx = time_ns_to_x(clip.timeline_start_ns.saturating_add(kf.time_offset_ns), px, scroll)
            .clamp(left_x + 4.0, right_x - 4.0);
        let ky = volume_db_to_y(kf.gain_db, clip_top, clip_h);
        let dot = Path::circle(Point::new(kx, ky), 4.5);
        frame.fill(&dot, theme::TEXT_PRIMARY);
        frame.stroke(&dot, Stroke::default().with_color(theme::ACCENT).with_width(1.8));
    }

    if right_x - left_x > 46.0 {
        let label = project.asset(clip.asset_id).map_or("Áudio", |a| a.name.as_str());
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(left_x + 8.0, clip_top + 4.0),
            color: theme::TEXT_PRIMARY,
            size: Pixels(10.0),
            ..Text::default()
        });
    }
}
