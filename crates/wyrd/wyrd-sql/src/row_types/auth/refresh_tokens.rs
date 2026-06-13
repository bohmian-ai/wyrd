//! Row mirrors for `wyrd.auth_refresh_tokens`.

use sqlx::types::{Uuid, chrono};

/// SQL row for `wyrd.auth_refresh_tokens`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshTokenRow {
    /// Refresh token identifier.
    pub id: String,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// User identifier.
    pub user_id: String,
    /// Hashed token material.
    pub token_hash: String,
    /// Issuance timestamp.
    pub issued_at: chrono::DateTime<chrono::Utc>,
    /// Expiration timestamp.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Previous refresh token identifier when rotated.
    pub rotated_from: Option<String>,
    /// Revocation timestamp.
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}
