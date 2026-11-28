use crate::app_project_io::ProjectIoState;
use crate::app_proxy::ProxyBannerState;
use crate::components::inspector::{self, InspectorMessage};
use crate::components::media_thumb::asset_thumbnail;
use crate::components::player::{self, PlayerMessage};
use crate::components::timeline_canvas::{self, TimelineCanvasMessage};
use crate::components::timeline_math::{TimelineTool, TimelineViewState};
use crate::i18n;
use crate::playback::PlaybackState;
use crate::views::files_model::{filtered_assets, format_duration_mm_ss, FilesState, MediaFilter};
use crate::views::transition_inspector::{self, TransitionInspectorMessage};
use crate::{icons, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, vertical_rule, Column, Row,
};
use iced::{Element, Length};
use meraki_core::{AppLanguage, AssetId, ClipId, ClipTransform, FolderId, HistoryStack, KeymapProfile, MediaAsset, Project};
use uuid::Uuid;

/// Estado local da aba Edição (gaveta compacta de mídia, rascunho e ferramentas/zoom da timeline).
#[derive(Debug, Clone, Default)]
pub struct EditState {
    pub search_query: String,
    pub selected_folder: Option<FolderId>,
    pub timeline: TimelineViewState,
    pub transform_draft: Option<(ClipId, ClipTransform)>,
    pub transition_duration_draft: Option<(Uuid, u64)>,
}

/// Mensagens de interação da aba Edição roteadas por `MerakiApp`.
#[derive(Debug, Clone)]
pub enum EditMessage {
    SearchChanged(String),
    SelectFolder(Option<FolderId>),
    InsertAsset(AssetId),
    Player(PlayerMessage),
    Inspector(InspectorMessage),
    TransitionInspector(TransitionInspectorMessage),
    Timeline(TimelineCanvasMessage),
}

impl EditState {
    /// Processa alterações puramente locais de ferramenta, snap, zoom, busca ou filtro de pasta.
    /// Retorna `true` se a mensagem foi consumida localmente.
    pub fn apply_local(&mut self, msg: &EditMessage) -> bool {
        match msg {
            EditMessage::SearchChanged(q) => self.search_query = q.clone(),
            EditMessage::SelectFolder(folder) => self.selected_folder = *folder,
            EditMessage::Timeline(TimelineCanvasMessage::SetTool(tool)) => self.timeline.tool = *tool,
            EditMessage::Timeline(TimelineCanvasMessage::ToggleSnap) => {
                self.timeline.snap_enabled = !self.timeline.snap_enabled;
            }
            EditMessage::Timeline(TimelineCanvasMessage::ZoomIn) => self.timeline.zoom_in(),
            EditMessage::Timeline(TimelineCanvasMessage::ZoomOut) => self.timeline.zoom_out(),
            EditMessage::Timeline(TimelineCanvasMessage::ZoomAnchored { factor, anchor_ns }) => {
                self.timeline.zoom_anchored_at(*factor, *anchor_ns);
            }
            _ => return false,
        }
        true
    }
}

/// Renderiza o layout completo da aba Edição: topo 3 colunas (240px | Player | 280px) + timeline.
#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    project: &'a Project,
    playback: &'a PlaybackState,
    history: &'a HistoryStack,
    state: &'a EditState,
    project_io: &'a ProjectIoState,
    profile: KeymapProfile,
    lang: AppLanguage,
    proxy_banner: ProxyBannerState,
) -> Element<'a, EditMessage> {
    let media_drawer = media_drawer_column(project, state, project_io, lang);
    let player_panel = player::view(project, playback, history.can_undo(), history.can_redo(), profile, lang, proxy_banner)
        .map(EditMessage::Player);
    let selected_tr = playback
        .selected_transition_id
        .and_then(|id| project.timeline.transition(id));
    let inspector_panel = if let Some(tr) = selected_tr {
        transition_inspector::view(project, tr, state.transition_duration_draft, lang)
            .map(EditMessage::TransitionInspector)
    } else {
        inspector::view(project, playback.selected_clip_id, state.transform_draft, lang)
            .map(EditMessage::Inspector)
    };

    let top_row = container(
        row![
            media_drawer,
            vertical_rule(1).style(theme::divider),
            player_panel,
            vertical_rule(1).style(theme::divider),
            inspector_panel,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(66));

    let bottom_timeline = container(
        timeline_canvas::view(project, playback, &state.timeline, profile, lang).map(EditMessage::Timeline),
    )
    .width(Length::Fill)
    .height(Length::FillPortion(34));

    column![
        top_row,
        horizontal_rule(1).style(theme::divider),
        bottom_timeline,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn media_drawer_column<'a>(
    project: &'a Project,
    state: &'a EditState,
    project_io: &'a ProjectIoState,
    lang: AppLanguage,
) -> Element<'a, EditMessage> {
    let s = i18n::strings(lang);
    let header = row![
        text(s.files_project_media).size(11).font(theme::FONT_BOLD).color(theme::TEXT_SECONDARY).wrapping(text::Wrapping::None),
        horizontal_space(),
        text(format!("{}", project.assets.len())).size(11).color(theme::TEXT_SECONDARY),
    ]
    .align_y(Vertical::Center);

    let search = row![
        icons::icon(icons::SEARCH, 13.0, theme::TEXT_SECONDARY),
        text_input(s.edit_search_placeholder, &state.search_query)
            .on_input(EditMessage::SearchChanged)
            .size(12)
            .padding([5, 8])
            .style(theme::text_input_style),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let mut folder_pills = Row::new()
        .spacing(4)
        .push(folder_pill(s.edit_all_folders, None, state.selected_folder.is_none()));
    for folder in project.child_folders(None) {
        let active = state.selected_folder == Some(folder.id);
        folder_pills = folder_pills.push(folder_pill(folder.name.as_str(), Some(folder.id), active));
    }
    let folder_pills = folder_pills.wrap();

    let filter_state = FilesState {
        search_query: state.search_query.clone(),
        selected_folder: state.selected_folder,
        active_filter: MediaFilter::All,
        ..FilesState::default()
    };
    let assets = filtered_assets(project, &filter_state);

    let body: Element<'a, EditMessage> = if assets.is_empty() {
        container(
            text(s.edit_empty_folder)
                .size(12)
                .color(theme::TEXT_SECONDARY)
                .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .center_y(Length::Fill)
        .padding(12)
        .style(theme::card)
        .into()
    } else {
        let mut grid = Column::new().spacing(8).width(Length::Fill);
        for pair in assets.chunks(2) {
            let mut r = Row::new().spacing(8).width(Length::Fill);
            r = r.push(compact_asset_card(pair[0], project_io));
            if let Some(second) = pair.get(1) {
                r = r.push(compact_asset_card(second, project_io));
            } else {
                r = r.push(container("").width(Length::FillPortion(1)));
            }
            grid = grid.push(r);
        }
        scrollable(grid).height(Length::Fill).style(theme::dark_scrollable).into()
    };

    container(
        column![
            header,
            search,
            folder_pills,
            horizontal_rule(1).style(theme::divider),
            body,
        ]
        .spacing(10)
        .height(Length::Fill),
    )
    .width(240)
    .height(Length::Fill)
    .padding(12)
    .style(theme::sidebar)
    .into()
}

fn folder_pill<'a>(label: &'a str, id: Option<FolderId>, active: bool) -> Element<'a, EditMessage> {
    button(text(label).size(10).font(theme::FONT_MEDIUM))
        .padding([4, 7])
        .style(theme::filter_pill_button(active))
        .on_press(EditMessage::SelectFolder(id))
        .into()
}

fn compact_asset_card<'a>(asset: &'a MediaAsset, project_io: &'a ProjectIoState) -> Element<'a, EditMessage> {
    let thumb = asset_thumbnail(asset, project_io, 44.0, 18.0, theme::ACCENT_HOVER);

    let meta_row = row![
        text(format_duration_mm_ss(asset.metadata.duration_ns))
            .size(10)
            .color(theme::TEXT_SECONDARY),
        horizontal_space(),
        container(icons::icon(icons::PLUS, 10.0, theme::TEXT_ON_ACCENT))
            .padding(3)
            .style(theme::badge_accent),
    ]
    .align_y(Vertical::Center);

    button(
        column![
            thumb,
            text(asset.name.as_str()).size(11).font(theme::FONT_MEDIUM).color(theme::TEXT_PRIMARY),
            meta_row,
        ]
        .spacing(5),
    )
    .width(Length::FillPortion(1))
    .padding(7)
    .style(theme::media_card_button(false))
    .on_press(EditMessage::InsertAsset(asset.id))
    .into()
}

pub const fn timeline_tool_names() -> [TimelineTool; 2] {
    [TimelineTool::Select, TimelineTool::Razor]
}
