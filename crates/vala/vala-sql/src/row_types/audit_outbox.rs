//! Row types for the Vala transactional audit outbox.

use sqlx::types::{Uuid, chrono};

/// One unshipped audit row returned by `vala.claim_unshipped_audit`.
///
/// Carries the full audited-operation fields plus the hash-chain columns the
/// relay needs to build the Iceberg batch. `data_tenant_id` and `seq` identify
/// the row; the relay groups by tenant and ships a contiguous `seq` range.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditOutboxRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Gapless per-tenant sequence number.
    pub seq: i64,
    /// SHA256 chain entry hash for this row.
    pub entry_hash: Vec<u8>,
    /// Chain entry hash of the previous row (32 zero bytes before row 1).
    pub prev_hash: Vec<u8>,
    /// Request correlation ID of the audited op.
    pub request_id: String,
    /// Distributed-trace ID, when present.
    pub trace_id: Option<String>,
    /// Logical operation name.
    pub operation: String,
    /// Target resource the op acted on.
    pub resource: String,
    /// Canonical writer-identity card string; `None` for a `User` principal.
    pub card_ref: Option<String>,
    /// Stable ID of the acting principal.
    pub principal_id: Uuid,
    /// Principal kind tag: `user`, `service`, or `agent`.
    pub principal_kind: String,
    /// Authentication method: `jwt` or `internal`.
    pub auth_method: String,
    /// Effective RBAC permission checked.
    pub permission: String,
    /// Authorization decision: `allow` or `deny`.
    pub decision: String,
    /// Operation result: `success` or `failure`.
    pub result: String,
    /// Redacted operation payload summary.
    pub payload_summary: String,
    /// Canonical JSON detail for the audited operation, when present.
    pub detail: Option<String>,
    /// Wall-clock append time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Relay batch idempotency key stamped at claim time; `None` for rows not yet
    /// claimed by the relay. On crash recovery the relay reuses this value instead
    /// of recomputing from the live seq range.
    pub ship_batch_id: Option<Vec<u8>>,
}
