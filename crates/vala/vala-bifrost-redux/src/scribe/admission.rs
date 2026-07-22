//! Pod-global Scribe admission accounting.
//!
//! Admission is deliberately separate from writer state. A request reserves
//! one item and one byte charge before it is split, serialized, or written.
//! The reservation stays attached to the accepted request until its consumer
//! finishes or drops it after a terminal error.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::contracts::ScribeError;

/// Maximum request size accepted by the Scribe seam.
pub const MAX_REQUEST_BYTES: usize = 33_554_432;

/// Fixed request overhead charged to every accepted append.
pub const REQUEST_OVERHEAD_BYTES: usize = 4 * 1024;

/// Default pod-wide admission limits.
#[derive(Debug, Clone, Copy)]
pub struct AdmissionConfig {
    /// Maximum accepted requests that have not completed processing.
    pub max_items: usize,
    /// Maximum byte charge for accepted requests.
    pub max_bytes: usize,
    /// Maximum lazily-created logical writers.
    pub max_writers: usize,
    /// Pod memory budget used by the 90% active/immutable breaker.
    pub memory_limit_bytes: usize,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            max_items: 10_000,
            max_bytes: 512 * 1024 * 1024,
            max_writers: 4_096,
            memory_limit_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Default)]
struct AdmissionState {
    items: usize,
    bytes: usize,
    writers: usize,
    active_bytes: usize,
    immutable_bytes: usize,
}

#[derive(Debug)]
struct AdmissionInner {
    config: AdmissionConfig,
    state: Mutex<AdmissionState>,
    wal_available: AtomicBool,
}

/// Pod-global admission controller.
#[derive(Debug, Clone)]
pub struct AdmissionController {
    inner: Arc<AdmissionInner>,
}

impl AdmissionController {
    /// Construct an admission controller with the production defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AdmissionConfig::default())
    }

    /// Construct an admission controller with explicit limits.
    #[must_use]
    pub fn with_config(config: AdmissionConfig) -> Self {
        Self {
            inner: Arc::new(AdmissionInner {
                config,
                state: Mutex::new(AdmissionState::default()),
                wal_available: AtomicBool::new(true),
            }),
        }
    }

    /// Reserve one request item and its byte charge without waiting.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngestBusy`] when either pod-global limit would
    /// be exceeded.
    pub fn try_reserve(
        &self,
        table: impl Into<String>,
        bytes: usize,
    ) -> Result<AdmissionReservation, ScribeError> {
        let table = table.into();
        if !self.inner.wal_available.load(Ordering::Acquire) {
            return Err(ScribeError::WalDiskFull);
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;

        let exceeds_items = state.items >= self.inner.config.max_items;
        let exceeds_bytes = bytes > self.inner.config.max_bytes.saturating_sub(state.bytes);
        if exceeds_items || exceeds_bytes {
            return Err(ScribeError::IngestBusy { table });
        }

        state.items += 1;
        state.bytes += bytes;
        drop(state);

        Ok(AdmissionReservation {
            inner: Some(Arc::clone(&self.inner)),
            bytes,
        })
    }

    /// Reserve one active logical-writer token without waiting.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngestBusy`] when the pod writer limit is full.
    pub fn try_reserve_writer(&self, table: impl Into<String>) -> Result<WriterLease, ScribeError> {
        let table = table.into();
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;
        if state.writers >= self.inner.config.max_writers {
            return Err(ScribeError::IngestBusy { table });
        }
        state.writers += 1;
        drop(state);

        Ok(WriterLease {
            inner: Some(Arc::clone(&self.inner)),
        })
    }

    /// Reserve retained Arrow bytes for an active memtable generation.
    ///
    /// The breaker is pod-global and trips at 90% of the configured memory
    /// budget. The caller must release the reservation if the memtable insert
    /// fails.
    pub fn try_reserve_active(
        &self,
        table: impl Into<String>,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let table = table.into();
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;
        let retained = state
            .active_bytes
            .saturating_add(state.immutable_bytes)
            .saturating_add(bytes);
        if retained > self.memory_breaker_bytes() {
            return Err(ScribeError::IngestBusy { table });
        }
        state.active_bytes = state.active_bytes.saturating_add(bytes);
        Ok(())
    }

    /// Release active Arrow bytes after a failed memtable insertion.
    pub fn release_active(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.active_bytes = state.active_bytes.saturating_sub(bytes);
        }
    }

    /// Transfer retained bytes from the active to immutable generation tier.
    pub fn transfer_active_to_immutable(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            let moved = bytes.min(state.active_bytes);
            state.active_bytes -= moved;
            state.immutable_bytes = state.immutable_bytes.saturating_add(moved);
        }
    }

    /// Release immutable bytes after a generation is retired.
    pub fn release_immutable(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.immutable_bytes = state.immutable_bytes.saturating_sub(bytes);
        }
    }

    /// Transfer retained bytes back to the active generation after a seal
    /// transaction rolls back.
    pub fn transfer_immutable_to_active(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            let moved = bytes.min(state.immutable_bytes);
            state.immutable_bytes -= moved;
            state.active_bytes = state.active_bytes.saturating_add(moved);
        }
    }

    /// Reconcile the counters with the authoritative memtable statistics.
    pub fn sync_memtable_bytes(&self, active_bytes: usize, immutable_bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.active_bytes = active_bytes;
            state.immutable_bytes = immutable_bytes;
        }
    }

    /// Return a point-in-time view of the pod admission counters.
    #[must_use]
    pub fn snapshot(&self) -> AdmissionSnapshot {
        let state = match self.inner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                tracing::error!("admission state lock poisoned while taking snapshot");
                poisoned.into_inner()
            }
        };
        AdmissionSnapshot {
            items: state.items,
            bytes: state.bytes,
            writers: state.writers,
            active_bytes: state.active_bytes,
            immutable_bytes: state.immutable_bytes,
            max_items: self.inner.config.max_items,
            max_bytes: self.inner.config.max_bytes,
            max_writers: self.inner.config.max_writers,
            memory_limit_bytes: self.inner.config.memory_limit_bytes,
            memory_breaker_bytes: self.memory_breaker_bytes(),
        }
    }

    fn memory_breaker_bytes(&self) -> usize {
        self.inner.config.memory_limit_bytes.saturating_mul(90) / 100
    }

    /// Trip the pod-wide WAL breaker after a terminal ENOSPC result.
    pub(crate) fn trip_wal_disk_full(&self) {
        self.inner.wal_available.store(false, Ordering::Release);
    }

    fn release_request(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.items = state.items.saturating_sub(1);
            state.bytes = state.bytes.saturating_sub(bytes);
        }
    }

    fn release_writer(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.writers = state.writers.saturating_sub(1);
        }
    }
}

impl Default for AdmissionController {
    fn default() -> Self {
        Self::new()
    }
}

/// A request's item and byte reservation.
#[derive(Debug)]
pub struct AdmissionReservation {
    inner: Option<Arc<AdmissionInner>>,
    bytes: usize,
}

impl AdmissionReservation {
    /// Return the request's charged byte count.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Release the reservation immediately.
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if let Some(inner) = self.inner.take() {
            AdmissionController { inner }.release_request(self.bytes);
        }
    }
}

impl Drop for AdmissionReservation {
    fn drop(&mut self) {
        self.release_inner();
    }
}

/// A writer-count reservation held by one registry entry.
#[derive(Debug)]
pub struct WriterLease {
    inner: Option<Arc<AdmissionInner>>,
}

impl Drop for WriterLease {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            AdmissionController { inner }.release_writer();
        }
    }
}

/// Read-only admission counters for inspection and telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionSnapshot {
    /// Accepted requests currently retained by Scribe.
    pub items: usize,
    /// Byte charge for accepted requests.
    pub bytes: usize,
    /// Active logical writers.
    pub writers: usize,
    /// Arrow bytes retained by writable memtables.
    pub active_bytes: usize,
    /// Arrow bytes retained by immutable generations.
    pub immutable_bytes: usize,
    /// Configured item limit.
    pub max_items: usize,
    /// Configured byte limit.
    pub max_bytes: usize,
    /// Configured writer limit.
    pub max_writers: usize,
    /// Configured pod memory budget.
    pub memory_limit_bytes: usize,
    /// 90% breaker threshold derived from the memory budget.
    pub memory_breaker_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_release_items_bytes_and_writers() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            max_items: 1,
            max_bytes: 10,
            max_writers: 1,
            memory_limit_bytes: 100,
        });
        let reservation = admission
            .try_reserve("vala.bifrost.events", 10)
            .expect("reserve");
        let writer = admission
            .try_reserve_writer("vala.bifrost.events")
            .expect("writer reserve");
        assert_eq!(admission.snapshot().items, 1);
        assert_eq!(admission.snapshot().bytes, 10);
        assert_eq!(admission.snapshot().writers, 1);
        drop(reservation);
        drop(writer);
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
        assert_eq!(admission.snapshot().writers, 0);
    }

    #[test]
    fn admission_rejects_before_mutation_when_full() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            max_items: 1,
            max_bytes: 10,
            max_writers: 1,
            memory_limit_bytes: 100,
        });
        let _reservation = admission.try_reserve("events", 10).expect("reserve");
        let error = admission.try_reserve("events", 1).expect_err("busy");
        assert!(matches!(error, ScribeError::IngestBusy { .. }));
        assert_eq!(admission.snapshot().items, 1);
        assert_eq!(admission.snapshot().bytes, 10);
    }

    #[test]
    fn active_and_immutable_bytes_use_the_ninety_percent_breaker() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            max_items: 10,
            max_bytes: 100,
            max_writers: 10,
            memory_limit_bytes: 100,
        });
        admission
            .try_reserve_active("events", 90)
            .expect("breaker allows exactly 90 percent");
        assert!(admission.try_reserve_active("events", 1).is_err());
        admission.transfer_active_to_immutable(90);
        assert_eq!(admission.snapshot().active_bytes, 0);
        assert_eq!(admission.snapshot().immutable_bytes, 90);
        admission.release_immutable(90);
        assert_eq!(admission.snapshot().immutable_bytes, 0);
    }
}
