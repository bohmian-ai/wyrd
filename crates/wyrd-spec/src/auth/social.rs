//! Wire contract for operator-managed social sign-in and deployment login
//! settings.
//!
//! A platform administrator configures one shared Google client and one shared
//! GitHub.com client for the whole deployment. Each configuration has a
//! revision; a changed client starts a new, untested revision, a real provider
//! sign-in marks exactly that revision tested, and only a tested revision can
//! be enabled. Client secrets travel inbound only: no response here carries
//! one.
//!
//! Login settings are the deployment-wide password and public-signup switches.
//! The login-method listing is the safe projection a sign-in page renders:
//! Rust computes it from these settings, the enabled providers and the tenant's
//! company connection, and enforces the same answer on every login path.
//!
//! The optional SMTP relay is the deployment's application mail: with one
//! configured, invitations are also emailed; its password, like a client
//! secret, travels inbound only.

use serde::{Deserialize, Serialize};

use crate::auth::{AbsoluteUrl, SecretBearer};
use crate::ids::TenantSlug;

/// A shared social sign-in provider an operator can enable.
///
/// Deliberately closed: Google and GitHub.com are the only shared providers,
/// not instances of a generic provider framework.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SocialProvider {
    /// Google sign-in through OpenID Connect.
    Google,
    /// GitHub.com sign-in through OAuth 2.0 and an authenticated user lookup.
    Github,
}

impl SocialProvider {
    /// Every shared provider, in display order.
    pub const ALL: [Self; 2] = [Self::Google, Self::Github];

    /// The stable lowercase name used in paths and storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
        }
    }

    /// Parse the stable lowercase name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.as_str() == name)
    }

    /// Path of this provider's fixed social callback on the public origin.
    #[must_use]
    pub const fn callback_path(self) -> &'static str {
        match self {
            Self::Google => "/auth/social/google/callback",
            Self::Github => "/auth/social/github/callback",
        }
    }
}

impl std::fmt::Display for SocialProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Body of `PUT /platform/social/providers/{provider}`: register or replace a
/// shared provider's OAuth client.
///
/// Storing a client always starts a new, untested, disabled revision, so a
/// replaced secret or client id is never used for sign-in before it passes a
/// real provider test.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConfigureSocialProviderRequest {
    /// The OAuth client id the provider console issued.
    pub client_id: String,
    /// The OAuth client secret; sealed before storage and never returned.
    #[schemars(with = "String")]
    pub client_secret: SecretBearer,
}

/// Lifecycle state of a shared provider's current revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SocialProviderState {
    /// Stored but not yet proven by a provider sign-in; not offered.
    Untested,
    /// Proven by a provider sign-in at this revision; not yet offered.
    Tested,
    /// Tested and offered on sign-in pages.
    Enabled,
}

/// A shared provider as operator management reads it, never with its secret.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SocialProviderView {
    /// Which provider.
    pub provider: SocialProvider,
    /// The configured OAuth client id, or `None` when unconfigured.
    pub client_id: Option<String>,
    /// The current configuration revision, or `None` when unconfigured.
    pub revision: Option<u64>,
    /// Lifecycle of the current revision, or `None` when unconfigured.
    pub state: Option<SocialProviderState>,
    /// The exact callback URL to register in the provider console.
    pub callback_url: AbsoluteUrl,
    /// What to do in the provider console before configuring Wyrd.
    pub registration_instructions: String,
}

/// Response of `GET /platform/social/providers`: every shared provider, in
/// display order, configured or not.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SocialProvidersResponse {
    /// Google, then GitHub.com.
    pub providers: Vec<SocialProviderView>,
}

/// Body of `POST /platform/social/providers/{provider}/test`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SocialProviderTestRequest {
    /// The revision the operator is testing; a newer revision refuses.
    pub expected_revision: u64,
}

/// Response of a begun provider test: where the operator's browser signs in.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SocialProviderTestResponse {
    /// The provider authorization URL for this one test sign-in.
    pub authorization_url: AbsoluteUrl,
}

/// Body of `PUT /platform/social/providers/{provider}/enabled`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EnableSocialProviderRequest {
    /// Whether the provider is offered for sign-in.
    pub enabled: bool,
    /// The revision being enabled; it must be the tested current revision.
    pub expected_revision: u64,
}

/// Deployment-wide login settings.
///
/// Password starts enabled and public signup starts disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LoginSettings {
    /// Whether tenant-local username/password sign-in is available.
    pub password_enabled: bool,
    /// Whether a visitor may create a new workspace with an enabled provider.
    pub signup_enabled: bool,
}

/// How Wyrd secures its connection to the SMTP relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SmtpTls {
    /// Plain SMTP; only for a relay on a trusted local network.
    None,
    /// Plain connection upgraded with `STARTTLS`, usually port 587.
    Starttls,
    /// TLS from the first byte (SMTPS), usually port 465.
    Tls,
}

impl SmtpTls {
    /// The stable label stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Starttls => "starttls",
            Self::Tls => "tls",
        }
    }

    /// Parse a stored label.
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "none" => Some(Self::None),
            "starttls" => Some(Self::Starttls),
            "tls" => Some(Self::Tls),
            _ => None,
        }
    }
}

/// Body of `PUT /platform/mail`: the deployment's one SMTP relay, replacing
/// any earlier one. Provider accounts and sending domains are set up outside
/// Wyrd; any standard SMTP relay works.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConfigureMailRelayRequest {
    /// Relay host name or address.
    pub host: String,
    /// Relay TCP port.
    pub port: u16,
    /// How the connection is secured.
    pub tls: SmtpTls,
    /// Username, when the relay requires authentication.
    #[serde(default)]
    pub username: Option<String>,
    /// Password for `username`; sealed before storage and never returned.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub password: Option<SecretBearer>,
    /// Sender address invitations are sent from, such as `invites@example.com`.
    pub sender: String,
}

/// The configured SMTP relay as management reads it, never its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct MailRelayView {
    /// Relay host name or address.
    pub host: String,
    /// Relay TCP port.
    pub port: u16,
    /// How the connection is secured.
    pub tls: SmtpTls,
    /// Username, when the relay authenticates.
    pub username: Option<String>,
    /// Whether a password is stored.
    pub password_set: bool,
    /// Sender address.
    pub sender: String,
}

/// Response of `GET`/`PUT /platform/mail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct MailSettings {
    /// The relay; `null` when Wyrd emails nothing and invitations are only
    /// copied.
    pub relay: Option<MailRelayView>,
}

/// Body of `POST /platform/mail/test`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct MailTestRequest {
    /// Address to send the test message to.
    pub recipient: String,
}

/// Response of `POST /platform/mail/test`. Acceptance means the relay took
/// the message, not that it reached an inbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct MailTestResponse {
    /// Whether the relay accepted the message.
    pub accepted: bool,
    /// The relay or connection error when it did not.
    pub error: Option<String>,
}

/// Query of `GET /auth/browser/methods`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LoginMethodsQuery {
    /// The tenant route key whose sign-in page is rendered, if any.
    #[serde(default)]
    pub tenant: Option<TenantSlug>,
}

/// The sign-in methods Rust currently allows, for a sign-in page to render.
///
/// Hiding a method never enforces anything: every login path rechecks the
/// same rules. An unknown tenant reads as a tenant without company SSO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LoginMethodsResponse {
    /// Returning sign-in on the tenant's page.
    #[serde(flatten)]
    pub sign_in: SignInMethods,
    /// Methods that prove a verified email address.
    #[serde(flatten)]
    pub email: EmailMethods,
    /// Workspace creation.
    #[serde(flatten)]
    pub creation: CreationMethods,
}

/// Returning sign-in methods of a [`LoginMethodsResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SignInMethods {
    /// The tenant's username/password sign-in is usable. Never offered
    /// without a tenant: password is not a deployment-wide login.
    pub password: bool,
    /// The tenant's Active company SSO connection is usable.
    pub company_sso: bool,
    /// Shared providers usable for returning sign-in and invitation
    /// acceptance, in display order.
    pub providers: Vec<SocialProvider>,
}

/// Email-proving methods of a [`LoginMethodsResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EmailMethods {
    /// A visitor may prove an email address to list the workspaces that
    /// address is a verified contact in.
    pub email_discovery: bool,
    /// A tenant password User may request a reset code at their verified
    /// contact.
    pub password_reset: bool,
}

/// Workspace-creation methods of a [`LoginMethodsResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreationMethods {
    /// At least one creation method is usable now: local signup or a
    /// provider in `signup_providers`.
    pub signup: bool,
    /// A visitor may create a workspace by proving an email address, then
    /// choosing a local username and password.
    pub local_signup: bool,
    /// Public workspace creation always proves a verified contact first.
    pub email_verification_required: bool,
    /// Shared providers usable to create a workspace, in display order.
    pub signup_providers: Vec<SocialProvider>,
}
