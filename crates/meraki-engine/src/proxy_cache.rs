use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

const SUFFIX: &str = "_720p.mp4";
const FINGERPRINT_LEN: usize = 32;

pub(crate) fn cache_dirs() -> Vec<PathBuf> {
    let configured = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(|root| {
            PathBuf::from(root)
                .join("Meraki")
                .join("cache")
                .join("proxies")
        })
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .map(|root| root.join("meraki").join("proxies"))
    };
    let fallback = env::temp_dir().join("meraki-proxies");
    configured
        .filter(|dir| dir.is_absolute() && *dir != fallback)
        .into_iter()
        .chain([fallback])
        .collect()
}

pub(crate) fn fingerprint(source: &Path) -> io::Result<String> {
    let path = fs::canonicalize(source)?;
    let meta = fs::metadata(&path)?;
    let mtime = match meta.modified()?.duration_since(UNIX_EPOCH) {
        Ok(delta) => delta.as_nanos() as i128,
        Err(delta) => -(delta.duration().as_nanos() as i128),
    };
    let mut data = path.as_os_str().as_encoded_bytes().to_vec();
    data.extend_from_slice(&meta.len().to_le_bytes());
    data.extend_from_slice(&mtime.to_le_bytes());
    glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &data)
        .and_then(|digest| {
            digest
                .get(..FINGERPRINT_LEN)
                .map(|hash| format!("{hash}{SUFFIX}"))
        })
        .ok_or_else(|| io::Error::other("SHA-256 indisponível"))
}

pub(crate) fn locate(name: &str) -> Option<PathBuf> {
    cache_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|path| valid_proxy(path))
}

pub(crate) fn valid_proxy(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
}

pub(crate) fn in_cache(path: &Path) -> bool {
    path.parent()
        .is_some_and(|dir| cache_dirs().iter().any(|cache| cache == dir))
}

pub(crate) fn is_managed(path: &Path) -> bool {
    let has_managed_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(SUFFIX))
        .is_some_and(|hash| {
            hash.len() == FINGERPRINT_LEN && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    has_managed_name && in_cache(path)
}

pub(crate) fn reserve_output(name: &str) -> io::Result<(PathBuf, PartialFile)> {
    reserve_in_dirs(name, cache_dirs().into_iter())
}

fn reserve_in_dirs(
    name: &str,
    dirs: impl Iterator<Item = PathBuf>,
) -> io::Result<(PathBuf, PartialFile)> {
    let mut result = Err(io::Error::other("diretório de proxy indisponível"));
    for dir in dirs {
        let output = dir.join(name);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            // As versões do filesrc/filesink para MSVC podem usar o limite MAX_PATH.
            if output.as_os_str().encode_wide().count() >= 260 {
                result = Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "caminho de proxy excede MAX_PATH",
                ));
                continue;
            }
        }
        match fs::create_dir_all(&dir).and_then(|()| PartialFile::reserve(&output)) {
            Ok(partial) => return Ok((output, partial)),
            Err(error) => {
                result = Err(io::Error::new(
                    error.kind(),
                    format!("{}: {error}", dir.display()),
                ))
            }
        }
    }
    result
}

pub(crate) struct PartialFile(PathBuf);

impl PartialFile {
    fn reserve(output: &Path) -> io::Result<Self> {
        let path = output.with_file_name(format!("{}.part", Uuid::new_v4().simple()));
        fs::File::create_new(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn publish(self, output: PathBuf, name: &str) -> io::Result<PathBuf> {
        match self.commit(&output) {
            Ok(()) => Ok(output),
            Err(original) => {
                // Uma ACL pode permitir gravar o arquivo temporário e recusar sua renomeação.
                // Reaproveita o conteúdo codificado e publica atomicamente no outro cache.
                let dirs = cache_dirs()
                    .into_iter()
                    .filter(|dir| Some(dir.as_path()) != output.parent());
                let (fallback, partial) = reserve_in_dirs(name, dirs).map_err(|_| original)?;
                fs::copy(self.path(), partial.path())?;
                partial.commit(&fallback)?;
                Ok(fallback)
            }
        }
    }

    fn commit(&self, output: &Path) -> io::Result<()> {
        match fs::rename(&self.0, output) {
            Ok(()) => Ok(()),
            Err(_) if valid_proxy(output) => Ok(()),
            Err(error) => Err(io::Error::new(
                error.kind(),
                format!("publicar {}: {error}", output.display()),
            )),
        }
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn clear(originals: &[PathBuf]) -> (u64, Option<String>) {
    let originals: std::collections::HashSet<_> = originals
        .iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect();
    let mut freed = 0_u64;
    let mut error = None;
    for dir in cache_dirs() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => {
                error = Some(format!("{}: {err}", dir.display()));
                continue;
            }
        };
        for entry in entries {
            let result = entry.and_then(|entry| {
                let path = entry.path();
                if !is_managed(&path)
                    || !entry.file_type()?.is_file()
                    || fs::canonicalize(&path).is_ok_and(|path| originals.contains(&path))
                {
                    return Ok(0);
                }
                let size = entry.metadata()?.len();
                fs::remove_file(path)?;
                Ok(size)
            });
            match result {
                Ok(bytes) => freed = freed.saturating_add(bytes),
                Err(err) => error = Some(format!("{}: {err}", dir.display())),
            }
        }
    }
    (freed, error)
}

/// Resolução codificada dentro do quadro 1280 × 720, com dimensões pares para H.264.
pub(crate) fn target_dimensions(width: u32, height: u32) -> (u32, u32) {
    let scale = (1280.0 / f64::from(width))
        .min(720.0 / f64::from(height))
        .min(1.0);
    let align = |value: f64| ((value.round() as u32).max(16) & !1).max(16);
    (
        align(f64::from(width) * scale),
        align(f64::from(height) * scale),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn fingerprint_changes_when_only_original_mtime_changes() {
        let input = crate::discoverer::tests::TempFileGuard::new("proxy_fingerprint", "mp4");
        fs::write(&input.0, b"meraki").unwrap();
        let file = fs::File::options().write(true).open(&input.0).unwrap();
        let original = SystemTime::now() - Duration::from_secs(60);
        file.set_modified(original).unwrap();
        let fingerprint_before = fingerprint(&input.0).unwrap();
        file.set_modified(original + Duration::from_secs(10))
            .unwrap();
        assert_ne!(fingerprint(&input.0).unwrap(), fingerprint_before);
        assert_eq!(fs::read(&input.0).unwrap(), b"meraki");
    }
}
