//! Row types for Vala OLAP catalog control tables.

use sqlx::types::{Uuid, chrono};

/// Registered Bifrost table row from `vala.bifrost_tables`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BifrostTableRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Fully-qualified table name, unique per tenant.
    pub fqn: String,
    /// 32-byte schema fingerprint.
    pub fingerprint: Vec<u8>,
    /// Lifecycle status: `active`, `deprecated`, or `quarantined`.
    pub status: String,
    /// Canonical physical layout as the exact `PhysicalLayoutWire` JSON object.
    pub physical_layout: serde_json::Value,
    /// Wall-clock registration time.
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Audit origin (Stage 1: `system`; Stage 3: real identity).
    pub origin: Option<String>,
    /// Audit actor (Stage 1: `system`; Stage 3: real identity).
    pub actor: Option<String>,
}
