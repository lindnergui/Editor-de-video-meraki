pub use super::files_model::*;

use crate::app_project_io::ProjectIoState;
use crate::components::media_thumb::asset_thumbnail;
use crate::i18n;
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, slider, text,
    text_input, vertical_rule, Column, Row,
};
use iced::{Element, Length};
use meraki_core::{AppLanguage, FolderId, MediaAsset, Project};

/// Renderiza a aba Arquivos em 3 colunas sobre `#111218` (240px | expansível | 300px).
pub fn view<'a>(
    project: &'a Project,
    state: &'a FilesState,
    project_io: &'a ProjectIoState,
    lang: AppLanguage,
) -> Element<'a, FilesMessage> {
    let filtered = filtered_assets(project, state);
    let selected = state.selected_asset.and_then(|id| project.asset(id));
    row![
        left_column(project, state, lang),
        vertical_rule(1).style(theme::divider),
        center_column(&filtered, state, project_io, lang),
        vertical_rule(1).style(theme::divider),
        right_column(selected, state.preview_progress, project_io, lang),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn left_column<'a>(project: &'a Project, state: &'a FilesState, lang: AppLanguage) -> Element<'a, FilesMessage> {
    let s = i18n::strings(lang);
    let header = text(s.files_project_media).size(11).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None);

    let import_btn = button(
        row![
            icons::icon(icons::PLUS, 14.0, theme::TEXT_ON_ACCENT),
            text(s.files_import_label).size(13).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT).wrapping(text::Wrapping::None),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([9, 12])
    .style(theme::primary_button)
    .on_press(FilesMessage::ImportClicked);

    let search = row![
        icons::icon(icons::SEARCH, 14.0, theme::TEXT_SECONDARY),
        text_input(s.files_search_placeholder, &state.search_query)
            .on_input(FilesMessage::SearchChanged)
            .size(12)
            .padding([6, 8])
            .style(theme::text_input_style),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let mut tree = Column::new().spacing(4).width(Length::Fill);
    tree = tree.push(folder_row(s.files_all_files, project.assets.len(), 0, state.selected_folder.is_none(), None));
    for root in project.child_folders(None) {
        tree = push_folder_subtree(tree, project, root.id, 1, state.selected_folder);
    }

    let new_folder_area: Element<'a, FilesMessage> = if state.creating_folder {
        column![
            row![
                text_input(s.files_folder_placeholder, &state.new_folder_name)
                    .on_input(FilesMessage::NewFolderNameChanged)
                    .on_submit(FilesMessage::ConfirmNewFolder)
                    .size(12)
                    .padding([6, 8])
                    .style(theme::text_input_style),
                button(text(s.create).size(12).font(theme::FONT_SEMIBOLD).wrapping(text::Wrapping::None))
                    .padding([6, 10])
                    .style(theme::primary_button)
                    .on_press(FilesMessage::ConfirmNewFolder),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
            button(text(s.cancel).size(11).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None))
                .padding([4, 8])
                .style(theme::ghost_button)
                .on_press(FilesMessage::ToggleNewFolderInput),
        ]
        .spacing(6)
        .into()
    } else {
        button(
            row![
                icons::icon(icons::PLUS, 13.0, theme::TEXT_SECONDARY),
                text(s.files_new_folder).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .padding([7, 10])
        .style(theme::ghost_button)
        .on_press(FilesMessage::ToggleNewFolderInput)
        .into()
    };

    container(
        column![
            header, import_btn, search,
            horizontal_rule(1).style(theme::divider),
            scrollable(tree).height(Length::Fill).style(theme::dark_scrollable),
            horizontal_rule(1).style(theme::divider),
            new_folder_area,
        ]
        .spacing(12)
        .height(Length::Fill),
    )
    .width(240)
    .height(Length::Fill)
    .padding(14)
    .style(theme::sidebar)
    .into()
}

fn push_folder_subtree<'a>(
    mut col: Column<'a, FilesMessage>,
    project: &'a Project,
    folder_id: FolderId,
    depth: usize,
    selected: Option<FolderId>,
) -> Column<'a, FilesMessage> {
    if let Some(folder) = project.folder(folder_id) {
        let count = folder_asset_count(project, folder_id);
        col = col.push(folder_row(folder.name.as_str(), count, depth, selected == Some(folder_id), Some(folder_id)));
        for child in project.child_folders(Some(folder_id)) {
            col = push_folder_subtree(col, project, child.id, depth + 1, selected);
        }
    }
    col
}

fn folder_row<'a>(name: &'a str, count: usize, depth: usize, active: bool, folder_id: Option<FolderId>) -> Element<'a, FilesMessage> {
    let indent = (depth.min(3) * 10) as f32;
    let name_color = if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    let icon_color = if active { theme::ACCENT } else { theme::TEXT_SECONDARY };
    let font = if active { theme::FONT_SEMIBOLD } else { theme::FONT_MEDIUM };
    let content = row![
        container("").width(indent),
        icons::icon(icons::TAB_FILES, 14.0, icon_color),
        text(name).size(12).font(font).color(name_color),
        horizontal_space(),
        container(text(count.to_string()).size(11).color(theme::TEXT_SECONDARY)).padding([2, 6]).style(theme::badge),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    button(content).width(Length::Fill).padding([7, 8]).style(theme::folder_item_button(active)).on_press(FilesMessage::SelectFolder(folder_id)).into()
}

fn center_column<'a>(
    assets: &[&'a MediaAsset],
    state: &FilesState,
    project_io: &'a ProjectIoState,
    lang: AppLanguage,
) -> Element<'a, FilesMessage> {
    let s = i18n::strings(lang);
    let pills = Row::with_children(MediaFilter::ALL.into_iter().map(|filter| {
        let active = state.active_filter == filter;
        let font = if active { theme::FONT_SEMIBOLD } else { theme::FONT_MEDIUM };
        button(text(i18n::media_filter_label(lang, filter)).size(12).font(font).wrapping(text::Wrapping::None))
            .padding([6, 12])
            .style(theme::filter_pill_button(active))
            .on_press(FilesMessage::SelectFilter(filter))
            .into()
    }))
    .spacing(8)
    .align_y(Vertical::Center);

    let filter_bar = row![
        text(s.files_quick_filters).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None),
        pills,
        horizontal_space(),
        text(i18n::format_file_count(lang, assets.len())).size(12).color(theme::TEXT_SECONDARY),
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    let body: Element<'a, FilesMessage> = if assets.is_empty() {
        container(
            column![
                icons::icon(icons::TAB_FILES, 32.0, theme::TEXT_SECONDARY),
                text(s.files_empty_title).size(14).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
                text(s.files_empty_desc).size(12).color(theme::TEXT_SECONDARY),
            ]
            .align_x(Horizontal::Center)
            .spacing(8),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::panel)
        .into()
    } else {
        let cards = assets.iter().map(|a| media_card(a, state.selected_asset == Some(a.id), project_io, lang));
        let grid = container(Row::with_children(cards).spacing(12).wrap()).width(Length::Fill).height(Length::Shrink);
        scrollable(grid).width(Length::Fill).height(Length::Fill).style(theme::dark_scrollable).into()
    };

    container(column![filter_bar, body].spacing(14).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(theme::base_canvas)
        .into()
}

fn media_card<'a>(
    asset: &'a MediaAsset,
    selected: bool,
    project_io: &'a ProjectIoState,
    lang: AppLanguage,
) -> Element<'a, FilesMessage> {
    let icon_color = if selected { theme::ACCENT } else { theme::TEXT_SECONDARY };
    let thumb = asset_thumbnail(asset, project_io, 88.0, 24.0, icon_color);
    let badges = row![
        container(text(i18n::media_kind_label(lang, asset.kind)).size(10).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT).wrapping(text::Wrapping::None))
            .padding([2, 6])
            .style(theme::badge_accent),
        horizontal_space(),
        text(format_duration_mm_ss(asset.metadata.duration_ns)).size(11).color(theme::TEXT_SECONDARY),
    ]
    .align_y(Vertical::Center);

    button(column![thumb, badges, text(asset.name.as_str()).size(12).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY)].spacing(8).height(Length::Shrink))
        .width(Length::Fixed(176.0))
        .height(Length::Fixed(170.0))
        .padding(10)
        .style(theme::media_card_button(selected))
        .on_press(FilesMessage::SelectAsset(asset.id))
        .into()
}

fn right_column<'a>(
    selected: Option<&'a MediaAsset>,
    preview_progress: f32,
    project_io: &'a ProjectIoState,
    lang: AppLanguage,
) -> Element<'a, FilesMessage> {
    let s = i18n::strings(lang);
    let progress = preview_progress.clamp(0.0, 1.0);
    let duration_ns = selected.map_or(0, |a| a.metadata.duration_ns);
    let current_ns = ((duration_ns as f64) * f64::from(progress)).round() as u64;
    let in_ns = selected.map_or(0, |a| a.in_point_ns);
    let out_ns = selected.and_then(|a| a.out_point_ns).unwrap_or(duration_ns);

    let viewport_content: Element<'a, FilesMessage> = match selected {
        Some(asset) => column![
            asset_thumbnail(asset, project_io, 96.0, 28.0, theme::ACCENT),
            text(asset.name.as_str()).size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
            text(format!("{} • {}", i18n::media_kind_label(lang, asset.kind), format_duration_mm_ss(current_ns)))
                .size(11)
                .color(theme::TEXT_SECONDARY),
        ]
        .align_x(Horizontal::Center)
        .spacing(5)
        .into(),
        None => text(s.files_select_media_hint).size(12).color(theme::TEXT_SECONDARY).into(),
    };

    let viewport = container(viewport_content).center_x(Length::Fill).center_y(153).padding(12).style(theme::preview_viewport);
    let scrub = slider(0.0..=1.0, progress, FilesMessage::PreviewScrubChanged).step(0.01_f32).width(Length::Fill).style(theme::preview_slider);
    let time_bar = row![
        text(format!("In {}", format_duration_mm_ss(in_ns))).size(11).color(theme::TEXT_SECONDARY),
        horizontal_space(),
        text(format_duration_mm_ss(current_ns)).size(11).font(theme::FONT_SEMIBOLD).color(theme::TEXT_PRIMARY),
        horizontal_space(),
        text(format!("Out {}", format_duration_mm_ss(out_ns))).size(11).color(theme::TEXT_SECONDARY),
    ]
    .align_y(Vertical::Center);

    let info_card = container(
        column![
            text(s.files_info_header).size(11).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY),
            info_row(s.files_info_name, selected.map_or_else(|| "—".to_string(), |a| a.name.clone())),
            info_row(s.files_info_duration, format_duration_mm_ss(duration_ns)),
            info_row(s.files_info_resolution, selected.map_or_else(|| "—".to_string(), |a| i18n::format_asset_resolution(lang, a))),
            info_row(s.files_info_size, selected.map_or_else(|| "—".to_string(), |a| format_file_size(a.metadata.file_size_bytes))),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(12)
    .style(theme::card);

    let mark_in = button(text(s.files_mark_in).size(12).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
        .width(Length::Fill).padding([8, 10]).style(theme::secondary_button).on_press_maybe(selected.map(|_| FilesMessage::MarkIn));
    let mark_out = button(text(s.files_mark_out).size(12).font(theme::FONT_MEDIUM).wrapping(text::Wrapping::None))
        .width(Length::Fill).padding([8, 10]).style(theme::secondary_button).on_press_maybe(selected.map(|_| FilesMessage::MarkOut));
    let send_to_timeline = button(
        row![icons::icon(icons::PLUS, 13.0, theme::TEXT_PRIMARY), text(s.files_send_timeline).size(12).font(theme::FONT_SEMIBOLD).wrapping(text::Wrapping::None)]
            .spacing(6).align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(theme::secondary_button)
    .on_press_maybe(selected.map(|asset| FilesMessage::SendToTimeline(asset.id)));

    container(
        column![text(s.files_preview_header).size(11).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY), viewport, scrub, time_bar, info_card, column![mark_in, mark_out, send_to_timeline].spacing(8)]
            .spacing(12)
            .height(Length::Fill),
    )
    .width(300)
    .height(Length::Fill)
    .padding(14)
    .style(theme::sidebar)
    .into()
}

fn info_row<'a>(label: &'a str, value: String) -> Element<'a, FilesMessage> {
    row![
        text(label).size(11).color(theme::TEXT_SECONDARY),
        horizontal_space(),
        text(value).size(11).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
    ]
    .align_y(Vertical::Center)
    .into()
}
