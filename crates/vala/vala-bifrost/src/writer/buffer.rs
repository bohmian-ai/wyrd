use std::collections::HashMap;
use std::collections::VecDeque;

use arrow::array::RecordBatch;
use wyrd_spec::ids::DataTenantId;

/// Fair multi-tenant append buffer for the group-commit coordinator.
///
/// Holds one whole write unit `T` per buffered write (the coordinator uses a
/// unit that carries the batch, its `batch_id`/ctx, and the caller's reply
/// channel — so a write's idempotency key is never decoupled from its batch).
///
/// [`drain_round_robin`](Self::drain_round_robin) returns the buffered units in
/// interleaved round-robin order: round *k* takes the *k*-th queued unit of each
/// tenant, in first-seen order. A high-volume tenant therefore cannot push all
/// its writes ahead of a quiet tenant's single write, so chunked group commits
/// stay fair (Q4).
pub struct TenantAppendBuffer<T> {
    /// Per-tenant FIFO queues.
    queues: HashMap<DataTenantId, VecDeque<T>>,
    /// Stable round-robin order: a tenant is appended when first seen this window.
    order: Vec<DataTenantId>,
    /// Buffered unit count across all tenants.
    count: usize,
    /// Total buffered row count across all tenants.
    total_rows: usize,
    /// Total buffered byte estimate (sum of batch memory sizes).
    total_bytes: usize,
}

impl<T> TenantAppendBuffer<T> {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            order: Vec::new(),
            count: 0,
            total_rows: 0,
            total_bytes: 0,
        }
    }

    /// Buffer one write unit for `tenant`, with its precomputed `rows`/`bytes`.
    pub fn push(&mut self, tenant: DataTenantId, item: T, rows: usize, bytes: usize) {
        self.count += 1;
        self.total_rows += rows;
        self.total_bytes += bytes;
        let queue = self.queues.entry(tenant).or_default();
        if queue.is_empty() {
            // First unit for this tenant in this window — add to the rotation.
            self.order.retain(|t| *t != tenant);
            self.order.push(tenant);
        }
        queue.push_back(item);
    }

    /// Drain every buffered unit in interleaved round-robin order (see the type
    /// docs). Each element is `(tenant, unit)`; the buffer is left empty.
    pub fn drain_round_robin(&mut self) -> Vec<(DataTenantId, T)> {
        let order = std::mem::take(&mut self.order);
        let mut queues = std::mem::take(&mut self.queues);
        let mut out = Vec::with_capacity(self.count);
        self.count = 0;
        self.total_rows = 0;
        self.total_bytes = 0;

        loop {
            let mut progressed = false;
            for tenant in &order {
                if let Some(item) = queues.get_mut(tenant).and_then(VecDeque::pop_front) {
                    out.push((*tenant, item));
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl<T> Default for TenantAppendBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Rough byte estimate for a record batch (sum of buffer sizes).
pub(crate) fn batch_byte_estimate(batch: &RecordBatch) -> usize {
    batch
        .columns()
        .iter()
        .map(|col| col.get_buffer_memory_size())
        .sum()
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

    /// Interleaved fairness: a quiet tenant's single unit lands in round one, not
    /// behind a busy tenant's backlog. Drain order is round-robin across tenants,
    /// FIFO within a tenant.
    #[test]
    fn round_robin_fair_drain() {
        let t1 = DataTenantId::new_v7();
        let t2 = DataTenantId::new_v7();
        let t3 = DataTenantId::new_v7();

        // t1 pushes three, t2 two, t3 one — t3 must not be starved to the tail.
        let mut buf: TenantAppendBuffer<&'static str> = TenantAppendBuffer::new();
        buf.push(t1, "a1", 5, 5);
        buf.push(t1, "a2", 3, 3);
        buf.push(t2, "b1", 7, 7);
        buf.push(t1, "a3", 1, 1);
        buf.push(t3, "c1", 2, 2);
        buf.push(t2, "b2", 4, 4);

        assert_eq!(buf.total_rows(), 22);
        assert!(!buf.is_empty());

        let drained = buf.drain_round_robin();
        assert!(buf.is_empty());
        assert_eq!(buf.total_rows(), 0);

        let order: Vec<(DataTenantId, &str)> = drained;
        // Round 1: t1,t2,t3 (first-seen order). Round 2: t1,t2. Round 3: t1.
        assert_eq!(
            order,
            vec![
                (t1, "a1"),
                (t2, "b1"),
                (t3, "c1"),
                (t1, "a2"),
                (t2, "b2"),
                (t1, "a3"),
            ]
        );
        // t3's single unit is at index 2 (round one), never starved to the tail.
        assert_eq!(order[2], (t3, "c1"));
    }

    #[test]
    fn empty_buffer_drains_to_empty_vec() {
        let mut buf: TenantAppendBuffer<RecordBatch> = TenantAppendBuffer::new();
        let drained = buf.drain_round_robin();
        assert!(drained.is_empty());
        assert!(buf.is_empty());
    }

    #[test]
    fn single_tenant_preserves_fifo() {
        let t = DataTenantId::new_v7();
        let mut buf: TenantAppendBuffer<RecordBatch> = TenantAppendBuffer::new();
        buf.push(t, make_batch(1), 1, 1);
        buf.push(t, make_batch(2), 2, 2);
        buf.push(t, make_batch(3), 3, 3);

        let drained = buf.drain_round_robin();
        assert_eq!(drained.len(), 3);
        let rows: Vec<usize> = drained.iter().map(|(_, b)| b.num_rows()).collect();
        assert_eq!(rows, vec![1, 2, 3]);
    }
}
