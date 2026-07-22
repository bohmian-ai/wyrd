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
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            max_items: 10_000,
            max_bytes: 512 * 1024 * 1024,
            max_writers: 4_096,
        }
    }
}

#[derive(Debug, Default)]
struct AdmissionState {
    items: usize,
    bytes: usize,
    writers: usize,
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
            max_items: self.inner.config.max_items,
            max_bytes: self.inner.config.max_bytes,
            max_writers: self.inner.config.max_writers,
        }
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
    /// Configured item limit.
    pub max_items: usize,
    /// Configured byte limit.
    pub max_bytes: usize,
    /// Configured writer limit.
    pub max_writers: usize,
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
        });
        let _reservation = admission.try_reserve("events", 10).expect("reserve");
        let error = admission.try_reserve("events", 1).expect_err("busy");
        assert!(matches!(error, ScribeError::IngestBusy { .. }));
        assert_eq!(admission.snapshot().items, 1);
        assert_eq!(admission.snapshot().bytes, 10);
    }
}
