//! YAML parsing and envelope validation.

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::reference::CardRef;

use super::error::{Diagnostic, LoadError};

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
    let bytes =
        std::fs::read(path).map_err(|e| LoadError::single(Diagnostic::io(path.into(), e)))?;

    let content = std::str::from_utf8(&bytes).map_err(|e| {
        LoadError::single(Diagnostic::yaml_syntax(
            path.into(),
            serde_yaml::Error::custom(format!("invalid UTF-8: {}", e)),
        ))
    })?;

    let mut cards = Vec::new();
    let mut diagnostics = Vec::new();

    for raw_doc in serde_yaml::Deserializer::from_str(content) {
        match parse_single_envelope(path, raw_doc) {
            Ok(card) => cards.push(card),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    if !diagnostics.is_empty() {
        return Err(LoadError::multiple(diagnostics));
    }

    Ok(cards)
}

fn parse_single_envelope(
    path: &Path,
    raw: serde_yaml::Deserializer<'_>,
) -> Result<AuthoredCard, Diagnostic> {
    let envelope = RawCardEnvelope::deserialize(raw).map_err(|e| {
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
            Diagnostic::yaml_syntax(path.into(), e)
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
        space,
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
}
