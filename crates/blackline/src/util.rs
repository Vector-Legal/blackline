//! Shared CLI helpers.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Read a JSON argument: inline, `@file`, or `-` (stdin).
pub fn read_json_arg(raw: &str) -> Result<String, String> {
    if raw == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("usage: failed to read stdin: {e}"))?;
        return Ok(buf);
    }
    if let Some(path) = raw.strip_prefix('@') {
        return std::fs::read_to_string(path)
            .map_err(|e| format!("usage: failed to read {path}: {e}"));
    }
    Ok(raw.to_string())
}

/// Resolve `-o` / `--in-place` to an output path. Returns `(path, is_inplace)`.
pub fn resolve_output(
    input: &Path,
    output: Option<&Path>,
    in_place: bool,
) -> Result<(PathBuf, bool), String> {
    match (output, in_place) {
        (Some(_), true) => Err("usage: use either -o/--output or --in-place, not both".into()),
        (None, false) => Err("usage: pass -o/--output PATH or --in-place".into()),
        (Some(p), false) => Ok((p.to_path_buf(), false)),
        (None, true) => Ok((input.to_path_buf(), true)),
    }
}

/// Atomic save: write to a sibling tempfile, then rename into place.
pub fn atomic_write(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(dir) = parent {
        std::fs::create_dir_all(dir).map_err(|e| format!("create dir {}: {e}", dir.display()))?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("out")
    ));
    write(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename {}: {e}", path.display())
    })
}

/// Author from `--author` or `BLACKLINE_AUTHOR`.
pub fn resolve_author(flag: Option<&str>) -> Result<String, String> {
    if let Some(a) = flag {
        if !a.trim().is_empty() {
            return Ok(a.to_string());
        }
    }
    if let Ok(a) = std::env::var("BLACKLINE_AUTHOR") {
        if !a.trim().is_empty() {
            return Ok(a);
        }
    }
    Err(
        "usage: author required: tracked changes and comments must carry an explicit author (pass --author or set BLACKLINE_AUTHOR)"
            .into(),
    )
}

/// Print JSON.
pub fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Slice a 1-based inclusive range.
pub fn slice_range<T>(items: &[T], from: Option<usize>, to: Option<usize>) -> &[T] {
    let start = from.unwrap_or(1).saturating_sub(1);
    let end = to.unwrap_or(items.len()).min(items.len());
    if start >= items.len() || start >= end {
        return &[];
    }
    &items[start..end]
}
