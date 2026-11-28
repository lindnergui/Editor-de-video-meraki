use crate::app_delivery::{DeliveryMessage, DeliveryState};
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::overlay::menu;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_input,
};
use iced::{Background, Border, Element, Length, Padding, Theme};
use meraki_core::{
    AppLanguage, ExportFormat, ExportFrameRate, ExportQuality, ExportResolution, Project,
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatOpt(ExportFormat, AppLanguage);
impl fmt::Display for FormatOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(i18n::export_format_label(self.1, self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolutionOpt(ExportResolution, AppLanguage);
impl fmt::Display for ResolutionOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(i18n::export_resolution_label(self.1, self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRateOpt(ExportFrameRate);
impl fmt::Display for FrameRateOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QualityOpt(ExportQuality, AppLanguage);
impl fmt::Display for QualityOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(i18n::export_quality_label(self.1, self.0))
    }
}

const FPS_OPTS: [FrameRateOpt; 3] = [
    FrameRateOpt(ExportFrameRate::Fps24),
    FrameRateOpt(ExportFrameRate::Fps30),
    FrameRateOpt(ExportFrameRate::Fps60),
];

/// Painel direito (`280px`, `#181922`): configurações do arquivo de saída e disparo da exportação.
pub fn view<'a>(
    project: &'a Project,
    state: &'a DeliveryState,
    lang: AppLanguage,
) -> Element<'a, DeliveryMessage> {
    let s = i18n::strings(lang);
    let header = text(s.delivery_file_settings_header)
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY)
        .wrapping(text::Wrapping::None);

    let settings = &state.settings;
    let duration_ns = project.timeline.total_duration_ns();
    let audio_only = settings.format.is_audio_only();

    let name_field = labeled_block(
        s.delivery_file_name,
        text_input(s.delivery_file_placeholder, &settings.file_name)
            .on_input(DeliveryMessage::FileNameChanged)
            .size(12)
            .padding([7, 9])
            .style(theme::text_input_style)
            .into(),
    );

    let dir_display = compact_dir_label(&settings.output_dir);
    let dir_row = row![
        container(
            row![
                icons::icon(icons::FOLDER_OPEN, 13.0, theme::TEXT_SECONDARY),
                text(dir_display).size(11).color(theme::TEXT_PRIMARY),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .padding([7, 9])
        .style(theme::card),
        button(
            text(s.delivery_change_dir)
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY)
                .wrapping(text::Wrapping::None),
        )
        .padding([7, 10])
        .style(theme::secondary_button)
        .on_press(DeliveryMessage::PickOutputDirClicked),
    ]
    .spacing(6)
    .align_y(Vertical::Center);
    let folder_field = labeled_block(s.delivery_save_to, dir_row.into());

    let format_opts = [
        FormatOpt(ExportFormat::Mp4H264, lang),
        FormatOpt(ExportFormat::MovProRes, lang),
        FormatOpt(ExportFormat::AudioWav, lang),
        FormatOpt(ExportFormat::AudioMp3, lang),
    ];
    let format_field = labeled_block(
        s.delivery_format,
        styled_pick_list(format_opts, Some(FormatOpt(settings.format, lang)), |opt| {
            DeliveryMessage::FormatChanged(opt.0)
        }),
    );

    let res_opts = [
        ResolutionOpt(ExportResolution::Sd720p, lang),
        ResolutionOpt(ExportResolution::FullHd1080p, lang),
        ResolutionOpt(ExportResolution::Uhd4k, lang),
        ResolutionOpt(ExportResolution::Vertical1080x1920, lang),
        ResolutionOpt(ExportResolution::Square1080x1080, lang),
    ];
    let res_widget: Element<'a, DeliveryMessage> = if audio_only {
        disabled_field_badge(s.delivery_na_audio_only)
    } else {
        styled_pick_list(res_opts, Some(ResolutionOpt(settings.resolution, lang)), |opt| {
            DeliveryMessage::ResolutionChanged(opt.0)
        })
    };
    let resolution_field = labeled_block(s.delivery_resolution, res_widget);

    let fps_widget: Element<'a, DeliveryMessage> = if audio_only {
        disabled_field_badge(s.delivery_na_audio_only)
    } else {
        styled_pick_list(FPS_OPTS, Some(FrameRateOpt(settings.fps)), |opt| {
            DeliveryMessage::FrameRateChanged(opt.0)
        })
    };
    let fps_field = labeled_block(s.delivery_fps, fps_widget);

    let quality_opts = [
        QualityOpt(ExportQuality::Low, lang),
        QualityOpt(ExportQuality::Medium, lang),
        QualityOpt(ExportQuality::High, lang),
        QualityOpt(ExportQuality::Master, lang),
    ];
    let quality_field = labeled_block(
        s.delivery_quality,
        styled_pick_list(quality_opts, Some(QualityOpt(settings.quality, lang)), |opt| {
            DeliveryMessage::QualityChanged(opt.0)
        }),
    );

    let fields_scroll = scrollable(
        column![name_field, folder_field, format_field, resolution_field, fps_field, quality_field]
            .spacing(10)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(theme::dark_scrollable);

    let mut footer = column![].spacing(8).width(Length::Fill);
    if duration_ns == 0 {
        footer = footer.push(notice_box(s.delivery_empty_timeline_warn, theme::WARNING));
    } else if let Some(msg) = &state.notice {
        footer = footer.push(notice_box(msg.as_str(), theme::DANGER));
    }

    let cta_content = row![
        horizontal_space(),
        icons::icon(icons::EXPORT, 15.0, theme::TEXT_ON_ACCENT),
        text(s.delivery_export_video)
            .size(13)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_ON_ACCENT)
            .wrapping(text::Wrapping::None),
        horizontal_space(),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let cta_btn = button(cta_content)
        .width(Length::Fill)
        .padding([10, 14])
        .style(theme::primary_button)
        .on_press_maybe((duration_ns > 0).then_some(DeliveryMessage::ExportClicked));
    footer = footer.push(cta_btn);

    if state.active_job_id.is_some() {
        footer = footer.push(text(s.delivery_running_hint).size(10).color(theme::TEXT_SECONDARY));
    }

    container(
        column![header, fields_scroll, footer]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fixed(280.0))
    .height(Length::Fill)
    .padding(14)
    .style(theme::topbar)
    .into()
}

fn labeled_block<'a>(
    label: &'a str,
    widget: Element<'a, DeliveryMessage>,
) -> Element<'a, DeliveryMessage> {
    column![
        text(label).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_SECONDARY),
        widget,
    ]
    .spacing(5)
    .width(Length::Fill)
    .into()
}

fn disabled_field_badge<'a>(label: &'a str) -> Element<'a, DeliveryMessage> {
    container(text(label).size(11).color(theme::TEXT_SECONDARY))
        .width(Length::Fill)
        .padding([8, 10])
        .style(theme::card)
        .into()
}

fn notice_box<'a>(msg: &'a str, tint: iced::Color) -> Element<'a, DeliveryMessage> {
    container(
        row![icons::icon(icons::ALERT_CIRCLE, 14.0, tint), text(msg).size(11).color(tint)]
            .spacing(6)
            .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([7, 9])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(theme::BG_SURFACE)),
        text_color: Some(tint),
        border: Border {
            color: tint.scale_alpha(0.55),
            width: theme::BORDER_WIDTH,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn compact_dir_label(path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    let count = raw.chars().count();
    if count <= 22 {
        raw
    } else {
        let tail: String = raw.chars().skip(count.saturating_sub(19)).collect();
        format!("...{tail}")
    }
}

fn styled_pick_list<'a, T, const N: usize>(
    options: [T; N],
    selected: Option<T>,
    on_select: impl Fn(T) -> DeliveryMessage + 'a,
) -> Element<'a, DeliveryMessage>
where
    T: ToString + PartialEq + Clone + 'a,
{
    pick_list(options, selected, on_select)
        .width(Length::Fill)
        .padding(Padding::from([7, 10]))
        .text_size(12)
        .style(dark_pick_list_style)
        .menu_style(dark_menu_style)
        .into()
}

fn dark_pick_list_style(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border_color = match status {
        pick_list::Status::Opened => theme::ACCENT,
        pick_list::Status::Hovered => theme::ACCENT_HOVER,
        pick_list::Status::Active => theme::BORDER_SUBTLE,
    };
    pick_list::Style {
        text_color: theme::TEXT_PRIMARY,
        placeholder_color: theme::TEXT_SECONDARY,
        handle_color: theme::TEXT_SECONDARY,
        background: Background::Color(theme::BG_SURFACE),
        border: Border {
            color: border_color,
            width: theme::BORDER_WIDTH,
            radius: theme::RADIUS_CONTROL.into(),
        },
    }
}

fn dark_menu_style(_theme: &Theme) -> menu::Style {
    menu::Style {
        background: Background::Color(theme::BG_PANEL),
        border: Border {
            color: theme::BORDER_SUBTLE,
            width: theme::BORDER_WIDTH,
            radius: theme::RADIUS_CONTROL.into(),
        },
        text_color: theme::TEXT_PRIMARY,
        selected_text_color: theme::TEXT_ON_ACCENT,
        selected_background: Background::Color(theme::ACCENT),
    }
}
