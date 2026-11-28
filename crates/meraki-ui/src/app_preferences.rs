use crate::app::MerakiApp;
use meraki_core::{AppLanguage, AppPreferences, KeymapProfile};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Resolve o caminho padrão de preferências globais sem dependências externas.
pub fn preferences_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(appdata).join("Meraki").join("preferences.json");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(xdg).join("meraki").join("preferences.json");
        }
        if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(home)
                .join(".config")
                .join("meraki")
                .join("preferences.json");
        }
    }
    std::env::temp_dir().join("meraki-preferences.json")
}

/// Carrega preferências a partir de `path`, retornando defaults com aviso se ausente ou inválido.
pub fn load_preferences_from(path: &Path) -> AppPreferences {
    let raw = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            warn!(path = %path.display(), error = %err, "preferências ausentes ou ilegíveis; usando padrão");
            return AppPreferences::default();
        }
    };
    match AppPreferences::from_json(&raw) {
        Ok(prefs) => prefs,
        Err(err) => {
            warn!(path = %path.display(), error = %err, "preferências corrompidas; restaurando padrão");
            AppPreferences::default()
        }
    }
}

/// Persiste preferências em `path`, criando o diretório pai quando necessário.
pub fn save_preferences_to(path: &Path, prefs: &AppPreferences) -> Result<(), String> {
    let json = prefs.to_json().map_err(|err| err.to_string())?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Falha ao preparar pasta de preferências: {err}"))?;
    }
    fs::write(path, json).map_err(|err| format!("Falha ao salvar preferências: {err}"))
}

/// Carrega as preferências globais do usuário a partir do caminho padrão da plataforma.
pub fn load_preferences() -> AppPreferences {
    load_preferences_from(&preferences_path())
}

/// Salva as preferências globais do usuário no caminho padrão da plataforma.
pub fn save_preferences(prefs: &AppPreferences) -> Result<(), String> {
    save_preferences_to(&preferences_path(), prefs)
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl MerakiApp {
    pub(crate) fn apply_proxy_preference(&mut self) {
        if self.preferences.proxy_mode != meraki_core::ProxyMode::default() {
            if let Err(err) = self.engine.send(meraki_engine::EngineCommand::SetProxyMode(self.preferences.proxy_mode)) {
                self.last_error = Some(err.to_string());
            }
        }
    }

    /// Substitui o caminho de persistência de preferências (útil para isolamento em testes).
    pub fn set_preferences_path(&mut self, path: PathBuf) {
        self.preferences_path = path;
    }

    /// Atualiza o idioma ativo da interface e persiste imediatamente no `preferences.json`.
    pub fn set_language(&mut self, language: AppLanguage) -> Result<(), String> {
        self.preferences.language = language;
        save_preferences_to(&self.preferences_path, &self.preferences)
    }

    /// Atualiza o perfil de atalhos ativo e persiste imediatamente no `preferences.json`.
    pub fn set_keymap_profile(&mut self, profile: KeymapProfile) -> Result<(), String> {
        self.preferences.keymap = profile;
        save_preferences_to(&self.preferences_path, &self.preferences)
    }

    /// Conclui o onboarding inicial com o perfil escolhido e persiste preferências.
    pub fn complete_onboarding(&mut self, profile: KeymapProfile) -> Result<(), String> {
        self.preferences.keymap = profile;
        self.preferences.onboarding_completed = true;
        save_preferences_to(&self.preferences_path, &self.preferences)
    }

    pub(crate) fn record_recent_project(&mut self, name: String, path: PathBuf) {
        self.preferences
            .upsert_recent(name, path, current_unix_secs());
        if let Err(err) = save_preferences_to(&self.preferences_path, &self.preferences) {
            warn!(error = %err, "não foi possível atualizar preferences.json com projeto recente");
        }
    }
}
