//! Tenant GitHub App connection contracts.
//!
//! The deployment operator configures one GitHub App; a tenant connects to it
//! under `/v1/integrations/github`. [`GithubConnectResponse`] begins a one-use
//! connection transaction, GitHub's setup and OAuth callback redirects
//! ([`GithubSetupQuery`], [`GithubCallbackQuery`]) complete it, and
//! [`GithubConnectionView`] is the only read shape. No request carries a
//! tenant id, App key or provider host, and no view carries a code, token,
//! secret or signing material.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AbsoluteUrl;

/// `POST /v1/integrations/github/connect` body.
///
/// Deliberately empty and closed: the authenticated tenant, the configured
/// App and its configured host supply everything else, and the repositories
/// are the ones the tenant selects in GitHub and Wyrd then verifies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GithubConnectRequest {}

/// `POST /v1/integrations/github/connect` response: a durable, bounded
/// connection transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GithubConnectResponse {
    /// GitHub's install page for the configured App. It carries the
    /// transaction's one-use state, which completes only in a browser signed
    /// in to Wyrd as the principal that began it.
    pub authorization_url: AbsoluteUrl,
    /// Instant after which the transaction can no longer complete.
    pub expires_at: DateTime<Utc>,
}

/// Lifecycle state of a tenant's GitHub connection.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::IntoStaticStr,
    strum::Display,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GithubConnectionState {
    /// No installation is associated with the tenant.
    NotConnected,
    /// A verified installation is associated and usable.
    Active,
    /// GitHub suspended the associated installation; Wyrd uses none of its
    /// repositories until GitHub unsuspends it.
    Suspended,
}

/// The deployment's configured GitHub App.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GithubAppIdentity {
    /// GitHub App id.
    pub id: u64,
    /// GitHub App slug, as in `https://github.com/apps/<slug>`.
    pub slug: String,
}

/// A repository the tenant selected that its installation can currently
/// reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GithubRepositoryAccess {
    /// Stable provider repository id as a positive decimal string.
    pub repository_id: String,
    /// `owner/name` as GitHub last reported it; display only, not identity.
    pub full_name: String,
}

/// `GET /v1/integrations/github` response, and the result of a completed
/// callback or a disconnect: the redacted connection and access view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GithubConnectionView {
    /// Configured GitHub web host, such as `github.com`.
    pub host: String,
    /// The deployment's configured App.
    pub app: GithubAppIdentity,
    /// Connection state.
    pub state: GithubConnectionState,
    /// Verified installation id; `None` when not connected.
    pub installation_id: Option<u64>,
    /// Selected repositories the installation can currently reach, by id.
    pub repositories: Vec<GithubRepositoryAccess>,
    /// When the current association was verified; `None` when not connected.
    pub connected_at: Option<DateTime<Utc>>,
    /// Last change to the association or its access; `None` when not
    /// connected.
    pub updated_at: Option<DateTime<Utc>>,
}

/// `GET /v1/integrations/github/setup` query: GitHub's redirect after an
/// installation is created or changed.
///
/// The installation id is only a hint. `state` is present when the
/// installation was begun from Wyrd and absent when it was begun on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GithubSetupQuery {
    /// Installation GitHub says was installed or updated; unverified.
    pub installation_id: Option<u64>,
    /// GitHub's `install`, `update` or `request`; informational.
    pub setup_action: Option<String>,
    /// The connection transaction's state, when begun from Wyrd.
    pub state: Option<String>,
}

/// `GET /v1/integrations/github/callback` query: GitHub's OAuth redirect.
///
/// Exactly one of `code` or `error` is expected. An `installation_id` here is
/// only a hint, as at setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GithubCallbackQuery {
    /// Transient user authorization code.
    pub code: Option<String>,
    /// The connection transaction's one-use state.
    pub state: Option<String>,
    /// Installation GitHub reports alongside the authorization; unverified.
    pub installation_id: Option<u64>,
    /// GitHub's `setup_action` when it redirects here after installing.
    pub setup_action: Option<String>,
    /// OAuth error code when the user denied or GitHub refused.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{GithubConnectRequest, GithubConnectionState};

    /// The connect body is closed: a tenant id, key or host is refused.
    ///
    /// # Panics
    /// Panics when an unknown field is accepted.
    #[test]
    fn connect_request_refuses_every_field() {
        assert!(serde_json::from_str::<GithubConnectRequest>("{}").is_ok());
        for body in [
            r#"{"tenant_id":"x"}"#,
            r#"{"private_key":"k"}"#,
            r#"{"host":"evil.example"}"#,
        ] {
            assert!(serde_json::from_str::<GithubConnectRequest>(body).is_err(), "{body}");
        }
    }

    /// States serialize in snake case.
    ///
    /// # Panics
    /// Panics when a state is spelled differently.
    #[test]
    fn states_are_snake_case() {
        assert_eq!(
            serde_json::to_value(GithubConnectionState::NotConnected).expect("serializes"),
            "not_connected"
        );
        assert_eq!(GithubConnectionState::Suspended.to_string(), "suspended");
    }
}
