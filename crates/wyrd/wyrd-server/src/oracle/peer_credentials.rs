//! Server-private Oracle peer credential ownership.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerCredentials};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::credential::ResolvedCredential;

/// Refreshing credential owner for private peer RPCs.
pub struct ServerOraclePeerCredentials {
    /// Shared client middleware that single-flights token exchange and refresh.
    auth: Arc<AuthMiddleware>,
}

impl std::fmt::Debug for ServerOraclePeerCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerOraclePeerCredentials")
            .finish_non_exhaustive()
    }
}

impl ServerOraclePeerCredentials {
    /// Builds credentials from the configured durable Oracle API key.
    ///
    /// # Errors
    /// Returns a redacted message when the key is absent or middleware setup fails.
    pub fn from_env() -> Result<Self, String> {
        let api_key = std::env::var("WYRD_BIFROST_ORACLE_PEER_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "WYRD_BIFROST_ORACLE_PEER_API_KEY is required".to_owned())?;
        let config = ClientConfig::from_env();
        let auth = AuthMiddleware::new(
            &config,
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
            .map_err(|_| "Oracle peer credential exchange failed".to_owned())
    }
}

#[async_trait]
impl OraclePeerCredentials for ServerOraclePeerCredentials {
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
