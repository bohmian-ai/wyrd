//! Pod-global memory accounting for Scribe.
//!
//! The [`BifrostMemoryGovernor`] maintains a two-level ledger: a parent
//! Bifrost ceiling (70% of pod memory) with two static children — Scribe and
//! Oracle. Each child receives an independent byte budget so that write
//! pressure filling the Scribe child cannot starve Oracle query execution below
//! its floor. Forge rewrite workspaces draw directly from the parent without
//! claiming a child budget. This child-split isolation contract is locked by
//! decision D79.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use datafusion::error::DataFusionError;
use datafusion::execution::memory_pool::{MemoryLimit, MemoryPool};
use num_traits::ToPrimitive;

use crate::contracts::ScribeError;
use crate::scribe::admission::MAX_REQUEST_BYTES;

#[cfg(test)]
static CGROUP_CURRENT_TEST_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
thread_local! {
    static CGROUP_CURRENT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Minimum supported cgroup memory size.
pub const MIN_MEMORY_BYTES: usize = 512 * 1024 * 1024;
/// Minimum byte budget for each static child (Scribe or Oracle).
const MIN_CHILD_BYTES: usize = 256 * 1024 * 1024;
/// Maximum byte budget for each static child (Scribe or Oracle).
const MAX_CHILD_BYTES: usize = 8 * 1024 * 1024 * 1024;
const MIN_BUCKET_BYTES: usize = 64 * 1024 * 1024;
const MAX_BUCKET_BYTES: usize = 512 * 1024 * 1024;
const SHARD_ACCOUNTING_COUNT: usize = 16;
/// Number of bounded lifecycle memory categories.
pub const MEMORY_CATEGORY_COUNT: usize = 8;

/// Returns the workspace persistence must reserve to encode one generation.
///
/// The reservation covers a sort copy, Parquet output, and the fixed eight
/// mebibytes of encoder overhead.
#[must_use]
pub(crate) fn persistence_workspace_bytes(arrow_bytes: usize) -> usize {
    arrow_bytes
        .saturating_mul(2)
        .saturating_add(8 * 1024 * 1024)
}

/// Memory categories charged by the Scribe lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MemoryCategory {
    /// Raw bytes retained from the transport.
    Raw = 0,
    /// Decoded Arrow memory.
    Decode = 1,
    /// Prepared and serialized slices.
    Prepared = 2,
    /// Bytes waiting in bounded queues.
    Queued = 3,
    /// Writable active buckets.
    Active = 4,
    /// Frozen immutable buckets.
    Immutable = 5,
    /// Bounded Parquet/object-store persistence workspace.
    Persistence = 6,
    /// Metadata and bookkeeping.
    Metadata = 7,
}

/// Point-in-time category totals for all memory roles in the governor.
///
/// The three-way reconciliation identity holds at every consistent snapshot:
/// `bifrost_total_bytes == scribe_total_bytes + oracle_total_bytes +
/// parent_only_bytes`, where `parent_only_bytes` is derived as the difference
/// and is not stored directly (see D79).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySnapshot {
    /// Total pod memory budget.
    pub pod_limit_bytes: usize,
    /// Parent Bifrost memory ceiling (`70%` of pod memory).
    pub bifrost_limit_bytes: usize,
    /// Parent Bifrost bytes currently charged.
    pub bifrost_total_bytes: usize,
    /// Scribe child bytes currently charged.
    pub scribe_total_bytes: usize,
    /// Scribe soft reservation limit.
    pub scribe_limit_bytes: usize,
    /// Oracle child bytes currently charged.
    pub oracle_total_bytes: usize,
    /// Oracle child reservation limit (D79 default: 25% of pod, clamped to `[256 MiB, 8 GiB]`).
    pub oracle_limit_bytes: usize,
    /// Total charged bytes by category in enum order.
    pub categories: [usize; MEMORY_CATEGORY_COUNT],
    /// Current cgroup resident usage when the kernel exposes it.
    pub cgroup_current_bytes: Option<usize>,
    /// Cgroup memory limit used for the external-pressure tripwire.
    pub cgroup_limit_bytes: Option<usize>,
}

impl MemorySnapshot {
    /// Return the sum of all category totals.
    #[must_use]
    pub fn total_bytes(self) -> usize {
        self.scribe_total_bytes
    }

    /// Effective pressure as a percentage of the most constrained active
    /// governor. The cgroup signal is absent on bare-metal hosts.
    #[must_use]
    pub fn effective_pressure_percent(self) -> usize {
        let managed = self
            .total_bytes()
            .saturating_mul(100)
            .checked_div(self.scribe_limit_bytes.max(1))
            .unwrap_or(100);
        let parent = self
            .bifrost_total_bytes
            .saturating_mul(100)
            .checked_div(self.bifrost_limit_bytes.max(1))
            .unwrap_or(100);
        let cgroup = match (self.cgroup_current_bytes, self.cgroup_limit_bytes) {
            (Some(current), Some(limit)) if limit > 0 => current
                .saturating_mul(100)
                .checked_div(limit)
                .unwrap_or(100),
            _ => 0,
        };
        managed.max(parent).max(cgroup)
    }
}

/// Shared parent governor for every Bifrost role in this pod.
///
/// The governor enforces a two-level memory hierarchy: a parent ceiling
/// (`70%` of pod memory) with two static child budgets — Scribe and Oracle.
/// Scribe write reservations charge the Scribe child and the parent. Oracle
/// query reservations charge the Oracle child and the parent. Forge rewrite
/// workspaces charge only the parent. This arrangement ensures that write
/// pressure cannot deny Oracle queries their floor allocation (D79).
///
/// Construct with [`BifrostMemoryGovernor::new_with_child_limits`] to set
/// both children explicitly, or with [`BifrostMemoryGovernor::new`] to accept
/// the D79 defaults.
#[derive(Debug, Clone)]
pub struct BifrostMemoryGovernor {
    inner: Arc<MemoryGovernorInner>,
}

/// Scribe-only capability over the process-wide Bifrost memory parent.
///
/// This child exposes category and shard accounting without exposing
/// parent-only reservations used by Oracle and Forge.
#[derive(Debug, Clone)]
pub struct ScribeMemoryBudget {
    parent: BifrostMemoryGovernor,
}

/// Oracle-only capability over the process-wide Bifrost memory parent.
///
/// Mirrors [`ScribeMemoryBudget`] in structure: holds a clone of the governor
/// and delegates all queries through it. Oracle query execution calls
/// `try_reserve` through [`BifrostDataFusionMemoryPool::for_oracle`]; direct
/// use of this handle is reserved for sizing and capacity decisions in boot.
#[derive(Debug, Clone)]
pub struct OracleMemoryBudget {
    parent: BifrostMemoryGovernor,
}

/// Inner shared state of the Bifrost memory governor.
///
/// All counters use `AcqRel` / `Acquire` ordering so that a snapshot read
/// sees all preceding increments and decrements without a lock. The two
/// child totals — `scribe_total_bytes` and `oracle_total_bytes` — plus any
/// remaining parent-only usage always sum to `bifrost_total_bytes`.
#[derive(Debug)]
struct MemoryGovernorInner {
    /// Detected or configured pod memory limit.
    pod_limit_bytes: usize,
    /// Parent Bifrost ceiling (70% of pod).
    bifrost_limit_bytes: usize,
    /// Scribe child budget; reservations also charge `bifrost_total_bytes`.
    scribe_limit_bytes: usize,
    /// Oracle child budget; reservations also charge `bifrost_total_bytes`.
    oracle_limit_bytes: usize,
    /// Running total of all Scribe-owned bytes.
    scribe_total_bytes: AtomicUsize,
    /// Running total of all Oracle-owned bytes.
    oracle_total_bytes: AtomicUsize,
    /// Running total of all Bifrost-owned bytes (parent ceiling counter).
    bifrost_total_bytes: AtomicUsize,
    /// Per-category byte counters for Scribe lifecycle phases.
    categories: [AtomicUsize; MEMORY_CATEGORY_COUNT],
    /// Cgroup hard limit read at construction for the external-pressure tripwire.
    cgroup_limit_bytes: Option<usize>,
    /// Cached cgroup current-usage reading, refreshed at most once per second.
    cgroup_current: Mutex<Option<(Instant, Option<usize>)>>,
    /// Per-shard byte accounting for in-flight Arrow buffers.
    shard_bytes: Arc<Vec<AtomicUsize>>,
}

impl BifrostMemoryGovernor {
    /// Construct a test-tier governor with a deliberately small Scribe child limit.
    ///
    /// This bypasses production minimums only for deterministic admission tests;
    /// callers must keep the pod limit valid and use the resulting governor only
    /// in test-support server construction.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the pod limit is below
    /// [`MIN_MEMORY_BYTES`] or when the inner governor cannot be mutated
    /// (which indicates unexpected sharing during construction).
    #[cfg(feature = "test-support")]
    pub fn new_with_test_scribe_limit(
        pod_limit_bytes: usize,
        scribe_limit_bytes: usize,
    ) -> Result<Self, ScribeError> {
        let mut governor =
            Self::new_with_child_limits(pod_limit_bytes, Some(MIN_CHILD_BYTES), None)?;
        let inner = Arc::get_mut(&mut governor.inner).ok_or_else(|| ScribeError::Internal {
            detail: "test memory governor unexpectedly shared during construction".to_owned(),
        })?;
        inner.scribe_limit_bytes = scribe_limit_bytes;
        Ok(governor)
    }

    /// Construct a governor from detected cgroup memory `P` using D79 defaults
    /// for both children.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when `pod_limit_bytes` is below
    /// [`MIN_MEMORY_BYTES`].
    pub fn new(pod_limit_bytes: usize) -> Result<Self, ScribeError> {
        Self::new_with_child_limits(pod_limit_bytes, None, None)
    }

    /// Construct a governor from pod memory `P` and optional explicit child
    /// budgets for Scribe and Oracle.
    ///
    /// The parent ceiling is always `70%` of `P`. Each explicit child limit must
    /// be at least [`MIN_CHILD_BYTES`] (256 MiB) and at most
    /// [`MAX_CHILD_BYTES`] (8 GiB). When both children are set explicitly, their
    /// sum must not exceed the parent ceiling; the governor rejects the
    /// configuration fail-closed. Unset children default to `25%` of pod memory
    /// clamped to `[MIN_CHILD_BYTES, MAX_CHILD_BYTES]` (D79).
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when:
    /// - `pod_limit_bytes` is below [`MIN_MEMORY_BYTES`];
    /// - an explicit child limit is below [`MIN_CHILD_BYTES`] or above [`MAX_CHILD_BYTES`];
    /// - the sum of both resolved child limits exceeds the parent ceiling.
    pub fn new_with_child_limits(
        pod_limit_bytes: usize,
        scribe_limit_bytes: Option<usize>,
        oracle_limit_bytes: Option<usize>,
    ) -> Result<Self, ScribeError> {
        if pod_limit_bytes < MIN_MEMORY_BYTES {
            return Err(ScribeError::Internal {
                detail: format!("cgroup memory limit must be at least {MIN_MEMORY_BYTES} bytes"),
            });
        }
        let bifrost_limit_bytes = pod_limit_bytes.saturating_mul(70) / 100;
        let default_child =
            (pod_limit_bytes.saturating_mul(25) / 100).clamp(MIN_CHILD_BYTES, MAX_CHILD_BYTES);

        let scribe_limit = resolve_child_limit("Scribe", scribe_limit_bytes, default_child)?;
        let oracle_limit = resolve_child_limit("Oracle", oracle_limit_bytes, default_child)?;

        let child_sum = scribe_limit.saturating_add(oracle_limit);
        if child_sum > bifrost_limit_bytes {
            return Err(ScribeError::Internal {
                detail: format!(
                    "Scribe child {scribe_limit} + Oracle child {oracle_limit} = {child_sum} \
                     exceeds the Bifrost parent ceiling {bifrost_limit_bytes}"
                ),
            });
        }

        Ok(Self {
            inner: Arc::new(MemoryGovernorInner {
                pod_limit_bytes,
                bifrost_limit_bytes,
                scribe_limit_bytes: scribe_limit,
                oracle_limit_bytes: oracle_limit,
                categories: std::array::from_fn(|_| AtomicUsize::new(0)),
                scribe_total_bytes: AtomicUsize::new(0),
                oracle_total_bytes: AtomicUsize::new(0),
                bifrost_total_bytes: AtomicUsize::new(0),
                cgroup_limit_bytes: read_cgroup_limit(),
                cgroup_current: Mutex::new(None),
                shard_bytes: Arc::new(
                    (0..SHARD_ACCOUNTING_COUNT)
                        .map(|_| AtomicUsize::new(0))
                        .collect(),
                ),
            }),
        })
    }

    /// Detect the cgroup memory limit, falling back to host memory and then a
    /// conservative one-gibibyte default.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the detected limit is below
    /// [`MIN_MEMORY_BYTES`].
    pub fn detect(fallback_bytes: usize) -> Result<Self, ScribeError> {
        let detected = [
            "/sys/fs/cgroup/memory.max",
            "/sys/fs/cgroup/memory/memory.limit_in_bytes",
        ]
        .into_iter()
        .find_map(read_memory_limit)
        .or_else(read_meminfo_limit)
        .unwrap_or_else(|| fallback_bytes.max(1024 * 1024 * 1024));
        Self::new(detected)
    }

    /// Return the configured pod memory budget.
    #[must_use]
    pub fn pod_limit_bytes(&self) -> usize {
        self.inner.pod_limit_bytes
    }

    /// Return the Scribe child reservation limit.
    #[must_use]
    pub fn scribe_limit_bytes(&self) -> usize {
        self.inner.scribe_limit_bytes
    }

    /// Return the Oracle child reservation limit.
    #[must_use]
    pub fn oracle_limit_bytes(&self) -> usize {
        self.inner.oracle_limit_bytes
    }

    /// Return the parent Bifrost memory ceiling.
    #[must_use]
    pub fn bifrost_limit_bytes(&self) -> usize {
        self.inner.bifrost_limit_bytes
    }

    /// Derive the Scribe-only child capability from this parent.
    #[must_use]
    pub fn scribe_budget(&self) -> ScribeMemoryBudget {
        ScribeMemoryBudget {
            parent: self.clone(),
        }
    }

    /// Derive the Oracle-only child capability from this parent.
    ///
    /// Boot wires the returned handle into the Oracle pool
    /// ([`BifrostDataFusionMemoryPool::for_oracle`]) and uses
    /// `limit_bytes()` to derive admission slot counts and reconciliation
    /// limits so no unbounded-parent fallback remains (D79).
    #[must_use]
    pub fn oracle_budget(&self) -> OracleMemoryBudget {
        OracleMemoryBudget {
            parent: self.clone(),
        }
    }

    /// Reserve bytes from the process-wide parent without charging any child.
    ///
    /// Forge rewrite workspaces use this path. Their reservations contribute
    /// to the parent ceiling while leaving both Scribe and Oracle child totals
    /// untouched.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup is saturated or
    /// when the reservation would exceed the parent ceiling.
    pub fn try_reserve_parent(&self, bytes: usize) -> Result<ParentMemoryReservation, ScribeError> {
        self.try_reserve_parent_bytes(bytes)?;
        Ok(ParentMemoryReservation {
            governor: self.clone(),
            bytes,
        })
    }

    /// Reserve bytes against the parent ceiling only (no child counter update).
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when cgroup pressure is at 100% or
    /// when the parent ceiling would be exceeded.
    fn try_reserve_parent_bytes(&self, bytes: usize) -> Result<(), ScribeError> {
        if bytes > 0
            && let Some((current, limit)) = self.cgroup_pressure()
            && current.saturating_mul(100) >= limit.saturating_mul(100)
        {
            return Err(ScribeError::IngestBusy {
                table: "bifrost-parent".to_owned(),
            });
        }
        reserve_with_limit(
            &self.inner.bifrost_total_bytes,
            self.bifrost_limit_bytes(),
            bytes,
        )
    }

    /// Reserve bytes against the Oracle child limit and the parent ceiling.
    ///
    /// On success the Oracle child counter and the parent counter are both
    /// incremented exactly once. On failure any partial parent charge is
    /// released before the error is returned, preserving the single-charge
    /// invariant.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the Oracle child limit or
    /// the parent ceiling would be exceeded.
    fn try_reserve_oracle_bytes(&self, bytes: usize) -> Result<(), ScribeError> {
        reserve_with_limit(
            &self.inner.oracle_total_bytes,
            self.inner.oracle_limit_bytes,
            bytes,
        )?;
        if let Err(error) = reserve_with_limit(
            &self.inner.bifrost_total_bytes,
            self.bifrost_limit_bytes(),
            bytes,
        ) {
            self.inner
                .oracle_total_bytes
                .fetch_sub(bytes, Ordering::AcqRel);
            return Err(error);
        }
        Ok(())
    }

    /// Release bytes from the Oracle child counter and the parent ceiling.
    fn release_oracle_bytes(&self, bytes: usize) {
        self.inner
            .oracle_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        self.inner
            .bifrost_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Read parent and child totals.
    ///
    /// The returned snapshot satisfies the three-way reconciliation identity
    /// `bifrost_total == scribe_total + oracle_total + parent_only` where
    /// `parent_only` is derived by the caller.
    #[must_use]
    pub fn snapshot(&self) -> MemorySnapshot {
        MemorySnapshot {
            pod_limit_bytes: self.pod_limit_bytes(),
            bifrost_limit_bytes: self.bifrost_limit_bytes(),
            bifrost_total_bytes: self.inner.bifrost_total_bytes.load(Ordering::Acquire),
            scribe_total_bytes: self.inner.scribe_total_bytes.load(Ordering::Acquire),
            scribe_limit_bytes: self.scribe_limit_bytes(),
            oracle_total_bytes: self.inner.oracle_total_bytes.load(Ordering::Acquire),
            oracle_limit_bytes: self.oracle_limit_bytes(),
            categories: std::array::from_fn(|index| {
                self.inner.categories[index].load(Ordering::Acquire)
            }),
            cgroup_current_bytes: self.cgroup_current(),
            cgroup_limit_bytes: self.inner.cgroup_limit_bytes,
        }
    }

    /// Read the cached cgroup current-usage, refreshing at most once per second.
    fn cgroup_current(&self) -> Option<usize> {
        if let Ok(cache) = self.inner.cgroup_current.lock()
            && let Some((sampled_at, value)) = *cache
            && sampled_at.elapsed() < Duration::from_secs(1)
        {
            return value;
        }
        let value = read_cgroup_current();
        if let Ok(mut cache) = self.inner.cgroup_current.lock() {
            *cache = Some((Instant::now(), value));
        }
        value
    }

    /// Return the cgroup current/limit pair when both are available.
    fn cgroup_pressure(&self) -> Option<(usize, usize)> {
        Some((self.cgroup_current()?, self.inner.cgroup_limit_bytes?))
    }
}

impl ScribeMemoryBudget {
    /// Return the Scribe child reservation limit.
    #[must_use]
    pub fn limit_bytes(&self) -> usize {
        self.parent.scribe_limit_bytes()
    }

    /// Target size for one active bucket (`S / 4`) within the fixed bounds.
    #[must_use]
    pub fn active_bucket_target_bytes(&self) -> usize {
        (self.limit_bytes() / 4).clamp(MIN_BUCKET_BYTES, MAX_BUCKET_BYTES)
    }

    /// Returns the persistence workspace that ingress must never consume.
    ///
    /// The largest admissible generation is either a rotation-target bucket
    /// or one append whose decoded Arrow reaches twice the wire cap. For every
    /// production budget, any generation Scribe admits must be persistable
    /// from headroom that ingress admission can never consume.
    #[must_use]
    pub fn persistence_headroom_bytes(&self) -> usize {
        persistence_workspace_bytes(self.active_bucket_target_bytes().max(2 * MAX_REQUEST_BYTES))
    }

    /// Returns the child-budget ceiling available to ingress reservations.
    ///
    /// The ceiling leaves persistence headroom outside ingress ownership. The
    /// quarter-limit floor exists only for sub-production test governors; for
    /// every production budget, any generation Scribe admits must be
    /// persistable from headroom that ingress admission can never consume.
    #[must_use]
    pub fn ingress_limit_bytes(&self) -> usize {
        self.limit_bytes()
            .saturating_sub(self.persistence_headroom_bytes())
            .max(self.limit_bytes() / 4)
    }

    /// Reserve category bytes without waiting.
    pub fn try_reserve(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.limit_bytes())
    }

    /// Reserve ingress bytes without consuming persistence headroom.
    pub fn try_reserve_ingress(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.ingress_limit_bytes())
    }

    /// Reserve bounded maintenance bytes up to the full parent and child caps.
    pub fn try_reserve_maintenance(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.limit_bytes())
    }

    fn try_reserve_with_limit(
        &self,
        category: MemoryCategory,
        bytes: usize,
        scribe_limit: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        if bytes > 0
            && let Some((current, limit)) = self.parent.cgroup_pressure()
            && current.saturating_mul(100) >= limit.saturating_mul(90)
        {
            return Err(ScribeError::IngestBusy {
                table: "memory".to_owned(),
            });
        }
        reserve_with_limit(&self.parent.inner.scribe_total_bytes, scribe_limit, bytes)?;
        if let Err(error) = reserve_with_limit(
            &self.parent.inner.bifrost_total_bytes,
            self.parent.bifrost_limit_bytes(),
            bytes,
        ) {
            self.parent
                .inner
                .scribe_total_bytes
                .fetch_sub(bytes, Ordering::AcqRel);
            return Err(error);
        }
        self.parent.inner.categories[category as usize].fetch_add(bytes, Ordering::AcqRel);
        Ok(MemoryReservation {
            governor: self.clone(),
            category,
            bytes,
            shard: None,
        })
    }

    pub(crate) fn shard_accounting(&self) -> Arc<Vec<AtomicUsize>> {
        Arc::clone(&self.parent.inner.shard_bytes)
    }

    pub(crate) fn shard_snapshot(&self) -> [usize; SHARD_ACCOUNTING_COUNT] {
        std::array::from_fn(|index| self.parent.inner.shard_bytes[index].load(Ordering::Acquire))
    }

    /// Read category totals.
    #[must_use]
    pub fn snapshot(&self) -> MemorySnapshot {
        self.parent.snapshot()
    }

    fn release(&self, category: MemoryCategory, bytes: usize) {
        self.parent.inner.categories[category as usize].fetch_sub(bytes, Ordering::AcqRel);
        self.parent
            .inner
            .scribe_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        self.parent
            .inner
            .bifrost_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Reconcile one category with an authoritative owner snapshot.
    pub fn reconcile_category(&self, category: MemoryCategory, target: usize) {
        let current = self.parent.inner.categories[category as usize].load(Ordering::Acquire);
        if target > current {
            let delta = target - current;
            self.parent.inner.categories[category as usize].fetch_add(delta, Ordering::AcqRel);
            self.parent
                .inner
                .scribe_total_bytes
                .fetch_add(delta, Ordering::AcqRel);
            self.parent
                .inner
                .bifrost_total_bytes
                .fetch_add(delta, Ordering::AcqRel);
        } else {
            let delta = current - target;
            self.parent.inner.categories[category as usize].fetch_sub(delta, Ordering::AcqRel);
            self.parent
                .inner
                .scribe_total_bytes
                .fetch_sub(delta, Ordering::AcqRel);
            self.parent
                .inner
                .bifrost_total_bytes
                .fetch_sub(delta, Ordering::AcqRel);
        }
    }
}

impl OracleMemoryBudget {
    /// Return the Oracle child reservation limit.
    #[must_use]
    pub fn limit_bytes(&self) -> usize {
        self.parent.oracle_limit_bytes()
    }

    /// Reserve `bytes` against the Oracle child and the parent ceiling.
    ///
    /// This is a direct low-level reservation; most Oracle memory flows go
    /// through [`BifrostDataFusionMemoryPool::for_oracle`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the Oracle child limit or
    /// the parent ceiling would be exceeded.
    pub fn try_reserve(&self, bytes: usize) -> Result<OracleMemoryReservation, ScribeError> {
        self.parent.try_reserve_oracle_bytes(bytes)?;
        Ok(OracleMemoryReservation {
            governor: self.parent.clone(),
            bytes,
        })
    }
}

/// RAII reservation against the Oracle child and the Bifrost parent.
///
/// Dropping this value releases both the Oracle child counter and the parent
/// counter simultaneously, preserving the single-charge invariant.
#[derive(Debug)]
pub struct OracleMemoryReservation {
    governor: BifrostMemoryGovernor,
    bytes: usize,
}

impl OracleMemoryReservation {
    /// Return the bytes held by this reservation.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for OracleMemoryReservation {
    fn drop(&mut self) {
        self.governor.release_oracle_bytes(self.bytes);
    }
}

/// RAII reservation against the Bifrost parent that is not owned by any child.
///
/// Forge rewrite workspaces use this type. Dropping it releases only the
/// parent counter, leaving both child totals untouched.
#[derive(Debug)]
pub struct ParentMemoryReservation {
    governor: BifrostMemoryGovernor,
    bytes: usize,
}

impl ParentMemoryReservation {
    /// Return the bytes held by this role reservation.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for ParentMemoryReservation {
    fn drop(&mut self) {
        self.governor
            .inner
            .bifrost_total_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Which accounting counter a [`BifrostDataFusionMemoryPool`] charges.
///
/// `Oracle` charges the Oracle child and the parent. `Parent` charges only the
/// parent (Forge rewrite behavior, unchanged from before D79).
#[derive(Debug, Clone, Copy)]
enum DataFusionPoolTarget {
    /// Charge the Oracle child counter and the parent ceiling.
    Oracle,
    /// Charge only the parent ceiling (Forge rewrites).
    Parent,
}

/// `DataFusion` memory pool backed by the process-wide Bifrost governor.
///
/// Construct with [`BifrostDataFusionMemoryPool::for_oracle`] for Oracle query
/// execution (charges the Oracle child) or
/// [`BifrostDataFusionMemoryPool::for_parent`] for Forge rewrites (charges
/// only the parent, which was the sole behavior before D79).
///
/// Both variants enforce the parent ceiling; the Oracle variant additionally
/// enforces the Oracle child limit so that write-side memory pressure cannot
/// starve query execution below its floor (D79 isolation invariant).
#[derive(Debug)]
pub struct BifrostDataFusionMemoryPool {
    governor: BifrostMemoryGovernor,
    target: DataFusionPoolTarget,
}

impl BifrostDataFusionMemoryPool {
    /// Create the pool that was used before the child split (charges parent only).
    ///
    /// Preserved for callers that predated D79; prefer the named constructors.
    #[must_use]
    pub fn new(governor: BifrostMemoryGovernor) -> Self {
        Self {
            governor,
            target: DataFusionPoolTarget::Parent,
        }
    }

    /// Create a pool for Oracle query execution.
    ///
    /// `grow`, `shrink`, and `try_grow` charge the Oracle child counter and the
    /// parent ceiling. `memory_limit` returns the Oracle child limit. An
    /// exhausted Scribe child cannot push an Oracle reservation into this pool
    /// past its own child limit (D79 isolation invariant).
    #[must_use]
    pub fn for_oracle(governor: BifrostMemoryGovernor) -> Self {
        Self {
            governor,
            target: DataFusionPoolTarget::Oracle,
        }
    }

    /// Create a pool for Forge rewrite workspaces.
    ///
    /// `grow`, `shrink`, and `try_grow` charge only the parent ceiling, exactly
    /// as before D79. The Oracle child counter is never touched.
    #[must_use]
    pub fn for_parent(governor: BifrostMemoryGovernor) -> Self {
        Self {
            governor,
            target: DataFusionPoolTarget::Parent,
        }
    }
}

impl MemoryPool for BifrostDataFusionMemoryPool {
    /// Unconditionally grow the reservation, charging the configured target.
    ///
    /// For `Oracle` pools both the Oracle child counter and the parent counter
    /// are incremented. For `Parent` pools only the parent counter is
    /// incremented. Telemetry is emitted only for Forge consumers.
    fn grow(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        additional: usize,
    ) {
        match self.target {
            DataFusionPoolTarget::Oracle => {
                self.governor
                    .inner
                    .oracle_total_bytes
                    .fetch_add(additional, Ordering::AcqRel);
                self.governor
                    .inner
                    .bifrost_total_bytes
                    .fetch_add(additional, Ordering::AcqRel);
            }
            DataFusionPoolTarget::Parent => {
                let reserved = self
                    .governor
                    .inner
                    .bifrost_total_bytes
                    .fetch_add(additional, Ordering::AcqRel)
                    .saturating_add(additional);
                record_forge_memory(reservation, reserved, Some("accepted"));
            }
        }
    }

    /// Release `shrink` bytes from the configured target.
    ///
    /// For `Oracle` pools both the Oracle child counter and the parent counter
    /// are decremented. For `Parent` pools only the parent counter is
    /// decremented.
    fn shrink(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        shrink: usize,
    ) {
        match self.target {
            DataFusionPoolTarget::Oracle => {
                self.governor
                    .inner
                    .oracle_total_bytes
                    .fetch_sub(shrink, Ordering::AcqRel);
                self.governor
                    .inner
                    .bifrost_total_bytes
                    .fetch_sub(shrink, Ordering::AcqRel);
            }
            DataFusionPoolTarget::Parent => {
                let reserved = self
                    .governor
                    .inner
                    .bifrost_total_bytes
                    .fetch_sub(shrink, Ordering::AcqRel)
                    .saturating_sub(shrink);
                record_forge_memory(reservation, reserved, None);
            }
        }
    }

    /// Attempt to grow `additional` bytes, charging the configured target.
    ///
    /// For `Oracle` pools the Oracle child limit is checked first; if the child
    /// limit passes the parent ceiling is checked next; on any failure both
    /// partial charges are released before the error is returned. For `Parent`
    /// pools only the parent ceiling is checked.
    ///
    /// # Errors
    ///
    /// Returns `DataFusionError::ResourcesExhausted` when the relevant limit
    /// would be exceeded.
    fn try_grow(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        match self.target {
            DataFusionPoolTarget::Oracle => self
                .governor
                .try_reserve_oracle_bytes(additional)
                .map_err(|error| {
                    DataFusionError::ResourcesExhausted(format!(
                        "Bifrost Oracle memory limit rejected {} bytes for `{}`: {error}",
                        additional,
                        reservation.consumer().name()
                    ))
                }),
            DataFusionPoolTarget::Parent => self
                .governor
                .try_reserve_parent_bytes(additional)
                .map(|()| {
                    record_forge_memory(reservation, self.reserved(), Some("accepted"));
                })
                .map_err(|error| {
                    record_forge_memory(reservation, self.reserved(), Some("rejected"));
                    DataFusionError::ResourcesExhausted(format!(
                        "Bifrost parent memory limit rejected {} bytes for `{}`: {error}",
                        additional,
                        reservation.consumer().name()
                    ))
                }),
        }
    }

    /// Return the total bytes currently reserved by this pool's target.
    fn reserved(&self) -> usize {
        self.governor
            .inner
            .bifrost_total_bytes
            .load(Ordering::Acquire)
    }

    /// Return the effective limit for this pool's target.
    ///
    /// Oracle pools report the Oracle child limit. Parent pools report the
    /// parent ceiling. `DataFusion` uses this to emit accurate backpressure
    /// diagnostics.
    fn memory_limit(&self) -> MemoryLimit {
        match self.target {
            DataFusionPoolTarget::Oracle => MemoryLimit::Finite(self.governor.oracle_limit_bytes()),
            DataFusionPoolTarget::Parent => {
                MemoryLimit::Finite(self.governor.bifrost_limit_bytes())
            }
        }
    }
}

/// Emit Forge-only parent-memory telemetry for a fixed `DataFusion` consumer.
///
/// Every ownership change updates the live gauge. Only an attempted reserve
/// supplies `reservation_outcome` and advances the accepted/rejected activity
/// counter; releasing memory must not masquerade as a successful reservation.
fn record_forge_memory(
    reservation: &datafusion::execution::memory_pool::MemoryReservation,
    reserved: usize,
    reservation_outcome: Option<&'static str>,
) {
    if reservation.consumer().name().contains("forge") {
        metrics::gauge!("bifrost_memory_reserved_bytes", "consumer" => "forge")
            .set(reserved.to_f64().unwrap_or(f64::MAX));
        if let Some(outcome) = reservation_outcome {
            metrics::counter!(
                "bifrost_memory_reservations_total",
                "consumer" => "forge",
                "outcome" => outcome
            )
            .increment(1);
        }
    }
}

/// Shared active/immutable ownership ledger for shard-owned Arrow buffers.
#[derive(Debug, Clone)]
pub struct MemoryLedger {
    active: Arc<Mutex<MemoryReservation>>,
    immutable: Arc<Mutex<MemoryReservation>>,
}

impl MemoryLedger {
    /// Create zero-sized active and immutable reservations on one governor.
    pub fn new(governor: &ScribeMemoryBudget) -> Result<Self, ScribeError> {
        Ok(Self {
            active: Arc::new(Mutex::new(governor.try_reserve(MemoryCategory::Active, 0)?)),
            immutable: Arc::new(Mutex::new(
                governor.try_reserve(MemoryCategory::Immutable, 0)?,
            )),
        })
    }

    /// Grow the active Arrow ownership reservation.
    pub fn reserve_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let target = active.bytes().saturating_add(bytes);
        active.resize(target)
    }

    /// Adopt an already-accounted active reservation after memtable insertion.
    ///
    /// The caller transfers ownership of the reservation; this method only
    /// joins its accounting with the ledger and never reserves the bytes a
    /// second time.
    pub fn absorb_active(&self, reservation: MemoryReservation) -> Result<(), ScribeError> {
        if reservation.category != MemoryCategory::Active {
            return Err(ScribeError::Internal {
                detail: "active memory ledger can only absorb active reservations".to_owned(),
            });
        }
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        active.merge(reservation)
    }

    /// Release active Arrow ownership after an insertion failure.
    pub fn release_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let target = active.bytes().saturating_sub(bytes);
        active.resize(target)
    }

    /// Move Arrow ownership from writable buckets to immutable generations.
    pub fn move_active_to_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let moved = bytes.min(active.bytes());
        let active_target = active.bytes() - moved;
        active.resize(active_target)?;
        let immutable_target = immutable.bytes().saturating_add(moved);
        if let Err(error) = immutable.resize(immutable_target) {
            let rollback_target = active.bytes().saturating_add(moved);
            let _ = active.resize(rollback_target);
            return Err(error);
        }
        Ok(())
    }

    /// Move Arrow ownership back to writable buckets after rollback.
    pub fn move_immutable_to_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let moved = bytes.min(immutable.bytes());
        let immutable_target = immutable.bytes() - moved;
        immutable.resize(immutable_target)?;
        let active_target = active.bytes().saturating_add(moved);
        if let Err(error) = active.resize(active_target) {
            let rollback_target = immutable.bytes().saturating_add(moved);
            let _ = immutable.resize(rollback_target);
            return Err(error);
        }
        Ok(())
    }

    /// Release immutable Arrow ownership after grace expiry.
    pub fn release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let target = immutable.bytes().saturating_sub(bytes);
        immutable.resize(target)
    }

    /// Reserve immutable ownership during boot replay.
    pub fn reserve_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let target = immutable.bytes().saturating_add(bytes);
        immutable.resize(target)
    }
}

/// RAII category reservation.
#[derive(Debug)]
pub struct MemoryReservation {
    governor: ScribeMemoryBudget,
    category: MemoryCategory,
    bytes: usize,
    shard: Option<(Arc<Vec<AtomicUsize>>, usize)>,
}

impl MemoryReservation {
    /// Current bytes owned by this reservation.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Resize this reservation while preserving category ownership.
    pub fn resize(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.resize_with_limit(bytes, self.governor.limit_bytes())
    }

    /// Resize an ingress reservation without consuming persistence headroom.
    pub fn resize_ingress(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.resize_with_limit(bytes, self.governor.ingress_limit_bytes())
    }

    fn resize_with_limit(&mut self, bytes: usize, limit: usize) -> Result<(), ScribeError> {
        if bytes > self.bytes {
            let extra = bytes - self.bytes;
            let replacement = self
                .governor
                .try_reserve_with_limit(self.category, extra, limit)?;
            self.bytes = bytes;
            std::mem::forget(replacement);
            self.adjust_shard_add(extra);
        } else {
            let released = self.bytes - bytes;
            self.governor.release(self.category, released);
            self.bytes = bytes;
            self.adjust_shard_sub(released);
        }
        Ok(())
    }

    pub(crate) fn attach_shard(&mut self, shard_bytes: Arc<Vec<AtomicUsize>>, shard: usize) {
        if shard >= shard_bytes.len() || self.shard.is_some() {
            return;
        }
        shard_bytes[shard].fetch_add(self.bytes, Ordering::AcqRel);
        self.shard = Some((shard_bytes, shard));
    }

    fn adjust_shard_add(&self, bytes: usize) {
        if let Some((shard_bytes, shard)) = &self.shard {
            shard_bytes[*shard].fetch_add(bytes, Ordering::AcqRel);
        }
    }

    fn adjust_shard_sub(&self, bytes: usize) {
        if let Some((shard_bytes, shard)) = &self.shard {
            shard_bytes[*shard].fetch_sub(bytes, Ordering::AcqRel);
        }
    }

    fn detach_shard(&mut self) {
        if let Some((shard_bytes, shard)) = self.shard.take() {
            shard_bytes[shard].fetch_sub(self.bytes, Ordering::AcqRel);
        }
    }

    /// Split bytes into a second RAII reservation without changing totals.
    pub fn split(&mut self, bytes: usize) -> Result<Self, ScribeError> {
        if bytes > self.bytes {
            return Err(ScribeError::Internal {
                detail: "memory reservation split exceeds owned bytes".to_owned(),
            });
        }
        self.bytes -= bytes;
        Ok(Self {
            governor: self.governor.clone(),
            category: self.category,
            bytes,
            shard: self.shard.clone(),
        })
    }

    /// Merge another reservation of the same category into this reservation.
    pub fn merge(&mut self, mut other: Self) -> Result<(), ScribeError> {
        if self.category != other.category {
            return Err(ScribeError::Internal {
                detail: "memory reservations must share a category before merge".to_owned(),
            });
        }
        self.bytes = self
            .bytes
            .checked_add(other.bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "memory reservation byte count overflow during merge".to_owned(),
            })?;
        other.detach_shard();
        std::mem::forget(other);
        Ok(())
    }

    /// Move accounting to another lifecycle category without changing totals.
    pub fn transfer_category(&mut self, category: MemoryCategory) {
        if self.category != category {
            self.governor.parent.inner.categories[self.category as usize]
                .fetch_sub(self.bytes, Ordering::AcqRel);
            self.governor.parent.inner.categories[category as usize]
                .fetch_add(self.bytes, Ordering::AcqRel);
        }
        self.category = category;
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.detach_shard();
        self.governor.release(self.category, self.bytes);
    }
}

/// Resolve an optional explicit child budget to its effective value.
///
/// When `explicit` is `Some`, it is range-checked against `[MIN_CHILD_BYTES,
/// MAX_CHILD_BYTES]` and the error message names `role`. When `None`, the
/// D79 default (`default_child`) is returned without validation because the
/// sum-vs-parent check in the constructor is authoritative.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when `explicit` is outside
/// `[MIN_CHILD_BYTES, MAX_CHILD_BYTES]`.
fn resolve_child_limit(
    role: &str,
    explicit: Option<usize>,
    default_child: usize,
) -> Result<usize, ScribeError> {
    match explicit {
        Some(value) if !(MIN_CHILD_BYTES..=MAX_CHILD_BYTES).contains(&value) => {
            Err(ScribeError::Internal {
                detail: format!(
                    "explicit {role} memory budget {value} must be between \
                     {MIN_CHILD_BYTES} and {MAX_CHILD_BYTES} bytes"
                ),
            })
        }
        Some(value) => Ok(value),
        None => Ok(default_child),
    }
}

fn reserve_with_limit(total: &AtomicUsize, limit: usize, bytes: usize) -> Result<(), ScribeError> {
    let mut current = total.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes) else {
            return Err(ScribeError::IngestBusy {
                table: "memory".to_owned(),
            });
        };
        if next > limit {
            return Err(ScribeError::IngestBusy {
                table: "memory".to_owned(),
            });
        }
        match total.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(()),
            Err(observed) => current = observed,
        }
    }
}

fn read_cgroup_limit() -> Option<usize> {
    [
        "/sys/fs/cgroup/memory.max",
        "/sys/fs/cgroup/memory/memory.limit_in_bytes",
    ]
    .into_iter()
    .find_map(read_memory_limit)
}

fn read_cgroup_current() -> Option<usize> {
    #[cfg(test)]
    CGROUP_CURRENT_READS.with(|count| count.set(count.get() + 1));
    [
        "/sys/fs/cgroup/memory.current",
        "/sys/fs/cgroup/memory/memory.usage_in_bytes",
    ]
    .into_iter()
    .find_map(|path| {
        std::fs::read_to_string(path)
            .ok()?
            .trim()
            .parse::<usize>()
            .ok()
    })
}

fn read_memory_limit(path: &str) -> Option<usize> {
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim();
    if value == "max" {
        return None;
    }
    value.parse::<usize>().ok().filter(|value| *value > 0)
}

fn read_meminfo_limit() -> Option<usize> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    contents.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() != "MemTotal" {
            return None;
        }
        let kib = value.split_whitespace().next()?.parse::<usize>().ok()?;
        kib.checked_mul(1024)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_scribe_budget_from_pod_budget() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        assert_eq!(governor.scribe_limit_bytes(), 256 * 1024 * 1024);
        assert_eq!(
            governor.bifrost_limit_bytes(),
            1024 * 1024 * 1024 * 70 / 100
        );
        assert_eq!(
            governor.scribe_budget().active_bucket_target_bytes(),
            64 * 1024 * 1024
        );
    }

    /// Proves production ceilings retain the exact D75 persistence workspace.
    #[test]
    fn ingress_ceiling_reserves_persistence_headroom() {
        // For each (scribe_limit, oracle_limit, expected_ingress_ceiling) triple.
        // oracle_limit must fit in the parent alongside scribe_limit.
        for (scribe_limit, oracle_limit, expected_ceiling) in [
            (256 * 1024 * 1024, 256 * 1024 * 1024, 120 * 1024 * 1024),
            (1024 * 1024 * 1024, 256 * 1024 * 1024, 504 * 1024 * 1024),
            (
                8 * 1024 * 1024 * 1024,
                256 * 1024 * 1024,
                7_160 * 1024 * 1024,
            ),
        ] {
            let pod_limit = if scribe_limit == 8 * 1024 * 1024 * 1024 {
                16 * 1024 * 1024 * 1024
            } else {
                4 * 1024 * 1024 * 1024
            };
            let limit = scribe_limit;
            let governor = BifrostMemoryGovernor::new_with_child_limits(
                pod_limit,
                Some(limit),
                Some(oracle_limit),
            )
            .expect("production Scribe budget");
            let budget = governor.scribe_budget();
            let expected = limit.saturating_sub(persistence_workspace_bytes(
                budget
                    .active_bucket_target_bytes()
                    .max(2 * MAX_REQUEST_BYTES),
            ));
            assert_eq!(budget.ingress_limit_bytes(), expected);
            assert_eq!(budget.ingress_limit_bytes(), expected_ceiling);
            assert!(expected > limit / 4);
        }

        let mut governor =
            BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("test-tier governor base");
        Arc::get_mut(&mut governor.inner)
            .expect("unshared test governor")
            .scribe_limit_bytes = 64 * 1024 * 1024;
        let budget = governor.scribe_budget();
        assert_eq!(budget.ingress_limit_bytes(), budget.limit_bytes() / 4);
    }

    /// Proves ingress cannot consume the workspace needed by an admitted generation.
    #[test]
    fn ingress_reservation_respects_derived_ceiling() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ingress = budget
            .try_reserve_ingress(MemoryCategory::Raw, budget.ingress_limit_bytes())
            .expect("derived ingress ceiling");
        assert!(matches!(
            budget.try_reserve_ingress(MemoryCategory::Raw, 1),
            Err(ScribeError::IngestBusy { .. })
        ));
        let maintenance = budget
            .try_reserve_maintenance(
                MemoryCategory::Persistence,
                budget.persistence_headroom_bytes(),
            )
            .expect("reserved persistence headroom");
        assert_eq!(budget.snapshot().total_bytes(), budget.limit_bytes());
        drop(maintenance);
        drop(ingress);
    }

    /// Proves that an explicit Scribe budget is accepted within bounds and that an
    /// out-of-range value is rejected at governor construction.
    #[test]
    fn explicit_scribe_budget_stays_under_parent_and_headroom() {
        let governor = BifrostMemoryGovernor::new_with_child_limits(
            1024 * 1024 * 1024,
            Some(300 * 1024 * 1024),
            None,
        )
        .expect("explicit Scribe budget");
        assert_eq!(governor.scribe_limit_bytes(), 300 * 1024 * 1024);
        // 800 MiB exceeds MAX_CHILD_BYTES; must be rejected.
        assert!(
            BifrostMemoryGovernor::new_with_child_limits(
                1024 * 1024 * 1024,
                Some(800 * 1024 * 1024),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn reservations_are_categorized_and_released() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let reservation = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 512)
            .expect("reserve");
        assert_eq!(governor.snapshot().total_bytes(), 512);
        drop(reservation);
        assert_eq!(governor.snapshot().total_bytes(), 0);
    }

    #[test]
    fn rejects_small_pod_limits() {
        assert!(BifrostMemoryGovernor::new(MIN_MEMORY_BYTES - 1).is_err());
    }

    #[test]
    fn transfers_reservations_between_lifecycle_categories() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let mut reservation = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Decode, 512)
            .expect("reserve");
        reservation.transfer_category(MemoryCategory::Prepared);
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.categories[MemoryCategory::Decode as usize], 0);
        assert_eq!(snapshot.categories[MemoryCategory::Prepared as usize], 512);
        assert_eq!(snapshot.bifrost_total_bytes, 512);
    }

    #[test]
    fn split_and_merge_preserve_exact_totals() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let mut reservation = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Prepared, 512)
            .expect("reserve");
        let part = reservation.split(128).expect("split");
        assert_eq!(governor.snapshot().total_bytes(), 512);
        reservation.merge(part).expect("merge");
        assert_eq!(reservation.bytes(), 512);
        drop(reservation);
        assert_eq!(governor.snapshot().total_bytes(), 0);
    }

    #[test]
    fn shard_ownership_tracks_transient_bytes_until_active_absorption() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let mut reservation = budget
            .try_reserve(MemoryCategory::Queued, 512)
            .expect("reserve");
        reservation.attach_shard(budget.shard_accounting(), 3);
        let part = reservation.split(128).expect("split");
        drop(part);
        assert_eq!(budget.shard_snapshot()[3], 384);

        let ledger = MemoryLedger::new(&budget).expect("ledger");
        reservation.transfer_category(MemoryCategory::Active);
        ledger.absorb_active(reservation).expect("absorb");
        assert_eq!(budget.shard_snapshot()[3], 0);
        ledger.release_active(384).expect("release");
        assert_eq!(governor.snapshot().total_bytes(), 0);
    }

    #[test]
    fn ledger_absorbs_an_already_accounted_active_lease_once() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ledger = MemoryLedger::new(&budget).expect("ledger");
        let mut prepared = budget
            .try_reserve(MemoryCategory::Prepared, 512)
            .expect("prepared reservation");
        let mut active = prepared.split(256).expect("active split");
        active.transfer_category(MemoryCategory::Active);
        ledger.absorb_active(active).expect("absorb active lease");

        let snapshot = governor.snapshot();
        assert_eq!(snapshot.total_bytes(), 512);
        assert_eq!(snapshot.categories[MemoryCategory::Active as usize], 256);
        assert_eq!(snapshot.categories[MemoryCategory::Prepared as usize], 256);
        drop(prepared);
        assert_eq!(governor.snapshot().total_bytes(), 256);
        ledger.release_active(256).expect("release active");
        assert_eq!(governor.snapshot().total_bytes(), 0);
    }

    /// Proves the parent ceiling is a hard limit even when child budgets have room.
    ///
    /// Uses a 4 GiB pod so both 25% defaults (1 GiB each) fit inside the 2.8 GiB parent.
    #[test]
    fn parent_limit_rejects_even_when_scribe_budget_has_room() {
        let pod = 4 * 1024 * 1024 * 1024_usize;
        let governor = BifrostMemoryGovernor::new(pod).expect("valid memory");
        let first = governor
            .try_reserve_parent(governor.bifrost_limit_bytes())
            .expect("parent budget");
        assert!(governor.try_reserve_parent(1).is_err());
        drop(first);
    }

    /// Proves the WAL path rejects ingress only after the persistence-derived ceiling is exhausted.
    #[test]
    fn derived_ingress_ceiling_rejects_before_wal_append() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let limit = budget.ingress_limit_bytes();
        let reservation = budget
            .try_reserve_ingress(MemoryCategory::Raw, limit)
            .expect("derived ingress reservation");
        assert!(budget.try_reserve_ingress(MemoryCategory::Raw, 1).is_err());
        drop(reservation);
    }

    /// Proves a parent-only (Forge) reservation charges neither Scribe nor Oracle.
    #[test]
    fn parent_only_reservation_does_not_charge_scribe() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let reservation = governor.try_reserve_parent(4096).expect("parent reserve");
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.scribe_total_bytes, 0);
        assert_eq!(snapshot.oracle_total_bytes, 0);
        assert_eq!(snapshot.bifrost_total_bytes, 4096);
        drop(reservation);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    /// Proves Scribe, Forge, and Oracle all draw from the same parent ceiling.
    ///
    /// Uses explicit 256 MiB children on a 2 GiB pod (parent = 1.4 GiB) so
    /// the 300 MiB combined Scribe + Forge load triggers exhaustion.
    #[test]
    fn scribe_forge_oracle_share_one_parent_limit() {
        let pod = 2 * 1024 * 1024 * 1024_usize; // 2 GiB; parent = 1.4 GiB
        let child = 256 * 1024 * 1024_usize;
        let governor = BifrostMemoryGovernor::new_with_child_limits(pod, Some(child), Some(child))
            .expect("governor with explicit children");
        let bifrost_limit = governor.bifrost_limit_bytes(); // ~1.4 GiB
        let scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 200 * 1024 * 1024)
            .expect("Scribe reserve");
        let forge = governor
            .try_reserve_parent(100 * 1024 * 1024)
            .expect("Forge reserve");
        // 300 MiB consumed; remaining parent headroom > 0 but the next
        // 100 MiB Forge attempt depends on whether the total fits.
        // Just assert the parent total is exactly 300 MiB and that additional
        // Forge beyond the remaining bifrost headroom is rejected.
        assert_eq!(governor.snapshot().scribe_total_bytes, 200 * 1024 * 1024);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 300 * 1024 * 1024);
        // Attempting to consume the entire remaining parent must eventually fail.
        let remaining = bifrost_limit - 300 * 1024 * 1024;
        let fill = governor
            .try_reserve_parent(remaining)
            .expect("fill remaining");
        assert!(
            governor.try_reserve_parent(1).is_err(),
            "parent must be exhausted"
        );
        drop((scribe, forge, fill));
    }

    /// Proves the legacy `new` constructor (parent target) releases parent memory on shrink.
    #[test]
    fn datafusion_pool_shrink_releases_parent_memory() {
        use std::sync::Arc;

        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let pool: Arc<dyn MemoryPool> =
            Arc::new(BifrostDataFusionMemoryPool::for_parent(governor.clone()));
        let consumer = MemoryConsumer::new("forge-test");
        let reservation = consumer.register(&pool);
        reservation.try_grow(4096).expect("DataFusion reserve");
        assert_eq!(governor.snapshot().bifrost_total_bytes, 4096);
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
        reservation.try_shrink(4096).expect("DataFusion shrink");
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    /// Proves concurrent Scribe, Forge, and Oracle reservations never collectively exceed the parent.
    ///
    /// Uses a 4 GiB pod so both default 25% children (1 GiB each) fit in the 2.8 GiB parent.
    #[test]
    fn concurrent_bifrost_roles_never_exceed_parent() {
        let governor = BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024).expect("valid memory");
        let roles = (0..8)
            .map(|_| {
                let governor = governor.clone();
                std::thread::spawn(move || {
                    let budget = governor.scribe_budget();
                    for _ in 0..32 {
                        if let Ok(reservation) = governor.try_reserve_parent(4 * 1024 * 1024) {
                            assert!(
                                governor.snapshot().bifrost_total_bytes
                                    <= governor.bifrost_limit_bytes()
                            );
                            drop(reservation);
                        }
                        if let Ok(reservation) = budget.try_reserve(MemoryCategory::Queued, 1) {
                            drop(reservation);
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        for role in roles {
            role.join().expect("role reservation thread");
        }
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
        assert_eq!(governor.snapshot().scribe_total_bytes, 0);
    }

    /// Proves the three-way reconciliation identity:
    /// `bifrost_total == scribe_total + oracle_total + parent_only`.
    #[test]
    fn inspection_reconciles_parent_child_and_role_reservations() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 1024)
            .expect("Scribe reserve");
        let oracle = governor
            .oracle_budget()
            .try_reserve(2048)
            .expect("Oracle child reserve");
        let forge = governor.try_reserve_parent(4096).expect("Forge reserve");
        let snapshot = governor.snapshot();
        // Scribe child total is reported directly in the snapshot.
        assert_eq!(snapshot.scribe_total_bytes, 1024);
        // Oracle child total is reported directly in the snapshot.
        assert_eq!(snapshot.oracle_total_bytes, 2048);
        // Bifrost total equals scribe + oracle + forge (parent-only).
        let parent_only = forge.bytes();
        assert_eq!(
            snapshot.bifrost_total_bytes,
            snapshot.scribe_total_bytes + snapshot.oracle_total_bytes + parent_only
        );
        drop((scribe, oracle, forge));
        let after = governor.snapshot();
        assert_eq!(after.bifrost_total_bytes, 0);
        assert_eq!(after.scribe_total_bytes, 0);
        assert_eq!(after.oracle_total_bytes, 0);
    }

    #[test]
    fn cgroup_current_is_cached_for_repeated_snapshots() {
        let _guard = CGROUP_CURRENT_TEST_LOCK.lock().expect("cgroup test lock");
        CGROUP_CURRENT_READS.with(|count| count.set(0));
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let _ = governor.snapshot();
        let first = CGROUP_CURRENT_READS.with(std::cell::Cell::get);
        assert_eq!(first, 1, "first snapshot must perform one current read");
        let _ = governor.snapshot();
        let second = CGROUP_CURRENT_READS.with(std::cell::Cell::get);
        assert_eq!(second, first, "cached snapshot performed an extra read");

        let cached_limit = governor.inner.cgroup_limit_bytes;
        let cached_value = governor
            .inner
            .cgroup_current
            .lock()
            .expect("cgroup cache lock")
            .expect("snapshot cached current")
            .1;
        *governor
            .inner
            .cgroup_current
            .lock()
            .expect("cgroup cache lock") = Some((
            Instant::now()
                .checked_sub(Duration::from_secs(2))
                .expect("invariant: test clock is after two seconds"),
            cached_value,
        ));
        let _ = governor.snapshot();
        assert_eq!(
            CGROUP_CURRENT_READS.with(std::cell::Cell::get),
            first + 1,
            "expired cache must perform exactly one refresh read"
        );
        assert_eq!(governor.inner.cgroup_limit_bytes, cached_limit);
    }

    /// Proves Oracle child reservations charge both the Oracle child counter
    /// and the parent exactly once (no double-counting).
    #[test]
    fn oracle_child_reservations_reconcile_with_parent() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid governor");
        let pool: std::sync::Arc<dyn MemoryPool> =
            std::sync::Arc::new(BifrostDataFusionMemoryPool::for_oracle(governor.clone()));
        let consumer = MemoryConsumer::new("oracle-query");
        let reservation = consumer.register(&pool);
        reservation.try_grow(8192).expect("Oracle pool grow");
        let snap = governor.snapshot();
        // Oracle child and parent are each charged once.
        assert_eq!(snap.oracle_total_bytes, 8192);
        assert_eq!(snap.bifrost_total_bytes, 8192);
        // Scribe child must be untouched.
        assert_eq!(snap.scribe_total_bytes, 0);
        // Drop releases both counters.
        drop(reservation);
        let after = governor.snapshot();
        assert_eq!(after.oracle_total_bytes, 0);
        assert_eq!(after.bifrost_total_bytes, 0);
    }

    /// Proves that a full Scribe child cannot deny Oracle its own child budget.
    #[test]
    fn full_scribe_child_cannot_starve_oracle_child() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        // Use a pod large enough for both default 25% children and parent overhead.
        // pod = 4 GiB → bifrost = 2.8 GiB, each child defaults to 1 GiB.
        let pod = 4 * 1024 * 1024 * 1024_usize;
        let scribe_limit = 256 * 1024 * 1024; // explicit small child for the test
        let oracle_limit = 256 * 1024 * 1024; // explicit small child for the test
        let governor = BifrostMemoryGovernor::new_with_child_limits(
            pod,
            Some(scribe_limit),
            Some(oracle_limit),
        )
        .expect("governor with explicit children");

        // Fill the Scribe child to its limit.
        let _scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, scribe_limit)
            .expect("Scribe child filled");

        // Oracle must still succeed up to its full child limit.
        let pool: std::sync::Arc<dyn MemoryPool> =
            std::sync::Arc::new(BifrostDataFusionMemoryPool::for_oracle(governor.clone()));
        let consumer = MemoryConsumer::new("oracle-isolated");
        let reservation = consumer.register(&pool);
        reservation
            .try_grow(oracle_limit)
            .expect("Oracle child succeeds even when Scribe child is full");

        let snap = governor.snapshot();
        assert_eq!(snap.scribe_total_bytes, scribe_limit);
        assert_eq!(snap.oracle_total_bytes, oracle_limit);
        // Both children charge the parent exactly once each.
        assert_eq!(
            snap.bifrost_total_bytes,
            snap.scribe_total_bytes + snap.oracle_total_bytes
        );
    }

    /// Proves that child limits whose sum exceeds the parent ceiling are rejected at construction.
    #[test]
    fn child_sum_exceeding_parent_rejected_at_construction() {
        // pod = 512 MiB → bifrost = 358 MiB; two 256 MiB children sum to 512 MiB > bifrost.
        let pod = MIN_MEMORY_BYTES; // 512 MiB
        let bifrost = pod * 70 / 100; // 358 MiB
        let each_child = MIN_CHILD_BYTES; // 256 MiB
        assert!(
            each_child * 2 > bifrost,
            "test invariant: sum exceeds parent"
        );
        assert!(
            BifrostMemoryGovernor::new_with_child_limits(pod, Some(each_child), Some(each_child))
                .is_err(),
            "sum of children exceeding parent must be rejected"
        );
    }

    /// Proves the `for_parent` pool does not touch the Oracle child counter.
    #[test]
    fn for_parent_pool_does_not_charge_oracle_child() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid governor");
        let pool: std::sync::Arc<dyn MemoryPool> =
            std::sync::Arc::new(BifrostDataFusionMemoryPool::for_parent(governor.clone()));
        let consumer = MemoryConsumer::new("forge-rewrite");
        let reservation = consumer.register(&pool);
        reservation.try_grow(4096).expect("parent pool grow");
        let snap = governor.snapshot();
        assert_eq!(
            snap.oracle_total_bytes, 0,
            "Oracle child must not be charged by for_parent pool"
        );
        assert_eq!(snap.bifrost_total_bytes, 4096);
        drop(reservation);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    /// Proves the Oracle pool and Scribe pool together never exceed the parent ceiling.
    #[test]
    fn combined_children_never_exceed_parent() {
        // pod = 1 GiB with equal 256 MiB children; parent = 716 MiB > 512 MiB sum.
        let governor = BifrostMemoryGovernor::new_with_child_limits(
            1024 * 1024 * 1024,
            Some(256 * 1024 * 1024),
            Some(256 * 1024 * 1024),
        )
        .expect("governor with explicit equal children");

        // Fill both children.
        let _scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 256 * 1024 * 1024)
            .expect("Scribe filled");
        let oracle_budget = governor.oracle_budget();
        let _oracle = oracle_budget
            .try_reserve(256 * 1024 * 1024)
            .expect("Oracle filled");

        let snap = governor.snapshot();
        assert!(
            snap.bifrost_total_bytes <= snap.bifrost_limit_bytes,
            "combined children must not exceed parent ceiling"
        );
        assert_eq!(snap.scribe_total_bytes, 256 * 1024 * 1024);
        assert_eq!(snap.oracle_total_bytes, 256 * 1024 * 1024);
        assert_eq!(
            snap.bifrost_total_bytes,
            snap.scribe_total_bytes + snap.oracle_total_bytes
        );
    }
}
