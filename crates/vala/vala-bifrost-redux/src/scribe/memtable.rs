//! In-memory row buffer keyed by seal-key with seal predicate.
//!
//! The memtable holds Arrow buffers + paired `AuditEvent` lists per
//! `SealKey = (DataTenantId, TableRef, EventDay)`. Seal predicate triggers
//! freeze at first-of: 50k rows | 1s wall-time | 128 MiB | 5s inactivity.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::ScribeAppendMeta;

/// Seal predicate thresholds (hardcoded constants for ).
const SEAL_ROWS_THRESHOLD: usize = 50_000;
const SEAL_BYTES_THRESHOLD: usize = 128 * 1024 * 1024; // 128 MiB
const SEAL_INTERVAL_SECS: u64 = 1;
const SEAL_INACTIVITY_SECS: u64 = 5;

/// Memtable — in-memory row buffer keyed by seal-key.
///
/// Each seal-key holds an Arrow buffer, paired `AuditEvent` list, and
/// `ScribeAppendMeta` list. Freezing a seal-key detaches an immutable snapshot.
#[derive(Debug)]
pub struct Memtable {
    pub(crate) buckets: Arc<Mutex<HashMap<SealKey, MemtableBucket>>>,
}

impl Memtable {
    /// Construct a new empty memtable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Insert an append (audit event, metadata, Arrow batch) into the memtable.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned or if
    /// Arrow batch merging fails.
    pub fn insert(
        &self,
        seal_key: &SealKey,
        event: AuditEvent,
        meta: ScribeAppendMeta,
        batch: RecordBatch,
    ) -> Result<(), ScribeError> {
        let mut buckets = self.buckets.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let bucket = buckets
            .entry(seal_key.clone())
            .or_insert_with(|| MemtableBucket::new(seal_key.clone(), batch.schema()));

        bucket.append(event, meta, batch);
        Ok(())
    }

    /// Check seal predicate for a specific seal-key.
    ///
    /// Returns `true` if the seal-key should be frozen.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn should_seal(&self, seal_key: &SealKey) -> Result<bool, ScribeError> {
        let buckets = self.buckets.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        if let Some(bucket) = buckets.get(seal_key) {
            Ok(bucket.should_seal())
        } else {
            Ok(false)
        }
    }

    /// Freeze a seal-key and return the immutable snapshot.
    ///
    /// Detaches the bucket's state and resets the writable bucket for this key.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned or if
    /// the seal-key does not exist.
    pub fn freeze(&self, seal_key: &SealKey) -> Result<FrozenMemtable, ScribeError> {
        let mut buckets = self.buckets.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let bucket = buckets
            .remove(seal_key)
            .ok_or_else(|| ScribeError::Internal {
                detail: format!("seal-key not found: {seal_key}"),
            })?;

        Ok(bucket.freeze())
    }

    /// Get the current row count for a seal-key.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn row_count(&self, seal_key: &SealKey) -> Result<usize, ScribeError> {
        let buckets = self.buckets.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        Ok(buckets.get(seal_key).map_or(0, |b| b.row_count))
    }

    /// Snapshot every active seal-key currently held by the memtable whose
    /// tenant equals `tenant`. Used by `ScribeImpl::force_seal` to drive a
    /// per-tenant seal loop without exposing the private `MemtableBucket` type.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn active_seal_keys_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<SealKey>, ScribeError> {
        let buckets = self.buckets.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets
            .keys()
            .filter(|k| k.tenant == tenant)
            .cloned()
            .collect())
    }
}

impl Default for Memtable {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-seal-key bucket holding Arrow buffers + audit events + metadata.
#[derive(Debug)]
pub(crate) struct MemtableBucket {
    seal_key: SealKey,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    events: Vec<AuditEvent>,
    metas: Vec<ScribeAppendMeta>,
    row_count: usize,
    bytes_accumulated: usize,
    first_insert_at: Instant,
    last_insert_at: Instant,
}

impl MemtableBucket {
    fn new(seal_key: SealKey, schema: SchemaRef) -> Self {
        let now = Instant::now();
        Self {
            seal_key,
            schema,
            batches: Vec::new(),
            events: Vec::new(),
            metas: Vec::new(),
            row_count: 0,
            bytes_accumulated: 0,
            first_insert_at: now,
            last_insert_at: now,
        }
    }

    fn append(&mut self, event: AuditEvent, meta: ScribeAppendMeta, batch: RecordBatch) {
        self.row_count += batch.num_rows();
        self.bytes_accumulated += estimate_batch_bytes(&batch);
        self.batches.push(batch);
        self.events.push(event);
        self.metas.push(meta);
        self.last_insert_at = Instant::now();
    }

    fn should_seal(&self) -> bool {
        let now = Instant::now();
        let elapsed_since_first = now.duration_since(self.first_insert_at).as_secs();
        let elapsed_since_last = now.duration_since(self.last_insert_at).as_secs();

        self.row_count >= SEAL_ROWS_THRESHOLD
            || self.bytes_accumulated >= SEAL_BYTES_THRESHOLD
            || elapsed_since_first >= SEAL_INTERVAL_SECS
            || elapsed_since_last >= SEAL_INACTIVITY_SECS
    }

    fn freeze(self) -> FrozenMemtable {
        use arrow::compute::concat_batches;

        // Merge all batches into one
        let batch = if self.batches.is_empty() {
            // Empty bucket — return an empty RecordBatch with the schema
            RecordBatch::new_empty(self.schema.clone())
        } else if self.batches.len() == 1 {
            // Single batch — no merge needed
            self.batches.into_iter().next().expect("single batch")
        } else {
            // Multiple batches — merge them
            concat_batches(&self.schema, &self.batches)
                .expect("schema mismatch is an invariant violation")
        };

        FrozenMemtable {
            seal_key: self.seal_key,
            schema: self.schema,
            batch,
            events: self.events,
            metas: self.metas,
        }
    }
}

/// Frozen memtable snapshot for one seal-key.
///
/// Immutable snapshot detached from the writable bucket. Carries one merged Arrow
/// batch, paired `AuditEvent` list, and `ScribeAppendMeta` list.
#[derive(Debug, Clone)]
pub struct FrozenMemtable {
    /// The seal-key this snapshot belongs to.
    pub seal_key: SealKey,
    /// Arrow schema for the batch.
    pub schema: SchemaRef,
    /// Merged Arrow batch (all appends concatenated).
    pub batch: RecordBatch,
    /// Ordered list of `AuditEvent`s staged for the seal transaction.
    pub events: Vec<AuditEvent>,
    /// Per-append metadata derived from WAL record headers.
    pub metas: Vec<ScribeAppendMeta>,
}

/// Estimate batch size in bytes (Arrow column sizes + overhead).
fn estimate_batch_bytes(batch: &RecordBatch) -> usize {
    batch
        .columns()
        .iter()
        .map(|col| col.get_array_memory_size())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::EventDay;
    use crate::scribe::wal::WalLsn;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use chrono::NaiveDate;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use wyrd_spec::ids::DataTenantId;

    fn make_test_batch(num_rows: usize) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let array = Int64Array::from(vec![1i64; num_rows]);
        RecordBatch::try_new(schema, vec![Arc::new(array)]).expect("batch")
    }

    fn make_test_event() -> AuditEvent {
        AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_string(),
            resource: "test.table".to_string(),
            card_ref: None,
            principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "test".to_string(),
            detail: None,
        }
    }

    fn make_test_meta(lsn: u64) -> ScribeAppendMeta {
        ScribeAppendMeta {
            batch_id: [0u8; 16],
            rows_accepted: 100,
            wal_lsn_min: WalLsn::new(lsn),
            wal_lsn_max: WalLsn::new(lsn),
            seal_key: "test".to_string(),
        }
    }

    fn make_test_seal_key() -> SealKey {
        SealKey::new(
            DataTenantId::SYSTEM_OWNER,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("valid date")),
        )
    }

    #[test]
    fn memtable_seal_predicate_rows() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 60k rows in 6 batches of 10k each
        for i in 0..6 {
            let batch = make_test_batch(10_000);
            memtable
                .insert(&seal_key, make_test_event(), make_test_meta(i), batch)
                .expect("insert");
        }

        assert!(memtable.should_seal(&seal_key).expect("should_seal"));
        assert_eq!(memtable.row_count(&seal_key).expect("row_count"), 60_000);
    }

    #[test]
    fn memtable_seal_predicate_interval() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 1 row, then wait >1s (30% margin for CI stability)
        let batch = make_test_batch(1);
        memtable
            .insert(&seal_key, make_test_event(), make_test_meta(0), batch)
            .expect("insert");

        thread::sleep(Duration::from_millis(1300));

        assert!(
            memtable.should_seal(&seal_key).expect("should_seal"),
            "1s interval triggers seal"
        );
    }

    #[test]
    fn memtable_seal_predicate_bytes() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert batches totaling >128 MiB
        // Each Int64 array with 1M rows ≈ 8MB
        for i in 0..20 {
            let batch = make_test_batch(1_000_000);
            memtable
                .insert(&seal_key, make_test_event(), make_test_meta(i), batch)
                .expect("insert");
        }

        assert!(
            memtable.should_seal(&seal_key).expect("should_seal"),
            "128 MiB threshold triggers seal"
        );
    }

    #[test]
    fn memtable_seal_predicate_inactivity() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 1 row, then wait >5s (40% margin for CI stability)
        let batch = make_test_batch(1);
        memtable
            .insert(&seal_key, make_test_event(), make_test_meta(0), batch)
            .expect("insert");

        thread::sleep(Duration::from_secs(7));

        assert!(
            memtable.should_seal(&seal_key).expect("should_seal"),
            "5s inactivity triggers seal"
        );
    }

    #[test]
    fn memtable_freeze_produces_immutable_snapshot_with_envelopes() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 3 appends
        for i in 0..3 {
            let batch = make_test_batch(100);
            let event = make_test_event();
            memtable
                .insert(&seal_key, event.clone(), make_test_meta(i), batch)
                .expect("insert");
        }

        // Freeze
        let frozen = memtable.freeze(&seal_key).expect("freeze");

        assert_eq!(frozen.batch.num_rows(), 300, "3 batches × 100 rows merged");
        assert_eq!(frozen.events.len(), 3);
        assert_eq!(frozen.metas.len(), 3);
        assert_eq!(frozen.seal_key, seal_key);

        // Insert after freeze should go to a new bucket
        let batch = make_test_batch(50);
        memtable
            .insert(&seal_key, make_test_event(), make_test_meta(10), batch)
            .expect("insert after freeze");

        assert_eq!(
            memtable.row_count(&seal_key).expect("row_count"),
            50,
            "new bucket has only the post-freeze append"
        );
    }

    #[test]
    fn memtable_per_seal_key_isolation() {
        let memtable = Memtable::new();
        let tenant = DataTenantId::SYSTEM_OWNER;
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");

        let key1 = SealKey::new(
            tenant,
            table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("valid")),
        );
        let key2 = SealKey::new(
            tenant,
            table,
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid")),
        );

        // Insert to key1
        for i in 0..3 {
            memtable
                .insert(
                    &key1,
                    make_test_event(),
                    make_test_meta(i),
                    make_test_batch(100),
                )
                .expect("insert key1");
        }

        // Insert to key2
        for i in 0..2 {
            memtable
                .insert(
                    &key2,
                    make_test_event(),
                    make_test_meta(i),
                    make_test_batch(50),
                )
                .expect("insert key2");
        }

        assert_eq!(memtable.row_count(&key1).expect("key1 count"), 300);
        assert_eq!(memtable.row_count(&key2).expect("key2 count"), 100);

        // Freeze key1
        let frozen1 = memtable.freeze(&key1).expect("freeze key1");
        assert_eq!(frozen1.batch.num_rows(), 300, "3 batches × 100 rows merged");

        // key2 untouched
        assert_eq!(
            memtable.row_count(&key2).expect("key2 count after freeze"),
            100
        );
    }
}
