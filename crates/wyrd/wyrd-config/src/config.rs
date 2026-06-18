//! TODO(commit 04): TOML schema types and load entry point.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::SpaceName;
use wyrd_spec::metadata::{Annotations, Labels};

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
    /// Resolved absolute path.
    #[serde(skip)]
    pub root_path: Option<std::path::PathBuf>,
}

impl WyrdConfig {
    /// Empty config.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// `[defaults]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Default `metadata.space`.
    pub space: Option<SpaceName>,
    /// Default labels.
    #[serde(default)]
    pub labels: Labels,
    /// Default annotations.
    #[serde(default)]
    pub annotations: Annotations,
}

/// `[kind.<Kind>]` override table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindOverride {
    /// Per-kind override for `metadata.space`.
    pub space: Option<SpaceName>,
    /// Per-kind labels.
    #[serde(default)]
    pub labels: Labels,
    /// Per-kind annotations.
    #[serde(default)]
    pub annotations: Annotations,
}
