//! Pure Scribe memory sizing, lifecycle categories, and refusal projections.
//!
//! Process capacity and live ownership belong exclusively to root-issued
//! [`crate::resources::ScribeResources`] and
//! [`crate::resources::ScribeMemoryLease`] values.

use num_traits::ToPrimitive;
use std::sync::Arc;
use std::sync::Mutex;

use crate::contracts::ScribeError;

#[cfg(test)]
thread_local! {
    static CGROUP_CURRENT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Minimum managed memory accepted by the checked Scribe/Oracle ledger.
///
/// The root resource plan removes the unmanaged process reserve before
/// constructing this ledger. A combined process therefore needs exactly two
/// 256 MiB role floors beneath this 512 MiB managed minimum.
pub const MIN_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// Minimum byte budget for each static child (Scribe or Oracle).
/// Complete conservative owner shared by both T4 Parquet producer paths.
pub(crate) const PARQUET_PRODUCER_OWNER_BYTES: usize = 256 * 1024 * 1024;
/// Exact encoded-footer child held from before writer creation through inspection.
pub(crate) const PARQUET_FOOTER_CHILD_BYTES: usize = 8 * 1024 * 1024;
/// Number of bounded lifecycle memory categories.
pub const MEMORY_CATEGORY_COUNT: usize = 8;

/// Returns the checked delta needed beside an admitted generation.
///
/// The immutable charge is transferred conceptually into the complete 256 MiB
/// producer owner and is never charged a second time.
///
/// Only the current generation's checked immutable ownership contributes to
/// the complete producer owner. A single generation larger than that owner is
/// refused before encoder construction because it cannot fit the fixed floor.
///
/// # Errors
///
/// Returns [`ScribeError::IngestBusy`] when `generation_bytes` exceeds the
/// complete conservative producer owner.
pub(crate) fn parquet_producer_delta(generation_bytes: usize) -> Result<usize, ScribeError> {
    if generation_bytes > PARQUET_PRODUCER_OWNER_BYTES {
        return Err(ScribeError::IngestBusy {
            table: "memory".to_owned(),
        });
    }
    Ok(PARQUET_PRODUCER_OWNER_BYTES.saturating_sub(generation_bytes))
}

/// Move-only encoded-footer child split from the complete producer owner.
///
/// Carrying this token into the CPU lane proves the exact eight-mebibyte
/// allowance remains charged from before encoder construction until every
/// sealed footer has been inspected. Dropping it restores those bytes to the
/// remaining producer reservation without changing aggregate accounting.
#[derive(Debug)]
pub(crate) struct EncodedFooterReservation {
    /// Exact checked memory reservation backing the footer child.
    reservation: Option<crate::resources::ScribeMemoryLease>,
}

impl EncodedFooterReservation {
    /// Splits the exact footer child from an already-admitted producer owner.
    ///
    /// # Errors
    /// Returns an internal error when the producer owner cannot supply the
    /// exact eight-mebibyte child.
    pub(crate) fn split_from(
        owner: &mut crate::resources::ScribeMemoryLease,
    ) -> Result<Self, ScribeError> {
        Ok(Self {
            reservation: Some(owner.split(PARQUET_FOOTER_CHILD_BYTES).map_err(|error| {
                ScribeError::Internal {
                    detail: error.to_string(),
                }
            })?),
        })
    }

    /// Transfers the footer child from the complete producer ownership tuple.
    ///
    /// The checked delta supplies the child when it owns at least eight MiB.
    /// Larger immutable generations already carry that memory, so the token
    /// records a category transition without double charging the governor.
    ///
    /// # Errors
    /// Returns an internal error only when neither reservation nor immutable
    /// ownership can cover the exact footer child.
    pub(crate) fn transfer_from(
        owner: &mut crate::resources::ScribeMemoryLease,
        immutable_bytes: usize,
    ) -> Result<Self, ScribeError> {
        if owner.bytes() >= PARQUET_FOOTER_CHILD_BYTES {
            return Self::split_from(owner);
        }
        if immutable_bytes >= PARQUET_FOOTER_CHILD_BYTES {
            return Ok(Self { reservation: None });
        }
        Err(ScribeError::Internal {
            detail: "complete producer owner cannot supply its encoded-footer child".to_owned(),
        })
    }

    /// Returns the exact bytes retained by this child.
    #[must_use]
    pub(crate) fn bytes(&self) -> usize {
        self.reservation.as_ref().map_or(
            PARQUET_FOOTER_CHILD_BYTES,
            crate::resources::ScribeMemoryLease::bytes,
        )
    }

    /// Constructs an isolated exact footer child for pure encoder tests.
    ///
    /// # Panics
    /// Panics only if the fixed test governor cannot admit its exact footer
    /// child, which would mean the production memory invariant regressed.
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            768 * 1024 * 1024,
            512 * 1024 * 1024,
            [crate::resources::BifrostRole::Scribe],
        );
        let reservation = roles
            .scribe()
            .expect("footer test Scribe capability")
            .try_reserve_maintenance(MemoryCategory::Persistence, PARQUET_FOOTER_CHILD_BYTES)
            .expect("footer test child must fit the production floor");
        Self {
            reservation: Some(reservation),
        }
    }
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
/// and is not stored directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySnapshot {
    /// Total pod memory budget.
    pub pod_limit_bytes: usize,
    /// Exact managed-memory ceiling supplied by the root resource owner.
    pub bifrost_limit_bytes: usize,
    /// Parent Bifrost bytes currently charged.
    pub bifrost_total_bytes: usize,
    /// Scribe child bytes currently charged.
    pub scribe_total_bytes: usize,
    /// Scribe soft reservation limit.
    pub scribe_limit_bytes: usize,
    /// Oracle child bytes currently charged.
    pub oracle_total_bytes: usize,
    /// Legacy Oracle process-reservation limit used outside query envelopes.
    pub oracle_limit_bytes: usize,
    /// Total charged bytes by category in enum order.
    pub categories: [usize; MEMORY_CATEGORY_COUNT],
    /// Current cgroup resident usage when the kernel exposes it.
    pub cgroup_current_bytes: Option<usize>,
    /// Cgroup memory limit used for the external-pressure tripwire.
    pub cgroup_limit_bytes: Option<usize>,
    /// Current ingress bytes charged (the value ingress admission counts).
    ///
    /// This equals `scribe_total_bytes`: ingress admission charges the whole
    /// Scribe child total against the ingress ceiling, so the pressure-seal
    /// watermark decision keys on this value rather than
    /// [`MemorySnapshot::effective_pressure_percent`] (D83).
    pub ingress_occupancy_bytes: usize,
    /// Ingress reservation ceiling (`limit_bytes() - persistence_headroom`).
    ///
    /// This is the denominator the watermark decision divides by; it is smaller
    /// than `scribe_limit_bytes`, so a run pinned at the ingress ceiling can sit
    /// below the effective-pressure threshold yet be at 100% ingress occupancy.
    pub ingress_limit_bytes: usize,
    /// High-water byte threshold = `ingress_limit_bytes * high_water / 100`.
    ///
    /// Populated by [`MemorySnapshot::with_ingress_watermarks`] from the
    /// runtime `ScribePressureConfig`; a bare governor snapshot leaves it `0`.
    pub ingress_high_water_bytes: usize,
    /// Low-water byte target = `ingress_limit_bytes * low_water / 100`.
    ///
    /// The release target pressure sealing drains toward. Populated by
    /// [`MemorySnapshot::with_ingress_watermarks`]; `0` on a bare snapshot.
    pub ingress_low_water_bytes: usize,
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

    /// Ingress occupancy as a percent of the ingress ceiling.
    ///
    /// This is `ingress_occupancy_bytes * 100 / ingress_limit_bytes` with a
    /// saturating multiply and an `ingress_limit_bytes.max(1)` denominator so a
    /// degenerate zero ceiling reports 100% rather than dividing by zero. This
    /// is the only ratio the pressure-seal watermark decision consults;
    /// [`MemorySnapshot::effective_pressure_percent`] (whose denominator is the
    /// larger `scribe_limit_bytes`) is deliberately not used for admission (D83).
    #[must_use]
    pub fn ingress_occupancy_percent(self) -> usize {
        self.ingress_occupancy_bytes
            .saturating_mul(100)
            .checked_div(self.ingress_limit_bytes.max(1))
            .unwrap_or(100)
    }

    /// Return a copy of this snapshot with the ingress watermark bytes filled
    /// from the runtime high/low-water percents.
    ///
    /// The governor snapshot populates `ingress_occupancy_bytes` and
    /// `ingress_limit_bytes` but cannot know the runtime
    /// [`crate::scribe::ScribePressureConfig`] percents, so the Scribe runtime
    /// applies them here before making a watermark decision (and before T40
    /// exports the four ingress gauges). `high_water_percent` and
    /// `low_water_percent` are percents of `ingress_limit_bytes`; the caller is
    /// responsible for the `low_water < high_water` invariant, which
    /// [`crate::scribe::ScribePressureConfig`] enforces at construction.
    #[must_use]
    pub fn with_ingress_watermarks(
        mut self,
        high_water_percent: usize,
        low_water_percent: usize,
    ) -> Self {
        self.ingress_high_water_bytes =
            self.ingress_limit_bytes.saturating_mul(high_water_percent) / 100;
        self.ingress_low_water_bytes =
            self.ingress_limit_bytes.saturating_mul(low_water_percent) / 100;
        self
    }

    /// Exports the closed Scribe ingress-watermark lifecycle gauges.
    ///
    /// Emitted from the production steady-state age scanner on every tick so a
    /// dashboard can read the D83 ingress watermarks without a debugger.
    /// `bifrost_scribe_ingress_watermark_bytes` carries the four D83 ingress
    /// marks under a closed `mark` label `{occupancy, limit, high_water,
    /// low_water}` — the exact numerator, denominator, and hysteresis band the
    /// pressure-seal decision keys on. No label carries tenant, table, or
    /// request identity. The snapshot should already be watermarked via
    /// [`Self::with_ingress_watermarks`]; an unwatermarked snapshot reports the
    /// two watermark marks as `0`.
    pub fn emit_ingress_watermark_gauges(self) {
        let as_f64 = |bytes: usize| bytes.to_f64().unwrap_or(f64::MAX);
        metrics::gauge!("bifrost_scribe_ingress_watermark_bytes", "mark" => "occupancy")
            .set(as_f64(self.ingress_occupancy_bytes));
        metrics::gauge!("bifrost_scribe_ingress_watermark_bytes", "mark" => "limit")
            .set(as_f64(self.ingress_limit_bytes));
        metrics::gauge!("bifrost_scribe_ingress_watermark_bytes", "mark" => "high_water")
            .set(as_f64(self.ingress_high_water_bytes));
        metrics::gauge!("bifrost_scribe_ingress_watermark_bytes", "mark" => "low_water")
            .set(as_f64(self.ingress_low_water_bytes));
    }

    /// Decide the pressure-seal release target from the ingress watermarks.
    ///
    /// This is the single, pure hysteresis decision shared by the admission
    /// path and the periodic age scanner (both call it through
    /// [`crate::scribe::ScribeImpl`]), so the seal contract is defined once:
    ///
    /// * Below `high_water_percent` of the ingress ceiling, or already at/below
    ///   the low-water byte target, it returns `None` — the no-op half of the
    ///   band that keeps the decision from thrashing on every tick.
    /// * At or above the high-water mark it returns `Some(bytes_to_release)`,
    ///   where the release target is the low-water byte mark filled by
    ///   [`Self::with_ingress_watermarks`]:
    ///   `ingress_occupancy_bytes - ingress_low_water_bytes` (saturating).
    ///
    /// The comparison uses [`Self::ingress_occupancy_percent`], whose
    /// denominator is `ingress_limit_bytes`, not the larger `scribe_limit_bytes`
    /// of [`Self::effective_pressure_percent`] (D83).
    #[must_use]
    pub fn pressure_release_bytes(self, high_water_percent: usize) -> Option<usize> {
        if self.ingress_occupancy_percent() < high_water_percent {
            return None;
        }
        let to_release = self
            .ingress_occupancy_bytes
            .saturating_sub(self.ingress_low_water_bytes);
        (to_release > 0).then_some(to_release)
    }
}

/// Closed identity of the memory ceiling that rejected a reservation (D84).
///
/// Every distinct reservation ceiling maps to exactly one variant so that a
/// user-visible ingress rejection can be labelled by the true limit that
/// tripped, instead of collapsing five physically distinct ceilings under one
/// constant `reason="memory"` label. The [`Self::as_metric_label`] values are
/// the only accepted values of the ceiling-labelled rejection metric; the label
/// set is closed and carries no tenant or table identity.
///
/// The ingress admission path constructs [`Self::CgroupBreaker`],
/// [`Self::IngressSublimit`], and [`Self::BifrostParent`] (see
/// the root-issued Scribe ingress capability); [`Self::ScribeChild`] is the
/// child-limit form used by the non-ingress reservation paths, and
/// [`Self::CgroupParent`] identifies the parent cgroup tripwire. All five are a
/// normative D84 interface, not implementor latitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeRejectionCeiling {
    /// The Scribe-child cgroup breaker tripped at or above 90% container usage.
    CgroupBreaker,
    /// A raw Scribe-child reservation exceeded the full child limit.
    ScribeChild,
    /// An ingress reservation exceeded the ingress sublimit (`limit - headroom`).
    IngressSublimit,
    /// A reservation exceeded the parent Bifrost ceiling.
    BifrostParent,
    /// The parent cgroup tripwire tripped at 100% container usage.
    CgroupParent,
}
impl ScribeRejectionCeiling {
    /// Return the stable `snake_case` label used by ceiling-rejection telemetry.
    ///
    /// The returned string is the value attached as the `reason` label on
    /// `bifrost_scribe_rejections_total` for a ceiling-labelled rejection; the
    /// five values form the closed label set and must stay stable across
    /// changes.
    #[must_use]
    pub fn as_metric_label(self) -> &'static str {
        match self {
            Self::CgroupBreaker => "cgroup_breaker",
            Self::ScribeChild => "scribe_child",
            Self::IngressSublimit => "ingress_sublimit",
            Self::BifrostParent => "bifrost_parent",
            Self::CgroupParent => "cgroup_parent",
        }
    }
}

/// Shared active/immutable ownership ledger for shard-owned Arrow buffers.
#[derive(Debug, Clone)]
pub struct ScribeOwnership {
    active: Arc<Mutex<crate::resources::ScribeMemoryLease>>,
    immutable: Arc<Mutex<crate::resources::ScribeMemoryLease>>,
    lifecycle: Arc<Mutex<ScribeGenerationLifecycleSnapshot>>,
}

/// Fixed-size lifecycle observations emitted by the enforcing generation owner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScribeGenerationLifecycleSnapshot {
    /// Active or replay generations planned from exact Arrow ownership.
    pub plans: u64,
    /// Exact bytes represented by completed generation plans.
    pub planned_bytes: usize,
    /// Active or replay reservations adopted by the generation ledger.
    pub reservations: u64,
    /// Exact bytes adopted by those reservations.
    pub reserved_bytes: usize,
    /// Materialized active or replay generations admitted to the memtable.
    pub materializations: u64,
    /// Exact Arrow bytes materialized by those generations.
    pub materialized_bytes: usize,
    /// Active generations transferred into immutable persistence ownership.
    pub transfers: u64,
    /// Exact Arrow bytes transferred into immutable ownership.
    pub transferred_bytes: usize,
    /// Replay generations reconstructed directly into immutable ownership.
    pub replay_materializations: u64,
    /// Exact Arrow bytes reconstructed during replay.
    pub replay_materialized_bytes: usize,
    /// Immutable-to-active rollback transitions.
    pub rollbacks: u64,
    /// Generation reservations terminally released.
    pub releases: u64,
    /// Exact active or immutable bytes terminally released.
    pub released_bytes: usize,
    /// Exact active Arrow bytes currently retained.
    pub active_bytes: usize,
    /// Exact immutable Arrow bytes currently retained through persistence or retirement.
    pub immutable_bytes: usize,
}

impl ScribeOwnership {
    /// Create zero-sized active and immutable reservations on one governor.
    pub fn new(governor: &crate::resources::ScribeResources) -> Result<Self, ScribeError> {
        Ok(Self {
            active: Arc::new(Mutex::new(
                governor.try_reserve_maintenance(MemoryCategory::Active, 0)?,
            )),
            immutable: Arc::new(Mutex::new(
                governor.try_reserve_maintenance(MemoryCategory::Immutable, 0)?,
            )),
            lifecycle: Arc::new(Mutex::new(ScribeGenerationLifecycleSnapshot::default())),
        })
    }

    /// Applies one scalar observation transition without creating a second governor.
    fn observe(&self, update: impl FnOnce(&mut ScribeGenerationLifecycleSnapshot)) {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update(&mut lifecycle);
    }

    /// Returns the generation lifecycle facts emitted by this enforcing owner.
    #[must_use]
    pub(crate) fn lifecycle_snapshot(&self) -> ScribeGenerationLifecycleSnapshot {
        *self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Grow the active Arrow ownership reservation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when checked byte arithmetic or the enforcing
    /// root reservation fails.
    pub fn reserve_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let target = active
            .bytes()
            .checked_add(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "active memory ledger byte count overflow".to_owned(),
            })?;
        active.resize_ingress(target)?;
        self.observe(|lifecycle| {
            lifecycle.plans = lifecycle.plans.saturating_add(1);
            lifecycle.planned_bytes = lifecycle.planned_bytes.saturating_add(bytes);
            lifecycle.reservations = lifecycle.reservations.saturating_add(1);
            lifecycle.reserved_bytes = lifecycle.reserved_bytes.saturating_add(bytes);
            lifecycle.materializations = lifecycle.materializations.saturating_add(1);
            lifecycle.materialized_bytes = lifecycle.materialized_bytes.saturating_add(bytes);
            lifecycle.active_bytes = lifecycle.active_bytes.saturating_add(bytes);
        });
        Ok(())
    }

    /// Adopt an already-accounted active reservation after memtable insertion.
    ///
    /// The caller transfers ownership of the reservation; this method only
    /// joins its accounting with the ledger and never reserves the bytes a
    /// second time.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when category transfer, identity clearing, or
    /// exact root-owner merging fails.
    pub(crate) fn absorb_active(
        &self,
        mut reservation: crate::resources::ScribeMemoryLease,
    ) -> Result<(), ScribeError> {
        let bytes = reservation.bytes();
        reservation.transfer_category(MemoryCategory::Active)?;
        reservation
            .clear_identity_attribution()
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        active
            .merge(reservation)
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        self.observe(|lifecycle| {
            lifecycle.plans = lifecycle.plans.saturating_add(1);
            lifecycle.planned_bytes = lifecycle.planned_bytes.saturating_add(bytes);
            lifecycle.reservations = lifecycle.reservations.saturating_add(1);
            lifecycle.reserved_bytes = lifecycle.reserved_bytes.saturating_add(bytes);
            lifecycle.materializations = lifecycle.materializations.saturating_add(1);
            lifecycle.materialized_bytes = lifecycle.materialized_bytes.saturating_add(bytes);
            lifecycle.active_bytes = lifecycle.active_bytes.saturating_add(bytes);
        });
        Ok(())
    }

    /// Release active Arrow ownership after an insertion failure.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the active ledger cannot cover `bytes` or
    /// the exact root release fails.
    pub fn release_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let target = active
            .bytes()
            .checked_sub(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "active memory ledger byte count underflow".to_owned(),
            })?;
        active.resize_ingress(target)?;
        self.observe(|lifecycle| {
            lifecycle.releases = lifecycle.releases.saturating_add(1);
            lifecycle.released_bytes = lifecycle.released_bytes.saturating_add(bytes);
            lifecycle.active_bytes = lifecycle.active_bytes.saturating_sub(bytes);
        });
        Ok(())
    }

    /// Check active ledger ownership before a coordinated cleanup releases it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the governor if the
    /// ledger cannot cover `bytes` or its lock is poisoned.
    pub(crate) fn preflight_release_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned during release preflight".to_owned(),
        })?;
        if active.bytes() < bytes {
            active.poison();
            return Err(ScribeError::Internal {
                detail: "active memory ledger underflow during release preflight".to_owned(),
            });
        }
        active.preflight_release(bytes)
    }

    /// Move Arrow ownership from writable buckets to immutable generations.
    ///
    /// The transfer is a net-zero category move: the bytes stay charged against
    /// the shared Scribe and Bifrost pools throughout, so nothing is released to
    /// the pool and nothing is re-reserved. This closes the shrink-then-grow
    /// race a concurrent reservation could otherwise win at the ceiling — the
    /// pre-D97 form shrank Active (releasing the bytes to the pool) and then
    /// grew Immutable (re-reserving them), and an ingress `try_reserve` racing
    /// into the transiently freed headroom made the grow fail exactly when the
    /// pod sat at ceiling and pressure seals ran. Because the category move
    /// never touches the pool totals it can no longer fail on a ceiling, only on
    /// a poisoned ledger lock, and it therefore needs no rollback.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the active or immutable ledger
    /// lock is poisoned. Poisoning indicates a prior panic inside a ledger
    /// critical section — a broken invariant, not a recoverable accounting
    /// failure — and no partial category move can have occurred because the
    /// method mutates nothing before both locks are held.
    pub fn move_active_to_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        active.transfer_bytes_to(&mut immutable, bytes)?;
        self.observe(|lifecycle| {
            lifecycle.transfers = lifecycle.transfers.saturating_add(1);
            lifecycle.transferred_bytes = lifecycle.transferred_bytes.saturating_add(bytes);
            lifecycle.active_bytes = lifecycle.active_bytes.saturating_sub(bytes);
            lifecycle.immutable_bytes = lifecycle.immutable_bytes.saturating_add(bytes);
        });
        Ok(())
    }

    /// Check active and immutable ledger ownership before a lifecycle transfer.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the governor when the
    /// source cannot cover `bytes`, the target would overflow, or either lock
    /// is poisoned.
    pub(crate) fn preflight_move_active_to_immutable(
        &self,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned during move preflight".to_owned(),
        })?;
        let immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned during move preflight".to_owned(),
        })?;
        if active.bytes() < bytes || immutable.bytes().checked_add(bytes).is_none() {
            active.poison();
            return Err(ScribeError::Internal {
                detail: "active-to-immutable ledger move failed preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Move Arrow ownership back to writable buckets after rollback.
    ///
    /// Symmetric net-zero counterpart to [`Self::move_active_to_immutable`]: the
    /// bytes stay charged against the shared pools while their category flips
    /// from immutable back to active, so a post-commit abort's accounting
    /// rollback cannot fail on a ceiling and needs no compensating resize. The
    /// lock order (active before immutable) matches the forward move so the two
    /// methods share one lock ordering and add no deadlock surface.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the active or immutable ledger
    /// lock is poisoned; no partial category move can have occurred because the
    /// method mutates nothing before both locks are held.
    pub fn move_immutable_to_active(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned".to_owned(),
        })?;
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        immutable.transfer_bytes_to(&mut active, bytes)?;
        self.observe(|lifecycle| {
            lifecycle.rollbacks = lifecycle.rollbacks.saturating_add(1);
            lifecycle.immutable_bytes = lifecycle.immutable_bytes.saturating_sub(bytes);
            lifecycle.active_bytes = lifecycle.active_bytes.saturating_add(bytes);
        });
        Ok(())
    }

    /// Check immutable-to-active ledger ownership before a post-commit abort.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the governor when the
    /// immutable source cannot cover `bytes`, the active target would overflow,
    /// or either ledger lock is poisoned.
    pub(crate) fn preflight_move_immutable_to_active(
        &self,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let active = self.active.lock().map_err(|_| ScribeError::Internal {
            detail: "active memory ledger lock poisoned during reverse move preflight".to_owned(),
        })?;
        let immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned during reverse move preflight"
                .to_owned(),
        })?;
        if immutable.bytes() < bytes || active.bytes().checked_add(bytes).is_none() {
            active.poison();
            return Err(ScribeError::Internal {
                detail: "immutable-to-active ledger move failed preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Release immutable Arrow ownership after grace expiry.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the immutable ledger cannot cover `bytes`
    /// or the exact root release fails.
    pub fn release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let target = immutable
            .bytes()
            .checked_sub(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "immutable memory ledger byte count underflow".to_owned(),
            })?;
        immutable.resize_ingress(target)?;
        self.observe(|lifecycle| {
            lifecycle.releases = lifecycle.releases.saturating_add(1);
            lifecycle.released_bytes = lifecycle.released_bytes.saturating_add(bytes);
            lifecycle.immutable_bytes = lifecycle.immutable_bytes.saturating_sub(bytes);
        });
        Ok(())
    }

    /// Check immutable ledger ownership before explicit retirement mutates it.
    pub(crate) fn preflight_release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned during preflight".to_owned(),
        })?;
        if immutable.bytes() < bytes {
            immutable.poison();
            return Err(ScribeError::Internal {
                detail: "immutable memory ledger underflow during preflight".to_owned(),
            });
        }
        immutable.preflight_release(bytes)
    }

    /// Poison the shared governor after an impossible post-preflight mutation.
    ///
    /// Explicit retirement performs all recoverable checks before releasing
    /// ownership. If the subsequent token commit nevertheless fails, the
    /// accounting state may have partially advanced and all future admission
    /// must fail closed.
    pub(crate) fn poison(&self) {
        if let Ok(immutable) = self.immutable.lock() {
            immutable.poison();
        }
    }

    /// Reserve immutable ownership during boot replay.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when checked byte arithmetic or the enforcing
    /// replay reservation fails.
    pub fn reserve_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let mut immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned".to_owned(),
        })?;
        let target = immutable
            .bytes()
            .checked_add(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "immutable memory ledger byte count overflow".to_owned(),
            })?;
        immutable.resize_ingress(target)?;
        self.observe(|lifecycle| {
            lifecycle.plans = lifecycle.plans.saturating_add(1);
            lifecycle.planned_bytes = lifecycle.planned_bytes.saturating_add(bytes);
            lifecycle.reservations = lifecycle.reservations.saturating_add(1);
            lifecycle.reserved_bytes = lifecycle.reserved_bytes.saturating_add(bytes);
            lifecycle.materializations = lifecycle.materializations.saturating_add(1);
            lifecycle.materialized_bytes = lifecycle.materialized_bytes.saturating_add(bytes);
            lifecycle.replay_materializations = lifecycle.replay_materializations.saturating_add(1);
            lifecycle.replay_materialized_bytes =
                lifecycle.replay_materialized_bytes.saturating_add(bytes);
            lifecycle.immutable_bytes = lifecycle.immutable_bytes.saturating_add(bytes);
        });
        Ok(())
    }

    /// Read the active-category byte total for accounting assertions in tests.
    ///
    /// Exposes the ledger's active reservation size so seal-path tests can pin
    /// the two-legal-states invariant (Active-accounted before a successful
    /// move, transferred out after). Test-only; no production caller reads a
    /// category total directly.
    ///
    /// # Panics
    /// Panics if the active ledger lock is poisoned; a poisoned ledger is a
    /// broken invariant that a test should surface loudly.
    #[cfg(test)]
    pub(crate) fn active_bytes(&self) -> usize {
        self.active
            .lock()
            .expect("active memory ledger lock poisoned")
            .bytes()
    }

    /// Read the immutable-category byte total for accounting assertions in
    /// tests.
    ///
    /// Counterpart to [`Self::active_bytes`]; together they let a seal-path
    /// test assert the net-zero category move (state A has zero immutable
    /// bytes; state B has the frozen bytes). Test-only.
    ///
    /// # Panics
    /// Panics if the immutable ledger lock is poisoned.
    #[cfg(test)]
    pub(crate) fn immutable_bytes(&self) -> usize {
        self.immutable
            .lock()
            .expect("immutable memory ledger lock poisoned")
            .bytes()
    }

    /// Reports the shared governor poison state for owner-level fault tests.
    pub(crate) fn is_poisoned(&self) -> bool {
        self.active
            .lock()
            .expect("active memory ledger lock invariant for inspection")
            .is_poisoned()
    }
}

pub(crate) fn read_cgroup_limit() -> Option<usize> {
    [
        "/sys/fs/cgroup/memory.max",
        "/sys/fs/cgroup/memory/memory.limit_in_bytes",
    ]
    .into_iter()
    .find_map(read_memory_limit)
}

pub(crate) fn read_cgroup_current() -> Option<usize> {
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

#[cfg(test)]
/// Focused ownership and lifecycle reconciliation proofs.
mod tests {
    use super::ScribeOwnership;

    /// Generation observations follow the enforcing active/immutable owner exactly.
    #[test]
    fn generation_lifecycle_reconciles_transfer_replay_and_retirement() {
        let resources =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let ownership = ScribeOwnership::new(&resources).expect("generation owner");

        ownership.reserve_active(128).expect("active reservation");
        ownership
            .move_active_to_immutable(128)
            .expect("persistence transfer");
        ownership
            .move_immutable_to_active(128)
            .expect("retry rollback");
        ownership
            .move_active_to_immutable(128)
            .expect("retry transfer");
        ownership
            .release_immutable(128)
            .expect("retirement release");
        ownership
            .reserve_immutable(64)
            .expect("replay materialization");
        ownership.release_immutable(64).expect("replay retirement");

        let lifecycle = ownership.lifecycle_snapshot();
        assert_eq!(lifecycle.plans, 2);
        assert_eq!(lifecycle.planned_bytes, 192);
        assert_eq!(lifecycle.reservations, 2);
        assert_eq!(lifecycle.reserved_bytes, 192);
        assert_eq!(lifecycle.materializations, 2);
        assert_eq!(lifecycle.materialized_bytes, 192);
        assert_eq!(lifecycle.transfers, 2);
        assert_eq!(lifecycle.transferred_bytes, 256);
        assert_eq!(lifecycle.replay_materializations, 1);
        assert_eq!(lifecycle.replay_materialized_bytes, 64);
        assert_eq!(lifecycle.rollbacks, 1);
        assert_eq!(lifecycle.releases, 2);
        assert_eq!(lifecycle.released_bytes, 192);
        assert_eq!(lifecycle.active_bytes, 0);
        assert_eq!(lifecycle.immutable_bytes, 0);
        assert_eq!(resources.memory_snapshot().total_bytes(), 0);
    }
}
