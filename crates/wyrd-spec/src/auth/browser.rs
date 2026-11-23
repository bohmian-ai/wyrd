//! Browser-session contracts for `/auth/browser`.
//!
//! The server alone creates, resolves, expires and revokes browser sessions.
//! A session is an opaque random identifier carried only in a host-only
//! HttpOnly cookie and stored only by digest; no operation here returns it,
//! an OAuth token, or a key. Login and recovery accept a tenant route key plus
//! the applicable recovery input, never a principal or permissions. Session
//! metadata is the safe projection a browser page may render.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::DataTenantId;
use crate::auth::{AbsoluteUrl, PrincipalId, PrincipalKindTag, SecretBearer, SocialProvider};
use crate::ids::TenantSlug;

/// Body of `POST /auth/browser/login`: begin a tenant provider sign-in in
/// this browser, through the tenant's company SSO or, when `provider` names
/// one, the deployment's shared social provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLoginRequest {
    /// The tenant's route key; pre-login routing context only.
    pub tenant: TenantSlug,
    /// The enabled shared social provider to sign in with; absent for
    /// company SSO.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<SocialProvider>,
    /// The code of the invitation link being redeemed: the provider identity
    /// becomes a new User holding exactly the invitation's assignments.
    /// Absent for a returning sign-in; requires `provider`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub invitation_code: Option<SecretBearer>,
}

/// Response of `POST /auth/browser/login`.
///
/// The response also sets the short-lived login-transaction cookie the
/// provider callback requires; the provider state exists only inside the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLoginResponse {
    /// The tenant provider's authorization URL to send the browser to.
    pub authorization_url: AbsoluteUrl,
}

/// Body of `POST /auth/signup`: begin creating a workspace by proving, in
/// this browser, a mailbox through Wyrd email or a shared social provider
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum SignupBeginRequest {
    /// Prove the mailbox `email` with a Wyrd email link; the creator then
    /// chooses a tenant-local username and password.
    Email {
        /// The address to prove, as typed.
        email: String,
    },
    /// Prove an identity at an enabled shared provider; the creator signs in
    /// with it and holds no password.
    Social {
        /// The enabled shared social provider.
        provider: SocialProvider,
    },
}

/// Body of `POST /auth/signup/complete`: create the workspace the proven
/// identity will administer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SignupCompleteRequest {
    /// The new workspace's route key, unique across the deployment.
    pub slug: TenantSlug,
    /// The new workspace's human-readable name.
    pub display_name: String,
    /// The tenant-local administrator an email proof creates. Required for
    /// an email proof and refused for a social one, whose creator signs in
    /// with the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_credentials: Option<LocalCredentials>,
    /// The Turnstile token the signup page's widget produced; required when
    /// the deployment configures a challenge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_token: Option<String>,
}

/// The username and password of a workspace's first local administrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LocalCredentials {
    /// The tenant-local username.
    pub username: String,
    /// The password, 15–128 characters after NFC normalization.
    pub password: SecretBearer,
}

/// Response of `POST /auth/signup/complete`: the created workspace, whose
/// session cookie the response sets for its creator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SignupCompleteResponse {
    /// The created workspace's route key.
    pub tenant: TenantSlug,
}

/// Where an email proof stands, as `POST /auth/signup` and
/// `POST /auth/signup/verify` report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProofStatus {
    /// A message was sent; open its link, or paste its proof, in this
    /// browser.
    CheckEmail,
    /// The mailbox is proven; this browser holds the completion cookie.
    Proven,
}

/// Response of an email proof step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProofStatusResponse {
    /// Where the proof stands.
    pub status: ProofStatus,
}

/// Body of `POST /auth/signup/verify`: the proof an email signup message
/// carried, presented by the browser that began it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SignupVerifyRequest {
    /// The proof from the message's link fragment, or pasted.
    pub proof: SecretBearer,
}

/// Body of `POST /auth/browser/email/request`: send a proof to the
/// recovery address the signed-in User wants verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ContactRequest {
    /// The tenant's route key whose session asks.
    pub tenant: TenantSlug,
    /// The address to verify, as typed.
    pub email: String,
}

/// Body of `POST /auth/browser/email/verify`: the proof a contact message
/// carried, presented by the session and browser that asked for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ContactVerifyRequest {
    /// The tenant's route key whose session verifies.
    pub tenant: TenantSlug,
    /// The proof from the message's link fragment, or pasted.
    #[schemars(with = "String")]
    pub proof: SecretBearer,
}

/// Body of `POST /auth/browser/workspaces/request`: mail a discovery proof to
/// an address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRequest {
    /// The address whose workspaces to find, as typed.
    pub email: String,
}

/// Body of `POST /auth/browser/workspaces/verify`: the proof a discovery
/// message carried, presented by the browser that asked for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DiscoveryVerifyRequest {
    /// The proof from the message's link fragment, or pasted.
    #[schemars(with = "String")]
    pub proof: SecretBearer,
}

/// Query of `GET /auth/browser/workspaces`: the next page of a discovery the
/// browser's read cookie already holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DiscoveryQuery {
    /// The `next_cursor` of the previous page.
    pub cursor: String,
}

/// One workspace a verified address may sign in to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DiscoveredWorkspace {
    /// The workspace's route key.
    pub tenant: TenantSlug,
    /// The workspace's display name.
    pub display_name: String,
    /// The workspace's fixed same-origin sign-in page.
    pub login_url: String,
}

/// One bounded page of the workspaces a verified address may sign in to.
/// An empty page is a normal result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DiscoveryPage {
    /// Workspaces in route-key order, each once.
    pub workspaces: Vec<DiscoveredWorkspace>,
    /// Present when more workspaces may follow; pass it as `cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Body of `POST /auth/browser/api-key`: recover tenant access with an API
/// key.
///
/// The key itself establishes its tenant; `tenant` must name that same
/// tenant or the request is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserApiKeyRequest {
    /// The tenant's route key the recovery page was opened for.
    pub tenant: TenantSlug,
    /// The recovery API key; verified, never stored.
    #[schemars(with = "String")]
    pub api_key: SecretBearer,
}

/// Body of `POST /auth/browser/password/signup`: enroll a local User with a
/// one-time invitation code.
///
/// The invitation alone decides the User's tenant membership and Roles; the
/// request names no principal, email or Role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PasswordSignupRequest {
    /// The tenant's route key the invitation was issued for.
    pub tenant: TenantSlug,
    /// The one-time enrollment code.
    #[schemars(with = "String")]
    pub invitation_code: SecretBearer,
    /// The chosen tenant-local username, canonicalized by the server.
    pub username: String,
    /// The chosen password.
    #[schemars(with = "String")]
    pub password: SecretBearer,
}

/// Body of `POST /auth/browser/password/login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PasswordLoginRequest {
    /// The tenant's route key.
    pub tenant: TenantSlug,
    /// The local username, canonicalized by the server.
    pub username: String,
    /// The password.
    #[schemars(with = "String")]
    pub password: SecretBearer,
}

/// Body of `POST /auth/browser/password/change`, sent with the tenant's
/// local password session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PasswordChangeRequest {
    /// The tenant's route key whose session is changing its password.
    pub tenant: TenantSlug,
    /// The current password, verified before any change.
    #[schemars(with = "String")]
    pub current_password: SecretBearer,
    /// The new password.
    #[schemars(with = "String")]
    pub new_password: SecretBearer,
}

/// Body of `POST /auth/browser/password/reset`: choose a new password with an
/// operator-issued one-time reset code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    /// The tenant's route key the reset was issued for.
    pub tenant: TenantSlug,
    /// The one-time reset code.
    #[schemars(with = "String")]
    pub reset_code: SecretBearer,
    /// The new password.
    #[schemars(with = "String")]
    pub new_password: SecretBearer,
}

/// Query of `GET /auth/browser/session`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionQuery {
    /// Resolve only this tenant's session; without it every presented
    /// tenant session is resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantSlug>,
}

/// Body of `POST /auth/browser/logout`: end this browser's session for one
/// tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLogoutRequest {
    /// The tenant route key whose session cookie is revoked and cleared.
    pub tenant: TenantSlug,
}

/// How a browser session was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BrowserCredential {
    /// The tenant's identity provider signed the person in.
    Sso,
    /// A recovery API key signed the browser in.
    ApiKey,
    /// A Wyrd-owned local username and password signed the person in.
    Password,
    /// A shared social provider, such as Google, signed the person in.
    Social,
}

/// Safe metadata of one live tenant browser session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionView {
    /// The tenant's route key.
    pub tenant: TenantSlug,
    /// The tenant's isolation key.
    pub tenant_id: DataTenantId,
    /// The tenant's display name.
    pub tenant_name: String,
    /// The signed-in principal.
    pub principal_id: PrincipalId,
    /// That principal's kind.
    pub principal_kind: PrincipalKindTag,
    /// The principal's canonical local username; `null` for an SSO-only User
    /// or a Service.
    #[serde(default)]
    pub username: Option<String>,
    /// How the session was created.
    pub credential: BrowserCredential,
    /// The principal's current operations as `resource:action` names,
    /// resolved for this request; informational only, never authority.
    pub permissions: Vec<String>,
    /// The session's absolute expiry; it never slides.
    pub expires_at: DateTime<Utc>,
}

/// Response of `GET /auth/browser/session`: every live presented session,
/// one per tenant, never merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionsResponse {
    /// The live sessions, in the order their cookies were presented.
    pub sessions: Vec<BrowserSessionView>,
}
