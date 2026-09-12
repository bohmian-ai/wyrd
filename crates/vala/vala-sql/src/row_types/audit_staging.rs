//! Row types for Vala transactional audit staging.

use sqlx::types::{Uuid, chrono};

/// One immutable staged audit row awaiting publication.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditStagingRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Gapless per-tenant sequence number.
    pub seq: i64,
    /// SHA256 chain entry hash for this row.
    pub entry_hash: Vec<u8>,
    /// Chain entry hash of the previous row (32 zero bytes before row 1).
    pub prev_hash: Vec<u8>,
    /// Request correlation ID of the audited decision.
    pub request_id: String,
    /// Distributed-trace ID, when present.
    pub trace_id: Option<String>,
    /// Logical operation name.
    pub operation: String,
    /// Target resource the decision authorized or refused.
    pub resource: String,
    /// Canonical writer-identity card string; `None` for a `User` principal.
    pub card_ref: Option<String>,
    /// Stable ID of the acting principal.
    pub principal_id: Uuid,
    /// Principal kind tag: `user`, `service`, or `agent`.
    pub principal_kind: String,
    /// Effective dynamic permission the boundary evaluated.
    pub permission: String,
    /// Authorization outcome: `allowed` or `denied`.
    pub outcome: String,
    /// Canonical JSON detail for the audited operation, when present.
    pub detail: Option<String>,
    /// Wall-clock append time.
    pub created_at: chrono::DateTime<chrono::Utc>,
}
