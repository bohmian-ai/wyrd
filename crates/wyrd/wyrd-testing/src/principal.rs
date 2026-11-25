//! Principal-bootstrap results shared by the Wyrd test harness.
//!
//! [`WyrdTestServer`](crate::WyrdTestServer) seeds principals through fixture
//! SQL and hands callers one of these values instead of a bare id, so a test
//! carries the credential appropriate to the principal kind it created: a
//! minted access token for a user, an API key to exchange for a machine.

use secrecy::SecretString;
use wyrd_runtime::PrincipalId;
use wyrd_spec::reference::CardRef;

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

/// The bound within which a committed access change reaches every healthy
/// replica's authority cache.
///
/// The committing request does not mark its own replica's cache; the change
/// arrives by notification, so an observation of it is bounded, not
/// immediate.
pub const AUTHORITY_PROPAGATION: std::time::Duration = std::time::Duration::from_secs(5);

/// Probe until the observation equals `expected` or [`AUTHORITY_PROPAGATION`]
/// elapses, and return the last observation.
///
/// Callers assert on the result, so a change that never propagates fails with
/// what was last observed.
pub async fn within_propagation<T, F, Fut>(expected: &T, mut probe: F) -> T
where
    T: PartialEq,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let deadline = tokio::time::Instant::now() + AUTHORITY_PROPAGATION;
    loop {
        let observed = probe().await;
        if &observed == expected || tokio::time::Instant::now() >= deadline {
            return observed;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
