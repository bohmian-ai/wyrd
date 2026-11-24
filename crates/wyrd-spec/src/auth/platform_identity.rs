//! Wire contract for the platform-scope human identity plane.
//!
//! One deployment-owned OIDC connection, and the human platform principals
//! registered against it. Provider secrets travel inbound only: no response
//! here carries one, which is why the configure request and the connection view
//! are separate types rather than one round-tripped shape.

use serde::{Deserialize, Serialize};

use crate::auth::{PrincipalId, SecretBearer};

/// How Wyrd authenticates to the platform provider's token endpoint.
///
/// The secret arms carry [`SecretBearer`], not `String`. This type derives
/// `Debug`, and a plain `String` would print the provider's client secret into
/// any log line, trace span, or error that formatted the request.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "method")]
pub enum PlatformClientAuth {
    /// HTTP Basic with a client secret.
    SecretBasic {
        /// The client secret, sealed before storage and never returned.
        #[schemars(with = "String")]
        secret: SecretBearer,
    },
    /// Client secret in the request body.
    SecretPost {
        /// The client secret, sealed before storage and never returned.
        #[schemars(with = "String")]
        secret: SecretBearer,
    },
    /// A public client with no secret.
    Public,
}

/// Install or replace the deployment's platform OIDC connection.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConfigurePlatformOidcRequest {
    /// Issuer URL, pinned as the `iss` of every accepted token.
    ///
    /// The JWKS endpoint is not accepted from the caller. It is resolved once,
    /// here, by OIDC discovery against this issuer under the deployment's
    /// address screening — so configuring a connection cannot be used to point
    /// the server's key fetches at an arbitrary host.
    pub issuer_url: String,
    /// Wyrd's client identifier at the provider, and therefore the `aud`
    /// every accepted ID token must name.
    pub client_id: String,
    /// How Wyrd authenticates to the provider.
    pub client_auth: PlatformClientAuth,
    /// JWKS cache lifetime in seconds.
    #[serde(default = "default_jwks_ttl_secs")]
    pub jwks_ttl_secs: i64,
}

/// Default JWKS cache lifetime: long enough to avoid hammering the provider,
/// short enough that a key rotation is picked up without an operator action.
const fn default_jwks_ttl_secs() -> i64 {
    300
}

/// The configured connection, with nothing secret in it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PlatformOidcConnectionView {
    /// Issuer URL.
    pub issuer_url: String,
    /// JWKS endpoint.
    pub jwks_uri: String,
    /// Client identifier, which is also the ID-token audience.
    pub client_id: String,
    /// Client authentication method name only — never the secret itself.
    pub client_auth: String,
    /// JWKS cache lifetime in seconds.
    pub jwks_ttl_secs: i64,
}

/// Pre-register a human platform administrator.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RegisterPlatformAdminRequest {
    /// Operator-facing name, unique across the deployment.
    pub name: String,
    /// Claim value matched on this administrator's first login through the
    /// platform OIDC connection only. After that the pinned subject resolves
    /// them and this is no longer consulted. Omit it to register a platform
    /// `user` that signs in with a platform-user password instead
    /// (`PUT /platform/admins/{principal_id}/password`); no connection is then
    /// required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_claim: Option<String>,
}

/// Body of `PUT /platform/admins/{principal_id}/password`: set or replace a
/// platform `user`'s platform-user password.
///
/// The password follows the tenant local-password policy: 15–128 characters
/// after NFC normalization, no control characters and not blocklisted.
/// Replacing it ends every platform browser session it created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SetPlatformPasswordRequest {
    /// The new password; never stored or returned.
    #[schemars(with = "String")]
    pub password: SecretBearer,
}

/// Body of `POST /platform/browser/password/login`: sign a platform `user` in
/// to the platform console with its platform-user password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformPasswordLoginRequest {
    /// The platform principal's operator-facing name.
    pub username: String,
    /// The platform-user password.
    #[schemars(with = "String")]
    pub password: SecretBearer,
}

/// Body of `POST /platform/browser/password/reauth`: prove the signed-in
/// platform `user`'s password again before a destructive operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformPasswordReauthRequest {
    /// The signed-in platform user's password.
    #[schemars(with = "String")]
    pub password: SecretBearer,
}

/// How a platform browser session was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PlatformBrowserMethod {
    /// A platform-user password.
    Password,
    /// The platform OIDC connection.
    Oidc,
}

/// Response of `GET /platform/browser/methods`: the sign-in methods the
/// signed-out platform entry may offer. Hiding a method enforces nothing;
/// each sign-in path still decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformSignInMethods {
    /// Whether platform-user password sign-in is available.
    pub password: bool,
    /// Whether a platform OIDC connection exists, so OIDC sign-in can begin.
    pub oidc: bool,
}

/// Safe metadata of the platform browser session a request presents. It
/// carries no session identifier, API token, bootstrap key or provider token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PlatformBrowserSessionView {
    /// The signed-in platform principal.
    pub principal_id: PrincipalId,
    /// Its operator-facing name.
    pub name: String,
    /// How the session was created.
    pub method: PlatformBrowserMethod,
    /// The absolute deadline; activity never extends it.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// The idle deadline after this request's activity.
    pub idle_expires_at: chrono::DateTime<chrono::Utc>,
    /// Whether the session holds fresh human proof, which destructive
    /// platform operations require.
    pub recent: bool,
}

/// A registered human platform administrator, awaiting first login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RegisterPlatformAdminResponse {
    /// Durable principal id, stable across every later login.
    pub principal_id: PrincipalId,
}

/// Begin a platform federated login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformLoginRequest {
    /// Where the provider returns the browser after authentication.
    pub redirect_uri: String,
}

/// Complete a platform federated login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformCallbackRequest {
    /// Authorization code returned by the provider.
    pub code: String,
    /// Opaque state key issued when the login began.
    pub state: String,
    /// RFC 9207 authorization-response issuer identifier, when the provider
    /// sends one. It must equal the login's recorded issuer exactly, and is
    /// required when the provider's discovery advertises
    /// `authorization_response_iss_parameter_supported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

/// One platform principal as an operator sees it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PlatformPrincipalSummary {
    /// Durable principal id.
    pub principal_id: PrincipalId,
    /// Principal kind: `global_admin` for the deployment root, `user` for a
    /// human administrator.
    pub principal_kind: String,
    /// Operator-facing name.
    pub name: String,
    /// Lifecycle status: `active`, `suspended`, or `deleted`.
    pub status: String,
    /// Claim a human administrator was registered against.
    pub match_claim: Option<String>,
    /// Subject pinned at first login, once one has happened. `None` means this
    /// administrator has never signed in.
    pub subject: Option<String>,
}

/// Every platform principal, including the ones no longer permitted to act.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PlatformPrincipalListResponse {
    /// Principals, newest first.
    pub principals: Vec<PlatformPrincipalSummary>,
}

/// Change whether a platform principal may act.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SetPlatformPrincipalStatusRequest {
    /// `active` to restore, `suspended` to stop this principal acting.
    pub status: String,
}

/// Mint a new credential for an existing platform principal.
///
/// Rotation is issue-then-revoke, so this never retires what it replaces: the
/// operator verifies the new credential works and revokes the old one as a
/// separate, deliberate call.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssuePlatformCredentialRequest {
    /// Lifetime in days, or unbounded when omitted.
    ///
    /// The deployment root's first credential is unbounded by necessity — it
    /// exists before anything that could rotate it — but a credential minted
    /// afterwards usually should not be, so the bound is expressible here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<u32>,
}
