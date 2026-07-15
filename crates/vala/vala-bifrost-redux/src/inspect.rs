//! Test-inspect trait for multi-pod harness verification.
//!
//! Gated behind `scribe-inspect` feature — never compiled in production.

use async_trait::async_trait;
use vala_sql::TenantConn;

use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `event_day`).
pub type MemtableKey = SealKey;

/// Test-inspect trait exposing pod-local state for multi-pod harness verification.
///
/// Every method operates on a single pod's local state. Cross-pod aggregation is
/// the harness's job, not the trait's.
#[async_trait]
pub trait ScribeInspect {
    /// Sum of pending (un-fsynced or un-truncated) WAL bytes on this pod.
    fn wal_pending_bytes(&self) -> u64;

    /// Row count in the writable bucket for `key` on this pod; 0 if no bucket.
    fn memtable_row_count(&self, key: &MemtableKey) -> usize;

    /// Every Parquet path this pod has sealed since boot (opendal path string,
    /// relative to the Operator root).
    fn sealed_parquet_paths(&self) -> Vec<String>;

    /// Force-seal every non-empty memtable bucket on this pod and wait for the
    /// seal tx to commit. Returns once `wal_pending_bytes() == 0` and every
    /// `memtable_row_count(&k) == 0`.
    ///
    /// The caller provides a tenant-scoped connection for the seal transaction.
    async fn force_seal(&self, conn: &mut TenantConn<'_>) -> Result<(), ScribeError>;
}
