use crate::app_delivery::{format_file_size, DeliveryMessage, DeliveryState};
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, progress_bar, row, scrollable, text,
    vertical_rule, Column,
};
use iced::{Background, Border, Color, Element, Length, Theme};
use meraki_core::{AppLanguage, ExportJob, ExportJobId, ExportJobStatus};

/// Painel inferior de largura total (`#181922`): Fila de Tarefas + Exportados Recentemente.
pub fn view<'a>(state: &'a DeliveryState, lang: AppLanguage) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let queue_header = row![
        text(s.delivery_queue_header)
            .size(11)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_SECONDARY)
            .wrapping(text::Wrapping::None),
        horizontal_space(),
        text(i18n::format_queue_counts(lang, state.queue.len(), state.history.len()))
            .size(11)
            .color(theme::TEXT_SECONDARY),
    ]
    .align_y(Vertical::Center);

    let columns_row = row![
        queue_column(state, lang),
        vertical_rule(1).style(theme::divider),
        history_column(state, lang),
    ]
    .spacing(12)
    .width(Length::Fill)
    .height(Length::Fill);

    container(
        column![queue_header, columns_row]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(24))
    .padding([10, 14])
    .style(theme::topbar)
    .into()
}

fn queue_column<'a>(state: &'a DeliveryState, lang: AppLanguage) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let first_queued = state
        .queue
        .iter()
        .find(|job| matches!(job.status, ExportJobStatus::Queued))
        .map(|job| job.id);
    let cancel_target = state.active_job_id.or(first_queued);
    let can_start = state.active_job_id.is_none() && first_queued.is_some();

    let header = row![
        text(s.delivery_task_queue)
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
        button(
            text(s.delivery_start_queue)
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(if can_start { theme::TEXT_ON_ACCENT } else { theme::TEXT_SECONDARY })
                .wrapping(text::Wrapping::None),
        )
        .padding([4, 10])
        .style(theme::primary_button)
        .on_press_maybe(can_start.then_some(DeliveryMessage::StartQueue)),
        button(
            text(s.cancel)
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY)
                .wrapping(text::Wrapping::None),
        )
        .padding([4, 10])
        .style(theme::secondary_button)
        .on_press_maybe(cancel_target.map(DeliveryMessage::CancelJob)),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let list_body: Element<'a, DeliveryMessage> = if state.queue.is_empty() {
        container(text(s.delivery_queue_empty).size(11).color(theme::TEXT_SECONDARY))
            .width(Length::Fill)
            .padding(10)
            .style(theme::card)
            .into()
    } else {
        let mut col = Column::new().spacing(6).width(Length::Fill);
        for job in &state.queue {
            col = col.push(queue_job_card(job, lang));
        }
        col.into()
    };

    column![
        header,
        scrollable(list_body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::dark_scrollable),
    ]
    .spacing(6)
    .width(Length::FillPortion(50))
    .height(Length::Fill)
    .into()
}

fn queue_job_card<'a>(job: &'a ExportJob, lang: AppLanguage) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let file_label = job
        .output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(job.settings.file_name.as_str());

    let (progress, status_text) = match &job.status {
        ExportJobStatus::Running { progress, eta_seconds } => {
            let pct = (progress.clamp(0.0, 1.0) * 100.0).round() as u32;
            (*progress, format!("{pct}% • ETA {}", i18n::format_eta_localized(lang, *eta_seconds)))
        }
        ExportJobStatus::Queued => (0.0, s.delivery_status_queued.to_string()),
        ExportJobStatus::Completed { file_size_bytes } => {
            (1.0, format!("{} ({})", s.delivery_status_completed, format_file_size(*file_size_bytes)))
        }
        ExportJobStatus::Failed(err) => (0.0, format!("{}: {err}", s.delivery_status_failed)),
        ExportJobStatus::Cancelled => (0.0, s.delivery_status_cancelled.to_string()),
    };

    let cancel_id: ExportJobId = job.id;
    let card_col = column![
        row![
            text(file_label).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(format!("• {}", i18n::export_preset_label(lang, job.settings.preset)))
                .size(11)
                .color(theme::TEXT_SECONDARY),
            horizontal_space(),
            text(status_text).size(11).font(theme::FONT_SEMIBOLD).color(theme::ACCENT_HOVER),
            button(text(s.cancel).size(10).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None))
                .padding([2, 6])
                .style(theme::ghost_button)
                .on_press(DeliveryMessage::CancelJob(cancel_id)),
        ]
        .spacing(6)
        .align_y(Vertical::Center),
        progress_bar(0.0..=1.0, progress.clamp(0.0, 1.0))
            .height(6.0)
            .style(queue_bar_style),
    ]
    .spacing(6);

    container(card_col)
        .width(Length::Fill)
        .padding([8, 10])
        .style(theme::card)
        .into()
}

fn history_column<'a>(state: &'a DeliveryState, lang: AppLanguage) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let mut header = row![
        text(s.delivery_recent_exports)
            .size(12)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_PRIMARY),
        horizontal_space(),
    ]
    .align_y(Vertical::Center);

    if !state.history.is_empty() {
        header = header.push(
            button(text(s.clear).size(11).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None))
                .padding([3, 8])
                .style(theme::ghost_button)
                .on_press(DeliveryMessage::ClearHistory),
        );
    }

    let list_body: Element<'a, DeliveryMessage> = if state.history.is_empty() {
        container(text(s.delivery_history_empty).size(11).color(theme::TEXT_SECONDARY))
            .width(Length::Fill)
            .padding(10)
            .style(theme::card)
            .into()
    } else {
        let mut col = Column::new().spacing(6).width(Length::Fill);
        for job in &state.history {
            col = col.push(history_job_card(job, lang));
        }
        col.into()
    };

    column![
        header,
        scrollable(list_body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::dark_scrollable),
    ]
    .spacing(6)
    .width(Length::FillPortion(50))
    .height(Length::Fill)
    .into()
}

fn history_job_card<'a>(job: &'a ExportJob, lang: AppLanguage) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let file_label = job
        .output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(job.settings.file_name.as_str());

    let fmt_label = i18n::export_format_label(lang, job.settings.format).to_string();
    let (detail, badge_label, badge_color, icon) = match &job.status {
        ExportJobStatus::Completed { file_size_bytes } => (
            format_file_size(*file_size_bytes),
            s.delivery_status_completed,
            theme::SUCCESS,
            icons::CHECK_CIRCLE,
        ),
        ExportJobStatus::Failed(err) => (
            truncate_text(err, 44),
            s.delivery_status_failed,
            theme::DANGER,
            icons::ALERT_CIRCLE,
        ),
        ExportJobStatus::Cancelled => (
            fmt_label,
            s.delivery_status_cancelled,
            theme::TEXT_SECONDARY,
            icons::ALERT_CIRCLE,
        ),
        ExportJobStatus::Queued | ExportJobStatus::Running { .. } => (
            fmt_label,
            s.delivery_status_in_queue,
            theme::ACCENT_HOVER,
            icons::EXPORT,
        ),
    };

    let status_pill = container(
        row![
            icons::icon(icon, 11.0, badge_color),
            text(badge_label).size(10).font(theme::FONT_BOLD).color(badge_color).wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .align_y(Vertical::Center),
    )
    .padding([3, 8])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(theme::BG_PANEL)),
        text_color: Some(badge_color),
        border: Border {
            color: badge_color.scale_alpha(0.5),
            width: theme::BORDER_WIDTH,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    });

    container(
        row![
            column![
                text(file_label).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                text(detail).size(10).color(theme::TEXT_SECONDARY),
            ]
            .spacing(2),
            horizontal_space(),
            status_pill,
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(theme::card)
    .into()
}

fn truncate_text(text_in: &str, max_chars: usize) -> String {
    if text_in.chars().count() <= max_chars {
        text_in.to_string()
    } else {
        let head: String = text_in.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{head}...")
    }
}

fn queue_bar_style(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(theme::BG_BASE),
        bar: Background::Color(theme::ACCENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
    }
}
