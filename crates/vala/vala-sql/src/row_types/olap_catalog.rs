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
    /// Table scope: `tenant_owned` or `system_shared`.
    pub scope: String,
    /// Lifecycle status: `active`, `deprecated`, or `quarantined`.
    pub status: String,
    /// Declared partition columns.
    pub partition_columns: Vec<String>,
    /// Wall-clock registration time.
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Audit origin (Stage 1: `system`; Stage 3: real identity).
    pub origin: Option<String>,
    /// Audit actor (Stage 1: `system`; Stage 3: real identity).
    pub actor: Option<String>,
}

/// 2PC anchor row from `vala.olap_commits`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OlapCommitRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Opaque 16-byte idempotency / anchor key.
    pub batch_id: Vec<u8>,
    /// Iceberg snapshot id — discovered post-commit; `NULL` while precommit.
    pub snapshot_id: Option<i64>,
    /// FSM state: `precommit` (in-flight), `committed` (terminal), `failed` (terminal),
    /// or `aborted` (terminal — set by the recovery path when no Iceberg snapshot is found).
    pub state: String,
    /// Wall-clock precommit time.
    pub precommit_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock commit time; set only on the `committed` transition.
    pub committed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Wall-clock finalization time; set on the `failed` transition.
    pub finalized_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Error code on failure.
    pub error_code: Option<String>,
    /// Error detail on failure.
    pub error_detail: Option<String>,
    /// Audit origin.
    pub origin: Option<String>,
    /// Audit actor.
    pub actor: Option<String>,
}

/// Claimed precommit row returned by `vala.claim_stale_precommits`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedPrecommitRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Opaque 16-byte idempotency / anchor key.
    pub batch_id: Vec<u8>,
    /// Monotonically increasing recovery fencing token stamped at claim time.
    pub fencing_token: i64,
    /// Fully-qualified table name (`namespace.name`).
    pub fqn: String,
    /// Namespace component derived from fqn.
    pub namespace: String,
    /// Name component derived from fqn.
    pub name: String,
    /// Table scope: `tenant_owned` or `system_shared`.
    pub scope: String,
}

/// Declared skip index row from `vala.olap_indexes`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeclaredIndexRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Column the index is declared over.
    pub column_name: String,
    /// Index kind: `bloom`, `zone_map`, `lookup_set`, or `none`.
    pub index_kind: String,
    /// Build state: `building`, `ready`, `failed`, or `deprecated`.
    pub index_state: String,
    /// Optional index parameters (e.g. bloom FPP).
    pub params: Option<serde_json::Value>,
    /// Wall-clock creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Per-entity min/max event-time bounds from `vala.entity_time_bounds`.
///
/// Best-effort acceleration for by-id lookups (M-06 / F-05). Present → set a
/// tight time window. Absent → fall back to the caller's `?since=` window or a
/// capped default. Never gate a read on presence; an absent bound is a miss, not
/// "entity does not exist."
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityTimeBoundsRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Entity kind: `trace`, `agent_run`, or `session`.
    pub entity_kind: String,
    /// Opaque entity identifier (e.g. trace_id hex string, dev_session_id).
    pub entity_id: String,
    /// Earliest known `wyrd_event_time` for this entity (over-approximation).
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    /// Latest known `wyrd_event_time` for this entity (over-approximation).
    pub max_event_time: chrono::DateTime<chrono::Utc>,
}

/// Cache-invalidation watermark row from `vala.refresh_epochs`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshEpochRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte table identifier.
    pub table_uid: Vec<u8>,
    /// Monotonically increasing epoch counter.
    pub epoch: i64,
    /// Wall-clock time of the last bump.
    pub bumped_at: chrono::DateTime<chrono::Utc>,
}
