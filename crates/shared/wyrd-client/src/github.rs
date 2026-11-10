//! Tenant GitHub connection: connect, read and disconnect the deployment's
//! configured GitHub App.
//!
//! The shared implementation every first-class SDK, the CLI, and MCP project.
//! It owns no durable state: the server verifies, associates, authorizes and
//! audits. Connecting returns GitHub's install URL, which completes only in a
//! browser signed in to Wyrd as the same principal; every response is the
//! redacted [`GithubConnectionView`] or the transaction receipt.

use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;

use reqwest::Method;

use crate::client::WyrdClient;

// The wire contract this handle speaks, re-exported so an SDK user reaches one
// module for the capability and its types.
pub use wyrd_spec::error::WyrdError;
pub use wyrd_spec::github_connection::{
    GithubAppIdentity, GithubConnectRequest, GithubConnectResponse, GithubConnectionState,
    GithubConnectionView, GithubRepositoryAccess,
};

/// The connection route.
const PATH: &str = "/v1/integrations/github";

/// Cheap-to-clone, tenant-scoped GitHub connection handle.
///
/// Shaped like [`OperatorConnections`](crate::OperatorConnections): one
/// `Arc`-shared authenticated client and discoverable inherent methods.
#[derive(Clone)]
pub struct GithubConnection {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl Debug for GithubConnection {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("GithubConnection").finish_non_exhaustive()
    }
}

impl GithubConnection {
    /// Connect the handle through the ambient client, as
    /// [`WyrdClient::from_global`] resolves it.
    ///
    /// # Errors
    /// Returns a Wyrd error when the global config file cannot be read or
    /// parsed, no credential can be resolved, or a required exchange is
    /// refused or unreachable.
    pub async fn connect() -> Result<Self, WyrdError> {
        Ok(Self {
            client: Arc::new(WyrdClient::connect_ambient().await?),
        })
    }

    /// Connect the handle as `client`'s principal, used as given.
    ///
    /// # Arguments
    /// * `client` - The client every request is sent through.
    ///
    /// # Errors
    /// Returns the server's stable error when the client's required exchange
    /// is refused or unreachable.
    pub async fn connect_with(client: &WyrdClient) -> Result<Self, WyrdError> {
        Ok(Self {
            client: Arc::new(client.connected().await?),
        })
    }

    /// Begin a connection transaction and return GitHub's install URL and
    /// the instant it expires.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks
    /// `identity_connections:write`, no GitHub App is configured, or the
    /// request fails.
    pub async fn begin(&self) -> Result<GithubConnectResponse, WyrdError> {
        self.client
            .http
            .request_json(
                Method::POST,
                &format!("{PATH}/connect"),
                Some(&GithubConnectRequest {}),
            )
            .await
    }

    /// Read the tenant's redacted connection.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks
    /// `identity_connections:write`, no GitHub App is configured, or the
    /// request fails.
    pub async fn get(&self) -> Result<GithubConnectionView, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(Method::GET, PATH, None)
            .await
    }

    /// Fence the tenant's GitHub work and release its installation, returning
    /// the released view.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks
    /// `identity_connections:write`, no installation is associated, no GitHub
    /// App is configured, or the request fails.
    pub async fn disconnect(&self) -> Result<GithubConnectionView, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(Method::DELETE, PATH, None)
            .await
    }
}
