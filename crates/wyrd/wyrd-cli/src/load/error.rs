//! Loader error types and diagnostic support.

use std::path::PathBuf;

use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Loader error collecting every diagnostic across the load pipeline.
#[derive(Debug, Error)]
#[error("load failed with diagnostics")]
pub struct LoadError {
    /// All diagnostics collected during the load.
    pub diagnostics: Vec<Diagnostic>,
}

impl LoadError {
    /// Create a single-diagnostic error.
    pub fn single(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    /// Create an error from a vec of diagnostics.
    pub fn multiple(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }
}

/// A single diagnostic emitted during the load pipeline.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The file this diagnostic refers to.
    pub path: PathBuf,
    /// The stable error code this will surface as at the wire boundary.
    pub code: &'static str,
    /// Human-readable message with remediation guidance.
    pub message: String,
    /// Optional structured context for IDE/tooling consumption.
    pub context: Option<serde_json::Value>,
}

impl Diagnostic {
    /// Create an IO error diagnostic.
    pub fn io(path: PathBuf, error: std::io::Error) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_IO",
            message: format!("IO error: {}", error),
            context: None,
        }
    }

    /// Create a YAML syntax error diagnostic.
    pub fn yaml_syntax(path: PathBuf, error: serde_yaml::Error) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_YAML_SYNTAX",
            message: format!("YAML syntax error: {}", error),
            context: None,
        }
    }

    /// Create an invalid envelope diagnostic.
    pub fn invalid_envelope(path: PathBuf, message: String) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_INVALID_ENVELOPE",
            message,
            context: None,
        }
    }

    /// Create a path escape diagnostic.
    pub fn path_escape(path: PathBuf, escaped_path: PathBuf) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_PATH_ESCAPE",
            message: format!(
                "Path reference escapes workspace root: {}",
                escaped_path.display()
            ),
            context: Some(serde_json::json!({
                "escaped_path": escaped_path
            })),
        }
    }

    /// Create a path absolute advisory diagnostic.
    pub fn path_absolute_advisory(path: PathBuf, absolute_path: PathBuf) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_PATH_ABSOLUTE_ADVISORY",
            message: format!(
                "Absolute path reference breaks portability: {}",
                absolute_path.display()
            ),
            context: Some(serde_json::json!({
                "absolute_path": absolute_path
            })),
        }
    }

    /// Create a config load failed diagnostic.
    pub fn config_load_failed(path: PathBuf, error: String) -> Self {
        Self {
            path,
            code: "WYRD_LOADER_400_CONFIG_LOAD_FAILED",
            message: format!("Config load failed: {}", error),
            context: None,
        }
    }
}

/// Convert a WyrdError into a Diagnostic.
impl From<WyrdError> for Diagnostic {
    fn from(error: WyrdError) -> Self {
        Self {
            path: PathBuf::from("<unknown>"),
            code: error.code(),
            message: error.to_string(),
            context: None,
        }
    }
}

impl From<LoadError> for Diagnostic {
    fn from(error: LoadError) -> Self {
        error.diagnostics.into_iter().next().unwrap_or_else(|| {
            Diagnostic::invalid_envelope(
                PathBuf::from("<unknown>"),
                "Loader failed without a diagnostic".to_string(),
            )
        })
    }
}
