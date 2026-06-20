//! Row mirrors for `wyrd.auth_api_keys`.

use sqlx::types::{Uuid, chrono};

/// SQL row for `wyrd.auth_api_keys`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    /// API key identifier.
    pub id: String,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Service account identifier.
    pub sa_id: String,
    /// Non-secret key prefix.
    pub prefix: String,
    /// Hashed key material.
    pub key_hash: String,
    /// Creating user identifier.
    pub created_by: String,
    /// Row creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last successful use timestamp.
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Expiration timestamp.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Revocation timestamp.
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}
