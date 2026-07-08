//! Row slots for Vala alert tables.

use sqlx::types::chrono;

/// A row from `vala.drift_alerts` — shared mutable alert table for all drift signal types.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DriftAlertRow {
    /// Tenant isolation key.
    pub data_tenant_id: uuid::Uuid,
    /// Card kind string (from `CardRef.kind.wire_name()`).
    pub drift_ref_kind: String,
    /// Card name string.
    pub drift_ref_name: String,
    /// Card version string.
    pub drift_ref_ver: String,
    /// Card space string.
    pub drift_ref_space: String,
    /// Discriminator: `spc`, `psi`, `custom`, or `eval`.
    pub drift_type: String,
    /// Feature or metric that alerted; `None` = aggregate alert.
    pub series: Option<String>,
    /// Alert payload (arbitrary JSON from the scoring layer).
    pub alert: serde_json::Value,
    /// Whether the alert is currently active.
    pub active: bool,
    /// Wall-clock creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
