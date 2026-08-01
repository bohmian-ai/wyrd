//! Pod-global memory accounting for Scribe.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use datafusion::error::DataFusionError;
use datafusion::execution::memory_pool::{MemoryLimit, MemoryPool};
use num_traits::ToPrimitive;

use crate::contracts::ScribeError;

#[cfg(test)]
static CGROUP_CURRENT_TEST_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
thread_local! {
    static CGROUP_CURRENT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Minimum supported cgroup memory size.
pub const MIN_MEMORY_BYTES: usize = 512 * 1024 * 1024;
const MIN_SCRIBE_BYTES: usize = 256 * 1024 * 1024;
const MAX_SCRIBE_BYTES: usize = 8 * 1024 * 1024 * 1024;
const MIN_BUCKET_BYTES: usize = 64 * 1024 * 1024;
const MAX_BUCKET_BYTES: usize = 512 * 1024 * 1024;
const SHARD_ACCOUNTING_COUNT: usize = 16;
/// Number of bounded lifecycle memory categories.
pub const MEMORY_CATEGORY_COUNT: usize = 8;

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

/// Point-in-time category totals.
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

/// Shared parent governor for every Scribe shard.
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

#[derive(Debug)]
struct MemoryGovernorInner {
    pod_limit_bytes: usize,
    bifrost_limit_bytes: usize,
    scribe_limit_bytes: usize,
    scribe_total_bytes: AtomicUsize,
    bifrost_total_bytes: AtomicUsize,
    categories: [AtomicUsize; MEMORY_CATEGORY_COUNT],
    cgroup_limit_bytes: Option<usize>,
    cgroup_current: Mutex<Option<(Instant, Option<usize>)>>,
    shard_bytes: Arc<Vec<AtomicUsize>>,
}

impl BifrostMemoryGovernor {
    /// Construct a governor from detected cgroup memory `P`.
    pub fn new(pod_limit_bytes: usize) -> Result<Self, ScribeError> {
        Self::new_with_scribe_limit(pod_limit_bytes, None)
    }

    /// Construct a governor from pod memory `P` and an optional explicit
    /// Scribe child budget. The parent remains `70%` of `P`; an explicit child
    /// must remain below that parent and leave `256 MiB` outside Scribe.
    pub fn new_with_scribe_limit(
        pod_limit_bytes: usize,
        explicit_scribe_limit_bytes: Option<usize>,
    ) -> Result<Self, ScribeError> {
        if pod_limit_bytes < MIN_MEMORY_BYTES {
            return Err(ScribeError::Internal {
                detail: format!("cgroup memory limit must be at least {MIN_MEMORY_BYTES} bytes"),
            });
        }
        let bifrost_limit_bytes = pod_limit_bytes.saturating_mul(70) / 100;
        let scribe_limit_bytes = match explicit_scribe_limit_bytes {
            Some(value)
                if (MIN_SCRIBE_BYTES..=MAX_SCRIBE_BYTES).contains(&value)
                    && value < bifrost_limit_bytes
                    && value <= pod_limit_bytes.saturating_sub(MIN_SCRIBE_BYTES) =>
            {
                value
            }
            Some(value) => {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "explicit Scribe memory budget {value} must be at least {MIN_SCRIBE_BYTES}, below the Bifrost parent {bifrost_limit_bytes}, and leave {MIN_SCRIBE_BYTES} bytes outside Scribe"
                    ),
                });
            }
            None => {
                (pod_limit_bytes.saturating_mul(25) / 100).clamp(MIN_SCRIBE_BYTES, MAX_SCRIBE_BYTES)
            }
        };
        Ok(Self {
            inner: Arc::new(MemoryGovernorInner {
                pod_limit_bytes,
                bifrost_limit_bytes,
                scribe_limit_bytes,
                categories: std::array::from_fn(|_| AtomicUsize::new(0)),
                scribe_total_bytes: AtomicUsize::new(0),
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

    /// Return the Scribe reservation limit.
    #[must_use]
    pub fn scribe_limit_bytes(&self) -> usize {
        self.inner.scribe_limit_bytes
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

    /// Reserve bytes from the process-wide parent without charging Scribe.
    ///
    /// Oracle query execution and Forge workspaces use this path. Their live
    /// reservations contribute to the parent ceiling, while Scribe category
    /// totals remain reserved for Scribe-owned buffers only.
    pub fn try_reserve_parent(&self, bytes: usize) -> Result<ParentMemoryReservation, ScribeError> {
        self.try_reserve_parent_bytes(bytes)?;
        Ok(ParentMemoryReservation {
            governor: self.clone(),
            bytes,
        })
    }

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

    /// Read parent and child totals.
    #[must_use]
    pub fn snapshot(&self) -> MemorySnapshot {
        MemorySnapshot {
            pod_limit_bytes: self.pod_limit_bytes(),
            bifrost_limit_bytes: self.bifrost_limit_bytes(),
            bifrost_total_bytes: self.inner.bifrost_total_bytes.load(Ordering::Acquire),
            scribe_total_bytes: self.inner.scribe_total_bytes.load(Ordering::Acquire),
            scribe_limit_bytes: self.scribe_limit_bytes(),
            categories: std::array::from_fn(|index| {
                self.inner.categories[index].load(Ordering::Acquire)
            }),
            cgroup_current_bytes: self.cgroup_current(),
            cgroup_limit_bytes: self.inner.cgroup_limit_bytes,
        }
    }

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

    /// Reserve category bytes without waiting.
    pub fn try_reserve(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.limit_bytes())
    }

    /// Reserve ingress bytes below the 90% Scribe breaker.
    pub fn try_reserve_ingress(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.limit_bytes() * 90 / 100)
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

/// RAII reservation against the Bifrost parent that is not owned by Scribe.
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

/// `DataFusion` adapter backed by the process-wide Bifrost parent governor.
#[derive(Debug)]
pub struct BifrostDataFusionMemoryPool {
    governor: BifrostMemoryGovernor,
}

impl BifrostDataFusionMemoryPool {
    /// Create one Oracle pool view over the shared Bifrost parent.
    #[must_use]
    pub fn new(governor: BifrostMemoryGovernor) -> Self {
        Self { governor }
    }
}

impl MemoryPool for BifrostDataFusionMemoryPool {
    fn grow(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        additional: usize,
    ) {
        let reserved = self
            .governor
            .inner
            .bifrost_total_bytes
            .fetch_add(additional, Ordering::AcqRel)
            .saturating_add(additional);
        record_forge_memory(reservation, reserved, Some("accepted"));
    }

    fn shrink(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        shrink: usize,
    ) {
        let reserved = self
            .governor
            .inner
            .bifrost_total_bytes
            .fetch_sub(shrink, Ordering::AcqRel)
            .saturating_sub(shrink);
        record_forge_memory(reservation, reserved, None);
    }

    fn try_grow(
        &self,
        reservation: &datafusion::execution::memory_pool::MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        self.governor
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
            })
    }

    fn reserved(&self) -> usize {
        self.governor
            .inner
            .bifrost_total_bytes
            .load(Ordering::Acquire)
    }

    fn memory_limit(&self) -> MemoryLimit {
        MemoryLimit::Finite(self.governor.bifrost_limit_bytes())
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

    /// Resize an ingress reservation without crossing the 90% hard breaker.
    pub fn resize_ingress(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.resize_with_limit(bytes, self.governor.limit_bytes() * 90 / 100)
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

    #[test]
    fn explicit_scribe_budget_stays_under_parent_and_headroom() {
        let governor = BifrostMemoryGovernor::new_with_scribe_limit(
            1024 * 1024 * 1024,
            Some(300 * 1024 * 1024),
        )
        .expect("explicit Scribe budget");
        assert_eq!(governor.scribe_limit_bytes(), 300 * 1024 * 1024);
        assert!(
            BifrostMemoryGovernor::new_with_scribe_limit(
                1024 * 1024 * 1024,
                Some(800 * 1024 * 1024),
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

    #[test]
    fn parent_limit_rejects_even_when_scribe_budget_has_room() {
        let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES).expect("valid memory");
        let first = governor
            .try_reserve_parent(governor.bifrost_limit_bytes())
            .expect("parent budget");
        assert!(governor.try_reserve_parent(1).is_err());
        drop(first);
    }

    #[test]
    fn ninety_percent_pressure_rejects_before_wal_append() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let limit = governor.scribe_limit_bytes() * 90 / 100;
        let reservation = budget
            .try_reserve_ingress(MemoryCategory::Raw, limit)
            .expect("90% ingress reservation");
        assert!(budget.try_reserve_ingress(MemoryCategory::Raw, 1).is_err());
        drop(reservation);
    }

    #[test]
    fn parent_only_reservation_does_not_charge_scribe() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let reservation = governor.try_reserve_parent(4096).expect("parent reserve");
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.scribe_total_bytes, 0);
        assert_eq!(snapshot.bifrost_total_bytes, 4096);
        drop(reservation);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    #[test]
    fn scribe_forge_oracle_share_one_parent_limit() {
        let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES).expect("valid memory");
        let scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 200 * 1024 * 1024)
            .expect("Scribe reserve");
        let forge = governor
            .try_reserve_parent(100 * 1024 * 1024)
            .expect("Forge reserve");
        assert!(governor.try_reserve_parent(100 * 1024 * 1024).is_err());
        assert_eq!(governor.snapshot().scribe_total_bytes, 200 * 1024 * 1024);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 300 * 1024 * 1024);
        drop((scribe, forge));
    }

    #[test]
    fn datafusion_pool_shrink_releases_parent_memory() {
        use std::sync::Arc;

        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let pool: Arc<dyn MemoryPool> =
            Arc::new(BifrostDataFusionMemoryPool::new(governor.clone()));
        let consumer = MemoryConsumer::new("oracle-test");
        let reservation = consumer.register(&pool);
        reservation.try_grow(4096).expect("DataFusion reserve");
        assert_eq!(governor.snapshot().bifrost_total_bytes, 4096);
        reservation.try_shrink(4096).expect("DataFusion shrink");
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    #[test]
    fn concurrent_bifrost_roles_never_exceed_parent() {
        let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES).expect("valid memory");
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

    #[test]
    fn inspection_reconciles_parent_child_and_role_reservations() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let scribe = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Active, 1024)
            .expect("Scribe reserve");
        let oracle = governor.try_reserve_parent(2048).expect("Oracle reserve");
        let forge = governor.try_reserve_parent(4096).expect("Forge reserve");
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.total_bytes(), snapshot.scribe_total_bytes);
        assert_eq!(
            snapshot.bifrost_total_bytes,
            snapshot.scribe_total_bytes + oracle.bytes() + forge.bytes()
        );
        drop((scribe, oracle, forge));
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
}
