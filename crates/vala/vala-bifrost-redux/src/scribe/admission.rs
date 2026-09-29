//! Pod-global Scribe admission accounting.
//!
//! Admission is deliberately separate from writer state. A request reserves
//! one in-flight item before it is split, serialized, or written; its bytes are
//! owned by the one shared Bifrost memory root, never by a second ledger here.
//! The reservation stays attached to the accepted request until its consumer
//! finishes or drops it after a terminal error.

use std::sync::{Arc, Mutex};

use crate::contracts::ScribeError;
use crate::resources::ScribeResources;
use crate::scribe::telemetry::ScribeTelemetry;

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
    /// Pod memory budget an embedded Scribe sizes its private governor from.
    ///
    /// Server deployments pass the resolved shared Bifrost cap; admission
    /// itself enforces no byte ceiling against it.
    pub memory_limit_bytes: usize,
    /// Acceptance window for caller-supplied `wyrd_event_time` values.
    ///
    /// Applied uniformly to both the native Arrow IPC and projected OTLP
    /// ingest surfaces. Values outside the window are rejected with
    /// `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`; the absent-column server-stamp
    /// path is unaffected. Defaults to 30 days past / 24 hours future per D85.
    pub event_time_window: EventTimeWindow,
}

/// Default pod memory budget for embedded and test Scribe construction.
pub const DEFAULT_POD_MEMORY_LIMIT_BYTES: usize = 4 * 1024 * 1024 * 1024;

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            memory_limit_bytes: DEFAULT_POD_MEMORY_LIMIT_BYTES,
            event_time_window: EventTimeWindow::default(),
        }
    }
}

/// Shared state behind every clone of one [`AdmissionController`].
#[derive(Debug)]
struct AdmissionInner {
    /// Resolved admission settings.
    config: AdmissionConfig,
    /// Accepted requests currently holding an in-flight item.
    items: Mutex<usize>,
    /// Shared memory root whose poison refuses new work.
    memory: ScribeResources,
    /// The pod's one staging and lifecycle observation owner.
    telemetry: Arc<ScribeTelemetry>,
}

/// Pod-global admission controller.
///
/// It bounds accepted requests by [`GLOBAL_INFLIGHT_ITEMS`] and refuses new
/// work once the shared memory root is poisoned. Bytes are charged only by
/// the root owners that actually hold them.
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

    /// Construct an admission controller over a private embedded governor.
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
                items: Mutex::new(0),
                memory,
                telemetry: Arc::new(ScribeTelemetry::default()),
            }),
        }
    }

    /// Returns the pod's one staging and lifecycle observation owner.
    ///
    /// Persistence records through the same instance so staged-member and
    /// claim totals reconcile against one owner.
    #[must_use]
    pub(crate) fn telemetry_handle(&self) -> Arc<ScribeTelemetry> {
        Arc::clone(&self.inner.telemetry)
    }

    /// Reserve one in-flight request item without waiting.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] naming `table` when the pod already
    /// holds [`GLOBAL_INFLIGHT_ITEMS`] requests, and [`ScribeError::Internal`]
    /// when the shared memory root is poisoned or the item lock is poisoned.
    pub fn try_reserve(
        &self,
        table: impl Into<String>,
    ) -> Result<InflightFrameReservation, ScribeError> {
        if self.inner.memory.is_poisoned() {
            return Err(ScribeError::Internal {
                detail: "memory accounting poisoned; restart required".to_owned(),
            });
        }
        let mut items = self
            .inner
            .items
            .lock()
            .map_err(|error| ScribeError::Internal {
                detail: format!("admission state lock poisoned: {error}"),
            })?;
        if *items >= GLOBAL_INFLIGHT_ITEMS {
            super::record_scribe_rejection("in_flight");
            return Err(ScribeError::IngestBusy {
                table: table.into(),
            });
        }
        *items += 1;
        Ok(InflightFrameReservation {
            inner: Some(Arc::clone(&self.inner)),
        })
    }

    /// Return a point-in-time view of the pod admission counters.
    #[must_use]
    pub fn snapshot(&self) -> AdmissionSnapshot {
        let items = match self.inner.items.lock() {
            Ok(items) => *items,
            Err(poisoned) => {
                tracing::error!("admission state lock poisoned while taking snapshot");
                *poisoned.into_inner()
            }
        };
        AdmissionSnapshot {
            items,
            max_items: GLOBAL_INFLIGHT_ITEMS,
        }
    }

    /// Return the resolved admission configuration.
    #[must_use]
    pub fn config(&self) -> AdmissionConfig {
        self.inner.config
    }
}

impl AdmissionInner {
    /// Returns one in-flight item.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the shared root when no
    /// item is held, and when the item lock is poisoned.
    fn release_item(&self) -> Result<(), ScribeError> {
        let mut items = self.items.lock().map_err(|_| ScribeError::Internal {
            detail: "admission state lock poisoned during request release".to_owned(),
        })?;
        if *items == 0 {
            self.memory.poison();
            return Err(ScribeError::Internal {
                detail: "admission request counter underflow".to_owned(),
            });
        }
        *items -= 1;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Default for AdmissionController {
    fn default() -> Self {
        Self::new()
    }
}

/// One in-flight request item carried by a shard queue.
#[derive(Debug)]
pub struct InflightFrameReservation {
    /// Controller that owns the item, until released.
    inner: Option<Arc<AdmissionInner>>,
}

impl InflightFrameReservation {
    /// Release the in-flight item immediately.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the item counter underflows or
    /// its lock is poisoned.
    pub fn release(mut self) -> Result<(), ScribeError> {
        self.release_inner()
    }

    /// Returns the item once; later calls are no-ops.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the item counter underflows or
    /// its lock is poisoned.
    fn release_inner(&mut self) -> Result<(), ScribeError> {
        self.inner
            .take()
            .map_or(Ok(()), |inner| inner.release_item())
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
    /// Configured item limit.
    pub max_items: usize,
}

#[cfg(test)]
mod tests {
    use super::{AdmissionController, GLOBAL_INFLIGHT_ITEMS};
    use crate::contracts::ScribeError;
    use crate::resources::ScribeMemoryCategory;

    /// Admission bounds only in-flight items; the shared root alone owns
    /// bytes. A request as large as the shared cap is charged once at the root
    /// with no 90-percent breaker or planned-envelope refusal, a full root
    /// refuses the next charge, and releasing the first charge admits it.
    #[test]
    fn one_root_charge_has_no_secondary_memory_ceiling() {
        let admission = AdmissionController::new();
        let memory = admission.inner.memory.clone();
        let cap = memory.limit_bytes();
        let item = admission.try_reserve("wyrd.root").expect("item fits");
        let whole = memory
            .try_reserve_ingress(ScribeMemoryCategory::Raw, cap)
            .expect("a request sized to the whole shared cap is admitted once");
        assert_eq!(
            memory
                .snapshot()
                .expect("snapshot")
                .scribe_memory_used_bytes,
            cap
        );
        assert!(
            memory
                .try_reserve_ingress(ScribeMemoryCategory::Raw, 1)
                .is_err(),
            "a full shared root refuses further growth"
        );
        drop(whole);
        let next = memory
            .try_reserve_ingress(ScribeMemoryCategory::Raw, cap)
            .expect("released root capacity admits the next write");
        drop(next);
        drop(item);
        assert_eq!(admission.snapshot().items, 0);
    }

    /// The fixed pod-global item bound refuses with the table's busy error
    /// and admits again once an item returns.
    #[test]
    fn admission_rejects_after_fixed_global_item_bound() {
        let admission = AdmissionController::new();
        let mut held: Vec<_> = (0..GLOBAL_INFLIGHT_ITEMS)
            .map(|_| admission.try_reserve("wyrd.items").expect("item fits"))
            .collect();
        assert!(matches!(
            admission.try_reserve("wyrd.items"),
            Err(ScribeError::IngestBusy { .. })
        ));
        held.pop().expect("one item").release().expect("release");
        admission
            .try_reserve("wyrd.items")
            .expect("released item admits");
    }

    /// A poisoned shared root refuses new admission before any item moves.
    #[test]
    fn admission_uses_shared_governor_poison() {
        let admission = AdmissionController::new();
        admission.inner.memory.poison();
        assert!(matches!(
            admission.try_reserve("wyrd.poison"),
            Err(ScribeError::Internal { .. })
        ));
        assert_eq!(admission.snapshot().items, 0);
    }
}
