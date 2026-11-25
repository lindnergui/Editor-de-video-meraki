use meraki_core::{AppPreferences, Project};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tracing::warn;

/// Resolve o diretório interno padrão onde projetos `.meraki` são gerenciados automaticamente.
pub fn internal_projects_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(appdata).join("Meraki").join("projects");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(xdg).join("meraki").join("projects");
        }
        if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("meraki")
                .join("projects");
        }
    }
    std::env::temp_dir().join("meraki-projects")
}

/// Constrói o caminho `<dir>/<project_id>.meraki` para um projeto gerenciado.
pub fn managed_project_path_in(dir: &Path, project_id: impl Display) -> PathBuf {
    dir.join(format!("{project_id}.meraki"))
}

/// Cria um novo projeto dentro de `dir`, grava o `.meraki` inicial no disco e retorna `(Project, PathBuf)`.
pub fn create_managed_project_in(dir: &Path, name: &str) -> Result<(Project, PathBuf), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Falha ao preparar pasta interna de projetos: {err}"))?;

    let mut project = Project::new(name);
    project.timeline.ensure_mixer_audio_tracks();
    let path = managed_project_path_in(dir, project.id);
    let json = project.to_json().map_err(|err| err.to_string())?;
    fs::write(&path, json)
        .map_err(|err| format!("Falha ao criar arquivo de projeto interno: {err}"))?;

    Ok((project, path))
}

/// Cria um novo projeto na pasta interna padrão da plataforma.
pub fn create_managed_project(name: &str) -> Result<(Project, PathBuf), String> {
    create_managed_project_in(&internal_projects_dir(), name)
}

/// Remove `path` do disco apenas quando o arquivo reside dentro da pasta interna `dir`.
pub fn delete_managed_project_if_internal(dir: &Path, path: &Path) -> Result<bool, String> {
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    let canonical_dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let canonical_parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    if canonical_parent != canonical_dir || !path.is_file() {
        return Ok(false);
    }
    fs::remove_file(path)
        .map_err(|err| format!("Falha ao remover projeto interno do disco: {err}"))?;
    Ok(true)
}

fn is_meraki_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("meraki"))
}

fn file_modified_unix_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |dur| dur.as_secs())
}

/// Descobre arquivos `.meraki` válidos em `dir` e os adiciona a `preferences.recent_projects` se ausentes.
pub fn scan_internal_projects_from(dir: &Path, preferences: &mut AppPreferences) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            warn!(dir = %dir.display(), error = %err, "falha ao varrer pasta interna de projetos");
            return 0;
        }
    };

    let mut discovered: Vec<(u64, String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_meraki_file(&path) {
            continue;
        }
        if preferences
            .recent_projects
            .iter()
            .any(|existing| existing.path == path)
        {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(project) = Project::from_json(&raw) else {
            continue;
        };
        discovered.push((file_modified_unix_secs(&path), project.name, path));
    }

    // Insere do mais antigo para o mais novo para preservar ordenação MRU no `upsert_recent`.
    discovered.sort_by_key(|(ts, _, _)| *ts);
    let count = discovered.len();
    for (ts, name, path) in discovered {
        preferences.upsert_recent(name, path, ts);
    }
    count
}

/// Descobre projetos válidos na pasta interna padrão da plataforma e os registra em `preferences`.
pub fn scan_internal_projects_into(preferences: &mut AppPreferences) -> usize {
    scan_internal_projects_from(&internal_projects_dir(), preferences)
}

/// Atualiza o campo `name` de um arquivo `.meraki` no disco quando ele existe.
pub(crate) fn sync_project_file_name_on_disk(path: &Path, new_name: &str) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let raw = fs::read_to_string(path).map_err(|err| format!("Falha ao ler .meraki: {err}"))?;
    let mut disk_proj = Project::from_json(&raw).map_err(|err| format!("Projeto inválido: {err}"))?;
    disk_proj.rename(new_name).map_err(|err| err.to_string())?;
    let json = disk_proj.to_json().map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| format!("Falha ao atualizar .meraki: {err}"))
}
