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
/// Fixed pod-global in-flight item ceiling from the Scribe contract.
pub const GLOBAL_INFLIGHT_ITEMS: usize = 4_096;

/// Fixed pod-wide admission settings.
#[derive(Debug, Clone, Copy)]
pub struct AdmissionConfig {
    /// Pod memory budget used by the Scribe memory governor.
    pub memory_limit_bytes: usize,
    /// Optional explicit Scribe child budget under the detected pod budget.
    pub scribe_memory_limit_bytes: Option<usize>,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            memory_limit_bytes: 1024 * 1024 * 1024,
            scribe_memory_limit_bytes: None,
        }
    }
}

#[derive(Debug, Default)]
struct AdmissionState {
    items: usize,
    bytes: usize,
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
    ) -> Result<InflightFrameReservation, ScribeError> {
        self.try_reserve_kind(table, bytes)
    }

    fn try_reserve_kind(
        &self,
        table: impl Into<String>,
        bytes: usize,
    ) -> Result<InflightFrameReservation, ScribeError> {
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

        if state.items >= GLOBAL_INFLIGHT_ITEMS {
            return Err(ScribeError::IngestBusy { table });
        }

        state.items += 1;
        state.bytes += bytes;
        drop(state);

        Ok(InflightFrameReservation {
            inner: Some(Arc::clone(&self.inner)),
            bytes,
        })
    }

    /// Record Arrow bytes for an active memtable generation.
    ///
    /// The parent memory governor owns admission. These counters are a
    /// non-reserving ownership dimension used for inspection and flush
    /// selection; the caller must release them if insertion fails.
    pub fn try_reserve_active(
        &self,
        table: impl Into<String>,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let _table = table.into();
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;
        state.active_bytes = state.active_bytes.saturating_add(bytes);
        Ok(())
    }

    /// Release active Arrow bytes after a failed memtable insertion.
    pub fn release_active(&self, bytes: usize) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.active_bytes = state.active_bytes.saturating_sub(bytes);
        }
    }

    /// Transfer active bytes to the immutable generation tier.
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

    /// Transfer immutable bytes back to the active generation after a seal
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
            active_bytes: state.active_bytes,
            immutable_bytes: state.immutable_bytes,
            max_items: GLOBAL_INFLIGHT_ITEMS,
            memory_limit_bytes: self.inner.config.memory_limit_bytes,
            memory_breaker_bytes: self.memory_breaker_bytes(),
        }
    }

    /// Return the resolved admission configuration.
    #[must_use]
    pub fn config(&self) -> AdmissionConfig {
        self.inner.config
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
}

impl Default for AdmissionController {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical in-flight item and byte reservation carried by a shard queue.
#[derive(Debug)]
pub struct InflightFrameReservation {
    inner: Option<Arc<AdmissionInner>>,
    bytes: usize,
}

impl InflightFrameReservation {
    /// Return the in-flight byte charge.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Resize the request charge while retaining the same global item.
    pub fn resize(&mut self, bytes: usize) -> Result<(), ScribeError> {
        let Some(inner) = &self.inner else {
            return Err(ScribeError::Internal {
                detail: "in-flight admission reservation was already released".to_owned(),
            });
        };
        let mut state = inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned while resizing reservation".to_owned(),
        })?;
        if bytes >= self.bytes {
            let extra = bytes - self.bytes;
            state.bytes =
                state
                    .bytes
                    .checked_add(extra)
                    .ok_or_else(|| ScribeError::IngestBusy {
                        table: "vala.bifrost".to_owned(),
                    })?;
        } else {
            state.bytes = state.bytes.saturating_sub(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    /// Release the in-flight reservation immediately.
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if let Some(inner) = self.inner.take() {
            AdmissionController { inner }.release_request(self.bytes);
        }
    }
}

impl Drop for InflightFrameReservation {
    fn drop(&mut self) {
        self.release_inner();
    }
}

/// Read-only admission counters for inspection and telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionSnapshot {
    /// Accepted requests currently in flight through Scribe.
    pub items: usize,
    /// Byte charge for accepted requests.
    pub bytes: usize,
    /// Arrow bytes retained by writable memtables.
    pub active_bytes: usize,
    /// Arrow bytes retained by immutable generations.
    pub immutable_bytes: usize,
    /// Configured item limit.
    pub max_items: usize,
    /// Configured pod memory budget.
    pub memory_limit_bytes: usize,
    /// 90% breaker threshold derived from the memory budget.
    pub memory_breaker_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_release_global_items_and_bytes() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            memory_limit_bytes: 100,
            scribe_memory_limit_bytes: None,
        });
        let reservation = admission
            .try_reserve("vala.bifrost.events", 10)
            .expect("reserve");
        assert_eq!(admission.snapshot().items, 1);
        assert_eq!(admission.snapshot().bytes, 10);
        drop(reservation);
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
    }

    #[test]
    fn admission_rejects_after_fixed_global_item_bound() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            memory_limit_bytes: 100,
            scribe_memory_limit_bytes: None,
        });
        let mut reservations = Vec::with_capacity(GLOBAL_INFLIGHT_ITEMS);
        for _ in 0..GLOBAL_INFLIGHT_ITEMS {
            reservations.push(admission.try_reserve("events", 1).expect("reserve"));
        }
        let error = admission.try_reserve("events", 1).expect_err("busy");
        assert!(matches!(error, ScribeError::IngestBusy { .. }));
        assert_eq!(admission.snapshot().items, GLOBAL_INFLIGHT_ITEMS);
    }

    #[test]
    fn active_and_immutable_bytes_are_non_reserving_counters() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            memory_limit_bytes: 100,
            scribe_memory_limit_bytes: None,
        });
        admission
            .try_reserve_active("events", 90)
            .expect("record active bytes");
        admission
            .try_reserve_active("events", 1)
            .expect("counters do not reserve memory");
        admission.transfer_active_to_immutable(90);
        assert_eq!(admission.snapshot().active_bytes, 1);
        assert_eq!(admission.snapshot().immutable_bytes, 90);
        admission.release_immutable(90);
        assert_eq!(admission.snapshot().immutable_bytes, 0);
    }

    #[test]
    fn in_flight_resize_preserves_item_and_updates_bytes() {
        let admission = AdmissionController::default();
        let mut reservation = admission.try_reserve("events", 8).expect("reserve");
        reservation.resize(16).expect("resize");
        assert_eq!(admission.snapshot().items, 1);
        assert_eq!(admission.snapshot().bytes, 16);
        drop(reservation);
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
    }
}
