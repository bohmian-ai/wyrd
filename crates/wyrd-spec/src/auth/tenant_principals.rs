//! Wire contract for tenant-scoped principal and credential administration.

use serde::{Deserialize, Serialize};

use crate::auth::{PrincipalId, SecretBearer};

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
    pub id: String,
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
    pub id: String,
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
    /// Principal whose credentials to list, as a UUID.
    pub principal_id: String,
}

/// Arguments naming one credential and the principal that owns it.
///
/// The principal is named as well as the credential so a credential id alone
/// cannot retire a credential belonging to a different principal.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeCredentialArgs {
    /// Principal that owns the credential, as a UUID.
    pub principal_id: String,
    /// Credential to retire, as a UUID.
    pub credential_id: String,
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
    /// The credential that was retired, as a UUID.
    pub credential_id: String,
}
