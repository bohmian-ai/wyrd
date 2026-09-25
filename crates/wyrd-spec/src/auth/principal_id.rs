//! Wire principal identifier.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Stable principal id on auth wire contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct PrincipalId(uuid::Uuid);

/// Reserved platform principal attributed to unauthenticated audit events
/// (pre-auth login attempts, platform-internal operations without a caller).
/// This UUID is a stable well-known sentinel — never a real user principal.
pub const PLATFORM_AUDIT_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::from_bytes([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x77, 0x79, 0x72, 0x64, 0x01,
]));

/// Reserved card-free `Service` principal that publishes gateway capture into
/// Bifrost.
///
/// It is an internal server-minted sentinel rather than a tenant record: only
/// `wyrd-server`'s internal capture path issues it, always for exactly one
/// tenant, with no Card reference, an empty Card-reference scope, the
/// informational `gateway_capture` Role, and exactly that tenant's two capture
/// table write grants. Verifiers reject any token that pairs this id with
/// another kind, a Card binding, another Role set, another permission set, or
/// a delegation chain, and no public issuance path may mint it.
pub const GATEWAY_CAPTURE_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::from_bytes([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x77, 0x79, 0x72, 0x64, 0x02,
]));

impl PrincipalId {
    /// Build from a UUID.
    #[must_use]
    pub const fn new(value: uuid::Uuid) -> Self {
        Self(value)
    }

    /// Borrow the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PrincipalId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::parse_str(value).map(Self)
    }
}
