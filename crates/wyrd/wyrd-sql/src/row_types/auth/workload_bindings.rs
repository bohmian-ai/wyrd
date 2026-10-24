//! Row mirror for `wyrd.auth_workload_bindings`.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

/// SQL row for `wyrd.auth_workload_bindings`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkloadBindingRow {
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// OIDC issuer URL (part of the FK composite with `data_tenant_id`).
    pub issuer_url: String,
    /// Token subject claim that identifies this workload.
    pub subject: String,
    /// Optional audience override; `None` acts as a wildcard fallback.
    pub audience: Option<String>,
    /// Stable Service account the subject authenticates as.
    pub principal_id: Uuid,
    /// Optional pinned default root.
    pub workload_uid: Option<Uuid>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Row update timestamp.
    pub updated_at: DateTime<Utc>,
}
