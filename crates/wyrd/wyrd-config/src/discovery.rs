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
        if ancestor == std::path::Path::new("/") {
            return Ok(None);
        }
        let is_git_root = ancestor.join(".git").exists();
        let is_home = home.as_deref().is_some_and(|h| h == ancestor);
        if is_git_root || is_home {
            return Ok(None);
        }
    }
    Ok(None)
}

#[cfg(test)]
#[allow(unsafe_code)]
mod discovery_tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::{WyrdConfig, WyrdConfigError};

    fn plant_git_marker(root: &std::path::Path) {
        fs::create_dir(root.join(".git")).unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn discovery_finds_in_cwd() {
        let dir = TempDir::new().unwrap();
        plant_git_marker(dir.path());
        fs::write(
            dir.path().join("wyrd.toml"),
            "[defaults]\nspace = \"prod\"\n",
        )
        .unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();
        assert!(cfg.root_path.is_some(), "expected to find wyrd.toml");
    }

    #[test]
    #[serial_test::serial]
    fn discovery_walks_three_ancestors_up_within_git_root() {
        let dir = TempDir::new().unwrap();
        plant_git_marker(dir.path());
        fs::write(
            dir.path().join("wyrd.toml"),
            "[defaults]\nspace = \"prod\"\n",
        )
        .unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();
        std::env::set_current_dir(&nested).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();
        assert!(
            cfg.root_path.is_some(),
            "expected to find wyrd.toml up the tree"
        );
    }

    #[test]
    #[serial_test::serial]
    fn discovery_missing_returns_empty() {
        let dir = TempDir::new().unwrap();
        plant_git_marker(dir.path());
        std::env::set_current_dir(dir.path()).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();
        assert!(
            cfg.root_path.is_none(),
            "should return empty when no wyrd.toml found"
        );
    }

    #[test]
    #[serial_test::serial]
    fn discovery_stops_at_git_boundary() {
        let outer = TempDir::new().unwrap();
        fs::write(outer.path().join("wyrd.toml"), "[defaults]\n").unwrap();

        let git_root = outer.path().join("inner-repo");
        fs::create_dir(&git_root).unwrap();
        plant_git_marker(&git_root);

        let cwd = git_root.join("subdir");
        fs::create_dir(&cwd).unwrap();
        std::env::set_current_dir(&cwd).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();
        assert!(
            cfg.root_path.is_none(),
            "walk must not cross .git boundary: {cfg:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn discovery_symlinked_dir_resolves() {
        let dir = TempDir::new().unwrap();
        plant_git_marker(dir.path());
        fs::write(dir.path().join("wyrd.toml"), "[defaults]\n").unwrap();

        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        #[cfg(not(unix))]
        {
            // Skip on non-unix: symlinks require elevated perms on Windows
            return;
        }
        std::env::set_current_dir(&link).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();
        assert!(cfg.root_path.is_some());
    }

    #[test]
    fn discovery_explicit_path_skips_walk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("wyrd.toml");
        fs::write(&path, "[defaults]\nspace = \"prod\"\n").unwrap();
        let cfg = WyrdConfig::load(Some(&path)).unwrap();
        assert_eq!(cfg.root_path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn discovery_explicit_relative_path_preserved() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("wyrd.toml"), "[defaults]\n").unwrap();
        let path = dir.path().join("wyrd.toml");
        let cfg = WyrdConfig::load(Some(&path)).unwrap();
        assert_eq!(cfg.root_path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn discovery_explicit_missing_path_errors() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("nope.toml");
        let err = WyrdConfig::load(Some(&missing)).unwrap_err();
        assert!(matches!(err, WyrdConfigError::Io { .. }), "got {err:?}");
    }

    #[test]
    #[serial_test::serial]
    fn discovery_stops_at_home_boundary() {
        let outer = TempDir::new().unwrap();
        fs::write(outer.path().join("wyrd.toml"), "[defaults]\n").unwrap();
        let home = outer.path().join("home");
        fs::create_dir(&home).unwrap();
        let cwd = home.join("project");
        fs::create_dir(&cwd).unwrap();

        let home_canonical = home.canonicalize().unwrap();
        let saved_home = std::env::var_os("HOME");
        // SAFETY: serial_test::serial ensures single-threaded access to env vars here.
        unsafe { std::env::set_var("HOME", &home_canonical) };
        std::env::set_current_dir(&cwd).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();

        // SAFETY: serial_test::serial ensures single-threaded access to env vars here.
        unsafe {
            match saved_home {
                Some(h) => std::env::set_var("HOME", h),
                None => std::env::remove_var("HOME"),
            }
        }

        assert!(
            cfg.root_path.is_none(),
            "walk must not cross HOME boundary: {cfg:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn discovery_finds_wyrd_toml_at_home() {
        let home = TempDir::new().unwrap();
        fs::write(
            home.path().join("wyrd.toml"),
            "[defaults]\nspace = \"home\"\n",
        )
        .unwrap();
        let cwd = home.path().join("project");
        fs::create_dir(&cwd).unwrap();

        let home_canonical = home.path().canonicalize().unwrap();
        let saved_home = std::env::var_os("HOME");
        // SAFETY: serial_test::serial ensures single-threaded access to env vars here.
        unsafe { std::env::set_var("HOME", &home_canonical) };
        std::env::set_current_dir(&cwd).unwrap();

        let cfg = WyrdConfig::load(None).unwrap();

        // SAFETY: serial_test::serial ensures single-threaded access to env vars here.
        unsafe {
            match saved_home {
                Some(h) => std::env::set_var("HOME", h),
                None => std::env::remove_var("HOME"),
            }
        }

        assert!(
            cfg.root_path.is_some(),
            "should find wyrd.toml placed at HOME: {cfg:?}"
        );
    }
}
