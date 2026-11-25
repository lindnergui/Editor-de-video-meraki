use meraki_core::{AddFolderCommand, AssetId, FolderId, HistoryStack, MediaAsset, MediaFolder, MediaKind, Project};
use std::path::{Path, PathBuf};

/// Filtro rápido de tipo de mídia na aba Arquivos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaFilter {
    #[default]
    All,
    Videos,
    Audios,
    Images,
}

impl MediaFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::Videos, Self::Audios, Self::Images];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::Videos => "Vídeos",
            Self::Audios => "Áudios",
            Self::Images => "Imagens",
        }
    }

    pub const fn matches(self, kind: MediaKind) -> bool {
        match self {
            Self::All => true,
            Self::Videos => matches!(kind, MediaKind::Video),
            Self::Audios => matches!(kind, MediaKind::Audio),
            Self::Images => matches!(kind, MediaKind::Image),
        }
    }
}

/// Eventos de interação específicos da aba Arquivos.
#[derive(Debug, Clone)]
pub enum FilesMessage {
    ImportClicked,
    FilesPicked(Vec<PathBuf>),
    SearchChanged(String),
    SelectFolder(Option<FolderId>),
    ToggleNewFolderInput,
    NewFolderNameChanged(String),
    ConfirmNewFolder,
    SelectFilter(MediaFilter),
    SelectAsset(AssetId),
    PreviewScrubChanged(f32),
    MarkIn,
    MarkOut,
    SendToTimeline(AssetId),
}

/// Estado local de navegação, busca, filtros e pré-visualização da aba Arquivos.
#[derive(Debug, Clone, Default)]
pub struct FilesState {
    pub search_query: String,
    pub selected_folder: Option<FolderId>,
    pub active_filter: MediaFilter,
    pub selected_asset: Option<AssetId>,
    pub preview_progress: f32,
    pub creating_folder: bool,
    pub new_folder_name: String,
}

/// Abre o seletor nativo de arquivos de mídia do sistema operacional de forma assíncrona.
pub async fn pick_media_files() -> Vec<PathBuf> {
    let handles = rfd::AsyncFileDialog::new()
        .set_title("Importar Mídia")
        .add_filter(
            "Mídias suportadas",
            &[
                "mp4", "mov", "mkv", "avi", "webm", "wav", "mp3", "aac", "flac", "ogg", "m4a",
                "png", "jpg", "jpeg", "webp",
            ],
        )
        .add_filter("Vídeos", &["mp4", "mov", "mkv", "avi", "webm"])
        .add_filter("Áudios", &["wav", "mp3", "aac", "flac", "ogg", "m4a"])
        .add_filter("Imagens", &["png", "jpg", "jpeg", "webp"])
        .pick_files()
        .await;
    handles
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.path().to_path_buf())
        .collect()
}

/// Retorna os ativos visíveis aplicando pasta selecionada, filtro de tipo e busca textual.
pub fn filtered_assets<'a>(project: &'a Project, state: &FilesState) -> Vec<&'a MediaAsset> {
    let query = state.search_query.trim().to_ascii_lowercase();
    project
        .assets
        .iter()
        .filter(|asset| {
            let folder_ok = state.selected_folder.is_none_or(|folder_id| {
                asset
                    .folder_id
                    .is_some_and(|fid| is_descendant_or_same(project, fid, folder_id))
            });
            let kind_ok = state.active_filter.matches(asset.kind);
            let search_ok = query.is_empty() || asset.name.to_ascii_lowercase().contains(&query);
            folder_ok && kind_ok && search_ok
        })
        .collect()
}

/// Conta quantos ativos pertencem a uma pasta e suas subpastas.
pub fn folder_asset_count(project: &Project, folder_id: FolderId) -> usize {
    project
        .assets
        .iter()
        .filter(|asset| {
            asset
                .folder_id
                .is_some_and(|fid| is_descendant_or_same(project, fid, folder_id))
        })
        .count()
}

fn is_descendant_or_same(project: &Project, candidate: FolderId, target: FolderId) -> bool {
    let mut current = Some(candidate);
    let mut depth = 0_usize;
    while let Some(fid) = current {
        if fid == target {
            return true;
        }
        if depth >= project.folders.len() {
            break;
        }
        depth += 1;
        current = project.folder(fid).and_then(|f| f.parent_id);
    }
    false
}

/// Formata uma duração em nanossegundos como `MM:SS`.
pub fn format_duration_mm_ss(duration_ns: u64) -> String {
    let total_secs = duration_ns / 1_000_000_000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

/// Formata tamanho de arquivo em bytes de forma legível.
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Formata resolução de um ativo para exibição no inspetor da aba Arquivos.
pub fn format_resolution(asset: &MediaAsset) -> String {
    match (asset.metadata.width, asset.metadata.height) {
        (Some(w), Some(h)) => format!("{w}x{h}"),
        _ if asset.kind == MediaKind::Audio => "Apenas áudio".to_string(),
        _ => "—".to_string(),
    }
}

/// Rótulo legível da categoria de mídia para cards e inspetor.
pub const fn media_kind_label(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Video => "Vídeo",
        MediaKind::Audio => "Áudio",
        MediaKind::Image => "Imagem",
    }
}

/// Infere o tipo de mídia pela extensão do arquivo importado.
pub fn infer_media_kind(path: &Path) -> MediaKind {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif") => MediaKind::Image,
        Some("wav" | "mp3" | "aac" | "flac" | "ogg" | "m4a") => MediaKind::Audio,
        _ => MediaKind::Video,
    }
}

/// Seleciona a pasta padrão na biblioteca conforme o tipo da mídia importada.
pub fn default_folder_for(project: &Project, path: &Path) -> Option<FolderId> {
    let target_name = match infer_media_kind(path) {
        MediaKind::Video => "Brutos Câmera",
        MediaKind::Audio => "Trilhas & SFX",
        MediaKind::Image => "Imagens & Logos",
    };
    project.child_folders(None).into_iter().find(|f| f.name == target_name).map(|f| f.id)
}

/// Aplica marcação de ponto In ou Out ao ativo selecionado usando o progresso normalizado.
pub fn apply_in_out_mark(project: &mut Project, asset_id: AssetId, preview_progress: f32, mark_in: bool) -> Result<(), String> {
    let Some(asset) = project.asset_mut(asset_id) else { return Ok(()); };
    let duration_ns = asset.metadata.duration_ns;
    if duration_ns == 0 {
        return Ok(());
    }
    let target_ns = ((duration_ns as f64) * f64::from(preview_progress.clamp(0.0, 1.0))).round() as u64;
    let result = if mark_in {
        let in_ns = target_ns.min(duration_ns.saturating_sub(1));
        let current_out = asset.out_point_ns.unwrap_or(duration_ns);
        let out_ns = if current_out <= in_ns { duration_ns } else { current_out };
        asset.set_in_out(in_ns, Some(out_ns))
    } else {
        let out_ns = target_ns.clamp(1, duration_ns);
        let in_ns = if asset.in_point_ns >= out_ns { out_ns.saturating_sub(1) } else { asset.in_point_ns };
        asset.set_in_out(in_ns, Some(out_ns))
    };
    result.map_err(|err| err.to_string())
}

/// Cria uma nova pasta na árvore a partir do estado do formulário da aba Arquivos.
pub fn confirm_new_folder(project: &mut Project, history: &mut HistoryStack, state: &mut FilesState) -> Result<(), String> {
    let name = state.new_folder_name.trim().to_string();
    if name.is_empty() {
        return Ok(());
    }
    let folder = MediaFolder::new(name, state.selected_folder);
    let folder_id = folder.id;
    history
        .execute(Box::new(AddFolderCommand::new(folder)), project)
        .map_err(|err| err.to_string())?;
    state.selected_folder = Some(folder_id);
    state.creating_folder = false;
    state.new_folder_name.clear();
    Ok(())
}
