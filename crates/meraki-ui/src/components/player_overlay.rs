use crate::theme;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, LineDash, Path, Stroke, Style, Text};
use iced::{border, Color, Element, Length, Pixels, Point, Rectangle, Renderer, Size, Theme};
use meraki_core::{Clip, ClipId, TitleStyle};

/// Ação emitida quando o usuário solta o arraste sobre o visor do player.
#[derive(Debug, Clone)]
pub enum PlayerOverlayMessage {
    CommitPosition { clip_id: ClipId, pos_x: f32, pos_y: f32 },
}

/// Calcula o retângulo útil 16:9 centralizado dentro do container do visor (`ContentFit::Contain`).
pub fn fitted_video_rect(bounds_size: Size) -> Rectangle {
    if bounds_size.width <= 1.0 || bounds_size.height <= 1.0 {
        return Rectangle::new(Point::ORIGIN, bounds_size);
    }
    let target_ratio = 16.0_f32 / 9.0_f32;
    let current_ratio = bounds_size.width / bounds_size.height;
    let (w, h) = if current_ratio > target_ratio {
        (bounds_size.height * target_ratio, bounds_size.height)
    } else {
        (bounds_size.width, bounds_size.width / target_ratio)
    };
    let x = (bounds_size.width - w) * 0.5;
    let y = (bounds_size.height - h) * 0.5;
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}

/// Calcula a caixa delimitadora tracejada dentro do quadro 16:9 com base em `pos_x`/`pos_y`/`scale`.
pub fn overlay_box_rect(video_rect: Rectangle, clip: &Clip, pos_x: f32, pos_y: f32) -> Rectangle {
    let scale = clip.transform.scale.clamp(0.1, 3.0);
    let (base_norm_y, w_ratio, h_ratio) = match clip.title.as_ref().map(|t| t.style) {
        Some(TitleStyle::QuickCaption) => (0.82_f32, 0.56_f32, 0.14_f32),
        Some(TitleStyle::ModernText | TitleStyle::Title3D) => (0.50_f32, 0.46_f32, 0.18_f32),
        None => (0.50_f32, 0.62_f32, 0.62_f32),
    };
    let box_w = (video_rect.width * w_ratio * scale).clamp(72.0, video_rect.width.max(72.0));
    let box_h = (video_rect.height * h_ratio * scale).clamp(32.0, video_rect.height.max(32.0));
    let cx = video_rect.x + video_rect.width * 0.5 + (pos_x / 1920.0) * video_rect.width;
    let cy = video_rect.y + video_rect.height * base_norm_y + (pos_y / 1080.0) * video_rect.height;
    Rectangle::new(Point::new(cx - box_w * 0.5, cy - box_h * 0.5), Size::new(box_w, box_h))
}

/// Retorna a camada `Canvas` transparente quando há um clipe selecionado na aba Efeitos.
pub fn optional_view<'a>(selected_clip: Option<&'a Clip>) -> Option<Element<'a, PlayerOverlayMessage>> {
    let clip = selected_clip?;
    Some(
        Canvas::new(OverlayProgram { clip })
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

struct OverlayProgram<'a> {
    clip: &'a Clip,
}

#[derive(Default)]
struct OverlayDragState {
    dragging: Option<ActiveOverlayDrag>,
}

struct ActiveOverlayDrag {
    clip_id: ClipId,
    start_cursor: Point,
    initial_x: f32,
    initial_y: f32,
    preview_x: f32,
    preview_y: f32,
}

impl canvas::Program<PlayerOverlayMessage> for OverlayProgram<'_> {
    type State = OverlayDragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<PlayerOverlayMessage>) {
        let canvas::Event::Mouse(mouse_event) = event else {
            return (canvas::event::Status::Ignored, None);
        };
        let video_rect = fitted_video_rect(bounds.size());
        let (cur_x, cur_y) = active_position(state, self.clip);
        let box_rect = overlay_box_rect(video_rect, self.clip, cur_x, cur_y);

        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(local) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                if box_rect.contains(local) {
                    state.dragging = Some(ActiveOverlayDrag {
                        clip_id: self.clip.id,
                        start_cursor: local,
                        initial_x: self.clip.transform.pos_x,
                        initial_y: self.clip.transform.pos_y,
                        preview_x: self.clip.transform.pos_x,
                        preview_y: self.clip.transform.pos_y,
                    });
                    return (canvas::event::Status::Captured, None);
                }
                (canvas::event::Status::Ignored, None)
            }
            mouse::Event::CursorMoved { .. } => {
                let Some(ref mut drag) = state.dragging else {
                    return (canvas::event::Status::Ignored, None);
                };
                let local = cursor.position().map_or(drag.start_cursor, |p| {
                    Point::new(p.x - bounds.x, p.y - bounds.y)
                });
                let dx_px = local.x - drag.start_cursor.x;
                let dy_px = local.y - drag.start_cursor.y;
                let scale_x = 1920.0 / video_rect.width.max(1.0);
                let scale_y = 1080.0 / video_rect.height.max(1.0);
                drag.preview_x = (drag.initial_x + dx_px * scale_x).clamp(-960.0, 960.0).round();
                drag.preview_y = (drag.initial_y + dy_px * scale_y).clamp(-540.0, 540.0).round();
                (canvas::event::Status::Captured, None)
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                let Some(drag) = state.dragging.take() else {
                    return (canvas::event::Status::Ignored, None);
                };
                if (drag.preview_x - drag.initial_x).abs() < 0.5
                    && (drag.preview_y - drag.initial_y).abs() < 0.5
                {
                    return (canvas::event::Status::Captured, None);
                }
                (
                    canvas::event::Status::Captured,
                    Some(PlayerOverlayMessage::CommitPosition {
                        clip_id: drag.clip_id,
                        pos_x: drag.preview_x,
                        pos_y: drag.preview_y,
                    }),
                )
            }
            _ => (canvas::event::Status::Ignored, None),
        }
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
        let video_rect = fitted_video_rect(bounds.size());
        let (pos_x, pos_y) = active_position(state, self.clip);
        let box_rect = overlay_box_rect(video_rect, self.clip, pos_x, pos_y);

        // Prévia textual suave quando o clipe selecionado é de Título (`T1`)
        if let Some(ref title) = self.clip.title {
            let [r, g, b, a] = title.color_rgba;
            let text_color = Color::from_rgba8(r, g, b, f32::from(a) / 255.0);
            let px_size = (f32::from(title.font_size as u16) * (video_rect.height / 1080.0)
                * self.clip.transform.scale)
                .clamp(12.0, 64.0);
            frame.fill_text(Text {
                content: title.text.clone(),
                position: Point::new(box_rect.x + 12.0, box_rect.y + (box_rect.height - px_size) * 0.5),
                color: text_color,
                size: Pixels(px_size),
                ..Text::default()
            });
        }

        let outline = Path::rounded_rectangle(
            box_rect.position(),
            box_rect.size(),
            border::Radius::from(4.0),
        );
        let dashed = Stroke {
            style: Style::Solid(theme::ACCENT),
            width: 1.8,
            line_dash: LineDash { segments: &[6.0, 4.0], offset: 0 },
            ..Stroke::default()
        };
        frame.stroke(&outline, dashed);

        // Alças arredondadas nos cantos e meios das bordas
        let mid_x = box_rect.x + box_rect.width * 0.5;
        let mid_y = box_rect.y + box_rect.height * 0.5;
        let right = box_rect.x + box_rect.width;
        let bottom = box_rect.y + box_rect.height;
        let handles = [
            Point::new(box_rect.x, box_rect.y),
            Point::new(mid_x, box_rect.y),
            Point::new(right, box_rect.y),
            Point::new(right, mid_y),
            Point::new(right, bottom),
            Point::new(mid_x, bottom),
            Point::new(box_rect.x, bottom),
            Point::new(box_rect.x, mid_y),
        ];
        for center in handles {
            let dot = Path::circle(center, 4.0);
            frame.fill(&dot, theme::ACCENT);
            frame.stroke(&dot, Stroke::default().with_color(Color::WHITE).with_width(1.2));
        }

        if state.dragging.is_some() {
            let badge_pos = Point::new(box_rect.x, (box_rect.y - 20.0).max(4.0));
            frame.fill_rectangle(badge_pos, Size::new(86.0, 16.0), theme::BG_PANEL);
            frame.fill_text(Text {
                content: format!("X:{pos_x:.0} Y:{pos_y:.0}"),
                position: Point::new(badge_pos.x + 6.0, badge_pos.y + 2.0),
                color: theme::TEXT_PRIMARY,
                size: Pixels(10.0),
                ..Text::default()
            });
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
        let Some(local) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let video_rect = fitted_video_rect(bounds.size());
        let box_rect = overlay_box_rect(
            video_rect,
            self.clip,
            self.clip.transform.pos_x,
            self.clip.transform.pos_y,
        );
        if box_rect.contains(local) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

fn active_position(state: &OverlayDragState, clip: &Clip) -> (f32, f32) {
    match &state.dragging {
        Some(d) if d.clip_id == clip.id => (d.preview_x, d.preview_y),
        _ => (clip.transform.pos_x, clip.transform.pos_y),
    }
}
