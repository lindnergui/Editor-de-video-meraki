use crate::app_delivery::{DeliveryMessage, DeliveryState};
use crate::{icons, theme};
use iced::alignment::Vertical;
use iced::overlay::menu;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_input,
};
use iced::{Background, Border, Element, Length, Padding, Theme};
use meraki_core::{ExportFormat, ExportFrameRate, ExportQuality, ExportResolution, Project};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatOpt(ExportFormat);

impl fmt::Display for FormatOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolutionOpt(ExportResolution);

impl fmt::Display for ResolutionOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
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
struct QualityOpt(ExportQuality);

impl fmt::Display for QualityOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
    }
}

const FORMAT_OPTS: [FormatOpt; 4] = [
    FormatOpt(ExportFormat::Mp4H264),
    FormatOpt(ExportFormat::MovProRes),
    FormatOpt(ExportFormat::AudioWav),
    FormatOpt(ExportFormat::AudioMp3),
];

const RESOLUTION_OPTS: [ResolutionOpt; 5] = [
    ResolutionOpt(ExportResolution::Sd720p),
    ResolutionOpt(ExportResolution::FullHd1080p),
    ResolutionOpt(ExportResolution::Uhd4k),
    ResolutionOpt(ExportResolution::Vertical1080x1920),
    ResolutionOpt(ExportResolution::Square1080x1080),
];

const FPS_OPTS: [FrameRateOpt; 3] = [
    FrameRateOpt(ExportFrameRate::Fps24),
    FrameRateOpt(ExportFrameRate::Fps30),
    FrameRateOpt(ExportFrameRate::Fps60),
];

const QUALITY_OPTS: [QualityOpt; 4] = [
    QualityOpt(ExportQuality::Low),
    QualityOpt(ExportQuality::Medium),
    QualityOpt(ExportQuality::High),
    QualityOpt(ExportQuality::Master),
];

/// Painel direito (`280px`, `#181922`): configurações do arquivo de saída e disparo da exportação.
pub fn view<'a>(project: &'a Project, state: &'a DeliveryState) -> Element<'a, DeliveryMessage> {
    let header = text("CONFIGURAÇÕES DE ARQUIVO")
        .size(11)
        .font(theme::FONT_BOLD)
        .color(theme::TEXT_SECONDARY);

    let settings = &state.settings;
    let duration_ns = project.timeline.total_duration_ns();
    let audio_only = settings.format.is_audio_only();

    let name_field = labeled_block(
        "Nome do Arquivo",
        text_input("meu-video-final.mp4", &settings.file_name)
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
            text("Alterar...")
                .size(11)
                .font(theme::FONT_SEMIBOLD)
                .color(theme::TEXT_PRIMARY),
        )
        .padding([7, 10])
        .style(theme::secondary_button)
        .on_press(DeliveryMessage::PickOutputDirClicked),
    ]
    .spacing(6)
    .align_y(Vertical::Center);
    let folder_field = labeled_block("Salvar Em", dir_row.into());

    let format_field = labeled_block(
        "Formato",
        styled_pick_list(FORMAT_OPTS, Some(FormatOpt(settings.format)), |opt| {
            DeliveryMessage::FormatChanged(opt.0)
        }),
    );

    let res_widget: Element<'a, DeliveryMessage> = if audio_only {
        disabled_field_badge("Não aplicável (Apenas Áudio)")
    } else {
        styled_pick_list(RESOLUTION_OPTS, Some(ResolutionOpt(settings.resolution)), |opt| {
            DeliveryMessage::ResolutionChanged(opt.0)
        })
    };
    let resolution_field = labeled_block("Resolução", res_widget);

    let fps_widget: Element<'a, DeliveryMessage> = if audio_only {
        disabled_field_badge("Não aplicável (Apenas Áudio)")
    } else {
        styled_pick_list(FPS_OPTS, Some(FrameRateOpt(settings.fps)), |opt| {
            DeliveryMessage::FrameRateChanged(opt.0)
        })
    };
    let fps_field = labeled_block("Taxa de Quadros", fps_widget);

    let quality_field = labeled_block(
        "Qualidade",
        styled_pick_list(QUALITY_OPTS, Some(QualityOpt(settings.quality)), |opt| {
            DeliveryMessage::QualityChanged(opt.0)
        }),
    );

    let fields_scroll = scrollable(
        column![
            name_field,
            folder_field,
            format_field,
            resolution_field,
            fps_field,
            quality_field,
        ]
        .spacing(10)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(theme::dark_scrollable);

    let mut footer = column![].spacing(8).width(Length::Fill);
    if duration_ns == 0 {
        footer = footer.push(notice_box(
            "Timeline vazia. Adicione clipes antes de exportar.",
            theme::WARNING,
        ));
    } else if let Some(msg) = &state.notice {
        footer = footer.push(notice_box(msg.as_str(), theme::DANGER));
    }

    let cta_content = row![
        horizontal_space(),
        icons::icon(icons::EXPORT, 15.0, theme::TEXT_ON_ACCENT),
        text("EXPORTAR VÍDEO")
            .size(13)
            .font(theme::FONT_BOLD)
            .color(theme::TEXT_ON_ACCENT),
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
        footer = footer.push(
            text("Exportação em andamento: novos envios entram na fila.")
                .size(10)
                .color(theme::TEXT_SECONDARY),
        );
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
        text(label)
            .size(11)
            .font(theme::FONT_SEMIBOLD)
            .color(theme::TEXT_SECONDARY),
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
        row![
            icons::icon(icons::ALERT_CIRCLE, 14.0, tint),
            text(msg).size(11).color(tint),
        ]
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
