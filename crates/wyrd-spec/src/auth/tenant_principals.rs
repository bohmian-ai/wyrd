//! Wire contract for tenant-scoped principal and credential administration.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{PrincipalId, SecretBearer};
use crate::reference::CardRef;

/// Request to create a tenant-scoped machine principal.
///
/// The principal binds no Card: it is tenant automation — a CI runner, a
/// deployment agent — rather than a deployed workload with an emit scope.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateServicePrincipalRequest {
    /// Operator-facing name, unique within the tenant.
    pub name: String,
    /// Roles to grant. Narrower than tenant administration, so automation never
    /// needs the tenant administrative credential.
    pub roles: Vec<String>,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// A created machine principal and its first credential.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateServicePrincipalResponse {
    /// Durable principal id, stable across credential rotation.
    pub principal_id: PrincipalId,
    /// Its first credential, returned exactly once.
    #[schemars(with = "String")]
    pub credential: SecretBearer,
}

/// A newly issued credential for an existing principal.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct IssuedCredential {
    /// Credential id, used to revoke it.
    pub id: Uuid,
    /// The plaintext, returned exactly once.
    #[schemars(with = "String")]
    pub credential: SecretBearer,
}

/// Non-secret metadata for one credential.
///
/// Rotation works by overlap — issue, verify, then revoke — so a listing shows
/// revoked and expired credentials too; an operator mid-rotation needs to see
/// that the superseded one really is gone.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CredentialMetadata {
    /// Credential id.
    pub id: Uuid,
    /// Non-secret lookup prefix.
    pub prefix: String,
    /// Creation time, RFC 3339.
    pub created_at: String,
    /// Expiry, when bounded.
    pub expires_at: Option<String>,
    /// Revocation time, when revoked.
    pub revoked_at: Option<String>,
    /// Last successful use.
    pub last_used_at: Option<String>,
}

/// A principal's credential metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CredentialListResponse {
    /// Credentials, newest first. Never any secret material.
    pub credentials: Vec<CredentialMetadata>,
}

/// Arguments naming the principal whose credentials to list.
///
/// The MCP tool advertises this type as its input schema and parses that same
/// schema back out, so an agent reading the catalog and the server reading the
/// call can never disagree about the shape. It lives here, beside the response
/// it leads to, rather than in the MCP surface, because the surface projects
/// the administrative contract instead of restating it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListCredentialsArgs {
    /// Principal whose credentials to list. Advertised as a UUID string, so a
    /// schema-valid argument is always one the server can act on.
    pub principal_id: PrincipalId,
}

/// Arguments naming one credential and the principal that owns it.
///
/// The principal is named as well as the credential so a credential id alone
/// cannot retire a credential belonging to a different principal.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeCredentialArgs {
    /// Principal that owns the credential.
    pub principal_id: PrincipalId,
    /// Credential to retire.
    pub credential_id: Uuid,
}

/// Acknowledgement that one credential was retired.
///
/// Revocation has nothing to return but the fact that it happened: the
/// credential is gone, and its metadata is no longer worth projecting. Naming
/// the credential back is what lets an agent confirm it retired the one it
/// meant to.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CredentialRevoked {
    /// Always true; the call fails rather than reporting a refusal here.
    pub revoked: bool,
    /// The credential that was retired.
    pub credential_id: Uuid,
}

/// Body of `POST /v1/auth/grant-role`: grant one Role to a Card-bound principal.
///
/// The target is named by its Card because a Service or Agent principal is the
/// Card's projection; a human user is never a target, since federated login
/// replaces their roles on every sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GrantRoleRequest {
    /// Service or Agent Card whose principal receives the Role.
    pub card_ref: CardRef,
    /// Name of a built-in or tenant Role, such as `workload`.
    pub role: String,
}

/// Response from `POST /v1/auth/grant-role`.
///
/// The grant takes effect at the principal's next key exchange; tokens already
/// minted keep the roles they were issued with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GrantRoleResponse {
    /// Principal the Role was granted to.
    pub principal_id: PrincipalId,
    /// The principal's stored Card binding.
    pub card_ref: CardRef,
    /// Every Role the principal now holds, ordered by name.
    pub roles: Vec<String>,
    /// `true` when this call added the Role, `false` when it was already held.
    pub granted: bool,
}
