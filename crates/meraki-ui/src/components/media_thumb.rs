use crate::app_project_io::ProjectIoState;
use crate::icons;
use crate::theme;
use iced::alignment::Vertical;
use iced::widget::{container, image, progress_bar, stack, text};
use iced::{Background, Border, Color, ContentFit, Element, Length};
use meraki_core::{MediaAsset, MediaKind};

/// Atalho conveniente usando `ProjectIoState` diretamente para renderizar miniatura e status.
pub fn asset_thumbnail<'a, M: 'a>(
    asset: &'a MediaAsset,
    project_io: &'a ProjectIoState,
    height: f32,
    icon_size: f32,
    icon_color: Color,
) -> Element<'a, M> {
    thumbnail_box(
        asset,
        project_io.thumbnails.get(&asset.id),
        project_io.proxy_progress.get(&asset.id).copied(),
        height,
        icon_size,
        icon_color,
    )
}

/// Retorna o ícone SVG apropriado para o tipo do ativo de mídia.
pub fn media_kind_icon(kind: MediaKind) -> &'static [u8] {
    match kind {
        MediaKind::Video => icons::MEDIA_VIDEO,
        MediaKind::Audio => icons::TAB_AUDIO,
        MediaKind::Image => icons::MEDIA_IMAGE,
    }
}

fn proxy_progress_bar_style(_theme: &iced::Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(theme::BG_SURFACE),
        bar: Background::Color(theme::ACCENT),
        border: Border::default(),
    }
}

/// Renderiza miniatura real (ou ícone placeholder) com barra de progresso de proxy e selo `720p`.
pub fn thumbnail_box<'a, M: 'a>(
    asset: &'a MediaAsset,
    thumb: Option<&'a image::Handle>,
    proxy_progress: Option<f32>,
    height: f32,
    icon_size: f32,
    icon_color: Color,
) -> Element<'a, M> {
    let base: Element<'a, M> = if let Some(handle) = thumb {
        container(
            image(handle.clone())
                .width(Length::Fill)
                .height(Length::Fixed(height))
                .content_fit(ContentFit::Cover),
        )
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .style(theme::preview_viewport)
        .into()
    } else {
        container(icons::icon(media_kind_icon(asset.kind), icon_size, icon_color))
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .center_x(Length::Fill)
            .center_y(Length::Fixed(height))
            .style(theme::preview_viewport)
            .into()
    };

    let has_proxy = asset.kind == MediaKind::Video && asset.proxy_path.is_some();
    if !has_proxy && proxy_progress.is_none() {
        return base;
    }

    let mut layers: Vec<Element<'a, M>> = vec![base];

    if has_proxy {
        let badge = container(
            text("720p")
                .size(9)
                .font(theme::FONT_BOLD)
                .color(theme::TEXT_ON_ACCENT),
        )
        .padding([2, 5])
        .style(theme::badge_accent);

        layers.push(
            container(badge)
                .width(Length::Fill)
                .height(Length::Fixed(height))
                .padding(4)
                .align_right(Length::Fill)
                .align_top(Length::Fill)
                .into(),
        );
    } else if let Some(progress) = proxy_progress {
        let bar = progress_bar(0.0..=1.0, progress.clamp(0.0, 1.0))
            .width(Length::Fill)
            .height(Length::Fixed(3.0))
            .style(proxy_progress_bar_style);

        layers.push(
            container(bar)
                .width(Length::Fill)
                .height(Length::Fixed(height))
                .align_y(Vertical::Bottom)
                .into(),
        );
    }

    stack(layers)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .into()
}
