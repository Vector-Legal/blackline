//! On-disk Kalosm GGUF cache. RAM/VRAM are not here — those live on the
//! Kalosm worker thread and go away when the last `Llama` handle drops.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::AiError;

/// Override the cache directory. Tests set this so `--clear-cache` never
/// touches a real Kalosm install. When set, [`load`](super::model::load)
/// points Kalosm at the same path.
pub const CACHE_DIR_ENV: &str = "BLACKLINE_KALOSM_CACHE";

/// Where Kalosm puts downloaded GGUFs (`dirs::data_dir()/kalosm/cache`),
/// or [`CACHE_DIR_ENV`] when that is set.
pub fn cache_dir() -> Result<PathBuf, AiError> {
    if let Ok(raw) = std::env::var(CACHE_DIR_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let data = dirs::data_dir()
        .ok_or_else(|| AiError::usage("cannot locate Kalosm cache (no platform data directory)"))?;
    Ok(data.join("kalosm").join("cache"))
}

/// Result of [`clear_cache`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClearCacheReport {
    /// Directory that was removed, or would have been.
    pub path: String,
    /// Sum of file sizes before delete.
    pub bytes: u64,
    /// False when the directory was already missing.
    pub existed: bool,
}

/// Delete the Kalosm cache directory. Safe to call when it is already gone.
pub fn clear_cache() -> Result<ClearCacheReport, AiError> {
    clear_cache_at(&cache_dir()?)
}

/// Delete `path` if it looks like a cache directory, not `$HOME` / `/`.
pub fn clear_cache_at(path: &Path) -> Result<ClearCacheReport, AiError> {
    refuse_if_too_broad(path)?;
    if !path.exists() {
        return Ok(ClearCacheReport {
            path: path.display().to_string(),
            bytes: 0,
            existed: false,
        });
    }
    if !path.is_dir() {
        return Err(AiError::usage(format!(
            "refusing to clear {}: not a directory",
            path.display()
        )));
    }
    let bytes = dir_size(path).map_err(|e| AiError::io_path(path, e))?;
    fs::remove_dir_all(path).map_err(|e| AiError::io_path(path, e))?;
    Ok(ClearCacheReport {
        path: path.display().to_string(),
        bytes,
        existed: true,
    })
}

/// Human line for `--clear-cache` without `--json`.
pub fn format_clear_report(report: &ClearCacheReport) -> String {
    if report.existed {
        format!("cleared {}  {}", format_bytes(report.bytes), report.path)
    } else {
        format!("already empty  {}", report.path)
    }
}

fn refuse_if_too_broad(path: &Path) -> Result<(), AiError> {
    if path.components().count() < 3 {
        return Err(AiError::usage(format!(
            "refusing to clear {}: path is too broad",
            path.display()
        )));
    }
    if let Some(home) = dirs::home_dir() {
        if path == home {
            return Err(AiError::usage(format!(
                "refusing to clear {}: that is the home directory",
                path.display()
            )));
        }
    }
    if let Some(data) = dirs::data_dir() {
        if path == data {
            return Err(AiError::usage(format!(
                "refusing to clear {}: that is the platform data directory",
                path.display()
            )));
        }
    }
    Ok(())
}

fn dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let meta = match current.symlink_metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            for entry in fs::read_dir(&current)? {
                stack.push(entry?.path());
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

fn format_bytes(n: u64) -> String {
    const KB: f64 = 1000.0;
    const MB: f64 = KB * 1000.0;
    const GB: f64 = MB * 1000.0;
    if n as f64 >= GB {
        format!("{:.1} GB", n as f64 / GB)
    } else if n as f64 >= MB {
        format!("{:.1} MB", n as f64 / MB)
    } else if n as f64 >= KB {
        format!("{:.1} KB", n as f64 / KB)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_cache_at, format_bytes, refuse_if_too_broad};
    use std::fs;
    use std::path::Path;

    #[test]
    fn refuse_root_and_shallow_paths() {
        assert!(refuse_if_too_broad(Path::new("/")).is_err());
        assert!(refuse_if_too_broad(Path::new("/tmp")).is_err());
        assert!(refuse_if_too_broad(Path::new("/tmp/blackline-kalosm-cache")).is_ok());
    }

    #[test]
    fn clear_missing_is_ok() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing-cache");
        let report = clear_cache_at(&path).unwrap();
        assert!(!report.existed);
        assert_eq!(report.bytes, 0);
    }

    #[test]
    fn clear_removes_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = dir.path().join("kalosm").join("cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("phi.gguf"), vec![0_u8; 2048]).unwrap();
        let report = clear_cache_at(&cache).unwrap();
        assert!(report.existed);
        assert_eq!(report.bytes, 2048);
        assert!(!cache.exists());
    }

    #[test]
    fn format_bytes_uses_decimal() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
    }
}
