use crate::app::Message;
use crate::navigation::WorkspaceTab;
use crate::{icons, theme};
use iced::alignment::Horizontal;
use iced::widget::{button, column, container, text, Column};
use iced::{Element, Length};

/// Barra lateral esquerda fixa com exatamente as 5 abas oficiais de navegação.
pub fn view<'a>(active_tab: WorkspaceTab) -> Element<'a, Message> {
    let items = Column::with_children(WorkspaceTab::ALL.into_iter().map(|tab| {
        let is_active = tab == active_tab;
        let icon_color = if is_active {
            theme::TEXT_ON_ACCENT
        } else {
            theme::TEXT_SECONDARY
        };
        let label_font = if is_active {
            theme::FONT_SEMIBOLD
        } else {
            theme::FONT_MEDIUM
        };

        let content = column![
            icons::icon(tab.icon_bytes(), 20.0, icon_color),
            text(tab.label()).size(11).font(label_font),
        ]
        .align_x(Horizontal::Center)
        .spacing(5)
        .width(Length::Fill);

        button(
            container(content)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(60)
        .padding([8, 4])
        .style(theme::nav_tab_button(is_active))
        .on_press(Message::SelectTab(tab))
        .into()
    }))
    .spacing(6)
    .width(Length::Fill);

    container(items)
        .width(theme::SIDEBAR_WIDTH)
        .height(Length::Fill)
        .padding([12, 8])
        .style(theme::sidebar)
        .into()
}
