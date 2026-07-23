//! Workspace config discovery and merge (`wyrd.toml`).

use std::path::Path;

use wyrd_config::{WyrdConfig, apply_defaults as apply_config_defaults};
use wyrd_spec::ids::SpaceName;

use super::error::{Diagnostic, LoadError};
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
pub fn discover(entry_path: &Path) -> Result<WyrdConfig, LoadError> {
    let start_dir = if entry_path.is_dir() {
        entry_path
    } else {
        entry_path.parent().unwrap_or(entry_path)
    };
    WyrdConfig::discover_from(start_dir).map_err(|error| {
        LoadError::single(Diagnostic::config_load_failed(
            start_dir.to_path_buf(),
            error.to_string(),
        ))
    })
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
///
/// # Panics
///
/// Panics only if the hard-coded system fallback space violates the `SpaceName`
/// invariant.
pub fn apply_defaults(cards: &mut [AuthoredCard], config: &WyrdConfig) {
    for card in cards {
        apply_config_defaults(&mut card.metadata, &card.kind, config);
        if card.metadata.space.is_none() {
            card.metadata.space = Some(
                SpaceName::new("default")
                    .expect("invariant: system fallback space is a valid SpaceName"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use wyrd_spec::envelope::CardKind;

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
        assert!(config.kind_overrides.contains_key(&CardKind::Model));
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
