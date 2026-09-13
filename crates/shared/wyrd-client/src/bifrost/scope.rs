//! Producer-pool identity: [`SinkKind`], [`ClientScope`], and the credential
//! fingerprint the token-opaque client tier can compute.

use crate::config::ClientConfig;
use crate::error::WyrdClientError;
use crate::transport::ResolvedCredential;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use wyrd_spec::error::WyrdError;

/// The observation-kind discriminant that keys the producer pool.
///
/// A purpose-built `Copy + Eq + Hash` enum, deliberately **not**
/// `wyrd_spec`'s `ObservationKind` (which derives neither `Eq` nor `Hash`).
/// Only `Record` is landed today; future kinds add variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkKind {
    /// Record observations shipped to the Bifrost ingest service.
    Record,
}

/// The client-tier scope half of a producer-pool key.
///
/// `(server_url, credential fingerprint)`, computed **once** at handle
/// construction. The fingerprint is a SHA-256 hex digest of the resolved
/// credential's secret bytes — the credential is **never** JWT-decoded, so no
/// server-derived identity is reachable from the client tier. Two configs with
/// the same base URL and secret material produce the same scope; differing
/// secrets never collide.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientScope {
    server_url: String,
    credential_fingerprint: String,
}

impl ClientScope {
    /// Derive the scope from a [`ClientConfig`], resolving the effective
    /// credential and fingerprinting its secret material.
    ///
    /// The `server_url` component is [`ClientConfig`]'s `http.base_url` — there
    /// is no `server_url()` accessor. The credential is read via
    /// `resolve_credential()` and its exposed secret bytes are hashed; the JWT
    /// is never parsed.
    ///
    /// # Errors
    /// Returns [`WyrdError`] mapped from [`WyrdClientError`] when the credential
    /// chain yields nothing (`WYRD_CLIENT_401_NO_CREDENTIALS`).
    pub fn from_config(config: &ClientConfig) -> Result<Self, WyrdError> {
        let credential = config.resolve_credential().map_err(client_error_to_wyrd)?;
        Ok(Self {
            server_url: config.http.base_url.trim_end_matches('/').to_owned(),
            credential_fingerprint: fingerprint_credential(&credential),
        })
    }

    /// Derive the scope from an already-assembled [`WyrdClient`].
    ///
    /// Reads the base URL and credential the client's [`AuthMiddleware`] is
    /// actually bound to, rather than re-running the credential chain, so a
    /// handle built from a client can never key its producer pool on a
    /// different credential than the one its requests carry.
    ///
    /// [`AuthMiddleware`]: crate::auth::AuthMiddleware
    #[must_use]
    pub fn from_client(client: &crate::WyrdClient) -> Self {
        let auth = client.auth();
        Self {
            server_url: auth.base_url().trim_end_matches('/').to_owned(),
            credential_fingerprint: fingerprint_credential(auth.credential()),
        }
    }

    /// The server base URL this scope is bound to.
    #[must_use]
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// The SHA-256 hex fingerprint of the resolved credential's secret material.
    #[must_use]
    pub fn credential_fingerprint(&self) -> &str {
        &self.credential_fingerprint
    }
}

/// SHA-256 hex digest of the credential's secret bytes.
///
/// Reads `expose_secret()` on the variant's secret material — the only
/// reachable identity material — and hashes the raw bytes. No JWT decode.
fn fingerprint_credential(credential: &ResolvedCredential) -> String {
    let secret = match credential {
        ResolvedCredential::BearerToken(token) => token.expose_secret(),
        ResolvedCredential::WorkloadJwt { jwt, .. } => jwt.expose_secret(),
        ResolvedCredential::ApiKey(key) => key.expose_secret(),
    };
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())
}

/// Map a client-local [`WyrdClientError`] onto the shared [`WyrdError`] catalog,
/// preserving its stable `WYRD_CLIENT_*` code.
fn client_error_to_wyrd(error: WyrdClientError) -> WyrdError {
    let code = error.code();
    let message = error.to_string();
    WyrdError::from_code(code, message.clone(), serde_json::json!({})).unwrap_or(
        WyrdError::Internal {
            message,
            details: serde_json::json!({ "original_code": code }),
        },
    )
}
