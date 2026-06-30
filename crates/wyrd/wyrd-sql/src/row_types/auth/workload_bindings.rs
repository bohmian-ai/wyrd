//! Row mirror for `wyrd.auth_workload_bindings`.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use wyrd_spec::reference::CardRef;

/// SQL row for `wyrd.auth_workload_bindings`.
///
/// `card_ref` is stored as JSONB and decoded to the full structured [`CardRef`]
/// (kind, name, version, space, uid) — never a flattened string.
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
    /// Structured Card reference decoded from JSONB.
    #[sqlx(json)]
    pub card_ref: CardRef,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Row update timestamp.
    pub updated_at: DateTime<Utc>,
}
