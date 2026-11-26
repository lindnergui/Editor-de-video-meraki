use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Limite máximo de projetos recentes mantidos no histórico de preferências.
pub const MAX_RECENT_PROJECTS: usize = 20;

/// Ações mapeáveis entre os perfis de atalhos de teclado suportados pelo Meraki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutAction {
    SplitAtPlayhead,
    ToolSelect,
    ToggleSnap,
    PlayPause,
    ShuttleReverse,
    ShuttlePause,
    ShuttleForward,
    StepFrameBack,
    StepFrameForward,
    Undo,
    Redo,
    DeleteClip,
    SaveProject,
    OpenProject,
}

impl ShortcutAction {
    /// Lista completa de todas as ações configuráveis nos perfis de teclado.
    pub const fn all() -> &'static [ShortcutAction] {
        &[
            Self::SplitAtPlayhead,
            Self::ToolSelect,
            Self::ToggleSnap,
            Self::PlayPause,
            Self::ShuttleReverse,
            Self::ShuttlePause,
            Self::ShuttleForward,
            Self::StepFrameBack,
            Self::StepFrameForward,
            Self::Undo,
            Self::Redo,
            Self::DeleteClip,
            Self::SaveProject,
            Self::OpenProject,
        ]
    }
}

/// Perfil de atalhos de teclado inspirado em editores populares de mercado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum KeymapProfile {
    #[default]
    PremierePro,
    DaVinciResolve,
    CapCut,
    VegasPro,
}

impl KeymapProfile {
    /// Todos os perfis de atalhos disponíveis no onboarding e nas configurações.
    pub const fn all() -> &'static [KeymapProfile] {
        &[
            Self::PremierePro,
            Self::DaVinciResolve,
            Self::CapCut,
            Self::VegasPro,
        ]
    }

    /// Nome amigável exibido na interface de escolha de atalhos.
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::PremierePro => "Adobe Premiere Pro",
            Self::DaVinciResolve => "DaVinci Resolve",
            Self::CapCut => "CapCut",
            Self::VegasPro => "VEGAS Pro",
        }
    }

    /// Subtítulo curto descrevendo os atalhos principais do perfil.
    pub const fn subtitle(&self) -> &'static str {
        match self {
            Self::PremierePro => "Corte com Ctrl+K, seleção em V e ímã em S.",
            Self::DaVinciResolve => "Corte com Ctrl+B, seleção em A e ímã em N.",
            Self::CapCut => "Fluxo ágil com corte Ctrl+B, seleção A e ímã N.",
            Self::VegasPro => "Corte direto em S, refazer Ctrl+Y e ímã em F8.",
        }
    }

    /// Rótulo legível do atalho para uma ação específica neste perfil.
    pub const fn shortcut_label(&self, action: ShortcutAction) -> &'static str {
        match action {
            ShortcutAction::SplitAtPlayhead => match self {
                Self::PremierePro => "Ctrl+K",
                Self::DaVinciResolve | Self::CapCut => "Ctrl+B",
                Self::VegasPro => "S",
            },
            ShortcutAction::ToolSelect => match self {
                Self::PremierePro => "V",
                Self::DaVinciResolve | Self::CapCut => "A",
                Self::VegasPro => "D",
            },
            ShortcutAction::ToggleSnap => match self {
                Self::PremierePro => "S",
                Self::DaVinciResolve | Self::CapCut => "N",
                Self::VegasPro => "F8",
            },
            ShortcutAction::PlayPause => "Space",
            ShortcutAction::ShuttleReverse => "J",
            ShortcutAction::ShuttlePause => "K",
            ShortcutAction::ShuttleForward => "L",
            ShortcutAction::StepFrameBack => "←",
            ShortcutAction::StepFrameForward => "→",
            ShortcutAction::Undo => "Ctrl+Z",
            ShortcutAction::Redo => match self {
                Self::VegasPro => "Ctrl+Y",
                Self::PremierePro | Self::DaVinciResolve | Self::CapCut => "Ctrl+Shift+Z",
            },
            ShortcutAction::DeleteClip => "Delete",
            ShortcutAction::SaveProject => "Ctrl+S",
            ShortcutAction::OpenProject => "Ctrl+O",
        }
    }
}

/// Entrada no histórico de projetos abertos/salvos recentemente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProjectEntry {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub last_opened_unix_secs: u64,
}

/// Preferências globais do aplicativo persistidas pelo shell fora do domínio puro.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPreferences {
    pub keymap: KeymapProfile,
    pub onboarding_completed: bool,
    pub recent_projects: Vec<RecentProjectEntry>,
}

impl AppPreferences {
    /// Registra ou atualiza um projeto recente, movendo-o para o topo e limitando a 20 itens.
    pub fn upsert_recent(&mut self, name: String, path: PathBuf, now_unix_secs: u64) {
        let existing_id = self
            .recent_projects
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.id)
            .unwrap_or_else(Uuid::new_v4);

        self.recent_projects.retain(|entry| entry.path != path);

        let clean_name = name.trim();
        let entry_name = if clean_name.is_empty() {
            "Projeto sem título".to_string()
        } else {
            clean_name.to_string()
        };

        self.recent_projects.insert(
            0,
            RecentProjectEntry {
                id: existing_id,
                name: entry_name,
                path,
                last_opened_unix_secs: now_unix_secs,
            },
        );
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    /// Renomeia a entrada correspondente a `path`, retornando `false` se ausente ou nome vazio.
    pub fn rename_recent(&mut self, path: &Path, new_name: String) -> bool {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return false;
        }
        let Some(entry) = self.recent_projects.iter_mut().find(|item| item.path == path) else {
            return false;
        };
        entry.name = trimmed.to_string();
        true
    }

    /// Remove o projeto associado a `path`, retornando `true` caso existisse na lista.
    pub fn remove_recent(&mut self, path: &Path) -> bool {
        let prev_len = self.recent_projects.len();
        self.recent_projects.retain(|entry| entry.path != path);
        self.recent_projects.len() < prev_len
    }

    /// Serializa as preferências em JSON legível.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Desserializa preferências a partir de JSON mantendo defaults em campos ausentes.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
