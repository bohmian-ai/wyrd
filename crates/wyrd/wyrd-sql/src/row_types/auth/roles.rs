//! Row mirrors for `wyrd.auth_roles`.

use sqlx::types::{JsonValue, Uuid, chrono};

/// SQL row for `wyrd.auth_roles`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleRow {
    /// Role identifier.
    pub id: String,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Role name.
    pub name: String,
    /// Permission payload stored in JSONB.
    pub permissions: JsonValue,
    /// Whether the role is seeded by Wyrd.
    pub builtin: bool,
    /// Row creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}
