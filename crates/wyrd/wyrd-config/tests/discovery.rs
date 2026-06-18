use std::fs;

use tempfile::TempDir;
use wyrd_config::{WyrdConfig, WyrdConfigError};

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
