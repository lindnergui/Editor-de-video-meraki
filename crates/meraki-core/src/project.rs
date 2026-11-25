pub use crate::export::ExportPreset;
use crate::timeline::Timeline;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self { Self(Uuid::new_v4()) }
        }
        impl Default for $name {
            fn default() -> Self { Self::new() }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
        }
    };
}

define_id!(FolderId, "Identificador único de uma pasta na biblioteca de mídia.");
define_id!(AssetId, "Identificador único de um ativo de mídia importado no projeto.");

/// Tipo principal de mídia para organização e filtragem na aba Arquivos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

/// Qualidade do preview em tempo real selecionável na aba Edição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewQuality {
    #[default]
    Proxy720p,
    Full1080p,
}

impl PreviewQuality {
    /// Dimensões máximas (largura, altura) permitidas para o preview selecionado.
    pub const fn max_dimensions(self) -> (u32, u32) {
        match self {
            Self::Proxy720p => (1280, 720),
            Self::Full1080p => (1920, 1080),
        }
    }
}

/// Bandas fixas do equalizador visual de 4 bandas na aba Áudio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EqBand {
    Low,
    LowMid,
    Mid,
    High,
}

/// Metadados técnicos descobertos pelo motor para um ativo de mídia.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub asset_id: AssetId,
    pub duration_ns: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    #[serde(default)]
    pub has_audio: bool,
    pub audio_channels: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub file_size_bytes: u64,
}

/// Pasta hierárquica de organização na aba Arquivos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFolder {
    pub id: FolderId,
    pub name: String,
    pub parent_id: Option<FolderId>,
}

impl MediaFolder {
    pub fn new(name: impl Into<String>, parent_id: Option<FolderId>) -> Self {
        Self { id: FolderId::new(), name: name.into(), parent_id }
    }
}

/// Ativo de mídia do projeto com pontos In/Out e caminho opcional de proxy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: AssetId,
    pub name: String,
    pub path: PathBuf,
    pub kind: MediaKind,
    pub folder_id: Option<FolderId>,
    pub metadata: MediaMetadata,
    pub in_point_ns: u64,
    pub out_point_ns: Option<u64>,
    pub proxy_path: Option<PathBuf>,
}

impl MediaAsset {
    pub fn new(
        name: impl Into<String>,
        path: PathBuf,
        kind: MediaKind,
        folder_id: Option<FolderId>,
        duration_ns: u64,
    ) -> Self {
        let id = AssetId::new();
        let metadata = MediaMetadata { asset_id: id, duration_ns, ..MediaMetadata::default() };
        Self {
            id,
            name: name.into(),
            path,
            kind,
            folder_id,
            metadata,
            in_point_ns: 0,
            out_point_ns: (duration_ns > 0).then_some(duration_ns),
            proxy_path: None,
        }
    }

    /// Define os marcadores In/Out garantindo intervalo válido dentro da duração conhecida.
    pub fn set_in_out(&mut self, in_point_ns: u64, out_point_ns: Option<u64>) -> Result<(), ProjectError> {
        validate_in_out(in_point_ns, out_point_ns, self.metadata.duration_ns)?;
        self.in_point_ns = in_point_ns;
        self.out_point_ns = out_point_ns;
        Ok(())
    }
}

fn validate_in_out(in_point_ns: u64, out_point_ns: Option<u64>, duration_ns: u64) -> Result<(), ProjectError> {
    if let Some(out_ns) = out_point_ns {
        if in_point_ns >= out_ns {
            return Err(ProjectError::InvalidInOutRange { in_point_ns, out_point_ns: out_ns });
        }
        if duration_ns > 0 && out_ns > duration_ns {
            return Err(ProjectError::OutPointExceedsDuration { out_point_ns: out_ns, duration_ns });
        }
    } else if duration_ns > 0 && in_point_ns >= duration_ns {
        return Err(ProjectError::OutPointExceedsDuration { out_point_ns: in_point_ns, duration_ns });
    }
    Ok(())
}

/// Erros de domínio ao manipular pastas, mídias ou serialização de um projeto.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("nome não pode ser vazio")]
    EmptyName,
    #[error("pasta não encontrada: {0}")]
    FolderNotFound(FolderId),
    #[error("pasta duplicada: {0}")]
    DuplicateFolder(FolderId),
    #[error("pasta não está vazia: {0}")]
    FolderNotEmpty(FolderId),
    #[error("ativo de mídia não encontrado: {0}")]
    AssetNotFound(AssetId),
    #[error("ativo de mídia duplicado: {0}")]
    DuplicateAsset(AssetId),
    #[error("intervalo In/Out inválido: in={in_point_ns}ns >= out={out_point_ns}ns")]
    InvalidInOutRange { in_point_ns: u64, out_point_ns: u64 },
    #[error("ponto Out ({out_point_ns}ns) excede a duração da mídia ({duration_ns}ns)")]
    OutPointExceedsDuration { out_point_ns: u64, duration_ns: u64 },
    #[error("falha na serialização JSON do projeto: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Raiz de domínio puro do projeto Meraki, serializável em JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub folders: Vec<MediaFolder>,
    pub assets: Vec<MediaAsset>,
    pub timeline: Timeline,
    pub preview_quality: PreviewQuality,
    pub export_preset: ExportPreset,
}

impl Project {
    /// Cria um projeto novo com pastas iniciais e trilhas padrão na timeline.
    pub fn new(name: impl Into<String>) -> Self {
        let raw_name = name.into();
        let normalized = if raw_name.trim().is_empty() {
            "Projeto sem título".to_string()
        } else {
            raw_name
        };
        Self {
            id: Uuid::new_v4(),
            name: normalized,
            folders: vec![
                MediaFolder::new("Brutos Câmera", None),
                MediaFolder::new("Trilhas & SFX", None),
                MediaFolder::new("Imagens & Logos", None),
            ],
            assets: Vec::new(),
            timeline: Timeline::with_default_tracks(),
            preview_quality: PreviewQuality::Proxy720p,
            export_preset: ExportPreset::YouTube,
        }
    }

    pub fn rename(&mut self, new_name: impl Into<String>) -> Result<(), ProjectError> {
        let candidate = new_name.into();
        if candidate.trim().is_empty() {
            return Err(ProjectError::EmptyName);
        }
        self.name = candidate;
        Ok(())
    }

    pub fn add_folder(&mut self, name: impl Into<String>, parent_id: Option<FolderId>) -> Result<FolderId, ProjectError> {
        let folder = MediaFolder::new(name, parent_id);
        let id = folder.id;
        self.insert_folder(folder)?;
        Ok(id)
    }

    pub fn insert_folder(&mut self, folder: MediaFolder) -> Result<(), ProjectError> {
        if folder.name.trim().is_empty() {
            return Err(ProjectError::EmptyName);
        }
        if self.folders.iter().any(|existing| existing.id == folder.id) {
            return Err(ProjectError::DuplicateFolder(folder.id));
        }
        if let Some(parent_id) = folder.parent_id {
            if self.folder(parent_id).is_none() {
                return Err(ProjectError::FolderNotFound(parent_id));
            }
        }
        self.folders.push(folder);
        Ok(())
    }

    pub fn remove_folder(&mut self, folder_id: FolderId) -> Result<MediaFolder, ProjectError> {
        let idx = self
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
            .ok_or(ProjectError::FolderNotFound(folder_id))?;
        let has_children = self.folders.iter().any(|f| f.parent_id == Some(folder_id));
        let has_assets = self.assets.iter().any(|a| a.folder_id == Some(folder_id));
        if has_children || has_assets {
            return Err(ProjectError::FolderNotEmpty(folder_id));
        }
        Ok(self.folders.remove(idx))
    }

    pub fn folder(&self, folder_id: FolderId) -> Option<&MediaFolder> { self.folders.iter().find(|f| f.id == folder_id) }
    pub fn child_folders(&self, parent_id: Option<FolderId>) -> Vec<&MediaFolder> {
        self.folders.iter().filter(|f| f.parent_id == parent_id).collect()
    }

    pub fn add_asset(&mut self, asset: MediaAsset) -> Result<AssetId, ProjectError> {
        if asset.name.trim().is_empty() { return Err(ProjectError::EmptyName); }
        if self.assets.iter().any(|existing| existing.id == asset.id) { return Err(ProjectError::DuplicateAsset(asset.id)); }
        if let Some(folder_id) = asset.folder_id {
            if self.folder(folder_id).is_none() { return Err(ProjectError::FolderNotFound(folder_id)); }
        }
        validate_in_out(asset.in_point_ns, asset.out_point_ns, asset.metadata.duration_ns)?;
        let id = asset.id;
        self.assets.push(asset);
        Ok(id)
    }

    pub fn remove_asset(&mut self, asset_id: AssetId) -> Result<MediaAsset, ProjectError> {
        let idx = self.assets.iter().position(|a| a.id == asset_id).ok_or(ProjectError::AssetNotFound(asset_id))?;
        Ok(self.assets.remove(idx))
    }

    pub fn asset(&self, asset_id: AssetId) -> Option<&MediaAsset> { self.assets.iter().find(|a| a.id == asset_id) }
    pub fn asset_mut(&mut self, asset_id: AssetId) -> Option<&mut MediaAsset> { self.assets.iter_mut().find(|a| a.id == asset_id) }

    /// Atualiza os metadados técnicos descobertos pelo motor para um ativo existente.
    pub fn apply_discovered_metadata(&mut self, metadata: MediaMetadata) -> bool {
        let Some(asset) = self.asset_mut(metadata.asset_id) else { return false; };
        asset.metadata = metadata;
        let duration_ns = asset.metadata.duration_ns;
        if duration_ns > 0 {
            if asset.in_point_ns >= duration_ns { asset.in_point_ns = 0; }
            if asset.out_point_ns.is_none_or(|out_ns| out_ns > duration_ns) {
                asset.out_point_ns = Some(duration_ns);
            }
        }
        true
    }

    pub fn assets_by_kind(&self, kind: MediaKind) -> Vec<&MediaAsset> {
        self.assets.iter().filter(|a| a.kind == kind).collect()
    }
    pub fn assets_in_folder(&self, folder_id: Option<FolderId>) -> Vec<&MediaAsset> {
        self.assets.iter().filter(|a| a.folder_id == folder_id).collect()
    }
    pub fn to_json(&self) -> Result<String, ProjectError> { Ok(serde_json::to_string_pretty(self)?) }
    pub fn from_json(json: &str) -> Result<Self, ProjectError> { Ok(serde_json::from_str(json)?) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Clip;

    #[test]
    fn new_project_creates_default_folders_and_tracks() {
        let project = Project::new("Meu Vídeo");
        let root_names: Vec<&str> = project.child_folders(None).into_iter().map(|f| f.name.as_str()).collect();
        assert_eq!(root_names, vec!["Brutos Câmera", "Trilhas & SFX", "Imagens & Logos"]);
        assert_eq!(project.timeline.tracks.len(), 2);
        assert_eq!(project.preview_quality, PreviewQuality::Proxy720p);
    }

    #[test]
    fn folder_tree_and_asset_filtering_work() {
        let mut project = Project::new("Teste");
        let camera_id = project.folders[0].id;
        let sub_id = project.add_folder("Cena 01", Some(camera_id)).unwrap();
        let video = MediaAsset::new("Take 1", PathBuf::from("take1.mp4"), MediaKind::Video, Some(sub_id), 10_000_000_000);
        let audio = MediaAsset::new("Música", PathBuf::from("bgm.wav"), MediaKind::Audio, None, 5_000_000_000);
        project.add_asset(video).unwrap();
        project.add_asset(audio).unwrap();

        assert_eq!(project.child_folders(Some(camera_id)).len(), 1);
        assert_eq!(project.assets_by_kind(MediaKind::Video).len(), 1);
        assert_eq!(project.assets_in_folder(Some(sub_id)).len(), 1);
        assert!(matches!(project.remove_folder(sub_id), Err(ProjectError::FolderNotEmpty(_))));
    }

    #[test]
    fn asset_in_out_validation_rejects_invalid_ranges() {
        let mut asset = MediaAsset::new("Clipe", PathBuf::from("clipe.mp4"), MediaKind::Video, None, 4_000_000_000);
        assert!(asset.set_in_out(1_000_000_000, Some(3_000_000_000)).is_ok());
        assert!(matches!(asset.set_in_out(3_000_000_000, Some(2_000_000_000)), Err(ProjectError::InvalidInOutRange { .. })));
        assert!(matches!(asset.set_in_out(0, Some(5_000_000_000)), Err(ProjectError::OutPointExceedsDuration { .. })));
    }

    #[test]
    fn project_json_roundtrip_preserves_folders_assets_and_timeline() {
        let mut project = Project::new("Episódio 04");
        let folder_id = project.folders[0].id;
        let mut asset = MediaAsset::new("Entrevista", PathBuf::from("entrevista.mp4"), MediaKind::Video, Some(folder_id), 8_000_000_000);
        asset.proxy_path = Some(PathBuf::from("proxies/entrevista_720p.mp4"));
        let asset_id = project.add_asset(asset).unwrap();

        let v1 = project.timeline.tracks[0].id;
        project.timeline.add_clip(v1, Clip::new(asset_id, 0, 0, 4_000_000_000)).unwrap();

        let json = project.to_json().unwrap();
        let restored = Project::from_json(&json).unwrap();
        assert_eq!(project, restored);
    }

    #[test]
    fn apply_discovered_metadata_updates_asset_and_out_point() {
        let mut project = Project::new("Descoberta");
        let asset = MediaAsset::new("Clipe", PathBuf::from("clipe.mp4"), MediaKind::Video, None, 0);
        let asset_id = project.add_asset(asset).unwrap();
        let discovered = MediaMetadata {
            asset_id,
            duration_ns: 5_000_000_000,
            width: Some(1920),
            height: Some(1080),
            has_audio: true,
            file_size_bytes: 123_456,
            ..MediaMetadata::default()
        };

        assert!(project.apply_discovered_metadata(discovered));
        let updated = project.asset(asset_id).unwrap();
        assert_eq!(updated.metadata.duration_ns, 5_000_000_000);
        assert_eq!(updated.metadata.width, Some(1920));
        assert_eq!(updated.out_point_ns, Some(5_000_000_000));
        assert!(updated.metadata.has_audio);
        assert!(!project.apply_discovered_metadata(MediaMetadata::default()));
    }
}
