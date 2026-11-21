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
    /// Username/password sign-in is offered.
    pub password: bool,
    /// The tenant's company SSO connection is offered.
    pub company_sso: bool,
    /// Enabled shared providers, in display order.
    pub providers: Vec<SocialProvider>,
    /// A visitor may create a new workspace with one of `providers`.
    pub signup: bool,
}
