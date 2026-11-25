#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use meraki_engine::EngineCommand;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BundledGStreamer {
    plugin_dir: PathBuf,
    scanner_path: Option<PathBuf>,
}

fn resolve_bundled_gstreamer(exe_dir: &Path) -> Option<BundledGStreamer> {
    let plugin_dir = exe_dir.join("lib").join("gstreamer-1.0");
    if !plugin_dir.is_dir() {
        return None;
    }

    let scanner_name = if cfg!(windows) {
        "gst-plugin-scanner.exe"
    } else {
        "gst-plugin-scanner"
    };
    let scanner_candidates = [
        exe_dir.join("libexec").join("gstreamer-1.0").join(scanner_name),
        plugin_dir.join(scanner_name),
        exe_dir.join(scanner_name),
    ];
    let scanner_path = scanner_candidates.into_iter().find(|path| path.is_file());

    Some(BundledGStreamer { plugin_dir, scanner_path })
}

fn apply_bundled_gstreamer_with<SetIfMissing>(
    bundle: &BundledGStreamer,
    mut set_if_missing: SetIfMissing,
) where
    SetIfMissing: FnMut(&str, &OsStr),
{
    set_if_missing("GST_PLUGIN_SYSTEM_PATH_1_0", bundle.plugin_dir.as_os_str());
    if let Some(scanner) = &bundle.scanner_path {
        set_if_missing("GST_PLUGIN_SCANNER", scanner.as_os_str());
    }
}

/// Configura `GST_PLUGIN_SYSTEM_PATH_1_0` e `GST_PLUGIN_SCANNER` quando houver runtime
/// GStreamer embutido em `lib/gstreamer-1.0` ao lado do executável, preservando overrides manuais.
fn configure_bundled_gstreamer() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe_path.parent() else {
        return;
    };
    let Some(bundle) = resolve_bundled_gstreamer(exe_dir) else {
        return;
    };

    apply_bundled_gstreamer_with(&bundle, |key, value| {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    });
    info!(
        plugin_dir = %bundle.plugin_dir.display(),
        scanner = ?bundle.scanner_path,
        "runtime GStreamer empacotado detectado ao lado do executável"
    );
}

fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    configure_bundled_gstreamer();

    let engine = meraki_engine::spawn_engine();
    let shutdown_handle = engine.clone();

    if let Err(err) = meraki_ui::run(engine) {
        error!(error = %err, "falha ao executar interface gráfica do Meraki");
    }

    if let Err(err) = shutdown_handle.send(EngineCommand::Shutdown) {
        warn!(error = %err, "não foi possível enviar Shutdown ao motor no encerramento");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs;

    #[test]
    fn resolves_bundled_plugins_and_scanner_when_present() {
        let root = std::env::temp_dir().join(format!("meraki-app-gst-{}", std::process::id()));
        let plugin_dir = root.join("lib").join("gstreamer-1.0");
        let scanner_dir = root.join("libexec").join("gstreamer-1.0");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::create_dir_all(&scanner_dir).unwrap();

        let scanner_name = if cfg!(windows) {
            "gst-plugin-scanner.exe"
        } else {
            "gst-plugin-scanner"
        };
        let scanner_file = scanner_dir.join(scanner_name);
        fs::write(&scanner_file, b"").unwrap();

        let resolved = resolve_bundled_gstreamer(&root).unwrap();
        assert_eq!(resolved.plugin_dir, plugin_dir);
        assert_eq!(resolved.scanner_path, Some(scanner_file));

        let empty_root = root.join("empty");
        assert!(resolve_bundled_gstreamer(&empty_root).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bundled_env_respects_existing_user_overrides() {
        let bundle = BundledGStreamer {
            plugin_dir: PathBuf::from("bundle/lib/gstreamer-1.0"),
            scanner_path: Some(PathBuf::from("bundle/libexec/gstreamer-1.0/gst-plugin-scanner")),
        };

        let mut env = HashMap::<String, OsString>::new();
        env.insert(
            "GST_PLUGIN_SYSTEM_PATH_1_0".to_string(),
            OsString::from("custom/plugins"),
        );

        apply_bundled_gstreamer_with(&bundle, |k, v| {
            env.entry(k.to_string()).or_insert_with(|| v.to_os_string());
        });

        assert_eq!(
            env.get("GST_PLUGIN_SYSTEM_PATH_1_0"),
            Some(&OsString::from("custom/plugins"))
        );
        assert_eq!(
            env.get("GST_PLUGIN_SCANNER"),
            Some(&OsString::from("bundle/libexec/gstreamer-1.0/gst-plugin-scanner"))
        );
    }
}
