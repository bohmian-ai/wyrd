//! Principal-bootstrap results shared by the Wyrd test harness.
//!
//! [`WyrdTestServer`](crate::WyrdTestServer) seeds principals through fixture
//! SQL and hands callers one of these values instead of a bare id, so a test
//! carries the credential appropriate to the principal kind it created: a
//! minted access token for a user, an API key to exchange for a machine.

use axum::http::StatusCode;
use secrecy::SecretString;
use wyrd_auth_check::AuthzCheckResponse;
use wyrd_runtime::PrincipalId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;

/// Result of a fixture-path principal bootstrap.
#[derive(Debug, Clone)]
pub enum Bootstrap {
    /// Human User bootstrap.
    User {
        /// The bootstrapped user principal.
        id: PrincipalId,
        /// Tenant JWT for that user.
        jwt: String,
    },
    /// Service or Agent bootstrap.
    Machine {
        /// The bootstrapped machine principal.
        id: PrincipalId,
        /// API key the machine authenticates with.
        api_key: SecretString,
        /// Card the machine acts as.
        card_ref: CardRef,
    },
}

impl Bootstrap {
    /// Return the principal id.
    #[must_use]
    pub fn id(&self) -> PrincipalId {
        match self {
            Self::User { id, .. } | Self::Machine { id, .. } => *id,
        }
    }

    /// Return the card ref for machine principals.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Machine { card_ref, .. } => Some(card_ref),
            Self::User { .. } => None,
        }
    }

    /// Return the minted access token for user principals.
    ///
    /// Machine principals authenticate by exchanging an API key rather than by
    /// carrying a pre-minted token, so they return `None` here.
    #[must_use]
    pub fn jwt(&self) -> Option<&str> {
        match self {
            Self::User { jwt, .. } => Some(jwt),
            Self::Machine { .. } => None,
        }
    }

    /// Return the API key for machine principals.
    #[must_use]
    pub fn api_key(&self) -> Option<&SecretString> {
        match self {
            Self::Machine { api_key, .. } => Some(api_key),
            Self::User { .. } => None,
        }
    }
}

/// Result of an authz-check request.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// HTTP status returned by `/v1/authz/check`.
    pub status: StatusCode,
    /// Request id emitted by the request-id middleware.
    pub wyrd_request_id: RequestId,
    /// Parsed authz-check response body when the route returned a valid JSON payload.
    pub response: Option<AuthzCheckResponse>,
}
