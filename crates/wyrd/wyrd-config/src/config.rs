//! TOML schema types and the `load` entry point.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wyrd_semver::{VersionBump, VersionSpec};
use wyrd_spec::card::trigger::TriggerSpec;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, RepositoryId};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{InlineableRef, Ref};

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

    /// `[repository]` table: the checkout's inferred Repository Card, the
    /// root `wyrd declare wyrd.toml` declares. Absent when this file only
    /// supplies defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryConfig>,

    /// Resolved absolute path of the discovered `wyrd.toml`.
    /// `None` when the config was synthesized via `empty()`.
    #[serde(skip)]
    pub root_path: Option<PathBuf>,
}

/// `[defaults]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
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
    /// Per-kind labels merged per-key into `metadata.labels`.
    #[serde(default)]
    pub labels: Labels,
    /// Per-kind annotations merged per-key into `metadata.annotations`.
    #[serde(default)]
    pub annotations: Annotations,
}

/// `[repository]` table: an inferred Repository Card and its review policy.
///
/// Identity comes from the checkout's origin remote unless `spec`
/// overrides it; the server verifies either against the tenant's GitHub
/// connection. `metadata` and `spec` hold overrides only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfig {
    /// `[repository.metadata]` overrides of the inferred Card metadata.
    #[serde(default)]
    pub metadata: RepositoryMetadata,
    /// `[repository.spec]` overrides of the inferred provider identity.
    #[serde(default)]
    pub spec: RepositoryIdentity,
    /// `[repository.verified_by]`: the complete desired review policy.
    pub verified_by: RepositoryBindings,
}

/// `[repository.metadata]`: explicit Card metadata that wins over inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryMetadata {
    /// Card name; inferred from the repository name when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<CardName>,
    /// Version scope or exact pin; the server assigns one when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionSpec>,
    /// Bump level for the automatic version paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<VersionBump>,
    /// Labels, merged with workspace defaults like any Card's.
    #[serde(default)]
    pub labels: Labels,
    /// Annotations, merged with workspace defaults like any Card's.
    #[serde(default)]
    pub annotations: Annotations,
}

/// `[repository.spec]`: explicit provider identity, still verified.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryIdentity {
    /// Provider web host; the configured host when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Stable provider repository id; resolved from the remote when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<RepositoryId>,
}

/// `[repository.verified_by]`: Verifiers bound to the Repository.
///
/// Each `verifier` entry expands into one ordinary verification binding
/// sharing `runs_on`. The list is complete: `verifier = []` binds none.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryBindings {
    /// Local Verifier paths or exact registered Verifier references.
    pub verifier: Vec<Ref>,
    /// Activation shared by every binding, such as `{ kind = "pull_request" }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs_on: Option<InlineableRef<TriggerSpec>>,
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

    use crate::{WyrdConfig, WyrdConfigError};

    fn parse(toml: &str) -> Result<WyrdConfig, WyrdConfigError> {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(toml.as_bytes()).unwrap();
        WyrdConfig::load(Some(tmp.path()))
    }

    /// An empty `wyrd.toml` yields no label defaults and no kind overrides.
    ///
    /// # Panics
    /// Panics when the temporary file cannot be written, the empty file fails
    /// to load, or the config carries a default.
    #[test]
    fn parse_empty_file_yields_empty_config() {
        let cfg = parse("").unwrap();
        assert!(cfg.defaults.labels.is_empty());
        assert!(cfg.kind_overrides.is_empty());
    }

    /// Card space is no longer a config default: `[defaults]` and
    /// `[kind.<Kind>]` refuse it by schema, while label and annotation
    /// defaults still parse.
    ///
    /// # Panics
    /// Panics when the temporary file cannot be written, a `space` default
    /// loads, its refusal does not name `space`, or the map defaults fail.
    #[test]
    fn removed_space_defaults_are_rejected() {
        for toml in [
            "[defaults]\nspace = \"prod\"\n",
            "[kind.Model]\nspace = \"mls\"\n",
        ] {
            let err = parse(toml).unwrap_err();
            assert!(
                matches!(&err, WyrdConfigError::Schema { message, .. } if message.contains("unknown field `space`")),
                "got {err:?}"
            );
        }

        let cfg = parse(
            "[defaults.labels]\nteam = \"ml\"\n[kind.Model.annotations]\n\"acme.com/owner\" = \"risk\"\n",
        )
        .unwrap();
        assert_eq!(cfg.defaults.labels.len(), 1);
        assert_eq!(cfg.kind_overrides[&CardKind::Model].annotations.len(), 1);
    }

    /// The minimal `[repository.verified_by]` table parses, with a local
    /// path and an exact registered reference, and the shared activation.
    ///
    /// # Panics
    /// Panics when the minimal table or its overrides do not parse.
    #[test]
    fn repository_table_parses_minimal_policy_and_overrides() {
        use wyrd_spec::reference::{InlineableRef, Ref};

        let cfg = parse(
            r#"
[repository.metadata]
name = "widget"
version = "1.2.0"

[repository.spec]
repository_id = "42"

[repository.verified_by]
verifier = ["./review/code-review.yaml", { kind = "Verifier", name = "shared", version = "1.0.0" }]
runs_on = { kind = "pull_request" }
"#,
        )
        .unwrap();
        let repository = cfg.repository.unwrap();
        assert_eq!(repository.metadata.name.unwrap().as_str(), "widget");
        assert_eq!(repository.spec.repository_id.unwrap().as_str(), "42");
        assert!(matches!(repository.verified_by.verifier[0], Ref::Path(_)));
        assert!(matches!(repository.verified_by.verifier[1], Ref::Ref(_)));
        assert!(matches!(
            repository.verified_by.runs_on,
            Some(InlineableRef::Inline(_))
        ));
        let empty = parse("[repository.verified_by]\nverifier = []\n").unwrap();
        assert!(empty.repository.unwrap().verified_by.verifier.is_empty());
    }

    /// The binding list is required and every table is closed.
    ///
    /// # Panics
    /// Panics when an incomplete or unknown repository table parses.
    #[test]
    fn repository_table_requires_the_policy_and_refuses_unknown_fields() {
        for toml in [
            "[repository.metadata]\nname = \"widget\"\n",
            "[repository.verified_by]\nverifier = []\nevents = [\"push\"]\n",
            "[repository.spec]\nfull_name = \"acme/widget\"\n[repository.verified_by]\nverifier = []\n",
            "[repository.verified_by]\nverifier = []\nruns_on = { kind = \"push\" }\n",
        ] {
            let err = parse(toml).unwrap_err();
            assert!(
                matches!(err, WyrdConfigError::Schema { .. }),
                "{toml}: {err:?}"
            );
        }
    }

    #[test]
    fn parse_unknown_top_level_table_rejected() {
        let err = parse("[bogus]\nk = \"v\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_typo_default_singular_rejected() {
        let err = parse("[default.labels]\nteam = \"ml\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_invalid_label_key_rejected() {
        let err = parse("[defaults.labels]\n\"BAD KEY\" = \"v\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_lowercase_kind_key_rejected() {
        let err = parse("[kind.model.labels]\nteam = \"ml\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_unknown_kind_rejected() {
        let err = parse("[kind.NotAKind.labels]\nteam = \"ml\"\n").unwrap_err();
        assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
    }

    #[test]
    fn parse_external_kind_rejected() {
        let toml = "[kind.External.labels]\nteam = \"ml\"\n";
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
        let err = parse("[defaults\nlabels = {}\n").unwrap_err();
        assert!(
            matches!(err, WyrdConfigError::TomlParse { .. }),
            "got {err:?}"
        );
    }
}
