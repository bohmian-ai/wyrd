//! Recently fenced batch identities, so an honest retry of a batch that is
//! already durable and visible is answered without re-entering a shard.
//!
//! A shard records each batch once its group has synced the WAL, committed
//! the batch fence, and inserted (or deliberately suppressed) its rows. The
//! ingress consults the index after preprocessing: a resend whose tenant,
//! table, batch id, and logical identity all match a recorded batch is
//! acknowledged at once with no first commit, instead of waiting behind every
//! fresh write's sync only to be suppressed by the fence. Any miss, eviction,
//! or mismatch takes the shard path, where the durable fence still decides,
//! so the index is never the authority for a first commit or a contradiction.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use uuid::Uuid;
use wyrd_spec::DataTenantId;

/// Most batch identities retained; the oldest is evicted first.
///
/// ponytail: fixed FIFO bound sized for retries that arrive within seconds of
/// their first attempt; an evicted resend only falls back to the shard path.
const CAPACITY: usize = 16_384;

/// One recorded batch's logical identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommittedBatch {
    /// Canonical logical table the batch was fenced for.
    pub(crate) table_fqn: String,
    /// Table version the batch was accepted under; a retry prepared under
    /// any other version is not this batch.
    pub(crate) table_version: wyrd_spec::vala::ids::TableVersion,
    /// Ordered logical slice-set digest the fence stores.
    pub(crate) logical_digest: [u8; 32],
    /// Closed slice-set cardinality the fence stores.
    pub(crate) slice_count: u32,
}

/// Bounded map from `(tenant, batch id)` to the batch's logical identity.
#[derive(Debug, Default)]
pub(crate) struct CommittedBatches {
    /// Recorded identities and their insertion order for FIFO eviction.
    inner: Mutex<Inner>,
}

/// Lock-guarded state of [`CommittedBatches`].
#[derive(Debug, Default)]
struct Inner {
    /// Recorded identity per tenant-scoped batch id.
    batches: HashMap<(DataTenantId, Uuid), CommittedBatch>,
    /// Keys in recording order; the front is evicted first.
    order: VecDeque<(DataTenantId, Uuid)>,
}

impl CommittedBatches {
    /// Records a batch whose fence is committed and whose rows are visible,
    /// evicting the oldest identity once the bound is reached.
    ///
    /// A batch already recorded keeps its first identity and position.
    pub(crate) fn record(&self, tenant: DataTenantId, batch_id: Uuid, batch: CommittedBatch) {
        let mut inner = self.lock();
        let key = (tenant, batch_id);
        if inner.batches.contains_key(&key) {
            return;
        }
        if inner.order.len() >= CAPACITY
            && let Some(oldest) = inner.order.pop_front()
        {
            inner.batches.remove(&oldest);
        }
        inner.batches.insert(key, batch);
        inner.order.push_back(key);
    }

    /// Whether `batch` matches the identity recorded for `(tenant, batch_id)`.
    ///
    /// Returns `false` when nothing is recorded or any field differs; the
    /// caller then defers to the shard and its durable fence.
    pub(crate) fn contains(
        &self,
        tenant: DataTenantId,
        batch_id: Uuid,
        batch: &CommittedBatch,
    ) -> bool {
        self.lock().batches.get(&(tenant, batch_id)) == Some(batch)
    }

    /// Whether anything is recorded for `(tenant, batch_id)`, so a caller can
    /// skip computing the logical identity of a fresh batch.
    pub(crate) fn knows(&self, tenant: DataTenantId, batch_id: Uuid) -> bool {
        self.lock().batches.contains_key(&(tenant, batch_id))
    }

    /// Takes the lock, recovering the map if a holder panicked; every update
    /// leaves it consistent, and a stale entry only routes a resend to the
    /// shard path.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test identity for `table` with digest byte `digest`.
    fn batch(table: &str, digest: u8) -> CommittedBatch {
        CommittedBatch {
            table_fqn: table.to_owned(),
            table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
            logical_digest: [digest; 32],
            slice_count: 1,
        }
    }

    /// Only an exact tenant, batch id, table, table version, and logical
    /// identity match; the
    /// first record wins and the oldest identity is evicted at the bound.
    #[test]
    fn matches_exact_identity_and_evicts_oldest() {
        let index = CommittedBatches::default();
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let first = Uuid::now_v7();
        index.record(tenant, first, batch("t.a", 1));
        index.record(tenant, first, batch("t.a", 2));
        assert!(index.contains(tenant, first, &batch("t.a", 1)));
        assert!(!index.contains(tenant, first, &batch("t.a", 2)));
        assert!(!index.contains(tenant, first, &batch("t.b", 1)));
        assert!(!index.contains(other, first, &batch("t.a", 1)));
        let mut next_version = batch("t.a", 1);
        next_version.table_version = next_version.table_version.next().expect("next version");
        assert!(!index.contains(tenant, first, &next_version));
        for _ in 0..CAPACITY {
            index.record(tenant, Uuid::now_v7(), batch("t.a", 3));
        }
        assert!(!index.knows(tenant, first));
    }
}
