use crate::components::tooltip::icon_tooltip;
use crate::views::project_hub_cards::{new_project_square_card, project_square_card};
pub use crate::views::project_hub_cards::{CARD_SIZE, GRID_COLUMNS, GRID_SPACING};
use crate::{icons, theme, window_icon};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::tooltip::Position;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, Column, Row, Space,
};
use iced::{Element, Length};
use meraki_core::RecentProjectEntry;
use std::path::{Path, PathBuf};

/// Estado local do painel inicial de projetos (`ProjectHub`).
#[derive(Debug, Clone, Default)]
pub struct ProjectHubState {
    pub search_query: String,
    pub renaming_path: Option<PathBuf>,
    pub rename_draft: String,
}

/// Ações emitidas pelo painel inicial de projetos.
#[derive(Debug, Clone)]
pub enum ProjectHubMessage {
    SearchChanged(String),
    NewProjectClicked,
    ToggleSettings,
    OpenFileDialog,
    OpenShortcuts,
    OpenRecent(PathBuf),
    StartRename { path: PathBuf, current_name: String },
    RenameDraftChanged(String),
    CommitRename,
    CancelRename,
    RemoveRecent(PathBuf),
}

/// Filtra projetos recentes por nome ou caminho de forma case-insensitive.
pub fn filtered_recent_projects<'a>(
    entries: &'a [RecentProjectEntry],
    query: &str,
) -> Vec<&'a RecentProjectEntry> {
    let q = query.trim().to_ascii_lowercase();
    entries
        .iter()
        .filter(|entry| {
            q.is_empty()
                || entry.name.to_ascii_lowercase().contains(&q)
                || entry.path.to_string_lossy().to_ascii_lowercase().contains(&q)
        })
        .collect()
}

/// Calcula quantos cards reais e quantos espaços vazios cada linha da grade 3xN recebe.
pub fn build_grid_row_slot_counts(total_items: usize) -> Vec<(usize, usize)> {
    if total_items == 0 {
        return Vec::new();
    }
    (0..total_items)
        .step_by(GRID_COLUMNS)
        .map(|start| {
            let count = (total_items - start).min(GRID_COLUMNS);
            (count, GRID_COLUMNS - count)
        })
        .collect()
}

/// Encurta caminhos longos mantendo pasta imediata e nome do arquivo.
pub fn format_short_path(path: &Path) -> String {
    let file = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
    let parent = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str());
    match (parent, file.is_empty()) {
        (Some(dir), false) => format!(".../{dir}/{file}"),
        (None, false) => file.to_string(),
        _ => path.display().to_string(),
    }
}

/// Formata timestamp Unix UTC em `DD/MM/AAAA HH:MM` sem dependências externas.
pub fn format_unix_timestamp(unix_secs: u64) -> String {
    if unix_secs == 0 {
        return "Recente".to_string();
    }
    let days = (unix_secs / 86_400) as i64;
    let secs_of_day = unix_secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!("{d:02}/{m:02}/{year:04} {hour:02}:{minute:02}")
}

/// Renderiza o Painel de Projetos com grade rolável 3xN de cards quadrados 260x260.
pub fn view<'a>(
    state: &'a ProjectHubState,
    recent: &'a [RecentProjectEntry],
    settings_open: bool,
    last_error: Option<&'a str>,
) -> Element<'a, ProjectHubMessage> {
    let header = build_header(settings_open);
    let filtered = filtered_recent_projects(recent, &state.search_query);
    let has_query = !state.search_query.trim().is_empty();

    let title_row = row![
        column![
            text("Seus Projetos").size(22).font(theme::FONT_BOLD).color(theme::TEXT_PRIMARY),
            text("Retome uma edição recente ou crie um novo projeto do zero.")
                .size(13)
                .color(theme::TEXT_SECONDARY),
        ]
        .spacing(4),
        horizontal_space(),
        container(
            row![
                icons::icon(icons::SEARCH, 14.0, theme::TEXT_SECONDARY),
                text_input("Pesquisar projetos...", &state.search_query)
                    .on_input(ProjectHubMessage::SearchChanged)
                    .size(13)
                    .padding([7, 10])
                    .style(theme::text_input_style),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .width(Length::Fixed(280.0)),
    ]
    .spacing(16)
    .align_y(Vertical::Center);

    let mut content_col = column![title_row].spacing(20).width(Length::Fill);

    if let Some(err) = last_error {
        content_col = content_col.push(
            container(
                row![
                    icons::icon(icons::ALERT_CIRCLE, 15.0, theme::WARNING),
                    text(err).size(12).color(theme::TEXT_PRIMARY),
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            )
            .width(Length::Fill)
            .padding([10, 14])
            .style(theme::card),
        );
    }

    let body_section: Element<'a, ProjectHubMessage> = if has_query && filtered.is_empty() {
        container(
            column![
                icons::icon(icons::SEARCH, 22.0, theme::TEXT_SECONDARY),
                text("Nenhum projeto encontrado")
                    .size(14)
                    .font(theme::FONT_SEMIBOLD)
                    .color(theme::TEXT_PRIMARY),
                text("Tente outro termo ou limpe o campo de pesquisa acima.")
                    .size(12)
                    .color(theme::TEXT_SECONDARY),
            ]
            .spacing(6)
            .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .padding([36, 16])
        .center_x(Length::Fill)
        .into()
    } else {
        let mut cards: Vec<Element<'a, ProjectHubMessage>> =
            Vec::with_capacity(filtered.len() + usize::from(!has_query));
        if !has_query {
            cards.push(new_project_square_card());
        }
        for entry in filtered {
            cards.push(project_square_card(entry, state));
        }

        let row_specs = build_grid_row_slot_counts(cards.len());
        let mut card_iter = cards.into_iter();
        let mut rows_col = Column::new().spacing(GRID_SPACING);
        for (count, empty_slots) in row_specs {
            let mut row_items: Vec<Element<'a, ProjectHubMessage>> =
                Vec::with_capacity(GRID_COLUMNS);
            for _ in 0..count {
                if let Some(card) = card_iter.next() {
                    row_items.push(card);
                }
            }
            for _ in 0..empty_slots {
                row_items.push(Space::with_width(Length::Fixed(CARD_SIZE)).into());
            }
            rows_col = rows_col.push(Row::with_children(row_items).spacing(GRID_SPACING));
        }

        scrollable(container(rows_col).width(Length::Fill).center_x(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::dark_scrollable)
            .into()
    };

    content_col = content_col.push(body_section);

    let grid_total_width =
        GRID_COLUMNS as f32 * CARD_SIZE + (GRID_COLUMNS as f32 - 1.0) * GRID_SPACING;

    let centered_body = container(
        container(content_col)
            .max_width(grid_total_width)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .padding([28, 32])
    .style(theme::base_canvas);

    column![header, horizontal_rule(1).style(theme::divider), centered_body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn build_header<'a>(settings_open: bool) -> Element<'a, ProjectHubMessage> {
    let brand = row![
        window_icon::brand_icon(24.0),
        text("meraki").size(18).font(theme::FONT_BOLD).color(theme::TEXT_PRIMARY),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let settings_btn = icon_tooltip(
        button(icons::icon(
            icons::SETTINGS,
            15.0,
            if settings_open {
                theme::TEXT_ON_ACCENT
            } else {
                theme::TEXT_PRIMARY
            },
        ))
        .padding([8, 10])
        .style(if settings_open {
            theme::primary_button
        } else {
            theme::secondary_button
        })
        .on_press(ProjectHubMessage::ToggleSettings),
        "Configurações",
        Position::Bottom,
    );

    let new_btn = icon_tooltip(
        button(
            row![
                icons::icon(icons::PLUS, 14.0, theme::TEXT_ON_ACCENT),
                text("Novo Projeto").size(12).font(theme::FONT_SEMIBOLD).color(theme::TEXT_ON_ACCENT),
            ]
            .spacing(6)
            .align_y(Vertical::Center),
        )
        .padding([8, 16])
        .style(theme::primary_button)
        .on_press(ProjectHubMessage::NewProjectClicked),
        "Criar um novo projeto de vídeo",
        Position::Bottom,
    );

    container(
        row![brand, horizontal_space(), row![settings_btn, new_btn].spacing(10).align_y(Vertical::Center)]
            .width(Length::Fill)
            .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(60.0))
    .padding([0, 24])
    .style(theme::topbar)
    .into()
}
