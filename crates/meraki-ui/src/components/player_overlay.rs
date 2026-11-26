use crate::theme;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, LineDash, Path, Stroke, Style};
use iced::{border, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use meraki_core::{
    title_pose, Clip, ClipId, ClipTransform, NormalizedTitleBox, TitleOverlay, TitleStyle,
};

const HANDLE_HIT_RADIUS_PX: f32 = 10.0;
const GUIDE_SNAP_TOLERANCE_PX: f32 = 4.0;

/// Rascunho ativo vindo do inspetor para atualizar o overlay em tempo real durante arrastes.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayLiveDraft {
    pub transform: Option<ClipTransform>,
    pub font_size: Option<u32>,
}

/// Ações emitidas ao mover ou redimensionar a caixa delimitadora sobre o visor do player.
#[derive(Debug, Clone)]
pub enum PlayerOverlayMessage {
    PreviewPosition { clip_id: ClipId, pos_x: f32, pos_y: f32 },
    CommitPosition { clip_id: ClipId, pos_x: f32, pos_y: f32 },
    PreviewScale { clip_id: ClipId, scale: f32 },
    CommitScale { clip_id: ClipId, scale: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CornerHandle {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DragMode {
    Move { initial_x: f32, initial_y: f32, preview_x: f32, preview_y: f32 },
    Resize { initial_scale: f32, initial_radius: f32, center: Point, preview_scale: f32 },
}

/// Calcula o retângulo útil 16:9 centralizado dentro do container do visor (`ContentFit::Contain`).
pub fn fitted_video_rect(bounds_size: Size) -> Rectangle {
    fit_video_rect(bounds_size, 16.0 / 9.0)
}

fn fit_video_rect(bounds_size: Size, target_ratio: f32) -> Rectangle {
    if bounds_size.width <= 1.0 || bounds_size.height <= 1.0 {
        return Rectangle::new(Point::ORIGIN, bounds_size);
    }
    let current_ratio = bounds_size.width / bounds_size.height;
    let (w, h) = if current_ratio > target_ratio {
        (bounds_size.height * target_ratio, bounds_size.height)
    } else {
        (bounds_size.width, bounds_size.width / target_ratio)
    };
    Rectangle::new(Point::new((bounds_size.width - w) * 0.5, (bounds_size.height - h) * 0.5), Size::new(w, h))
}

/// Estima a caixa normalizada `[0, 1]` de um `TitleOverlay` alinhada à mesma âncora de `title_pose` no GES.
pub fn normalized_title_overlay_box(
    title: &TitleOverlay,
    font_size: u32,
    transform: ClipTransform,
) -> NormalizedTitleBox {
    let pose = title_pose(title.style, font_size, transform);
    let lines = title.text.lines().count().max(1) as f64;
    let max_chars = title
        .text
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let bold_factor = if title.style == TitleStyle::Title3D { 1.08 } else { 1.0 };
    let est_w = ((pose.font_size * 0.62 * bold_factor * max_chars + 24.0) / 1920.0).clamp(0.06, 0.96);
    let est_h = ((pose.font_size * 1.40 * lines + 16.0) / 1080.0).clamp(0.04, 0.92);
    let left = (pose.x - est_w * 0.5).clamp(0.0, 1.0 - est_w);
    let top = (pose.y - est_h * 0.5).clamp(0.0, 1.0 - est_h);
    NormalizedTitleBox { x: left, y: top, width: est_w, height: est_h }
}

/// Projeta uma caixa normalizada `[0, 1]` diretamente sobre o retângulo de vídeo exibido no player.
pub fn project_normalized_box(video_rect: Rectangle, norm: NormalizedTitleBox) -> Rectangle {
    Rectangle::new(
        Point::new(video_rect.x + video_rect.width * norm.x as f32, video_rect.y + video_rect.height * norm.y as f32),
        Size::new(video_rect.width * norm.width as f32, video_rect.height * norm.height as f32),
    )
}

/// Resolve a caixa delimitadora exata (alinhada ao GES e sem duplicar texto) para títulos e vídeos.
pub fn resolve_overlay_rect(
    video_rect: Rectangle,
    clip: &Clip,
    live_transform: ClipTransform,
    live_font_size: Option<u32>,
    title_bounds: Option<NormalizedTitleBox>,
) -> Rectangle {
    if let Some(title) = &clip.title {
        let eff_font = live_font_size.unwrap_or(title.font_size);
        let norm = match title_bounds {
            Some(bounds) => {
                let base_pose = title_pose(title.style, title.font_size, clip.transform);
                let live_pose = title_pose(title.style, eff_font, live_transform);
                let ratio = live_pose.font_size / base_pose.font_size.max(1.0);
                let new_w = (bounds.width * ratio).clamp(0.02, 1.0);
                let new_h = (bounds.height * ratio).clamp(0.02, 1.0);
                let cx = bounds.x + bounds.width * 0.5 + (live_pose.x - base_pose.x);
                let cy = bounds.y + bounds.height * 0.5 + (live_pose.y - base_pose.y);
                NormalizedTitleBox {
                    x: (cx - new_w * 0.5).clamp(0.0, 1.0 - new_w),
                    y: (cy - new_h * 0.5).clamp(0.0, 1.0 - new_h),
                    width: new_w,
                    height: new_h,
                }
            }
            None => normalized_title_overlay_box(title, eff_font, live_transform),
        };
        return project_normalized_box(video_rect, norm);
    }

    let scale = live_transform.scale.clamp(0.1, 3.0);
    let box_w = (video_rect.width * 0.62 * scale).clamp(72.0, video_rect.width.max(72.0));
    let box_h = (video_rect.height * 0.62 * scale).clamp(32.0, video_rect.height.max(32.0));
    let cx = video_rect.x + video_rect.width * (0.5 + live_transform.pos_x / 1920.0);
    let cy = video_rect.y + video_rect.height * (0.5 + live_transform.pos_y / 1080.0);
    Rectangle::new(Point::new(cx - box_w * 0.5, cy - box_h * 0.5), Size::new(box_w, box_h))
}

/// Atalho compatível para calcular o retângulo da caixa delimitadora usando apenas `(pos_x, pos_y)`.
pub fn overlay_box_rect(video_rect: Rectangle, clip: &Clip, pos_x: f32, pos_y: f32) -> Rectangle {
    resolve_overlay_rect(
        video_rect,
        clip,
        ClipTransform { pos_x, pos_y, ..clip.transform },
        None,
        None,
    )
}

/// Camada transparente de seleção (moldura + alças + guias de centro) sobre o visor do player.
pub fn optional_view(
    selected_clip: Option<&Clip>,
    title_bounds: Option<NormalizedTitleBox>,
    video_aspect_ratio: f32,
    live_draft: OverlayLiveDraft,
) -> Option<Element<'_, PlayerOverlayMessage>> {
    let clip = selected_clip?;
    Some(
        Canvas::new(OverlayProgram { clip, title_bounds, video_aspect_ratio, live_draft })
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

struct OverlayProgram<'a> {
    clip: &'a Clip,
    title_bounds: Option<NormalizedTitleBox>,
    video_aspect_ratio: f32,
    live_draft: OverlayLiveDraft,
}

impl OverlayProgram<'_> {
    fn active_transform(&self, state: &OverlayDragState) -> ClipTransform {
        let mut tr = self.live_draft.transform.unwrap_or(self.clip.transform);
        if let Some(d) = &state.dragging {
            if d.clip_id == self.clip.id {
                match d.mode {
                    DragMode::Move { preview_x, preview_y, .. } => {
                        tr.pos_x = preview_x;
                        tr.pos_y = preview_y;
                    }
                    DragMode::Resize { preview_scale, .. } => tr.scale = preview_scale,
                }
            }
        }
        tr
    }

    fn box_rect(&self, video_rect: Rectangle, state: &OverlayDragState) -> Rectangle {
        let tr = self.active_transform(state);
        resolve_overlay_rect(video_rect, self.clip, tr, self.live_draft.font_size, self.title_bounds)
    }
}

#[derive(Default)]
struct OverlayDragState {
    dragging: Option<ActiveOverlayDrag>,
}

struct ActiveOverlayDrag {
    clip_id: ClipId,
    start_cursor: Point,
    mode: DragMode,
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
        let video_rect = fit_video_rect(bounds.size(), self.video_aspect_ratio);

        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(local) = cursor.position_in(bounds) else { return (canvas::event::Status::Ignored, None); };
                let box_rect = self.box_rect(video_rect, state);
                let base_tr = self.live_draft.transform.unwrap_or(self.clip.transform);
                let mode = if hit_corner_handle(box_rect, local).is_some() {
                    let center = rect_center(box_rect);
                    DragMode::Resize {
                        initial_scale: base_tr.scale,
                        initial_radius: point_distance(center, local).max(8.0),
                        center,
                        preview_scale: base_tr.scale,
                    }
                } else if box_rect.contains(local) {
                    DragMode::Move {
                        initial_x: base_tr.pos_x,
                        initial_y: base_tr.pos_y,
                        preview_x: base_tr.pos_x,
                        preview_y: base_tr.pos_y,
                    }
                } else {
                    return (canvas::event::Status::Ignored, None);
                };
                state.dragging = Some(ActiveOverlayDrag { clip_id: self.clip.id, start_cursor: local, mode });
                (canvas::event::Status::Captured, None)
            }
            mouse::Event::CursorMoved { .. } => {
                let Some(ref mut drag) = state.dragging else { return (canvas::event::Status::Ignored, None); };
                let local = cursor.position().map_or(drag.start_cursor, |p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let msg = match &mut drag.mode {
            DragMode::Move { initial_x, initial_y, preview_x, preview_y } => {
                let scale_x = 1920.0 / video_rect.width.max(1.0);
                let scale_y = 1080.0 / video_rect.height.max(1.0);
                *preview_x = (*initial_x + (local.x - drag.start_cursor.x) * scale_x).clamp(-960.0, 960.0).round();
                *preview_y = (*initial_y + (local.y - drag.start_cursor.y) * scale_y).clamp(-540.0, 540.0).round();
                PlayerOverlayMessage::PreviewPosition { clip_id: drag.clip_id, pos_x: *preview_x, pos_y: *preview_y }
            }
            DragMode::Resize { initial_scale, initial_radius, center, preview_scale } => {
                let ratio = point_distance(*center, local).max(4.0) / (*initial_radius).max(1.0);
                *preview_scale = (((*initial_scale * ratio) * 20.0).round() / 20.0).clamp(0.1, 3.0);
                PlayerOverlayMessage::PreviewScale { clip_id: drag.clip_id, scale: *preview_scale }
            }
        };
        (canvas::event::Status::Captured, Some(msg))
    }
    mouse::Event::ButtonReleased(mouse::Button::Left) => {
        let Some(drag) = state.dragging.take() else { return (canvas::event::Status::Ignored, None); };
        let msg = match drag.mode {
            DragMode::Move { initial_x, initial_y, preview_x, preview_y } => {
                if (preview_x - initial_x).abs() < 0.5 && (preview_y - initial_y).abs() < 0.5 {
                    return (canvas::event::Status::Captured, None);
                }
                PlayerOverlayMessage::CommitPosition { clip_id: drag.clip_id, pos_x: preview_x, pos_y: preview_y }
            }
            DragMode::Resize { initial_scale, preview_scale, .. } => {
                if (preview_scale - initial_scale).abs() < 0.01 {
                    return (canvas::event::Status::Captured, None);
                }
                PlayerOverlayMessage::CommitScale { clip_id: drag.clip_id, scale: preview_scale }
            }
        };
        (canvas::event::Status::Captured, Some(msg))
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
        let video_rect = fit_video_rect(bounds.size(), self.video_aspect_ratio);
        let box_rect = self.box_rect(video_rect, state);

        draw_center_guides(&mut frame, video_rect, box_rect, state.dragging.is_some());

        let outline = Path::rounded_rectangle(box_rect.position(), box_rect.size(), border::Radius::from(4.0));
        let dashed = Stroke {
            style: Style::Solid(theme::ACCENT),
            width: 1.8,
            line_dash: LineDash { segments: &[6.0, 4.0], offset: 0 },
            ..Stroke::default()
        };
        frame.stroke(&outline, dashed);

        for (_, center) in corner_points(box_rect) {
            let dot = Path::circle(center, 4.5);
            frame.fill(&dot, theme::ACCENT);
            frame.stroke(&dot, Stroke::default().with_color(Color::WHITE).with_width(1.3));
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(drag) = &state.dragging {
            return match drag.mode {
                DragMode::Move { .. } => mouse::Interaction::Grabbing,
                DragMode::Resize { .. } => mouse::Interaction::ResizingDiagonallyDown,
            };
        }
        let Some(local) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let video_rect = fit_video_rect(bounds.size(), self.video_aspect_ratio);
        let box_rect = self.box_rect(video_rect, state);
        match hit_corner_handle(box_rect, local) {
            Some(CornerHandle::TopLeft | CornerHandle::BottomRight) => mouse::Interaction::ResizingDiagonallyDown,
            Some(CornerHandle::TopRight | CornerHandle::BottomLeft) => mouse::Interaction::ResizingDiagonallyUp,
            None if box_rect.contains(local) => mouse::Interaction::Grab,
            None => mouse::Interaction::default(),
        }
    }
}

fn draw_center_guides(frame: &mut Frame, video_rect: Rectangle, box_rect: Rectangle, dragging: bool) {
    let box_center = rect_center(box_rect);
    let video_center = rect_center(video_rect);
    let guide_stroke = Stroke {
        style: Style::Solid(theme::ACCENT.scale_alpha(if dragging { 0.65 } else { 0.35 })),
        width: 1.0,
        line_dash: LineDash { segments: &[4.0, 4.0], offset: 0 },
        ..Stroke::default()
    };
    if (box_center.x - video_center.x).abs() <= GUIDE_SNAP_TOLERANCE_PX {
        let v_line = Path::line(
            Point::new(video_center.x, video_rect.y),
            Point::new(video_center.x, video_rect.y + video_rect.height),
        );
        frame.stroke(&v_line, guide_stroke);
    }
    if (box_center.y - video_center.y).abs() <= GUIDE_SNAP_TOLERANCE_PX {
        let h_line = Path::line(
            Point::new(video_rect.x, video_center.y),
            Point::new(video_rect.x + video_rect.width, video_center.y),
        );
        frame.stroke(&h_line, guide_stroke);
    }
}

fn corner_points(rect: Rectangle) -> [(CornerHandle, Point); 4] {
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;
    [
        (CornerHandle::TopLeft, Point::new(rect.x, rect.y)),
        (CornerHandle::TopRight, Point::new(right, rect.y)),
        (CornerHandle::BottomRight, Point::new(right, bottom)),
        (CornerHandle::BottomLeft, Point::new(rect.x, bottom)),
    ]
}

fn hit_corner_handle(rect: Rectangle, pt: Point) -> Option<CornerHandle> {
    corner_points(rect)
        .into_iter()
        .find(|(_, center)| point_distance(*center, pt) <= HANDLE_HIT_RADIUS_PX)
        .map(|(handle, _)| handle)
}

fn rect_center(rect: Rectangle) -> Point {
    Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

fn point_distance(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}
