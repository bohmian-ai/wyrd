//! Operator connection management: the tenant's encrypted provider credentials.
//!
//! The shared implementation every first-class SDK, the CLI, and MCP project.
//! It owns no durable state: the server validates, encrypts, authorizes, and
//! audits each request. Secrets travel only inside write-only
//! [`SecretBearer`] request fields, and every response is the redacted
//! [`OperatorConnectionView`].

use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;

use reqwest::Method;

use crate::client::WyrdClient;

// The wire contract this handle speaks, re-exported so an SDK user reaches one
// module for the capability and its types.
pub use wyrd_spec::auth::SecretBearer;
pub use wyrd_spec::error::WyrdError;
pub use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
pub use wyrd_spec::operator_connection::{
    CreateOperatorConnectionRequest, HttpAuthScheme, HttpConnectionAuth, HttpsOrigin,
    OperatorConnectionConfig, OperatorConnectionStatus, OperatorConnectionView, OperatorProvider,
    UpdateOperatorConnectionRequest,
};

/// Cheap-to-clone, tenant-scoped Operator connection handle.
///
/// Shaped like [`Verification`](crate::Verification): one `Arc`-shared
/// authenticated client and discoverable inherent methods.
#[derive(Clone)]
pub struct OperatorConnections {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl Debug for OperatorConnections {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("OperatorConnections")
            .finish_non_exhaustive()
    }
}

impl OperatorConnections {
    /// Construct a handle around an already assembled client.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Construct a handle from the ambient client configuration.
    ///
    /// # Errors
    /// Returns a Wyrd error when the local configuration or credential cannot
    /// be resolved.
    pub fn from_env() -> Result<Self, WyrdError> {
        let client = WyrdClient::from_env().map_err(WyrdError::from)?;
        Ok(Self::with_client(client))
    }

    /// Create one connection and return its redacted view.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `operators:write`, the body
    /// is invalid, the name is taken for the provider, the server's active key
    /// is unavailable, or the request fails.
    pub async fn create(
        &self,
        request: &CreateOperatorConnectionRequest,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.client
            .request_json(Method::POST, "/v1/operator-connections", Some(request))
            .await
    }

    /// List the tenant's connections, redacted.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `operators:read` or the
    /// request fails.
    pub async fn list(&self) -> Result<Vec<OperatorConnectionView>, WyrdError> {
        self.client
            .request_json::<(), _>(Method::GET, "/v1/operator-connections", None)
            .await
    }

    /// Read one connection, redacted.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `operators:read`, the
    /// connection is not in the caller's tenant, or the request fails.
    pub async fn get(
        &self,
        connection_id: &OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.client
            .request_json::<(), _>(Method::GET, &Self::path(connection_id), None)
            .await
    }

    /// Patch one connection: omitted fields are preserved, a supplied secret
    /// replaces the stored one, and `status` disables or re-enables it.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `operators:write`, the
    /// connection is unknown, the provider tag differs, the update is invalid,
    /// the active key is unavailable, or the request fails.
    pub async fn update(
        &self,
        connection_id: &OperatorConnectionId,
        request: &UpdateOperatorConnectionRequest,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.client
            .request_json(Method::PATCH, &Self::path(connection_id), Some(request))
            .await
    }

    /// Disable one connection; it is never deleted.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `operators:write`, the
    /// connection is unknown, or the request fails.
    pub async fn disable(
        &self,
        connection_id: &OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.client
            .request_json::<(), _>(Method::DELETE, &Self::path(connection_id), None)
            .await
    }

    /// The item route for `connection_id`.
    fn path(connection_id: &OperatorConnectionId) -> String {
        format!("/v1/operator-connections/{connection_id}")
    }
}
