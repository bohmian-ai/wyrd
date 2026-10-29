//! In-memory row buffer keyed by seal-key with seal predicate.
//!
//! The memtable holds Arrow buffers + paired append metadata per
//! `SealKey = (DataTenantId, TableRef, TimePartition)`. Rotation is driven by
//! retained Arrow size, active-generation age, and global pressure. Automatic
//! writer rotation swaps every non-empty bucket as one shard cohort; selective
//! pressure and explicit seals continue to freeze individual keys.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use arrow::array::RecordBatch;
use arrow::datatypes::{FieldRef, Schema, SchemaRef};
use datafusion::common::stats::Precision;
use datafusion::common::{ColumnStatistics, Statistics};
use datafusion::functions_aggregate::min_max::{MaxAccumulator, MinAccumulator};
use datafusion::logical_expr::Accumulator as _;
use wyrd_spec::DataTenantId;

use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::ScribeAppendMeta;
use crate::tables::fields::PARQUET_FIELD_ID;

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
/// Each seal-key holds an Arrow buffer and its paired `ScribeAppendMeta`
/// list. Freezing a seal-key detaches an immutable snapshot.
///
/// Retirement is immediate: a generation is eligible the moment it enters
/// [`ImmutableState::Durable`], which is only reached after the fenced
/// `vala.file_list` transaction commits. Retired Arrow memory is
/// released before WAL segment retirement is submitted, preserving the
/// ordering contract in `ShardOwner::retire_committed`.
#[derive(Debug)]
pub struct Memtable {
    pub(crate) writable: Arc<RwLock<HashMap<SealKey, MemtableBucket>>>,
    pub(crate) immutable: Arc<RwLock<HashMap<SealKey, Vec<ImmutableEntry>>>>,
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

    /// Allocates the next generation number this lane may freeze under `key`.
    ///
    /// The counter is pod-local and starts at one, so on its own it would
    /// re-issue ordinals that startup recovery restored for members an earlier
    /// process froze. Every allocation therefore takes the counter forward past
    /// whatever the authority registry already owns for this lane and key,
    /// which keeps generations unique and in age order across a restart.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the authority registry is
    /// poisoned, because an unreadable registry cannot prove the ordinal is
    /// free.
    fn allocate_seal_id(&self, key: &SealKey) -> Result<u64, ScribeError> {
        let candidate = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(candidate);
        };
        let highest = hot_sources
            .highest_generation(key, u16::try_from(self.shard_id).unwrap_or(u16::MAX))
            .map_err(|error| ScribeError::Internal {
                detail: format!("read the highest registered generation for {key}: {error}"),
            })?;
        if candidate > highest {
            return Ok(candidate);
        }
        self.next_seal_id.fetch_max(highest + 1, Ordering::Relaxed);
        let reserved = self.next_seal_id.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            operation = "scribe_generation_reserved",
            shard = self.shard_id,
            seal_key = %key,
            restored_high_water = highest,
            allocated = reserved,
            "lane resumed generation numbering above the authorities recovery restored"
        );
        Ok(reserved)
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
            writable: Arc::new(RwLock::new(HashMap::new())),
            immutable: Arc::new(RwLock::new(HashMap::new())),
            next_seal_id: Arc::new(AtomicU64::new(1)),
            rotation_bytes,
            seal_max_age,
            shard_id: 0,
            hot_sources: None,
        }
    }

    /// Insert an append (metadata, Arrow batch) into the memtable.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned or if
    /// Arrow batch merging fails.
    pub fn insert(
        &self,
        seal_key: &SealKey,
        meta: ScribeAppendMeta,
        batch: RecordBatch,
    ) -> Result<(), ScribeError> {
        self.insert_with_layout(seal_key, meta, batch, None)
    }

    /// Inserts an append whose rows are already in `layout`, or canonical when `None`.
    ///
    /// A new generation pins `layout` for its whole life and keeps the
    /// layout's logical schema as its table schema, while its batches hold the
    /// physical rows. Every later append to that generation must carry the
    /// same layout; the shard chose it from [`Self::generation_layout`] before
    /// converting, so a difference is an ownership bug and is refused.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned or the
    /// append's layout is not the generation's pinned layout.
    pub(crate) fn insert_with_layout(
        &self,
        seal_key: &SealKey,
        meta: ScribeAppendMeta,
        batch: RecordBatch,
        layout: Option<Arc<crate::scribe::layout_cache::LearnedLayout>>,
    ) -> Result<(), ScribeError> {
        let mut buckets = self.writable.write().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let bucket = buckets.entry(seal_key.clone()).or_insert_with(|| {
            let schema = layout
                .as_ref()
                .map_or_else(|| batch.schema(), |layout| Arc::clone(layout.logical()));
            MemtableBucket::new(seal_key.clone(), schema).with_layout(layout.clone())
        });
        let same_layout = match (&bucket.layout, &layout) {
            (None, None) => true,
            (Some(pinned), Some(layout)) => Arc::ptr_eq(pinned, layout),
            _ => false,
        };
        if !same_layout {
            return Err(ScribeError::Internal {
                detail: format!("append to {seal_key} is not in its generation's pinned layout"),
            });
        }

        bucket.append(meta, batch);
        Ok(())
    }

    /// Returns the Variant layout rows of `seal_key` must take; `None` is canonical.
    ///
    /// An open generation keeps the layout it pinned. With none open, the
    /// generation the rows will open takes `fresh()`, which is called with
    /// the bucket lock released.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    pub(crate) fn generation_layout(
        &self,
        seal_key: &SealKey,
        fresh: impl FnOnce() -> Option<Arc<crate::scribe::layout_cache::LearnedLayout>>,
    ) -> Result<Option<Arc<crate::scribe::layout_cache::LearnedLayout>>, ScribeError> {
        let pinned = self
            .writable
            .read()
            .map_err(|e| ScribeError::Internal {
                detail: format!("memtable bucket lock poisoned: {e}"),
            })?
            .get(seal_key)
            .map(|bucket| bucket.layout.clone());
        Ok(pinned.unwrap_or_else(fresh))
    }

    /// Reports, per writable bucket of `tenant`'s `table`, whether it pinned a learned layout.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the bucket lock is poisoned.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn writable_layouts_for_test(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<bool>, ScribeError> {
        let buckets = self.writable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets
            .iter()
            .filter(|(key, _)| key.tenant == tenant && &key.table == table)
            .map(|(_, bucket)| bucket.layout.is_some())
            .collect())
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
            .read()
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
            .read()
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
        frozen.seal_id = self.allocate_seal_id(&frozen.seal_key)?;
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

        let mut batches = Vec::with_capacity(replayed.data_records.len());
        let mut statistics = Vec::with_capacity(replayed.data_records.len());
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
            let (data_digest, data_len) = crate::scribe::preprocess::logical_data_identity(&batch)?;
            statistics.push(batch_statistics(&batch));
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
                seal_key: replayed.seal_key.canonical_key(),
            });
        }

        let schema =
            batches
                .first()
                .map(RecordBatch::schema)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "replayed seal range contained no data".to_owned(),
                })?;
        let arrow_bytes = batches
            .iter()
            .map(crate::scribe::memory::retained_arrow_bytes)
            .sum();
        Ok(FrozenMemtable {
            seal_id: 0,
            // Both identities are stamped by the memtable that adopts this
            // decoded generation, in `insert_replayed_frozen`.
            shard_id: 0,
            seal_key: replayed.seal_key.clone(),
            schema,
            batches: batches.into(),
            statistics: statistics.into(),
            metas,
            opened_at: Instant::now(),
            closed_at: Instant::now(),
            arrow_bytes,
            charge: Arc::default(),
        })
    }

    /// Insert a decoded replay generation into this owner-local memtable.
    pub(crate) fn insert_replayed_frozen(
        &self,
        mut frozen: FrozenMemtable,
    ) -> Result<FrozenMemtable, ScribeError> {
        if frozen.seal_id == 0 {
            frozen.seal_id = self.allocate_seal_id(&frozen.seal_key)?;
        }
        frozen.shard_id = self.shard_id;
        let mut immutable = self.immutable.write().map_err(|e| ScribeError::Internal {
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
        let buckets = self.writable.read().map_err(|e| ScribeError::Internal {
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
            .read()
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
        let mut buckets = self.writable.write().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;

        let Some(bucket) = buckets.remove(seal_key) else {
            drop(buckets);
            let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
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

        let seal_id = self.allocate_seal_id(seal_key)?;
        let frozen = bucket.freeze(seal_id, self.shard_id);

        let mut immutable = self.immutable.write().map_err(|e| ScribeError::Internal {
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
            .write()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        let mut immutable = self
            .immutable
            .write()
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
            let seal_id = self.allocate_seal_id(&key)?;
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
        let buckets = self.writable.read().map_err(|e| ScribeError::Internal {
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
            .read()
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
        let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok(immutable
            .get(seal_key)
            .is_some_and(|entries| entries.iter().any(ImmutableEntry::is_pending)))
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
        let buckets = self.writable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        Ok(buckets.keys().cloned().collect())
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
            .read()
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
            .read()
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

    /// Captures one atomic active-plus-immutable live-read cut for one exact
    /// partition range.
    ///
    /// Both maps stay locked while the cut is selected, so the answer is one
    /// consistent moment rather than two, and live reads may call this from any
    /// task without the owning shard. Tenant, table, and time partition are
    /// compared under the locks and only the requested columns are projected,
    /// so a cut never exposes an unrelated bucket or an unrequested column.
    ///
    /// Publication is not decided here and is not inferred from WAL positions.
    /// A generation already marked [`ImmutableState::Durable`] is skipped and
    /// every other immutable batch is tagged with its generation. A member's
    /// runs become readable before the shard marks its generation durable, so
    /// the live reader resolves staged runs after this cut and skips the
    /// generations it tagged. WAL records are numbered from one node-global
    /// counter while generations are sealed per tenant, table, partition, and
    /// shard, so a published member's bounds routinely enclose positions a live
    /// generation owns; treating that containment as ownership would drop
    /// acknowledged rows no object ever carried.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the requested range is inverted,
    /// or when locks, column selection, or Arrow projection fail.
    #[cfg(test)]
    pub(crate) fn readable_batches_for_range(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        start_partition: crate::catalog::layout::TimePartition,
        end_partition: crate::catalog::layout::TimePartition,
        required: &[FieldRef],
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
            required,
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
    /// The locks are held only while each selected bucket and generation is
    /// captured as reference-counted batches and bounds; column selection and
    /// projection run after both are released, so concurrent reads share the
    /// read locks briefly and an insert never waits behind a projection.
    ///
    /// # Errors
    ///
    /// Returns an internal error when locks, column selection, or Arrow
    /// projection fail.
    #[cfg(test)]
    fn collect_readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        partitions: Option<&std::ops::RangeInclusive<crate::catalog::layout::TimePartition>>,
        required: &[FieldRef],
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        let mut batches = Vec::new();
        for source in self.readable_sources(tenant, table, partitions)? {
            source.project(
                required,
                None,
                None,
                &mut batches,
                &mut MemoryScan::default(),
            )?;
        }
        Ok(batches)
    }

    /// Captures every bucket and generation a live read serves for one tenant
    /// table, optionally restricted to one partition range.
    ///
    /// This is the one live-selection rule: every writable bucket, and every
    /// immutable generation not yet [`ImmutableState::Durable`]. Both maps
    /// stay locked only while the selected sources are captured by reference.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] if either memtable lock is poisoned.
    #[cfg(test)]
    fn readable_sources(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        partitions: Option<&std::ops::RangeInclusive<crate::catalog::layout::TimePartition>>,
    ) -> Result<Vec<ReadableSource>, ScribeError> {
        let guards = self.read_guards()?;
        let mut sources = Vec::new();
        self.select_sources(&guards, tenant, table, partitions, &mut sources);
        drop(guards);
        Ok(sources)
    }

    /// Read-locks both maps, writable first, the order freeze locks them in.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] if either memtable lock is poisoned.
    fn read_guards(&self) -> Result<MemtableReadGuards<'_>, ScribeError> {
        let writable = self.writable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable bucket lock poisoned: {e}"),
        })?;
        let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok((writable, immutable))
    }

    /// Appends the live-selection rule's sources under `guards` to `sources`.
    ///
    /// Every writable bucket and every immutable generation not yet
    /// [`ImmutableState::Durable`] is captured by reference.
    fn select_sources(
        &self,
        (writable, immutable): &MemtableReadGuards<'_>,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        partitions: Option<&std::ops::RangeInclusive<crate::catalog::layout::TimePartition>>,
        sources: &mut Vec<ReadableSource>,
    ) {
        let selects = |seal_key: &SealKey| {
            seal_key.tenant == tenant
                && seal_key.table == *table
                && partitions.is_none_or(|range| range.contains(&seal_key.partition.time()))
        };
        for (seal_key, bucket) in writable.iter() {
            if selects(seal_key) {
                sources.push(bucket.readable_source());
            }
        }
        for (seal_key, entries) in immutable.iter() {
            if selects(seal_key) {
                for entry in entries {
                    // A durable generation's rows are already served by the
                    // staged member the registry advanced it onto, and the
                    // staging owner registers that authority before it marks
                    // the generation durable here. Reading the retained Arrow
                    // copy as well would return every one of those rows twice
                    // until the retirement sweep happens to drop it.
                    if matches!(entry.state, ImmutableState::Durable { .. }) {
                        continue;
                    }
                    sources.push(entry.readable_source(self.ordinal(entry.seal_id)));
                }
            }
        }
    }

    /// Return all readable batches for a table, preserving the old unit-test
    /// convenience while applying the full table scope and schema.
    #[cfg(test)]
    pub fn readable_batches(
        &self,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<Vec<ReadableBatch>, ScribeError> {
        self.collect_readable_batches(tenant, table, None, &[])
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
        let mut immutable = self.immutable.write().map_err(|e| ScribeError::Internal {
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
    /// [`ImmutableState::Durable`], which is reached once its rows are a staged
    /// member on the staging volume, so retired data is always readable from
    /// that member or, after publication, from published Parquet. The
    /// returned WAL ranges are the
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
        let mut immutable = self.immutable.write().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        let mut retired = Vec::new();
        for (seal_key, entries) in immutable.iter_mut() {
            let mut kept = Vec::with_capacity(entries.len());
            for entry in entries.drain(..) {
                // A committed generation is immediately retirement-eligible:
                // `Committed` is only entered after the fenced file-list
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
            .read()
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
    /// `retained` is the generation's exact root-backed bytes, already detached
    /// from the shard ledger. It moves into the generation's
    /// [`crate::scribe::memory::RetainedArrowCharge`], so the bytes are returned
    /// only when the last live capture or opened read of the generation's
    /// batches drops, or here, outside the lock, when nothing else holds them.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when the token no longer matches a
    /// committed immutable generation, its charge already holds a retired
    /// lease, or its lock is poisoned. `retained` is dropped, returning its
    /// bytes, on every error.
    pub(crate) fn commit_retirement(
        &self,
        token: CommittedRetirement,
        retained: crate::resources::ScribeMemoryLease,
    ) -> Result<WalRange, ScribeError> {
        let mut immutable = self
            .immutable
            .write()
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
            let retired = entries.remove(index);
            retired.frozen.charge.hold(retained)?;
            immutable.retain(|_, values| !values.is_empty());
            drop(immutable);
            drop(retired);
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
        let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable immutable lock poisoned: {e}"),
        })?;
        Ok(immutable.values().map(Vec::len).sum())
    }

    /// Number of immutable generations still pending post-commit.
    pub fn pending_generation_count(&self) -> Result<usize, ScribeError> {
        let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
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
            .write()
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
        let writable = self.writable.read().map_err(|e| ScribeError::Internal {
            detail: format!("memtable writable lock poisoned: {e}"),
        })?;
        let writable_rows = writable.values().map(|bucket| bucket.row_count).sum();
        let writable_bytes = writable
            .values()
            .map(|bucket| bucket.bytes_accumulated)
            .sum();
        let writable_buckets = writable.len();
        drop(writable);

        let immutable = self.immutable.read().map_err(|e| ScribeError::Internal {
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
            .read()
            .map_err(|error| ScribeError::Internal {
                detail: format!("memtable writable lock poisoned: {error}"),
            })?;
        let immutable = self
            .immutable
            .read()
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

/// Per-seal-key bucket holding Arrow buffers + metadata.
#[derive(Debug)]
pub(crate) struct MemtableBucket {
    seal_key: SealKey,
    /// Logical table schema of the generation's rows.
    schema: SchemaRef,
    /// Variant layout the generation pinned when it opened; `None` is canonical.
    ///
    /// Every batch is in this layout's physical schema, which may differ from
    /// `schema` only in shredded Variant storage.
    layout: Option<Arc<crate::scribe::layout_cache::LearnedLayout>>,
    /// Sealed runs of [`APPEND_RUN_LEN`] appends each, oldest first, shared
    /// by reference with every live capture.
    runs: Vec<AppendRun>,
    /// The appends after the last sealed run, copied by a live capture.
    tail: AppendTail,
    metas: Vec<ScribeAppendMeta>,
    row_count: usize,
    bytes_accumulated: usize,
    first_insert_at: Instant,
    last_insert_at: Instant,
    /// Accounting owner shared by every holder of this bucket's batches,
    /// carried into its frozen generation.
    charge: Arc<crate::scribe::memory::RetainedArrowCharge>,
}

impl MemtableBucket {
    fn new(seal_key: SealKey, schema: SchemaRef) -> Self {
        let now = Instant::now();
        Self {
            seal_key,
            schema,
            layout: None,
            runs: Vec::new(),
            tail: AppendTail::default(),
            metas: Vec::new(),
            row_count: 0,
            bytes_accumulated: 0,
            first_insert_at: now,
            last_insert_at: now,
            charge: Arc::default(),
        }
    }

    /// Pins the Variant layout this generation's rows are retained in.
    fn with_layout(
        mut self,
        layout: Option<Arc<crate::scribe::layout_cache::LearnedLayout>>,
    ) -> Self {
        self.layout = layout;
        self
    }

    /// Retains one append batch with its metadata and exact column bounds.
    ///
    /// The bounds are computed here, once per batch, so a live read prunes
    /// the batch without scanning its rows. The tail seals into a shared run
    /// once it holds [`APPEND_RUN_LEN`] appends.
    fn append(&mut self, meta: ScribeAppendMeta, batch: RecordBatch) {
        self.row_count += batch.num_rows();
        self.bytes_accumulated += estimate_batch_bytes(&batch);
        self.tail.statistics.push(batch_statistics(&batch));
        self.tail.batches.push(batch);
        self.tail.wal_lsns.push(meta.wal_lsn_max);
        self.metas.push(meta);
        if self.tail.batches.len() == APPEND_RUN_LEN {
            self.runs.push(self.tail.seal());
        }
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
    ///
    /// The generation's batches and bounds are concatenated from the sealed
    /// runs and the tail once here, so the generation is shared by reference
    /// from then on.
    fn freeze(mut self, seal_id: u64, shard_id: usize) -> FrozenMemtable {
        self.runs.push(self.tail.seal());
        let batches = self
            .runs
            .iter()
            .flat_map(|run| run.batches.iter().cloned())
            .collect();
        let statistics = self
            .runs
            .iter()
            .flat_map(|run| run.statistics.iter().cloned())
            .collect();

        let opened_at = self.first_insert_at;
        let closed_at = Instant::now();
        let arrow_bytes = self.bytes_accumulated;

        FrozenMemtable {
            seal_id,
            shard_id,
            seal_key: self.seal_key,
            schema: self.schema,
            batches,
            statistics,
            metas: self.metas,
            opened_at,
            closed_at,
            arrow_bytes,
            charge: self.charge,
        }
    }

    /// Captures this active bucket's batches for projection outside the lock.
    ///
    /// The sealed runs are shared by reference; only the tail's fewer than
    /// [`APPEND_RUN_LEN`] appends are copied.
    fn readable_source(&self) -> ReadableSource {
        let mut runs = Vec::with_capacity(self.runs.len() + 1);
        runs.extend(self.runs.iter().cloned());
        if !self.tail.batches.is_empty() {
            runs.push(self.tail.run());
        }
        ReadableSource::capture(
            &self.schema,
            runs,
            self.seal_key.clone(),
            None,
            &self.charge,
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
    /// The frozen generation's appends as one run, built once so every live
    /// capture shares it and its alignment is checked at insertion.
    run: AppendRun,
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

    /// Tracks `frozen` as a generation awaiting its durable commit.
    ///
    /// # Panics
    ///
    /// Panics when `frozen` does not carry one append meta per batch (see
    /// [`AppendRun::new`]).
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
            run: AppendRun::new(
                Arc::clone(&frozen.batches),
                Arc::clone(&frozen.statistics),
                frozen.metas.iter().map(|meta| meta.wal_lsn_max).collect(),
            ),
            frozen,
            state: ImmutableState::PendingCommit,
        }
    }

    /// Captures this immutable generation's batches for projection outside
    /// the lock, sharing its batches, bounds, and WAL positions by reference.
    ///
    /// Every batch is tagged with `generation`, the registry identity of this
    /// generation, so a caller can tell which generations the cut served and
    /// must not also read from their staged runs.
    fn readable_source(
        &self,
        generation: crate::scribe::hot_source::GenerationOrdinal,
    ) -> ReadableSource {
        ReadableSource::capture(
            &self.frozen.schema,
            vec![self.run.clone()],
            self.frozen.seal_key.clone(),
            Some(generation),
            &self.frozen.charge,
        )
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
    /// Maximum WAL LSN of the append.
    pub wal_lsn_max: crate::scribe::wal::WalLsn,
    /// Arrow rows for the append.
    pub batch: RecordBatch,
    /// Exact column bounds of `batch`, in its projected column order.
    pub statistics: Arc<Statistics>,
    /// Registry identity of the immutable generation serving the rows, or
    /// `None` for rows still in a writable bucket.
    ///
    /// A generation's staged runs become readable before the shard learns its
    /// member is durable, so for a moment both sources hold the same rows. The
    /// reader uses this to take such a generation from exactly one of them.
    pub generation: Option<crate::scribe::hot_source::GenerationOrdinal>,
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
    /// Logical table schema of every append batch.
    pub schema: SchemaRef,
    /// Original append batches, preserved for LSN-granular live tail reads
    /// and shared by reference with every live capture.
    ///
    /// All batches share one physical schema: `schema` for a canonical
    /// generation, or the layout the generation pinned when it opened, which
    /// differs from `schema` only in shredded Variant storage.
    pub batches: Arc<[RecordBatch]>,
    /// Exact column bounds of each batch, index-aligned with `batches`.
    ///
    /// A batch without an entry, as in a hand-built test generation, reads
    /// with unknown bounds and is never pruned.
    pub statistics: Arc<[Arc<Statistics>]>,
    /// Per-append metadata derived from WAL record headers.
    pub metas: Vec<ScribeAppendMeta>,
    /// Monotonic time at which the active generation first received a row.
    pub opened_at: Instant,
    /// Monotonic time at which the generation was detached.
    pub closed_at: Instant,
    /// Estimated Arrow memory retained by the generation.
    pub arrow_bytes: usize,
    /// Accounting owner shared by every holder of `batches`; retirement moves
    /// the generation's charged bytes into it (see
    /// [`crate::scribe::memory::RetainedArrowCharge`]).
    pub(crate) charge: Arc<crate::scribe::memory::RetainedArrowCharge>,
}

impl FrozenMemtable {
    /// Return the rows retained by this immutable generation.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }
}

/// Both memtable maps, read-locked together for one consistent selection.
type MemtableReadGuards<'a> = (
    std::sync::RwLockReadGuard<'a, HashMap<SealKey, MemtableBucket>>,
    std::sync::RwLockReadGuard<'a, HashMap<SealKey, Vec<ImmutableEntry>>>,
);

/// One coherent whole-table live capture over a pod's memtables and staged
/// runs.
///
/// Taken while every memtable's maps are read-locked, with the staged half
/// leased in the same registry critical section that decides which memory
/// generations stay readable. A freeze must write-lock the memtable it
/// detaches from, so no row can move from a writable bucket to staged runs
/// between the two halves, and each generation is served by exactly one of
/// its memory copy, its staged runs, or a published object. The capture keeps
/// its batches by reference and its staged runs leased until the last holder
/// drops it; each read filters and projects it without re-reading live state.
#[derive(Debug)]
pub(crate) struct LiveCapture {
    /// Memory sources the capture serves, in memtable order.
    memory: Vec<ReadableSource>,
    /// Lease over the staged members the memory half did not serve.
    staged: Option<Arc<crate::scribe::hot_source::StagedSourceLease>>,
    /// Revision of this node's last locally completed publication of the
    /// table, read with the staged boundary; 0 without a registry.
    publication_revision: i64,
}

impl LiveCapture {
    /// Captures one tenant table across `memtables` and `registry`.
    ///
    /// Returns `None`, holding nothing, while the registry refuses the table
    /// because a publication outcome is unsettled (see
    /// [`crate::scribe::hot_source::ScribeHotSourceRegistry::capture_staged`]).
    /// Without a registry there is no staged boundary and the capture is the
    /// memory half alone.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a memtable lock or the registry
    /// lock is poisoned.
    pub(crate) fn take<'a>(
        memtables: impl IntoIterator<Item = &'a Memtable>,
        tenant: DataTenantId,
        table: &crate::catalog::TableRef,
        registry: Option<&Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
    ) -> Result<Option<Self>, ScribeError> {
        let memtables: Vec<&Memtable> = memtables.into_iter().collect();
        let guards = memtables
            .iter()
            .map(|memtable| memtable.read_guards())
            .collect::<Result<Vec<_>, _>>()?;
        let mut memory = Vec::new();
        for (memtable, guards) in memtables.iter().zip(&guards) {
            memtable.select_sources(guards, tenant, table, None, &mut memory);
        }
        #[cfg(test)]
        if let Some(hook) = CAPTURE_SELECTED.with(|hook| hook.borrow_mut().take()) {
            hook();
        }
        let mut publication_revision = 0;
        let staged = if let Some(registry) = registry {
            let served = memory
                .iter()
                .filter_map(|source| source.generation.map(|g| (source.seal_key.clone(), g)))
                .collect();
            let Some(capture) =
                registry
                    .capture_staged(tenant, table, &served)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("capture the staged members of a live-tail read: {error}"),
                    })?
            else {
                return Ok(None);
            };
            memory.retain(|source| {
                source.generation.is_none_or(|generation| {
                    !capture
                        .superseded
                        .contains(&(source.seal_key.clone(), generation))
                })
            });
            publication_revision = capture.publication_revision;
            Some(Arc::new(capture.lease))
        } else {
            None
        };
        drop(guards);
        Ok(Some(Self {
            memory,
            staged,
            publication_revision,
        }))
    }

    /// Returns the revision of this node's last locally completed
    /// publication of the table as of the capture.
    ///
    /// The Oracle compares it against its cut: a higher revision means the
    /// cut predates a publication whose rows the capture no longer serves.
    pub(crate) fn publication_revision(&self) -> i64 {
        self.publication_revision
    }

    /// Reports whether the capture holds no memory source and no staged run.
    pub(crate) fn is_empty(&self) -> bool {
        self.memory.is_empty()
            && self
                .staged
                .as_ref()
                .is_none_or(|lease| lease.sources().is_empty())
    }

    /// Returns each captured partition once, sorted, with the bytes `scope`
    /// reads from its memory batches and staged-run row groups.
    pub(crate) fn partition_bytes(
        &self,
        scope: &crate::scan_sizing::ScanScope,
    ) -> Vec<(crate::catalog::layout::TimePartition, u64)> {
        let mut partitions =
            std::collections::BTreeMap::<crate::catalog::layout::TimePartition, u64>::new();
        let memory = self
            .memory
            .iter()
            .map(|source| (source.seal_key.partition.time(), source.scoped_bytes(scope)));
        let staged = self.staged.iter().flat_map(|lease| {
            lease.sources().iter().map(|source| {
                (
                    source.key.partition.time(),
                    source.footers.scoped_bytes(scope),
                )
            })
        });
        for (partition, bytes) in memory.chain(staged) {
            let total = partitions.entry(partition).or_default();
            *total = total.saturating_add(bytes);
        }
        partitions.into_iter().collect()
    }

    /// Reads one partition range of the capture: its memory batches `pruning`
    /// keeps, projected to `required` or, with `live`, filtered and evaluated
    /// as [`LiveProjection`] says, the accounting owners of those
    /// batches, its staged runs in merge order with the size and footer
    /// staging recorded for each, and the lease that keeps those runs on disk.
    ///
    /// The batch list is sized once from the in-range sources, so a read
    /// never regrows it.
    ///
    /// Pruning never changes which generations the read serves: the staged
    /// half was fixed when the capture was taken, so a generation whose every
    /// batch is pruned is still not read again from its staged runs.
    ///
    /// Projection shares the captured Arrow buffers, so the caller must hold
    /// the returned charges for as long as any returned batch lives, and the
    /// returned lease for as long as it reads any returned run path.
    ///
    /// # Errors
    ///
    /// Returns the column-selection error unchanged, or an internal error
    /// when Arrow projection fails.
    pub(crate) fn read(
        &self,
        partitions: &std::ops::RangeInclusive<crate::catalog::layout::TimePartition>,
        required: &[FieldRef],
        pruning: Option<&crate::scan_sizing::BatchPruning>,
        live: Option<&LiveProjection>,
    ) -> Result<CapturedRead, ScribeError> {
        let in_range: Vec<&ReadableSource> = self
            .memory
            .iter()
            .filter(|source| partitions.contains(&source.seal_key.partition.time()))
            .collect();
        // Sized once: growing one batch at a time reallocates into fresh
        // pages on every query while writes keep many batches live.
        let mut batches =
            Vec::with_capacity(in_range.iter().map(|source| source.batch_count()).sum());
        let mut charges = Vec::with_capacity(in_range.len());
        let mut scanned = MemoryScan::default();
        for source in in_range {
            source.project(required, pruning, live, &mut batches, &mut scanned)?;
            charges.push(Arc::clone(&source.charge));
        }
        let runs = self
            .staged
            .iter()
            .flat_map(|lease| lease.sources())
            .filter(|source| partitions.contains(&source.key.partition.time()))
            .flat_map(|source| source.footers.reads(&source.runs))
            .collect();
        Ok(CapturedRead {
            batches,
            charges,
            scanned,
            runs,
            lease: self.staged.clone(),
        })
    }
}

#[cfg(test)]
thread_local! {
    /// One-shot hook run by [`LiveCapture::take`] on this thread between its
    /// memory selection and its staged lease, with every guard still held.
    static CAPTURE_SELECTED: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Arms `hook` to run inside the next capture taken on this thread, at the
/// point a concurrent rollover would split its memory and staged halves.
#[cfg(test)]
pub(crate) fn on_capture_selected_for_test(hook: Box<dyn FnOnce()>) {
    CAPTURE_SELECTED.with(|slot| *slot.borrow_mut() = Some(hook));
}

/// The rows a live read returns and the values it returns of them.
///
/// Both are planned over the read's required fields. A read given one filters
/// every kept memory batch exactly by `filter` and returns only `exprs`, each
/// evaluated on the batch's physical columns, so a scalar read of a shredded
/// Variant reads its typed leaves and never reconstructs the whole value; an
/// expression that is a bare column is reconstructed as a whole read is.
#[derive(Debug, Clone)]
pub struct LiveProjection {
    /// Predicate every returned row satisfies, or `None` to keep every row.
    pub filter: Option<Arc<dyn datafusion::physical_expr::PhysicalExpr>>,
    /// Output expressions, in output order.
    pub exprs: datafusion::physical_expr::projection::ProjectionExprs,
}

impl LiveProjection {
    /// Rewrites the filter and expressions from `logical` onto one batch's
    /// `physical` layout with `DataFusion`'s default expression adapter.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when an expression does not adapt.
    fn adapted(&self, logical: &SchemaRef, physical: SchemaRef) -> Result<Self, ScribeError> {
        use datafusion::physical_expr_adapter::PhysicalExprAdapterFactory as _;
        let failed = |error: datafusion::error::DataFusionError| ScribeError::Internal {
            detail: format!("live-tail projection does not adapt: {error}"),
        };
        let adapter = datafusion::physical_expr_adapter::DefaultPhysicalExprAdapterFactory
            .create(Arc::clone(logical), physical)
            .map_err(failed)?;
        Ok(Self {
            filter: self
                .filter
                .as_ref()
                .map(|filter| adapter.rewrite(Arc::clone(filter)))
                .transpose()
                .map_err(failed)?,
            exprs: self
                .exprs
                .clone()
                .try_map_exprs(|expr| adapter.rewrite(expr))
                .map_err(failed)?,
        })
    }
}

/// One partition range read from a [`LiveCapture`].
#[derive(Debug)]
pub(crate) struct CapturedRead {
    /// Memory batches projected to the read's columns.
    pub(crate) batches: Vec<ReadableBatch>,
    /// Accounting owners of `batches`, which keep their bytes charged to the
    /// Scribe root while the read holds them.
    pub(crate) charges: Vec<Arc<crate::scribe::memory::RetainedArrowCharge>>,
    /// The memory scan work producing `batches`, including batches the
    /// read's filter emptied.
    pub(crate) scanned: MemoryScan,
    /// Staged runs with their recorded size and footer, in merge order.
    pub(crate) runs: Vec<crate::scribe::hot_source::StagedRunRead>,
    /// Lease keeping `runs` on disk while the read holds it.
    pub(crate) lease: Option<Arc<crate::scribe::hot_source::StagedSourceLease>>,
}

/// Memory scan work of one live read: the rows its kept memory batches held
/// and the distinct Arrow buffer bytes its filter and expressions read.
///
/// Counted per batch after bounds pruning and before row filtering, so a
/// batch the bounds exclude contributes nothing and a batch the filter
/// empties still counts. A shredded scalar read counts only the leaves its
/// functions declare (see [`BufferTally`]). This is in-memory scan work, not
/// Parquet IO, retained heap, a sizing estimate, returned rows or transport
/// bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryScan {
    /// Rows of the kept batches, before the read's filter.
    pub rows: u64,
    /// Distinct Arrow buffer bytes the read's expressions access.
    pub bytes: u64,
}

/// Distinct Arrow buffer bytes one batch's read touches.
///
/// Buffers are deduplicated by address within the batch, so two expressions
/// over one column count it once. A column is counted whole unless a scalar
/// function declares, through `required_input_fields`, the struct paths it
/// reads; then only those subtrees and their ancestors' validity count, so a
/// Variant path read excludes unused shredded siblings.
#[derive(Default)]
struct BufferTally {
    /// Addresses of the buffers already counted.
    seen: std::collections::HashSet<usize>,
    /// Bytes of the counted buffers.
    bytes: u64,
}

impl BufferTally {
    /// Counts `buffer` once.
    fn buffer(&mut self, buffer: &arrow::buffer::Buffer) {
        if self.seen.insert(buffer.as_ptr() as usize) {
            self.bytes = self.bytes.saturating_add(buffer.len() as u64);
        }
    }

    /// Counts every buffer of `data` and its children.
    fn data(&mut self, data: &arrow::array::ArrayData) {
        for buffer in data.buffers() {
            self.buffer(buffer);
        }
        if let Some(nulls) = data.nulls() {
            self.buffer(nulls.buffer());
        }
        for child in data.child_data() {
            self.data(child);
        }
    }

    /// Counts the subtree `path` selects in `data`, with each struct
    /// ancestor's validity; a path through a non-struct counts it whole.
    fn path(&mut self, data: &arrow::array::ArrayData, path: &[String]) {
        let Some((name, rest)) = path.split_first() else {
            return self.data(data);
        };
        let arrow::datatypes::DataType::Struct(fields) = data.data_type() else {
            return self.data(data);
        };
        if let Some(nulls) = data.nulls() {
            self.buffer(nulls.buffer());
        }
        if let Some(child) = fields
            .iter()
            .position(|field| field.name() == name)
            .and_then(|at| data.child_data().get(at))
        {
            self.path(child, rest);
        }
    }

    /// Counts the columns of `batch` that `expr` reads.
    fn expr(
        &mut self,
        expr: &Arc<dyn datafusion::physical_expr::PhysicalExpr>,
        batch: &RecordBatch,
    ) {
        use datafusion::physical_expr::ScalarFunctionExpr;
        use datafusion::physical_expr::expressions::Column;
        let column = |expr: &Arc<dyn datafusion::physical_expr::PhysicalExpr>| {
            expr.downcast_ref::<Column>()
                .and_then(|column| batch.columns().get(column.index()))
                .map(arrow::array::Array::to_data)
        };
        if expr.downcast_ref::<Column>().is_some() {
            if let Some(data) = column(expr) {
                self.data(&data);
            }
            return;
        }
        if let Some(function) = expr.downcast_ref::<ScalarFunctionExpr>()
            && let Some(required) = function.required_input_fields(batch.schema_ref())
        {
            for (index, arg) in function.args().iter().enumerate() {
                match (
                    required.iter().find(|required| required.arg_index == index),
                    column(arg),
                ) {
                    (Some(required), Some(data)) => {
                        for path in &required.field_paths {
                            self.path(&data, path);
                        }
                    }
                    _ => self.expr(arg, batch),
                }
            }
            return;
        }
        for child in expr.children() {
            self.expr(child, batch);
        }
    }
}

/// Most appends a writable bucket's tail holds before it seals into a shared
/// [`AppendRun`].
///
/// A live capture shares every sealed run and copies only the tail, so this
/// bounds the per-capture copy while keeping a bucket's run count small.
const APPEND_RUN_LEN: usize = 64;

/// Consecutive appends shared by reference between the bucket or generation
/// that retains them and every live capture of them.
///
/// Cloning a run clones three handles, never a batch, so a capture taken
/// under the memtable read locks costs a few pointer copies per run.
#[derive(Debug, Clone)]
struct AppendRun {
    /// The appended batches, in append order.
    batches: Arc<[RecordBatch]>,
    /// Exact bounds of each batch, index-aligned with `batches`; a batch
    /// without an entry reads with unknown bounds.
    statistics: Arc<[Arc<Statistics>]>,
    /// Maximum WAL LSN of each append, index-aligned with `batches`.
    wal_lsns: Arc<[crate::scribe::wal::WalLsn]>,
}

impl AppendRun {
    /// Builds a run over index-aligned appends.
    ///
    /// `statistics` may be shorter than `batches`, as in a hand-built test
    /// generation; those batches read with unknown bounds.
    ///
    /// # Panics
    ///
    /// Panics when `wal_lsns` does not carry exactly one WAL position per
    /// batch. A live read pairs batches with positions run by run, so a
    /// mismatch would misalign every later run's bounds and pruning.
    fn new(
        batches: Arc<[RecordBatch]>,
        statistics: Arc<[Arc<Statistics>]>,
        wal_lsns: Arc<[crate::scribe::wal::WalLsn]>,
    ) -> Self {
        assert_eq!(
            batches.len(),
            wal_lsns.len(),
            "an append run carries exactly one WAL position per batch"
        );
        Self {
            batches,
            statistics,
            wal_lsns,
        }
    }
}

/// A writable bucket's appends after its last sealed [`AppendRun`].
#[derive(Debug, Default)]
struct AppendTail {
    /// The appended batches, in append order.
    batches: Vec<RecordBatch>,
    /// Exact bounds of each batch, index-aligned with `batches`.
    statistics: Vec<Arc<Statistics>>,
    /// Maximum WAL LSN of each append, index-aligned with `batches`.
    wal_lsns: Vec<crate::scribe::wal::WalLsn>,
}

impl AppendTail {
    /// Copies the tail into a run a capture holds while the bucket keeps
    /// appending.
    fn run(&self) -> AppendRun {
        AppendRun::new(
            self.batches.as_slice().into(),
            self.statistics.as_slice().into(),
            self.wal_lsns.as_slice().into(),
        )
    }

    /// Moves the tail into a sealed run, leaving it empty.
    fn seal(&mut self) -> AppendRun {
        AppendRun::new(
            std::mem::take(&mut self.batches).into(),
            std::mem::take(&mut self.statistics).into(),
            std::mem::take(&mut self.wal_lsns).into(),
        )
    }
}

/// One bucket's or generation's batches, captured under the memtable read
/// locks and projected only after they are released.
///
/// Capturing shares each [`AppendRun`] by reference and copies only a
/// writable bucket's tail, never row data, so the locks are held for a few
/// pointer copies per run.
#[derive(Debug)]
struct ReadableSource {
    /// Arrow schema shared by every batch.
    schema: SchemaRef,
    /// The captured appends, in append order.
    runs: Vec<AppendRun>,
    /// Complete seal key owning every batch; its generation ordinals are
    /// unique only within it.
    seal_key: SealKey,
    /// Serving immutable generation, or `None` for a writable bucket.
    generation: Option<crate::scribe::hot_source::GenerationOrdinal>,
    /// Accounting owner of `batches`, shared with the bucket or generation
    /// they were captured from so retirement cannot uncharge them.
    charge: Arc<crate::scribe::memory::RetainedArrowCharge>,
}

impl ReadableSource {
    /// Captures `runs`, sharing `charge`, the accounting owner of their
    /// batches.
    fn capture(
        schema: &SchemaRef,
        runs: Vec<AppendRun>,
        seal_key: SealKey,
        generation: Option<crate::scribe::hot_source::GenerationOrdinal>,
        charge: &Arc<crate::scribe::memory::RetainedArrowCharge>,
    ) -> Self {
        Self {
            schema: Arc::clone(schema),
            runs,
            seal_key,
            generation,
            charge: Arc::clone(charge),
        }
    }

    /// Returns how many batches the source captured.
    fn batch_count(&self) -> usize {
        self.runs.iter().map(|run| run.batches.len()).sum()
    }

    /// Returns the bytes `scope` reads from the captured batches, measured by
    /// [`crate::scan_sizing::ScanSizer::batch_bytes`] run by run.
    fn scoped_bytes(&self, scope: &crate::scan_sizing::ScanScope) -> u64 {
        let sizer = crate::scan_sizing::ScanSizer::new(scope, &self.schema);
        self.runs
            .iter()
            .map(|run| sizer.batch_bytes(&run.statistics))
            .fold(0_u64, u64::saturating_add)
    }

    /// Appends each captured batch `pruning` keeps, projected to `required`,
    /// to `output`, with its bounds projected like its columns.
    ///
    /// Each required field resolves by field id (see [`projection_indices`]),
    /// so rows admitted under an earlier schema revision are emitted under the
    /// required field's current name and type. A required field the batch
    /// predates is filled with nulls and has unknown bounds. An empty
    /// `required` keeps every column.
    ///
    /// `pruning` is planned over `required` and runs on the projected bounds
    /// of every captured batch before any batch is projected, so a batch the
    /// query's predicate rules out is never cast or assembled. `None` keeps
    /// every batch. Bounds are shared unchanged when the projection reads
    /// every written column, in order and uncast.
    ///
    /// A generation retained in a learned Variant layout is emitted in its
    /// logical schema: each projected shredded Variant column is reconstructed
    /// from its typed leaves and residual, so a capture never exposes a
    /// physical layout and memory unions with staged and published rows value
    /// for value. Before reconstruction such a batch is filtered by `pruning`'s
    /// predicate, adapted to the physical layout by DataFusion's
    /// [`DefaultPhysicalExprAdapterFactory`], so only matching rows are
    /// reconstructed and Variant functions can read typed leaves. A filtered
    /// batch's bounds become inexact. A predicate that does not adapt or
    /// evaluate filters nothing here; the query's own filter still runs.
    ///
    /// With `live`, a kept batch is instead filtered exactly by its filter and
    /// emitted as its expressions, evaluated on the physical columns (see
    /// [`LiveProjection`]), with bounds computed from the emitted rows.
    ///
    /// [`DefaultPhysicalExprAdapterFactory`]: datafusion::physical_expr_adapter::DefaultPhysicalExprAdapterFactory
    ///
    /// # Errors
    ///
    /// Returns the column-selection error unchanged, or an internal error
    /// when Variant reconstruction, Arrow cast, batch assembly, or a `live`
    /// filter or expression fails.
    fn project(
        &self,
        required: &[FieldRef],
        pruning: Option<&crate::scan_sizing::BatchPruning>,
        live: Option<&LiveProjection>,
        output: &mut Vec<ReadableBatch>,
        scanned: &mut MemoryScan,
    ) -> Result<(), ScribeError> {
        let internal = |error: arrow::error::ArrowError| ScribeError::Internal {
            detail: format!("live-tail Arrow projection failed: {error}"),
        };
        let projection = projection_indices(&self.schema, required)?;
        let schema = Arc::new(Schema::new_with_metadata(
            projection
                .iter()
                .map(|(field, _)| Arc::clone(field))
                .collect::<Vec<_>>(),
            self.schema.metadata().clone(),
        ));
        // A source column keeps its recorded bounds only when it is read
        // unchanged; a cast or null-filled column's bounds are unknown.
        let kept: Vec<Option<usize>> = projection
            .iter()
            .map(|(field, source)| {
                source.filter(|column| self.schema.field(*column).data_type() == field.data_type())
            })
            .collect();
        let whole = kept.len() == self.schema.fields().len()
            && kept
                .iter()
                .enumerate()
                .all(|(at, column)| *column == Some(at));
        let statistics: Vec<Arc<Statistics>> = self
            .runs
            .iter()
            .flat_map(|run| (0..run.batches.len()).map(|index| run.statistics.get(index)))
            .map(|recorded| match recorded {
                Some(bounds) if whole => Arc::clone(bounds),
                Some(bounds) => Arc::new(Statistics {
                    num_rows: bounds.num_rows,
                    total_byte_size: Precision::Absent,
                    column_statistics: kept
                        .iter()
                        .map(|column| {
                            column.map_or_else(ColumnStatistics::new_unknown, |column| {
                                bounds.column_statistics[column].clone()
                            })
                        })
                        .collect(),
                }),
                None => Arc::new(Statistics::new_unknown(&schema)),
            })
            .collect();
        let keep = pruning.map(|pruning| pruning.keep(&statistics, statistics.len()));
        let live_schema = live
            .map(|live| live.exprs.project_schema(&schema).map(Arc::new))
            .transpose()
            .map_err(|error| ScribeError::Internal {
                detail: format!("live-tail projection does not plan: {error}"),
            })?;
        // One generation pins one layout, so the predicate adapts once to the
        // first physical batch and serves them all.
        let mut adapted: Option<Option<Arc<dyn datafusion::physical_expr::PhysicalExpr>>> = None;
        let mut live_adapted: Option<LiveProjection> = None;
        for (index, ((batch, wal_lsn_max), mut statistics)) in self
            .runs
            .iter()
            .flat_map(|run| run.batches.iter().zip(run.wal_lsns.iter().copied()))
            .zip(statistics)
            .enumerate()
        {
            if keep.as_ref().is_some_and(|keep| !keep[index]) {
                continue;
            }
            let shredded = projection.iter().any(|(_, source)| {
                source.is_some_and(|column| {
                    batch.column(column).data_type() != self.schema.field(column).data_type()
                })
            });
            let mut physical = physical_projection(batch, &schema, &projection);
            scanned.rows = scanned.rows.saturating_add(physical.num_rows() as u64);
            let mut tally = BufferTally::default();
            if let (Some(live), Some(live_schema)) = (live, &live_schema) {
                let adapted = match &live_adapted {
                    Some(adapted) => adapted,
                    None => live_adapted.insert(live.adapted(&schema, physical.schema())?),
                };
                for expr in adapted
                    .filter
                    .iter()
                    .chain(adapted.exprs.iter().map(|expr| &expr.expr))
                {
                    tally.expr(expr, &physical);
                }
                scanned.bytes = scanned.bytes.saturating_add(tally.bytes);
                if let Some(filter) = &adapted.filter {
                    physical = datafusion::physical_plan::filter::batch_filter(&physical, filter)
                        .map_err(|error| ScribeError::Internal {
                        detail: format!("live-tail filter failed: {error}"),
                    })?;
                    if physical.num_rows() == 0 {
                        continue;
                    }
                }
                let rows = physical.num_rows();
                let columns = adapted
                    .exprs
                    .iter()
                    .zip(live.exprs.iter())
                    .zip(live_schema.fields())
                    .map(|((adapted, logical), field)| {
                        let column = match logical
                            .expr
                            .downcast_ref::<datafusion::physical_expr::expressions::Column>(
                        ) {
                            // A whole column is reconstructed as a whole read is.
                            Some(column) => {
                                let at = column.index();
                                match projection[at].1 {
                                    Some(source) => crate::schema::logical_column(
                                        &physical.schema_ref().fields()[at],
                                        physical.column(at),
                                        self.schema.field(source),
                                    )
                                    .map_err(|detail| ScribeError::Internal {
                                        detail: format!(
                                            "live-tail Variant reconstruction failed: {detail}"
                                        ),
                                    })?,
                                    None => Arc::clone(physical.column(at)),
                                }
                            }
                            // Any other expression reads the physical leaves.
                            None => adapted
                                .expr
                                .evaluate(&physical)
                                .and_then(|value| value.into_array(rows))
                                .map_err(|error| ScribeError::Internal {
                                    detail: format!("live-tail projection failed: {error}"),
                                })?,
                        };
                        if column.data_type() == field.data_type() {
                            Ok(column)
                        } else {
                            arrow::compute::cast(&column, field.data_type()).map_err(internal)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let batch = RecordBatch::try_new_with_options(
                    Arc::clone(live_schema),
                    columns,
                    &arrow::array::RecordBatchOptions::new().with_row_count(Some(rows)),
                )
                .map_err(internal)?;
                output.push(ReadableBatch {
                    partition_day: self.seal_key.partition.time(),
                    wal_lsn_max,
                    statistics: batch_statistics(&batch),
                    batch,
                    generation: self.generation,
                });
                continue;
            }
            for column in physical.columns() {
                tally.data(&column.to_data());
            }
            scanned.bytes = scanned.bytes.saturating_add(tally.bytes);
            if let Some(pruning) = pruning.filter(|_| shredded) {
                let predicate = adapted.get_or_insert_with(|| {
                    use datafusion::physical_expr_adapter::PhysicalExprAdapterFactory as _;
                    datafusion::physical_expr_adapter::DefaultPhysicalExprAdapterFactory
                        .create(Arc::clone(&schema), physical.schema())
                        .ok()?
                        .rewrite(Arc::clone(pruning.predicate().orig_expr()))
                        .ok()
                });
                if let Some(kept) = predicate.as_ref().and_then(|predicate| {
                    datafusion::physical_plan::filter::batch_filter(&physical, predicate).ok()
                }) {
                    if kept.num_rows() == 0 {
                        continue;
                    }
                    if kept.num_rows() != physical.num_rows() {
                        statistics = Arc::new(statistics.as_ref().clone().to_inexact());
                    }
                    physical = kept;
                }
            }
            let rows = physical.num_rows();
            let columns = projection
                .iter()
                .zip(physical.schema_ref().fields())
                .zip(physical.columns())
                .map(|(((field, source), stored), column)| match source {
                    Some(source) => {
                        let logical = crate::schema::logical_column(
                            stored,
                            column,
                            self.schema.field(*source),
                        )
                        .map_err(|detail| ScribeError::Internal {
                            detail: format!("live-tail Variant reconstruction failed: {detail}"),
                        })?;
                        if logical.data_type() == field.data_type() {
                            Ok(logical)
                        } else {
                            arrow::compute::cast(&logical, field.data_type()).map_err(internal)
                        }
                    }
                    None => Ok(Arc::clone(column)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let batch = RecordBatch::try_new_with_options(
                Arc::clone(&schema),
                columns,
                &arrow::array::RecordBatchOptions::new().with_row_count(Some(rows)),
            )
            .map_err(internal)?;
            output.push(ReadableBatch {
                partition_day: self.seal_key.partition.time(),
                wal_lsn_max,
                batch,
                statistics,
                generation: self.generation,
            });
        }
        Ok(())
    }
}

/// Returns `batch`'s columns `projection` reads, unconverted, under the names
/// of `schema`, with a projected field the batch predates filled with nulls:
/// the stored layout a capture filters before reconstructing it in `schema`.
///
/// # Panics
///
/// Never in practice: every column comes from `batch` or is built at its row
/// count, under its own field.
fn physical_projection(
    batch: &RecordBatch,
    schema: &SchemaRef,
    projection: &[(FieldRef, Option<usize>)],
) -> RecordBatch {
    let (fields, columns): (Vec<_>, Vec<_>) = projection
        .iter()
        .zip(schema.fields())
        .map(|((field, source), named)| match source {
            Some(column) => (
                Arc::new(
                    batch
                        .schema_ref()
                        .field(*column)
                        .clone()
                        .with_name(named.name()),
                ),
                Arc::clone(batch.column(*column)),
            ),
            None => (
                Arc::clone(named),
                arrow::array::new_null_array(field.data_type(), batch.num_rows()),
            ),
        })
        .unzip();
    RecordBatch::try_new_with_options(
        Arc::new(Schema::new(fields)),
        columns,
        &arrow::array::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
    )
    .expect("columns taken from one batch share its row count and their own fields")
}

/// Computes one append batch's exact row count, every column's byte size,
/// and primitive-column bounds.
///
/// This is the one producer of memtable batch statistics, at append and at
/// replay; projection and the follower carry them unchanged.
///
/// Each column's byte size is its Arrow data, sliced to the batch, as
/// [`arrow::array::ArrayData::get_slice_memory_size`] counts it: the decoded
/// bytes a scan of that column reads, independent of how many columns share
/// one received allocation. Recorded once here, it lets
/// [`crate::scan_sizing::ScanSizer::batch_bytes`] size every query by adding
/// numbers. A size Arrow cannot compute stays unknown, which sizing counts
/// as a fallback.
///
/// Only primitive columns (integers, floats, decimals, dates, times,
/// timestamps) get a minimum and maximum: they cost one vectorized pass at
/// insert, which a live read then saves on every query that can rule the
/// batch out. Other columns, and a primitive column whose bound cannot be
/// computed, stay unknown, which never prunes.
pub(crate) fn batch_statistics(batch: &RecordBatch) -> Arc<Statistics> {
    let mut statistics = Statistics::new_unknown(batch.schema_ref());
    statistics.num_rows = Precision::Exact(batch.num_rows());
    for (column, bounds) in batch
        .columns()
        .iter()
        .zip(statistics.column_statistics.iter_mut())
    {
        bounds.null_count = Precision::Exact(column.null_count());
        if let Ok(bytes) = column.to_data().get_slice_memory_size() {
            bounds.byte_size = Precision::Exact(bytes);
        }
        if !column.data_type().is_primitive() {
            continue;
        }
        let values = std::slice::from_ref(column);
        let min = MinAccumulator::try_new(column.data_type())
            .and_then(|mut min| min.update_batch(values).and_then(|()| min.evaluate()));
        let max = MaxAccumulator::try_new(column.data_type())
            .and_then(|mut max| max.update_batch(values).and_then(|()| max.evaluate()));
        if let (Ok(min), Ok(max)) = (min, max) {
            bounds.min_value = Precision::Exact(min);
            bounds.max_value = Precision::Exact(max);
        }
    }
    Arc::new(statistics)
}

/// Resolves each required field to its column in one captured schema.
///
/// A required field carrying a `PARQUET:field_id` matches the column with the
/// same id when the captured schema carries ids, so a column renamed after the
/// rows were admitted still resolves, and a column added after them resolves
/// to `None`. A field without an id selects the captured column of that name
/// unchanged, and that column must be present. An empty `required` selects
/// every captured column.
///
/// # Errors
///
/// Returns an internal error when a field without an id is absent, or when
/// two required fields resolve to the same column.
fn projection_indices(
    schema: &SchemaRef,
    required: &[FieldRef],
) -> Result<Vec<(FieldRef, Option<usize>)>, ScribeError> {
    if required.is_empty() {
        return Ok(schema
            .fields()
            .iter()
            .enumerate()
            .map(|(index, field)| (Arc::clone(field), Some(index)))
            .collect());
    }
    let field_id = |field: &FieldRef| field.metadata().get(PARQUET_FIELD_ID).cloned();
    let schema_has_ids = schema
        .fields()
        .iter()
        .any(|field| field_id(field).is_some());
    let mut projection: Vec<(FieldRef, Option<usize>)> = Vec::with_capacity(required.len());
    for field in required {
        let (target, index) = if let Some(id) = field_id(field).filter(|_| schema_has_ids) {
            (
                Arc::clone(field),
                schema
                    .fields()
                    .iter()
                    .position(|column| field_id(column).as_ref() == Some(&id)),
            )
        } else {
            let index = schema
                .index_of(field.name())
                .map_err(|_| ScribeError::Internal {
                    detail: format!(
                        "live-tail required column is not in the table schema: {}",
                        field.name()
                    ),
                })?;
            (Arc::clone(&schema.fields()[index]), Some(index))
        };
        if index.is_some() && projection.iter().any(|(_, kept)| *kept == index) {
            return Err(ScribeError::Internal {
                detail: format!("live-tail required column is duplicated: {}", field.name()),
            });
        }
        projection.push((target, index));
    }
    Ok(projection)
}

/// Returns the bytes one batch adds to the memtable's charged ownership.
///
/// This is the same measure preprocessing charged when it admitted the rows,
/// so the freeze that moves a generation from active to immutable ownership
/// transfers exactly what the insert reserved. Summing Arrow's per-column
/// allocation instead would charge a zero-copy native frame once per column and
/// break that transfer.
fn estimate_batch_bytes(batch: &RecordBatch) -> usize {
    crate::scribe::memory::retained_arrow_bytes(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableRef;

    use crate::namespaces::BifrostNamespace;
    use crate::scribe::wal::WalLsn;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};

    use std::sync::Arc;
    use std::sync::Barrier;

    fn make_test_batch(num_rows: usize) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let array = Int64Array::from(vec![1i64; num_rows]);
        RecordBatch::try_new(schema, vec![Arc::new(array)]).expect("batch")
    }

    /// A required field resolves by field id across a rename, an added field
    /// resolves to no column, and an id-less field selects by name unchanged.
    ///
    /// # Panics
    ///
    /// Panics when a field resolves to the wrong column or a duplicate is
    /// accepted.
    #[test]
    fn projection_indices_resolve_by_field_id() {
        let with_id = |name: &str, id: &str| {
            Arc::new(
                Field::new(name, DataType::Utf8, true)
                    .with_metadata([(PARQUET_FIELD_ID.to_owned(), id.to_owned())]),
            )
        };
        let captured: SchemaRef = Arc::new(Schema::new(vec![
            with_id("id", "1"),
            with_id("customer", "2"),
        ]));
        let required = [with_id("client", "2"), with_id("note", "3")];

        let projection = projection_indices(&captured, &required).expect("projection");

        assert_eq!(
            projection
                .iter()
                .map(|(field, index)| (field.name().as_str(), *index))
                .collect::<Vec<_>>(),
            [("client", Some(1)), ("note", None)]
        );
        let by_name = projection_indices(
            &captured,
            &[Arc::new(Field::new("id", DataType::Null, true))],
        )
        .expect("named projection");
        assert_eq!(by_name[0].0.data_type(), &DataType::Utf8);
        assert!(projection_indices(&captured, &[with_id("a", "2"), with_id("b", "2")]).is_err());
    }

    /// A capture read prunes its memory batches by the query's predicate,
    /// planned over the required fields, before projecting them: a batch
    /// whose bounds exclude the key is never returned, the matching one is,
    /// and every batch is kept without a predicate.
    ///
    /// # Panics
    ///
    /// Panics when the capture cannot be taken or read, the excluded batch is
    /// kept, or the matching batch is dropped.
    #[test]
    fn capture_read_prunes_batches_before_projecting() {
        let memtable = Memtable::new();
        let key = make_test_seal_key();
        memtable
            .insert(&key, make_test_meta(1), make_test_batch(2))
            .expect("generation insert");
        memtable.freeze(&key).expect("frozen generation");
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let matching = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![50_i64]))],
        )
        .expect("matching batch");
        memtable
            .insert(&key, make_test_meta(2), matching)
            .expect("active insert");
        let pruning = crate::scan_sizing::BatchPruning::try_new(
            &schema,
            Arc::new(datafusion::physical_expr::expressions::BinaryExpr::new(
                datafusion::physical_expr::expressions::col("id", &schema).expect("id column"),
                datafusion::logical_expr::Operator::Eq,
                datafusion::physical_expr::expressions::lit(50_i64),
            )),
        )
        .expect("pruning plans");
        let capture = LiveCapture::take([&memtable], key.tenant, &key.table, None)
            .expect("locks")
            .expect("no registry to wait on");
        let partitions = key.partition.time()..=key.partition.time();
        let lsns = |pruning| {
            let mut lsns = capture
                .read(&partitions, schema.fields(), pruning, None)
                .expect("capture read")
                .batches
                .iter()
                .map(|batch| batch.wal_lsn_max.as_u64())
                .collect::<Vec<_>>();
            lsns.sort_unstable();
            lsns
        };

        assert_eq!(lsns(Some(&pruning)), vec![2]);
        assert_eq!(lsns(None), vec![1, 2]);
    }

    /// A capture shares a bucket's sealed runs and a generation's batches by
    /// reference and copies only the bucket's unsealed tail; every read and
    /// freeze keeps each append's rows paired with its WAL position, in order.
    ///
    /// Each append's single `id` row equals its LSN, so a reordered or
    /// misaligned batch is caught. The bucket is captured at exactly one full
    /// run (no tail), then again after more appends span two sealed runs and
    /// a tail; the earlier capture keeps its rows. Pruning keeps exactly the
    /// batches whose bounds match across a run boundary, and freezing shares
    /// one concatenated generation with all its bounds.
    ///
    /// # Panics
    ///
    /// Panics when a capture copies a sealed run or a frozen generation, or a
    /// read, prune, or freeze loses, reorders, or misaligns an append.
    #[test]
    fn captures_share_sealed_runs_and_frozen_generations() {
        use arrow::array::Array as _;

        let memtable = Memtable::new();
        let key = make_test_seal_key();
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let insert = |lsns: std::ops::RangeInclusive<u64>| {
            for lsn in lsns {
                let value = i64::try_from(lsn).expect("lsn value");
                let batch = RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![Arc::new(Int64Array::from(vec![value]))],
                )
                .expect("valued batch");
                memtable
                    .insert(&key, make_test_meta(lsn), batch)
                    .expect("active insert");
            }
        };
        let partitions = key.partition.time()..=key.partition.time();
        let lsns = |capture: &LiveCapture, pruning| -> Vec<u64> {
            capture
                .read(&partitions, schema.fields(), pruning, None)
                .expect("capture read")
                .batches
                .iter()
                .map(|readable| {
                    let lsn = readable.wal_lsn_max.as_u64();
                    let ids = readable
                        .batch
                        .column(0)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .expect("id column");
                    assert_eq!(ids.len(), 1);
                    assert_eq!(
                        u64::try_from(ids.value(0)).ok(),
                        Some(lsn),
                        "rows follow their LSN"
                    );
                    lsn
                })
                .collect()
        };
        let take = || {
            LiveCapture::take([&memtable], key.tenant, &key.table, None)
                .expect("locks")
                .expect("no registry to wait on")
        };
        let shares_sealed = |capture: &LiveCapture, sealed: usize| {
            let writable = memtable.writable.read().expect("writable lock");
            let bucket = &writable[&key];
            assert_eq!(bucket.runs.len(), sealed);
            for (captured, owned) in capture.memory[0].runs.iter().zip(&bucket.runs) {
                assert!(Arc::ptr_eq(&captured.batches, &owned.batches));
                assert!(Arc::ptr_eq(&captured.statistics, &owned.statistics));
                assert!(Arc::ptr_eq(&captured.wal_lsns, &owned.wal_lsns));
            }
        };
        let full = u64::try_from(APPEND_RUN_LEN).expect("run length");
        let total = 2 * full + 2;

        insert(1..=full);
        let exact = take();
        assert_eq!(exact.memory[0].runs.len(), 1, "a full run leaves no tail");
        shares_sealed(&exact, 1);
        assert_eq!(lsns(&exact, None), (1..=full).collect::<Vec<_>>());

        insert(full + 1..=total);
        let active = take();
        assert_eq!(
            active.memory[0].runs.len(),
            3,
            "two sealed runs and the tail"
        );
        shares_sealed(&active, 2);
        assert_eq!(active.memory[0].runs[2].batches.len(), 2);
        assert_eq!(lsns(&active, None), (1..=total).collect::<Vec<_>>());
        assert_eq!(
            lsns(&exact, None),
            (1..=full).collect::<Vec<_>>(),
            "an earlier capture keeps its rows"
        );

        let bound = |operator, value: u64| {
            Arc::new(datafusion::physical_expr::expressions::BinaryExpr::new(
                datafusion::physical_expr::expressions::col("id", &schema).expect("id column"),
                operator,
                datafusion::physical_expr::expressions::lit(
                    i64::try_from(value).expect("bound value"),
                ),
            ))
        };
        let pruning = crate::scan_sizing::BatchPruning::try_new(
            &schema,
            Arc::new(datafusion::physical_expr::expressions::BinaryExpr::new(
                bound(datafusion::logical_expr::Operator::GtEq, full - 2),
                datafusion::logical_expr::Operator::And,
                bound(datafusion::logical_expr::Operator::LtEq, full + 3),
            )),
        )
        .expect("pruning plans");
        assert_eq!(
            lsns(&active, Some(&pruning)),
            (full - 2..=full + 3).collect::<Vec<_>>(),
            "pruning keeps exactly the matching batches across the run boundary"
        );

        let frozen = memtable.freeze(&key).expect("freeze");
        let appended = usize::try_from(total).expect("append count");
        assert_eq!(frozen.batches.len(), appended);
        assert_eq!(frozen.statistics.len(), appended);
        let captured = take();
        assert!(Arc::ptr_eq(
            &captured.memory[0].runs[0].batches,
            &frozen.batches
        ));
        assert_eq!(lsns(&captured, None), (1..=total).collect::<Vec<_>>());
        assert_eq!(
            lsns(&captured, Some(&pruning)),
            (full - 2..=full + 3).collect::<Vec<_>>()
        );
        assert_eq!(lsns(&active, None), (1..=total).collect::<Vec<_>>());
        assert_eq!(lsns(&exact, None), (1..=full).collect::<Vec<_>>());
    }

    /// A capture read that projects every written column, in order and
    /// uncast, shares each batch's recorded bounds instead of copying them,
    /// and a narrower projection copies only its columns' bounds. Either read
    /// sizes its batch list exactly to the captured batches.
    ///
    /// # Panics
    ///
    /// Panics when the capture cannot be taken or read, or the whole read
    /// does not share the recorded bounds.
    #[test]
    fn whole_schema_capture_read_shares_recorded_bounds() {
        let memtable = Memtable::new();
        let key = make_test_seal_key();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("other", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(Int64Array::from(vec![2_i64])),
            ],
        )
        .expect("two-column batch");
        memtable
            .insert(&key, make_test_meta(1), batch)
            .expect("active insert");
        let capture = LiveCapture::take([&memtable], key.tenant, &key.table, None)
            .expect("locks")
            .expect("no registry to wait on");
        let partitions = key.partition.time()..=key.partition.time();
        let recorded = Arc::clone(&capture.memory[0].runs[0].statistics[0]);
        let read = |fields: &[FieldRef]| {
            let mut batches = capture
                .read(&partitions, fields, None, None)
                .expect("capture read")
                .batches;
            assert_eq!(batches.capacity(), 1, "the batch list is sized once");
            batches.remove(0).statistics
        };

        assert!(Arc::ptr_eq(&read(schema.fields()), &recorded));
        let narrow = read(&schema.fields()[..1]);
        assert!(!Arc::ptr_eq(&narrow, &recorded));
        assert_eq!(narrow.column_statistics.len(), 1);
    }

    /// An append batch's bounds are exact for a primitive column and unknown
    /// for any other, so a string predicate never prunes on a guess.
    ///
    /// # Panics
    ///
    /// Panics when a bound or the row count differs from the batch.
    #[test]
    fn batch_statistics_bound_primitive_columns_only() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("payload", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![7, 3, 9])),
                Arc::new(arrow::array::StringArray::from(vec!["a", "b", "c"])),
            ],
        )
        .expect("bounds fixture batch");

        let statistics = batch_statistics(&batch);

        let id = &statistics.column_statistics[0];
        let payload = &statistics.column_statistics[1];
        assert_eq!(statistics.num_rows, Precision::Exact(3));
        assert_eq!(
            id.min_value,
            Precision::Exact(datafusion::common::ScalarValue::Int64(Some(3)))
        );
        assert_eq!(
            id.max_value,
            Precision::Exact(datafusion::common::ScalarValue::Int64(Some(9)))
        );
        assert_eq!(payload.min_value, Precision::Absent);
        assert_eq!(payload.null_count, Precision::Exact(0));
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
            first.partition.clone(),
        );
        memtable
            .insert(&first, make_test_meta(1), make_test_batch(1))
            .expect("first active member");
        memtable
            .insert(&second, make_test_meta(2), make_test_batch(1))
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
            .insert(&first, make_test_meta(3), make_test_batch(1))
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
            first.partition.clone(),
        );
        let empty = SealKey::new(
            first.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "empty"),
            first.partition.clone(),
        );
        for (key, lsn) in [(&first, 1), (&second, 2)] {
            memtable
                .insert(key, make_test_meta(lsn), make_test_batch(1))
                .expect("populate cohort member");
        }
        memtable.writable.write().expect("writable lock").insert(
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

    /// A provider cut suppresses exactly the generations a durable member serves.
    ///
    /// Publication is a fact about a generation, not about a WAL interval. WAL
    /// records are numbered from one node-global counter while generations are
    /// sealed per tenant, table, partition, and shard, so a published member's
    /// bounds routinely enclose positions a still-live generation owns. The cut
    /// therefore excludes a generation only once something durable is already
    /// serving its rows, and an enclosed live generation stays readable.
    ///
    /// # Panics
    ///
    /// Panics when a live generation is dropped or a durable one is served
    /// twice.
    #[test]
    fn a_provider_cut_suppresses_only_generations_a_durable_member_serves() {
        let memtable = Memtable::new();
        let key = make_test_seal_key();
        for lsn in [1_u64, 30] {
            memtable
                .insert(&key, make_test_meta(lsn), make_test_batch(1))
                .expect("enclosing generation insert");
        }
        let enclosing = memtable.freeze(&key).expect("enclosing generation");
        for lsn in [12_u64, 20] {
            memtable
                .insert(&key, make_test_meta(lsn), make_test_batch(1))
                .expect("enclosed generation insert");
        }
        memtable.freeze(&key).expect("enclosed generation");
        memtable
            .insert(&key, make_test_meta(31), make_test_batch(1))
            .expect("active insert");
        let cut = |memtable: &Memtable| {
            memtable
                .readable_batches_for_range(
                    key.tenant,
                    &key.table,
                    key.partition.time(),
                    key.partition.time(),
                    &[],
                )
                .expect("provider cut")
                .iter()
                .map(|batch| batch.wal_lsn_max.as_u64())
                .collect::<std::collections::BTreeSet<_>>()
        };

        assert_eq!(
            cut(&memtable),
            [1_u64, 12, 20, 30, 31].into_iter().collect(),
            "nothing durable serves these rows yet, so every one of them is live"
        );

        memtable
            .complete_staged(enclosing.seal_id, staged_member(1))
            .expect("the enclosing generation is staged");

        assert_eq!(
            cut(&memtable),
            [12_u64, 20, 31].into_iter().collect(),
            "the enclosed generation owns its own rows and the enclosing member's \
             bounds are not evidence about them"
        );
    }

    /// Builds one staged-member identity standing for a published shard run.
    fn staged_member(generation: u64) -> crate::scribe::assembly::StagedMemberId {
        crate::scribe::assembly::StagedMemberId::new(0, generation)
    }

    #[test]
    fn test_freeze_promotes_to_immutable_not_drop() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        memtable
            .insert(&seal_key, make_test_meta(10), make_test_batch(2))
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
            .insert(&seal_key, meta, make_test_batch(2))
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

    /// One seal key retains several frozen generations, and only the ones that
    /// have not yet been staged are served from memory.
    ///
    /// The two clauses belong together: freezing must not collapse generations
    /// into one another, and a generation that reached its staged member must
    /// stop being served here, because from that point the staged source is its
    /// single authority and serving both would double its rows.
    #[test]
    fn test_multiple_frozen_generations_same_seal_key() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();
        memtable
            .insert(&seal_key, make_test_meta(10), make_test_batch(1))
            .expect("first insert");
        let first = memtable.freeze(&seal_key).expect("first freeze");
        memtable
            .complete_staged(first.seal_id, staged_member(10))
            .expect("first complete");

        memtable
            .insert(&seal_key, make_test_meta(20), make_test_batch(1))
            .expect("second insert");
        let second = memtable.freeze(&seal_key).expect("second freeze");

        assert_ne!(first.seal_id, second.seal_id);
        assert_eq!(memtable.immutable_generation_count().expect("immutable"), 2);
        let lsns: Vec<_> = memtable
            .readable_batches(crate::test_support::tenant(), &seal_key.table)
            .expect("readable")
            .into_iter()
            .map(|batch| batch.wal_lsn_max)
            .collect();
        assert_eq!(
            lsns,
            vec![WalLsn::new(20)],
            "a staged generation is served by its staged member, not by the retained Arrow copy"
        );
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
            .insert(&seal_key, make_test_meta(10), make_test_batch(1))
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
            .insert(&seal_key, make_test_meta(10), make_test_batch(1))
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
                .insert(&seal_key, make_test_meta(i), batch)
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
            .insert(&seal_key, make_test_meta(1), make_test_batch(1))
            .expect("seed bucket");
        {
            let mut buckets = memtable.writable.write().expect("writable lock");
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
        memtable.writable.write().expect("writable lock").insert(
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
            .insert(&seal_key, make_test_meta(0), batch)
            .expect("insert");

        let mut buckets = memtable.writable.write().expect("writable lock");
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
            .insert(&second, make_test_meta(1), make_test_batch(1))
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
            .insert(&seal_key, make_test_meta(10), make_test_batch(1))
            .expect("first insert");
        let first = memtable.freeze(&seal_key).expect("first freeze");
        memtable
            .complete_staged(first.seal_id, staged_member(10))
            .expect("first complete");
        let first_bytes = memtable.stats().expect("first stats").immutable_bytes;

        // Second generation: still in PendingCommit — must not be retired.
        memtable
            .insert(&seal_key, make_test_meta(20), make_test_batch(1))
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
            .insert(&seal_key, make_test_meta(0), make_test_batch(1))
            .expect("insert");

        assert!(!memtable.should_seal(&seal_key).expect("should_seal"));
    }

    #[test]
    fn inactivity_does_not_trigger_generation_rotation() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        let batch = make_test_batch(1);
        memtable
            .insert(&seal_key, make_test_meta(0), batch)
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
            .insert(&first, make_test_meta(1), make_test_batch(1))
            .expect("first insert");
        memtable
            .insert(&second, make_test_meta(2), make_test_batch(1))
            .expect("second insert");
        {
            let mut buckets = memtable.writable.write().expect("writable lock");
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
            .insert(&key, make_test_meta(1), make_test_batch(1))
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

        let (size_registry, size) = exercise(1, Duration::from_mins(1));
        size.insert(&key, make_test_meta(1), make_test_batch(1))
            .expect("size member inserts");
        assert!(size.should_seal(&key).expect("size predicate"));
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
        age.insert(&key, make_test_meta(2), make_test_batch(1))
            .expect("age member inserts");
        assert!(age.should_seal(&key).expect("age predicate"));
        assert_eq!(
            age.freeze(&key).expect("age member freezes").shard_id,
            SHARD
        );

        let (_, pressure) = exercise(usize::MAX, Duration::from_mins(1));
        pressure
            .insert(&key, make_test_meta(3), make_test_batch(1))
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

        let (_, rotation) = exercise(usize::MAX, Duration::from_mins(1));
        let peer = SealKey::new(
            key.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "nonzero-peer"),
            key.partition.clone(),
        );
        for (candidate, lsn) in [(&key, 4), (&peer, 5)] {
            rotation
                .insert(candidate, make_test_meta(lsn), make_test_batch(1))
                .expect("rotation member inserts");
        }
        let cohort = rotation.freeze_all_nonempty().expect("full rotation");
        assert_eq!(cohort.len(), 2);
        assert!(cohort.iter().all(|member| member.shard_id == SHARD));
    }

    /// Two revisions of one partition can hold the same generation ordinal:
    /// recovery restores an old revision's staged member without advancing a
    /// fresh lane's counter, and the new revision's first freeze reuses the
    /// ordinal. Capture keys sources by their complete seal key, so both are
    /// selected once, and publishing one suppresses only its own memory copy.
    ///
    /// # Panics
    ///
    /// Panics when the freeze does not reuse the ordinal, a capture omits or
    /// duplicates either source, or publication suppresses the other key.
    #[test]
    fn revisions_sharing_an_ordinal_are_captured_by_full_key() {
        use crate::scribe::hot_source::{
            GenerationOrdinal, HotAuthority, RunFooters, ScribeHotSourceRegistry,
        };

        let registry = Arc::new(ScribeHotSourceRegistry::new());
        let old = make_test_seal_key();
        let new = old
            .clone()
            .with_table_version(old.table_version.next().expect("a next version"));
        let ordinal = GenerationOrdinal::new(0, 1);
        registry
            .restore_durable(
                &old,
                ordinal,
                HotAuthority::StagedRun {
                    member: crate::scribe::assembly::StagedMemberId::new(0, 1),
                    runs: vec![std::path::PathBuf::from("/stage/old-run.parquet")],
                    bytes: 1_024,
                    footers: RunFooters::default(),
                    wal: (WalLsn::new(1), WalLsn::new(9)),
                },
            )
            .expect("the old revision's staged member restores");
        let memtable = Memtable::new_with_config(usize::MAX, Duration::from_mins(1))
            .with_hot_sources(0, Arc::clone(&registry));
        memtable
            .insert(&new, make_test_meta(10), make_test_batch(3))
            .expect("the new revision inserts");
        let frozen = memtable.freeze(&new).expect("the new revision freezes");
        assert_eq!(
            GenerationOrdinal::new(0, frozen.seal_id),
            ordinal,
            "the fresh lane reuses the restored ordinal"
        );

        let selected = |capture: &LiveCapture| {
            let memory: Vec<_> = capture
                .memory
                .iter()
                .map(|source| (source.seal_key.clone(), source.batch_count()))
                .collect();
            let staged: Vec<_> = capture
                .staged
                .as_ref()
                .expect("a registry capture")
                .sources()
                .iter()
                .map(|source| (source.key.clone(), source.generation))
                .collect();
            (memory, staged)
        };
        let capture = LiveCapture::take([&memtable], new.tenant, &new.table, Some(&registry))
            .expect("locks")
            .expect("no publication is unsettled");
        assert_eq!(
            selected(&capture),
            (vec![(new.clone(), 1)], vec![(old.clone(), ordinal)]),
            "the new memory rows and the old staged run are each selected once"
        );
        drop(capture);

        registry
            .begin_publication(&new, &[ordinal])
            .expect("the new generation's publication is marked");
        registry
            .advance(
                &new,
                ordinal,
                HotAuthority::Published {
                    object_keys: vec!["objects/new.parquet".to_owned()],
                },
            )
            .expect("the new generation publishes");
        let capture = LiveCapture::take([&memtable], new.tenant, &new.table, Some(&registry))
            .expect("locks")
            .expect("the publication settled");
        assert_eq!(
            selected(&capture),
            (Vec::new(), vec![(old, ordinal)]),
            "publication suppresses only its own key's memory copy"
        );
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
            .insert(&key, make_test_meta(1), make_test_batch(1))
            .expect("insert");
        let buckets = memtable.writable.write().expect("writable lock");
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
    fn stale_pressure_key_is_a_noop() {
        assert!(Memtable::select_pressure_victims(Vec::new(), 1).is_empty());
    }

    #[test]
    fn memtable_freeze_produces_immutable_snapshot() {
        let memtable = Memtable::new();
        let seal_key = make_test_seal_key();

        // Insert 3 appends
        for i in 0..3 {
            let batch = make_test_batch(100);
            memtable
                .insert(&seal_key, make_test_meta(i), batch)
                .expect("insert");
        }

        // Freeze
        let frozen = memtable.freeze(&seal_key).expect("freeze");

        assert_eq!(frozen.row_count(), 300, "3 batches × 100 rows retained");
        assert_eq!(frozen.batches.len(), 3);
        assert_eq!(frozen.metas.len(), 3);
        assert_eq!(frozen.seal_key, seal_key);

        // Insert after freeze should go to a new bucket
        let batch = make_test_batch(50);
        memtable
            .insert(&seal_key, make_test_meta(10), batch)
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
            .insert(&seal_key, make_test_meta(1), make_test_batch(1))
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
            inserter_memtable.insert(&inserter_key, make_test_meta(2), make_test_batch(1))
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
                .insert(&key1, make_test_meta(i), make_test_batch(100))
                .expect("insert key1");
        }

        // Insert to key2
        for i in 0..2 {
            memtable
                .insert(&key2, make_test_meta(i), make_test_batch(50))
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
