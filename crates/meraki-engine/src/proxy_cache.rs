use meraki_core::{AssetId, MediaMetadata};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tracing::debug;

const PROXY_MAX_W: u32 = 1280;
const PROXY_MAX_H: u32 = 720;
const MIN_VALID_PROXY_BYTES: u64 = 1024;

/// Retorna `true` apenas para vídeos (não imagens) acima de `1280x720`.
pub(crate) fn needs_proxy(path: &Path, metadata: &MediaMetadata) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    if matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg" | "webp")) {
        return false;
    }
    match (metadata.width, metadata.height) {
        (Some(w), Some(h)) => w > PROXY_MAX_W || h > PROXY_MAX_H,
        _ => false,
    }
}

/// Calcula resolução alvo proporcional dentro de `1280x720` alinhada a múltiplos pares `>= 16`.
pub(crate) fn proxy_target_dimensions(src_w: u32, src_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (PROXY_MAX_W, PROXY_MAX_H);
    }
    let scale = (f64::from(PROXY_MAX_W) / f64::from(src_w))
        .min(f64::from(PROXY_MAX_H) / f64::from(src_h))
        .min(1.0);
    let align_even = |val: f64, max: u32| -> u32 {
        let rounded = (val.round() as u32).clamp(16, max);
        (rounded & !1).max(16)
    };
    (
        align_even(f64::from(src_w) * scale, PROXY_MAX_W),
        align_even(f64::from(src_h) * scale, PROXY_MAX_H),
    )
}

pub(crate) fn proxy_cache_dir() -> PathBuf {
    std::env::temp_dir().join("meraki-proxies")
}

pub(crate) fn proxy_output_path(asset_id: AssetId) -> PathBuf {
    proxy_cache_dir().join(format!("{asset_id}_720p.mp4"))
}

pub(crate) fn source_fingerprint(path: &Path, size_bytes: u64) -> String {
    let norm = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("{}|{size_bytes}", norm.to_string_lossy())
}

pub(crate) fn source_cache_pointer(dir: &Path, path: &Path, size_bytes: u64) -> PathBuf {
    let fp = source_fingerprint(path, size_bytes);
    let mut h = DefaultHasher::new();
    fp.hash(&mut h);
    dir.join(format!("src_{:016x}.cache", h.finish()))
}

fn meta_path_for(output_path: &Path) -> PathBuf {
    output_path.with_extension("meta")
}

fn is_valid_proxy_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > MIN_VALID_PROXY_BYTES)
}

/// Reaproveita um proxy 720p já existente em disco para o mesmo arquivo e tamanho.
pub(crate) fn try_reuse_cached_proxy(
    output_path: &Path,
    source_pointer: &Path,
    fingerprint: &str,
) -> Option<PathBuf> {
    if is_valid_proxy_file(output_path)
        && fs::read_to_string(meta_path_for(output_path)).ok().as_deref() == Some(fingerprint)
    {
        return Some(output_path.to_path_buf());
    }
    let cached_text = fs::read_to_string(source_pointer).ok()?;
    let cached_path = PathBuf::from(cached_text.trim());
    if !is_valid_proxy_file(&cached_path) {
        debug!(path = %cached_path.display(), "entrada de cache de proxy ausente ou inválida");
        return None;
    }
    if cached_path != output_path {
        let _ = fs::remove_file(output_path);
        if fs::hard_link(&cached_path, output_path).is_err() {
            fs::copy(&cached_path, output_path).ok()?;
        }
        let _ = fs::write(meta_path_for(output_path), fingerprint);
    }
    Some(output_path.to_path_buf())
}

/// Promove arquivo temporário `.part.mp4` para destino final e atualiza o índice em disco.
pub(crate) fn finalize_proxy_files(
    part_path: &Path,
    output_path: &Path,
    source_pointer: &Path,
    fingerprint: &str,
) -> std::io::Result<()> {
    let _ = fs::remove_file(output_path);
    if fs::rename(part_path, output_path).is_err() {
        fs::copy(part_path, output_path)?;
        let _ = fs::remove_file(part_path);
    }
    fs::write(meta_path_for(output_path), fingerprint)?;
    fs::write(source_pointer, output_path.to_string_lossy().as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_proxy_and_target_dimensions_respect_720p_threshold_and_even_alignment() {
        let hd_meta = MediaMetadata { width: Some(1280), height: Some(720), ..MediaMetadata::default() };
        let fhd_meta = MediaMetadata { width: Some(1920), height: Some(1080), ..MediaMetadata::default() };
        let portrait_meta = MediaMetadata { width: Some(1080), height: Some(1920), ..MediaMetadata::default() };

        assert!(!needs_proxy(Path::new("clip.mp4"), &hd_meta));
        assert!(needs_proxy(Path::new("clip.mp4"), &fhd_meta));
        assert!(needs_proxy(Path::new("vertical.mp4"), &portrait_meta));
        assert!(!needs_proxy(
            Path::new("foto_4k.png"),
            &MediaMetadata { width: Some(3840), height: Some(2160), ..MediaMetadata::default() }
        ));

        assert_eq!(proxy_target_dimensions(1920, 1080), (1280, 720));
        assert_eq!(proxy_target_dimensions(3840, 2160), (1280, 720));
        let (pw, ph) = proxy_target_dimensions(1080, 1920);
        assert!(pw <= 1280 && ph <= 720 && pw % 2 == 0 && ph % 2 == 0);
    }
}
