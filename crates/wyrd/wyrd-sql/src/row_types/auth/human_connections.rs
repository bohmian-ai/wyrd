//! Row mirror for `wyrd.auth_human_connections`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// One tenant human OIDC login connection, including tombstones.
///
/// `client_secret_enc` is the sealing-keyring envelope returned byte for byte;
/// only the connection owner in `wyrd-auth` opens it. `jwks_uri`,
/// `tested_revision`, and `tested_until` are set only by a successful candidate
/// test, and `removed_at` marks a non-login tombstone whose secret was wiped.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HumanConnectionRow {
    /// Stable connection identifier, kept after removal.
    pub connection_id: Uuid,
    /// Owning tenant.
    pub data_tenant_id: Uuid,
    /// Tenant-monotonic configuration revision.
    pub revision: i64,
    /// `Candidate`, `Active`, or `Inactive`.
    pub state: String,
    /// Normalized OIDC issuer URL.
    pub issuer_url: String,
    /// Wyrd's client id at the provider; also the ID-token audience.
    pub client_id: String,
    /// `SecretBasic`, `SecretPost`, or `Public`.
    pub client_auth: String,
    /// Sealed client secret, absent for `Public` clients and tombstones.
    pub client_secret_enc: Option<Vec<u8>>,
    /// Claim mapping JSON (`subject`, optional `email` and `groups` paths).
    #[sqlx(json)]
    pub claim_mapping: Value,
    /// Provider group to tenant role names JSON.
    #[sqlx(json)]
    pub group_role_map: Value,
    /// JWKS key-cache TTL in seconds.
    pub jwks_ttl_secs: i64,
    /// JWKS endpoint discovered by the last successful test.
    pub jwks_uri: Option<String>,
    /// Revision the last successful test stamped.
    pub tested_revision: Option<i64>,
    /// Instant after which the test stamp no longer authorizes activation.
    pub tested_until: Option<DateTime<Utc>>,
    /// Tombstone instant; `None` for a live connection.
    pub removed_at: Option<DateTime<Utc>>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
}
