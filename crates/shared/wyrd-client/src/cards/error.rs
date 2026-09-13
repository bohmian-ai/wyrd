//! Registry-local error composition and public boundary conversion.

use crate::error::WyrdClientError;
use crate::storage::StorageClientError;
use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Errors produced while composing a client-side registry operation.
#[derive(Debug, Error)]
pub(crate) enum RegistryEngineError {
    /// A server returned a structured Wyrd error.
    #[error("{0}")]
    Wyrd(#[from] WyrdError),
    /// The client transport could not be assembled.
    #[error("client configuration failed: {0}")]
    Client(#[from] WyrdClientError),
    /// Artifact transfer failed.
    #[error("artifact transfer failed: {0}")]
    Storage(#[from] StorageClientError),
    /// Local artifact processing failed.
    #[error("local artifact processing failed: {0}")]
    Io(#[from] std::io::Error),
    /// A locally-created wire value could not be serialized.
    #[error("wire serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// A list query could not be encoded as URL query parameters.
    #[error("query serialization failed: {0}")]
    QuerySerialization(#[from] serde_urlencoded::ser::Error),
}

impl RegistryEngineError {
    /// Convert an internal error to the shared public Wyrd error catalog.
    pub(crate) fn into_wyrd(self) -> WyrdError {
        match self {
            Self::Wyrd(error) => error,
            Self::Client(WyrdClientError::Config { field, reason }) => WyrdError::Internal {
                message: "client configuration is invalid".to_owned(),
                details: serde_json::json!({ "field": field, "reason": reason }),
            },
            Self::Client(WyrdClientError::TransportDown { transport, .. }) => {
                WyrdError::RegistryUnavailable {
                    message: "registry transport is unavailable".to_owned(),
                    details: serde_json::json!({ "transport": transport }),
                }
            }
            Self::Client(WyrdClientError::NoCredentials) => WyrdError::Internal {
                message: "no Wyrd credentials are configured".to_owned(),
                details: serde_json::json!({ "source": "credential_chain" }),
            },
            Self::Storage(error) => error.into(),
            Self::Io(_) => WyrdError::RegistryUploadInterrupted {
                message: "local artifact materialization failed".to_owned(),
                details: serde_json::json!({}),
            },
            Self::Serialization(_) => WyrdError::Internal {
                message: "registry request serialization failed".to_owned(),
                details: serde_json::json!({}),
            },
            Self::QuerySerialization(_) => WyrdError::Internal {
                message: "registry list query serialization failed".to_owned(),
                details: serde_json::json!({}),
            },
        }
    }
}

impl From<RegistryEngineError> for WyrdError {
    /// Convert an engine error at the public `Cards` boundary.
    fn from(error: RegistryEngineError) -> Self {
        error.into_wyrd()
    }
}
