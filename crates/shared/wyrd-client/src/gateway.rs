//! Tenant gateway administration client.
//!
//! [`Gateway`] projects the `/v1/admin/gateway` routes onto typed
//! `wyrd-spec` contracts. The server owns authorization, audit, validation,
//! and persistence; this handle only chooses the route and decodes
//! the redacted response, so every language SDK, the CLI, and agents see the
//! same shapes and stable errors.
//!
//! Provider-credential mutation is deliberately absent. Submission carries a
//! provider key value, and revocation and deletion name a credential without
//! saying which source backs it, so neither can be offered to a first-class
//! SDK without also offering `ManagedSecret` mutation. All three therefore live
//! on [`crate::gateway_credential::CredentialWriter`], a separate module that
//! `wyrd-sdk-rust` does not re-export and the Python and TypeScript bindings
//! do not construct. This handle keeps the redacted credential reads.

use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::error::WyrdError;

/// Gateway contracts, so SDK callers construct every administration input
/// and decode every response without depending on `wyrd-spec` directly.
pub use wyrd_spec::auth::{AbsoluteUrl, PrincipalId};
pub use wyrd_spec::gateway::*;
pub use wyrd_spec::ids::{
    CredentialBindingName, ModelId, ProviderCredentialName, ProviderDeploymentName, ProviderId,
    RoleName, SecretBackendName,
};

use crate::WyrdClient;

/// Route prefix for provider credentials.
const CREDENTIALS: &str = "/v1/admin/gateway/provider-credentials";
/// Route prefix for provider deployments.
const DEPLOYMENTS: &str = "/v1/admin/gateway/provider-deployments";
/// Singleton fallback-policy route.
const FALLBACK: &str = "/v1/admin/gateway/fallback-policy";
/// Singleton governance-policy route.
const GOVERNANCE: &str = "/v1/admin/gateway/governance-policy";
/// Singleton capture-policy route.
const CAPTURE: &str = "/v1/admin/gateway/capture-policy";

/// Tenant gateway administration handle over one shared [`WyrdClient`].
///
/// Cloning is cheap; the underlying client shares its connection pool and
/// token cache. Repeated `put_*` calls replace the named resource and
/// concurrent writers resolve by commit order on the server.
#[derive(Debug, Clone)]
pub struct Gateway {
    /// Authenticated transport used for every administration request.
    client: WyrdClient,
}

impl Gateway {
    /// Bind the handle to an assembled client.
    #[must_use]
    pub fn new(client: WyrdClient) -> Self {
        Self { client }
    }

    /// Reads one redacted provider credential.
    ///
    /// # Errors
    /// Returns the server's permission, not-found, or availability error.
    pub async fn credential(
        &self,
        name: &ProviderCredentialName,
    ) -> Result<ProviderCredentialView, WyrdError> {
        self.send(Method::GET, &credential_path(name), None::<&()>)
            .await
    }

    /// Lists the tenant's redacted provider credentials ordered by name.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn credentials(&self) -> Result<Vec<ProviderCredentialView>, WyrdError> {
        self.send(Method::GET, CREDENTIALS, None::<&()>).await
    }

    /// Creates or replaces a provider deployment.
    ///
    /// # Errors
    /// Returns the server's permission, invalid-configuration, or availability
    /// error.
    pub async fn put_deployment(
        &self,
        deployment: &ProviderDeployment,
    ) -> Result<ProviderDeployment, WyrdError> {
        self.send(
            Method::PUT,
            &deployment_path(&deployment.name),
            Some(deployment),
        )
        .await
    }

    /// Reads one provider deployment.
    ///
    /// # Errors
    /// Returns the server's permission, not-found, or availability error.
    pub async fn deployment(
        &self,
        name: &ProviderDeploymentName,
    ) -> Result<ProviderDeployment, WyrdError> {
        self.send(Method::GET, &deployment_path(name), None::<&()>)
            .await
    }

    /// Lists the tenant's provider deployments ordered by name.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn deployments(&self) -> Result<Vec<ProviderDeployment>, WyrdError> {
        self.send(Method::GET, DEPLOYMENTS, None::<&()>).await
    }

    /// Deletes a provider deployment; an absent name succeeds.
    ///
    /// # Errors
    /// Returns the server's invalid-name, permission, or availability error.
    pub async fn delete_deployment(&self, name: &ProviderDeploymentName) -> Result<(), WyrdError> {
        self.send(Method::DELETE, &deployment_path(name), None::<&()>)
            .await
    }

    /// Replaces the tenant fallback policy.
    ///
    /// # Errors
    /// Returns the server's permission, invalid-configuration, or availability
    /// error.
    pub async fn put_fallback_policy(
        &self,
        policy: &GatewayFallbackPolicy,
    ) -> Result<GatewayFallbackPolicy, WyrdError> {
        self.send(Method::PUT, FALLBACK, Some(policy)).await
    }

    /// Reads the tenant fallback policy, or the default when none is set.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn fallback_policy(&self) -> Result<GatewayFallbackPolicy, WyrdError> {
        self.send(Method::GET, FALLBACK, None::<&()>).await
    }

    /// Restores the default fallback policy.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn delete_fallback_policy(&self) -> Result<(), WyrdError> {
        self.send(Method::DELETE, FALLBACK, None::<&()>).await
    }

    /// Replaces the tenant governance policy.
    ///
    /// # Errors
    /// Returns the server's permission, invalid-configuration, or availability
    /// error.
    pub async fn put_governance_policy(
        &self,
        policy: &GatewayGovernancePolicy,
    ) -> Result<GatewayGovernancePolicy, WyrdError> {
        self.send(Method::PUT, GOVERNANCE, Some(policy)).await
    }

    /// Reads the tenant governance policy, or the default when none is set.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn governance_policy(&self) -> Result<GatewayGovernancePolicy, WyrdError> {
        self.send(Method::GET, GOVERNANCE, None::<&()>).await
    }

    /// Restores the default governance policy.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn delete_governance_policy(&self) -> Result<(), WyrdError> {
        self.send(Method::DELETE, GOVERNANCE, None::<&()>).await
    }

    /// Replaces the tenant capture policy and returns its versioned view.
    ///
    /// # Errors
    /// Returns the server's permission, invalid-configuration, or availability
    /// error.
    pub async fn put_capture_policy(
        &self,
        policy: &GatewayCapturePolicyWrite,
    ) -> Result<GatewayCapturePolicy, WyrdError> {
        self.send(Method::PUT, CAPTURE, Some(policy)).await
    }

    /// Reads the tenant capture policy, or the disabled default.
    ///
    /// # Errors
    /// Returns the server's permission or availability error.
    pub async fn capture_policy(&self) -> Result<GatewayCapturePolicy, WyrdError> {
        self.send(Method::GET, CAPTURE, None::<&()>).await
    }

    /// Sends one JSON administration request through the shared transport.
    ///
    /// # Errors
    /// Returns the server's stable error or a transport/decoding failure.
    async fn send<S: Serialize, D: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<D, WyrdError> {
        self.client.request_json(method, path, body).await
    }
}

/// Route for one named credential; names are grammar-validated and URL-safe.
pub(crate) fn credential_path(name: &ProviderCredentialName) -> String {
    format!("{CREDENTIALS}/{}", name.as_str())
}

/// Route for one named deployment; names are grammar-validated and URL-safe.
fn deployment_path(name: &ProviderDeploymentName) -> String {
    format!("{DEPLOYMENTS}/{}", name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Named resource routes stay under the versioned gateway administration prefix.
    #[test]
    fn named_routes_use_the_admin_gateway_prefix() {
        let credential = ProviderCredentialName::new("primary").expect("valid name");
        let deployment = ProviderDeploymentName::new("primary").expect("valid name");
        assert_eq!(
            credential_path(&credential),
            "/v1/admin/gateway/provider-credentials/primary"
        );
        assert_eq!(
            deployment_path(&deployment),
            "/v1/admin/gateway/provider-deployments/primary"
        );
    }
}
