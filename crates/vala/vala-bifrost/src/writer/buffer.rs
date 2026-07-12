use std::collections::HashMap;
use std::collections::VecDeque;

use arrow::array::RecordBatch;
use wyrd_spec::ids::DataTenantId;

/// A batch queued from a single tenant write.
#[derive(Debug)]
pub struct TenantBatch {
    pub tenant: DataTenantId,
    pub batch: RecordBatch,
}

/// Per-tenant FIFO queue of buffered batches, with fair round-robin drain.
///
/// The group-commit coordinator buffers batches from N tenants writing to one
/// physical table. `drain_round_robin` returns one batch per tenant in
/// round-robin order (FIFO within a tenant), so no tenant can starve another
/// under high ingest from a single tenant (Q4).
pub struct TenantAppendBuffer {
    /// Per-tenant FIFO queues.
    queues: HashMap<DataTenantId, VecDeque<RecordBatch>>,
    /// Stable round-robin order: new tenants are appended when first seen.
    order: Vec<DataTenantId>,
    /// Total buffered row count across all tenants.
    total_rows: usize,
    /// Total buffered byte estimate (sum of batch memory sizes).
    total_bytes: usize,
}

impl TenantAppendBuffer {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            order: Vec::new(),
            total_rows: 0,
            total_bytes: 0,
        }
    }

    /// Push a batch into the per-tenant FIFO queue.
    pub fn push(&mut self, tenant: DataTenantId, batch: RecordBatch) {
        self.total_rows += batch.num_rows();
        self.total_bytes += batch_byte_estimate(&batch);
        let queue = self.queues.entry(tenant).or_default();
        if queue.is_empty() {
            // First batch for this tenant in this flush window — add to rotation.
            self.order.retain(|t| *t != tenant);
            self.order.push(tenant);
        }
        queue.push_back(batch);
    }

    /// Drain all queued batches grouped by tenant, in round-robin order.
    ///
    /// Returns one `Vec<(DataTenantId, Vec<RecordBatch>)>` per tenant that
    /// had buffered data. Order follows the insertion-time round-robin so all
    /// tenants get an equal slot in the output (Q4).
    pub fn drain_by_tenant(&mut self) -> Vec<(DataTenantId, Vec<RecordBatch>)> {
        let order = std::mem::take(&mut self.order);
        let mut queues = std::mem::take(&mut self.queues);
        self.total_rows = 0;
        self.total_bytes = 0;

        order
            .into_iter()
            .filter_map(|tenant| {
                let batches: Vec<RecordBatch> = queues.remove(&tenant)?.into();
                if batches.is_empty() {
                    None
                } else {
                    Some((tenant, batches))
                }
            })
            .collect()
    }

    /// Restore a previously-drained set of tenant batches on flush failure.
    ///
    /// Called when `run_group_commit` fails after draining; the coordinator
    /// returns batches to the buffer so the caller can retry.
    pub fn restore(&mut self, groups: Vec<(DataTenantId, Vec<RecordBatch>)>) {
        for (tenant, batches) in groups {
            for batch in batches {
                self.push(tenant, batch);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total_rows == 0
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl Default for TenantAppendBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Rough byte estimate for a record batch (sum of buffer sizes).
fn batch_byte_estimate(batch: &RecordBatch) -> usize {
    batch
        .columns()
        .iter()
        .map(|col| col.get_buffer_memory_size())
        .sum()
}

/// Single-tenant append buffer (kept for internal paths that do not use the
/// group-commit coordinator — e.g. system-only paths).
pub struct AppendBuffer {
    batches: Vec<RecordBatch>,
    total_rows: usize,
}

impl AppendBuffer {
    pub fn new() -> Self {
        Self {
            batches: Vec::new(),
            total_rows: 0,
        }
    }

    /// Rebuild a buffer from previously-drained batches, recomputing `total_rows`.
    /// Used to restore buffered writes when a flush fails.
    pub fn from_vec(batches: Vec<RecordBatch>) -> Self {
        let total_rows = batches.iter().map(RecordBatch::num_rows).sum();
        Self {
            batches,
            total_rows,
        }
    }

    pub fn push(&mut self, batch: RecordBatch) {
        self.total_rows += batch.num_rows();
        self.batches.push(batch);
    }

    pub fn drain(&mut self) -> Vec<RecordBatch> {
        self.total_rows = 0;
        std::mem::take(&mut self.batches)
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
}

impl Default for AppendBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::ids::DataTenantId;

    use super::TenantAppendBuffer;

    fn make_batch(rows: usize) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)]));
        let col = Arc::new(Int64Array::from(vec![1i64; rows]));
        RecordBatch::try_new(schema, vec![col]).expect("valid batch")
    }

    #[test]
    fn drain_round_robin_fair_per_tenant() {
        let t1 = DataTenantId::new_v7();
        let t2 = DataTenantId::new_v7();
        let t3 = DataTenantId::new_v7();

        let mut buf = TenantAppendBuffer::new();
        buf.push(t1, make_batch(5));
        buf.push(t1, make_batch(3));
        buf.push(t2, make_batch(7));
        buf.push(t3, make_batch(2));
        buf.push(t2, make_batch(4));

        assert_eq!(buf.total_rows(), 21);
        assert!(!buf.is_empty());

        let groups = buf.drain_by_tenant();

        // All three tenants present.
        assert_eq!(groups.len(), 3);
        assert!(buf.is_empty());
        assert_eq!(buf.total_rows(), 0);

        // t1 first (inserted first), then t2, then t3 — round-robin insertion order.
        assert_eq!(groups[0].0, t1);
        assert_eq!(groups[1].0, t2);
        assert_eq!(groups[2].0, t3);

        // FIFO within each tenant.
        assert_eq!(groups[0].1.len(), 2); // two batches for t1
        assert_eq!(groups[1].1.len(), 2); // two batches for t2
        assert_eq!(groups[2].1.len(), 1); // one batch for t3
    }

    #[test]
    fn restore_puts_batches_back() {
        let t1 = DataTenantId::new_v7();
        let mut buf = TenantAppendBuffer::new();
        buf.push(t1, make_batch(10));
        assert_eq!(buf.total_rows(), 10);

        let drained = buf.drain_by_tenant();
        assert!(buf.is_empty());

        buf.restore(drained);
        assert_eq!(buf.total_rows(), 10);
        assert!(!buf.is_empty());
    }

    #[test]
    fn empty_buffer_drains_to_empty_vec() {
        let mut buf = TenantAppendBuffer::new();
        let groups = buf.drain_by_tenant();
        assert!(groups.is_empty());
        assert!(buf.is_empty());
    }

    #[test]
    fn single_tenant_collapses_to_fifo() {
        let t = DataTenantId::new_v7();
        let mut buf = TenantAppendBuffer::new();
        buf.push(t, make_batch(1));
        buf.push(t, make_batch(2));
        buf.push(t, make_batch(3));

        let groups = buf.drain_by_tenant();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, t);
        assert_eq!(groups[0].1.len(), 3);
        assert_eq!(groups[0].1[0].num_rows(), 1);
        assert_eq!(groups[0].1[1].num_rows(), 2);
        assert_eq!(groups[0].1[2].num_rows(), 3);
    }
}
