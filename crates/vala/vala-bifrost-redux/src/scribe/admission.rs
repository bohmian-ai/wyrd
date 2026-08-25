//! Pod-global Scribe admission accounting.
//!
//! Admission is deliberately separate from writer state. A request reserves
//! one item and one byte charge before it is split, serialized, or written.
//! The reservation stays attached to the accepted request until its consumer
//! finishes or drops it after a terminal error.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::contracts::ScribeError;
use crate::resources::ScribeResources;

/// Fixed request overhead charged to every accepted append.
pub const REQUEST_OVERHEAD_BYTES: usize = 4 * 1024;
/// Fixed pod-global in-flight item ceiling from the Scribe contract.
pub const GLOBAL_INFLIGHT_ITEMS: usize = 4_096;

/// Bounded acceptance window for caller-supplied `wyrd_event_time`.
///
/// Both the native Arrow IPC path and the projected OTLP path evaluate a
/// present `wyrd_event_time` column against this window at admission time.
/// Values that fall outside the window are rejected with
/// [`ScribeError::EventTimeOutOfRange`]; values within the window (inclusive
/// at both edges) are preserved verbatim. When the column is absent the
/// server stamps receipt time and no check runs.
///
/// A single per-batch receipt instant governs all rows so an in-flight clock
/// tick cannot split a batch verdict.
#[derive(Debug, Clone, Copy)]
pub struct EventTimeWindow {
    /// Maximum age below the per-batch server receipt time.
    ///
    /// Default: 30 days, accommodating backfill workloads.
    pub past: std::time::Duration,
    /// Maximum lead above receipt time for clock-skew tolerance.
    ///
    /// Default: 24 hours.
    pub future: std::time::Duration,
}

impl EventTimeWindow {
    /// Returns `true` when `event_micros` lies within the inclusive window
    /// `[receipt_micros - past, receipt_micros + future]`.
    ///
    /// Duration-to-micros conversions saturate at `i64::MAX`/`i64::MIN` rather
    /// than panicking so a pathologically large window cannot overflow.
    #[must_use]
    pub fn contains(&self, event_micros: i64, receipt_micros: i64) -> bool {
        // Saturating cast: past_micros fits an i64 for any sane Duration.
        let past_micros = i64::try_from(self.past.as_micros()).unwrap_or(i64::MAX);
        let future_micros = i64::try_from(self.future.as_micros()).unwrap_or(i64::MAX);
        let lo = receipt_micros.saturating_sub(past_micros);
        let hi = receipt_micros.saturating_add(future_micros);
        event_micros >= lo && event_micros <= hi
    }

    /// Returns the UTC-noon instant `days_before_receipt` days below the
    /// production receipt instant, in the microsecond unit
    /// [`Self::contains`] itself compares against.
    ///
    /// Scribe fixtures must never pin an absolute event-time literal. A pinned
    /// instant silently ages out of `[receipt - past, receipt + future]` as
    /// wall-clock advances, so a fixture that passes today becomes a permanent
    /// failure on some later date and stops proving anything about the path it
    /// covers. Deriving the instant here -- from this window, against the same
    /// [`crate::scribe::execution_lanes::current_receipt_micros`] clock the
    /// enforcement path reads -- keeps a fixture correct at any wall-clock
    /// time without restating the bound arithmetic outside this type.
    ///
    /// Noon is chosen so the returned instant sits at least twelve hours from
    /// either surrounding day boundary. A caller that derives an `EventDay`
    /// from this value therefore lands on the same calendar day as the value
    /// itself no matter when the test runs.
    ///
    /// # Panics
    /// Panics when the receipt clock is unavailable, when `days_before_receipt`
    /// does not land on a representable instant, or when the derived instant
    /// falls outside this window. Each case would leave a fixture asserting
    /// against an event time the production admission path would refuse, which
    /// is precisely the condition this accessor exists to prevent.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn admitted_event_time_micros(&self, days_before_receipt: i64) -> i64 {
        let receipt_micros = crate::scribe::execution_lanes::current_receipt_micros()
            .expect("receipt clock must be readable to derive an admitted event time");
        let receipt = chrono::DateTime::from_timestamp_micros(receipt_micros)
            .expect("receipt instant must be representable");
        let event_micros = (receipt - chrono::Duration::days(days_before_receipt))
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon must be a valid time on any date")
            .and_utc()
            .timestamp_micros();
        assert!(
            self.contains(event_micros, receipt_micros),
            "derived event time {event_micros} must lie inside the admission window"
        );
        event_micros
    }
}

impl Default for EventTimeWindow {
    fn default() -> Self {
        const SECS_PER_DAY: u64 = 86_400;
        Self {
            past: std::time::Duration::from_secs(30 * SECS_PER_DAY),
            future: std::time::Duration::from_secs(SECS_PER_DAY),
        }
    }
}

/// Fixed pod-wide admission settings.
#[derive(Debug, Clone, Copy)]
pub struct AdmissionConfig {
    /// Pod memory budget used by the Scribe memory governor.
    pub memory_limit_bytes: usize,
    /// Optional explicit Scribe child budget under the detected pod budget.
    pub scribe_memory_limit_bytes: Option<usize>,
    /// Acceptance window for caller-supplied `wyrd_event_time` values.
    ///
    /// Applied uniformly to both the native Arrow IPC and projected OTLP
    /// ingest surfaces. Values outside the window are rejected with
    /// `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`; the absent-column server-stamp
    /// path is unaffected. Defaults to 30 days past / 24 hours future per D85.
    pub event_time_window: EventTimeWindow,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            memory_limit_bytes: 1024 * 1024 * 1024,
            scribe_memory_limit_bytes: None,
            event_time_window: EventTimeWindow::default(),
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
    memory: ScribeResources,
}

/// Pod-global admission controller.
#[derive(Debug, Clone)]
pub struct AdmissionController {
    inner: Arc<AdmissionInner>,
}

impl AdmissionController {
    /// Construct an admission controller with the production defaults.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AdmissionConfig::default())
    }

    /// Construct an admission controller with explicit limits.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed one-gibibyte test governor violates the
    /// governor constructor invariant.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn with_config(config: AdmissionConfig) -> Self {
        let memory = super::embedded_scribe_resources(&config);
        Self::with_config_and_memory(config, memory)
    }

    /// Construct admission with the already-resolved process-wide governor.
    #[must_use]
    pub fn with_config_and_memory(config: AdmissionConfig, memory: ScribeResources) -> Self {
        Self {
            inner: Arc::new(AdmissionInner {
                config,
                state: Mutex::new(AdmissionState::default()),
                wal_available: AtomicBool::new(true),
                memory,
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
        if bytes > 0 && self.inner.memory.is_poisoned() {
            return Err(ScribeError::Internal {
                detail: "memory accounting poisoned; restart required".to_owned(),
            });
        }
        if !self.inner.wal_available.load(Ordering::Acquire) {
            super::record_scribe_rejection("wal");
            return Err(ScribeError::WalDiskFull);
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;

        let next_items = state.items.checked_add(1).ok_or_else(|| {
            self.inner.memory.poison();
            ScribeError::Internal {
                detail: "admission item counter overflow".to_owned(),
            }
        })?;
        let next_bytes = state
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| ScribeError::IngestBusy {
                table: table.clone(),
            })?;
        if next_items > GLOBAL_INFLIGHT_ITEMS {
            super::record_scribe_rejection("in_flight");
            return Err(ScribeError::IngestBusy { table });
        }
        if next_bytes > self.memory_breaker_bytes() {
            super::record_scribe_rejection("memory");
            return Err(ScribeError::IngestBusy { table });
        }

        state.items = next_items;
        state.bytes = next_bytes;
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
        state.active_bytes =
            state
                .active_bytes
                .checked_add(bytes)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "admission active counter overflow".to_owned(),
                })?;
        Ok(())
    }

    /// Release active Arrow bytes after a failed memtable insertion.
    pub fn release_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during active release".to_owned(),
        })?;
        if state.active_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission active counter underflow".to_owned(),
            });
        }
        state.active_bytes -= bytes;
        Ok(())
    }

    /// Check active ownership before a coordinated cleanup releases counters.
    ///
    /// The shard owner calls this beside the matching memory-ledger preflight
    /// so a detected mismatch leaves both ownership dimensions unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the shared governor when
    /// the active counter cannot cover `bytes` or its lock is poisoned.
    pub(crate) fn preflight_release_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during active preflight".to_owned(),
        })?;
        if state.active_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission active counter underflow during preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Transfer active bytes to the immutable generation tier.
    pub fn transfer_active_to_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during active transfer".to_owned(),
        })?;
        if state.active_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission active counter underflow during transfer".to_owned(),
            });
        }
        let immutable = state.immutable_bytes.checked_add(bytes).ok_or_else(|| {
            self.inner.memory.poison();
            ScribeError::Internal {
                detail: "admission immutable counter overflow during transfer".to_owned(),
            }
        })?;
        state.active_bytes -= bytes;
        state.immutable_bytes = immutable;
        Ok(())
    }

    /// Check an active-to-immutable move before its lifecycle owner mutates.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the shared governor when
    /// active ownership is insufficient, immutable ownership would overflow,
    /// or the admission lock is poisoned.
    pub(crate) fn preflight_transfer_active_to_immutable(
        &self,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during active transfer preflight".to_owned(),
        })?;
        if state.active_bytes < bytes || state.immutable_bytes.checked_add(bytes).is_none() {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission active-to-immutable transfer failed preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Release immutable bytes after a generation is retired.
    pub fn release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during immutable release".to_owned(),
        })?;
        if state.immutable_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission immutable counter underflow".to_owned(),
            });
        }
        state.immutable_bytes -= bytes;
        Ok(())
    }

    /// Check immutable ownership before an explicit retirement mutates counters.
    pub(crate) fn preflight_release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during immutable preflight".to_owned(),
        })?;
        if state.immutable_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission immutable counter underflow during preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Transfer immutable bytes back to the active generation after a seal
    /// transaction rolls back.
    pub fn transfer_immutable_to_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during immutable transfer".to_owned(),
        })?;
        if state.immutable_bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission immutable counter underflow during transfer".to_owned(),
            });
        }
        let active = state.active_bytes.checked_add(bytes).ok_or_else(|| {
            self.inner.memory.poison();
            ScribeError::Internal {
                detail: "admission active counter overflow during transfer".to_owned(),
            }
        })?;
        state.immutable_bytes -= bytes;
        state.active_bytes = active;
        Ok(())
    }

    /// Check immutable-to-active ownership before a post-commit abort mutates.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the shared governor when
    /// immutable ownership is insufficient, active ownership would overflow,
    /// or the admission lock is poisoned.
    pub(crate) fn preflight_transfer_immutable_to_active(
        &self,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during immutable transfer preflight".to_owned(),
        })?;
        if state.immutable_bytes < bytes || state.active_bytes.checked_add(bytes).is_none() {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission immutable-to-active transfer failed preflight".to_owned(),
            });
        }
        Ok(())
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

    fn release_request(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut state = self.inner.state.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during request release".to_owned(),
        })?;
        if state.items == 0 || state.bytes < bytes {
            self.inner.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission request counter underflow".to_owned(),
            });
        }
        state.items -= 1;
        state.bytes -= bytes;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
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
            let released = self.bytes - bytes;
            let Some(next_bytes) = state.bytes.checked_sub(released) else {
                inner.memory.poison();
                return Err(ScribeError::Internal {
                    detail:
                        "in-flight admission byte counter underflow while shrinking reservation"
                            .to_owned(),
                });
            };
            state.bytes = next_bytes;
        }
        self.bytes = bytes;
        Ok(())
    }

    /// Release the in-flight reservation immediately.
    pub fn release(mut self) -> Result<(), ScribeError> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<(), ScribeError> {
        if let Some(inner) = self.inner.take() {
            AdmissionController { inner }.release_request(self.bytes)
        } else {
            Ok(())
        }
    }
}

impl Drop for InflightFrameReservation {
    fn drop(&mut self) {
        if let Err(error) = self.release_inner() {
            tracing::error!(error = %error, "in-flight admission cleanup poisoned accounting");
        }
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
            event_time_window: EventTimeWindow::default(),
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
            memory_limit_bytes: usize::MAX,
            scribe_memory_limit_bytes: None,
            event_time_window: EventTimeWindow::default(),
        });
        let mut reservations = Vec::with_capacity(GLOBAL_INFLIGHT_ITEMS);
        for _ in 0..GLOBAL_INFLIGHT_ITEMS {
            reservations.push(admission.try_reserve("events", 1).expect("reserve"));
        }
        let error = admission.try_reserve("events", 1).expect_err("busy");
        assert!(matches!(error, ScribeError::IngestBusy { .. }));
        assert_eq!(admission.snapshot().items, GLOBAL_INFLIGHT_ITEMS);
    }

    /// Actual admission branches emit their exact closed rejection reasons once.
    #[test]
    fn admission_rejections_emit_exact_wal_and_in_flight_reasons() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let unavailable = AdmissionController::default();
            unavailable.trip_wal_disk_full();
            assert!(matches!(
                unavailable.try_reserve("events", 1),
                Err(ScribeError::WalDiskFull)
            ));

            let bounded = AdmissionController::default();
            let reservations = (0..GLOBAL_INFLIGHT_ITEMS)
                .map(|_| bounded.try_reserve("events", 1).expect("bounded reserve"))
                .collect::<Vec<_>>();
            assert!(matches!(
                bounded.try_reserve("events", 1),
                Err(ScribeError::IngestBusy { .. })
            ));
            drop(reservations);
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_rejections_total{reason=\"wal\"}"),
            Some(&1)
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_rejections_total{reason=\"in_flight\"}"),
            Some(&1)
        );
        assert!(!snapshot.counters.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }

    #[test]
    fn active_and_immutable_bytes_are_non_reserving_counters() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            memory_limit_bytes: 100,
            scribe_memory_limit_bytes: None,
            event_time_window: EventTimeWindow::default(),
        });
        admission
            .try_reserve_active("events", 90)
            .expect("record active bytes");
        admission
            .try_reserve_active("events", 1)
            .expect("counters do not reserve memory");
        admission
            .transfer_active_to_immutable(90)
            .expect("active transfer");
        assert_eq!(admission.snapshot().active_bytes, 1);
        assert_eq!(admission.snapshot().immutable_bytes, 90);
        admission.release_immutable(90).expect("immutable release");
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

    /// Admission observes the same governor poison bit as memory reservations.
    #[test]
    fn admission_uses_shared_governor_poison() {
        let memory = super::super::embedded_scribe_resources(&AdmissionConfig::default());
        let admission =
            AdmissionController::with_config_and_memory(AdmissionConfig::default(), memory.clone());
        memory.poison();
        assert!(matches!(
            admission.try_reserve("events", 1),
            Err(ScribeError::Internal { .. })
        ));
    }

    /// Authoritative memtable synchronization repairs inspection counters without poisoning.
    #[test]
    fn authoritative_memtable_sync_does_not_poison() {
        let memory = super::super::embedded_scribe_resources(&AdmissionConfig::default());
        let admission =
            AdmissionController::with_config_and_memory(AdmissionConfig::default(), memory.clone());
        admission.sync_memtable_bytes(64, 32);
        assert_eq!(admission.snapshot().active_bytes, 64);
        assert_eq!(admission.snapshot().immutable_bytes, 32);
        assert!(!memory.is_poisoned());
    }

    /// A request-byte overflow rejects before either admission counter mutates.
    #[test]
    fn admission_byte_overflow_leaves_items_and_bytes_unchanged() {
        let admission = AdmissionController::default();
        let held = admission
            .try_reserve("events", 1)
            .expect("seed reservation");
        let before = admission.snapshot();
        assert!(matches!(
            admission.try_reserve("events", usize::MAX),
            Err(ScribeError::IngestBusy { .. })
        ));
        assert_eq!(admission.snapshot(), before);
        drop(held);
        let ordinary = admission
            .try_reserve("events", 1)
            .expect("ordinary reservation");
        drop(ordinary);
    }
}
