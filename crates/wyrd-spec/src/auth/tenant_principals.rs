//! Wire contract for tenant-scoped principal and credential administration.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{PrincipalId, PrincipalKindTag, SecretBearer};
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

/// Where one of a principal's Role assignments came from.
///
/// A user's assignments are owned per source: federated login replaces only
/// `idp` rows on every sign-in, and a tenant administrator grants and revokes
/// only `direct` rows. Service and Agent assignments are always `direct`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RoleSource {
    /// Asserted by the tenant's identity provider at the user's last login.
    Idp,
    /// Granted by a tenant administrator.
    Direct,
}

impl RoleSource {
    /// Stable lowercase label, matching the serde representation and the
    /// durable `source` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idp => "idp",
            Self::Direct => "direct",
        }
    }
}

/// One Role a principal holds, and where that assignment came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleAssignment {
    /// Role name.
    pub role: String,
    /// Assignment source.
    pub source: RoleSource,
}

/// Response of `GET /v1/principals/{principal_id}/roles`.
///
/// The principal's effective Roles are the distinct names across every source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PrincipalRoles {
    /// Principal the assignments belong to.
    pub principal_id: PrincipalId,
    /// Principal kind the server resolved from the id: `user`, `service`, or
    /// `agent`.
    pub kind: PrincipalKindTag,
    /// Assignments ordered by Role name, then source.
    pub roles: Vec<RoleAssignment>,
}

/// Response of `PUT` and `DELETE /v1/principals/{principal_id}/roles/{role}`.
///
/// Both writes are idempotent. A change takes effect at the principal's next
/// token; tokens already issued keep their bounded lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleAssignmentChange {
    /// Principal the assignment belongs to.
    pub principal_id: PrincipalId,
    /// Principal kind the server resolved from the id.
    pub kind: PrincipalKindTag,
    /// Role the call granted or revoked.
    pub role: String,
    /// `true` when this call added or removed the direct assignment, `false`
    /// when it already held the requested state.
    pub changed: bool,
    /// Every assignment after the change, ordered by Role name, then source.
    pub roles: Vec<RoleAssignment>,
}

/// Lifecycle status of an assignable principal. Deleted principals are never
/// listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrincipalStatus {
    /// The principal can authenticate.
    Active,
    /// The principal is suspended; its Roles apply once it is reinstated.
    Suspended,
}

/// One assignable principal in a `GET /v1/principals` page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PrincipalSummary {
    /// Durable principal id.
    pub principal_id: PrincipalId,
    /// `user`, `service`, or `agent`.
    pub kind: PrincipalKindTag,
    /// Lifecycle status.
    pub status: PrincipalStatus,
    /// Email, for users.
    pub email: Option<String>,
    /// Principal name, for services and agents.
    pub name: Option<String>,
    /// Bound Card, for Card-bound services and agents.
    pub card_ref: Option<CardRef>,
}

/// Response of `GET /v1/principals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PrincipalPage {
    /// Principals ordered by id, which is UUIDv7 and therefore creation order.
    pub principals: Vec<PrincipalSummary>,
    /// Cursor for the next page, present only when one may exist.
    pub next: Option<PrincipalId>,
}

/// Query of `GET /v1/principals`: exact-match filters and keyset paging.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct PrincipalQuery {
    /// `user`, `service`, or `agent`; omitted lists all three.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PrincipalKindTag>,
    /// Exact email; matches users only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Exact principal name; matches services and agents only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Page size between 1 and 200; default 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 200))]
    #[cfg_attr(feature = "server", param(minimum = 1, maximum = 200))]
    pub limit: Option<u32>,
    /// Last `principal_id` of the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<PrincipalId>,
}
