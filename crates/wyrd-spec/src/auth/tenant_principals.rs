//! Wire contract for tenant-scoped principal and credential administration.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{PrincipalId, PrincipalKindTag, SecretBearer, WorkloadRestriction};

/// Request to create a stable tenant Service account.
///
/// The account outlives every release: it is never a Card version. The exact
/// Service version a request runs as is selected per token or
/// request as workload context, within `workload_restriction`.
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
    /// Workload-family limit; omitted means unrestricted. A family restriction
    /// is that family's unique scheduled runtime account and additionally
    /// requires tenant-administrative wildcard permission.
    #[serde(default)]
    pub workload_restriction: Option<WorkloadRestriction>,
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
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Revocation time, when revoked.
    #[serde(default)]
    pub revoked_at: Option<String>,
    /// Last successful use.
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// Normalized name of a User key; absent for Service, Admin and platform
    /// credentials, whose authority is their principal's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<String>")]
    pub name: Option<String>,
    /// Permission ceiling of a User key in canonical sorted order; absent for
    /// Service, Admin and platform credentials, which carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<Vec<String>>")]
    pub permission_ceiling: Option<Vec<String>>,
}

/// A principal's credential metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CredentialListResponse {
    /// Credentials, newest first. Never any secret material.
    pub credentials: Vec<CredentialMetadata>,
    /// Continuation of a paged listing; present only when more rows remain
    /// and never on a complete (unpaged) listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<String>")]
    pub next_cursor: Option<String>,
}

/// Largest page of `GET /v1/principals/{principal_id}/credentials`.
pub const CREDENTIAL_PAGE_MAX: u32 = 100;

/// Page size a paged credential listing uses when it names none.
pub const CREDENTIAL_PAGE_DEFAULT: u32 = 50;

/// Longest credential continuation cursor accepted, in bytes.
pub const CREDENTIAL_CURSOR_MAX_BYTES: usize = 2048;

/// Query of `GET /v1/principals/{principal_id}/credentials`.
///
/// Supplying `limit` or `cursor` selects one bounded page, newest first with
/// the credential id as a stable tie-break; supplying neither keeps the
/// complete administrator listing. A cursor resumes strictly after the last
/// row it was issued for and binds the tenant and principal; it confers no
/// authority, which every page re-checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct CredentialListQuery {
    /// Page size, 1–100; a cursor alone pages by 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    /// Opaque continuation from the previous page's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 2048))]
    pub cursor: Option<String>,
}

impl CredentialListQuery {
    /// Whether this query selects page mode rather than the complete listing.
    #[must_use]
    pub const fn is_paged(&self) -> bool {
        self.limit.is_some() || self.cursor.is_some()
    }
}

/// Longest User credential name, in characters.
pub const USER_CREDENTIAL_NAME_MAX_CHARS: usize = 64;

/// Longest User credential lifetime: 365 days, in seconds.
pub const USER_CREDENTIAL_MAX_LIFETIME_SECONDS: u64 = 31_536_000;

/// Body of `POST /v1/principals/{principal_id}/credentials` for a User target.
///
/// A User key always carries a name, a nonempty explicit permission ceiling
/// and a bounded lifetime; there is no unrestricted User key. Service and
/// Admin targets keep their bodiless issuance and refuse this body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateUserCredentialRequest {
    /// Display name; trimmed, 1–64 characters.
    pub name: String,
    /// Distinct `resource:action` permission expressions bounding the key;
    /// each exchange grants their intersection with the User's current
    /// authority.
    pub permission_ceiling: Vec<String>,
    /// Lifetime in seconds, 1–31536000.
    #[schemars(range(min = 1, max = 31_536_000))]
    pub expires_in: u64,
}

/// How a User account authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccountAuthentication {
    /// A Wyrd-owned local username and password.
    Password,
    /// The tenant's identity provider.
    Sso,
}

/// Response of `GET /v1/principals/me`: the calling User's safe profile.
///
/// Derived from the verified caller and current User state; never an email,
/// password verifier, provider subject, session identifier or key secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CurrentUserProfile {
    /// The tenant's route key.
    pub tenant: crate::ids::TenantSlug,
    /// The tenant's isolation key.
    pub tenant_id: crate::DataTenantId,
    /// The User.
    pub principal_id: PrincipalId,
    /// Always `user`.
    pub principal_kind: PrincipalKindTag,
    /// Canonical local username; `null` for an SSO-only account.
    #[serde(default)]
    pub username: Option<String>,
    /// How the account authenticates, independent of this request's credential.
    pub authentication: AccountAuthentication,
    /// Sorted, distinct caller-effective `resource:action` names;
    /// informational only, never authority.
    pub permissions: Vec<String>,
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
    /// Page size, 1–100; with `cursor`, selects page mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    /// Continuation from the previous page's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 2048))]
    pub cursor: Option<String>,
}

/// Empty input of the read-only `principals.me` MCP tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurrentUserArgs {}

/// Arguments replacing one Service account's workload restriction.
///
/// The MCP projection of `PUT /v1/principals/{principal_id}/workload-restriction`,
/// whose HTTP body is the bare [`WorkloadRestriction`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetWorkloadRestrictionArgs {
    /// Service account whose restriction to replace.
    pub principal_id: PrincipalId,
    /// The complete replacement restriction.
    pub workload_restriction: WorkloadRestriction,
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

/// Body of `POST /v1/principals/users/invitations`.
///
/// The issuer's tenant is the invitation's tenant, and its lifetime is a fixed
/// twenty-four hours; neither is caller-selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateUserInvitationRequest {
    /// Distinct existing tenant Role names the enrolled User receives, such
    /// as `editor`; nonempty.
    pub roles: Vec<String>,
}

/// Body of `POST /v1/principals/users`: an administrator creates a local
/// password User directly, without an invitation.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateUserRequest {
    /// The tenant-local username, canonicalized by the server.
    pub username: String,
    /// The User's password, checked against the local password policy.
    #[schemars(with = "String")]
    pub password: SecretBearer,
    /// Distinct existing tenant Role names the User receives, such as `admin`.
    pub roles: Vec<String>,
}

/// The local User created by `POST /v1/principals/users`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreatedUser {
    /// The new User's principal id.
    pub principal_id: PrincipalId,
    /// The canonical username the User signs in with.
    pub username: String,
}

/// Safe metadata of one enrollment invitation; never its code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct UserInvitationView {
    /// Invitation id, used to revoke it.
    pub id: Uuid,
    /// The tenant's route key.
    pub tenant: crate::ids::TenantSlug,
    /// Role ids granted at enrollment, ordered by UUID.
    pub role_ids: Vec<Uuid>,
    /// Creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Absolute expiry, twenty-four hours after creation.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// A newly issued invitation and its one-time code, returned once.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssuedUserInvitation {
    /// Invitation metadata.
    pub invitation: UserInvitationView,
    /// The 43-character base64url enrollment code; delivered separately from
    /// the URL and never recoverable.
    #[schemars(with = "String")]
    pub invitation_code: SecretBearer,
    /// The fixed tenant signup page; it contains no code.
    pub signup_url: crate::auth::AbsoluteUrl,
}

/// Safe metadata of one operator-issued password reset; never its code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct UserPasswordResetView {
    /// Reset id.
    pub id: Uuid,
    /// The tenant's route key.
    pub tenant: crate::ids::TenantSlug,
    /// The local User the reset is for.
    pub principal_id: PrincipalId,
    /// Creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Absolute expiry, fifteen minutes after creation.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// A newly issued password reset and its one-time code, returned once.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssuedUserPasswordReset {
    /// Reset metadata.
    pub reset: UserPasswordResetView,
    /// The 43-character base64url reset code; delivered separately from the
    /// URL and never recoverable.
    #[schemars(with = "String")]
    pub reset_code: SecretBearer,
    /// The fixed tenant reset page; it contains no code.
    pub reset_url: crate::auth::AbsoluteUrl,
}

/// Where one of a principal's Role assignments came from.
///
/// A user's assignments are owned per source: federated login replaces only
/// `idp` rows on every sign-in, and a tenant administrator grants and revokes
/// only `direct` rows. Service assignments are always `direct`.
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
    /// Principal kind the server resolved from the id: `user` or `service`.
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
    /// `user` or `service`.
    pub kind: PrincipalKindTag,
    /// Lifecycle status.
    pub status: PrincipalStatus,
    /// Email, for users.
    #[serde(default)]
    pub email: Option<String>,
    /// Account name, for services.
    #[serde(default)]
    pub name: Option<String>,
    /// The account's workload restriction: explicit for a Service, null for a
    /// User.
    #[serde(default)]
    pub workload_restriction: Option<WorkloadRestriction>,
}

/// Response of `GET /v1/principals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PrincipalPage {
    /// Principals ordered by id, which is UUIDv7 and therefore creation order.
    pub principals: Vec<PrincipalSummary>,
    /// Cursor for the next page, present only when one may exist.
    #[serde(default)]
    pub next: Option<PrincipalId>,
}

/// Query of `GET /v1/principals`: exact-match filters and keyset paging.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct PrincipalQuery {
    /// `user` or `service`; omitted lists both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PrincipalKindTag>,
    /// Exact email; matches users only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Exact account name; matches services only.
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
