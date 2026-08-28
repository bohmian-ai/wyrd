//! In-memory row buffer keyed by seal-key with seal predicate.
//!
//! The memtable holds Arrow buffers + paired `AuditEvent` lists per
//! `SealKey = (DataTenantId, TableRef, TimePartition)`. Rotation is driven by
//! retained Arrow size, active-generation age, and global pressure. Automatic
//! writer rotation swaps every non-empty bucket as one shard cohort; selective
//! pressure and explicit seals continue to freeze individual keys.

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
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::ScribeAppendMeta;

/// Estimated Arrow bytes at which the active generation rotates.
pub const MEMTABLE_ROTATION_BYTES: usize = 512 * 1024 * 1024;
/// Fallback active-generation max age used by [`Memtable::new`] and
/// [`Memtable::new_with_rotation`] when no runtime value is supplied.
///
/// Production wiring threads the D83 seconds-scale
/// [`crate::scribe::ScribePressureConfig::seal_max_age`] (600 s) through the
/// shard runtime instead of reading this constant, so trickle workloads get
/// bounded visibility latency. This value is retained only so bare-`Memtable`
/// unit tests keep compiling.
pub const ACTIVE_GENERATION_MAX_AGE: Duration = Duration::from_mins(10);

/// Closed reason a memtable bucket was sealed/rotated (D83/D84 seam).
///
/// Every seal or rotation event classifies into exactly one variant.
/// `MemtableBucket::should_seal_at` distinguishes [`SealTriggerReason::Size`]
/// from [`SealTriggerReason::Age`]; the coordinated ingress-pressure path tags
/// [`SealTriggerReason::Pressure`]. T40 (18-H2) reads this to label the
/// `bifrost_scribe_seal_total{trigger}` counter, so the variant names are a
/// normative interface, not implementor latitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealTriggerReason {
    /// `bytes_accumulated >= rotation_bytes` (size trigger).
    Size,
    /// Active generation exceeded `seal_max_age` (age trigger).
    Age,
    /// Coordinated ingress-pressure seal toward the low-water mark.
    Pressure,
}

impl SealTriggerReason {
    /// Return the stable lowercase label used by seal telemetry.
    ///
    /// The returned string is the value T40 attaches as the `trigger` label on
    /// `bifrost_scribe_seal_total`; keep it stable across changes.
    #[must_use]
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Age => "age",
            Self::Pressure => "pressure",
        }
    }
}

/// Memtable — in-memory row buffer keyed by seal-key.
///
/// Each seal-key holds an Arrow buffer, paired `AuditEvent` list, and
/// `ScribeAppendMeta` list. Freezing a seal-key detaches an immutable snapshot.
///
/// Retirement is immediate: a generation is eligible the moment it enters
/// [`ImmutableState::Durable`], which is only reached after the fenced
/// `vala.file_list` + audit transaction commits. Retired Arrow memory is
/// released before WAL segment retirement is submitted, preserving the
/// ordering contract in `ShardOwner::retire_committed`.
#[derive(Debug)]
pub struct Memtable {
    pub(crate) writable: Arc<Mutex<HashMap<SealKey, MemtableBucket>>>,
    pub(crate) immutable: Arc<Mutex<HashMap<SealKey, Vec<ImmutableEntry>>>>,
    next_seal_id: Arc<AtomicU64>,
    rotation_bytes: usize,
    /// Runtime active-generation max age consulted by the seal predicates.
    ///
    /// Threaded from [`crate::scribe::ScribePressureConfig::seal_max_age`]
    /// through the shard runtime the same way `rotation_bytes` is threaded, so
    /// the age trigger uses the configured seconds-scale value rather than the
    /// [`ACTIVE_GENERATION_MAX_AGE`] constant.
    seal_max_age: Duration,
    /// Pod-local shard lane this memtable belongs to, for registry identity.
    ///
    /// Zero for a bare memtable in a unit test, which registers nothing.
    shard_id: usize,
    /// Pod-wide authority registry, when this memtable belongs to a pod.
    ///
    /// Every immutable generation is registered here as memtable-authoritative
    /// and forgotten when it retires, which is what lets the staging and
    /// publication owners move that authority forward and what stops a
    /// generation's Arrow from being released while nothing durable holds its
    /// rows. A bare memtable in a unit test has no pod and therefore no
    /// registry; it tracks no authority and none is asked of it.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
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

/// Resolves one retained batch ID only when stable logical slice facts match.
///
/// # Errors
///
/// Returns an invariant error when a caller reuses a retained batch ID for a
/// different schema/data, length, slice ordinal, or complete slice count.
fn retained_identity_rows(
    meta: &ScribeAppendMeta,
    identity: crate::scribe::wal::ScribeAppendPayloadIdentity,
) -> Result<Option<u64>, ScribeError> {
    if meta.schema_fingerprint != identity.schema_fingerprint
        || meta.data_digest != identity.data_digest
        || meta.data_len != identity.data_len
        || meta.slice_index != identity.slice_index
        || meta.slice_count != identity.slice_count
    {
        return Err(ScribeError::Internal {
            detail: "retained Scribe batch ID was reused with contradictory payload identity"
                .to_owned(),
        });
    }
    Ok(Some(u64::try_from(meta.rows_accepted).unwrap_or(u64::MAX)))
}

impl Memtable {
    /// Construct a new empty memtable with the default active-bucket rotation threshold.
    ///
    /// Retirement is immediate: a [`ImmutableState::Durable`] generation is
    /// eligible at the first lifecycle sweep after its SQL commit.
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_rotation(MEMTABLE_ROTATION_BYTES)
    }

    /// Construct a memtable with an explicit rotation threshold and the fallback
    /// [`ACTIVE_GENERATION_MAX_AGE`].
    ///
    /// The grace-period parameter that previously existed here has been removed.
    /// Committed generations are immediately retirement-eligible; there is no
    /// configurable delay. Production wiring uses [`Memtable::new_with_config`]
    /// to supply the runtime [`crate::scribe::ScribePressureConfig::seal_max_age`];
    /// this constructor retains the constant fallback for bare-`Memtable` tests.
    ///
    /// # Errors
    ///
    /// This constructor is infallible; it always returns a fresh, empty memtable.
    #[must_use]
    pub fn new_with_rotation(rotation_bytes: usize) -> Self {
        Self::new_with_config(rotation_bytes, ACTIVE_GENERATION_MAX_AGE)
    }

    /// Binds this memtable to its pod's hot-source authority registry.
    ///
    /// Production wiring calls this immediately after construction, before the
    /// memtable accepts a row, so every generation it ever freezes is tracked
    /// from its first moment as memtable-authoritative.
    #[must_use]
    pub(crate) fn with_hot_sources(
        mut self,
        shard_id: usize,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) -> Self {
        self.shard_id = shard_id;
        self.hot_sources = Some(hot_sources);
        self
    }

    /// Returns this memtable's registry identity for one of its generations.
    ///
    /// The shard lane is part of the identity because every lane numbers its
    /// own generations from one: without it, two lanes serving the same seal
    /// key would claim the same registry entry.
    fn ordinal(&self, seal_id: u64) -> crate::scribe::hot_source::GenerationOrdinal {
        crate::scribe::hot_source::GenerationOrdinal::new(
            u16::try_from(self.shard_id).unwrap_or(u16::MAX),
            seal_id,
        )
    }

    /// Registers one newly frozen generation as memtable-authoritative.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry refuses the
    /// registration, which means the generation identity was reused — a
    /// reader following the older registration would look for rows in a
    /// generation that no longer holds them.
    fn register_hot_source(&self, key: &SealKey, seal_id: u64) -> Result<(), ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(());
        };
        hot_sources
            .register_memtable(key, self.ordinal(seal_id))
            .map_err(|error| ScribeError::Internal {
                detail: format!("register generation {seal_id} as memtable-authoritative: {error}"),
            })
    }

    /// Construct a memtable with an explicit rotation threshold and max age.
    ///
    /// This is the constructor production wiring uses: the shard runtime threads
    /// `rotation_bytes` (the active-bucket target) and the configured
    /// `seal_max_age` (D83 default 600 s) so the size and age seal predicates read
    /// runtime values rather than module constants.
    ///
    /// # Errors
    ///
    /// This constructor is infallible; it always returns a fresh, empty memtable.
    #[must_use]
    pub fn new_with_config(rotation_bytes: usize, seal_max_age: Duration) -> Self {
        Self {
            writable: Arc::new(Mutex::new(HashMap::new())),
            immutable: Arc::new(Mutex::new(HashMap::new())),
            next_seal_id: Arc::new(AtomicU64::new(1)),
            rotation_bytes,
            seal_max_age,
            shard_id: 0,
            hot_sources: None,
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

    /// Computes the encoder candidate peak after appending whole incoming batches.
    ///
    /// The calculation borrows the active bucket and does not concatenate or
    /// clone Arrow arrays. Its grouping rule is shared with the Parquet writer,
    /// allowing the shard owner to rotate before WAL mutation when appending to
    /// an existing candidate would make the generation unreplayable.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the writable lock is poisoned or
    /// candidate arithmetic overflows.
    pub(crate) fn projected_candidate_peak(
        &self,
        seal_key: &SealKey,
        incoming: impl IntoIterator<Item = (usize, usize)>,
    ) -> Result<usize, ScribeError> {
        let writable = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        let retained = writable.get(seal_key).into_iter().flat_map(|bucket| {
            bucket
                .batches
                .iter()
                .map(|batch| (batch.num_rows(), estimate_batch_bytes(batch)))
        });
        crate::scribe::parquet_writer::largest_candidate_bytes_from_facts(retained.chain(incoming))
    }

    /// Return the row count for a batch that is still retained in the active
    /// or immutable grace state for this exact seal key.
    ///
    /// The lookup is the runtime idempotency index. It is intentionally
    /// derived from the bounded buckets instead of maintaining a process-wide
    /// append-ID set whose memory would grow with the lifetime of a shard.
    pub(crate) fn retained_batch_rows(
        &self,
        seal_key: &SealKey,
        identity: crate::scribe::wal::ScribeAppendPayloadIdentity,
    ) -> Result<Option<u64>, ScribeError> {
        let writable = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        if let Some(bucket) = writable.get(seal_key)
            && let Some(meta) = bucket
                .metas
                .iter()
                .find(|meta| {
                    meta.batch_id == identity.batch_id && meta.slice_index == identity.slice_index
                })
                .or_else(|| {
                    bucket
                        .metas
                        .iter()
                        .find(|meta| meta.batch_id == identity.batch_id)
                })
        {
            return retained_identity_rows(meta, identity);
        }
        drop(writable);

        let immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        if let Some(meta) = immutable.get(seal_key).and_then(|entries| {
            entries.iter().find_map(|entry| {
                entry
                    .frozen
                    .metas
                    .iter()
                    .find(|meta| {
                        meta.batch_id == identity.batch_id
                            && meta.slice_index == identity.slice_index
                    })
                    .or_else(|| {
                        entry
                            .frozen
                            .metas
                            .iter()
                            .find(|meta| meta.batch_id == identity.batch_id)
                    })
            })
        }) {
            return retained_identity_rows(meta, identity);
        }
        Ok(None)
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
            let (data_digest, data_len) = crate::scribe::preprocess::logical_data_identity(&batch)?;
            batches.push(batch);
            metas.push(ScribeAppendMeta {
                batch_id: replayed_meta.batch_id,
                schema_fingerprint: replayed_meta.schema_fingerprint,
                data_digest,
                data_len,
                payload_digest: replayed_meta.payload_digest,
                payload_len: replayed_meta.payload_len,
                slice_index: replayed_meta.append_slice_id.slice_index,
                slice_count: replayed_meta.slice_count,
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
            // Both identities are stamped by the memtable that adopts this
            // decoded generation, in `insert_replayed_frozen`.
            shard_id: 0,
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
        frozen.shard_id = self.shard_id;
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        immutable
            .entry(frozen.seal_key.clone())
            .or_default()
            .push(ImmutableEntry::pending(frozen.clone()));
        drop(immutable);
        self.register_hot_source(&frozen.seal_key, frozen.seal_id)?;
        Ok(frozen)
    }

    /// Check seal predicate for a specific seal-key.
    ///
    /// Returns `true` if the seal-key should be frozen.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn should_seal(&self, seal_key: &SealKey) -> Result<bool, ScribeError> {
        Ok(self.should_seal_reason(seal_key)?.is_some())
    }

    /// Returns the size/age trigger that would seal this key, if any.
    ///
    /// This is the reason-carrying form of [`Self::should_seal`]: it applies the
    /// same size-then-age precedence as `MemtableBucket::should_seal_at` and
    /// returns the [`SealTriggerReason`] that fired so the caller (the shard
    /// rotation sweep) can split keys by trigger and label the
    /// `bifrost_scribe_seal_total{trigger}` counter (D84). Only `Size` and `Age`
    /// are ever returned here; `Pressure` originates from the coordinated
    /// ingress-pressure path, not this per-key rotation check.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub fn should_seal_reason(
        &self,
        seal_key: &SealKey,
    ) -> Result<Option<SealTriggerReason>, ScribeError> {
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        if let Some(bucket) = buckets.get(seal_key) {
            Ok(bucket.should_seal_at(Instant::now(), self.rotation_bytes, self.seal_max_age))
        } else {
            Ok(None)
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
        let frozen = bucket.freeze(seal_id, self.shard_id);

        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        immutable
            .entry(seal_key.clone())
            .or_default()
            .push(ImmutableEntry::pending(frozen.clone()));
        drop(immutable);
        self.register_hot_source(seal_key, frozen.seal_id)?;
        Ok(frozen)
    }

    /// Atomically freezes every non-empty writable key in stable identity order.
    ///
    /// The writable and immutable locks are held across the complete swap, so
    /// readers observe either the old active generation or every new immutable
    /// member. No Arrow batch or lease is duplicated.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when either memtable lock is poisoned.
    pub(crate) fn freeze_all_nonempty(&self) -> Result<Vec<FrozenMemtable>, ScribeError> {
        let mut writable = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        let mut immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        let mut buckets: Vec<_> = writable
            .drain()
            .filter(|(_, bucket)| bucket.row_count != 0)
            .collect();
        buckets.sort_by_key(|(key, _)| key.to_string());
        let mut frozen = Vec::with_capacity(buckets.len());
        for (key, bucket) in buckets {
            let seal_id = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
            let member = bucket.freeze(seal_id, self.shard_id);
            self.register_hot_source(&key, seal_id)?;
            immutable
                .entry(key)
                .or_default()
                .push(ImmutableEntry::pending(member.clone()));
            frozen.push(member);
        }
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

    /// Returns exact writable Arrow ownership for one seal-key before freeze.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the writable-bucket lock is poisoned.
    pub(crate) fn writable_bytes(&self, seal_key: &SealKey) -> Result<usize, ScribeError> {
        let buckets = self
            .writable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable bucket lock poisoned: {error}"),
            })?;
        Ok(buckets
            .get(seal_key)
            .map_or(0, |bucket| bucket.bytes_accumulated))
    }

    /// Report whether a stranded pending frozen generation exists for a key.
    ///
    /// The shard seal-retry path uses this to tell a genuinely stranded
    /// state-A generation — one [`Self::freeze`] produced but a downstream
    /// persistence-prep step failed to queue — apart from a stale retry mark
    /// whose generation was already completed and discarded. Returning `false`
    /// lets the caller drop the stale mark as a safe no-op instead of calling
    /// [`Self::freeze`], which would fail with "seal-key not found" when no
    /// writable bucket and no pending entry remain. Only entries still in the
    /// pending state are counted; committing or aborting a generation clears
    /// its pending flag, so a completed seal reports `false`.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the immutable-entry lock is
    /// poisoned.
    pub(crate) fn has_pending_frozen(&self, seal_key: &SealKey) -> Result<bool, ScribeError> {
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok(immutable
            .get(seal_key)
            .is_some_and(|entries| entries.iter().any(ImmutableEntry::is_pending)))
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

    /// Snapshot all active seal-keys held by this shard's memtable.
    ///
    /// Under batch-spread routing a shard may hold buckets for any (tenant,
    /// table) combination, so this method returns every bucket the shard
    /// actually holds rather than filtering by a recomputed routing key.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub(crate) fn active_seal_keys_for_shard(
        &self,
        _shard: usize,
    ) -> Result<Vec<SealKey>, ScribeError> {
        let buckets = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets.keys().cloned().collect())
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

    /// Snapshot age-expired buckets held by this shard's memtable.
    ///
    /// Under batch-spread routing a shard may hold buckets for any (tenant,
    /// table) combination. This method returns all age-expired buckets the
    /// shard actually holds rather than filtering by a recomputed routing key.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the writable bucket lock is poisoned.
    pub(crate) fn expired_seal_keys_for_shard(
        &self,
        _shard: usize,
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
            .filter(|(_, bucket)| bucket.is_age_expired(now, self.seal_max_age))
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

    /// Return writable and immutable append batches for one exact partition range.
    ///
    /// Structural pruning happens while the memtable locks are held: tenant,
    /// table, and time partition are compared against the exact request. The
    /// selected columns are then projected before the detached snapshot is
    /// returned, so a snapshot never exposes an unrelated bucket or an
    /// unrequested Arrow column.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the requested range is inverted,
    /// [`ScribeError::IngestBusy`] before projection when the batch or
    /// retained-byte ceiling is exhausted, or an internal error when locks,
    /// checked arithmetic, column selection, or Arrow projection fail.
    pub(crate) fn readable_batches_for_range(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        start_partition: crate::catalog::layout::TimePartition,
        end_partition: crate::catalog::layout::TimePartition,
        required_columns: &[String],
        limits: ReadableBatchLimits,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        if start_partition > end_partition {
            return Err(ScribeError::Internal {
                detail: "live-tail start partition is after end partition".to_owned(),
            });
        }
        self.collect_readable_batches(
            tenant,
            table,
            Some(&(start_partition..=end_partition)),
            required_columns,
            limits,
        )
    }

    /// Collects readable batches, optionally restricted to one partition range.
    ///
    /// `partitions` is `None` for a whole-table scan; production tail reads
    /// always supply an exact inclusive range. Keeping the option here means a
    /// whole-table scan does not need a synthetic minimum/maximum partition,
    /// which would be wrong anyway because partition ordering is keyed on
    /// granularity before start.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] before projection when the batch or
    /// retained-byte ceiling is exhausted, or an internal error when locks,
    /// checked arithmetic, column selection, or Arrow projection fail.
    fn collect_readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        partitions: Option<&std::ops::RangeInclusive<crate::catalog::layout::TimePartition>>,
        required_columns: &[String],
        limits: ReadableBatchLimits,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let selects = |seal_key: &SealKey| {
            seal_key.tenant == tenant
                && seal_key.table == *table
                && partitions.is_none_or(|range| range.contains(&seal_key.partition))
        };
        let writable = self.writable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        let immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut batches =
            ReadableBatchCollector::new(limits.max_batches, limits.max_retained_bytes);

        for (seal_key, bucket) in writable.iter() {
            if selects(seal_key) {
                bucket.append_readable_batches(required_columns, &mut batches)?;
            }
        }
        for (seal_key, entries) in immutable.iter() {
            if selects(seal_key) {
                for entry in entries {
                    entry
                        .frozen
                        .append_readable_batches(required_columns, &mut batches)?;
                }
            }
        }
        Ok(batches.finish())
    }

    /// Captures one atomic active-plus-immutable provider cut at a persisted cursor.
    ///
    /// Both owner maps remain locked while the cut is selected. A publication
    /// that has not advanced the caller's pinned manifest cursor therefore
    /// remains represented by its immutable Arrow member, while batches at or
    /// below the cursor are excluded because the persisted provider owns them.
    /// This provider interlock is consumed by Oracle execution; it performs no
    /// remote query and introduces no local-Parquet tier.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the requested range, owner locks, capacity
    /// bounds, checked arithmetic, or Arrow projection is invalid.
    pub(crate) fn readable_batches_for_provider_cut(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        cut: &ProviderCut<'_>,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let batches = self.readable_batches_for_range(
            tenant,
            table,
            cut.start_partition,
            cut.end_partition,
            cut.required_columns,
            cut.limits,
        )?;
        Ok(batches
            .into_iter()
            .filter(|batch| {
                let lsn = batch.meta.wal_lsn_max;
                (cut.persisted_cursor == crate::scribe::wal::WalLsn::ZERO
                    || lsn > cut.persisted_cursor)
                    && !cut
                        .persisted_ranges
                        .iter()
                        .any(|(min, max)| *min <= lsn && lsn <= *max)
            })
            .collect())
    }

    /// Return all readable batches for a table, preserving the old unit-test
    /// convenience while applying the full table scope and schema.
    #[cfg(test)]
    pub fn readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let stats = self.stats()?;
        self.collect_readable_batches(
            tenant,
            table,
            None,
            &[],
            ReadableBatchLimits {
                max_batches: stats.writable_rows.saturating_add(stats.immutable_rows),
                max_retained_bytes: stats.writable_bytes.saturating_add(stats.immutable_bytes),
            },
        )
    }

    /// Mark a prepared generation durable after its staged member is published.
    ///
    /// This is the boundary Scribe normally retires against: the member's runs
    /// are fsynced, checksum-validated, and named by a record that makes them a
    /// query source, so the rows survive without the WAL long before the claim
    /// that publishes them is due. Repeating the operation is idempotent, and a
    /// generation that is already durable keeps its first evidence.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is
    /// poisoned or no generation carries `seal_id`, which would mean a shard
    /// retired a generation the staged boundary never covered.
    pub fn complete_staged(
        &self,
        seal_id: u64,
        member: crate::scribe::assembly::StagedMemberId,
    ) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        for entries in immutable.values_mut() {
            if let Some(entry) = entries.iter_mut().find(|entry| entry.seal_id() == seal_id) {
                entry.record_durable(DurableEvidence::Staged(member));
                return Ok(());
            }
        }
        Err(ScribeError::Internal {
            detail: format!("staged token references unknown seal generation {seal_id}"),
        })
    }

    /// Retire all committed immutable generations at the current sweep.
    ///
    /// A generation is retirement-eligible the moment its state is
    /// [`ImmutableState::Durable`]. `Committed` is only reached after the
    /// fenced `vala.file_list` + audit transaction commits, so retired data is
    /// always readable from published parquet. The returned WAL ranges are the
    /// only ranges eligible for WAL retirement on this sweep.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is
    /// poisoned.
    pub fn sweep_once(&self) -> Result<Vec<(SealKey, WalRange)>, ScribeError> {
        Ok(self
            .sweep_once_with_sizes()?
            .into_iter()
            .map(|(seal_key, wal_range, _arrow_bytes)| (seal_key, wal_range))
            .collect())
    }

    /// Retire all committed immutable generations and return their exact Arrow sizes.
    ///
    /// This shared primitive keeps ordinary WAL retirement and test-only memory
    /// accounting aligned: both consume exactly the generations removed here.
    /// Eligibility is immediate for any generation in [`ImmutableState::Durable`];
    /// [`ImmutableState::PendingCommit`] generations are never retired.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is poisoned.
    fn sweep_once_with_sizes(&self) -> Result<Vec<(SealKey, WalRange, usize)>, ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut retired = Vec::new();
        for (seal_key, entries) in immutable.iter_mut() {
            let mut kept = Vec::with_capacity(entries.len());
            for entry in entries.drain(..) {
                // A committed generation is immediately retirement-eligible:
                // `Committed` is only entered after the fenced file-list/audit
                // transaction commits, so parquet is already readable.
                if matches!(entry.state, ImmutableState::Durable { .. }) {
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

    /// Retire eligible committed generations and return their total Arrow bytes.
    ///
    /// This test-support helper reports only bytes removed from the immutable
    /// map, so callers cannot double-release memory for a still-pending
    /// generation. Eligibility is immediate: any [`ImmutableState::Durable`]
    /// generation is retired on the first call after its commit completes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the immutable-generation lock is poisoned.
    #[cfg(test)]
    pub(crate) fn sweep_once_for_test(&self) -> Result<usize, ScribeError> {
        Ok(self
            .sweep_once_with_sizes()?
            .into_iter()
            .fold(0_usize, |total, (_, _, arrow_bytes)| {
                total.saturating_add(arrow_bytes)
            }))
    }

    /// Inspect one committed generation without removing its ownership.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when the immutable map lock is poisoned.
    pub(crate) fn plan_committed_retirement(
        &self,
        seal_id: u64,
    ) -> Result<Option<CommittedRetirement>, ScribeError> {
        let immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        Ok(immutable.values().flatten().find_map(|entry| {
            if entry.seal_id == seal_id && matches!(entry.state, ImmutableState::Durable { .. }) {
                Some(CommittedRetirement {
                    seal_id,
                    arrow_bytes: entry.frozen.arrow_bytes,
                    wal_range: entry.wal_range(),
                })
            } else {
                None
            }
        }))
    }

    /// Commit a previously planned retirement and remove exactly that entry.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when the token no longer matches a
    /// committed immutable generation or its lock is poisoned.
    pub(crate) fn commit_retirement(
        &self,
        token: CommittedRetirement,
    ) -> Result<WalRange, ScribeError> {
        let mut immutable = self
            .immutable
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable immutable lock poisoned: {error}"),
            })?;
        for entries in immutable.values_mut() {
            let Some(index) = entries.iter().position(|entry| {
                entry.seal_id == token.seal_id
                    && entry.frozen.arrow_bytes == token.arrow_bytes
                    && matches!(entry.state, ImmutableState::Durable { .. })
            }) else {
                continue;
            };
            let range = entries[index].wal_range();
            if range != token.wal_range {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "retirement token WAL range changed for seal {}",
                        token.seal_id
                    ),
                });
            }
            entries.remove(index);
            immutable.retain(|_, values| !values.is_empty());
            drop(immutable);
            // The generation's authority is not released here. Dropping this
            // Arrow copy is not a handover: the staged run is already
            // authoritative for these rows and stays so until the published
            // object replaces it, so the registry entry is released by the
            // publication owner once the member is retired from the volume.
            return Ok(token.wal_range);
        }
        Err(ScribeError::Internal {
            detail: format!("retirement token no longer matches seal {}", token.seal_id),
        })
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
        let mut discarded = None;
        for entries in immutable.values_mut() {
            if let Some(index) = entries
                .iter()
                .position(|entry| entry.seal_id == seal_id && entry.is_pending())
            {
                discarded = Some(entries[index].frozen.seal_key.clone());
                entries.remove(index);
                break;
            }
        }
        immutable.retain(|_, entries| !entries.is_empty());
        drop(immutable);
        if let Some(key) = discarded
            && let Some(hot_sources) = &self.hot_sources
        {
            hot_sources
                .discard(&key, self.ordinal(seal_id))
                .map_err(|error| ScribeError::Internal {
                    detail: format!("forget abandoned generation {seal_id}: {error}"),
                })?;
        }
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

    /// Classify whether this bucket should seal, and why, at `now`.
    ///
    /// Returns [`SealTriggerReason::Size`] when accumulated Arrow bytes have
    /// reached `rotation_bytes`, otherwise [`SealTriggerReason::Age`] when the
    /// active generation has lived at least `seal_max_age`, otherwise `None`.
    /// Size takes precedence so a bucket that is both full and old is labelled
    /// by the primary (size) trigger. The pressure path does not flow through
    /// this predicate; it tags [`SealTriggerReason::Pressure`] directly.
    fn should_seal_at(
        &self,
        now: Instant,
        rotation_bytes: usize,
        seal_max_age: Duration,
    ) -> Option<SealTriggerReason> {
        if self.bytes_accumulated >= rotation_bytes {
            Some(SealTriggerReason::Size)
        } else if self.is_age_expired(now, seal_max_age) {
            Some(SealTriggerReason::Age)
        } else {
            None
        }
    }

    /// Report whether the active generation has reached `seal_max_age` at `now`.
    fn is_age_expired(&self, now: Instant, seal_max_age: Duration) -> bool {
        now.saturating_duration_since(self.first_insert_at) >= seal_max_age
    }

    /// Detaches this bucket as one frozen generation of `shard_id`'s lane.
    ///
    /// The lane is stamped here rather than by the caller because the same
    /// `(shard_id, seal_id)` pair is what the memtable registers in the
    /// hot-source registry. Producing both from one value is what keeps a
    /// published member's ordinal equal to its registered one.
    fn freeze(self, seal_id: u64, shard_id: usize) -> FrozenMemtable {
        let batches = self.batches;

        let opened_at = self.first_insert_at;
        let closed_at = Instant::now();
        let arrow_bytes = self.bytes_accumulated;

        FrozenMemtable {
            seal_id,
            shard_id,
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

    /// Appends this active bucket through the bounded shallow collector.
    ///
    /// # Errors
    ///
    /// Returns the collector capacity or Arrow projection error unchanged.
    fn append_readable_batches(
        &self,
        required_columns: &[String],
        output: &mut ReadableBatchCollector,
    ) -> Result<(), ScribeError> {
        let projection = projection_indices(&self.schema, required_columns)?;
        append_projected_batches(
            &self.batches,
            &self.metas,
            self.seal_key.partition,
            &projection,
            output,
        )
    }
}

/// What makes one immutable generation's rows survive without its WAL.
///
/// Both variants are equally authoritative for retirement; they differ only in
/// who serves the rows. A staged member is Parquet this pod fsynced,
/// checksum-validated, and registered as a query source, and it becomes a
/// published object later when its assembly claim is due. A published key names
/// rows already in the fenced file list.
#[derive(Debug, Clone)]
pub enum DurableEvidence {
    /// Rows are durable and servable as a staged member on this pod's volume.
    Staged(crate::scribe::assembly::StagedMemberId),
}

/// The lifecycle state of one immutable generation.
///
/// The two variants govern retirement eligibility: only [`Durable`] generations
/// are retirement-eligible, and they are eligible immediately — there is no
/// timed grace period. [`PendingCommit`] generations are never retired until
/// their rows survive without the WAL behind them.
///
/// [`Durable`]: ImmutableState::Durable
/// [`PendingCommit`]: ImmutableState::PendingCommit
#[derive(Debug)]
pub enum ImmutableState {
    /// The generation's rows do not yet survive without their WAL.
    ///
    /// A generation in this state is never retirement-eligible, even if the
    /// lifecycle sweep runs. Retirement would release accounting before the
    /// rows are readable from anything else, violating the invariant that
    /// retired data is always accessible without the WAL.
    PendingCommit,
    /// The generation's rows survive and are servable without their WAL.
    ///
    /// A generation in this state is immediately retirement-eligible: the
    /// evidence naming where its rows now live is durable before this variant
    /// is entered. The `observed_at` timestamp records when that was
    /// acknowledged locally, for observability and tracing.
    Durable {
        /// Local time at which the durable boundary was acknowledged.
        observed_at: Instant,
        /// Exact durable identity used for replay reconciliation.
        evidence: DurableEvidence,
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
    /// Records the first durable evidence this generation earned.
    ///
    /// A generation that is already durable keeps the evidence it entered that
    /// state with: overwriting it would move the retirement boundary a shard
    /// may already have acted on, and both evidences authorize retirement
    /// equally, so there is nothing to gain by replacing one with the other.
    fn record_durable(&mut self, evidence: DurableEvidence) {
        match &self.state {
            ImmutableState::PendingCommit => {
                self.state = ImmutableState::Durable {
                    observed_at: Instant::now(),
                    evidence,
                };
            }
            ImmutableState::Durable { .. } => {}
        }
    }

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
    pub partition_day: crate::catalog::layout::TimePartition,
    /// WAL metadata for the append.
    pub meta: ScribeAppendMeta,
    /// Arrow rows for the append.
    pub batch: RecordBatch,
}

/// Immutable count and byte ceilings for one shallow live-tail snapshot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadableBatchLimits {
    /// Maximum number of projected batch descriptors returned to the caller.
    pub(crate) max_batches: usize,
    /// Maximum source-derived Arrow bytes retained by the returned batches.
    pub(crate) max_retained_bytes: usize,
}

/// Manifest-pinned bounds defining one atomic hot-provider query cut.
pub(crate) struct ProviderCut<'a> {
    /// First included event day.
    pub(crate) start_partition: crate::catalog::layout::TimePartition,
    /// Last included event day.
    pub(crate) end_partition: crate::catalog::layout::TimePartition,
    /// Requested projection in caller order.
    pub(crate) required_columns: &'a [String],
    /// Inclusive persisted prefix owned by the pinned manifest.
    pub(crate) persisted_cursor: crate::scribe::wal::WalLsn,
    /// Independently published non-prefix ranges owned by that manifest.
    pub(crate) persisted_ranges: &'a [(crate::scribe::wal::WalLsn, crate::scribe::wal::WalLsn)],
    /// Count and retained-byte bounds for the shallow snapshot.
    pub(crate) limits: ReadableBatchLimits,
}

/// Inclusive WAL range eligible for retirement after a committed sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalRange {
    /// Inclusive lower LSN.
    pub min: crate::scribe::wal::WalLsn,
    /// Inclusive upper LSN.
    pub max: crate::scribe::wal::WalLsn,
}

/// Private no-mutation retirement plan used by the shard owner preflight.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommittedRetirement {
    /// Exact generation identity being retired.
    pub(crate) seal_id: u64,
    /// Arrow bytes that must be released with this generation.
    pub(crate) arrow_bytes: usize,
    /// WAL range released after ownership is removed.
    pub(crate) wal_range: WalRange,
}

/// Frozen memtable snapshot for one seal-key.
///
/// Immutable snapshot detached from the writable bucket. It retains the original
/// append batches so live tail reads preserve WAL/append granularity. A merged
/// batch is materialized only inside the bounded persistence encoder; retaining
/// both forms here would double the Arrow memory charged to Scribe.
///
/// The `shard_id` field carries the pod-local shard lane that froze this
/// generation. It is stamped by the freezing memtable itself, which is the
/// same owner that registers the generation's hot-source ordinal, so
/// post-commit routing and authority lookup can never disagree about the lane.
#[derive(Debug, Clone)]
pub struct FrozenMemtable {
    /// Local immutable-generation identity.
    pub seal_id: u64,
    /// The seal-key this snapshot belongs to.
    pub seal_key: SealKey,
    /// Pod-local shard lane that owns this frozen generation.
    ///
    /// Stamped by the freezing memtable from its bound lane identity, which is
    /// the same value its hot-source ordinals carry. Post-commit routing MUST
    /// use this field rather than recomputing the route from `seal_key`.
    pub shard_id: usize,
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

    /// Appends this immutable generation through the bounded shallow collector.
    ///
    /// # Errors
    ///
    /// Returns the collector capacity or Arrow projection error unchanged.
    fn append_readable_batches(
        &self,
        required_columns: &[String],
        output: &mut ReadableBatchCollector,
    ) -> Result<(), ScribeError> {
        let projection = projection_indices(&self.schema, required_columns)?;
        append_projected_batches(
            &self.batches,
            &self.metas,
            self.seal_key.partition,
            &projection,
            output,
        )
    }
}

/// Appends shallow projected batches only after source-derived capacity checks.
///
/// # Errors
///
/// Returns [`ScribeError::IngestBusy`] before projection when the configured
/// batch or retained-byte ceiling would be exceeded, or an internal error when
/// byte arithmetic or Arrow projection fails.
fn append_projected_batches(
    batches: &[RecordBatch],
    metas: &[ScribeAppendMeta],
    partition_day: crate::catalog::layout::TimePartition,
    projection: &[usize],
    output: &mut ReadableBatchCollector,
) -> Result<(), ScribeError> {
    for (batch, meta) in batches.iter().zip(metas) {
        output.preflight(batch.get_array_memory_size())?;
        let batch = batch
            .project(projection)
            .map_err(|error| ScribeError::Internal {
                detail: format!("live-tail Arrow projection failed: {error}"),
            })?;
        output.push(ReadableBatch {
            partition_day,
            meta: meta.clone(),
            batch,
        });
    }
    Ok(())
}

/// Owns one exact-capacity shallow live-tail snapshot under configured bounds.
struct ReadableBatchCollector {
    /// Maximum number of shallow batches accepted by this snapshot.
    max_batches: usize,
    /// Maximum source-derived Arrow bytes retained by this snapshot.
    max_retained_bytes: usize,
    /// Source-derived bytes accepted so far.
    retained_bytes: usize,
    /// Exact-capacity result backing filled only after each preflight.
    output: Vec<ReadableBatch>,
}

impl ReadableBatchCollector {
    /// Creates an empty collector with its complete batch descriptor capacity.
    fn new(max_batches: usize, max_retained_bytes: usize) -> Self {
        Self {
            max_batches,
            max_retained_bytes,
            retained_bytes: 0,
            output: Vec::with_capacity(max_batches),
        }
    }

    /// Refuses a candidate before Arrow projection when either ceiling is exhausted.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the candidate exceeds a
    /// configured ceiling, or [`ScribeError::Internal`] when byte arithmetic
    /// overflows.
    fn preflight(&mut self, source_bytes: usize) -> Result<(), ScribeError> {
        if self.output.len() == self.max_batches {
            return Err(ScribeError::IngestBusy {
                table: "live-tail snapshot".to_owned(),
            });
        }
        let next_bytes = self
            .retained_bytes
            .checked_add(source_bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "live-tail retained byte count overflow".to_owned(),
            })?;
        if next_bytes > self.max_retained_bytes {
            return Err(ScribeError::IngestBusy {
                table: "live-tail snapshot".to_owned(),
            });
        }
        self.retained_bytes = next_bytes;
        Ok(())
    }

    /// Appends one post-preflight shallow projection without growing capacity.
    fn push(&mut self, batch: ReadableBatch) {
        debug_assert!(self.output.len() < self.output.capacity());
        self.output.push(batch);
    }

    /// Consumes the collector and returns its bounded shallow snapshot.
    fn finish(self) -> Vec<ReadableBatch> {
        self.output
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
    use crate::scribe::wal::WalLsn;
    use arrow::array::{Int32Array, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;

    use std::sync::Arc;
    use std::sync::Barrier;

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
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
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
            crate::test_support::day_partition(2026, 7, 14),
        )
    }

    /// Automatic rotation snapshots every non-empty key in stable order and
    /// leaves the active generation empty for the incoming unit.
    #[test]
    fn automatic_rotation_swaps_all_keys_before_incoming_unit() {
        let memtable = Memtable::new();
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "spans"),
            first.partition,
        );
        memtable
            .insert(
                &first,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("first active member");
        memtable
            .insert(
                &second,
                make_test_event(),
                make_test_meta(2),
                make_test_batch(1),
            )
            .expect("second active member");

        let frozen = memtable
            .freeze_all_nonempty()
            .expect("atomic all-key freeze");
        assert_eq!(frozen.len(), 2);
        assert!(
            frozen
                .windows(2)
                .all(|pair| pair[0].seal_key.to_string() < pair[1].seal_key.to_string())
        );
        assert_eq!(memtable.stats().expect("cohort stats").writable_buckets, 0);
        assert_eq!(
            memtable
                .stats()
                .expect("cohort stats")
                .immutable_generations,
            2
        );
        memtable
            .insert(
                &first,
                make_test_event(),
                make_test_meta(3),
                make_test_batch(1),
            )
            .expect("incoming unit enters fresh generation");
        assert_eq!(memtable.row_count(&first).expect("fresh active rows"), 1);
        assert_eq!(
            memtable.row_count(&second).expect("old key stays frozen"),
            0
        );
    }

    /// Proves a rotation cohort snapshots every non-empty active key exactly once.
    ///
    /// Empty buckets are excluded, while every populated key is returned in the
    /// stable cohort and removed from the active generation before new ingress.
    #[test]
    fn cohort_snapshot_contains_every_nonempty_seal_key() {
        let memtable = Memtable::new();
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "metrics"),
            first.partition,
        );
        let empty = SealKey::new(
            first.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "empty"),
            first.partition,
        );
        for (key, lsn) in [(&first, 1), (&second, 2)] {
            memtable
                .insert(
                    key,
                    make_test_event(),
                    make_test_meta(lsn),
                    make_test_batch(1),
                )
                .expect("populate cohort member");
        }
        memtable.writable.lock().expect("writable lock").insert(
            empty.clone(),
            MemtableBucket::new(empty, make_test_batch(1).schema()),
        );

        let cohort = memtable.freeze_all_nonempty().expect("cohort snapshot");
        let mut actual = cohort
            .iter()
            .map(|member| member.seal_key.to_string())
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = [first.to_string(), second.to_string()];
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(cohort.len(), 2, "each populated key appears once");
        assert_eq!(
            memtable.stats().expect("post-snapshot stats").writable_rows,
            0
        );
    }

    /// A pinned provider cut includes every not-yet-persisted immutable member
    /// plus active rows and excludes exactly the cursor-covered prefix.
    #[test]
    fn provider_cut_filters_cursor_and_nonprefix_ranges() {
        let memtable = Memtable::new();
        let key = make_test_seal_key();
        for lsn in [1_u64, 2] {
            memtable
                .insert(
                    &key,
                    make_test_event(),
                    make_test_meta(lsn),
                    make_test_batch(1),
                )
                .expect("cohort source insert");
        }
        memtable.freeze(&key).expect("immutable cohort member");
        memtable
            .insert(
                &key,
                make_test_event(),
                make_test_meta(3),
                make_test_batch(1),
            )
            .expect("fresh active insert");
        let limits = ReadableBatchLimits {
            max_batches: 3,
            max_retained_bytes: usize::MAX,
        };
        let before_publication = memtable
            .readable_batches_for_provider_cut(
                key.tenant,
                &key.table,
                &ProviderCut {
                    start_partition: key.partition,
                    end_partition: key.partition,
                    required_columns: &[],
                    persisted_cursor: WalLsn::ZERO,
                    persisted_ranges: &[],
                    limits,
                },
            )
            .expect("pre-publication cut");
        assert_eq!(
            before_publication
                .iter()
                .map(|batch| batch.meta.wal_lsn_max.as_u64())
                .collect::<std::collections::BTreeSet<_>>(),
            [1_u64, 2, 3].into_iter().collect()
        );
        let after_partial_publication = memtable
            .readable_batches_for_provider_cut(
                key.tenant,
                &key.table,
                &ProviderCut {
                    start_partition: key.partition,
                    end_partition: key.partition,
                    required_columns: &[],
                    persisted_cursor: WalLsn::ZERO,
                    persisted_ranges: &[(WalLsn::new(2), WalLsn::new(2))],
                    limits,
                },
            )
            .expect("post-publication cut");
        assert_eq!(after_partial_publication.len(), 2);
        assert_eq!(
            after_partial_publication
                .iter()
                .map(|batch| batch.meta.wal_lsn_max.as_u64())
                .collect::<std::collections::BTreeSet<_>>(),
            [1_u64, 3].into_iter().collect()
        );
    }

    /// Builds one staged-member identity standing for a published shard run.
    fn staged_member(generation: u64) -> crate::scribe::assembly::StagedMemberId {
        crate::scribe::assembly::StagedMemberId::new(0, generation)
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
        let payload_len = u32::try_from(bytes.len()).expect("test payload length");
        let seal_key = make_test_seal_key();
        let batch_id = uuid::Uuid::now_v7();
        ReplayedSealKey {
            stream: crate::scribe::stream_identity::StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::generate(),
                crate::scribe::stream_identity::WriterEpoch::new(1),
            ),
            seal_key: seal_key.clone(),
            shard_id: 0,
            audit_events: vec![make_test_event()],
            data_records: vec![bytes],
            append_metas: vec![ReplayedAppendMeta {
                batch_id: *batch_id.as_bytes(),
                payload_digest: [0; 32],
                payload_len,
                slice_count: 1,
                wal_lsn: WalLsn::new(1),
                rows_accepted,
                append_slice_id: AppendSliceId {
                    batch_id,
                    seal_key,
                    slice_index: 0,
                },
                schema_fingerprint: [0; 32],
            }],
            wal_segments: Vec::new(),
            commits: Vec::new(),
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
        let identity = crate::scribe::wal::ScribeAppendPayloadIdentity {
            batch_id,
            schema_fingerprint: meta.schema_fingerprint,
            data_digest: meta.data_digest,
            data_len: meta.data_len,
            slice_index: meta.slice_index,
            slice_count: meta.slice_count,
        };
        memtable
            .insert(&seal_key, make_test_event(), meta, make_test_batch(2))
            .expect("insert");
        assert_eq!(
            memtable
                .retained_batch_rows(&seal_key, identity,)
                .expect("active lookup"),
            Some(100)
        );
        memtable.freeze(&seal_key).expect("freeze");
        assert_eq!(
            memtable
                .retained_batch_rows(&seal_key, identity,)
                .expect("immutable lookup"),
            Some(100)
        );
        assert_eq!(
            memtable
                .retained_batch_rows(
                    &seal_key,
                    crate::scribe::wal::ScribeAppendPayloadIdentity {
                        batch_id: [8u8; 16],
                        schema_fingerprint: [0; 32],
                        data_digest: [0; 32],
                        data_len: 0,
                        slice_index: 0,
                        slice_count: 1,
                    },
                )
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
            .complete_staged(first.seal_id, staged_member(10))
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

    /// Live-tail snapshots refuse count and byte overflow before projection growth.
    #[test]
    fn live_tail_snapshot_enforces_preallocated_count_and_byte_bounds() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        for lsn in [10, 20] {
            memtable
                .insert(
                    &seal_key,
                    make_test_event(),
                    make_test_meta(lsn),
                    make_test_batch(1),
                )
                .expect("bounded fixture insert");
        }
        let tenant = crate::test_support::tenant();
        let rows = memtable
            .readable_batches_for_range(
                tenant,
                &seal_key.table,
                seal_key.partition,
                seal_key.partition,
                &[],
                ReadableBatchLimits {
                    max_batches: 2,
                    max_retained_bytes: usize::MAX,
                },
            )
            .expect("exact batch ceiling");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.capacity(), 2);

        assert!(matches!(
            memtable.readable_batches_for_range(
                tenant,
                &seal_key.table,
                seal_key.partition,
                seal_key.partition,
                &[],
                ReadableBatchLimits {
                    max_batches: 1,
                    max_retained_bytes: usize::MAX,
                },
            ),
            Err(ScribeError::IngestBusy { .. })
        ));
        let first_bytes = rows[0].batch.get_array_memory_size();
        assert!(matches!(
            memtable.readable_batches_for_range(
                tenant,
                &seal_key.table,
                seal_key.partition,
                seal_key.partition,
                &[],
                ReadableBatchLimits {
                    max_batches: 2,
                    max_retained_bytes: first_bytes.saturating_sub(1),
                },
            ),
            Err(ScribeError::IngestBusy { .. })
        ));
    }

    /// Verifies that a committed generation retires on the first sweep after commit,
    /// and that its empty bucket is also removed.
    ///
    /// Retirement is immediate: `Committed` implies durably published parquet,
    /// so no grace period is required before the accounting releases.
    #[test]
    fn committed_generation_retires_at_first_sweep() {
        let memtable = Memtable::new();
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
            .complete_staged(frozen.seal_id, staged_member(10))
            .expect("complete");

        assert_eq!(
            memtable.sweep_once().expect("first sweep").len(),
            1,
            "committed generation retires at the first sweep"
        );
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 0);
        assert_eq!(
            memtable.stats().expect("retired stats").immutable_buckets,
            0
        );
    }

    /// Verifies that a frozen generation in `PendingCommit` state never retires,
    /// even when a sweep runs. Retirement is gated on `Committed`, which requires
    /// the SQL transaction to have committed durably.
    #[test]
    fn pending_generations_never_retire() {
        let memtable = Memtable::new();
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
            memtable.sweep_once().expect("sweep").is_empty(),
            "PendingCommit generation must not retire before SQL commit"
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
        let memtable = Memtable::new_with_rotation(incoming_bytes + 1);
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
            crate::test_support::day_partition(2026, 7, 16),
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

    /// Expired bucket discovery feeds the complete non-empty memtable freeze primitive.
    ///
    /// # Panics
    ///
    /// Panics if age discovery misses the expired key or the complete freeze
    /// omits a younger peer. Owner WAL and clock behavior is proved in `shards`.
    #[test]
    fn expired_bucket_discovery_feeds_complete_nonempty_freeze() {
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
        let second = SealKey::new(
            seal_key.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "age_peer"),
            seal_key.partition,
        );
        memtable
            .insert(
                &second,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("younger peer");
        let frozen = memtable
            .freeze_all_nonempty()
            .expect("whole-shard age swap");
        assert_eq!(frozen.len(), 2, "age rotation includes the younger peer");
        assert_eq!(
            memtable.stats().expect("post-age stats").writable_buckets,
            0
        );
    }

    /// Test-only retirement reports only bytes from generations it removed.
    ///
    /// A `PendingCommit` generation is ineligible and is excluded from the sweep.
    /// Only the `Committed` generation's bytes are returned and released.
    #[test]
    fn retirement_bytes_exclude_pending_commit_generations() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // First generation: committed and immediately retirement-eligible.
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
            .complete_staged(first.seal_id, staged_member(10))
            .expect("first complete");
        let first_bytes = memtable.stats().expect("first stats").immutable_bytes;

        // Second generation: still in PendingCommit — must not be retired.
        memtable
            .insert(
                &seal_key,
                make_test_event(),
                make_test_meta(20),
                make_test_batch(1),
            )
            .expect("second insert");
        memtable.freeze(&seal_key).expect("second freeze");
        let total_bytes = memtable.stats().expect("combined stats").immutable_bytes;

        let retired_bytes = memtable.sweep_once_for_test().expect("test retirement");
        assert_eq!(
            retired_bytes, first_bytes,
            "only the committed generation retires"
        );
        assert!(
            retired_bytes < total_bytes,
            "pending generation bytes excluded"
        );
        assert_eq!(
            memtable.stats().expect("remaining stats").immutable_bytes,
            total_bytes.saturating_sub(retired_bytes)
        );
        assert_eq!(
            memtable.immutable_generation_count().expect("remaining"),
            1,
            "pending generation remains"
        );
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
            crate::test_support::day_partition(2026, 7, 15),
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

    /// Victim selection depends only on the aggregated candidate set, never on
    /// how a seal-key's buckets were partitioned across shards.
    ///
    /// This pins the D83 shard-count invariance: T35 batch-spread routing
    /// scatters one seal-key's buckets over 16 shards, and the pressure path
    /// flattens every shard's `pressure_candidates` before selecting. Whether
    /// the same three buckets arrive as one list or split across four shard
    /// sub-lists (one empty), the flattened selection is identical.
    #[test]
    fn pressure_selection_is_shard_count_invariant() {
        let base = make_test_seal_key();
        let candidate = |day: u32, bytes: usize| PressureCandidate {
            seal_key: SealKey::new(
                base.tenant,
                base.table.clone(),
                crate::test_support::day_partition(2026, 7, day),
            ),
            writable_bytes: bytes,
            first_insert_at: Instant::now(),
            oldest_wal_lsn: None,
        };
        let a = candidate(10, 300);
        let b = candidate(11, 200);
        let c = candidate(12, 100);
        let to_release = 450;

        let one_shard = vec![a.clone(), b.clone(), c.clone()];
        let four_shards: Vec<PressureCandidate> = vec![
            vec![a.clone()],
            Vec::new(),
            vec![b.clone(), c.clone()],
            Vec::new(),
        ]
        .into_iter()
        .flatten()
        .collect();

        assert_eq!(
            Memtable::select_pressure_victims(one_shard, to_release),
            Memtable::select_pressure_victims(four_shards, to_release),
        );
    }

    /// The configured `seal_max_age` threads through `should_seal` so the age
    /// trigger fires on the runtime value rather than the 10-minute constant.
    ///
    /// A zero max-age makes the active generation immediately age-expired, so a
    /// freshly inserted (sub-rotation) bucket seals at once — proving the value
    /// passed to [`Memtable::new_with_config`] reaches the age predicate.
    #[test]
    fn configured_seal_max_age_threads_into_age_trigger() {
        let memtable = Memtable::new_with_config(1_000_000, Duration::from_secs(0));
        let key = make_test_seal_key();
        memtable
            .insert(
                &key,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("insert");
        assert!(memtable.should_seal(&key).expect("should_seal"));
    }

    /// Every seal cause and whole-generation rotation preserve a nonzero lane.
    ///
    /// # Panics
    ///
    /// Panics when size, age, pressure, or full rotation rewrites the bound
    /// shard identity or registers a different hot-source ordinal.
    #[test]
    fn nonzero_shard_survives_size_age_pressure_and_full_rotation() {
        const SHARD: usize = 7;
        let exercise = |rotation_bytes: usize, max_age: Duration| {
            let registry = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
            let memtable = Memtable::new_with_config(rotation_bytes, max_age)
                .with_hot_sources(SHARD, Arc::clone(&registry));
            (registry, memtable)
        };
        let key = make_test_seal_key();

        let (size_registry, size) = exercise(1, Duration::from_secs(60));
        size.insert(
            &key,
            make_test_event(),
            make_test_meta(1),
            make_test_batch(1),
        )
        .expect("size member inserts");
        assert_eq!(size.should_seal(&key).expect("size predicate"), true);
        let size_frozen = size.freeze(&key).expect("size member freezes");
        assert_eq!(size_frozen.shard_id, SHARD);
        assert!(
            size_registry
                .authority(
                    &key,
                    crate::scribe::hot_source::GenerationOrdinal::new(
                        u16::try_from(SHARD).expect("fixture shard fits"),
                        size_frozen.seal_id,
                    ),
                )
                .expect("size authority")
                .is_some()
        );

        let (_, age) = exercise(usize::MAX, Duration::ZERO);
        age.insert(
            &key,
            make_test_event(),
            make_test_meta(2),
            make_test_batch(1),
        )
        .expect("age member inserts");
        assert!(age.should_seal(&key).expect("age predicate"));
        assert_eq!(
            age.freeze(&key).expect("age member freezes").shard_id,
            SHARD
        );

        let (_, pressure) = exercise(usize::MAX, Duration::from_secs(60));
        pressure
            .insert(
                &key,
                make_test_event(),
                make_test_meta(3),
                make_test_batch(1),
            )
            .expect("pressure member inserts");
        let victim = Memtable::select_pressure_victims(
            pressure.pressure_candidates().expect("pressure candidates"),
            1,
        )
        .pop()
        .expect("one pressure victim");
        assert_eq!(
            pressure
                .freeze(&victim)
                .expect("pressure member freezes")
                .shard_id,
            SHARD
        );

        let (_, rotation) = exercise(usize::MAX, Duration::from_secs(60));
        let peer = SealKey::new(
            key.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "nonzero-peer"),
            key.partition,
        );
        for (candidate, lsn) in [(&key, 4), (&peer, 5)] {
            rotation
                .insert(
                    candidate,
                    make_test_event(),
                    make_test_meta(lsn),
                    make_test_batch(1),
                )
                .expect("rotation member inserts");
        }
        let cohort = rotation.freeze_all_nonempty().expect("full rotation");
        assert_eq!(cohort.len(), 2);
        assert!(cohort.iter().all(|member| member.shard_id == SHARD));
    }

    /// The age trigger fires exactly at `seal_max_age`, and size takes priority.
    ///
    /// Below `rotation_bytes` only the age trigger can fire: at `29s` the bucket
    /// is not yet expired, at the configured `30s` it seals as
    /// [`SealTriggerReason::Age`].
    #[test]
    fn age_trigger_seals_at_configured_thirty_seconds() {
        let seal_max_age = Duration::from_secs(30);
        let memtable = Memtable::new_with_config(1_000_000, seal_max_age);
        let key = make_test_seal_key();
        memtable
            .insert(
                &key,
                make_test_event(),
                make_test_meta(1),
                make_test_batch(1),
            )
            .expect("insert");
        let buckets = memtable.writable.lock().expect("writable lock");
        let bucket = buckets.get(&key).expect("bucket");
        let opened = bucket.first_insert_at;
        assert_eq!(
            bucket.should_seal_at(opened + Duration::from_secs(29), 1_000_000, seal_max_age),
            None,
        );
        assert_eq!(
            bucket.should_seal_at(opened + seal_max_age, 1_000_000, seal_max_age),
            Some(SealTriggerReason::Age),
        );
    }

    /// Pressure victim selection accumulates keys until the low-water target is met.
    ///
    /// # Panics
    ///
    /// Panics if deterministic victim ordering cannot select enough writable
    /// bytes. Owner clock and WAL preservation is proved in `shards`.
    #[test]
    fn pressure_victim_selection_reaches_low_water_target() {
        let first = make_test_seal_key();
        let second = SealKey::new(
            first.tenant,
            first.table.clone(),
            crate::test_support::day_partition(2026, 7, 15),
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
            crate::test_support::day_partition(2026, 7, 15),
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
            crate::test_support::day_partition(2026, 7, 14),
        );
        let key2 = SealKey::new(
            tenant,
            table,
            crate::test_support::day_partition(2026, 7, 15),
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
