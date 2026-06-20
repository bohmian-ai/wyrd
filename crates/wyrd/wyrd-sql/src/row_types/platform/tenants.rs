//! Row mirrors for `platform.tenants`.

use sqlx::types::{Uuid, chrono};

/// SQL row for `platform.tenants`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantRow {
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Human-visible tenant slug.
    pub slug: String,
    /// Display name shown in platform-admin surfaces.
    pub display_name: String,
    /// Tenant lifecycle status.
    pub status: String,
    /// Optional commercial plan label.
    pub plan: Option<String>,
    /// Row creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Row update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Soft deletion timestamp.
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}
