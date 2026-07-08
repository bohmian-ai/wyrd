//! Row slots for Vala alert tables.

use sqlx::types::chrono;

/// A row from `vala.drift_alerts`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DriftAlertRow {
    pub data_tenant_id: uuid::Uuid,
    pub drift_ref_kind: String,
    pub drift_ref_name: String,
    pub drift_ref_ver: String,
    pub drift_ref_space: String,
    pub drift_type: String,
    pub series: Option<String>,
    pub alert: serde_json::Value,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
