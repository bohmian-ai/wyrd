//! In-memory row buffer keyed by seal-key with seal predicate.
//!
//! The memtable holds Arrow buffers + paired `AuditEvent` lists per
//! `SealKey = (DataTenantId, TableRef, EventDay)`. Rotation is driven by
//! retained Arrow size, active-generation age, and global pressure. WAL
//! segment rollover is deliberately independent from bucket rotation.

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
use crate::scribe::routing::shard_for;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::ScribeAppendMeta;

/// Estimated Arrow bytes at which the active generation rotates.
pub const MEMTABLE_ROTATION_BYTES: usize = 512 * 1024 * 1024;
/// Maximum age of an active generation before the lifecycle scanner requests rotation.
pub const ACTIVE_GENERATION_MAX_AGE: Duration = Duration::from_mins(10);

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
    rotation_bytes: usize,
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
    /// Writable seal-key buckets currently present.
    pub writable_buckets: usize,
    /// Seal-key buckets with immutable generations currently present.
    pub immutable_buckets: usize,
}

/// Memory owned by one exact seal-key bucket, used by test-tier inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketMemorySnapshot {
    /// Exact tenant/table/day identity of the bucket.
    pub seal_key: SealKey,
    /// Rows currently retained for this seal-key.
    pub row_count: usize,
    /// Bytes in the writable bucket.
    pub writable_bytes: usize,
    /// Bytes in immutable generations for the bucket.
    pub immutable_bytes: usize,
}

/// Scanner-only pressure metadata for one writable seal-key bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PressureCandidate {
    /// Exact tenant/table/day identity of the bucket.
    pub seal_key: SealKey,
    /// Arrow bytes eligible for a pressure rotation.
    pub writable_bytes: usize,
    /// Stable age tie-breaker.
    pub first_insert_at: Instant,
    /// Oldest WAL LSN currently retained by the bucket.
    pub oldest_wal_lsn: Option<crate::scribe::wal::WalLsn>,
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
        Self::new_with_limits(retention_grace, MEMTABLE_ROTATION_BYTES)
    }

    /// Construct a memtable with explicit retention and active-bucket limits.
    #[must_use]
    pub fn new_with_limits(retention_grace: Duration, rotation_bytes: usize) -> Self {
        Self {
            writable: Arc::new(Mutex::new(HashMap::new())),
            immutable: Arc::new(Mutex::new(HashMap::new())),
            next_seal_id: Arc::new(AtomicU64::new(1)),
            retention_grace,
            rotation_bytes,
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

    /// Return the row count for a batch that is still retained in the active
    /// or immutable grace state for this exact seal key.
    ///
    /// The lookup is the runtime idempotency index. It is intentionally
    /// derived from the bounded buckets instead of maintaining a process-wide
    /// append-ID set whose memory would grow with the lifetime of a shard.
    pub fn retained_batch_rows(
        &self,
        seal_key: &SealKey,
        batch_id: [u8; 16],
    ) -> Result<Option<u64>, ScribeError> {
        let writable = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        if let Some(bucket) = writable.get(seal_key)
            && let Some(meta) = bucket.metas.iter().find(|meta| meta.batch_id == batch_id)
        {
            return Ok(Some(u64::try_from(meta.rows_accepted).unwrap_or(u64::MAX)));
        }
        drop(writable);

        let immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        Ok(immutable.get(seal_key).and_then(|entries| {
            entries.iter().find_map(|entry| {
                entry
                    .frozen
                    .metas
                    .iter()
                    .find(|meta| meta.batch_id == batch_id)
                    .map(|meta| u64::try_from(meta.rows_accepted).unwrap_or(u64::MAX))
            })
        }))
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
        let mut frozen = Self::decode_replayed(replayed)?;
        frozen.seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        self.insert_replayed_frozen(frozen)
    }

    /// Decode one replayed WAL state without mutating owner state.
    pub(crate) fn decode_replayed(
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
            crate::schema::managed_columns::row_ordinals(&batch).map_err(|error| {
                ScribeError::Internal {
                    detail: format!("replayed row identity invariant failed: {error}"),
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
        let arrow_bytes = batches.iter().map(RecordBatch::get_array_memory_size).sum();
        Ok(FrozenMemtable {
            seal_id: 0,
            seal_key: replayed.seal_key.clone(),
            schema,
            batches,
            events: replayed.audit_events.clone(),
            metas,
            opened_at: Instant::now(),
            closed_at: Instant::now(),
            arrow_bytes,
        })
    }

    /// Insert a decoded replay generation into this owner-local memtable.
    pub(crate) fn insert_replayed_frozen(
        &self,
        mut frozen: FrozenMemtable,
    ) -> Result<FrozenMemtable, ScribeError> {
        if frozen.seal_id == 0 {
            frozen.seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        }
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        immutable
            .entry(frozen.seal_key.clone())
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
            Ok(bucket.should_seal_at(Instant::now(), self.rotation_bytes))
        } else {
            Ok(false)
        }
    }

    /// Reports whether an append would push a non-empty writable bucket past rotation.
    ///
    /// Empty and absent buckets return `false`, allowing one oversized append
    /// to land before the existing post-insert rotation seals it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] if the writable-bucket lock is poisoned.
    pub fn would_cross_rotation(
        &self,
        seal_key: &SealKey,
        incoming: &RecordBatch,
    ) -> Result<bool, ScribeError> {
        let buckets = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable bucket lock poisoned: {error}"),
            })?;
        Ok(buckets.get(seal_key).is_some_and(|bucket| {
            bucket.bytes_accumulated > 0
                && bucket
                    .bytes_accumulated
                    .saturating_add(estimate_batch_bytes(incoming))
                    > self.rotation_bytes
        }))
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
        let mut buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let Some(bucket) = buckets.remove(seal_key) else {
            drop(buckets);
            let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {e}"),
            })?;
            return immutable
                .get(seal_key)
                .and_then(|entries| entries.iter().find(|entry| entry.is_pending()))
                .map(|entry| entry.frozen.clone())
                .ok_or_else(|| ScribeError::Internal {
                    detail: format!("seal-key not found: {seal_key}"),
                });
        };

        let seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        let frozen = bucket.freeze(seal_id);

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

    /// Snapshot active seal-keys owned by one fixed shard.
    pub(crate) fn active_seal_keys_for_shard(
        &self,
        shard: usize,
    ) -> Result<Vec<SealKey>, ScribeError> {
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets
            .keys()
            .filter(|key| shard_for(key.tenant, &key.table) == shard)
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

    /// Snapshot age-expired buckets owned by one fixed shard.
    pub(crate) fn expired_seal_keys_for_shard(
        &self,
        shard: usize,
        now: Instant,
    ) -> Result<Vec<SealKey>, ScribeError> {
        let buckets = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        Ok(buckets
            .iter()
            .filter(|(seal_key, bucket)| {
                shard_for(seal_key.tenant, &seal_key.table) == shard && bucket.is_age_expired(now)
            })
            .map(|(seal_key, _)| seal_key.clone())
            .collect())
    }

    /// Scan writable pressure metadata without mutating any bucket state.
    pub(crate) fn pressure_candidates(&self) -> Result<Vec<PressureCandidate>, ScribeError> {
        let buckets = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        Ok(buckets
            .iter()
            .filter(|(_, bucket)| bucket.bytes_accumulated > 0)
            .map(|(seal_key, bucket)| PressureCandidate {
                seal_key: seal_key.clone(),
                writable_bytes: bucket.bytes_accumulated,
                first_insert_at: bucket.first_insert_at,
                oldest_wal_lsn: bucket.metas.iter().map(|meta| meta.wal_lsn_min).min(),
            })
            .collect())
    }

    /// Select the globally deterministic largest writable victims.
    pub(crate) fn select_pressure_victims(
        mut candidates: Vec<PressureCandidate>,
        bytes_to_release: usize,
    ) -> Vec<SealKey> {
        if bytes_to_release == 0 {
            return Vec::new();
        }
        candidates.sort_by(|left, right| {
            right
                .writable_bytes
                .cmp(&left.writable_bytes)
                .then_with(|| left.first_insert_at.cmp(&right.first_insert_at))
                .then_with(|| left.seal_key.to_string().cmp(&right.seal_key.to_string()))
        });
        let mut selected = Vec::new();
        let mut selected_bytes = 0_usize;
        for candidate in candidates {
            selected_bytes = selected_bytes.saturating_add(candidate.writable_bytes);
            selected.push(candidate.seal_key);
            if selected_bytes >= bytes_to_release {
                break;
            }
        }
        selected
    }

    /// Select exactly one globally oldest writable WAL victim.
    pub(crate) fn select_oldest_wal_victim(candidates: &[PressureCandidate]) -> Option<SealKey> {
        candidates
            .iter()
            .filter_map(|candidate| {
                candidate.oldest_wal_lsn.map(|lsn| {
                    (
                        lsn,
                        candidate.first_insert_at,
                        candidate.seal_key.to_string(),
                        candidate.seal_key.clone(),
                    )
                })
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
                    .then_with(|| left.2.cmp(&right.2))
            })
            .map(|(_, _, _, seal_key)| seal_key)
    }

    /// Return writable and immutable append batches for one exact day range.
    ///
    /// Structural pruning happens while the memtable locks are held: tenant,
    /// table, and partition day are compared against the exact request. The
    /// selected columns are then projected before the detached snapshot is
    /// returned, so a snapshot never exposes an unrelated bucket or an
    /// unrequested Arrow column.
    pub fn readable_batches_for_range(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        start_day: crate::scribe::seal_key::EventDay,
        end_day: crate::scribe::seal_key::EventDay,
        required_columns: &[String],
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        if start_day > end_day {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }
        let writable = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut batches = Vec::new();

        for (seal_key, bucket) in writable.iter() {
            if seal_key.tenant == tenant
                && seal_key.table == *table
                && (start_day..=end_day).contains(&seal_key.day)
            {
                batches.extend(bucket.readable_batches(required_columns)?);
            }
        }
        for (seal_key, entries) in immutable.iter() {
            if seal_key.tenant == tenant
                && seal_key.table == *table
                && (start_day..=end_day).contains(&seal_key.day)
            {
                for entry in entries {
                    batches.extend(entry.frozen.readable_batches(required_columns)?);
                }
            }
        }
        Ok(batches)
    }

    /// Return all readable batches for a table, preserving the old unit-test
    /// convenience while applying the full table scope and schema.
    #[cfg(test)]
    pub fn readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        self.readable_batches_for_range(
            tenant,
            table,
            crate::scribe::seal_key::EventDay::new(chrono::NaiveDate::MIN),
            crate::scribe::seal_key::EventDay::new(chrono::NaiveDate::MAX),
            &[],
        )
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
        Ok(self
            .sweep_once_with_sizes_at(now)?
            .into_iter()
            .map(|(seal_key, wal_range, _arrow_bytes)| (seal_key, wal_range))
            .collect())
    }

    /// Retire committed immutable generations and retain their exact Arrow sizes.
    ///
    /// This shared primitive keeps ordinary WAL retirement and test-only memory
    /// accounting aligned: both consume exactly the generations removed here.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is poisoned.
    fn sweep_once_with_sizes_at(
        &self,
        now: Instant,
    ) -> Result<Vec<(SealKey, WalRange, usize)>, ScribeError> {
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
                    retired.push((
                        seal_key.clone(),
                        entry.wal_range(),
                        entry.frozen.arrow_bytes,
                    ));
                } else {
                    kept.push(entry);
                }
            }
            *entries = kept;
        }
        immutable.retain(|_, entries| !entries.is_empty());
        Ok(retired)
    }

    /// Retire eligible committed generations and return their exact Arrow bytes.
    ///
    /// This test-support control reports only bytes removed from the immutable
    /// map, so callers cannot release memory for a still-pending generation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is poisoned.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn sweep_once_for_test(&self, now: Instant) -> Result<usize, ScribeError> {
        Ok(self
            .sweep_once_with_sizes_at(now)?
            .into_iter()
            .fold(0_usize, |total, (_, _, arrow_bytes)| {
                total.saturating_add(arrow_bytes)
            }))
    }

    /// Retire one published generation after its grace period.
    pub(crate) fn retire_generation_at(
        &self,
        seal_id: u64,
        now: Instant,
    ) -> Result<Option<WalRange>, ScribeError> {
        let mut immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        let mut retired = None;
        for entries in immutable.values_mut() {
            let Some(index) = entries.iter().position(|entry| {
                entry.seal_id() == seal_id
                    && matches!(
                        &entry.state,
                        ImmutableState::Committed { observed_at, .. }
                            if now.saturating_duration_since(*observed_at) >= self.retention_grace
                    )
            }) else {
                continue;
            };
            let entry = entries.remove(index);
            retired = Some(entry.wal_range());
            break;
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

    /// Remove a replay generation when boot-time memory admission rejects it.
    pub(crate) fn discard_pending_generation(&self, seal_id: u64) -> Result<(), ScribeError> {
        let mut immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        for entries in immutable.values_mut() {
            if let Some(index) = entries
                .iter()
                .position(|entry| entry.seal_id == seal_id && entry.is_pending())
            {
                entries.remove(index);
                break;
            }
        }
        immutable.retain(|_, entries| !entries.is_empty());
        Ok(())
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
        let writable_buckets = writable.len();
        drop(writable);

        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let immutable_rows = immutable
            .values()
            .flatten()
            .map(|entry| entry.frozen.row_count())
            .sum();
        let immutable_bytes = immutable
            .values()
            .flatten()
            .map(|entry| entry.frozen.arrow_bytes)
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
            writable_buckets,
            immutable_buckets: immutable.len(),
        })
    }

    /// Return exact writable and immutable byte ownership by seal-key.
    pub(crate) fn bucket_memory(&self) -> Result<Vec<BucketMemorySnapshot>, ScribeError> {
        let writable = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        let immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        let mut by_key = HashMap::<SealKey, BucketMemorySnapshot>::new();
        for (seal_key, bucket) in writable.iter() {
            by_key.insert(
                seal_key.clone(),
                BucketMemorySnapshot {
                    seal_key: seal_key.clone(),
                    row_count: bucket.row_count,
                    writable_bytes: bucket.bytes_accumulated,
                    immutable_bytes: 0,
                },
            );
        }
        for (seal_key, entries) in immutable.iter() {
            let entry = by_key
                .entry(seal_key.clone())
                .or_insert_with(|| BucketMemorySnapshot {
                    seal_key: seal_key.clone(),
                    row_count: 0,
                    writable_bytes: 0,
                    immutable_bytes: 0,
                });
            entry.row_count = entry
                .row_count
                .saturating_add(entries.iter().map(|entry| entry.frozen.row_count()).sum());
            entry.immutable_bytes = entries.iter().map(|entry| entry.frozen.arrow_bytes).sum();
        }
        Ok(by_key.into_values().collect())
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

    fn should_seal_at(&self, now: Instant, rotation_bytes: usize) -> bool {
        self.bytes_accumulated >= rotation_bytes
            || now.saturating_duration_since(self.first_insert_at) >= ACTIVE_GENERATION_MAX_AGE
    }

    fn is_age_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.first_insert_at) >= ACTIVE_GENERATION_MAX_AGE
    }

    fn freeze(self, seal_id: u64) -> FrozenMemtable {
        let batches = self.batches;

        let opened_at = self.first_insert_at;
        let closed_at = Instant::now();
        let arrow_bytes = self.bytes_accumulated;

        FrozenMemtable {
            seal_id,
            seal_key: self.seal_key,
            schema: self.schema,
            batches,
            events: self.events,
            metas: self.metas,
            opened_at,
            closed_at,
            arrow_bytes,
        }
    }

    fn readable_batches(
        &self,
        required_columns: &[String],
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let projection = projection_indices(&self.schema, required_columns)?;
        self.batches
            .iter()
            .cloned()
            .zip(self.metas.iter().cloned())
            .map(|(batch, meta)| {
                let batch = batch
                    .project(&projection)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("live-tail Arrow projection failed: {error}"),
                    })?;
                Ok(ReadableBatch {
                    partition_day: self.seal_key.day,
                    meta,
                    batch,
                })
            })
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
    /// Exact partition day owning the batch.
    pub partition_day: crate::scribe::seal_key::EventDay,
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
/// Immutable snapshot detached from the writable bucket. It retains the original
/// append batches so live tail reads preserve WAL/append granularity. A merged
/// batch is materialized only inside the bounded persistence encoder; retaining
/// both forms here would double the Arrow memory charged to Scribe.
#[derive(Debug, Clone)]
pub struct FrozenMemtable {
    /// Local immutable-generation identity.
    pub seal_id: u64,
    /// The seal-key this snapshot belongs to.
    pub seal_key: SealKey,
    /// Arrow schema shared by all append batches.
    pub schema: SchemaRef,
    /// Original append batches, preserved for LSN-granular live tail reads.
    pub batches: Vec<RecordBatch>,
    /// Ordered list of `AuditEvent`s staged for the seal transaction.
    pub events: Vec<AuditEvent>,
    /// Per-append metadata derived from WAL record headers.
    pub metas: Vec<ScribeAppendMeta>,
    /// Monotonic time at which the active generation first received a row.
    pub opened_at: Instant,
    /// Monotonic time at which the generation was detached.
    pub closed_at: Instant,
    /// Estimated Arrow memory retained by the generation.
    pub arrow_bytes: usize,
}

impl FrozenMemtable {
    /// Return the rows retained by this immutable generation.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }

    fn readable_batches(
        &self,
        required_columns: &[String],
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let projection = projection_indices(&self.schema, required_columns)?;
        self.batches
            .iter()
            .cloned()
            .zip(self.metas.iter().cloned())
            .map(|(batch, meta)| {
                let batch = batch
                    .project(&projection)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("live-tail Arrow projection failed: {error}"),
                    })?;
                Ok(ReadableBatch {
                    partition_day: self.seal_key.day,
                    meta,
                    batch,
                })
            })
            .collect()
    }
}

/// Resolve and validate the bounded projection requested by Oracle.
fn projection_indices(
    schema: &SchemaRef,
    required_columns: &[String],
) -> Result<Vec<usize>, ScribeError> {
    if required_columns.is_empty() {
        return Ok((0..schema.fields().len()).collect());
    }
    let mut indices = Vec::with_capacity(required_columns.len());
    for name in required_columns {
        let Some(index) = schema.index_of(name).ok() else {
            return Err(ScribeError::Internal {
                detail: format!("live-tail required column is not in the table schema: {name}"),
            });
        };
        if indices.contains(&index) {
            return Err(ScribeError::Internal {
                detail: format!("live-tail required column is duplicated: {name}"),
            });
        }
        indices.push(index);
    }
    Ok(indices)
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
    use crate::scribe::preprocess::AppendSliceId;
    use crate::scribe::replay::{ReplayedAppendMeta, ReplayedSealKey};
    use crate::scribe::seal_key::EventDay;
    use crate::scribe::wal::WalLsn;
    use arrow::array::{Int32Array, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use chrono::NaiveDate;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::time::Duration;

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
            crate::test_support::tenant(),
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("valid date")),
        )
    }

    fn make_file_list_key(min: u64, max: u64) -> FileListCommitKey {
        FileListCommitKey {
            data_tenant_id: crate::test_support::tenant(),
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            node_id: uuid::Uuid::nil(),
            writer_epoch: 1,
            wal_lsn_min: i64::try_from(min).expect("test lsn"),
            wal_lsn_max: i64::try_from(max).expect("test lsn"),
        }
    }

    /// Encodes one replay handoff carrying caller-selected persisted ordinals.
    fn replayed_ordinals(values: Vec<i32>) -> ReplayedSealKey {
        let rows_accepted = values.len();
        let schema = Arc::new(Schema::new(vec![Field::new(
            wyrd_spec::vala::WYRD_ROW_ORDINAL,
            DataType::Int32,
            false,
        )]));
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int32Array::from(values))])
            .expect("ordinal batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
        writer.write(&batch).expect("IPC batch");
        writer.finish().expect("IPC finish");
        let seal_key = make_test_seal_key();
        let batch_id = uuid::Uuid::now_v7();
        ReplayedSealKey {
            stream: crate::scribe::stream_identity::StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::generate(),
                crate::scribe::stream_identity::WriterEpoch::new(1),
            ),
            seal_key: seal_key.clone(),
            audit_events: vec![make_test_event()],
            data_records: vec![bytes],
            append_metas: vec![ReplayedAppendMeta {
                batch_id: *batch_id.as_bytes(),
                wal_lsn: WalLsn::new(1),
                rows_accepted,
                append_slice_id: AppendSliceId { batch_id, seal_key },
                schema_fingerprint: [0; 32],
            }],
            wal_segments: Vec::new(),
        }
    }

    /// Fails replay before state mutation when persisted row identity is negative.
    #[test]
    fn negative_persisted_ordinal_fails_invariant() {
        let error = Memtable::decode_replayed(&replayed_ordinals(vec![-1]))
            .expect_err("negative row identity cannot enter replayed state");
        assert!(matches!(
            error,
            ScribeError::Internal { detail }
                if detail.contains("row identity invariant") && detail.contains("negative")
        ));
    }

    /// Preserves a valid non-zero slice ordinal instead of assigning from replay position.
    #[test]
    fn replayed_batch_does_not_reassign_ordinal() {
        let frozen = Memtable::decode_replayed(&replayed_ordinals(vec![2, 3]))
            .expect("valid replayed ordinals decode");
        let ordinals = frozen.batches[0]
            .column_by_name(wyrd_spec::vala::WYRD_ROW_ORDINAL)
            .expect("ordinal column persists")
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("ordinal remains Int32");
        assert_eq!(ordinals.values(), &[2, 3]);
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

        assert_eq!(frozen.row_count(), 2);
        assert_eq!(memtable.row_count(&seal_key).expect("row count"), 0);
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 1);
        assert_eq!(memtable.pending_generation_count().expect("pending"), 1);
        assert_eq!(
            memtable
                .readable_batches(crate::test_support::tenant(), &seal_key.table)
                .expect("readable")
                .len(),
            1
        );
    }

    #[test]
    fn retained_batch_lookup_covers_active_and_immutable_state() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        let batch_id = [7u8; 16];
        let mut meta = make_test_meta(10);
        meta.batch_id = batch_id;
        memtable
            .insert(&seal_key, make_test_event(), meta, make_test_batch(2))
            .expect("insert");
        assert_eq!(
            memtable
                .retained_batch_rows(&seal_key, batch_id)
                .expect("active lookup"),
            Some(100)
        );
        memtable.freeze(&seal_key).expect("freeze");
        assert_eq!(
            memtable
                .retained_batch_rows(&seal_key, batch_id)
                .expect("immutable lookup"),
            Some(100)
        );
        assert_eq!(
            memtable
                .retained_batch_rows(&seal_key, [8u8; 16])
                .expect("missing lookup"),
            None
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
            .readable_batches(crate::test_support::tenant(), &seal_key.table)
            .expect("readable")
            .into_iter()
            .map(|batch| batch.meta.wal_lsn_max)
            .collect();
        assert_eq!(lsns, vec![WalLsn::new(10), WalLsn::new(20)]);
    }

    /// Verifies elapsed retention removes both a generation and its empty bucket.
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
        assert_eq!(
            memtable.stats().expect("retired stats").immutable_buckets,
            0
        );
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
    fn row_count_does_not_trigger_generation_rotation() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 60k rows in 6 batches of 10k each
        for i in 0..6 {
            let batch = make_test_batch(10_000);
            memtable
                .insert(&seal_key, make_test_event(), make_test_meta(i), batch)
                .expect("insert");
        }

        assert!(!memtable.should_seal(&seal_key).expect("should_seal"));
        assert_eq!(memtable.row_count(&seal_key).expect("row_count"), 60_000);
    }

    /// Proves only a non-empty bucket crossing the target requests a pre-insert seal.
    #[test]
    fn would_cross_rotation_bounds_generation() {
        let incoming = make_test_batch(2);
        let incoming_bytes = estimate_batch_bytes(&incoming);
        let memtable = Memtable::new_with_limits(Duration::from_mins(1), incoming_bytes + 1);
        let seal_key = make_test_seal_key();

        assert!(
            !memtable
                .would_cross_rotation(&seal_key, &make_test_batch(100))
                .expect("absent bucket")
        );
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("seed bucket");
        {
            let mut buckets = memtable.writable.lock().expect("writable lock");
            buckets
                .get_mut(&seal_key)
                .expect("seed bucket")
                .bytes_accumulated = memtable.rotation_bytes - 1;
        }
        assert!(
            memtable
                .would_cross_rotation(&seal_key, &incoming)
                .expect("crossing bucket")
        );

        let empty_key = SealKey::new(
            seal_key.tenant,
            seal_key.table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 16).expect("valid date")),
        );
        memtable.writable.lock().expect("writable lock").insert(
            empty_key.clone(),
            MemtableBucket::new(empty_key.clone(), incoming.schema()),
        );
        assert!(
            !memtable
                .would_cross_rotation(&empty_key, &make_test_batch(100))
                .expect("empty oversized bucket")
        );
    }

    #[test]
    fn active_generation_age_is_the_only_time_predicate() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        let batch = make_test_batch(1);
        memtable
            .insert(&seal_key, make_test_event(), make_test_meta(0), batch)
            .expect("insert");

        let mut buckets = memtable.writable.lock().expect("writable lock");
        buckets.get_mut(&seal_key).expect("bucket").first_insert_at = Instant::now()
            .checked_sub(ACTIVE_GENERATION_MAX_AGE)
            .expect("test clock supports age offset");
        drop(buckets);

        assert!(
            memtable.should_seal(&seal_key).expect("should_seal"),
            "600s active age triggers rotation"
        );
    }

    /// Test-only retirement reports only bytes from generations it removed.
    #[test]
    fn test_retirement_bytes_exclude_ineligible_immutable_generations() {
        let memtable = Memtable::new_with_retention(Duration::from_secs(1));
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
        let first_bytes = memtable.stats().expect("first stats").immutable_bytes;
        std::thread::sleep(Duration::from_millis(1_100));

        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(20),
                make_test_batch(1),
            )
            .expect("second insert");
        let second = memtable.freeze(&seal_key).expect("second freeze");
        memtable
            .complete_post_commit(second.seal_id, make_file_list_key(20, 20))
            .expect("second complete");
        let total_bytes = memtable.stats().expect("combined stats").immutable_bytes;

        let retired_bytes = memtable
            .sweep_once_for_test(Instant::now())
            .expect("test retirement");
        assert_eq!(retired_bytes, first_bytes);
        assert!(retired_bytes < total_bytes);
        assert_eq!(
            memtable.stats().expect("remaining stats").immutable_bytes,
            total_bytes.saturating_sub(retired_bytes)
        );
        assert_eq!(memtable.immutable_generation_count().expect("remaining"), 1);
    }

    #[test]
    fn historical_128_mib_predicate_is_removed() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(0),
                make_test_batch(1),
            )
            .expect("insert");

        assert!(!memtable.should_seal(&seal_key).expect("should_seal"));
    }

    #[test]
    fn inactivity_does_not_trigger_generation_rotation() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        let batch = make_test_batch(1);
        memtable
            .insert(&seal_key, make_test_event(), make_test_meta(0), batch)
            .expect("insert");

        assert!(!memtable.should_seal(&seal_key).expect("should_seal"));
    }

    #[test]
    fn pressure_selection_prefers_largest_then_oldest_writable_buckets() {
        let memtable = Memtable::new();
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            first.table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid date")),
        );
        memtable
            .insert(
                &first,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("first insert");
        memtable
            .insert(
                &second,
                make_test_event(),
                make_test_meta(2),
                make_test_batch(1),
            )
            .expect("second insert");
        {
            let mut buckets = memtable.writable.lock().expect("writable lock");
            buckets
                .get_mut(&first)
                .expect("first bucket")
                .bytes_accumulated = 200;
            buckets
                .get_mut(&second)
                .expect("second bucket")
                .bytes_accumulated = 100;
        }
        let selected = Memtable::select_pressure_victims(
            memtable.pressure_candidates().expect("pressure candidates"),
            150,
        );
        assert_eq!(selected, vec![first]);
    }

    #[test]
    fn global_pressure_flushes_from_seventy_five_to_sixty_five_percent() {
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            first.table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid date")),
        );
        let now = Instant::now();
        let selected = Memtable::select_pressure_victims(
            vec![
                PressureCandidate {
                    seal_key: first.clone(),
                    writable_bytes: 200,
                    first_insert_at: now,
                    oldest_wal_lsn: Some(crate::scribe::wal::WalLsn::new(10)),
                },
                PressureCandidate {
                    seal_key: second.clone(),
                    writable_bytes: 100,
                    first_insert_at: now,
                    oldest_wal_lsn: Some(crate::scribe::wal::WalLsn::new(11)),
                },
            ],
            250,
        );
        assert_eq!(selected, vec![first, second]);
    }

    #[test]
    fn wal_pressure_flushes_exactly_one_global_oldest_bucket() {
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            first.table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid date")),
        );
        let now = Instant::now();
        let selected = Memtable::select_oldest_wal_victim(&[
            PressureCandidate {
                seal_key: first,
                writable_bytes: 200,
                first_insert_at: now,
                oldest_wal_lsn: Some(crate::scribe::wal::WalLsn::new(11)),
            },
            PressureCandidate {
                seal_key: second.clone(),
                writable_bytes: 100,
                first_insert_at: now,
                oldest_wal_lsn: Some(crate::scribe::wal::WalLsn::new(10)),
            },
        ]);
        assert_eq!(selected, Some(second));
    }

    #[test]
    fn stale_pressure_key_is_a_noop() {
        assert!(Memtable::select_pressure_victims(Vec::new(), 1).is_empty());
        assert!(Memtable::select_oldest_wal_victim(&[]).is_none());
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

        assert_eq!(frozen.row_count(), 300, "3 batches × 100 rows retained");
        assert_eq!(frozen.batches.len(), 3);
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
    fn snapshot_detach_is_safe_during_concurrent_append_and_flush() {
        let memtable = Arc::new(Memtable::new());
        let seal_key = make_test_seal_key();
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("initial insert");
        let barrier = Arc::new(Barrier::new(2));
        let freezer_memtable = Arc::clone(&memtable);
        let freezer_key = seal_key.clone();
        let freezer_barrier = Arc::clone(&barrier);
        let freezer = std::thread::spawn(move || {
            freezer_barrier.wait();
            freezer_memtable.freeze(&freezer_key)
        });
        let inserter_memtable = Arc::clone(&memtable);
        let inserter_key = seal_key.clone();
        let inserter_barrier = Arc::clone(&barrier);
        let inserter = std::thread::spawn(move || {
            inserter_barrier.wait();
            inserter_memtable.insert(
                &inserter_key,
                make_test_event(),
                make_test_meta(2),
                make_test_batch(1),
            )
        });

        freezer.join().expect("freeze thread").expect("freeze");
        inserter.join().expect("append thread").expect("append");
        let stats = memtable.stats().expect("stats");
        assert_eq!(stats.writable_rows + stats.immutable_rows, 2);
        assert_eq!(stats.immutable_generations, 1);
    }

    #[test]
    fn memtable_per_seal_key_isolation() {
        let memtable = Memtable::new();
        let tenant = crate::test_support::tenant();
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
        assert_eq!(frozen1.row_count(), 300, "3 batches × 100 rows retained");

        // key2 untouched
        assert_eq!(
            memtable.row_count(&key2).expect("key2 count after freeze"),
            100
        );
    }
}
