//! Row mirrors for `wyrd.auth_users`.

use sqlx::types::{Uuid, chrono};

/// SQL row for `wyrd.auth_users`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    /// User identifier.
    pub id: String,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// User email address.
    pub email: Option<String>,
    /// Optional password hash for password-backed users.
    pub password_hash: Option<String>,
    /// Authentication source.
    pub auth_type: String,
    /// User lifecycle status.
    pub status: String,
    /// Row creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Row update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
