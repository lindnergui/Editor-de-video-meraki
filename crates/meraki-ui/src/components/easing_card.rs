use crate::theme;
use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Theme};
use meraki_core::KeyframeEasing;

const QUICK_COLORS: [[u8; 4]; 6] = [
    [255, 255, 255, 255],
    [255, 238, 128, 255],
    [115, 91, 242, 255],
    [56, 189, 248, 255],
    [248, 113, 113, 255],
    [61, 214, 140, 255],
];

/// Renderiza os dois cartões gráficos de interpolação (`Linear` e `Suave [Ease]`).
pub fn easing_selector<'a, M: Clone + 'a>(
    active: KeyframeEasing,
    on_select: impl Fn(KeyframeEasing) -> M + Copy + 'a,
) -> Element<'a, M> {
    row![
        curve_card("Linear", KeyframeEasing::Linear, active == KeyframeEasing::Linear, on_select),
        curve_card("Suave [Ease]", KeyframeEasing::EaseInOut, active == KeyframeEasing::EaseInOut, on_select),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

fn curve_card<'a, M: Clone + 'a>(
    label: &'a str,
    easing: KeyframeEasing,
    selected: bool,
    on_select: impl Fn(KeyframeEasing) -> M + 'a,
) -> Element<'a, M> {
    let preview = Canvas::new(CurvePreview { easing, selected })
        .width(Length::Fill)
        .height(Length::Fixed(34.0));
    button(
        column![
            preview,
            text(label)
                .size(11)
                .font(if selected { theme::FONT_SEMIBOLD } else { theme::FONT_MEDIUM })
                .color(if selected { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY })
                .align_x(Horizontal::Center)
                .width(Length::Fill),
        ]
        .spacing(4),
    )
    .width(Length::FillPortion(1))
    .padding(8)
    .style(theme::media_card_button(selected))
    .on_press(on_select(easing))
    .into()
}

struct CurvePreview {
    easing: KeyframeEasing,
    selected: bool,
}

impl<M> canvas::Program<M> for CurvePreview {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let pad = 6.0_f32;
        let x0 = pad;
        let y0 = (bounds.height - pad).max(pad);
        let x1 = (bounds.width - pad).max(pad);
        let y1 = pad;

        let baseline = Path::line(Point::new(x0, y0), Point::new(x1, y0));
        frame.stroke(&baseline, Stroke::default().with_color(theme::BORDER_SUBTLE).with_width(1.0));

        let curve = Path::new(|b| {
            b.move_to(Point::new(x0, y0));
            match self.easing {
                KeyframeEasing::Linear => b.line_to(Point::new(x1, y1)),
                KeyframeEasing::EaseInOut => {
                    let mid_x = (x0 + x1) * 0.5;
                    b.bezier_curve_to(Point::new(mid_x, y0), Point::new(mid_x, y1), Point::new(x1, y1));
                }
            }
        });
        let stroke_color = if self.selected { theme::ACCENT_HOVER } else { theme::ACCENT };
        frame.stroke(&curve, Stroke::default().with_color(stroke_color).with_width(2.2));
        frame.fill(&Path::circle(Point::new(x0, y0), 2.8), stroke_color);
        frame.fill(&Path::circle(Point::new(x1, y1), 2.8), stroke_color);
        vec![frame.into_geometry()]
    }
}

/// Linha de amostras rápidas de cor para títulos.
pub fn color_swatch_row<'a, M: Clone + 'a>(
    current_rgba: [u8; 4],
    on_pick: impl Fn([u8; 4]) -> M + 'a,
) -> Element<'a, M> {
    let mut swatch_row = row![].spacing(6);
    for rgba in QUICK_COLORS {
        let active = current_rgba == rgba;
        let color = Color::from_rgba8(rgba[0], rgba[1], rgba[2], f32::from(rgba[3]) / 255.0);
        let msg = on_pick(rgba);
        swatch_row = swatch_row.push(
            button(container("").width(Length::Fill).height(Length::Fill))
                .width(Length::Fixed(22.0))
                .height(Length::Fixed(22.0))
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(color)),
                    border: Border {
                        color: if active { Color::WHITE } else { theme::BORDER_SUBTLE },
                        width: if active { 2.0 } else { 1.0 },
                        radius: theme::RADIUS_CONTROL.into(),
                    },
                    ..button::Style::default()
                })
                .on_press(msg),
        );
    }
    swatch_row.into()
}
