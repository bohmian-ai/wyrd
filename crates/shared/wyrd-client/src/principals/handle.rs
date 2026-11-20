//! The tenant principal-administration handle.

use std::sync::Arc;

use reqwest::Method;
use uuid::Uuid;
use wyrd_spec::auth::{
    AccessSummary, CreateGroupRequest, CreateRoleBindingRequest, CreateServicePrincipalRequest,
    CreateServicePrincipalResponse, CreateUserInvitationRequest, CreateUserRequest, CreatedUser,
    CredentialListQuery, CredentialListResponse, CurrentUserProfile, GroupList, GroupView,
    IssuedCredential, IssuedUserInvitation, PrincipalId, PrincipalPage, PrincipalQuery,
    PrincipalSummary, PutRoleRequest, RevokePrincipalRequest, RoleBindingList, RoleBindingQuery,
    RoleBindingView, RoleList, RoleView, WorkloadRestriction,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{AccessGroupId, RoleBindingId};

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
    /// Connect the handle through the ambient client, as
    /// [`WyrdClient::from_global`] resolves it.
    ///
    /// Completes any credential exchange the resolved credential requires
    /// before returning; a presented-as-is bearer or fresh cached token is
    /// reused without a request.
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
    /// * `client` - The client every request is sent through, such as a
    ///   delegated client from [`WyrdClient::on_behalf_of`].
    ///
    /// # Errors
    /// Returns the server's stable error when the client's required exchange
    /// is refused or unreachable.
    pub async fn connect_with(client: &WyrdClient) -> Result<Self, WyrdError> {
        Ok(Self {
            client: Arc::new(client.connected().await?),
        })
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

    /// Create a local password User holding the named tenant Roles.
    ///
    /// # Arguments
    /// * `request` - The username, password and Role names.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks User management and tenant
    /// administration, the username or password is outside policy, the
    /// username is taken, or a Role does not exist in the tenant.
    pub async fn create_user(&self, request: &CreateUserRequest) -> Result<CreatedUser, WyrdError> {
        self.client
            .http
            .request_json(Method::POST, "/v1/principals/users", Some(request))
            .await
    }

    /// Invite a person to sign up as a local User holding the named Roles;
    /// the one-time code is returned once.
    ///
    /// # Arguments
    /// * `request` - The Role names the enrolled User receives.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks User management and tenant
    /// administration or a Role does not exist in the tenant.
    pub async fn invite_user(
        &self,
        request: &CreateUserInvitationRequest,
    ) -> Result<IssuedUserInvitation, WyrdError> {
        self.client
            .http
            .request_json(
                Method::POST,
                "/v1/principals/users/invitations",
                Some(request),
            )
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

    /// Read one page of a principal's credential metadata, newest first.
    ///
    /// Sends exactly one request: an omitted `limit` is sent as 50, and the
    /// response's `next_cursor`, when present, continues with the same
    /// `principal_id`. A User reads their own keys only this way.
    ///
    /// # Arguments
    /// * `principal_id` - The principal whose credentials are listed.
    /// * `query` - Page size and the previous page's continuation.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized, the cursor is
    /// malformed or names another principal, or the read fails.
    ///
    /// # Panics
    /// Never in practice: a query of optional scalar fields always
    /// URL-encodes.
    pub async fn list_credentials_page(
        &self,
        principal_id: &PrincipalId,
        query: CredentialListQuery,
    ) -> Result<CredentialListResponse, WyrdError> {
        let query = CredentialListQuery {
            limit: Some(query.limit.unwrap_or(50)),
            ..query
        };
        let query = serde_urlencoded::to_string(&query)
            .expect("a credential page query of optional scalars always URL-encodes");
        self.client
            .http
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/principals/{principal_id}/credentials?{query}"),
                None,
            )
            .await
    }

    /// Read the calling User's safe profile: tenant, username, account
    /// authentication method and informational effective permissions.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a User or is suspended.
    pub async fn me(&self) -> Result<CurrentUserProfile, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(Method::GET, "/v1/principals/me", None)
            .await
    }

    /// Replace a Service account's workload restriction atomically.
    ///
    /// A family restriction makes the account that family's unique scheduled
    /// runtime account. The account id, roles, and credentials are unchanged,
    /// and replaying the current value is idempotent.
    ///
    /// # Arguments
    /// * `principal_id` - The Service account to restrict.
    /// * `restriction` - The complete replacement restriction.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks tenant-administrative
    /// wildcard permission, the target is unknown or a User, or another live
    /// account already holds the family.
    pub async fn set_workload_restriction(
        &self,
        principal_id: &PrincipalId,
        restriction: &WorkloadRestriction,
    ) -> Result<PrincipalSummary, WyrdError> {
        self.client
            .http
            .request_json(
                Method::PUT,
                &format!("/v1/principals/{principal_id}/workload-restriction"),
                Some(restriction),
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

    /// Discover the tenant's assignable principals: users and Service
    /// accounts that are not deleted, ordered by id.
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
    ///
    /// # Panics
    /// Never in practice: a query of optional scalar fields always
    /// URL-encodes.
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

    /// One account's current access: every contributing direct, group and
    /// provider binding with its source, and the combined scoped permissions
    /// the server enforces.
    ///
    /// # Arguments
    /// * `principal_id` - The account whose access is summarized.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration or
    /// no assignable principal has this id in the caller's tenant.
    pub async fn access(&self, principal_id: &PrincipalId) -> Result<AccessSummary, WyrdError> {
        self.get(&format!("/v1/principals/{principal_id}/access"))
            .await
    }

    /// Every tenant role with its operations.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration.
    pub async fn roles(&self) -> Result<RoleList, WyrdError> {
        self.get("/v1/access/roles").await
    }

    /// Create or replace a custom role's operations; built-ins are read-only.
    ///
    /// # Arguments
    /// * `role` - The custom role name.
    /// * `request` - Its operations from the closed catalog.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator,
    /// the name is built-in or malformed, or the bundle is empty.
    pub async fn put_role(
        &self,
        role: &str,
        request: &PutRoleRequest,
    ) -> Result<RoleView, WyrdError> {
        self.client
            .http
            .request_json(
                Method::PUT,
                &format!("/v1/access/roles/{}", urlencoding::encode(role)),
                Some(request),
            )
            .await
    }

    /// Delete a custom role no binding references.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator, or
    /// the role is built-in, unknown or still bound.
    pub async fn delete_role(&self, role: &str) -> Result<RoleView, WyrdError> {
        self.send::<(), _>(
            Method::DELETE,
            &format!("/v1/access/roles/{}", urlencoding::encode(role)),
            None,
        )
        .await
    }

    /// List role bindings matching `query`.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration.
    pub async fn bindings(&self, query: &RoleBindingQuery) -> Result<RoleBindingList, WyrdError> {
        let query = serde_urlencoded::to_string(query)
            .map_err(|error| WyrdError::internal(format!("encode binding query: {error}")))?;
        let path = if query.is_empty() {
            "/v1/access/bindings".to_owned()
        } else {
            format!("/v1/access/bindings?{query}")
        };
        self.get(&path).await
    }

    /// Bind one role to one account or group at one explicit scope.
    ///
    /// Idempotent. The grant reaches the subject's existing credentials once
    /// the server's authority propagates, still bounded by each credential's
    /// ceiling.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator, the
    /// role or subject is unknown, or the role grants nothing at the scope.
    pub async fn bind(
        &self,
        request: &CreateRoleBindingRequest,
    ) -> Result<RoleBindingView, WyrdError> {
        self.send(Method::POST, "/v1/access/bindings", Some(request))
            .await
    }

    /// Remove one directly administered binding. Authority another binding
    /// grants remains.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator or
    /// the binding is unknown or provider-administered.
    pub async fn unbind(&self, binding_id: &RoleBindingId) -> Result<RoleBindingView, WyrdError> {
        self.send::<(), _>(
            Method::DELETE,
            &format!("/v1/access/bindings/{binding_id}"),
            None,
        )
        .await
    }

    /// Every local group with its members.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks principal administration.
    pub async fn groups(&self) -> Result<GroupList, WyrdError> {
        self.get("/v1/access/groups").await
    }

    /// Create an empty local group.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator or
    /// the name is empty or taken.
    pub async fn create_group(&self, request: &CreateGroupRequest) -> Result<GroupView, WyrdError> {
        self.send(Method::POST, "/v1/access/groups", Some(request))
            .await
    }

    /// Delete a group with its memberships and bindings.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator or
    /// the group is unknown.
    pub async fn delete_group(&self, group_id: &AccessGroupId) -> Result<GroupView, WyrdError> {
        self.send::<(), _>(
            Method::DELETE,
            &format!("/v1/access/groups/{group_id}"),
            None,
        )
        .await
    }

    /// Add one account to a group; idempotent.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is not a tenant administrator, or
    /// the group or account is unknown.
    pub async fn add_group_member(
        &self,
        group_id: &AccessGroupId,
        principal_id: &PrincipalId,
    ) -> Result<GroupView, WyrdError> {
        self.send::<(), _>(
            Method::PUT,
            &format!("/v1/access/groups/{group_id}/members/{principal_id}"),
            None,
        )
        .await
    }

    /// Remove one account from a group; idempotent.
    ///
    /// # Errors
    /// Returns the errors documented on [`Self::add_group_member`].
    pub async fn remove_group_member(
        &self,
        group_id: &AccessGroupId,
        principal_id: &PrincipalId,
    ) -> Result<GroupView, WyrdError> {
        self.send::<(), _>(
            Method::DELETE,
            &format!("/v1/access/groups/{group_id}/members/{principal_id}"),
            None,
        )
        .await
    }

    /// Send one bodyless read.
    ///
    /// # Errors
    /// Returns the server's stable Wyrd error.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, WyrdError> {
        self.send::<(), _>(Method::GET, path, None).await
    }

    /// Send one request with an optional JSON body.
    ///
    /// # Errors
    /// Returns the server's stable Wyrd error.
    async fn send<B: serde::Serialize + Sync, T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, WyrdError> {
        self.client.http.request_json(method, path, body).await
    }
}
