use crate::components::tooltip::icon_tooltip;
use crate::theme;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{overlay, renderer, Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::tooltip::Position;
use iced::widget::{column, container, horizontal_space, row, slider, text};
use iced::{event, Element, Event, Length, Rectangle, Size, Vector};
use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

/// Janela máxima entre dois cliques esquerdos para disparo de reset por duplo-clique.
pub const DOUBLE_CLICK_WINDOW_MS: u64 = 350;

/// Retorna `true` se o intervalo entre `previous` e `now` caracterizar um duplo-clique.
pub fn is_double_click(previous: Option<Instant>, now: Instant) -> bool {
    previous.is_some_and(|prev| {
        now.saturating_duration_since(prev) < Duration::from_millis(DOUBLE_CLICK_WINDOW_MS)
    })
}

/// Parâmetros de construção de um slider numérico com reset por duplo-clique.
pub struct ResettableSliderSpec<'a, Message> {
    pub label: &'a str,
    pub value_label: String,
    pub reset_tooltip: &'a str,
    pub range: RangeInclusive<f32>,
    pub step: f32,
    pub current: f32,
    pub on_change: Box<dyn Fn(f32) -> Message + 'a>,
    pub on_release: Message,
    pub on_reset: Message,
    pub trailing: Option<Element<'a, Message>>,
}

/// Constrói um card compacto de slider onde duplo-clique na barra ou no valor restaura o padrão.
pub fn resettable_slider<'a, Message: Clone + 'a>(
    spec: ResettableSliderSpec<'a, Message>,
) -> Element<'a, Message> {
    let ResettableSliderSpec {
        label,
        value_label,
        reset_tooltip,
        range,
        step,
        current,
        on_change,
        on_release,
        on_reset,
        trailing,
    } = spec;

    let value_target = double_click_target(
        text(value_label)
            .size(11)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::ACCENT_HOVER)
            .wrapping(text::Wrapping::None)
            .into(),
        on_reset.clone(),
    );
    let value_with_tip = icon_tooltip(value_target, reset_tooltip, Position::Top);

    let mut header_row = row![
        text(label)
            .size(12)
            .font(theme::FONT_MEDIUM)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
        value_with_tip,
    ]
    .spacing(4)
    .align_y(Vertical::Center);

    if let Some(extra) = trailing {
        header_row = header_row.push(extra);
    }

    let slider_widget = slider(range, current, on_change)
        .step(step)
        .on_release(on_release)
        .width(Length::Fill)
        .style(theme::preview_slider)
        .into();

    let interactive_slider = icon_tooltip(
        double_click_target(slider_widget, on_reset),
        reset_tooltip,
        Position::Top,
    );

    container(column![header_row, interactive_slider].spacing(6))
        .width(Length::Fill)
        .padding([8, 10])
        .style(theme::card)
        .into()
}

/// Envolve qualquer widget capturando duplo-clique esquerdo antes que o filho consuma o evento.
pub fn double_click_target<'a, Message: Clone + 'a>(
    content: Element<'a, Message>,
    on_double_click: Message,
) -> Element<'a, Message> {
    Element::new(DoubleClickInterceptor {
        content,
        on_double_click,
    })
}

#[derive(Default)]
struct ClickTracker {
    last_click: Option<Instant>,
}

struct DoubleClickInterceptor<'a, Message> {
    content: Element<'a, Message>,
    on_double_click: Message,
}

impl<Message: Clone> Widget<Message, iced::Theme, iced::Renderer>
    for DoubleClickInterceptor<'_, Message>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ClickTracker>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ClickTracker::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Some(child) = tree.children.first_mut() else {
            return layout::Node::new(Size::ZERO);
        };
        self.content.as_widget().layout(child, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(child) = tree.children.first() {
            self.content
                .as_widget()
                .draw(child, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(child) = tree.children.first_mut() {
            self.content
                .as_widget()
                .operate(child, layout, renderer, operation);
        }
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = &event {
            if cursor.is_over(layout.bounds()) {
                let now = Instant::now();
                let tracker = tree.state.downcast_mut::<ClickTracker>();
                if is_double_click(tracker.last_click, now) {
                    tracker.last_click = None;
                    shell.publish(self.on_double_click.clone());
                    return event::Status::Captured;
                }
                tracker.last_click = Some(now);
            }
        }

        let Some(child) = tree.children.first_mut() else {
            return event::Status::Ignored;
        };
        self.content.as_widget_mut().on_event(
            child, event, layout, cursor, renderer, clipboard, shell, viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let Some(child) = tree.children.first() else {
            return mouse::Interaction::None;
        };
        self.content
            .as_widget()
            .mouse_interaction(child, layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let child = tree.children.first_mut()?;
        self.content
            .as_widget_mut()
            .overlay(child, layout, renderer, translation)
    }
}
