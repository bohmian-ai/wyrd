//! YAML parsing and envelope validation.

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::reference::CardRef;

use super::error::{Diagnostic, LoadError};
use super::path::PathSandbox;

/// An authored card carrying its source path and parsed envelope.
///
/// `metadata` uses the wire-format [`Metadata`] so downstream stages
/// (`wyrd-config::apply_defaults`, resolve, validate) operate on the
/// same type the server sees on the wire. Optional identity fields
/// (`space`, `version`, `labels`, `annotations`) are filled in from
/// `wyrd.toml` by `apply_defaults` before validate/order run.
#[derive(Debug, Clone)]
pub struct AuthoredCard {
    /// The file this card was loaded from.
    pub source_path: PathBuf,
    /// The card's API version.
    pub api_version: ApiVersion,
    /// The card's kind.
    pub kind: CardKind,
    /// The card's metadata envelope.
    pub metadata: Metadata,
    /// The card's spec body (kind-specific).
    pub spec: Spec,
    /// Heavy artifact manifest entries declared by this card.
    pub artifacts: Vec<wyrd_spec::registry::ArtifactManifestEntry>,
}

/// Raw envelope shape for parsing multi-doc YAML.
#[derive(Debug, Deserialize, Serialize)]
struct RawCardEnvelope {
    #[serde(rename = "apiVersion")]
    api_version: ApiVersion,
    kind: CardKind,
    metadata: Metadata,
    spec: serde_json::Value,
    #[serde(default)]
    artifacts: Vec<wyrd_spec::registry::ArtifactManifestEntry>,
}

/// Parse a YAML file containing one or more card envelopes.
///
/// Returns a vec of `AuthoredCard`s — one per `---`-separated document.
/// Collects every envelope violation across all docs; does not short-circuit.
pub fn parse_file(path: &Path) -> Result<Vec<AuthoredCard>, LoadError> {
    let root = path.parent().unwrap_or(Path::new("."));
    let sandbox = PathSandbox::new(root).map_err(LoadError::single)?;
    parse_file_with_sandbox(path, &sandbox)
}

pub(crate) fn parse_file_with_sandbox(
    path: &Path,
    sandbox: &PathSandbox,
) -> Result<Vec<AuthoredCard>, LoadError> {
    let source_path = path.to_path_buf();
    let canonical_path = sandbox
        .ensure_contained(path, path)
        .map_err(LoadError::single)?;
    let metadata = std::fs::metadata(&canonical_path)
        .map_err(|error| LoadError::single(Diagnostic::io(source_path.clone(), &error)))?;
    if !metadata.is_file() {
        return Err(LoadError::single(Diagnostic::invalid_envelope(
            source_path,
            "loader entry is not a regular file".to_owned(),
        )));
    }
    let bytes = std::fs::read(&canonical_path)
        .map_err(|error| LoadError::single(Diagnostic::io(source_path.clone(), &error)))?;

    let content = std::str::from_utf8(&bytes).map_err(|e| {
        LoadError::single(Diagnostic::yaml_syntax(
            source_path.clone(),
            &serde_yaml::Error::custom(format!("invalid UTF-8: {e}")),
        ))
    })?;

    let mut cards = Vec::new();
    let mut diagnostics = Vec::new();

    for raw_doc in serde_yaml::Deserializer::from_str(content) {
        match serde_yaml::Value::deserialize(raw_doc) {
            Ok(mut value) => {
                if let Err(diagnostic) =
                    materialize_inline_files(&mut value, &source_path, false, sandbox)
                {
                    diagnostics.push(diagnostic);
                    continue;
                }
                match parse_single_envelope(&source_path, value) {
                    Ok(card) => cards.push(card),
                    Err(diagnostic) => diagnostics.push(diagnostic),
                }
            }
            Err(error) => diagnostics.push(Diagnostic::yaml_syntax(source_path.clone(), &error)),
        }
    }

    if !diagnostics.is_empty() {
        return Err(LoadError::multiple(diagnostics));
    }

    Ok(cards)
}

/// Parse either one YAML file or every YAML file beneath a directory.
///
/// Directory entries are sorted by path before parsing so repeated loads have
/// identical input order on every supported filesystem.
pub fn parse_path(path: &Path) -> Result<Vec<AuthoredCard>, LoadError> {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(Path::new("."))
    };
    let sandbox = PathSandbox::new(root).map_err(LoadError::single)?;
    parse_path_with_sandbox(path, &sandbox)
}

pub(crate) fn parse_path_with_sandbox(
    path: &Path,
    sandbox: &PathSandbox,
) -> Result<Vec<AuthoredCard>, LoadError> {
    if path.is_file() {
        return parse_file_with_sandbox(path, sandbox);
    }
    if !path.is_dir() {
        let error = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "loader entry is neither a file nor a directory",
        );
        return Err(LoadError::single(Diagnostic::io(
            path.to_path_buf(),
            &error,
        )));
    }

    let mut files = Vec::new();
    collect_yaml_files(path, sandbox, &mut files)?;
    files.sort();
    let mut cards = Vec::new();
    let mut diagnostics = Vec::new();
    for file in files {
        match parse_file_with_sandbox(&file, sandbox) {
            Ok(mut parsed) => cards.append(&mut parsed),
            Err(error) => diagnostics.extend(error.diagnostics),
        }
    }
    if diagnostics.is_empty() {
        Ok(cards)
    } else {
        Err(LoadError::multiple(diagnostics))
    }
}

fn collect_yaml_files(
    path: &Path,
    sandbox: &PathSandbox,
    files: &mut Vec<PathBuf>,
) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(path)
        .map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), &error)))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), &error)))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| LoadError::single(Diagnostic::io(entry_path.clone(), &error)))?;
        if file_type.is_dir() {
            collect_yaml_files(&entry_path, sandbox, files)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
        {
            files.push(
                sandbox
                    .ensure_contained(&entry_path, &entry_path)
                    .map_err(LoadError::single)?,
            );
        }
    }
    Ok(())
}

fn parse_single_envelope(path: &Path, raw: serde_yaml::Value) -> Result<AuthoredCard, Diagnostic> {
    let envelope = serde_yaml::from_value::<RawCardEnvelope>(raw).map_err(|e| {
        // Check for missing required envelope fields
        let error_msg = e.to_string();
        if error_msg.contains("apiVersion") || error_msg.contains("api_version") {
            Diagnostic::invalid_envelope(
                path.into(),
                "Missing required field: apiVersion".to_string(),
            )
        } else if error_msg.contains("kind") {
            Diagnostic::invalid_envelope(path.into(), "Missing required field: kind".to_string())
        } else if error_msg.contains("metadata") {
            Diagnostic::invalid_envelope(
                path.into(),
                "Missing required field: metadata".to_string(),
            )
        } else if error_msg.contains("spec") {
            Diagnostic::invalid_envelope(path.into(), "Missing required field: spec".to_string())
        } else {
            Diagnostic::yaml_syntax(path.into(), &e)
        }
    })?;

    let spec = Spec::from_kind_and_value(&envelope.kind, envelope.spec).map_err(|e| {
        Diagnostic::invalid_envelope(
            path.into(),
            format!("Invalid spec for kind {}: {}", envelope.kind.wire_name(), e),
        )
    })?;

    Ok(AuthoredCard {
        source_path: path.into(),
        api_version: envelope.api_version,
        kind: envelope.kind,
        metadata: envelope.metadata,
        spec,
        artifacts: envelope.artifacts,
    })
}

fn materialize_inline_files(
    value: &mut serde_yaml::Value,
    source_path: &Path,
    inside_inline: bool,
    sandbox: &PathSandbox,
) -> Result<(), Diagnostic> {
    match value {
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                materialize_inline_files(value, source_path, inside_inline, sandbox)?;
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let agent_action = mapping
                .get(serde_yaml::Value::String("type".to_owned()))
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|kind| kind == "agent");
            for (key, value) in mapping {
                let nested_inline = inside_inline
                    || key.as_str().is_some_and(|key| {
                        key.eq_ignore_ascii_case("inline")
                            || is_inlineable_body(key, value, agent_action)
                    });
                materialize_inline_files(value, source_path, nested_inline, sandbox)?;
            }
        }
        serde_yaml::Value::Tagged(tagged) if tagged.tag == "!file" => {
            if !inside_inline {
                return Err(Diagnostic::invalid_envelope(
                    source_path.to_path_buf(),
                    "!file is only legal inside an inline body".to_owned(),
                ));
            }
            let relative = tagged.value.as_str().ok_or_else(|| {
                Diagnostic::invalid_envelope(
                    source_path.to_path_buf(),
                    "!file requires a UTF-8 path string".to_owned(),
                )
            })?;
            let relative_path = Path::new(relative);
            if relative_path.is_absolute()
                || relative_path
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(Diagnostic::path_escape(
                    source_path.to_path_buf(),
                    relative_path,
                ));
            }
            let resolved = sandbox.resolve_contained(source_path, relative_path)?;
            let metadata = std::fs::metadata(&resolved)
                .map_err(|error| Diagnostic::io(source_path.to_path_buf(), &error))?;
            if !metadata.is_file() {
                return Err(Diagnostic::invalid_envelope(
                    source_path.to_path_buf(),
                    format!("!file target is not a regular file: {relative}"),
                ));
            }
            let content = std::fs::read_to_string(&resolved)
                .map_err(|error| Diagnostic::io(source_path.to_path_buf(), &error))?;
            *value = serde_yaml::Value::String(content);
        }
        serde_yaml::Value::Tagged(tagged) => {
            materialize_inline_files(&mut tagged.value, source_path, inside_inline, sandbox)?;
        }
        serde_yaml::Value::Null
        | serde_yaml::Value::Bool(_)
        | serde_yaml::Value::Number(_)
        | serde_yaml::Value::String(_) => {}
    }
    Ok(())
}

fn is_inlineable_body(key: &str, value: &serde_yaml::Value, agent_action: bool) -> bool {
    let is_inlineable_slot =
        matches!(key, "prompt" | "judge_ref") || (agent_action && key == "target");
    if !is_inlineable_slot {
        return false;
    }
    let Some(mapping) = value.as_mapping() else {
        return false;
    };
    let has_identity_field = ["kind", "name", "version"]
        .iter()
        .all(|field| mapping.contains_key(serde_yaml::Value::String((*field).to_owned())));
    !has_identity_field
}

/// Build the exact reference identity required by a resolved spec slot.
pub(crate) fn card_ref_for(card: &AuthoredCard) -> Result<CardRef, Diagnostic> {
    let version = card.metadata.resolved_pin().cloned().ok_or_else(|| {
        Diagnostic::invalid_envelope(
            card.source_path.clone(),
            "Path references require an exact metadata.version pin".to_string(),
        )
    })?;
    let space = card.metadata.space.clone().ok_or_else(|| {
        Diagnostic::invalid_envelope(
            card.source_path.clone(),
            "Path references require metadata.space or a workspace default".to_string(),
        )
    })?;

    Ok(CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version,
        space: Some(space),
        uid: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_rejects_missing_api_version() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
kind: Agent
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  prompt:
    kind: Prompt
    provider: anthropic
    model: claude-sonnet-4.5
    messages: []
"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = parse_file(file.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.diagnostics.len(), 1);
        assert_eq!(err.diagnostics[0].code, "WYRD_LOADER_400_INVALID_ENVELOPE");
        assert!(err.diagnostics[0].message.contains("apiVersion"));
    }

    #[test]
    fn parse_rejects_missing_kind() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
apiVersion: wyrd/v1
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  prompt:
    kind: Prompt
    name: system-prompt
    version: "1.0.0"
    space: default
"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = parse_file(file.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.diagnostics.len(), 1);
        assert_eq!(err.diagnostics[0].code, "WYRD_LOADER_400_INVALID_ENVELOPE");
        assert!(err.diagnostics[0].message.contains("kind"));
    }

    #[test]
    fn parse_accepts_multi_doc_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: system-prompt
  space: default
  version: "1.0.0"
spec:
  provider: anthropic
  model: claude-sonnet-4.5
  messages: []
---
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: test-agent
  space: default
  version: "1.0.0"
spec:
  prompt:
    kind: Prompt
    name: system-prompt
    version: "1.0.0"
    space: default
"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = parse_file(file.path());
        assert!(result.is_ok());
        let cards = result.unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].kind, CardKind::Prompt);
        assert_eq!(cards[1].kind, CardKind::Agent);
    }

    #[test]
    fn parse_preserves_source_path() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  provider: anthropic
  model: claude-sonnet-4.5
  messages: []
"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = parse_file(file.path()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source_path, file.path());
    }

    #[test]
    fn parse_collects_every_envelope_violation() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
kind: Prompt
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  provider: anthropic
  model: claude-sonnet-4.5
  messages: []
---
apiVersion: wyrd/v1
metadata:
  name: test2
  space: default
  version: "1.0.0"
spec:
  provider: anthropic
  model: claude-sonnet-4.5
  messages: []
"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = parse_file(file.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.diagnostics.len(), 2);
        // Both docs have violations
        assert!(err.diagnostics[0].message.contains("apiVersion"));
        assert!(err.diagnostics[1].message.contains("kind"));
    }

    #[cfg(unix)]
    #[test]
    fn parse_rejects_inline_file_symlink_outside_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "secret").unwrap();
        symlink(&outside_file, workspace.path().join("runbook.txt")).unwrap();
        let source = workspace.path().join("agent.yaml");
        std::fs::write(
            &source,
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: agent\n  space: default\n  version: 1.0.0\nspec:\n  prompt:\n    request:\n      contents: []\n      system_instruction:\n        role: system\n        parts:\n          - text: !file runbook.txt\n    model: gemini-flash-lite\n    variables: []\n    media_variables: []\n    response_type: text\n  tool_names: []\n  run_config: {}\n",
        )
        .unwrap();

        let error = parse_file(&source).expect_err("inline file must remain contained");
        assert_eq!(error.diagnostics[0].code, "WYRD_LOADER_400_PATH_ESCAPE");
    }
}
