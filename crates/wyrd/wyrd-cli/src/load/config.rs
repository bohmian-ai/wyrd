//! Workspace config discovery and merge (`wyrd.toml`).

use std::path::Path;

use super::error::LoadError;
use super::parse::AuthoredCard;

/// Discover `wyrd.toml` via ancestor walk from the given path's parent directory.
///
/// Walks from the parent of `entry_path` upward (since `entry_path` is typically
/// a file, not a directory), stopping at the first `wyrd.toml` found or at the
/// nearest `.git` ancestor or `$HOME`, whichever comes first.
///
/// Returns empty config (space = "default", empty maps) when no config is found;
/// this is not an error.
///
/// # Errors
///
/// Returns diagnostic only when a `wyrd.toml` file exists but is malformed.
pub fn discover(entry_path: &Path) -> Result<wyrd_config::WyrdConfig, LoadError> {
    let start_dir = entry_path.parent().unwrap_or(entry_path);
    let canonical_start = start_dir.canonicalize().map_err(|error| {
        LoadError::single(super::error::Diagnostic::io(start_dir.to_path_buf(), error))
    })?;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .and_then(|path| path.canonicalize().ok());

    for ancestor in canonical_start.ancestors() {
        let candidate = ancestor.join("wyrd.toml");
        if candidate.is_file() {
            return wyrd_config::WyrdConfig::load(Some(&candidate)).map_err(|error| {
                LoadError::single(super::error::Diagnostic::config_load_failed(
                    candidate,
                    error.to_string(),
                ))
            });
        }
        if ancestor == std::path::Path::new("/")
            || ancestor.join(".git").exists()
            || home.as_deref().is_some_and(|home| home == ancestor)
        {
            break;
        }
    }

    Ok(wyrd_config::WyrdConfig::empty())
}

/// Apply workspace defaults to every authored card.
///
/// Merges config values using authority precedence:
/// 1. Value explicit in the card YAML (highest)
/// 2. `[kind.<Kind>]` table value
/// 3. `[defaults]` table value
/// 4. System fallback (lowest)
///
/// Runs before resolve/validate/order.
pub fn apply_defaults(cards: &mut [AuthoredCard], config: &wyrd_config::WyrdConfig) {
    for card in cards {
        wyrd_config::apply_defaults(&mut card.metadata, &card.kind, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn config_absent_uses_system_defaults() {
        let temp = TempDir::new().unwrap();
        let entry_file = temp.path().join("test.yaml");
        std::fs::write(&entry_file, "").unwrap();

        let config = discover(&entry_file).unwrap();
        assert!(config.defaults.space.is_none());
        assert!(config.defaults.labels.is_empty());
        assert!(config.defaults.annotations.is_empty());
    }

    #[test]
    fn config_precedence_is_handled_by_apply_defaults() {
        // This is tested in wyrd-config's own tests; loader just calls through
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("wyrd.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[defaults]
space = "prod"

[kind.Model]
space = "ml-prod"
"#
        )
        .unwrap();
        file.flush().unwrap();

        let entry_file = temp.path().join("test.yaml");
        std::fs::write(&entry_file, "").unwrap();

        let config = discover(&entry_file).unwrap();
        assert!(config.defaults.space.is_some());
        assert!(
            config
                .kind_overrides
                .contains_key(&wyrd_spec::envelope::CardKind::Model)
        );
    }

    #[test]
    fn config_rejects_name_default() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("wyrd.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[defaults]
name = "foo"
"#
        )
        .unwrap();
        file.flush().unwrap();

        let entry_file = temp.path().join("test.yaml");
        std::fs::write(&entry_file, "").unwrap();

        let result = discover(&entry_file);
        assert!(result.is_err());
    }
}
