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

    /// Per-kind overrides. Keys are PascalCase CardKind variants
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

    // `version` is intentionally absent. Filling `metadata.version`
    // from the loader silently changes which auto-bump branch the
    // server takes (None / Scope / Pin are three distinct intents).
    // L7 in the overview locks this; Q5 documents the rationale and
    // the `bump_intent` follow-up that re-enables defaulting.
    //
    // `name` is intentionally absent. `deny_unknown_fields` surfaces
    // `[defaults] name = "..."` as WYRD_CFG_400_SCHEMA_MISMATCH; the
    // parser additionally maps that specific case to
    // WYRD_CFG_400_NAME_DEFAULT_REJECTED for a clearer message.
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
        let resolved_path = match start {
            Some(explicit) => {
                if !explicit.is_file() {
                    return Err(WyrdConfigError::Io {
                        message: "file not found".to_string(),
                        path: explicit.to_path_buf(),
                    });
                }
                explicit.to_path_buf()
            }
            None => {
                let cwd = std::env::current_dir().map_err(WyrdConfigError::CwdRead)?;
                match crate::discovery::find_wyrd_toml(&cwd)? {
                    Some(p) => p,
                    None => return Ok(Self::empty()),
                }
            }
        };

        parse_file(&resolved_path)
    }
}

fn parse_file(path: &Path) -> Result<WyrdConfig, WyrdConfigError> {
    let text = std::fs::read_to_string(path).map_err(|e| WyrdConfigError::Io {
        message: e.to_string(),
        path: path.to_path_buf(),
    })?;

    let value: toml::Value = text.parse().map_err(|e: toml::de::Error| {
        WyrdConfigError::TomlParse {
            message: e.to_string(),
            path: path.to_path_buf(),
        }
    })?;

    if let Some(table) = scan_for_name_default(&value) {
        return Err(WyrdConfigError::NameDefaultRejected { table });
    }

    let mut cfg: WyrdConfig = value
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

    if let Some(defaults) = root.get("defaults").and_then(|v| v.as_table()) {
        if defaults.contains_key("name") {
            return Some("[defaults]".to_string());
        }
    }

    if let Some(kind) = root.get("kind").and_then(|v| v.as_table()) {
        for (key, inner) in kind.iter() {
            if let Some(table) = inner.as_table() {
                if table.contains_key("name") {
                    return Some(format!("[kind.{key}]"));
                }
            }
        }
    }

    None
}
