//! Ancestor-walk for `wyrd.toml`, bounded by the nearest `.git`
//! ancestor or `$HOME`, whichever comes first (Q6).

use std::path::{Path, PathBuf};

use crate::error::WyrdConfigError;

pub(crate) const FILENAME: &str = "wyrd.toml";

/// Walk ancestors of `start` looking for `wyrd.toml`. Stops at the
/// first ancestor that either contains a `.git` entry or equals the
/// user's `$HOME` (whichever is hit first). Returns `None` when no
/// file is found within the bounded region — not an error.
pub(crate) fn find_wyrd_toml(start: &Path) -> Result<Option<PathBuf>, WyrdConfigError> {
    let canonical = start.canonicalize().map_err(|e| WyrdConfigError::Io {
        message: format!("canonicalize failed: {e}"),
        path: start.to_path_buf(),
    })?;

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok());

    for ancestor in canonical.ancestors() {
        let candidate = ancestor.join(FILENAME);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        let is_git_root = ancestor.join(".git").exists();
        let is_home = home.as_deref().is_some_and(|h| h == ancestor);
        if is_git_root || is_home {
            return Ok(None);
        }
    }
    Ok(None)
}
