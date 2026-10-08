//! The tenant principal-administration handle.

use std::sync::Arc;

use reqwest::Method;
use uuid::Uuid;
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    IssuedCredential, PrincipalId, PrincipalPage, PrincipalQuery, PrincipalRoles,
    RevokePrincipalRequest, RoleAssignmentChange,
};
use wyrd_spec::error::WyrdError;

use crate::client::WyrdClient;
use std::fmt::{Debug, Formatter, Result as FmtResult};

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

impl Debug for Principals {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Principals").finish_non_exhaustive()
    }
}

impl Principals {
    /// Construct a handle around an already assembled client.
    ///
    /// # Arguments
    /// * `client` - The authenticated client every request is sent through, as
    ///   its principal.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Construct a handle from the ambient client, as
    /// [`WyrdClient::from_global`] resolves it.
    ///
    /// No network call or token exchange happens here; the first request does
    /// the exchange.
    ///
    /// # Errors
    /// Returns a Wyrd error when the global config file cannot be read or
    /// parsed, or when no credential can be resolved.
    pub fn from_env() -> Result<Self, WyrdError> {
        let client = WyrdClient::from_global().map_err(WyrdError::from)?;
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
    /// # Arguments
    /// * `request` - The principal's name, the tenant roles it is granted, and an
    ///   optional description.
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
            .http
            .request_json(Method::POST, "/v1/principals", Some(request))
            .await
    }

    /// Issue an additional credential for an existing principal.
    ///
    /// The first half of a rotation: the principal now holds two live
    /// credentials, so the old one can be retired once the new one is verified.
    ///
    /// # Arguments
    /// * `principal_id` - The principal the new credential is issued for.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the principal is
    /// unknown in this tenant.
    pub async fn issue_credential(
        &self,
        principal_id: &PrincipalId,
    ) -> Result<IssuedCredential, WyrdError> {
        self.client
            .http
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
    /// # Arguments
    /// * `principal_id` - The principal whose credentials are listed.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the read fails.
    pub async fn list_credentials(
        &self,
        principal_id: &PrincipalId,
    ) -> Result<CredentialListResponse, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            )
            .await
    }

    /// Revoke a principal outright, suspending it so it mints no new tokens.
    ///
    /// The blunt instrument next to [`Self::revoke_credential`]: rather than
    /// retiring one credential, this suspends the principal so none of its
    /// credentials can exchange again and a user's refresh sessions end. Tokens
    /// it already holds are five-minute snapshots that lapse at expiry. Use it
    /// when the identity is compromised, not when a credential is merely being
    /// rotated.
    ///
    /// # Arguments
    /// * `principal_id` - The principal to suspend.
    /// * `request` - The principal's kind and the recorded revocation reason.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration or
    /// the principal is unknown in this tenant.
    pub async fn revoke_principal(
        &self,
        principal_id: &PrincipalId,
        request: &RevokePrincipalRequest,
    ) -> Result<(), WyrdError> {
        self.client
            .http
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
    /// # Arguments
    /// * `principal_id` - The principal that owns the credential.
    /// * `credential_id` - The credential to revoke.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the credential
    /// is not found for that principal.
    pub async fn revoke_credential(
        &self,
        principal_id: &PrincipalId,
        credential_id: Uuid,
    ) -> Result<(), WyrdError> {
        // Through the shared request owner like every other control call, so a
        // cached bearer the server has stopped accepting is re-exchanged and
        // the revoke replayed once rather than failing terminally. The 204 it
        // answers with decodes as the unit type: revocation's only outcome
        // worth reporting is whether it happened.
        self.client
            .http
            .request_json::<(), ()>(
                Method::DELETE,
                &format!("/v1/principals/{principal_id}/credentials/{credential_id}"),
                None,
            )
            .await?;
        Ok(())
    }

    /// Discover the tenant's assignable principals: users and Service and
    /// Agent principals that are not deleted, ordered by id.
    ///
    /// Filters are exact matches. Pass the returned page's `next` as
    /// `query.after` to read the following page; `next` is `None` on the last
    /// page.
    ///
    /// # Arguments
    /// * `query` - Optional `kind`, `email`, and `name` filters, a page size
    ///   between 1 and 200 (default 100), and the keyset cursor.
    ///
    /// # Errors
    /// Returns a Wyrd error when the query is invalid, the caller lacks
    /// principal administration, or the read fails.
    pub async fn list(&self, query: &PrincipalQuery) -> Result<PrincipalPage, WyrdError> {
        let query = serde_urlencoded::to_string(query)
            .expect("a principal query of optional scalars always URL-encodes");
        let path = if query.is_empty() {
            "/v1/principals".to_owned()
        } else {
            format!("/v1/principals?{query}")
        };
        self.client
            .http
            .request_json::<(), _>(Method::GET, &path, None)
            .await
    }

    /// List a principal's Role assignments and where each came from.
    ///
    /// A user may hold one Role from both its identity provider (`idp`) and
    /// an administrator (`direct`); its effective Roles are the union.
    ///
    /// # Arguments
    /// * `principal_id` - The principal whose assignments are listed.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration or
    /// no assignable principal has this id in the caller's tenant.
    pub async fn roles(&self, principal_id: &PrincipalId) -> Result<PrincipalRoles, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/principals/{principal_id}/roles"),
                None,
            )
            .await
    }

    /// Grant a principal a direct Role assignment.
    ///
    /// Idempotent: `changed` is `false` when the principal already held the
    /// Role directly. The Role reaches the principal's next token.
    ///
    /// # Arguments
    /// * `principal_id` - The principal to grant the Role to.
    /// * `role` - The built-in or tenant Role name.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator, the
    /// Role does not exist, or no assignable principal has this id.
    pub async fn grant_role(
        &self,
        principal_id: &PrincipalId,
        role: &str,
    ) -> Result<RoleAssignmentChange, WyrdError> {
        self.change_role(Method::PUT, principal_id, role).await
    }

    /// Revoke a principal's direct Role assignment.
    ///
    /// Idempotent: `changed` is `false` when the principal held no direct
    /// assignment of the Role. A user's `idp` assignment of the same Role is
    /// left for the identity provider. Tokens already issued keep their
    /// bounded lifetime; the next token omits the Role.
    ///
    /// # Arguments
    /// * `principal_id` - The principal to revoke the Role from.
    /// * `role` - The built-in or tenant Role name.
    ///
    /// # Errors
    /// Returns the errors documented on [`Self::grant_role`].
    pub async fn revoke_role(
        &self,
        principal_id: &PrincipalId,
        role: &str,
    ) -> Result<RoleAssignmentChange, WyrdError> {
        self.change_role(Method::DELETE, principal_id, role).await
    }

    /// Send one bodyless direct-assignment write.
    ///
    /// # Errors
    /// Returns the server's stable Wyrd error for a refused write.
    async fn change_role(
        &self,
        method: Method,
        principal_id: &PrincipalId,
        role: &str,
    ) -> Result<RoleAssignmentChange, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                method,
                &format!(
                    "/v1/principals/{principal_id}/roles/{}",
                    urlencoding::encode(role)
                ),
                None,
            )
            .await
    }
}
