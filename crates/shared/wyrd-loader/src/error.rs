//! Structured loader errors and agent-readable diagnostics.

use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Loader error collecting every error-severity diagnostic.
#[derive(Debug, Error)]
#[error("load failed with {} diagnostic(s)", .diagnostics.len())]
pub struct LoadError {
    /// All diagnostics collected during the load.
    pub diagnostics: Vec<Diagnostic>,
}

impl LoadError {
    /// Create a single-diagnostic error.
    #[must_use]
    pub fn single(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    /// Create an error from several diagnostics.
    #[must_use]
    pub fn multiple(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }
}

/// Diagnostic severity used by human and machine-readable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The tree cannot be submitted.
    Error,
    /// The tree is valid but the authoring choice is discouraged.
    Warning,
}

/// One-based source location when supplied by the YAML parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SourceSpan {
    /// One-based source line.
    pub line: usize,
    /// One-based source column.
    pub column: usize,
}

/// A single loader diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Diagnostic {
    /// Stable public Wyrd error code.
    pub code: &'static str,
    /// Canonical HTTP status associated with the error code.
    pub status: u16,
    /// Severity of this occurrence.
    pub severity: Severity,
    /// Authored file this diagnostic concerns.
    pub path: PathBuf,
    /// Optional parser location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    /// Human-readable problem detail.
    pub message: String,
    /// Catalog-authored remediation.
    pub remediation: &'static str,
    /// Optional structured context for tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<serde_json::Value>")]
    pub context: Option<Box<serde_json::Value>>,
}

impl Diagnostic {
    /// Build a diagnostic from the canonical public error catalog.
    #[must_use]
    pub fn from_wyrd_error(
        path: PathBuf,
        severity: Severity,
        span: Option<SourceSpan>,
        error: &WyrdError,
    ) -> Self {
        let problem = error.as_problem_json();
        let context = problem
            .get("details")
            .filter(|details| !details.is_null())
            .cloned()
            .map(Box::new);
        Self {
            code: error.code(),
            status: problem
                .get("status")
                .and_then(serde_json::Value::as_u64)
                .and_then(|status| u16::try_from(status).ok())
                .unwrap_or_else(|| error.status()),
            severity,
            path,
            span,
            message: error.to_string(),
            remediation: error.remediation(),
            context,
        }
    }

    /// Create an IO diagnostic.
    #[must_use]
    pub fn io(path: PathBuf, source: &std::io::Error) -> Self {
        let message = source.to_string();
        Self::from_wyrd_error(
            path,
            Severity::Error,
            None,
            &WyrdError::LoaderIo {
                message,
                details: serde_json::Value::Null,
            },
        )
    }

    /// Create a YAML syntax diagnostic with parser location when available.
    #[must_use]
    pub fn yaml_syntax(path: PathBuf, source: &serde_yaml::Error) -> Self {
        let span = source.location().map(|location| SourceSpan {
            line: location.line(),
            column: location.column(),
        });
        let message = source.to_string();
        Self::from_wyrd_error(
            path,
            Severity::Error,
            span,
            &WyrdError::LoaderYamlSyntax {
                message,
                details: serde_json::Value::Null,
            },
        )
    }

    /// Create an invalid-envelope diagnostic.
    #[must_use]
    pub fn invalid_envelope(path: PathBuf, message: String) -> Self {
        Self::from_wyrd_error(
            path,
            Severity::Error,
            None,
            &WyrdError::LoaderInvalidEnvelope {
                message,
                details: serde_json::Value::Null,
            },
        )
    }

    /// Create a path escape diagnostic.
    #[must_use]
    pub fn path_escape(path: PathBuf, _escaped_path: &std::path::Path) -> Self {
        Self::from_wyrd_error(
            path,
            Severity::Error,
            None,
            &WyrdError::LoaderPathEscape {
                message: "path reference escapes workspace".to_owned(),
                details: serde_json::json!({ "reason": "outside_workspace" }),
            },
        )
    }

    /// Create an absolute-path portability warning.
    #[must_use]
    pub fn path_absolute_advisory(path: PathBuf, _absolute_path: &std::path::Path) -> Self {
        Self::from_wyrd_error(
            path,
            Severity::Warning,
            None,
            &WyrdError::LoaderPathAbsoluteAdvisory {
                message: "absolute path breaks portability".to_owned(),
                details: serde_json::json!({ "absolute": true }),
            },
        )
    }

    /// Create a config-load diagnostic.
    #[must_use]
    pub fn config_load_failed(path: PathBuf, message: String) -> Self {
        Self::from_wyrd_error(
            path,
            Severity::Error,
            None,
            &WyrdError::LoaderConfigLoadFailed {
                message,
                details: serde_json::Value::Null,
            },
        )
    }
}

/// Return the JSON Schema for the machine-readable diagnostic contract.
#[must_use]
#[cfg(test)]
pub fn diagnostic_schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(Diagnostic)
}

/// Serialize diagnostics in a stable order for agent and CLI consumption.
///
/// # Errors
/// Returns a JSON serialization error if structured context is invalid.
pub fn emit_json(diagnostics: &[Diagnostic]) -> Result<String, serde_json::Error> {
    let mut ordered = diagnostics.to_vec();
    ordered.sort_by(|left, right| {
        (
            &left.path,
            left.span.map(|span| (span.line, span.column)),
            left.code,
        )
            .cmp(&(
                &right.path,
                right.span.map(|span| (span.line, span.column)),
                right.code,
            ))
    });
    serde_json::to_string_pretty(&ordered)
}
