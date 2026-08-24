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

/// Projection candidate row from `vala.olap_projections`.
///
/// Loaded by the matcher to decide whether a projection is substitutable for
/// a source table scan. Freshness fields drive the matcher decision:
/// - LookupSet: requires `refresh_epoch == source_refresh_epoch` AND
///   `built_for_snapshot_id` matches the current source snapshot (exact).
/// - Rollup/MV: staleness decided by `commit_lag` versus the configured knob.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectionCandidateRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte projection identifier.
    pub projection_uid: Vec<u8>,
    /// Opaque 16-byte source table identifier.
    pub source_table_uid: Vec<u8>,
    /// Fully-qualified projection name (`namespace.name`).
    pub fqn: String,
    /// Projection kind: `rollup`, `mv`, or `lookup_set`.
    pub projection_kind: String,
    /// Current lifecycle state.
    pub projection_state: String,
    /// Refresh epoch at last successful refresh.
    pub refresh_epoch: i64,
    /// Source table's refresh epoch at last successful refresh.
    pub source_refresh_epoch: i64,
    /// Iceberg snapshot id the projection was built for (identity anchor, not numeric).
    pub built_for_snapshot_id: Option<i64>,
    /// Commit-lag since last refresh (Rollup/MV staleness knob input).
    pub commit_lag: i64,
    /// F14 guard: source schema fingerprint at last refresh (32 bytes).
    pub source_schema_fingerprint: Vec<u8>,
}
