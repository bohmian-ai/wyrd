//! In-memory row buffer keyed by seal-key with seal predicate.
//!
//! The memtable holds Arrow buffers + paired `AuditEvent` lists per
//! `SealKey = (DataTenantId, TableRef, EventDay)`. Seal predicate triggers
//! freeze at first-of: 50k rows | 1s wall-time | 128 MiB | 5s inactivity.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;
use crate::scribe::file_list_writer::FileListCommitKey;
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
    pub(crate) writable: Arc<Mutex<HashMap<SealKey, MemtableBucket>>>,
    pub(crate) immutable: Arc<Mutex<HashMap<SealKey, Vec<ImmutableEntry>>>>,
    next_seal_id: Arc<AtomicU64>,
    retention_grace: Duration,
}

/// Aggregate memory and generation state for a Scribe pod.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemtableStats {
    /// Rows in writable buckets.
    pub writable_rows: usize,
    /// Estimated bytes in writable buckets.
    pub writable_bytes: usize,
    /// Rows retained in immutable generations.
    pub immutable_rows: usize,
    /// Estimated bytes retained in immutable generations.
    pub immutable_bytes: usize,
    /// Total immutable generations.
    pub immutable_generations: usize,
    /// Immutable generations still awaiting post-commit completion.
    pub pending_generations: usize,
}

impl Memtable {
    /// Construct a new empty memtable.
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_retention(Duration::from_mins(1))
    }

    /// Construct a memtable with an explicit immutable-generation grace period.
    #[must_use]
    pub fn new_with_retention(retention_grace: Duration) -> Self {
        Self {
            writable: Arc::new(Mutex::new(HashMap::new())),
            immutable: Arc::new(Mutex::new(HashMap::new())),
            next_seal_id: Arc::new(AtomicU64::new(1)),
            retention_grace,
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
        let mut buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let bucket = buckets
            .entry(seal_key.clone())
            .or_insert_with(|| MemtableBucket::new(seal_key.clone(), batch.schema()));

        bucket.append(event, meta, batch);
        Ok(())
    }

    /// Restore one unretired WAL range as a pending immutable generation.
    ///
    /// Replay is intentionally conservative: every data payload must decode
    /// as Arrow IPC and line up one-for-one with its WAL metadata. The restored
    /// generation remains pending until the caller reconciles the exact
    /// `vala.file_list` commit key.
    pub fn restore_replayed(
        &self,
        replayed: &crate::scribe::replay::ReplayedSealKey,
    ) -> Result<FrozenMemtable, ScribeError> {
        if replayed.data_records.len() != replayed.append_metas.len() {
            return Err(ScribeError::Internal {
                detail: format!(
                    "replayed data/meta count mismatch: {} data records, {} metadata records",
                    replayed.data_records.len(),
                    replayed.append_metas.len()
                ),
            });
        }
        if replayed.audit_events.len() != replayed.data_records.len() {
            return Err(ScribeError::Internal {
                detail: format!(
                    "replayed audit/data count mismatch: {} audit events, {} data records",
                    replayed.audit_events.len(),
                    replayed.data_records.len()
                ),
            });
        }

        let mut batches = Vec::with_capacity(replayed.data_records.len());
        let mut metas = Vec::with_capacity(replayed.append_metas.len());
        for (payload, replayed_meta) in replayed.data_records.iter().zip(&replayed.append_metas) {
            let reader = arrow::ipc::reader::StreamReader::try_new(Cursor::new(payload), None)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("replayed Arrow IPC reader init failed: {error}"),
                })?;
            let decoded: Vec<RecordBatch> =
                reader
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("replayed Arrow IPC decode failed: {error}"),
                    })?;
            let schema =
                decoded
                    .first()
                    .map(RecordBatch::schema)
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "replayed Arrow IPC payload contained no batches".to_owned(),
                    })?;
            let batch = arrow::compute::concat_batches(&schema, &decoded).map_err(|error| {
                ScribeError::Internal {
                    detail: format!("replayed Arrow batch merge failed: {error}"),
                }
            })?;
            let rows_accepted = batch.num_rows();
            batches.push(batch);
            metas.push(ScribeAppendMeta {
                batch_id: replayed_meta.batch_id,
                rows_accepted,
                wal_lsn_min: replayed_meta.wal_lsn,
                wal_lsn_max: replayed_meta.wal_lsn,
                seal_key: replayed.seal_key.as_path_components(),
            });
        }

        let schema =
            batches
                .first()
                .map(RecordBatch::schema)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "replayed seal range contained no data".to_owned(),
                })?;
        let batch = arrow::compute::concat_batches(&schema, &batches).map_err(|error| {
            ScribeError::Internal {
                detail: format!("replayed seal range merge failed: {error}"),
            }
        })?;
        let seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        let frozen = FrozenMemtable {
            seal_id,
            seal_key: replayed.seal_key.clone(),
            schema,
            batch,
            batches,
            events: replayed.audit_events.clone(),
            metas,
        };
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        immutable
            .entry(replayed.seal_key.clone())
            .or_default()
            .push(ImmutableEntry::pending(frozen.clone()));
        Ok(frozen)
    }

    /// Check seal predicate for a specific seal-key.
    ///
    /// Returns `true` if the seal-key should be frozen.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn should_seal(&self, seal_key: &SealKey) -> Result<bool, ScribeError> {
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
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
    /// Detaches the bucket's state into a pending immutable generation.
    ///
    /// A pending generation remains readable and is returned again when a
    /// caller retries the same seal before completing its transaction. This
    /// keeps encode/PUT/SQL failures retryable without losing the WAL-backed
    /// in-memory snapshot.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned or if
    /// the seal-key does not exist.
    pub fn freeze(&self, seal_key: &SealKey) -> Result<FrozenMemtable, ScribeError> {
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        if let Some(entry) = immutable
            .get(seal_key)
            .and_then(|entries| entries.iter().find(|entry| entry.is_pending()))
        {
            return Ok(entry.frozen.clone());
        }
        drop(immutable);

        let mut buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let bucket = buckets
            .remove(seal_key)
            .ok_or_else(|| ScribeError::Internal {
                detail: format!("seal-key not found: {seal_key}"),
            })?;

        let seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        let frozen = bucket.freeze(seal_id)?;

        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        immutable
            .entry(seal_key.clone())
            .or_default()
            .push(ImmutableEntry::pending(frozen.clone()));
        Ok(frozen)
    }

    /// Get the current row count for a seal-key.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn row_count(&self, seal_key: &SealKey) -> Result<usize, ScribeError> {
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
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
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets
            .keys()
            .filter(|k| k.tenant == tenant)
            .cloned()
            .collect())
    }

    /// Snapshot writable and pending immutable seal keys for one tenant.
    pub fn seal_keys_for_tenant(&self, tenant: DataTenantId) -> Result<Vec<SealKey>, ScribeError> {
        let writable = self.active_seal_keys_for_tenant(tenant)?;
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut keys = writable;
        keys.extend(
            immutable
                .iter()
                .filter(|(key, entries)| {
                    key.tenant == tenant && entries.iter().any(ImmutableEntry::is_pending)
                })
                .map(|(key, _)| key.clone()),
        );
        keys.sort_by_key(ToString::to_string);
        keys.dedup();
        Ok(keys)
    }

    /// Return writable and immutable append batches for one tenant/table.
    ///
    /// The returned snapshots are detached from the locks. Callers must still
    /// verify the exact seal key before projecting a batch to an external
    /// format.
    pub fn readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let writable = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut batches = Vec::new();

        for (seal_key, bucket) in writable.iter() {
            if seal_key.tenant == tenant && seal_key.table == *table {
                batches.extend(bucket.readable_batches());
            }
        }
        for (seal_key, entries) in immutable.iter() {
            if seal_key.tenant == tenant && seal_key.table == *table {
                for entry in entries {
                    batches.extend(entry.frozen.readable_batches());
                }
            }
        }
        Ok(batches)
    }

    /// Mark a prepared generation committed after the owning SQL transaction commits.
    /// Repeating the operation with the same key is idempotent.
    pub fn complete_post_commit(
        &self,
        seal_id: u64,
        file_list_key: FileListCommitKey,
    ) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        for entries in immutable.values_mut() {
            if let Some(entry) = entries.iter_mut().find(|entry| entry.seal_id() == seal_id) {
                match &entry.state {
                    ImmutableState::PendingCommit => {
                        entry.state = ImmutableState::Committed {
                            observed_at: Instant::now(),
                            file_list_key,
                        };
                    }
                    ImmutableState::Committed { .. } => {}
                }
                return Ok(());
            }
        }
        Err(ScribeError::Internal {
            detail: format!("post-commit token references unknown seal generation {seal_id}"),
        })
    }

    /// Keep a prepared generation pending after a transaction rollback.
    /// Repeating the operation is intentionally idempotent.
    pub fn abort_post_commit(&self, seal_id: u64) -> Result<(), ScribeError> {
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        if immutable
            .values()
            .flatten()
            .any(|entry| entry.seal_id() == seal_id)
        {
            return Ok(());
        }
        Err(ScribeError::Internal {
            detail: format!("abort token references unknown seal generation {seal_id}"),
        })
    }

    /// Retire only committed immutable generations whose grace period elapsed.
    /// The returned ranges are the only ranges eligible for WAL retirement.
    pub fn sweep_once_at(&self, now: Instant) -> Result<Vec<(SealKey, WalRange)>, ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut retired = Vec::new();
        for (seal_key, entries) in immutable.iter_mut() {
            let mut kept = Vec::with_capacity(entries.len());
            for entry in entries.drain(..) {
                let retire = matches!(
                    entry.state,
                    ImmutableState::Committed { observed_at, .. }
                        if now.saturating_duration_since(observed_at) >= self.retention_grace
                );
                if retire {
                    retired.push((seal_key.clone(), entry.wal_range()));
                } else {
                    kept.push(entry);
                }
            }
            *entries = kept;
        }
        immutable.retain(|_, entries| !entries.is_empty());
        Ok(retired)
    }

    /// Number of immutable generations currently retained.
    pub fn immutable_generation_count(&self) -> Result<usize, ScribeError> {
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok(immutable.values().map(Vec::len).sum())
    }

    /// Number of immutable generations still pending post-commit.
    pub fn pending_generation_count(&self) -> Result<usize, ScribeError> {
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok(immutable
            .values()
            .flatten()
            .filter(|entry| entry.is_pending())
            .count())
    }

    /// Return aggregate writable and immutable state without exposing buckets.
    pub fn stats(&self) -> Result<MemtableStats, ScribeError> {
        let writable = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable writable lock poisoned: {e}"),
        })?;
        let writable_rows = writable.values().map(|bucket| bucket.row_count).sum();
        let writable_bytes = writable
            .values()
            .map(|bucket| bucket.bytes_accumulated)
            .sum();
        drop(writable);

        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let immutable_rows = immutable
            .values()
            .flatten()
            .map(|entry| entry.frozen.batch.num_rows())
            .sum();
        let immutable_bytes = immutable
            .values()
            .flatten()
            .map(|entry| entry.frozen.batch.get_array_memory_size())
            .sum();
        let immutable_generations = immutable.values().map(Vec::len).sum();
        let pending_generations = immutable
            .values()
            .flatten()
            .filter(|entry| entry.is_pending())
            .count();

        Ok(MemtableStats {
            writable_rows,
            writable_bytes,
            immutable_rows,
            immutable_bytes,
            immutable_generations,
            pending_generations,
        })
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

    fn freeze(self, seal_id: u64) -> Result<FrozenMemtable, ScribeError> {
        use arrow::compute::concat_batches;

        let batches = self.batches;

        // Merge all batches into one
        let batch = if batches.is_empty() {
            // Empty bucket — return an empty RecordBatch with the schema
            RecordBatch::new_empty(self.schema.clone())
        } else if batches.len() == 1 {
            // Single batch — no merge needed
            batches[0].clone()
        } else {
            // Multiple batches — merge them
            concat_batches(&self.schema, &batches).map_err(|error| ScribeError::Internal {
                detail: format!("memtable batch schema mismatch: {error}"),
            })?
        };

        Ok(FrozenMemtable {
            seal_id,
            seal_key: self.seal_key,
            schema: self.schema,
            batch,
            batches,
            events: self.events,
            metas: self.metas,
        })
    }

    fn readable_batches(&self) -> Vec<ReadableBatch> {
        self.batches
            .iter()
            .cloned()
            .zip(self.metas.iter().cloned())
            .map(|(batch, meta)| ReadableBatch { meta, batch })
            .collect()
    }
}

/// The state of one immutable generation.
#[derive(Debug)]
pub enum ImmutableState {
    /// The generation is readable but its SQL transaction is not durable yet.
    PendingCommit,
    /// The SQL row is durable and the generation is retained for the grace period.
    Committed {
        /// Local observation time used by deterministic sweeping.
        observed_at: Instant,
        /// Exact durable file-list identity used for replay reconciliation.
        file_list_key: FileListCommitKey,
    },
}

/// One detached immutable generation.
#[derive(Debug)]
pub struct ImmutableEntry {
    /// Monotonic local identity for post-commit tokens.
    pub seal_id: u64,
    /// Frozen Arrow snapshot.
    pub frozen: FrozenMemtable,
    /// Inclusive minimum WAL LSN in the generation.
    pub wal_lsn_min: crate::scribe::wal::WalLsn,
    /// Inclusive maximum WAL LSN in the generation.
    pub wal_lsn_max: crate::scribe::wal::WalLsn,
    /// Lifecycle state.
    pub state: ImmutableState,
}

impl ImmutableEntry {
    fn pending(frozen: FrozenMemtable) -> Self {
        Self {
            seal_id: frozen.seal_id,
            wal_lsn_min: frozen
                .metas
                .iter()
                .map(|meta| meta.wal_lsn_min)
                .min()
                .unwrap_or_else(|| crate::scribe::wal::WalLsn::new(0)),
            wal_lsn_max: frozen
                .metas
                .iter()
                .map(|meta| meta.wal_lsn_max)
                .max()
                .unwrap_or_else(|| crate::scribe::wal::WalLsn::new(0)),
            frozen,
            state: ImmutableState::PendingCommit,
        }
    }

    fn is_pending(&self) -> bool {
        matches!(self.state, ImmutableState::PendingCommit)
    }

    fn seal_id(&self) -> u64 {
        self.seal_id
    }

    fn wal_range(&self) -> WalRange {
        WalRange {
            min: self.wal_lsn_min,
            max: self.wal_lsn_max,
        }
    }
}

/// An append batch that can be projected into a tail frame.
#[derive(Debug, Clone)]
pub struct ReadableBatch {
    /// WAL metadata for the append.
    pub meta: ScribeAppendMeta,
    /// Arrow rows for the append.
    pub batch: RecordBatch,
}

/// Inclusive WAL range eligible for retirement after a committed sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalRange {
    /// Inclusive lower LSN.
    pub min: crate::scribe::wal::WalLsn,
    /// Inclusive upper LSN.
    pub max: crate::scribe::wal::WalLsn,
}

/// Frozen memtable snapshot for one seal-key.
///
/// Immutable snapshot detached from the writable bucket. Carries one merged Arrow
/// batch, paired `AuditEvent` list, and `ScribeAppendMeta` list.
#[derive(Debug, Clone)]
pub struct FrozenMemtable {
    /// Local immutable-generation identity.
    pub seal_id: u64,
    /// The seal-key this snapshot belongs to.
    pub seal_key: SealKey,
    /// Arrow schema for the batch.
    pub schema: SchemaRef,
    /// Merged Arrow batch (all appends concatenated).
    pub batch: RecordBatch,
    /// Original append batches, preserved for LSN-granular live tail reads.
    pub batches: Vec<RecordBatch>,
    /// Ordered list of `AuditEvent`s staged for the seal transaction.
    pub events: Vec<AuditEvent>,
    /// Per-append metadata derived from WAL record headers.
    pub metas: Vec<ScribeAppendMeta>,
}

impl FrozenMemtable {
    fn readable_batches(&self) -> Vec<ReadableBatch> {
        self.batches
            .iter()
            .cloned()
            .zip(self.metas.iter().cloned())
            .map(|(batch, meta)| ReadableBatch { meta, batch })
            .collect()
    }
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

    fn make_file_list_key(min: u64, max: u64) -> FileListCommitKey {
        FileListCommitKey {
            data_tenant_id: DataTenantId::SYSTEM_OWNER,
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            node_id: uuid::Uuid::nil(),
            writer_epoch: 1,
            wal_lsn_min: i64::try_from(min).expect("test lsn"),
            wal_lsn_max: i64::try_from(max).expect("test lsn"),
        }
    }

    #[test]
    fn test_freeze_promotes_to_immutable_not_drop() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(10),
                make_test_batch(2),
            )
            .expect("insert");

        let frozen = memtable.freeze(&seal_key).expect("freeze");

        assert_eq!(frozen.batch.num_rows(), 2);
        assert_eq!(memtable.row_count(&seal_key).expect("row count"), 0);
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 1);
        assert_eq!(memtable.pending_generation_count().expect("pending"), 1);
        assert_eq!(
            memtable
                .readable_batches(DataTenantId::SYSTEM_OWNER, &seal_key.table)
                .expect("readable")
                .len(),
            1
        );
    }

    #[test]
    fn test_multiple_frozen_generations_same_seal_key() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(10),
                make_test_batch(1),
            )
            .expect("first insert");
        let first = memtable.freeze(&seal_key).expect("first freeze");
        memtable
            .complete_post_commit(first.seal_id, make_file_list_key(10, 10))
            .expect("first complete");

        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(20),
                make_test_batch(1),
            )
            .expect("second insert");
        let second = memtable.freeze(&seal_key).expect("second freeze");

        assert_ne!(first.seal_id, second.seal_id);
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 2);
        let lsns: Vec<_> = memtable
            .readable_batches(DataTenantId::SYSTEM_OWNER, &seal_key.table)
            .expect("readable")
            .into_iter()
            .map(|batch| batch.meta.wal_lsn_max)
            .collect();
        assert_eq!(lsns, vec![WalLsn::new(10), WalLsn::new(20)]);
    }

    #[test]
    fn committed_generations_retire_only_after_grace() {
        let memtable = Memtable::new_with_retention(Duration::from_secs(1));
        let seal_key = make_test_seal_key();
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(10),
                make_test_batch(1),
            )
            .expect("insert");
        let frozen = memtable.freeze(&seal_key).expect("freeze");
        memtable
            .complete_post_commit(frozen.seal_id, make_file_list_key(10, 10))
            .expect("complete");

        let now = Instant::now();
        assert!(memtable.sweep_once_at(now).expect("sweep").is_empty());
        assert_eq!(
            memtable
                .sweep_once_at(now + Duration::from_secs(2))
                .expect("elapsed sweep")
                .len(),
            1
        );
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 0);
    }

    #[test]
    fn pending_generations_never_retire() {
        let memtable = Memtable::new_with_retention(Duration::from_secs(1));
        let seal_key = make_test_seal_key();
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(10),
                make_test_batch(1),
            )
            .expect("insert");
        memtable.freeze(&seal_key).expect("freeze");
        assert!(
            memtable
                .sweep_once_at(Instant::now() + Duration::from_secs(10))
                .expect("sweep")
                .is_empty()
        );
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 1);
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
