//! TOML schema types and the `load` entry point.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::SpaceName;
use wyrd_spec::metadata::{Annotations, Labels};

use crate::error::WyrdConfigError;

/// Wyrd workspace configuration loaded from `wyrd.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WyrdConfig {
    /// Workspace-wide defaults applied to every card unless overridden.
    #[serde(default)]
    pub defaults: Defaults,

    /// Per-kind overrides. Keys are `PascalCase` `CardKind` variants
    /// **excluding `External`** (rejected post-deserialize).
    #[serde(default, rename = "kind")]
    pub kind_overrides: BTreeMap<CardKind, KindOverride>,

    /// Resolved absolute path of the discovered `wyrd.toml`.
    /// `None` when the config was synthesized via `empty()`.
    #[serde(skip)]
    pub root_path: Option<PathBuf>,
}

/// `[defaults]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Default `metadata.space` for any card that omits it.
    pub space: Option<SpaceName>,

    /// Default labels merged per-key into `metadata.labels`.
    #[serde(default)]
    pub labels: Labels,

    /// Default annotations merged per-key into `metadata.annotations`.
    #[serde(default)]
    pub annotations: Annotations,
}

/// `[kind.<Kind>]` override table.
///
/// Same fields as `Defaults`. Per-kind `version` is intentionally
/// out of scope alongside the workspace-level `version` field (Q5).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindOverride {
    /// Per-kind override for `metadata.space`.
    pub space: Option<SpaceName>,
    /// Per-kind labels merged per-key into `metadata.labels`.
    #[serde(default)]
    pub labels: Labels,
    /// Per-kind annotations merged per-key into `metadata.annotations`.
    #[serde(default)]
    pub annotations: Annotations,
}

impl WyrdConfig {
    /// Empty config — equivalent to "no wyrd.toml on disk."
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load `wyrd.toml`.
    ///
    /// - `start = Some(path)`: treat as the explicit config path. The
    ///   file must exist; missing → `WyrdConfigError::Io`. Relative
    ///   paths are preserved as given (the ancestor walk does not run).
    /// - `start = None`: ancestor-walk from CWD, bounded by the
    ///   nearest `.git` ancestor or `$HOME`, whichever comes first.
    ///   Missing → returns `WyrdConfig::empty()` (not an error).
    #[tracing::instrument(fields(start = ?start))]
    pub fn load(start: Option<&Path>) -> Result<Self, WyrdConfigError> {
        let resolved_path = if let Some(explicit) = start {
            if !explicit.is_file() {
                return Err(WyrdConfigError::Io {
                    message: "file not found".to_string(),
                    path: explicit.to_path_buf(),
                });
            }
            explicit.to_path_buf()
        } else {
            let cwd = std::env::current_dir().map_err(WyrdConfigError::CwdRead)?;
            match crate::discovery::find_wyrd_toml(&cwd)? {
                Some(p) => p,
                None => return Ok(Self::empty()),
            }
        };

        parse_file(&resolved_path)
    }

    /// Discover `wyrd.toml` by walking ancestors from an explicit directory.
    ///
    /// This preserves the same `.git`/home boundary as [`WyrdConfig::load`]
    /// without depending on the process working directory.
    ///
    /// # Errors
    /// Returns an error when the start directory cannot be canonicalized or a
    /// discovered config cannot be read or parsed.
    pub fn discover_from(start: &Path) -> Result<Self, WyrdConfigError> {
        match crate::discovery::find_wyrd_toml(start)? {
            Some(path) => parse_file(&path),
            None => Ok(Self::empty()),
        }
    }
}

fn parse_file(path: &Path) -> Result<WyrdConfig, WyrdConfigError> {
    let text = std::fs::read_to_string(path).map_err(|e| WyrdConfigError::Io {
        message: e.to_string(),
        path: path.to_path_buf(),
    })?;

    let value: toml::Value =
        text.parse()
            .map_err(|e: toml::de::Error| WyrdConfigError::TomlParse {
                message: e.to_string(),
                path: path.to_path_buf(),
            })?;

    if let Some(table) = scan_for_name_default(&value) {
        return Err(WyrdConfigError::NameDefaultRejected { table });
    }

    let mut cfg: WyrdConfig =
        value
            .try_into()
            .map_err(|e: toml::de::Error| WyrdConfigError::Schema {
                message: e.to_string(),
                path: path.to_path_buf(),
            })?;

    if cfg.kind_overrides.contains_key(&CardKind::External) {
        return Err(WyrdConfigError::Schema {
            message: "[kind.External] is reserved for the forward-compat wire catch-all and is not a valid override-table key".to_string(),
            path: path.to_path_buf(),
        });
    }

    cfg.root_path = Some(path.to_path_buf());
    Ok(cfg)
}

/// Returns the table label when a `name` key is set under it.
fn scan_for_name_default(value: &toml::Value) -> Option<String> {
    let root = value.as_table()?;

    if let Some(defaults) = root.get("defaults").and_then(|v| v.as_table())
        && defaults.contains_key("name")
    {
        return Some("[defaults]".to_string());
    }

    if let Some(kind) = root.get("kind").and_then(|v| v.as_table()) {
        for (key, inner) in kind {
            if let Some(table) = inner.as_table()
                && table.contains_key("name")
            {
                return Some(format!("[kind.{key}]"));
            }
        }
    }

    None
}

#[cfg(test)]
mod parse {
    use std::io::Write;

    use tempfile::NamedTempFile;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::SpaceName;

    use crate::{WyrdConfig, WyrdConfigError};

    fn parse(toml: &str) -> Result<WyrdConfig, WyrdConfigError> {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(toml.as_bytes()).unwrap();
        WyrdConfig::load(Some(tmp.path()))
    }

    #[test]
    fn parse_empty_file_yields_empty_config() {
        let cfg = parse("").unwrap();
        assert!(cfg.defaults.space.is_none());
        assert!(cfg.kind_overrides.is_empty());
    }

    #[test]
    fn parse_defaults_only() {
        let cfg = parse("[defaults]\nspace = \"prod\"\n").unwrap();
        assert_eq!(
            cfg.defaults.space.as_ref().map(SpaceName::as_str),
            Some("prod")
        );
        assert!(cfg.kind_overrides.is_empty());
    }

    #[test]
    fn parse_kind_only() {
        let cfg = parse("[kind.Model]\nspace = \"mls\"\n").unwrap();
        assert!(cfg.defaults.space.is_none());
        let ko = cfg.kind_overrides.get(&CardKind::Model).unwrap();
        assert_eq!(ko.space.as_ref().map(SpaceName::as_str), Some("mls"));
    }

    #[test]
    fn parse_defaults_and_kind() {
        let toml = "[defaults]\nspace = \"prod\"\n[kind.Model]\nspace = \"mls\"\n";
        let cfg = parse(toml).unwrap();
        assert_eq!(
            cfg.defaults.space.as_ref().map(SpaceName::as_str),
            Some("prod")
        );
        assert!(cfg.kind_overrides.contains_key(&CardKind::Model));
    }

    #[test]
    fn parse_unknown_top_level_table_rejected() {
        let err = parse("[bogus]\nk = \"v\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_typo_default_singular_rejected() {
        let err = parse("[default]\nspace = \"prod\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_invalid_space_value_rejected() {
        let err = parse("[defaults]\nspace = \"PROD\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_invalid_label_key_rejected() {
        let err = parse("[defaults.labels]\n\"BAD KEY\" = \"v\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_lowercase_kind_key_rejected() {
        let err = parse("[kind.model]\nspace = \"ml\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_unknown_kind_rejected() {
        let err = parse("[kind.NotAKind]\nspace = \"ml\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_external_kind_rejected() {
        let toml = "[kind.External]\nspace = \"mls\"\n";
        let err = parse(toml).unwrap_err();
        match &err {
            WyrdConfigError::Schema { message, .. } => {
                assert!(message.contains("[kind.External]"), "got {message:?}");
            }
            other => panic!("expected Schema, got {other:?}"),
        }
    }

    #[test]
    fn parse_name_in_defaults_is_rejected() {
        let toml = "[defaults]\nname = \"foo\"\n";
        let err = parse(toml).unwrap_err();
        assert!(
            matches!(
                &err,
                WyrdConfigError::NameDefaultRejected { table } if table == "[defaults]"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_name_in_kind_table_is_rejected() {
        let toml = "[kind.Model]\nname = \"foo\"\n";
        let err = parse(toml).unwrap_err();
        assert!(
            matches!(
                &err,
                WyrdConfigError::NameDefaultRejected { table } if table == "[kind.Model]"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_version_under_defaults_rejected() {
        let err = parse("[defaults]\nversion = \"1.0.0\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_syntax_error_returns_toml_parse_variant() {
        let err = parse("[defaults\nspace = \"prod\"\n").unwrap_err();
        assert!(
            matches!(err, WyrdConfigError::TomlParse { .. }),
            "got {err:?}"
        );
    }
}
