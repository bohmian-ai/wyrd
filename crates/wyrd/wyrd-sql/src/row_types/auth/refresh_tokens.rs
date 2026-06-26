//! Row mirrors for `wyrd.auth_refresh_tokens`.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

/// SQL row for `wyrd.auth_refresh_tokens`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshTokenRow {
    /// Refresh token identifier.
    pub id: Uuid,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Principal kind: `user`, `service`, or `agent`.
    pub principal_kind: String,
    /// Stable principal identifier.
    pub principal_id: Uuid,
    /// SHA-256 hex digest of the raw JWT string.
    pub token_hash: String,
    /// Issuance timestamp.
    pub issued_at: DateTime<Utc>,
    /// Expiration timestamp.
    pub expires_at: DateTime<Utc>,
    /// Previous refresh token id when this row was created by rotation.
    pub rotated_from: Option<Uuid>,
    /// Revocation timestamp; `None` when the token is still active.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Human-readable revocation reason, for example `"rotated"` or `"reuse_detected"`.
    pub revoked_reason: Option<String>,
}
