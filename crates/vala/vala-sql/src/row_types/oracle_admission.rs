//! Typed inputs and outcomes for durable Oracle admission.

use chrono::{DateTime, Utc};
use wyrd_spec::{
    DataTenantId,
    vala::api::{AdmissionScope, FencingToken, NodeId, OracleAdmissionLease, QueryClass},
};

/// One atomic admission request and its three ceilings.
pub struct AdmissionRequest {
    /// Lease and selected nodes to insert.
    pub lease: OracleAdmissionLease,
    /// Global cluster slot ceiling.
    pub cluster_limit: u32,
    /// Query-class slot ceiling.
    pub class_limit: u32,
    /// Tenant-class slot ceiling.
    pub tenant_limit: u32,
}

/// Fenced leader identity for lease mutation.
pub struct RoleFence {
    /// Leader node identity.
    pub node_id: NodeId,
    /// Leader Oracle-role fence.
    pub fencing_token: FencingToken,
}

/// Outcome of acquiring durable admission.
pub enum AdmissionAcquire {
    /// Lease and all counters were committed.
    Acquired(OracleAdmissionLease),
    /// One scope lacked capacity.
    Rejected {
        /// First canonical scope that rejected demand.
        scope: AdmissionScope,
        /// Fixed bounded caller backoff.
        retry_after_ms: u64,
    },
}

/// Outcome of a fenced renewal or idempotent release.
pub enum LeaseMutation {
    /// Active matching lease was renewed.
    Renewed(OracleAdmissionLease),
    /// Matching lease and counters were released.
    Released,
    /// Release found no lease.
    AlreadyReleased,
    /// Renewal found no matching active lease.
    Missing,
    /// Present lease belonged to another leader fence.
    StaleLeaderFence,
}

/// Typed counter scope eligible for bounded reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AdmissionReconcileScope {
    /// One global cluster counter.
    Cluster,
    /// One query-class counter.
    Class {
        /// Class being reconciled.
        query_class: QueryClass,
    },
    /// One tenant-class counter.
    Tenant {
        /// Tenant being reconciled.
        data_tenant_id: DataTenantId,
        /// Class being reconciled.
        query_class: QueryClass,
    },
}

/// Result of one bounded expiry transaction.
pub struct AdmissionExpiryReport {
    /// Number of leases removed.
    pub expired_leases: u16,
    /// Total slot units released.
    pub released_slots: u64,
}

/// Result of bounded authoritative counter reconciliation.
pub struct AdmissionReconciliationReport {
    /// Counters whose stored value changed.
    pub repaired_scopes: u8,
    /// Counters already matching active leases.
    pub unchanged_scopes: u8,
}

/// Internal lease row locked for mutation.
#[derive(sqlx::FromRow)]
pub(crate) struct AdmissionLeaseRow {
    /// Query whose admission consumes the leased capacity.
    pub(crate) query_id: uuid::Uuid,
    /// Tenant scope against which the lease is accounted.
    pub(crate) data_tenant_id: uuid::Uuid,
    /// Persisted closed query-class label used to select the capacity counter.
    pub(crate) query_class: String,
    /// Capacity units reserved by this lease.
    pub(crate) slot_units: i32,
    /// Oracle leader that owns and may release the lease.
    pub(crate) leader_node_id: uuid::Uuid,
    /// Leader epoch that fences stale release and renewal attempts.
    pub(crate) leader_fencing_token: i64,
    /// Database timestamp at which capacity was first acquired.
    pub(crate) acquired_at: DateTime<Utc>,
    /// Database timestamp after which reconciliation may reclaim the capacity.
    pub(crate) expires_at: DateTime<Utc>,
}
