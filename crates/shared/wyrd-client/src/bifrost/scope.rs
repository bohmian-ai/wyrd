//! Producer-pool identity: [`SinkKind`], [`ClientScope`], and the credential
//! fingerprint the token-opaque client tier can compute.

#[cfg(feature = "internal")]
use crate::config::ClientConfig;
use crate::transport::ResolvedCredential;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
#[cfg(feature = "internal")]
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;

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
/// server-derived identity is reachable from the client tier. The workload
/// root the credential runs as is hashed beside it, so rootless, R1, and R2
/// handles over one secret never share a pool. Two configs with the same base
/// URL, secret material, and root produce the same scope; differing secrets or
/// roots never collide.
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
    /// # Arguments
    /// * `config` - The client configuration whose base URL and resolved
    ///   credential key the scope.
    ///
    /// # Errors
    /// Returns [`WyrdError`] mapped from [`WyrdClientError`](crate::error::WyrdClientError) when the credential
    /// chain yields nothing (`WYRD_CLIENT_401_NO_CREDENTIALS`).
    #[cfg(feature = "internal")]
    pub fn from_config(config: &ClientConfig) -> Result<Self, WyrdError> {
        let credential = config.resolve_credential().map_err(WyrdError::from)?;
        let workload_uid = config.environment.var("WYRD_WORKLOAD_UID");
        Ok(Self {
            server_url: config.http.base_url.trim_end_matches('/').to_owned(),
            credential_fingerprint: fingerprint_credential(&credential, workload_uid.as_deref()),
        })
    }

    /// Derive the scope from an already-assembled [`WyrdClient`](crate::WyrdClient).
    ///
    /// Reads the base URL and credential the client's [`AuthMiddleware`] is
    /// actually bound to, rather than re-running the credential chain, so a
    /// handle built from a client can never key its producer pool on a
    /// different credential than the one its requests carry.
    ///
    /// [`AuthMiddleware`]: crate::auth::AuthMiddleware
    ///
    /// # Arguments
    /// * `client` - The assembled client whose bound base URL and credential key
    ///   the scope.
    #[must_use]
    pub(crate) fn from_client(client: &crate::WyrdClient) -> Self {
        let auth = &client.auth;
        Self {
            server_url: auth.base_url().trim_end_matches('/').to_owned(),
            credential_fingerprint: scoped_fingerprint(
                fingerprint_credential(auth.credential(), auth.workload_uid().map(CardUid::as_str)),
                auth.deployment_environment(),
            ),
        }
    }

    /// The server base URL this scope is bound to.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// The SHA-256 hex fingerprint of the resolved credential's secret material.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn credential_fingerprint(&self) -> &str {
        &self.credential_fingerprint
    }
}

/// `fingerprint` extended with the configured deployment environment, so two
/// states over one credential and root in different environments never share
/// a producer pool whose metadata carries the other's environment.
fn scoped_fingerprint(fingerprint: String, deployment_environment: Option<&str>) -> String {
    match deployment_environment {
        Some(environment) => {
            let mut hasher = Sha256::new();
            hasher.update(fingerprint);
            hasher.update([0]);
            hasher.update(environment);
            hex::encode(hasher.finalize())
        }
        None => fingerprint,
    }
}

/// SHA-256 hex digest of the credential's secret bytes and the workload root
/// it runs as.
///
/// Reads `expose_secret()` on the variant's secret material — the only
/// reachable identity material — and hashes the raw bytes. No JWT decode. A
/// delegated credential hashes the subject token, the audience, and the
/// actor's own fingerprint, so two subjects behind one actor never share a
/// producer pool. A root is hashed after a separator byte no secret or UID
/// spelling contains, so rootless and rooted scopes never collide.
fn fingerprint_credential(credential: &ResolvedCredential, workload_uid: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    match credential {
        ResolvedCredential::Renewable(source) => hasher.update(source.identity()),
        ResolvedCredential::BearerToken(token) => hasher.update(token.expose_secret()),
        ResolvedCredential::WorkloadJwt { jwt, .. } => hasher.update(jwt.expose_secret()),
        ResolvedCredential::ApiKey(key) => hasher.update(key.expose_secret()),
        ResolvedCredential::Delegated {
            subject_token,
            audience,
            actor,
        } => {
            hasher.update(subject_token.expose_secret());
            hasher.update(audience.as_str());
            hasher.update(fingerprint_credential(
                actor.credential(),
                actor.workload_uid().map(CardUid::as_str),
            ));
        }
    }
    if let Some(uid) = workload_uid {
        hasher.update([0]);
        hasher.update(uid);
    }
    hex::encode(hasher.finalize())
}
