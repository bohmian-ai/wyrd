//! YAML parsing and envelope validation.

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::reference::CardRef;
use wyrd_spec::refs::InlineableSlotField;

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
#[cfg(test)]
pub(crate) fn parse_file(path: &Path) -> Result<Vec<AuthoredCard>, LoadError> {
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
        match YamlValue::deserialize(raw_doc) {
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
#[cfg(test)]
pub(crate) fn parse_path(path: &Path) -> Result<Vec<AuthoredCard>, LoadError> {
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

fn parse_single_envelope(path: &Path, raw: YamlValue) -> Result<AuthoredCard, Diagnostic> {
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

/// Normalize one raw authored document before typed envelope decoding.
///
/// Walks every mapping and sequence. A key that [`InlineableSlotField`]
/// recognizes — with its mapping's `type` discriminator, and per element for
/// list slots — is an inlineable reference slot: its value is materialized
/// and its keyed `ref`/`path`/`inline` form collapsed by
/// [`materialize_reference_slot`]. Any other value is walked with
/// `inside_inline` carried down, set once an `inline` key is entered. A
/// `!file` tag is replaced with the contained file's text only inside an
/// inline body, resolved relative to `source_path` within `sandbox`.
///
/// # Errors
/// Returns an invalid-envelope diagnostic for `!file` outside an inline body,
/// a non-string or non-regular-file target, or a slot combining several keyed
/// forms; a path-escape diagnostic for an absolute or parent-relative `!file`
/// path or one leaving the sandbox; and an IO diagnostic when the target
/// cannot be read.
fn materialize_inline_files(
    value: &mut YamlValue,
    source_path: &Path,
    inside_inline: bool,
    sandbox: &PathSandbox,
) -> Result<(), Diagnostic> {
    match value {
        YamlValue::Sequence(values) => {
            for value in values {
                materialize_inline_files(value, source_path, inside_inline, sandbox)?;
            }
        }
        YamlValue::Mapping(mapping) => {
            let mapping_type = mapping
                .get(YamlValue::String("type".to_owned()))
                .and_then(YamlValue::as_str)
                .map(str::to_owned);
            for (key, value) in mapping {
                let slot = key
                    .as_str()
                    .and_then(|key| InlineableSlotField::find(key, mapping_type.as_deref()));
                match slot {
                    Some(InlineableSlotField { list: true, .. }) => {
                        let YamlValue::Sequence(elements) = value else {
                            materialize_inline_files(value, source_path, inside_inline, sandbox)?;
                            continue;
                        };
                        for element in elements {
                            materialize_reference_slot(
                                element,
                                source_path,
                                inside_inline,
                                sandbox,
                            )?;
                        }
                    }
                    Some(_) => {
                        materialize_reference_slot(value, source_path, inside_inline, sandbox)?;
                    }
                    None => {
                        let nested_inline = inside_inline
                            || key
                                .as_str()
                                .is_some_and(|key| key.eq_ignore_ascii_case("inline"));
                        materialize_inline_files(value, source_path, nested_inline, sandbox)?;
                    }
                }
            }
        }
        YamlValue::Tagged(tagged) if tagged.tag == "!file" => {
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
            *value = YamlValue::String(content);
        }
        YamlValue::Tagged(tagged) => {
            materialize_inline_files(&mut tagged.value, source_path, inside_inline, sandbox)?;
        }
        YamlValue::Null | YamlValue::Bool(_) | YamlValue::Number(_) | YamlValue::String(_) => {}
    }
    Ok(())
}

/// Materialize one inlineable reference slot and collapse its keyed form.
///
/// A slot value that is an inline body (a mapping without a Card identity)
/// may use `!file`, so it is materialized as inline content; the authored
/// `ref`/`path`/`inline` wrapper is then unwrapped to the wire shape that
/// typed deserialization and the canonical reference visitor accept.
///
/// # Errors
/// Returns the `!file` diagnostics of [`materialize_inline_files`] and the
/// combined-form diagnostic of [`unwrap_reference_form`].
fn materialize_reference_slot(
    value: &mut YamlValue,
    source_path: &Path,
    inside_inline: bool,
    sandbox: &PathSandbox,
) -> Result<(), Diagnostic> {
    let nested_inline = inside_inline || is_inline_body(value);
    materialize_inline_files(value, source_path, nested_inline, sandbox)?;
    unwrap_reference_form(value, source_path)
}

/// Collapse the authored keyed reference form at an inlineable slot.
///
/// The authoring contract discriminates a slot by its single key: `ref:`
/// wraps a Card reference, `path:` a loader-local file, and `inline:` an
/// embedded body. The wire shape carries the unwrapped value, so this rewrite
/// keeps one spec schema while accepting the documented authored form. A
/// mapping without any of those keys is already in wire shape and is left
/// untouched.
///
/// # Errors
/// Returns an invalid-envelope diagnostic when one slot combines more than
/// one of `ref`, `path`, and `inline`.
fn unwrap_reference_form(value: &mut YamlValue, source_path: &Path) -> Result<(), Diagnostic> {
    let Some(mapping) = value.as_mapping_mut() else {
        return Ok(());
    };
    let forms = ["ref", "path", "inline"]
        .into_iter()
        .filter(|form| mapping.contains_key(*form))
        .collect::<Vec<_>>();
    match forms.as_slice() {
        [] => Ok(()),
        [form] if mapping.len() == 1 => {
            if let Some(inner) = mapping.remove(*form) {
                *value = inner;
            }
            Ok(())
        }
        _ => Err(Diagnostic::invalid_envelope(
            source_path.to_path_buf(),
            "a reference slot must use exactly one of ref, path, or inline".to_owned(),
        )),
    }
}

/// Return whether a reference slot value is an inline body rather than a
/// Card reference: a mapping that lacks any of `kind`, `name`, or `version`.
fn is_inline_body(value: &YamlValue) -> bool {
    let Some(mapping) = value.as_mapping() else {
        return false;
    };
    let has_identity_field = ["kind", "name", "version"]
        .iter()
        .all(|field| mapping.contains_key(YamlValue::String((*field).to_owned())));
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
    use wyrd_spec::reference::InlineableRef;

    /// A slot that combines two authored reference forms is refused rather
    /// than guessed at.
    ///
    /// # Panics
    /// Panics when the fixture cannot be written or the parse is not refused
    /// with the combined-form diagnostic.
    #[test]
    fn parse_rejects_combined_reference_forms() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: test\n  space: default\n  version: \"1.0.0\"\nspec:\n  prompt:\n    path: ./prompt.yaml\n    inline:\n      model: m\n"
        )
        .unwrap();

        let error = parse_file(file.path()).expect_err("combined forms are refused");
        assert!(
            error.diagnostics[0]
                .message
                .contains("exactly one of ref, path, or inline")
        );
    }

    /// Keyed forms at the binding slots the canonical inventory owns — a
    /// single `runs_on` and every `on_failure` list element — collapse to the
    /// wire shape before typed decoding, alongside the Agent `prompt` slot.
    ///
    /// # Panics
    /// Panics when the fixture cannot be written, fails to parse, or any slot
    /// keeps its keyed wrapper or decodes to the wrong reference form.
    #[test]
    fn parse_unwraps_keyed_binding_slots() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"apiVersion: wyrd/v1
kind: Agent
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  prompt:
    ref: {{kind: Prompt, name: review-prompt, version: "1.0.0"}}
  verified_by:
    - verifier: {{kind: Verifier, name: review-quality, version: "1.0.0"}}
      runs_on:
        inline:
          kind: schedule
          cron: "0 * * * *"
      on_failure:
        - ref: {{kind: Operator, name: page, version: "1.0.0"}}
        - path: ./notify.yaml
        - inline:
            kind: workflow
            workflow_ref: {{kind: Workflow, name: remediate, version: "1.0.0"}}
"#
        )
        .unwrap();

        let cards = parse_file(file.path()).expect("keyed binding slots parse");
        let Spec::Agent(agent) = &cards[0].spec else {
            panic!("fixture is an Agent");
        };
        assert!(
            matches!(&agent.prompt, InlineableRef::Ref(prompt) if prompt.name.as_str() == "review-prompt")
        );
        let binding = &agent.verified_by[0];
        assert!(matches!(&binding.runs_on, InlineableRef::Inline(_)));
        let [page, notify, remediate] = binding.on_failure.as_slice() else {
            panic!("three on_failure elements decode");
        };
        assert!(matches!(page, InlineableRef::Ref(page) if page.name.as_str() == "page"));
        assert!(matches!(notify, InlineableRef::Path(path) if path == Path::new("./notify.yaml")));
        assert!(matches!(remediate, InlineableRef::Inline(_)));
    }

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

    /// Refuse the retired `Drift` and `Eval` Card kinds at the loader boundary.
    ///
    /// Both are Verifier implementations now, so a file authored against the
    /// old model must fail here rather than reaching validation or the wire.
    ///
    /// # Panics
    /// Panics when the temporary fixture file cannot be created or written,
    /// and when either retired kind parses or its diagnostic does not name the
    /// offending kind.
    #[test]
    fn parse_rejects_retired_drift_and_eval_card_kinds() {
        for kind in ["Drift", "Eval"] {
            let mut file = NamedTempFile::new().unwrap();
            writeln!(
                file,
                r#"
apiVersion: wyrd/v1
kind: {kind}
metadata:
  name: quality
  space: default
  version: "1.0.0"
spec: {{}}
"#
            )
            .unwrap();
            file.flush().unwrap();

            let err =
                parse_file(file.path()).expect_err("the retired {kind} Card kind must not parse");
            assert_eq!(err.diagnostics.len(), 1, "{:?}", err.diagnostics);
            assert_eq!(err.diagnostics[0].code, "WYRD_LOADER_400_INVALID_ENVELOPE");
        }
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
