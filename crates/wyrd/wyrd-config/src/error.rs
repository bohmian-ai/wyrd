//! Crate-local error type. Maps to `wyrd_spec::error::WyrdError`
//! `WYRD_CFG_*` variants at CLI / SDK boundaries.

use std::path::PathBuf;

use thiserror::Error;

/// Errors raised by the `wyrd-config` crate.
#[derive(Debug, Error)]
pub enum WyrdConfigError {
    /// Reading the current working directory failed.
    #[error("failed to read current directory: {0}")]
    CwdRead(#[source] std::io::Error),

    /// Filesystem read failed at `path`.
    #[error("io error at {path}: {message}")]
    Io {
        /// Human-readable error message.
        message: String,
        /// Path the IO operation targeted.
        path: PathBuf,
    },

    /// TOML syntax parse failed at `path`.
    #[error("toml parse error at {path}: {message}")]
    TomlParse {
        /// Underlying parser message.
        message: String,
        /// Path of the offending file.
        path: PathBuf,
    },

    /// TOML deserialized but failed the typed schema. Catches
    /// `deny_unknown_fields` violations, lowercase or unknown
    /// `[kind.X]` keys, invalid validated-newtype values
    /// (`SpaceName`, `LabelKey`, …), and the explicit
    /// `[kind.External]` rejection.
    #[error("schema mismatch at {path}: {message}")]
    Schema {
        /// Underlying deserialization message.
        message: String,
        /// Path of the offending file.
        path: PathBuf,
    },

    /// `name` is not defaultable; remove from `table`.
    #[error("`name` may not be defaulted; remove from table `{table}`")]
    NameDefaultRejected {
        /// The TOML table that contained the disallowed `name` key.
        table: String,
    },
}

impl From<WyrdConfigError> for wyrd_spec::error::WyrdError {
    fn from(err: WyrdConfigError) -> Self {
        use serde_json::json;
        use wyrd_spec::error::WyrdError;
        match err {
            WyrdConfigError::CwdRead(io) => WyrdError::Internal {
                message: format!("cwd read: {io}"),
                details: json!({ "source": "cwd_read" }),
            },
            WyrdConfigError::Io { message, path } => WyrdError::CfgInvalidToml {
                message,
                details: json!({
                    "path": path.file_name().and_then(|n| n.to_str()).unwrap_or("<unknown>"),
                    "source": "io",
                }),
            },
            WyrdConfigError::TomlParse { message, path } => WyrdError::CfgInvalidToml {
                message,
                details: json!({
                    "path": path.file_name().and_then(|n| n.to_str()).unwrap_or("<unknown>"),
                    "source": "toml",
                }),
            },
            WyrdConfigError::Schema { message, path } => WyrdError::CfgSchemaMismatch {
                message: message.clone(),
                details: json!({
                    "path": path.file_name().and_then(|n| n.to_str()).unwrap_or("<unknown>"),
                    "serde_message": message,
                }),
            },
            WyrdConfigError::NameDefaultRejected { table } => {
                let message = format!("`name` may not be defaulted; remove from table `{table}`");
                WyrdError::CfgNameDefaultRejected {
                    message,
                    details: json!({ "table": table }),
                }
            }
        }
    }
}
