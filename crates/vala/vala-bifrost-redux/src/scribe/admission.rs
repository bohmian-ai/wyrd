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
use crate::scribe::contention::{ActivationOutcome, ContentionKey, ScribeContentionLedger};
use crate::scribe::geometry::{ContentionCategory, ScribeArtifactPolicy, ScribeGeometryError};

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
    /// Per-table contention policy every admitted table reserves against.
    ///
    /// Held here rather than derived per request so the reserve vector one
    /// table installs is by construction the vector startup proved the pod can
    /// hold, and so a configuration change cannot move the two apart.
    pub policy: ScribeArtifactPolicy,
    /// Acceptance window for caller-supplied `wyrd_event_time` values.
    ///
    /// Applied uniformly to both the native Arrow IPC and projected OTLP
    /// ingest surfaces. Values outside the window are rejected with
    /// `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`; the absent-column server-stamp
    /// path is unaffected. Defaults to 30 days past / 24 hours future per D85.
    pub event_time_window: EventTimeWindow,
}

/// Smallest pod memory budget the default Scribe geometry can be installed on.
///
/// [`ScribeArtifactPolicy::pod_capacity`] splits the Scribe memory ceiling into
/// four equal category shares, and the largest single-table component is merge
/// scratch at `minimum_merge_lane_scratch_bytes`. Four times that — half a
/// gibibyte — is the hard floor at which every category holds one complete
/// lifecycle vector, and this default is eight times that floor so an embedded
/// or test controller derives a useful multi-table ownership ceiling rather than
/// a pod that can only ever carry one table. Nothing here scales with tenants or
/// tables: a smaller budget still starts, it just completes fewer tables at once.
pub const DEFAULT_POD_MEMORY_LIMIT_BYTES: usize =
    32 * super::geometry::DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES;

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            memory_limit_bytes: DEFAULT_POD_MEMORY_LIMIT_BYTES,
            scribe_memory_limit_bytes: None,
            policy: ScribeArtifactPolicy::default(),
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
    contention: ScribeContentionLedger,
}

/// Pod-global admission controller.
#[derive(Debug, Clone)]
pub struct AdmissionController {
    inner: Arc<AdmissionInner>,
}

impl AdmissionController {
    /// Construct an admission controller with the production defaults.
    ///
    /// # Panics
    ///
    /// Panics when the default geometry does not fit the default governor,
    /// which is a fixed-constant invariant this crate proves in its own tests
    /// rather than a condition a caller can provoke.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AdmissionConfig::default())
            .expect("the default Scribe geometry must fit the default embedded governor")
    }

    /// Construct an admission controller with explicit limits.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError`] when the resolved pod capacity cannot
    /// hold one complete lifecycle vector, exactly as production startup does.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_config(config: AdmissionConfig) -> Result<Self, ScribeGeometryError> {
        let memory = super::embedded_scribe_resources(&config);
        Self::with_config_and_memory(config, memory)
    }

    /// Construct admission with the already-resolved process-wide governor.
    ///
    /// This is the startup capacity gate. It derives the pod's real capacity in
    /// every governed category from the memory Scribe actually owns and the
    /// staging volume actually present, then refuses to build the ledger unless every category holds at least one
    /// complete lifecycle vector. A pod that cannot carry even one canonical
    /// table from admission through release must fail here, before it reports
    /// ready, rather than accept appends it cannot stage, merge or publish and
    /// discover the shortfall as runtime pressure. How many tables the pod can
    /// then own is derived from that capacity, not configured.
    ///
    /// The Scribe-owned child budget wins over the whole-pod budget whenever it
    /// is configured and smaller, because that child budget is what Scribe may
    /// actually own; validating against the larger pod figure would prove a
    /// capacity Scribe never has.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeGeometryError::Capacity`] naming the category and the
    /// required and actual totals of the first shortfall, and
    /// [`ScribeGeometryError::EmptyReserve`] when the policy derives a zero
    /// component.
    pub fn with_config_and_memory(
        config: AdmissionConfig,
        memory: ScribeResources,
    ) -> Result<Self, ScribeGeometryError> {
        // The staging volume is disk the ledger only ever compares against, so
        // an absent volume yields zero rather than a fabricated allowance: a pod
        // with no staging volume must fail startup, not pretend.
        let staging_bytes = memory
            .snapshot()
            .map_or(0, |snapshot| snapshot.plan.scratch_limit_bytes);
        let scribe_memory_bytes = config
            .scribe_memory_limit_bytes
            .map_or(config.memory_limit_bytes, |scribe| {
                scribe.min(config.memory_limit_bytes)
            });
        let capacity = config.policy.pod_capacity(
            scribe_memory_bytes,
            usize::try_from(staging_bytes).unwrap_or(usize::MAX),
            GLOBAL_INFLIGHT_ITEMS,
        );
        config.policy.validate_capacity(&capacity)?;
        Ok(Self {
            inner: Arc::new(AdmissionInner {
                config,
                state: Mutex::new(AdmissionState::default()),
                wal_available: AtomicBool::new(true),
                memory,
                contention: ScribeContentionLedger::new(config.policy, capacity),
            }),
        })
    }

    /// Borrows the per-table contention ledger this controller owns.
    ///
    /// Exposed so shard and staging owners charge the same ledger admission
    /// does rather than keeping a second, divergent view of who owns what.
    #[must_use]
    pub fn contention(&self) -> &ScribeContentionLedger {
        &self.inner.contention
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
        self.try_reserve_kind(table, None, bytes)
    }

    /// Reserve one request item and its byte charge against a table's own cell.
    ///
    /// This is the production path. The pod-global counters still bound the node
    /// as a whole, but the request is additionally charged to the caller's own
    /// contention cell, so a tenant that saturates the node is refused on its own
    /// account instead of pushing that refusal onto a quiet neighbour. The cell
    /// is installed on first use and its whole reserve vector is committed at
    /// that moment.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when a pod-global limit, the pod's
    /// activation width, or the table's own reserve plus available surplus
    /// cannot cover the request, [`ScribeError::WalDiskFull`] when the WAL
    /// breaker is tripped, and [`ScribeError::Internal`] when memory accounting
    /// is poisoned or a lock failed.
    pub fn try_reserve_for_cell(
        &self,
        key: &ContentionKey,
        table: impl Into<String>,
        bytes: usize,
    ) -> Result<InflightFrameReservation, ScribeError> {
        self.try_reserve_kind(table, Some(key), bytes)
    }

    fn try_reserve_kind(
        &self,
        table: impl Into<String>,
        cell: Option<&ContentionKey>,
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

        if let Some(key) = cell {
            // Charge the cell only after the pod-global counters accepted, and
            // hand back the global charge if the cell refuses, so a per-table
            // refusal never leaves the pod counting a request nobody owns.
            if let Err(refusal) = self.charge_cell(key, bytes) {
                let _ = self.release_request(bytes);
                super::record_scribe_rejection("contention");
                return Err(refusal);
            }
        }

        Ok(InflightFrameReservation {
            inner: Some(Arc::clone(&self.inner)),
            bytes,
            cell: cell.cloned(),
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

    /// Activates a table's cell if needed and charges one admitted request.
    ///
    /// Activation and the two admission charges are one reversible transition.
    ///
    /// A cell that held an item without its bytes, or the reverse, would let a
    /// table pass admission it cannot actually own. A cell created here and then
    /// refused is removed again, so a rejected first request leaves no trace of
    /// the table at all -- an empty cell would otherwise occupy one of the pod's
    /// derived ownership slots forever while serving nothing. Only a cell *this
    /// call* created is rolled back: a table that was already active keeps its
    /// cell and everything it owns, because the refusal says nothing about the
    /// work it is already carrying.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeError`] projection of the contention refusal.
    fn charge_cell(&self, key: &ContentionKey, bytes: usize) -> Result<(), ScribeError> {
        let ledger = &self.inner.contention;
        let outcome = ledger.activate(key)?;
        let rollback = |controller: &Self| {
            if outcome == ActivationOutcome::Created {
                // Every charge has been returned by this point, so the cell is
                // empty and deactivation cannot refuse.
                if let Err(error) = controller.inner.contention.deactivate(key) {
                    tracing::error!(
                        error = %error,
                        "failed admission could not release the cell it created"
                    );
                }
            }
        };
        if let Err(refusal) = ledger.charge(key, ContentionCategory::AdmissionItems, 1) {
            rollback(self);
            return Err(refusal.into());
        }
        if let Err(refusal) = ledger.charge(key, ContentionCategory::AdmissionBytes, bytes) {
            if let Err(error) = ledger.release(key, ContentionCategory::AdmissionItems, 1) {
                tracing::error!(error = %error, "admission item rollback failed");
            }
            rollback(self);
            return Err(refusal.into());
        }
        Ok(())
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
    /// Contention cell charged alongside the pod-global counters, when the
    /// caller reserved through the per-table path.
    cell: Option<ContentionKey>,
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
            let next_bytes =
                state
                    .bytes
                    .checked_add(extra)
                    .ok_or_else(|| ScribeError::IngestBusy {
                        table: "vala.bifrost".to_owned(),
                    })?;
            state.bytes = next_bytes;
            if let Some(key) = &self.cell {
                // The global counter moved first, so a contention refusal must
                // put it back. Leaving it raised would charge the pod for bytes
                // no table owns, and every later admission would be measured
                // against that phantom.
                if let Err(refusal) =
                    inner
                        .contention
                        .charge(key, ContentionCategory::AdmissionBytes, extra)
                {
                    state.bytes -= extra;
                    return Err(refusal.into());
                }
            }
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
            if let Some(key) = &self.cell {
                inner
                    .contention
                    .release(key, ContentionCategory::AdmissionBytes, released)?;
            }
        }
        self.bytes = bytes;
        Ok(())
    }

    /// Release the in-flight reservation immediately.
    pub fn release(mut self) -> Result<(), ScribeError> {
        self.release_inner()
    }

    /// Returns both the pod-global charge and the contention cell's share.
    ///
    /// The cell is released first and unconditionally: leaking a cell charge
    /// would permanently shrink one table's usable reserve, which is a worse
    /// failure than double-counting a global byte that the governor's own
    /// reconciliation can still correct.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when either counter underflows or its
    /// lock is poisoned.
    fn release_inner(&mut self) -> Result<(), ScribeError> {
        let Some(inner) = self.inner.take() else {
            return Ok(());
        };
        let cell_result = match self.cell.take() {
            Some(key) => inner
                .contention
                .release(&key, ContentionCategory::AdmissionBytes, self.bytes)
                .and_then(|()| {
                    inner
                        .contention
                        .release(&key, ContentionCategory::AdmissionItems, 1)
                })
                .map_err(ScribeError::from),
            None => Ok(()),
        };
        let global_result = AdmissionController { inner }.release_request(self.bytes);
        cell_result.and(global_result)
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

    /// Builds a contention cell identity for one tenant ordinal and table name.
    ///
    /// # Panics
    ///
    /// Panics when the synthetic tenant identifier is not accepted.
    fn cell_key(tenant: u8, table: &str) -> ContentionKey {
        let mut bytes = [tenant; 16];
        bytes[6] = 0x70 | (bytes[6] & 0x0f);
        bytes[8] = 0x80 | (bytes[8] & 0x3f);
        ContentionKey::new(
            wyrd_spec::ids::DataTenantId::new(uuid::Uuid::from_bytes(bytes))
                .expect("UUIDv7 test tenant"),
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Datasets, table),
        )
    }

    /// Returns the pod capacity the default admission configuration derives.
    fn default_pod_capacity() -> crate::scribe::geometry::ScribeGlobalCapacity {
        let config = AdmissionConfig::default();
        let memory = super::super::embedded_scribe_resources(&config);
        let staging = memory
            .snapshot()
            .map_or(0, |snapshot| snapshot.plan.scratch_limit_bytes);
        config.policy.pod_capacity(
            config.memory_limit_bytes,
            usize::try_from(staging).unwrap_or(usize::MAX),
            GLOBAL_INFLIGHT_ITEMS,
        )
    }

    /// A first request refused on admission items installs no cell at all.
    ///
    /// Activation and the first charges are one reversible transition, so the
    /// arriving table must leave no trace: an empty cell would hold one of the
    /// pod's derived ownership slots while serving nothing.
    ///
    /// # Panics
    ///
    /// Panics when the refused arrival keeps a cell, or when the saturating
    /// owner loses anything.
    #[test]
    fn a_first_request_refused_on_items_installs_no_cell() {
        let admission = AdmissionController::with_config(AdmissionConfig::default())
            .expect("the default geometry fits the default budget");
        let ledger = admission.contention();
        let filler = cell_key(7, "filler");
        let capacity = default_pod_capacity();
        ledger.activate(&filler).expect("filler activates");
        ledger
            .charge(
                &filler,
                ContentionCategory::AdmissionItems,
                capacity.admission_items,
            )
            .expect("an idle pod lets one owner hold every admitted item");

        let arrival = cell_key(8, "arrival");
        admission
            .try_reserve_for_cell(&arrival, "wyrd.arrival", 0)
            .expect_err("the arrival cannot reserve an item the pod does not have");
        assert!(matches!(
            ledger.usage(&arrival, ContentionCategory::AdmissionItems),
            Err(crate::scribe::contention::ContentionRefusal::Inactive)
        ));
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
        assert_eq!(
            ledger
                .usage(&filler, ContentionCategory::AdmissionItems)
                .expect("the saturating owner keeps its cell"),
            capacity.admission_items
        );
    }

    /// A first request refused on admission bytes installs no cell at all.
    ///
    /// The item charge succeeds and the byte charge does not, so this proves the
    /// rollback returns the item as well as the cell.
    ///
    /// # Panics
    ///
    /// Panics when the refused arrival keeps a cell or leaves an item charged.
    #[test]
    fn a_first_request_refused_on_bytes_installs_no_cell() {
        let admission = AdmissionController::with_config(AdmissionConfig::default())
            .expect("the default geometry fits the default budget");
        let ledger = admission.contention();
        let filler = cell_key(7, "filler");
        let capacity = default_pod_capacity();
        ledger.activate(&filler).expect("filler activates");
        ledger
            .charge(
                &filler,
                ContentionCategory::AdmissionBytes,
                capacity.admission_bytes,
            )
            .expect("an idle pod lets one owner hold every admitted byte");

        let arrival = cell_key(8, "arrival");
        admission
            .try_reserve_for_cell(&arrival, "wyrd.arrival", 4_096)
            .expect_err("the arrival cannot reserve bytes the pod does not have");
        assert!(matches!(
            ledger.usage(&arrival, ContentionCategory::AdmissionBytes),
            Err(crate::scribe::contention::ContentionRefusal::Inactive)
        ));
        assert_eq!(
            ledger
                .committed(ContentionCategory::AdmissionItems)
                .expect("pod items"),
            0,
            "the rolled-back arrival must return the item it charged first"
        );
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
    }

    /// A rejected resize leaves the global and cell counters exactly as they were.
    ///
    /// The global byte counter moves before contention is consulted, so a
    /// refusal must put it back; leaving it raised would charge the pod for
    /// bytes no table owns.
    ///
    /// # Panics
    ///
    /// Panics when a refused resize changes any counter, or when dropping the
    /// reservation afterwards does not balance every level back to zero.
    #[test]
    fn a_rejected_resize_changes_no_counter_and_still_balances_on_drop() {
        let admission = AdmissionController::with_config(AdmissionConfig::default())
            .expect("the default geometry fits the default budget");
        let ledger = admission.contention();
        let capacity = default_pod_capacity();
        let owner = cell_key(9, "events");
        let mut reservation = admission
            .try_reserve_for_cell(&owner, "wyrd.events", 4_096)
            .expect("a first request activates the cell and reserves inside it");

        // Saturate the remaining admitted bytes from another owner so the resize
        // is refused by contention rather than by the pod-global breaker.
        let filler = cell_key(10, "filler");
        ledger.activate(&filler).expect("filler activates");
        ledger
            .charge(
                &filler,
                ContentionCategory::AdmissionBytes,
                capacity.admission_bytes - 4_096,
            )
            .expect("the rest of the admitted bytes are taken");

        let before = admission.snapshot();
        reservation
            .resize(4_096 + 1_024)
            .expect_err("a saturated pod refuses the growth");
        assert_eq!(admission.snapshot().bytes, before.bytes);
        assert_eq!(admission.snapshot().items, before.items);
        assert_eq!(
            ledger
                .usage(&owner, ContentionCategory::AdmissionBytes)
                .expect("the cell survives a refused resize"),
            4_096
        );
        assert_eq!(reservation.bytes(), 4_096);

        drop(reservation);
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
        assert_eq!(
            ledger
                .committed(ContentionCategory::AdmissionBytes)
                .expect("pod bytes"),
            capacity.admission_bytes - 4_096,
            "only the filler's charge remains"
        );
    }

    /// A per-table reservation charges and returns its own contention cell.
    ///
    /// # Panics
    ///
    /// Panics when the cell is not charged alongside the pod-global counters,
    /// when releasing the reservation leaves the cell holding anything, or when
    /// a refused per-table charge leaves the pod counting an unowned request.
    #[test]
    fn per_table_reservations_charge_and_return_their_own_cell() {
        let admission = AdmissionController::with_config(AdmissionConfig::default())
            .expect("the default geometry fits the default budget");
        let mut bytes = [7_u8; 16];
        bytes[6] = 0x70 | (bytes[6] & 0x0f);
        bytes[8] = 0x80 | (bytes[8] & 0x3f);
        let key = ContentionKey::new(
            wyrd_spec::ids::DataTenantId::new(uuid::Uuid::from_bytes(bytes))
                .expect("UUIDv7 test tenant"),
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Datasets, "events"),
        );

        let reservation = admission
            .try_reserve_for_cell(&key, "wyrd.events", 4_096)
            .expect("a first request activates the cell and reserves inside it");
        let ledger = admission.contention();
        assert_eq!(
            ledger
                .usage(&key, ContentionCategory::AdmissionItems)
                .expect("cell is active"),
            1
        );
        assert_eq!(
            ledger
                .usage(&key, ContentionCategory::AdmissionBytes)
                .expect("cell is active"),
            4_096
        );
        assert_eq!(admission.snapshot().items, 1);

        reservation.release().expect("release succeeds");
        assert_eq!(
            ledger
                .usage(&key, ContentionCategory::AdmissionItems)
                .expect("cell stays installed"),
            0
        );
        assert_eq!(
            ledger
                .usage(&key, ContentionCategory::AdmissionBytes)
                .expect("cell stays installed"),
            0
        );
        assert_eq!(admission.snapshot().items, 0);
        assert_eq!(admission.snapshot().bytes, 0);
    }

    #[test]
    fn reservations_release_global_items_and_bytes() {
        let admission = AdmissionController::with_config(AdmissionConfig {
            memory_limit_bytes: DEFAULT_POD_MEMORY_LIMIT_BYTES,
            scribe_memory_limit_bytes: None,
            policy: ScribeArtifactPolicy::default(),
            event_time_window: EventTimeWindow::default(),
        })
        .expect("the default geometry fits the default budget");
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
            policy: ScribeArtifactPolicy::default(),
            event_time_window: EventTimeWindow::default(),
        })
        .expect("the default geometry fits the default budget");
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
            memory_limit_bytes: DEFAULT_POD_MEMORY_LIMIT_BYTES,
            scribe_memory_limit_bytes: None,
            policy: ScribeArtifactPolicy::default(),
            event_time_window: EventTimeWindow::default(),
        })
        .expect("the default geometry fits the default budget");
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

    /// Startup refuses a pod that cannot hold one complete lifecycle vector.
    ///
    /// The controller is the only place the derived pod capacity and the
    /// configured policy meet, so this is where the AC2 refusal must happen. A
    /// pod one share short used to silently substitute a default or zero
    /// capacity and report ready.
    ///
    /// # Panics
    ///
    /// Panics when an undersized pod builds a controller, or when the refusal
    /// does not name the category and requirement an operator must fix.
    #[test]
    fn startup_refuses_a_pod_that_cannot_hold_one_lifecycle_vector() {
        let config = AdmissionConfig::default();
        let memory = super::super::embedded_scribe_resources(&config);
        AdmissionController::with_config_and_memory(config, memory.clone())
            .expect("the default budget holds one complete lifecycle vector");

        // The hard floor is one complete lifecycle vector, which memory reaches
        // at four times the merge-lane scratch minimum. Exactly the floor serves
        // one table; one memory share below it serves none.
        let floor = 4 * crate::scribe::geometry::DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES;
        AdmissionController::with_config_and_memory(
            AdmissionConfig {
                memory_limit_bytes: floor,
                ..AdmissionConfig::default()
            },
            memory.clone(),
        )
        .expect("the exact one-vector floor still serves");

        let short = AdmissionConfig {
            memory_limit_bytes: floor - 4,
            ..AdmissionConfig::default()
        };
        let error = AdmissionController::with_config_and_memory(short, memory)
            .expect_err("one share below the requirement must refuse before serving");
        let crate::scribe::geometry::ScribeGeometryError::Capacity {
            category,
            required,
            actual,
        } = error
        else {
            panic!("a startup shortfall must report the capacity diagnostic");
        };
        assert!(
            actual < required,
            "the refusal must name the real shortfall"
        );
        assert!(
            ContentionCategory::ALL
                .iter()
                .any(|known| known.label() == category),
            "the refusal must name a governed category"
        );
    }

    /// The Scribe child budget bounds startup even when the pod budget is ample.
    ///
    /// Validating against the whole-pod figure would prove a capacity Scribe
    /// never owns, which is precisely how a pod could report ready and then fail
    /// to carry the one table lifecycle it promised.
    ///
    /// # Panics
    ///
    /// Panics when the larger pod budget is used in place of the smaller
    /// Scribe-owned child limit.
    #[test]
    fn startup_validates_against_the_smaller_scribe_child_budget() {
        let config = AdmissionConfig {
            memory_limit_bytes: DEFAULT_POD_MEMORY_LIMIT_BYTES * 4,
            scribe_memory_limit_bytes: Some(
                4 * crate::scribe::geometry::DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES - 4,
            ),
            ..AdmissionConfig::default()
        };
        let memory = super::super::embedded_scribe_resources(&config);
        let error = AdmissionController::with_config_and_memory(config, memory)
            .expect_err("the child budget, not the pod budget, bounds Scribe");
        assert!(
            matches!(
                error,
                crate::scribe::geometry::ScribeGeometryError::Capacity { .. }
            ),
            "an undersized child budget must refuse with the capacity diagnostic"
        );
    }

    /// Admission observes the same governor poison bit as memory reservations.
    #[test]
    fn admission_uses_shared_governor_poison() {
        let memory = super::super::embedded_scribe_resources(&AdmissionConfig::default());
        let admission =
            AdmissionController::with_config_and_memory(AdmissionConfig::default(), memory.clone())
                .expect("the default geometry fits the default budget");
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
            AdmissionController::with_config_and_memory(AdmissionConfig::default(), memory.clone())
                .expect("the default geometry fits the default budget");
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
