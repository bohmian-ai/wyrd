//! Typed rows for durable Oracle admission policies and delegated blocks.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One canonical global or tenant query-class ceiling.
#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct OracleAdmissionPolicyRow {
    /// Durable scope discriminator.
    pub scope_kind: String,
    /// Tenant identity for tenant scope.
    pub data_tenant_id: Option<Uuid>,
    /// Durable query-class discriminator.
    pub query_class: String,
    /// Canonical maximum live delegated units.
    pub capacity: i32,
}

/// One accounting-level row in a three-scope delegated allocation.
#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct OracleAdmissionBlockRow {
    /// Unique row identity.
    pub block_id: Uuid,
    /// Identity shared by the global, tenant, and principal rows.
    pub allocation_id: Uuid,
    /// Durable accounting-level discriminator.
    pub scope_kind: String,
    /// Tenant charged by the allocation.
    pub data_tenant_id: Uuid,
    /// Principal charged only by the principal row.
    pub principal_id: Option<Uuid>,
    /// Durable query-class discriminator.
    pub query_class: String,
    /// Units delegated at every accounting level.
    pub units: i32,
    /// Physical holder node.
    pub holder_node_id: Uuid,
    /// Exact Oracle role-incarnation fence.
    pub holder_fencing_token: i64,
    /// Database-time beginning of uninterrupted validity.
    pub valid_from: DateTime<Utc>,
    /// Absolute database-time validity bound.
    pub expires_at: DateTime<Utc>,
    /// Database-time closure marker after continuity loss.
    pub closed_at: Option<DateTime<Utc>>,
}
