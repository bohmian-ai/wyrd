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
            Self::Client(error) => error.into(),
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

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::registry::ListCardsRequest;

    use crate::cards::Cards;

    /// Credential variables the shared chain consults before the home file.
    const CREDENTIAL_VARS: [&str; 4] = [
        "WYRD_ACCESS_TOKEN",
        "WYRD_WORKLOAD_TOKEN",
        "WYRD_TENANT",
        "WYRD_API_KEY",
    ];

    /// Public `Cards` construction and operations report shared client failures
    /// with the same `WYRD_CLIENT_*` code and status as `Bifrost`.
    ///
    /// The credential chain is emptied under the crate env lock with `HOME`
    /// redirected to an empty directory, so only the explicit arguments decide
    /// each outcome.
    ///
    /// # Panics
    ///
    /// Panics when any failure loses its client catalog code or status.
    #[tokio::test]
    async fn cards_client_failures_keep_client_catalog_identity() {
        let home = tempfile::tempdir().expect("temporary home is created");
        let env = crate::ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            for name in CREDENTIAL_VARS {
                std::env::remove_var(name);
            }
            std::env::set_var("HOME", home.path());
        }

        let missing = Cards::new(Some("http://127.0.0.1:1"), None)
            .err()
            .expect("an empty credential chain is refused");
        let invalid = Cards::new(Some(""), Some(SecretString::from("wyrd_sk_t_v_s")))
            .err()
            .expect("an empty explicit server URL is refused");
        let cards = Cards::new(
            Some("http://127.0.0.1:1"),
            Some(SecretString::from("wyrd_sk_t_v_s")),
        )
        .expect("a complete offline configuration constructs without network");
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("HOME");
        }

        drop(env);
        let down = cards
            .list(ListCardsRequest {
                kind: Some(CardKind::Prompt),
                space: None,
                name: None,
                version_range: None,
                status: None,
                filter: None,
                include_prerelease: false,
                limit: None,
                cursor: None,
            })
            .await
            .expect_err("an unreachable token exchange is a transport failure");

        assert_eq!(
            (missing.code(), missing.status()),
            ("WYRD_CLIENT_401_NO_CREDENTIALS", 401)
        );
        assert_eq!(
            (invalid.code(), invalid.status()),
            ("WYRD_CLIENT_400_CONFIG_INVALID", 400)
        );
        assert_eq!(
            (down.code(), down.status()),
            ("WYRD_CLIENT_503_TRANSPORT_DOWN", 503)
        );
        assert_eq!(missing.problem().details, serde_json::json!({}));
        assert_eq!(
            invalid.problem().details,
            serde_json::json!({ "field": "server_url", "reason": "must not be empty" })
        );
        assert_eq!(
            down.problem().details,
            serde_json::json!({ "transport": "http" })
        );
    }
}
