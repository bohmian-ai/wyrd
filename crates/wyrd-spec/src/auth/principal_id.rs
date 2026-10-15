//! Wire principal identifier.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Stable principal id on auth wire contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct PrincipalId(uuid::Uuid);

/// Reserved no-verified-caller sentinel: the subject and actor of a record no
/// verified principal caused — an audited credential-exchange refusal that
/// verified no identity, or a creation with no caller (SYSTEM bootstrap,
/// startup built-in reconciliation, the reserved audit owner's tables, the
/// SYSTEM backfill, fixture seeding). This UUID is a stable well-known
/// sentinel — never a real principal, never SYSTEM, and never authorizes work.
pub const PLATFORM_AUDIT_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::from_bytes([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x77, 0x79, 0x72, 0x64, 0x01,
]));

/// Reserved gateway-capture principal that no token may name.
///
/// Capture is a server-internal write: `wyrd-server` submits it straight to
/// Scribe without a token, permission check, or audit decision, and each
/// captured row carries the verified pair the call ran under. Like
/// [`PLATFORM_AUDIT_PRINCIPAL`], this id never appears in a token; issuers
/// refuse to mint it and verifiers reject any token naming it.
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
