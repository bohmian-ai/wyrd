//! The tenant principal-administration handle.

use std::sync::Arc;

use reqwest::Method;
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    IssuedCredential, PrincipalId, RevokePrincipalRequest,
};
use wyrd_spec::error::WyrdError;

use crate::client::WyrdClient;

/// Cheap-to-clone, tenant-scoped principal administration handle.
///
/// Shaped like [`Cards`](crate::cards::Cards): one `Arc`-shared authenticated
/// client, discoverable inherent methods, no state of its own. Cloning only
/// bumps the `Arc`, so every clone speaks to the same server through the same
/// transport and token cache.
#[derive(Clone)]
pub struct Principals {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl std::fmt::Debug for Principals {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Principals").finish_non_exhaustive()
    }
}

impl Principals {
    /// Construct a handle around an already assembled client.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Construct a handle from the ambient client configuration.
    ///
    /// No network call or token exchange happens here; the first request does
    /// the exchange.
    ///
    /// # Errors
    /// Returns a Wyrd error when the local configuration or credential cannot
    /// be resolved.
    pub fn from_env() -> Result<Self, WyrdError> {
        let client = WyrdClient::from_env().map_err(WyrdError::from)?;
        Ok(Self::with_client(client))
    }

    /// Create a machine principal in the caller's tenant and receive its first
    /// credential.
    ///
    /// The tenant is never named in the request: the server takes it from the
    /// verified token, so a client cannot aim this at another tenant even by
    /// constructing the request by hand.
    ///
    /// The returned credential is the only time its plaintext exists outside
    /// the server. Store it before dropping the response.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration, a
    /// requested role does not exist in the tenant, or the server rejects the
    /// request.
    pub async fn create_service_principal(
        &self,
        request: &CreateServicePrincipalRequest,
    ) -> Result<CreateServicePrincipalResponse, WyrdError> {
        self.client
            .request_json(Method::POST, "/v1/principals", Some(request))
            .await
    }

    /// Issue an additional credential for an existing principal.
    ///
    /// The first half of a rotation: the principal now holds two live
    /// credentials, so the old one can be retired once the new one is verified.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the principal is
    /// unknown in this tenant.
    pub async fn issue_credential(
        &self,
        principal_id: &PrincipalId,
    ) -> Result<IssuedCredential, WyrdError> {
        self.client
            .request_json::<(), _>(
                Method::POST,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            )
            .await
    }

    /// List a principal's credential metadata, newest first.
    ///
    /// Includes revoked and expired credentials on purpose: an operator
    /// mid-rotation needs to see that the superseded one really is gone. No
    /// secret material is ever returned.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the read fails.
    pub async fn list_credentials(
        &self,
        principal_id: &PrincipalId,
    ) -> Result<CredentialListResponse, WyrdError> {
        self.client
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            )
            .await
    }

    /// Revoke a principal outright, ending every token it holds.
    ///
    /// The blunt instrument next to [`Self::revoke_credential`]: rather than
    /// retiring one credential, this advances the principal's authorization
    /// epoch, so tokens already minted and cached anywhere in the deployment
    /// stop verifying on their next use. Use it when the identity is
    /// compromised, not when a credential is merely being rotated.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration, the
    /// principal is unknown in this tenant, or the revocation decision cannot be
    /// audited.
    pub async fn revoke_principal(
        &self,
        principal_id: &PrincipalId,
        request: &RevokePrincipalRequest,
    ) -> Result<(), WyrdError> {
        self.client
            .request_json(
                Method::POST,
                &format!("/v1/principals/{principal_id}/revoke"),
                Some(request),
            )
            .await
    }

    /// Revoke one credential, leaving the principal and its roles untouched.
    ///
    /// The principal is named as well as the credential so a credential id
    /// alone cannot retire a credential belonging to a different principal.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the credential
    /// is not found for that principal.
    pub async fn revoke_credential(
        &self,
        principal_id: &PrincipalId,
        credential_id: &str,
    ) -> Result<(), WyrdError> {
        // Through the shared request owner like every other control call, so a
        // cached bearer the server has stopped accepting is re-exchanged and
        // the revoke replayed once rather than failing terminally. The 204 it
        // answers with decodes as the unit type: revocation's only outcome
        // worth reporting is whether it happened.
        self.client
            .request_json::<(), ()>(
                Method::DELETE,
                &format!("/v1/principals/{principal_id}/credentials/{credential_id}"),
                None,
            )
            .await?;
        Ok(())
    }
}
