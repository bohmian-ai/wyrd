//! Server-private Oracle peer credential ownership.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerCredentials};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::credential::ResolvedCredential;

/// Refreshing credential owner for private Bifrost peer RPCs.
///
/// One process holds exactly one of these regardless of which roles it serves,
/// so an Oracle and a Scribe on the same replica authenticate to the peer plane
/// as the same configured Service principal.
pub struct ServerBifrostPeerCredentials {
    /// Shared client middleware that single-flights token exchange and refresh.
    auth: Arc<AuthMiddleware>,
}

impl std::fmt::Debug for ServerBifrostPeerCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerBifrostPeerCredentials")
            .finish_non_exhaustive()
    }
}

impl ServerBifrostPeerCredentials {
    /// Builds credentials from the configured peer workload key.
    ///
    /// The key arrives through `bifrost.peer.api_key`, which is projected from
    /// `WYRD_BIFROST_PEER_API_KEY`; this owner never reads the environment
    /// itself so configuration validation stays in one place.
    ///
    /// # Errors
    /// Returns a redacted message when the key is absent or middleware setup fails.
    pub fn from_configured_key(api_key: Option<String>) -> Result<Self, String> {
        Self::from_resolved_key(api_key, &ClientConfig::from_env())
    }

    /// Builds credentials from one resolved production key value.
    ///
    /// # Errors
    /// Returns a redacted message when the value is absent, blank, or cannot
    /// initialize the shared auth middleware.
    fn from_resolved_key(api_key: Option<String>, config: &ClientConfig) -> Result<Self, String> {
        let api_key = api_key
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "bifrost.peer.api_key is required".to_owned())?;
        let auth = AuthMiddleware::new(
            config,
            ResolvedCredential::ApiKey(SecretString::from(api_key)),
        )
        .map_err(|_| "Oracle peer credential middleware initialization failed".to_owned())?;
        Ok(Self { auth })
    }

    /// Performs the mandatory initial exchange before Oracle readiness.
    ///
    /// # Errors
    /// Returns a redacted message when the credential is rejected or unavailable.
    pub async fn initialize(&self) -> Result<String, String> {
        self.auth
            .bearer()
            .await
            .map(|token| token.expose().to_owned())
            .map_err(|_| "Bifrost peer credential exchange failed".to_owned())
    }
}

#[async_trait]
impl OraclePeerCredentials for ServerBifrostPeerCredentials {
    async fn bearer(&self, force_refresh: bool) -> Result<String, DispatchError> {
        let token = if force_refresh {
            self.auth.force_refresh().await
        } else {
            self.auth.bearer().await
        }
        .map_err(|_| DispatchError::Terminal)?;
        Ok(token.expose().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::ServerBifrostPeerCredentials;
    use wyrd_client::config::ClientConfig;

    /// Production configuration fails closed when the peer key is missing.
    #[test]
    fn configured_key_rejects_missing_value() {
        let error =
            ServerBifrostPeerCredentials::from_resolved_key(None, &ClientConfig::default())
                .expect_err("missing key is rejected");

        assert_eq!(error, "bifrost.peer.api_key is required");
    }

    /// Production configuration fails closed when the peer key is blank.
    #[test]
    fn configured_key_rejects_blank_value() {
        let error = ServerBifrostPeerCredentials::from_resolved_key(
            Some("   ".to_owned()),
            &ClientConfig::default(),
        )
        .expect_err("blank key is rejected");

        assert_eq!(error, "bifrost.peer.api_key is required");
    }
}
