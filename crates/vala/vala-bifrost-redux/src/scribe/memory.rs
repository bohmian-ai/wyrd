//! Pod-global memory accounting for Scribe.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::contracts::ScribeError;

/// Minimum supported cgroup memory size.
pub const MIN_MEMORY_BYTES: usize = 512 * 1024 * 1024;
const MIN_SCRIBE_BYTES: usize = 256 * 1024 * 1024;
const MAX_SCRIBE_BYTES: usize = 8 * 1024 * 1024 * 1024;
const MIN_BUCKET_BYTES: usize = 64 * 1024 * 1024;
const MAX_BUCKET_BYTES: usize = 512 * 1024 * 1024;
const CATEGORY_COUNT: usize = 8;

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
    /// Scribe soft reservation limit.
    pub scribe_limit_bytes: usize,
    /// Total charged bytes by category in enum order.
    pub categories: [usize; CATEGORY_COUNT],
}

impl MemorySnapshot {
    /// Return the sum of all category totals.
    #[must_use]
    pub fn total_bytes(self) -> usize {
        self.categories.iter().copied().sum()
    }
}

/// Shared parent governor for every Scribe shard.
#[derive(Debug, Clone)]
pub struct BifrostMemoryGovernor {
    inner: Arc<MemoryGovernorInner>,
}

#[derive(Debug)]
struct MemoryGovernorInner {
    pod_limit_bytes: usize,
    bifrost_limit_bytes: usize,
    scribe_limit_bytes: usize,
    scribe_total_bytes: AtomicUsize,
    bifrost_total_bytes: AtomicUsize,
    categories: [AtomicUsize; CATEGORY_COUNT],
}

impl BifrostMemoryGovernor {
    /// Construct a governor from detected cgroup memory `P`.
    pub fn new(pod_limit_bytes: usize) -> Result<Self, ScribeError> {
        if pod_limit_bytes < MIN_MEMORY_BYTES {
            return Err(ScribeError::Internal {
                detail: format!("cgroup memory limit must be at least {MIN_MEMORY_BYTES} bytes"),
            });
        }
        let bifrost_limit_bytes = pod_limit_bytes.saturating_mul(70) / 100;
        let scribe_limit_bytes =
            (pod_limit_bytes.saturating_mul(25) / 100).clamp(MIN_SCRIBE_BYTES, MAX_SCRIBE_BYTES);
        Ok(Self {
            inner: Arc::new(MemoryGovernorInner {
                pod_limit_bytes,
                bifrost_limit_bytes,
                scribe_limit_bytes,
                categories: std::array::from_fn(|_| AtomicUsize::new(0)),
                scribe_total_bytes: AtomicUsize::new(0),
                bifrost_total_bytes: AtomicUsize::new(0),
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

    /// Target size for one active bucket (`S / 4`) within the fixed bounds.
    #[must_use]
    pub fn active_bucket_target_bytes(&self) -> usize {
        (self.scribe_limit_bytes() / 4).clamp(MIN_BUCKET_BYTES, MAX_BUCKET_BYTES)
    }

    /// Reserve category bytes without waiting.
    pub fn try_reserve(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.scribe_limit_bytes())
    }

    /// Reserve ingress bytes below the 90% Scribe breaker.
    pub fn try_reserve_ingress(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.scribe_limit_bytes() * 90 / 100)
    }

    /// Reserve bounded maintenance bytes up to the full parent and child caps.
    pub fn try_reserve_maintenance(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_with_limit(category, bytes, self.scribe_limit_bytes())
    }

    fn try_reserve_with_limit(
        &self,
        category: MemoryCategory,
        bytes: usize,
        scribe_limit: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        reserve_with_limit(&self.inner.scribe_total_bytes, scribe_limit, bytes)?;
        if let Err(error) = reserve_with_limit(
            &self.inner.bifrost_total_bytes,
            self.bifrost_limit_bytes(),
            bytes,
        ) {
            self.inner
                .scribe_total_bytes
                .fetch_sub(bytes, Ordering::AcqRel);
            return Err(error);
        }
        self.inner.categories[category as usize].fetch_add(bytes, Ordering::AcqRel);
        Ok(MemoryReservation {
            governor: self.clone(),
            category,
            bytes,
        })
    }

    /// Read category totals.
    #[must_use]
    pub fn snapshot(&self) -> MemorySnapshot {
        MemorySnapshot {
            pod_limit_bytes: self.pod_limit_bytes(),
            bifrost_limit_bytes: self.bifrost_limit_bytes(),
            bifrost_total_bytes: self.inner.bifrost_total_bytes.load(Ordering::Acquire),
            scribe_limit_bytes: self.scribe_limit_bytes(),
            categories: std::array::from_fn(|index| {
                self.inner.categories[index].load(Ordering::Acquire)
            }),
        }
    }

    fn release(&self, category: MemoryCategory, bytes: usize) {
        self.inner.categories[category as usize].fetch_sub(bytes, Ordering::AcqRel);
        self.inner
            .scribe_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        self.inner
            .bifrost_total_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
    }
}

/// RAII category reservation.
#[derive(Debug)]
pub struct MemoryReservation {
    governor: BifrostMemoryGovernor,
    category: MemoryCategory,
    bytes: usize,
}

impl MemoryReservation {
    /// Current bytes owned by this reservation.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Resize this reservation while preserving category ownership.
    pub fn resize(&mut self, bytes: usize) -> Result<(), ScribeError> {
        if bytes > self.bytes {
            let extra = bytes - self.bytes;
            let replacement = self.governor.try_reserve(self.category, extra)?;
            self.bytes = bytes;
            std::mem::forget(replacement);
        } else {
            let released = self.bytes - bytes;
            self.governor.release(self.category, released);
            self.bytes = bytes;
        }
        Ok(())
    }

    /// Move accounting to another lifecycle category without changing totals.
    pub fn transfer_category(&mut self, category: MemoryCategory) {
        if self.category != category {
            self.governor.inner.categories[self.category as usize]
                .fetch_sub(self.bytes, Ordering::AcqRel);
            self.governor.inner.categories[category as usize]
                .fetch_add(self.bytes, Ordering::AcqRel);
        }
        self.category = category;
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
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
        assert_eq!(governor.active_bucket_target_bytes(), 64 * 1024 * 1024);
    }

    #[test]
    fn reservations_are_categorized_and_released() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let reservation = governor
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
            .try_reserve(MemoryCategory::Decode, 512)
            .expect("reserve");
        reservation.transfer_category(MemoryCategory::Prepared);
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.categories[MemoryCategory::Decode as usize], 0);
        assert_eq!(snapshot.categories[MemoryCategory::Prepared as usize], 512);
        assert_eq!(snapshot.bifrost_total_bytes, 512);
    }

    #[test]
    fn parent_limit_rejects_even_when_scribe_budget_has_room() {
        let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES).expect("valid memory");
        let first = governor
            .try_reserve_with_limit(
                MemoryCategory::Raw,
                governor.bifrost_limit_bytes(),
                usize::MAX,
            )
            .expect("parent budget");
        assert!(
            governor
                .try_reserve_with_limit(MemoryCategory::Raw, 1, usize::MAX)
                .is_err()
        );
        drop(first);
    }
}
