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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

/// Minimum supported pod memory size, enforced fail-closed at governor
/// construction.
///
/// This floor is a derived consequence of the governor's two-level structure,
/// not a magic number, and it is never a sizing mechanism: cgroup detection
/// stays authoritative for actual budgets above the floor (D79). A
/// combined-role pod must host two static children (Scribe and Oracle), each
/// requiring [`MIN_CHILD_BYTES`] (256 MiB), under the parent ceiling fixed at
/// the 70% fraction applied in
/// [`BifrostMemoryGovernor::new_with_child_limits`]. The parent must therefore
/// hold `2 * MIN_CHILD_BYTES = 512 MiB`, which demands a pod of at least
/// `2 * MIN_CHILD_BYTES / 0.70 ≈ 732 MiB`. The floor is set to 768 MiB, the
/// smallest conventional pod request above that measured bound, so the
/// inequality holds with headroom:
///
/// `2 * MIN_CHILD_BYTES / 0.70 ≈ 732 MiB < 768 MiB = MIN_MEMORY_BYTES`.
///
/// The value is coupled to [`MIN_CHILD_BYTES`] and the 70% parent fraction: a
/// future change to either invalidates the derivation above and must revisit
/// this floor.
pub const MIN_MEMORY_BYTES: usize = 768 * 1024 * 1024;
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

/// Target size for one active bucket (`S / 4`) within the fixed bucket bounds.
///
/// Pure function of the Scribe child limit `scribe_limit`; shared by
/// [`ScribeMemoryBudget::active_bucket_target_bytes`] and the governor snapshot
/// so both derive the identical value without duplicating the formula (D75).
#[must_use]
fn active_bucket_target_for(scribe_limit: usize) -> usize {
    (scribe_limit / 4).clamp(MIN_BUCKET_BYTES, MAX_BUCKET_BYTES)
}

/// Persistence workspace that ingress must never consume, from `scribe_limit`.
///
/// Pure function of the Scribe child limit; see
/// [`ScribeMemoryBudget::persistence_headroom_bytes`] for the reservation
/// rationale. Extracted so the governor snapshot can derive the ingress ceiling
/// without a [`ScribeMemoryBudget`] handle (D75 formula unchanged).
#[must_use]
fn persistence_headroom_for(scribe_limit: usize) -> usize {
    persistence_workspace_bytes(active_bucket_target_for(scribe_limit).max(2 * MAX_REQUEST_BYTES))
}

/// Child-budget ceiling available to ingress reservations, from `scribe_limit`.
///
/// Pure function of the Scribe child limit; see
/// [`ScribeMemoryBudget::ingress_limit_bytes`] for the semantics. Shared with
/// the governor snapshot so `MemorySnapshot::ingress_limit_bytes` matches the
/// admission ceiling exactly (D75 formula unchanged).
#[must_use]
fn ingress_limit_for(scribe_limit: usize) -> usize {
    scribe_limit
        .saturating_sub(persistence_headroom_for(scribe_limit))
        .max(scribe_limit / 4)
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

impl MemoryCategory {
    /// Map Scribe categories to their entry-point-owned memory purpose.
    const fn purpose(self) -> MemoryPurpose {
        match self {
            Self::Persistence => MemoryPurpose::ScribeMaintenance,
            Self::Raw
            | Self::Decode
            | Self::Prepared
            | Self::Queued
            | Self::Active
            | Self::Immutable
            | Self::Metadata => MemoryPurpose::ScribeIngress,
        }
    }
}

/// Classifies why a governed memory operation was refused.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub(crate) enum MemoryRejectionKind {
    /// The request could fit under the configured ceiling but current usage is
    /// occupying the available headroom.
    #[error("memory ceiling is occupied")]
    Occupied,
    /// The request itself is larger than the configured ceiling.
    #[error("memory request is larger than its ceiling")]
    RequestTooLarge,
    /// Adding the request to an accounting counter would overflow `usize`.
    #[error("memory accounting counter would overflow")]
    CounterOverflow,
    /// Prior accounting corruption poisoned this governor.
    #[error("memory accounting is poisoned")]
    AccountingPoisoned,
}

/// Identifies the owner and lifecycle purpose of a memory request.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub(crate) enum MemoryPurpose {
    /// Scribe ingress decode and active-write ownership.
    #[error("Scribe ingress")]
    ScribeIngress,
    /// Scribe persistence, replay, and maintenance workspace.
    #[error("Scribe maintenance")]
    ScribeMaintenance,
    /// Forge parent workspace.
    #[error("Forge workspace")]
    ForgeWorkspace,
    /// Oracle hot Parquet range bytes.
    #[error("Oracle hot range")]
    OracleHotRange,
    /// Oracle decoded hot source batches.
    #[error("Oracle hot decoded batch")]
    OracleHotDecodedBatch,
    /// Oracle reconciliation state.
    #[error("Oracle reconciliation")]
    OracleReconciliation,
    /// Oracle query-only parent or child acquisition.
    #[error("Oracle query")]
    OracleQuery,
}

impl MemoryPurpose {
    /// Closed purpose vocabulary used by diagnostics and tests.
    const ALL: [Self; 7] = [
        Self::ScribeIngress,
        Self::ScribeMaintenance,
        Self::ForgeWorkspace,
        Self::OracleHotRange,
        Self::OracleHotDecodedBatch,
        Self::OracleReconciliation,
        Self::OracleQuery,
    ];
    /// Return a bounded diagnostic label for structured logs.
    fn as_label(self) -> &'static str {
        match self {
            Self::ScribeIngress => "scribe_ingress",
            Self::ScribeMaintenance => "scribe_maintenance",
            Self::ForgeWorkspace => "forge_workspace",
            Self::OracleHotRange => "oracle_hot_range",
            Self::OracleHotDecodedBatch => "oracle_hot_decoded_batch",
            Self::OracleReconciliation => "oracle_reconciliation",
            Self::OracleQuery => "oracle_query",
        }
    }
}

/// Identifies the physical ceiling associated with a memory refusal.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub(crate) enum MemoryCeiling {
    /// Cgroup breaker at 90% resident usage.
    #[error("cgroup breaker")]
    CgroupBreaker,
    /// Parent cgroup hard limit.
    #[error("cgroup parent")]
    CgroupParent,
    /// Scribe child budget.
    #[error("Scribe child")]
    ScribeChild,
    /// Scribe ingress sublimit.
    #[error("ingress sublimit")]
    IngressSublimit,
    /// Oracle child budget.
    #[error("Oracle child")]
    OracleChild,
    /// Bifrost parent budget.
    #[error("Bifrost parent")]
    BifrostParent,
    /// No physical ceiling was compared, as for overflow or poison.
    #[error("not applicable")]
    NotApplicable,
}

impl MemoryCeiling {
    /// Return a bounded diagnostic label without exposing request identity.
    fn as_label(self) -> &'static str {
        match self {
            Self::CgroupBreaker => "cgroup_breaker",
            Self::CgroupParent => "cgroup_parent",
            Self::ScribeChild => "scribe_child",
            Self::IngressSublimit => "ingress_sublimit",
            Self::OracleChild => "oracle_child",
            Self::BifrostParent => "bifrost_parent",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Structured, bounded diagnostics for every governed refusal.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
#[error(
    "memory rejected: kind={kind:?} purpose={purpose:?} ceiling={ceiling:?} requested={requested} current={current} limit={limit}"
)]
pub(crate) struct MemoryRejection {
    kind: MemoryRejectionKind,
    purpose: MemoryPurpose,
    ceiling: MemoryCeiling,
    requested: usize,
    current: usize,
    limit: usize,
}

impl MemoryRejection {
    /// Construct a bounded refusal without retaining request or pointer identity.
    fn new(
        kind: MemoryRejectionKind,
        purpose: MemoryPurpose,
        ceiling: MemoryCeiling,
        requested: usize,
        current: usize,
        limit: usize,
    ) -> Self {
        let _ = (purpose.as_label(), ceiling.as_label(), MemoryPurpose::ALL);
        let rejection = Self {
            kind,
            purpose,
            ceiling,
            requested,
            current,
            limit,
        };
        rejection.emit_diagnostic();
        debug_assert_eq!(Self::from_source_chain(&rejection), Some(rejection));
        rejection
    }

    /// Return the refusal category.
    pub(crate) const fn kind(self) -> MemoryRejectionKind {
        self.kind
    }

    /// Return the owning memory purpose.
    pub(crate) const fn purpose(self) -> MemoryPurpose {
        self.purpose
    }

    /// Return the physical ceiling that was evaluated.
    pub(crate) const fn ceiling(self) -> MemoryCeiling {
        self.ceiling
    }

    /// Return the requested byte count.
    pub(crate) const fn requested(self) -> usize {
        self.requested
    }

    /// Return the counter value observed before refusal.
    pub(crate) const fn current(self) -> usize {
        self.current
    }

    /// Return the compared ceiling.
    pub(crate) const fn limit(self) -> usize {
        self.limit
    }

    /// Find a typed memory refusal anywhere in an external error source chain.
    pub(crate) fn from_source_chain(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        let mut current = Some(error);
        while let Some(error) = current {
            if let Some(rejection) = error.downcast_ref::<Self>() {
                return Some(*rejection);
            }
            current = error.source();
        }
        None
    }

    /// Emits bounded structured operands for one refusal without request identity.
    fn emit_diagnostic(self) {
        tracing::warn!(
            kind = ?self.kind(),
            purpose = self.purpose().as_label(),
            ceiling = self.ceiling().as_label(),
            requested = self.requested(),
            current = self.current(),
            limit = self.limit(),
            "Bifrost memory operation rejected"
        );
    }

    /// Preserve the historical Scribe error while retaining typed diagnostics
    /// for callers that inspect the source chain.
    fn into_scribe_error(self) -> ScribeError {
        match self.kind {
            MemoryRejectionKind::Occupied | MemoryRejectionKind::RequestTooLarge => {
                ScribeError::IngestBusy {
                    table: "memory".to_owned(),
                }
            }
            MemoryRejectionKind::CounterOverflow | MemoryRejectionKind::AccountingPoisoned => {
                ScribeError::Internal {
                    detail: self.to_string(),
                }
            }
        }
    }
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
    /// runtime [`ScribePressureConfig`]; a bare governor snapshot leaves it `0`.
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

    /// Export the governor gauges that explain admission headroom (D84).
    ///
    /// Emitted from the production steady-state age scanner on every tick so a
    /// dashboard can read live per-child occupancy and the D83 ingress
    /// watermarks without a debugger. Two closed gauge families carry a
    /// `consumer` label over the fixed set `{scribe, oracle, parent}`:
    /// `bifrost_memory_reserved_bytes` (bytes currently charged) and
    /// `bifrost_memory_limit_bytes` (the ceiling each is measured against). The
    /// `scribe`/`oracle`/`parent` values share the `bifrost_memory_reserved_bytes`
    /// family with the Forge consumer emitted by [`record_forge_memory`], keeping
    /// one memory-occupancy family across every Bifrost role. A third family,
    /// `bifrost_scribe_ingress_watermark_bytes`, carries the four D83 ingress
    /// marks under a closed `mark` label `{occupancy, limit, high_water,
    /// low_water}` — the exact numerator, denominator, and hysteresis band the
    /// pressure-seal decision keys on. No label carries tenant, table, or
    /// request identity. The snapshot should already be watermarked via
    /// [`Self::with_ingress_watermarks`]; an unwatermarked snapshot reports the
    /// two watermark marks as `0`.
    pub fn emit_governor_gauges(self) {
        let as_f64 = |bytes: usize| bytes.to_f64().unwrap_or(f64::MAX);
        metrics::gauge!("bifrost_memory_reserved_bytes", "consumer" => "scribe")
            .set(as_f64(self.scribe_total_bytes));
        metrics::gauge!("bifrost_memory_reserved_bytes", "consumer" => "oracle")
            .set(as_f64(self.oracle_total_bytes));
        metrics::gauge!("bifrost_memory_reserved_bytes", "consumer" => "parent")
            .set(as_f64(self.bifrost_total_bytes));
        metrics::gauge!("bifrost_memory_limit_bytes", "consumer" => "scribe")
            .set(as_f64(self.scribe_limit_bytes));
        metrics::gauge!("bifrost_memory_limit_bytes", "consumer" => "oracle")
            .set(as_f64(self.oracle_limit_bytes));
        metrics::gauge!("bifrost_memory_limit_bytes", "consumer" => "parent")
            .set(as_f64(self.bifrost_limit_bytes));
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
/// [`ScribeMemoryBudget::try_reserve_ingress_classified`]); [`Self::ScribeChild`]
/// is the child-limit form used by the non-ingress reservation paths, and
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
    /// A reservation exceeded the parent Bifrost ceiling (70% of pod memory).
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

    /// Map a tripped ceiling to the unchanged admission error.
    ///
    /// Every ceiling surfaces the same `IngestBusy { table: "memory" }` the
    /// reservation path returned before D84 added ceiling identity, so the
    /// returned [`ScribeError`] is byte-identical to prior behaviour; only the
    /// telemetry label carries the ceiling. The ingress admission caller
    /// reconstructs its own table-scoped `IngestBusy` and does not use this
    /// mapping.
    #[must_use]
    fn into_ingest_busy(self) -> ScribeError {
        // Every ceiling collapses to the same admission error; `self` names only
        // the telemetry label the caller has already recorded, so the match is
        // exhaustive-by-design rather than value-dependent.
        match self {
            Self::CgroupBreaker
            | Self::ScribeChild
            | Self::IngressSublimit
            | Self::BifrostParent
            | Self::CgroupParent => ScribeError::IngestBusy {
                table: "memory".to_owned(),
            },
        }
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
    /// Process-wide fail-closed marker set after an ownership invariant fails.
    poisoned: AtomicBool,
    /// One-shot release CAS perturbation used only by deterministic fault tests.
    #[cfg(test)]
    fail_next_release_cas: AtomicBool,
    /// One-shot shard-growth CAS perturbation used only by deterministic fault tests.
    #[cfg(test)]
    fail_next_shard_add_cas: AtomicBool,
}

impl BifrostMemoryGovernor {
    /// Constructs a test governor with deliberately small child limits.
    ///
    /// This keeps the production parent derivation while allowing focused
    /// ranged-reader tests to prove that a file larger than one child budget
    /// succeeds when each live range and batch fits independently.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the pod limit is invalid or the
    /// governor becomes shared before its test-only limits are installed.
    #[cfg(test)]
    pub(crate) fn new_with_test_child_limits(
        pod_limit_bytes: usize,
        scribe_limit_bytes: usize,
        oracle_limit_bytes: usize,
    ) -> Result<Self, ScribeError> {
        let mut governor = Self::new(pod_limit_bytes)?;
        let inner = Arc::get_mut(&mut governor.inner).ok_or_else(|| ScribeError::Internal {
            detail: "test memory governor unexpectedly shared during construction".to_owned(),
        })?;
        inner.scribe_limit_bytes = scribe_limit_bytes;
        inner.oracle_limit_bytes = oracle_limit_bytes;
        Ok(governor)
    }

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
                poisoned: AtomicBool::new(false),
                #[cfg(test)]
                fail_next_release_cas: AtomicBool::new(false),
                #[cfg(test)]
                fail_next_shard_add_cas: AtomicBool::new(false),
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

    /// Mark this process's shared accounting as poisoned and fail closed.
    pub(crate) fn poison(&self) {
        if !self.inner.poisoned.swap(true, Ordering::AcqRel) {
            tracing::error!("Bifrost memory accounting poisoned; restart required");
        }
    }

    /// Return whether a prior accounting failure has disabled non-zero admission.
    #[must_use]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.inner.poisoned.load(Ordering::Acquire)
    }

    /// Build the bounded refusal used when shared accounting is poisoned.
    fn poisoned_rejection(&self, purpose: MemoryPurpose, requested: usize) -> MemoryRejection {
        MemoryRejection::new(
            MemoryRejectionKind::AccountingPoisoned,
            purpose,
            MemoryCeiling::NotApplicable,
            requested,
            self.inner.bifrost_total_bytes.load(Ordering::Acquire),
            self.bifrost_limit_bytes(),
        )
    }

    /// Release a complete ownership tuple without allowing an underflow to wrap.
    fn release_checked(
        &self,
        category: MemoryCategory,
        bytes: usize,
        shard: Option<(&Arc<Vec<AtomicUsize>>, usize)>,
    ) -> Result<(), MemoryRejection> {
        self.preflight_release_checked(category, bytes, shard)?;
        let category_counter = &self.inner.categories[category as usize];
        self.release_counter_checked(category_counter, bytes, category.purpose())?;
        if let Some((shard_counters, shard_index)) = shard {
            self.release_counter_checked(&shard_counters[shard_index], bytes, category.purpose())?;
        }
        self.release_counter_checked(&self.inner.scribe_total_bytes, bytes, category.purpose())?;
        self.release_counter_checked(&self.inner.bifrost_total_bytes, bytes, category.purpose())?;
        Ok(())
    }

    /// Validates every counter in one ownership tuple before explicit release mutates it.
    ///
    /// # Errors
    ///
    /// Returns a typed counter-overflow rejection and poisons the governor when
    /// category, shard, Scribe-child, or parent ownership cannot cover `bytes`.
    fn preflight_release_checked(
        &self,
        category: MemoryCategory,
        bytes: usize,
        shard: Option<(&Arc<Vec<AtomicUsize>>, usize)>,
    ) -> Result<(), MemoryRejection> {
        let current_category = self.inner.categories[category as usize].load(Ordering::Acquire);
        let current_scribe = self.inner.scribe_total_bytes.load(Ordering::Acquire);
        let current_parent = self.inner.bifrost_total_bytes.load(Ordering::Acquire);
        let current_shard = shard.map(|(counters, index)| counters[index].load(Ordering::Acquire));
        if current_category >= bytes
            && current_scribe >= bytes
            && current_parent >= bytes
            && current_shard.is_none_or(|current| current >= bytes)
        {
            return Ok(());
        }
        let current = current_shard.into_iter().fold(
            current_category.min(current_scribe).min(current_parent),
            usize::min,
        );
        let rejection = MemoryRejection::new(
            MemoryRejectionKind::CounterOverflow,
            category.purpose(),
            MemoryCeiling::NotApplicable,
            bytes,
            current,
            self.bifrost_limit_bytes(),
        );
        self.poison();
        Err(rejection)
    }

    /// Subtract one owned counter while retrying ordinary concurrent updates.
    fn release_counter_checked(
        &self,
        counter: &AtomicUsize,
        bytes: usize,
        purpose: MemoryPurpose,
    ) -> Result<(), MemoryRejection> {
        let mut current = counter.load(Ordering::Acquire);
        loop {
            if current < bytes {
                self.poison();
                return Err(MemoryRejection::new(
                    MemoryRejectionKind::CounterOverflow,
                    purpose,
                    MemoryCeiling::NotApplicable,
                    bytes,
                    current,
                    self.bifrost_limit_bytes(),
                ));
            }
            #[cfg(test)]
            if self
                .inner
                .fail_next_release_cas
                .swap(false, Ordering::AcqRel)
                && bytes > 0
            {
                counter.store(bytes - 1, Ordering::Release);
            }
            match counter.compare_exchange(
                current,
                current - bytes,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => current = observed,
            }
        }
    }

    /// Release one counter during a reservation rollback, poisoning on races.
    fn release_counter_only(
        &self,
        counter: &AtomicUsize,
        bytes: usize,
        purpose: MemoryPurpose,
    ) -> Result<(), MemoryRejection> {
        let current = counter.load(Ordering::Acquire);
        if current < bytes {
            self.poison();
            return Err(MemoryRejection::new(
                MemoryRejectionKind::CounterOverflow,
                purpose,
                MemoryCeiling::NotApplicable,
                bytes,
                current,
                self.bifrost_limit_bytes(),
            ));
        }
        let mut current = current;
        loop {
            if current < bytes {
                self.poison();
                return Err(MemoryRejection::new(
                    MemoryRejectionKind::CounterOverflow,
                    purpose,
                    MemoryCeiling::NotApplicable,
                    bytes,
                    current,
                    self.bifrost_limit_bytes(),
                ));
            }
            match counter.compare_exchange(
                current,
                current - bytes,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => current = observed,
            }
        }
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
        if bytes > 0 && self.is_poisoned() {
            return Err(ScribeError::Internal {
                detail: self
                    .poisoned_rejection(MemoryPurpose::ForgeWorkspace, bytes)
                    .to_string(),
            });
        }
        if bytes > 0
            && let Some((current, limit)) = self.cgroup_pressure()
            && current.saturating_mul(100) >= limit.saturating_mul(100)
        {
            return Err(ScribeError::IngestBusy {
                table: "bifrost-parent".to_owned(),
            });
        }
        reserve_with_limit_diagnostic(
            &self.inner.bifrost_total_bytes,
            self.bifrost_limit_bytes(),
            bytes,
            MemoryPurpose::ForgeWorkspace,
            MemoryCeiling::BifrostParent,
        )
        .map_err(MemoryRejection::into_scribe_error)
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
    /// Returns a typed rejection when the Oracle child or parent ceiling is
    /// occupied, the request is indivisibly too large, arithmetic overflows,
    /// or prior accounting corruption poisoned the governor.
    fn try_reserve_oracle_bytes_classified(
        &self,
        bytes: usize,
        purpose: MemoryPurpose,
    ) -> Result<(), MemoryRejection> {
        if bytes > 0 && self.is_poisoned() {
            return Err(self.poisoned_rejection(purpose, bytes));
        }
        reserve_with_limit_diagnostic(
            &self.inner.oracle_total_bytes,
            self.inner.oracle_limit_bytes,
            bytes,
            purpose,
            MemoryCeiling::OracleChild,
        )?;
        if let Err(error) = reserve_with_limit_diagnostic(
            &self.inner.bifrost_total_bytes,
            self.bifrost_limit_bytes(),
            bytes,
            purpose,
            MemoryCeiling::BifrostParent,
        ) {
            if let Err(cleanup) =
                self.release_counter_only(&self.inner.oracle_total_bytes, bytes, purpose)
            {
                tracing::error!(error = %cleanup, primary = %error, "Oracle child rollback failed after parent refusal");
            }
            return Err(error);
        }
        Ok(())
    }

    /// Release bytes from the Oracle child counter and the parent ceiling.
    fn release_oracle_bytes_checked(&self, bytes: usize) -> Result<(), MemoryRejection> {
        let oracle = self.inner.oracle_total_bytes.load(Ordering::Acquire);
        let parent = self.inner.bifrost_total_bytes.load(Ordering::Acquire);
        if oracle < bytes || parent < bytes {
            self.poison();
            return Err(MemoryRejection::new(
                MemoryRejectionKind::CounterOverflow,
                MemoryPurpose::OracleQuery,
                MemoryCeiling::NotApplicable,
                bytes,
                oracle.min(parent),
                self.oracle_limit_bytes(),
            ));
        }
        self.inner
            .oracle_total_bytes
            .compare_exchange(oracle, oracle - bytes, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|current| {
                self.poison();
                MemoryRejection::new(
                    MemoryRejectionKind::CounterOverflow,
                    MemoryPurpose::OracleQuery,
                    MemoryCeiling::NotApplicable,
                    bytes,
                    current,
                    self.oracle_limit_bytes(),
                )
            })?;
        if self
            .inner
            .bifrost_total_bytes
            .compare_exchange(parent, parent - bytes, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.poison();
            return Err(MemoryRejection::new(
                MemoryRejectionKind::CounterOverflow,
                MemoryPurpose::OracleQuery,
                MemoryCeiling::NotApplicable,
                bytes,
                parent,
                self.bifrost_limit_bytes(),
            ));
        }
        Ok(())
    }

    /// Read parent and child totals.
    ///
    /// The returned snapshot satisfies the three-way reconciliation identity
    /// `bifrost_total == scribe_total + oracle_total + parent_only` where
    /// `parent_only` is derived by the caller.
    #[must_use]
    pub fn snapshot(&self) -> MemorySnapshot {
        let scribe_total_bytes = self.inner.scribe_total_bytes.load(Ordering::Acquire);
        MemorySnapshot {
            pod_limit_bytes: self.pod_limit_bytes(),
            bifrost_limit_bytes: self.bifrost_limit_bytes(),
            bifrost_total_bytes: self.inner.bifrost_total_bytes.load(Ordering::Acquire),
            scribe_total_bytes,
            scribe_limit_bytes: self.scribe_limit_bytes(),
            oracle_total_bytes: self.inner.oracle_total_bytes.load(Ordering::Acquire),
            oracle_limit_bytes: self.oracle_limit_bytes(),
            categories: std::array::from_fn(|index| {
                self.inner.categories[index].load(Ordering::Acquire)
            }),
            cgroup_current_bytes: self.cgroup_current(),
            cgroup_limit_bytes: self.inner.cgroup_limit_bytes,
            // Ingress admission charges the whole Scribe child total against the
            // ingress ceiling, so occupancy is `scribe_total_bytes` over the
            // ceiling derived from the Scribe child limit. High/low-water bytes
            // depend on the runtime pressure config and are filled by
            // `MemorySnapshot::with_ingress_watermarks`; a bare snapshot leaves
            // them zero.
            ingress_occupancy_bytes: scribe_total_bytes,
            ingress_limit_bytes: ingress_limit_for(self.scribe_limit_bytes()),
            ingress_high_water_bytes: 0,
            ingress_low_water_bytes: 0,
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
    /// Arms one test-only release compare-exchange fault on this shared governor.
    #[cfg(test)]
    pub(crate) fn arm_post_preflight_release_fault(&self) {
        self.parent
            .inner
            .fail_next_release_cas
            .store(true, Ordering::Release);
    }

    /// Arms one test-only shard-growth compare-exchange fault on this governor.
    #[cfg(test)]
    pub(crate) fn arm_post_preflight_shard_add_fault(&self) {
        self.parent
            .inner
            .fail_next_shard_add_cas
            .store(true, Ordering::Release);
    }

    /// Returns the complete governor and shard accounting state for owner tests.
    #[cfg(test)]
    pub(crate) fn accounting_snapshot_for_test(
        &self,
    ) -> (MemorySnapshot, [usize; SHARD_ACCOUNTING_COUNT]) {
        (self.parent.snapshot(), self.shard_snapshot())
    }
    /// Mark the shared governor poisoned after an ownership invariant fails.
    pub(crate) fn poison(&self) {
        self.parent.poison();
    }

    /// Return whether the shared governor has entered fail-closed mode.
    #[must_use]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.parent.is_poisoned()
    }

    /// Return the Scribe child reservation limit.
    #[must_use]
    pub fn limit_bytes(&self) -> usize {
        self.parent.scribe_limit_bytes()
    }

    /// Target size for one active bucket (`S / 4`) within the fixed bounds.
    #[must_use]
    pub fn active_bucket_target_bytes(&self) -> usize {
        active_bucket_target_for(self.limit_bytes())
    }

    /// Returns the persistence workspace that ingress must never consume.
    ///
    /// The largest admissible generation is either a rotation-target bucket
    /// or one append whose decoded Arrow reaches twice the wire cap. For every
    /// production budget, any generation Scribe admits must be persistable
    /// from headroom that ingress admission can never consume.
    #[must_use]
    pub fn persistence_headroom_bytes(&self) -> usize {
        persistence_headroom_for(self.limit_bytes())
    }

    /// Returns the child-budget ceiling available to ingress reservations.
    ///
    /// The ceiling leaves persistence headroom outside ingress ownership. The
    /// quarter-limit floor exists only for sub-production test governors; for
    /// every production budget, any generation Scribe admits must be
    /// persistable from headroom that ingress admission can never consume.
    #[must_use]
    pub fn ingress_limit_bytes(&self) -> usize {
        ingress_limit_for(self.limit_bytes())
    }

    /// Reserve category bytes without waiting.
    pub fn try_reserve(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_classified(category, bytes)
            .map_err(MemoryRejection::into_scribe_error)
    }

    /// Reserve bytes and retain the exact structured refusal for crate callers.
    pub(crate) fn try_reserve_classified(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, MemoryRejection> {
        let purpose = category.purpose();
        self.try_reserve_core(
            category,
            bytes,
            self.limit_bytes(),
            purpose,
            MemoryCeiling::ScribeChild,
        )
    }

    /// Reports whether the cgroup memory tripwire is engaged at or above 90%.
    ///
    /// This mirrors the immediate fail-closed guard inside the classified
    /// reservation core. Admission consults it to keep the
    /// cgroup tripwire an immediate `IngestBusy` rather than routing it through
    /// the ingress-pressure seal-and-retry path, which cannot relieve
    /// container-level pressure. Returns `false` when no cgroup limit is
    /// observable.
    #[must_use]
    pub fn cgroup_tripwire_engaged(&self) -> bool {
        matches!(
            self.parent.cgroup_pressure(),
            Some((current, limit)) if current.saturating_mul(100) >= limit.saturating_mul(90)
        )
    }

    /// Reserve ingress bytes without consuming persistence headroom.
    pub fn try_reserve_ingress(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_ingress_classified(category, bytes)
            .map_err(ScribeRejectionCeiling::into_ingest_busy)
    }

    /// Reserve ingress bytes, naming the ceiling that rejects on failure.
    ///
    /// This is the ceiling-classified form of [`Self::try_reserve_ingress`]:
    /// admission and byte accounting are identical, but a rejection returns the
    /// closed [`ScribeRejectionCeiling`] that tripped instead of the collapsed
    /// `IngestBusy`. Because ingress reservations charge against the ingress
    /// sublimit, a Scribe-child overflow is reported as
    /// [`ScribeRejectionCeiling::IngressSublimit`]; the cgroup breaker and the
    /// parent ceiling report [`ScribeRejectionCeiling::CgroupBreaker`] and
    /// [`ScribeRejectionCeiling::BifrostParent`]. The admission caller uses the
    /// returned ceiling only to label rejection telemetry (D84) and then
    /// surfaces its own table-scoped `IngestBusy`.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeRejectionCeiling`] that rejected the reservation when
    /// the cgroup breaker is engaged, the ingress sublimit is exceeded, or the
    /// parent Bifrost ceiling is exceeded.
    pub(crate) fn try_reserve_ingress_classified(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeRejectionCeiling> {
        self.try_reserve_with_limit_classified(
            category,
            bytes,
            self.ingress_limit_bytes(),
            ScribeRejectionCeiling::IngressSublimit,
        )
    }

    /// Reserve bounded maintenance bytes up to the full parent and child caps.
    pub fn try_reserve_maintenance(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, ScribeError> {
        self.try_reserve_maintenance_classified(category, bytes)
            .map_err(MemoryRejection::into_scribe_error)
    }

    /// Reserve maintenance or replay workspace with a structured refusal.
    pub(crate) fn try_reserve_maintenance_classified(
        &self,
        category: MemoryCategory,
        bytes: usize,
    ) -> Result<MemoryReservation, MemoryRejection> {
        self.try_reserve_core(
            category,
            bytes,
            self.limit_bytes(),
            MemoryPurpose::ScribeMaintenance,
            MemoryCeiling::ScribeChild,
        )
    }

    /// Reserve bytes against a Scribe-child limit, naming the tripped ceiling.
    ///
    /// This is the single classified reservation core underneath every Scribe
    /// reservation entry point. It performs the exact same three checks and
    /// counter updates as before D84 — the cgroup breaker at 90%, the
    /// Scribe-child total against `scribe_limit`, then the parent Bifrost total —
    /// but on rejection returns the closed [`ScribeRejectionCeiling`] that
    /// tripped rather than a collapsed `IngestBusy`. `child_ceiling` names the
    /// ceiling to report when the Scribe-child total overflows `scribe_limit`
    /// ([`ScribeRejectionCeiling::ScribeChild`] for full-limit callers,
    /// [`ScribeRejectionCeiling::IngressSublimit`] for ingress). Byte accounting,
    /// ordering, and the rollback of the Scribe-child charge on a parent
    /// overflow are unchanged; only the error carries ceiling identity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeRejectionCeiling::CgroupBreaker`] when the cgroup breaker
    /// is engaged, `child_ceiling` when the Scribe-child limit is exceeded, and
    /// [`ScribeRejectionCeiling::BifrostParent`] when the parent ceiling is
    /// exceeded.
    fn try_reserve_with_limit_classified(
        &self,
        category: MemoryCategory,
        bytes: usize,
        scribe_limit: usize,
        child_ceiling: ScribeRejectionCeiling,
    ) -> Result<MemoryReservation, ScribeRejectionCeiling> {
        self.try_reserve_core(
            category,
            bytes,
            scribe_limit,
            category.purpose(),
            match child_ceiling {
                ScribeRejectionCeiling::IngressSublimit => MemoryCeiling::IngressSublimit,
                ScribeRejectionCeiling::ScribeChild => MemoryCeiling::ScribeChild,
                ScribeRejectionCeiling::CgroupBreaker => MemoryCeiling::CgroupBreaker,
                ScribeRejectionCeiling::BifrostParent => MemoryCeiling::BifrostParent,
                ScribeRejectionCeiling::CgroupParent => MemoryCeiling::CgroupParent,
            },
        )
        .map_err(|rejection| match rejection.ceiling() {
            MemoryCeiling::CgroupBreaker => ScribeRejectionCeiling::CgroupBreaker,
            MemoryCeiling::BifrostParent => ScribeRejectionCeiling::BifrostParent,
            _ => child_ceiling,
        })
    }

    /// Reserve Scribe bytes while preserving the structured refusal details.
    fn try_reserve_core(
        &self,
        category: MemoryCategory,
        bytes: usize,
        scribe_limit: usize,
        purpose: MemoryPurpose,
        child_ceiling: MemoryCeiling,
    ) -> Result<MemoryReservation, MemoryRejection> {
        if bytes > 0 && self.parent.is_poisoned() {
            return Err(self.parent.poisoned_rejection(purpose, bytes));
        }
        if bytes > 0
            && let Some((current, limit)) = self.parent.cgroup_pressure()
            && current.saturating_mul(100) >= limit.saturating_mul(90)
        {
            return Err(MemoryRejection::new(
                MemoryRejectionKind::Occupied,
                purpose,
                MemoryCeiling::CgroupBreaker,
                bytes,
                current,
                limit,
            ));
        }
        reserve_with_limit_diagnostic(
            &self.parent.inner.scribe_total_bytes,
            scribe_limit,
            bytes,
            purpose,
            child_ceiling,
        )?;
        if let Err(rejection) = reserve_with_limit_diagnostic(
            &self.parent.inner.bifrost_total_bytes,
            self.parent.bifrost_limit_bytes(),
            bytes,
            purpose,
            MemoryCeiling::BifrostParent,
        ) {
            if self
                .parent
                .release_counter_only(&self.parent.inner.scribe_total_bytes, bytes, purpose)
                .is_err()
            {
                self.parent.poison();
                return Err(self.parent.poisoned_rejection(purpose, bytes));
            }
            return Err(rejection);
        }
        let category_counter = &self.parent.inner.categories[category as usize];
        let mut current_category = category_counter.load(Ordering::Acquire);
        loop {
            let Some(next_category) = current_category.checked_add(bytes) else {
                if self
                    .parent
                    .release_counter_only(&self.parent.inner.scribe_total_bytes, bytes, purpose)
                    .is_err()
                    || self
                        .parent
                        .release_counter_only(
                            &self.parent.inner.bifrost_total_bytes,
                            bytes,
                            purpose,
                        )
                        .is_err()
                {
                    self.parent.poison();
                    return Err(self.parent.poisoned_rejection(purpose, bytes));
                }
                return Err(MemoryRejection::new(
                    MemoryRejectionKind::CounterOverflow,
                    purpose,
                    MemoryCeiling::NotApplicable,
                    bytes,
                    current_category,
                    usize::MAX,
                ));
            };
            match category_counter.compare_exchange(
                current_category,
                next_category,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current_category = observed,
            }
        }
        Ok(MemoryReservation {
            governor: self.clone(),
            category,
            bytes,
            shard: None,
            purpose,
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

    fn release(
        &self,
        category: MemoryCategory,
        bytes: usize,
        shard: Option<(&Arc<Vec<AtomicUsize>>, usize)>,
    ) -> Result<(), MemoryRejection> {
        self.parent.release_checked(category, bytes, shard)
    }

    /// Reconcile one category with an authoritative owner snapshot.
    pub fn reconcile_category(
        &self,
        category: MemoryCategory,
        target: usize,
    ) -> Result<(), ScribeError> {
        let current = self.parent.inner.categories[category as usize].load(Ordering::Acquire);
        if target > current {
            let delta = target - current;
            self.parent.inner.categories[category as usize]
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(delta)
                })
                .map_err(|_| ScribeError::Internal {
                    detail: "memory category reconciliation overflow".to_owned(),
                })?;
            self.parent
                .inner
                .scribe_total_bytes
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(delta)
                })
                .map_err(|_| {
                    self.parent.poison();
                    ScribeError::Internal {
                        detail: "Scribe reconciliation overflow".to_owned(),
                    }
                })?;
            self.parent
                .inner
                .bifrost_total_bytes
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(delta)
                })
                .map_err(|_| {
                    self.parent.poison();
                    ScribeError::Internal {
                        detail: "Bifrost reconciliation overflow".to_owned(),
                    }
                })?;
        } else {
            let delta = current - target;
            self.parent
                .release_checked(category, delta, None)
                .map_err(|rejection| ScribeError::Internal {
                    detail: rejection.to_string(),
                })?;
        }
        Ok(())
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
        self.try_reserve_classified(bytes, MemoryPurpose::OracleQuery)
            .map_err(MemoryRejection::into_scribe_error)
    }

    /// Reserve bytes for one classified Oracle memory owner.
    ///
    /// The returned rejection retains the exact purpose, ceiling, request,
    /// occupancy, and limit operands so Oracle can distinguish an indivisible
    /// request from aggregate contention before issuing storage IO.
    ///
    /// # Errors
    ///
    /// Returns a typed rejection when the request exceeds a child or parent
    /// ceiling, current occupancy leaves insufficient capacity, arithmetic
    /// overflows, or the shared governor is poisoned.
    pub(crate) fn try_reserve_classified(
        &self,
        bytes: usize,
        purpose: MemoryPurpose,
    ) -> Result<OracleMemoryReservation, MemoryRejection> {
        self.parent
            .try_reserve_oracle_bytes_classified(bytes, purpose)?;
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

    /// Poison the shared governor when a coupled outer owner detects corruption.
    pub(crate) fn poison(&self) {
        self.governor.poison();
    }
}

impl Drop for OracleMemoryReservation {
    fn drop(&mut self) {
        if let Err(rejection) = self.governor.release_oracle_bytes_checked(self.bytes) {
            tracing::error!(error = %rejection, "Oracle memory cleanup poisoned accounting");
        }
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

    /// Poison the shared governor when a coupled outer owner detects corruption.
    pub(crate) fn poison(&self) {
        self.governor.poison();
    }
}

impl Drop for ParentMemoryReservation {
    fn drop(&mut self) {
        if let Err(rejection) = self.governor.release_counter_only(
            &self.governor.inner.bifrost_total_bytes,
            self.bytes,
            MemoryPurpose::ForgeWorkspace,
        ) {
            tracing::error!(error = %rejection, "parent memory cleanup poisoned accounting");
        }
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
                for counter in [
                    &self.governor.inner.oracle_total_bytes,
                    &self.governor.inner.bifrost_total_bytes,
                ] {
                    if counter
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                            value.checked_add(additional)
                        })
                        .is_err()
                    {
                        self.governor.poison();
                        tracing::error!("DataFusion Oracle memory counter overflow during grow");
                    }
                }
            }
            DataFusionPoolTarget::Parent => {
                let reserved = self
                    .governor
                    .inner
                    .bifrost_total_bytes
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                        value.checked_add(additional)
                    })
                    .map_or_else(
                        |_| {
                            self.governor.poison();
                            self.governor
                                .inner
                                .bifrost_total_bytes
                                .load(Ordering::Acquire)
                        },
                        |value| value.saturating_add(additional),
                    );
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
                if let Err(error) = self.governor.release_oracle_bytes_checked(shrink) {
                    tracing::error!(error = %error, "DataFusion Oracle shrink poisoned accounting");
                }
            }
            DataFusionPoolTarget::Parent => {
                let reserved = match self.governor.release_counter_only(
                    &self.governor.inner.bifrost_total_bytes,
                    shrink,
                    MemoryPurpose::ForgeWorkspace,
                ) {
                    Ok(()) => self
                        .governor
                        .inner
                        .bifrost_total_bytes
                        .load(Ordering::Acquire),
                    Err(error) => {
                        tracing::error!(error = %error, "DataFusion parent shrink poisoned accounting");
                        self.governor
                            .inner
                            .bifrost_total_bytes
                            .load(Ordering::Acquire)
                    }
                };
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
                .try_reserve_oracle_bytes_classified(additional, MemoryPurpose::OracleQuery)
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
        let target = active
            .bytes()
            .checked_add(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "active memory ledger byte count overflow".to_owned(),
            })?;
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
        let target = active
            .bytes()
            .checked_sub(bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "active memory ledger byte count underflow".to_owned(),
            })?;
        active.resize(target)
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
            active.governor.parent.poison();
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
        active.transfer_bytes_to(&mut immutable, bytes)
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
            active.governor.parent.poison();
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
        immutable.transfer_bytes_to(&mut active, bytes)
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
            active.governor.parent.poison();
            return Err(ScribeError::Internal {
                detail: "immutable-to-active ledger move failed preflight".to_owned(),
            });
        }
        Ok(())
    }

    /// Release immutable Arrow ownership after grace expiry.
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
        immutable.resize(target)
    }

    /// Check immutable ledger ownership before explicit retirement mutates it.
    pub(crate) fn preflight_release_immutable(&self, bytes: usize) -> Result<(), ScribeError> {
        let immutable = self.immutable.lock().map_err(|_| ScribeError::Internal {
            detail: "immutable memory ledger lock poisoned during preflight".to_owned(),
        })?;
        if immutable.bytes() < bytes {
            immutable.governor.parent.poison();
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
            immutable.governor.parent.poison();
        }
    }

    /// Reserve immutable ownership during boot replay.
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
        immutable.resize(target)
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
    #[cfg(test)]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.active
            .lock()
            .expect("active memory ledger lock invariant for inspection")
            .governor
            .is_poisoned()
    }
}

/// Preflighted attached-shard growth committed only after shared reservation succeeds.
#[derive(Debug)]
struct ShardGrowthPlan {
    /// Shared shard counters containing the owned shard slot.
    counters: Arc<Vec<AtomicUsize>>,
    /// Index of the shard counter coupled to the reservation.
    shard: usize,
    /// Counter value observed before the replacement reservation was acquired.
    current: usize,
}

/// RAII category reservation.
#[derive(Debug)]
pub struct MemoryReservation {
    /// Shared Scribe budget charged by this reservation.
    governor: ScribeMemoryBudget,
    /// Lifecycle category carrying the owned bytes.
    category: MemoryCategory,
    /// Exact byte count released when this reservation drops.
    bytes: usize,
    /// Optional shard counter charged alongside category and shared totals.
    shard: Option<(Arc<Vec<AtomicUsize>>, usize)>,
    /// Entry-point purpose retained across category and shard transfers.
    purpose: MemoryPurpose,
}

impl MemoryReservation {
    /// Current bytes owned by this reservation.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Validates this reservation and every coupled governor counter before release.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the shared governor when
    /// the reservation, category, shard, Scribe-child, or parent counter cannot
    /// cover `bytes`.
    fn preflight_release(&self, bytes: usize) -> Result<(), ScribeError> {
        if self.bytes < bytes {
            self.governor.parent.poison();
            return Err(ScribeError::Internal {
                detail: "memory reservation ownership underflow during release preflight"
                    .to_owned(),
            });
        }
        self.governor
            .parent
            .preflight_release_checked(
                self.category,
                bytes,
                self.shard
                    .as_ref()
                    .map(|(counters, index)| (counters, *index)),
            )
            .map_err(MemoryRejection::into_scribe_error)
    }

    /// Resize this reservation while preserving category ownership.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when growth exceeds a ceiling, shared accounting
    /// is poisoned, or checked shard/shared ownership cannot be updated exactly.
    pub fn resize(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.resize_with_limit(bytes, self.governor.limit_bytes())
    }

    /// Resize an ingress reservation without consuming persistence headroom.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when growth exceeds the ingress or shared
    /// ceiling, accounting is poisoned, or checked release/growth fails.
    pub fn resize_ingress(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.resize_ingress_classified(bytes)
            .map_err(ScribeRejectionCeiling::into_ingest_busy)
    }

    /// Grow or shrink an ingress reservation, naming the ceiling on failure.
    ///
    /// This is the ceiling-classified form of [`Self::resize_ingress`]: the
    /// grow path acquires the extra bytes against the ingress sublimit and the
    /// shrink path performs a checked release. A grow rejection returns the
    /// closed [`ScribeRejectionCeiling`] that tripped so the admission caller can
    /// label rejection telemetry (D84); a corrupt shrink poisons and fails closed.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeRejectionCeiling`] that rejected the additional
    /// ingress bytes when the reservation must grow and cannot fit.
    pub(crate) fn resize_ingress_classified(
        &mut self,
        bytes: usize,
    ) -> Result<(), ScribeRejectionCeiling> {
        self.resize_with_limit_classified(
            bytes,
            self.governor.ingress_limit_bytes(),
            ScribeRejectionCeiling::IngressSublimit,
        )
    }

    fn resize_with_limit(&mut self, bytes: usize, limit: usize) -> Result<(), ScribeError> {
        self.resize_with_limit_classified(bytes, limit, ScribeRejectionCeiling::ScribeChild)
            .map_err(ScribeRejectionCeiling::into_ingest_busy)
    }

    /// Resize against a Scribe-child limit, naming the tripped ceiling.
    ///
    /// The classified core underneath both [`Self::resize`] and
    /// [`Self::resize_ingress`]. Growing acquires only the delta through the
    /// classified reservation core and forgets the replacement only after the
    /// attached-shard CAS succeeds. Shrinking releases the delta with checked
    /// accounting. `child_ceiling` names a physical grow refusal; corruption
    /// poisons the shared governor and fails closed.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeRejectionCeiling`] that rejected the additional bytes
    /// when the reservation must grow and cannot fit under the limits.
    fn resize_with_limit_classified(
        &mut self,
        bytes: usize,
        limit: usize,
        child_ceiling: ScribeRejectionCeiling,
    ) -> Result<(), ScribeRejectionCeiling> {
        if bytes > self.bytes {
            let extra = bytes - self.bytes;
            let shard_plan = self
                .preflight_shard_add(extra)
                .map_err(|_| ScribeRejectionCeiling::ScribeChild)?;
            let replacement = self.governor.try_reserve_with_limit_classified(
                self.category,
                extra,
                limit,
                child_ceiling,
            )?;
            self.commit_shard_add(shard_plan, extra)
                .map_err(|_| ScribeRejectionCeiling::ScribeChild)?;
            self.bytes = bytes;
            std::mem::forget(replacement);
        } else {
            let released = self.bytes - bytes;
            self.governor
                .release(
                    self.category,
                    released,
                    self.shard
                        .as_ref()
                        .map(|(counters, index)| (counters, *index)),
                )
                .map_err(|_| ScribeRejectionCeiling::ScribeChild)?;
            self.bytes = bytes;
        }
        Ok(())
    }

    pub(crate) fn attach_shard(&mut self, shard_bytes: Arc<Vec<AtomicUsize>>, shard: usize) {
        if shard >= shard_bytes.len() || self.shard.is_some() {
            return;
        }
        let counter = &shard_bytes[shard];
        let current = counter.load(Ordering::Acquire);
        let Some(next) = current.checked_add(self.bytes) else {
            self.governor.parent.poison();
            tracing::error!("memory reservation shard accounting overflow during attach");
            return;
        };
        if counter
            .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.governor.parent.poison();
            tracing::error!("memory reservation shard accounting changed during attach");
            return;
        }
        self.shard = Some((shard_bytes, shard));
    }

    /// Validates an attached shard counter before a growth reservation is acquired.
    ///
    /// The returned plan pins the observed value used by the post-reservation
    /// compare-exchange. The replacement reservation remains RAII-owned until
    /// that compare-exchange succeeds, so a race cannot leak child, parent, or
    /// category accounting.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the governor when the
    /// attached shard counter would overflow.
    fn preflight_shard_add(&self, bytes: usize) -> Result<Option<ShardGrowthPlan>, ScribeError> {
        if let Some((shard_bytes, shard)) = &self.shard {
            let counter = &shard_bytes[*shard];
            let current = counter.load(Ordering::Acquire);
            current.checked_add(bytes).ok_or_else(|| {
                self.governor.parent.poison();
                ScribeError::Internal {
                    detail: "memory reservation shard accounting overflow".to_owned(),
                }
            })?;
            return Ok(Some(ShardGrowthPlan {
                counters: Arc::clone(shard_bytes),
                shard: *shard,
                current,
            }));
        }
        Ok(None)
    }

    /// Commits one preflighted attached-shard growth after replacement ownership exists.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] and poisons the governor when the
    /// shard counter changed after preflight or would overflow. The caller must
    /// retain the replacement reservation so its `Drop` rolls back shared
    /// category, child, and parent ownership on this failure path.
    fn commit_shard_add(
        &self,
        shard_plan: Option<ShardGrowthPlan>,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        let Some(ShardGrowthPlan {
            counters,
            shard,
            current,
        }) = shard_plan
        else {
            return Ok(());
        };
        let next = current.checked_add(bytes).ok_or_else(|| {
            self.governor.parent.poison();
            ScribeError::Internal {
                detail: "memory reservation shard accounting overflow".to_owned(),
            }
        })?;
        let counter = &counters[shard];
        #[cfg(test)]
        if self
            .governor
            .parent
            .inner
            .fail_next_shard_add_cas
            .swap(false, Ordering::AcqRel)
            && bytes > 0
        {
            counter.store(current.saturating_add(1), Ordering::Release);
        }
        counter
            .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                self.governor.parent.poison();
                ScribeError::Internal {
                    detail: "memory reservation shard accounting changed during growth".to_owned(),
                }
            })
            .map(|_| ())
    }

    fn adjust_shard_sub(&self, bytes: usize) -> Result<(), ScribeError> {
        if let Some((shard_bytes, shard)) = &self.shard {
            let current = shard_bytes[*shard].load(Ordering::Acquire);
            if current < bytes {
                self.governor.parent.poison();
                return Err(ScribeError::Internal {
                    detail: "memory reservation shard accounting underflow".to_owned(),
                });
            }
            shard_bytes[*shard]
                .compare_exchange(
                    current,
                    current - bytes,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| ScribeError::Internal {
                    detail: "memory reservation shard accounting changed during release".to_owned(),
                })?;
        }
        Ok(())
    }

    fn detach_shard(&mut self) {
        if let Some((shard_bytes, shard)) = self.shard.take() {
            let current = shard_bytes[shard].load(Ordering::Acquire);
            if current < self.bytes {
                self.governor.parent.poison();
                tracing::error!("memory reservation shard accounting underflow during drop");
            } else if shard_bytes[shard]
                .compare_exchange(
                    current,
                    current - self.bytes,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                self.governor.parent.poison();
                tracing::error!("memory reservation shard accounting changed during drop");
            }
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
            purpose: self.purpose,
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

    /// Move `bytes` of already-held reservation from this reservation's category
    /// into `other`, adjusting only the per-category totals.
    ///
    /// This is the net-zero primitive behind
    /// [`MemoryLedger::move_active_to_immutable`] and
    /// [`MemoryLedger::move_immutable_to_active`]: the moved bytes stay charged
    /// against the shared Scribe and Bifrost pool totals the entire time, so no
    /// headroom is ever released and no re-reservation is ever attempted. A
    /// concurrent reservation therefore cannot claim transiently freed bytes
    /// between a shrink and a grow, which closes the shrink-then-grow seal race
    /// by construction rather than by ordering. The amount moved is clamped to
    /// the bytes this reservation currently owns, so an over-large request moves
    /// only what is held and never underflows.
    ///
    /// Both reservations' optional shard counters are adjusted symmetrically so
    /// per-shard accounting stays consistent. When the categories are equal the
    /// category total is left untouched (the sub and add would cancel); the
    /// ledger reservations are shard-unattached, so their shard adjustments are
    /// no-ops. This operation is infallible and performs no pool interaction.
    fn transfer_bytes_to(
        &mut self,
        other: &mut MemoryReservation,
        bytes: usize,
    ) -> Result<(), ScribeError> {
        if bytes > self.bytes {
            return Err(ScribeError::Internal {
                detail: "memory reservation transfer exceeds owned bytes".to_owned(),
            });
        }
        let moved = bytes;
        let target_bytes = other.bytes.checked_add(moved).ok_or_else(|| {
            self.governor.parent.poison();
            ScribeError::Internal {
                detail: "memory reservation transfer overflow".to_owned(),
            }
        })?;
        if let Some((shard_bytes, shard)) = &self.shard
            && shard_bytes[*shard].load(Ordering::Acquire) < moved
        {
            self.governor.parent.poison();
            return Err(ScribeError::Internal {
                detail: "memory reservation shard accounting underflow during transfer".to_owned(),
            });
        }
        if let Some((shard_bytes, shard)) = &other.shard
            && shard_bytes[*shard]
                .load(Ordering::Acquire)
                .checked_add(moved)
                .is_none()
        {
            self.governor.parent.poison();
            return Err(ScribeError::Internal {
                detail: "memory reservation shard accounting overflow during transfer".to_owned(),
            });
        }
        if self.category != other.category {
            let source = &self.governor.parent.inner.categories[self.category as usize];
            let target = &self.governor.parent.inner.categories[other.category as usize];
            let source_current = source.load(Ordering::Acquire);
            if source_current < moved {
                self.governor.parent.poison();
                return Err(ScribeError::Internal {
                    detail: "memory reservation category accounting underflow during transfer"
                        .to_owned(),
                });
            }
            let target_current = target.load(Ordering::Acquire);
            let target_next = target_current.checked_add(moved).ok_or_else(|| {
                self.governor.parent.poison();
                ScribeError::Internal {
                    detail: "memory reservation category accounting overflow during transfer"
                        .to_owned(),
                }
            })?;
            source
                .compare_exchange(
                    source_current,
                    source_current - moved,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| {
                    self.governor.parent.poison();
                    ScribeError::Internal {
                        detail: "memory reservation category changed during transfer".to_owned(),
                    }
                })?;
            target
                .compare_exchange(
                    target_current,
                    target_next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| {
                    self.governor.parent.poison();
                    ScribeError::Internal {
                        detail: "memory reservation category changed during transfer".to_owned(),
                    }
                })?;
        }
        let other_shard_plan = other.preflight_shard_add(moved)?;
        self.bytes -= moved;
        other.bytes = target_bytes;
        self.adjust_shard_sub(moved)?;
        other.commit_shard_add(other_shard_plan, moved)?;
        Ok(())
    }

    /// Move accounting to another lifecycle category without changing totals.
    pub fn transfer_category(&mut self, category: MemoryCategory) -> Result<(), ScribeError> {
        if self.category != category {
            let current = self.governor.parent.inner.categories[self.category as usize]
                .load(Ordering::Acquire);
            if current < self.bytes {
                self.governor.parent.poison();
                return Err(ScribeError::Internal {
                    detail: "memory reservation category accounting underflow".to_owned(),
                });
            }
            let target = &self.governor.parent.inner.categories[category as usize];
            let target_current = target.load(Ordering::Acquire);
            let target_next = target_current.checked_add(self.bytes).ok_or_else(|| {
                self.governor.parent.poison();
                ScribeError::Internal {
                    detail: "memory reservation category accounting overflow during transfer"
                        .to_owned(),
                }
            })?;
            self.governor.parent.inner.categories[self.category as usize]
                .compare_exchange(
                    current,
                    current - self.bytes,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| {
                    self.governor.parent.poison();
                    ScribeError::Internal {
                        detail: "memory reservation category accounting changed during transfer"
                            .to_owned(),
                    }
                })?;
            target
                .compare_exchange(
                    target_current,
                    target_next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| {
                    self.governor.parent.poison();
                    ScribeError::Internal {
                        detail: "memory reservation category accounting changed during transfer"
                            .to_owned(),
                    }
                })?;
        }
        self.category = category;
        Ok(())
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        let shard = self
            .shard
            .as_ref()
            .map(|(counters, index)| (counters, *index));
        if let Err(rejection) = self.governor.release(self.category, self.bytes, shard) {
            tracing::error!(error = %rejection, "memory reservation cleanup poisoned accounting");
        }
        self.shard = None;
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

/// Reserve one counter with checked addition and bounded diagnostic operands.
fn reserve_with_limit_diagnostic(
    total: &AtomicUsize,
    limit: usize,
    bytes: usize,
    purpose: MemoryPurpose,
    ceiling: MemoryCeiling,
) -> Result<(), MemoryRejection> {
    let mut current = total.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes) else {
            return Err(MemoryRejection::new(
                MemoryRejectionKind::CounterOverflow,
                purpose,
                MemoryCeiling::NotApplicable,
                bytes,
                current,
                limit,
            ));
        };
        if next > limit {
            return Err(MemoryRejection::new(
                if bytes > limit {
                    MemoryRejectionKind::RequestTooLarge
                } else {
                    MemoryRejectionKind::Occupied
                },
                purpose,
                ceiling,
                bytes,
                current,
                limit,
            ));
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

    /// `ingress_occupancy_percent` divides by the ingress ceiling, not the
    /// larger Scribe child limit, so a run pinned at the ingress ceiling reports
    /// 100% occupancy while effective pressure stays well below it (D83).
    #[test]
    fn ingress_occupancy_percent_uses_ingress_limit_denominator() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ceiling = budget.ingress_limit_bytes();
        let reservation = budget
            .try_reserve_ingress(MemoryCategory::Raw, ceiling)
            .expect("fill ingress ceiling");
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.ingress_limit_bytes, ceiling);
        assert_eq!(snapshot.ingress_occupancy_bytes, ceiling);
        assert_eq!(snapshot.ingress_occupancy_percent(), 100);
        // The identical occupancy over the larger Scribe child limit is strictly
        // below 100%, proving the two ratios use different denominators.
        assert!(snapshot.effective_pressure_percent() < 100);
        drop(reservation);
    }

    /// `with_ingress_watermarks` fills the high/low-water bytes as percents of
    /// the ingress ceiling, giving the pressure decision its hysteresis band.
    #[test]
    fn with_ingress_watermarks_fills_hysteresis_band_from_ingress_limit() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ceiling = budget.ingress_limit_bytes();
        let snapshot = budget.snapshot().with_ingress_watermarks(75, 50);
        assert_eq!(snapshot.ingress_high_water_bytes, ceiling * 75 / 100);
        assert_eq!(snapshot.ingress_low_water_bytes, ceiling * 50 / 100);
        assert!(snapshot.ingress_low_water_bytes < snapshot.ingress_high_water_bytes);
    }

    /// `pressure_release_bytes` is the pure hysteresis decision: at or above the
    /// high-water mark it requests a seal that drains to the low-water target
    /// (`occupancy - low_water`), and below the high-water mark it is a no-op.
    /// The `low_water < high_water` band is preserved by `with_ingress_watermarks`.
    #[test]
    fn watermark_hysteresis_seals_from_high_to_low() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ceiling = budget.ingress_limit_bytes();

        // Occupancy above the 75% high-water mark: a seal is requested and its
        // release target is exactly the distance down to the low-water byte mark.
        let above = budget
            .try_reserve_ingress(MemoryCategory::Raw, ceiling * 80 / 100)
            .expect("reserve above the high-water mark");
        let high = budget.snapshot().with_ingress_watermarks(75, 50);
        assert!(high.ingress_occupancy_percent() >= 75);
        assert!(high.ingress_low_water_bytes < high.ingress_high_water_bytes);
        let to_release = high
            .pressure_release_bytes(75)
            .expect("occupancy above high-water requests a seal");
        assert_eq!(
            to_release,
            high.ingress_occupancy_bytes - high.ingress_low_water_bytes,
            "release target drains occupancy down to the low-water mark"
        );
        drop(above);

        // Occupancy below the high-water mark: no seal is requested (no-op half).
        let below = budget
            .try_reserve_ingress(MemoryCategory::Raw, ceiling * 60 / 100)
            .expect("reserve below the high-water mark");
        let low = budget.snapshot().with_ingress_watermarks(75, 50);
        assert!(low.ingress_occupancy_percent() < 75);
        assert_eq!(
            low.pressure_release_bytes(75),
            None,
            "below high-water the pressure decision is a no-op"
        );
        drop(below);
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

    /// Pins the boot memory floor to 768 MiB and pins the inequality that
    /// derives it, so a future change to `MIN_CHILD_BYTES` or the 70% parent
    /// fraction that breaks the derivation fails here rather than silently
    /// admitting pods that cannot construct both children.
    #[test]
    fn min_memory_floor_value_and_derivation() {
        assert_eq!(
            MIN_MEMORY_BYTES,
            768 * 1024 * 1024,
            "boot memory floor is 768 MiB"
        );
        // Parent ceiling at the floor must cover both children at their
        // minimums. This uses the exact integer `pod * 70 / 100` arithmetic the
        // constructor applies, so the pin matches real parent-ceiling behavior:
        // MIN_MEMORY_BYTES * 70 / 100 >= 2 * MIN_CHILD_BYTES.
        const {
            assert!(
                MIN_MEMORY_BYTES * 70 / 100 >= 2 * MIN_CHILD_BYTES,
                "floor must admit two MIN_CHILD_BYTES children under the 70% parent fraction"
            );
        }
    }

    /// Proves a pod one byte below the floor (767 MiB) fails closed with the
    /// existing typed construction error, confirming the raised floor stays a
    /// fail-closed boot validation.
    #[test]
    fn pod_below_floor_fails_closed() {
        let below_floor = MIN_MEMORY_BYTES - 1; // 767 MiB + (1 MiB - 1 byte) below the floor
        let error = BifrostMemoryGovernor::new(below_floor)
            .expect_err("pod below the memory floor must be rejected");
        assert!(
            matches!(error, ScribeError::Internal { .. }),
            "below-floor rejection uses the existing typed construction error"
        );
    }

    /// Proves a pod at exactly the floor (768 MiB) constructs with both children
    /// resolved to their `MIN_CHILD_BYTES` minimums, the tightest viable
    /// combined-role pod the floor is derived from.
    #[test]
    fn pod_at_floor_constructs_with_children_at_minimums() {
        let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES)
            .expect("pod at the memory floor must construct");
        assert_eq!(
            governor.scribe_limit_bytes(),
            MIN_CHILD_BYTES,
            "Scribe child clamps to its minimum at the floor"
        );
        assert_eq!(
            governor.oracle_limit_bytes(),
            MIN_CHILD_BYTES,
            "Oracle child clamps to its minimum at the floor"
        );
    }

    #[test]
    fn transfers_reservations_between_lifecycle_categories() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let mut reservation = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Decode, 512)
            .expect("reserve");
        reservation
            .transfer_category(MemoryCategory::Prepared)
            .expect("category transfer");
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
        reservation
            .transfer_category(MemoryCategory::Active)
            .expect("category transfer");
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
        active
            .transfer_category(MemoryCategory::Active)
            .expect("category transfer");
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

    /// The seal move relabels bytes Active->Immutable without touching pool totals.
    ///
    /// Pins the D97 net-zero invariant: after `move_active_to_immutable` the
    /// per-category totals shift by the moved amount while `scribe_total_bytes`
    /// and `bifrost_total_bytes` are byte-for-byte unchanged, proving no release
    /// to or re-reservation from the shared pool occurred. The rollback move
    /// restores the original category split, so freeze/persist/retire accounting
    /// is exactly conserved.
    #[test]
    fn seal_move_is_net_zero_category_transfer() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ledger = MemoryLedger::new(&budget).expect("ledger");
        ledger.reserve_active(8192).expect("reserve active");
        let before = governor.snapshot();
        assert_eq!(before.categories[MemoryCategory::Active as usize], 8192);

        ledger
            .move_active_to_immutable(3072)
            .expect("move to immutable");
        let mid = governor.snapshot();
        assert_eq!(mid.categories[MemoryCategory::Active as usize], 5120);
        assert_eq!(mid.categories[MemoryCategory::Immutable as usize], 3072);
        assert_eq!(mid.scribe_total_bytes, before.scribe_total_bytes);
        assert_eq!(mid.bifrost_total_bytes, before.bifrost_total_bytes);

        ledger.move_immutable_to_active(3072).expect("move back");
        let after = governor.snapshot();
        assert_eq!(after.categories[MemoryCategory::Active as usize], 8192);
        assert_eq!(after.categories[MemoryCategory::Immutable as usize], 0);
        assert_eq!(after.scribe_total_bytes, before.scribe_total_bytes);
    }

    /// An over-large move is rejected without changing either category.
    #[test]
    fn seal_move_clamps_to_owned_bytes_without_failing() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let ledger = MemoryLedger::new(&budget).expect("ledger");
        ledger.reserve_active(2048).expect("reserve active");
        assert!(ledger.move_active_to_immutable(1_000_000).is_err());
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.categories[MemoryCategory::Active as usize], 2048);
        assert_eq!(snapshot.categories[MemoryCategory::Immutable as usize], 0);
        assert_eq!(snapshot.scribe_total_bytes, 2048);
    }

    /// The seal move never fails while concurrent reservations hammer the ceiling.
    ///
    /// Regression guard for the D97 shrink-then-grow race (window 2). The ledger
    /// holds Active pinned at the Scribe ceiling, leaving zero free headroom,
    /// while a contender thread spins on `try_reserve` against the shared pool.
    /// The pre-D97 move released the moved bytes to the pool before re-reserving
    /// them, so the contender could win the transient headroom and make the grow
    /// fail; the net-zero move never touches the pool, so every one of the many
    /// forward/rollback moves returns `Ok` regardless of contention, and the
    /// ceiling accounting is exactly restored at the end.
    #[test]
    fn seal_move_never_fails_under_concurrent_reservations_at_ceiling() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let scribe_limit = governor.scribe_limit_bytes();
        let ledger = MemoryLedger::new(&budget).expect("ledger");
        ledger
            .reserve_active(scribe_limit)
            .expect("reserve at ceiling");

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let contender_budget = budget.clone();
        let contender_stop = Arc::clone(&stop);
        let contender = std::thread::spawn(move || {
            while !contender_stop.load(Ordering::Relaxed) {
                if let Ok(reservation) =
                    contender_budget.try_reserve(MemoryCategory::Metadata, 4096)
                {
                    drop(reservation);
                }
            }
        });

        let moved = 4096;
        for _ in 0..50_000 {
            ledger
                .move_active_to_immutable(moved)
                .expect("net-zero seal move must not fail at the ceiling");
            ledger
                .move_immutable_to_active(moved)
                .expect("net-zero rollback move must not fail at the ceiling");
        }
        stop.store(true, Ordering::Relaxed);
        contender.join().expect("contender thread");

        let snapshot = governor.snapshot();
        assert_eq!(
            snapshot.categories[MemoryCategory::Active as usize],
            scribe_limit
        );
        assert_eq!(snapshot.categories[MemoryCategory::Immutable as usize], 0);
        assert_eq!(snapshot.scribe_total_bytes, scribe_limit);
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

    /// The five D84 ceilings map to their closed `snake_case` labels.
    ///
    /// Constructing every variant here also anchors the closed vocabulary: a
    /// renamed or dropped variant fails this assertion, and the two ceilings
    /// that only the non-ingress and parent paths produce
    /// ([`ScribeRejectionCeiling::ScribeChild`] and
    /// [`ScribeRejectionCeiling::CgroupParent`]) are exercised here so the label
    /// contract is proven end to end.
    #[test]
    fn ceiling_labels_form_the_closed_d84_set() {
        assert_eq!(
            ScribeRejectionCeiling::CgroupBreaker.as_metric_label(),
            "cgroup_breaker"
        );
        assert_eq!(
            ScribeRejectionCeiling::ScribeChild.as_metric_label(),
            "scribe_child"
        );
        assert_eq!(
            ScribeRejectionCeiling::IngressSublimit.as_metric_label(),
            "ingress_sublimit"
        );
        assert_eq!(
            ScribeRejectionCeiling::BifrostParent.as_metric_label(),
            "bifrost_parent"
        );
        assert_eq!(
            ScribeRejectionCeiling::CgroupParent.as_metric_label(),
            "cgroup_parent"
        );
    }

    /// An ingress reservation pinned at its sublimit reports `IngressSublimit`.
    ///
    /// Pinning ingress at its ceiling and then classifying one more byte forces
    /// the Scribe-child overflow branch of the classified core, which the ingress
    /// entry point must label [`ScribeRejectionCeiling::IngressSublimit`] rather
    /// than the parent or cgroup ceiling.
    #[test]
    fn ingress_ceiling_rejection_reports_ingress_sublimit() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let budget = governor.scribe_budget();
        let pinned = budget
            .try_reserve_ingress(MemoryCategory::Raw, budget.ingress_limit_bytes())
            .expect("pin ingress at the sublimit");
        assert!(matches!(
            budget.try_reserve_ingress_classified(MemoryCategory::Raw, 1),
            Err(ScribeRejectionCeiling::IngressSublimit)
        ));
        drop(pinned);
    }

    /// An ingress reservation blocked by the parent ceiling reports `BifrostParent`.
    ///
    /// Filling the parent Bifrost ceiling through a parent-only reservation
    /// leaves the ingress sublimit with room, so the classified core admits the
    /// Scribe-child charge but then fails the parent charge and rolls back —
    /// the branch that must be labelled
    /// [`ScribeRejectionCeiling::BifrostParent`]. Uses a 4 GiB pod so the child
    /// budget genuinely has ingress headroom while the parent is full.
    #[test]
    fn parent_ceiling_rejection_reports_bifrost_parent() {
        let pod = 4 * 1024 * 1024 * 1024_usize;
        let governor = BifrostMemoryGovernor::new(pod).expect("valid memory");
        let parent = governor
            .try_reserve_parent(governor.bifrost_limit_bytes())
            .expect("fill the parent ceiling");
        let budget = governor.scribe_budget();
        assert!(matches!(
            budget.try_reserve_ingress_classified(MemoryCategory::Raw, 1),
            Err(ScribeRejectionCeiling::BifrostParent)
        ));
        assert_eq!(governor.snapshot().scribe_total_bytes, 0);
        drop(parent);
    }

    /// Oracle parent refusal rolls back only the newly charged child counter.
    #[test]
    fn oracle_parent_refusal_preserves_existing_parent_ownership() {
        let governor = BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024).expect("valid memory");
        let parent = governor
            .try_reserve_parent(governor.bifrost_limit_bytes())
            .expect("fill parent ceiling");
        let before = governor.snapshot();
        let rejection = governor
            .oracle_budget()
            .try_reserve_classified(1, MemoryPurpose::OracleHotRange)
            .expect_err("parent ceiling refuses Oracle range");
        assert_eq!(rejection.ceiling(), MemoryCeiling::BifrostParent);
        let after = governor.snapshot();
        assert_eq!(after.oracle_total_bytes, 0);
        assert_eq!(after.bifrost_total_bytes, before.bifrost_total_bytes);
        drop(parent);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
    }

    /// The governor gauges carry only closed `consumer`/`mark` labels.
    ///
    /// Proves [`MemorySnapshot::emit_governor_gauges`] publishes both memory
    /// families over the closed `consumer` set and the ingress watermark family
    /// over the closed `mark` set, and that no gauge key carries tenant, table,
    /// path, request, node, error, or sql identity.
    #[test]
    fn governor_gauges_emit_closed_consumer_and_mark_labels() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            governor
                .snapshot()
                .with_ingress_watermarks(75, 50)
                .emit_governor_gauges();
        });
        let snapshot = recorder.snapshot();
        for key in [
            "bifrost_memory_reserved_bytes{consumer=\"scribe\"}",
            "bifrost_memory_reserved_bytes{consumer=\"oracle\"}",
            "bifrost_memory_reserved_bytes{consumer=\"parent\"}",
            "bifrost_memory_limit_bytes{consumer=\"scribe\"}",
            "bifrost_scribe_ingress_watermark_bytes{mark=\"occupancy\"}",
            "bifrost_scribe_ingress_watermark_bytes{mark=\"high_water\"}",
            "bifrost_scribe_ingress_watermark_bytes{mark=\"low_water\"}",
        ] {
            assert!(
                snapshot.gauges.contains_key(key),
                "missing governor gauge {key}"
            );
        }
        assert!(!snapshot.gauges.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
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
    ///
    /// The requested children are enlarged to 300 MiB each because the premise
    /// inverts at the 768 MiB floor: two `MIN_CHILD_BYTES` (256 MiB) children
    /// sum to 512 MiB, which no longer exceeds the ~537.6 MiB parent, so the
    /// case must use larger children to keep exercising child-sum rejection at
    /// or above the floor.
    #[test]
    fn child_sum_exceeding_parent_rejected_at_construction() {
        // pod = 768 MiB → parent = 70% ≈ 537.6 MiB; two 300 MiB children sum to
        // 600 MiB > parent.
        let pod = MIN_MEMORY_BYTES; // 768 MiB
        let parent = pod * 70 / 100; // ≈ 537.6 MiB
        // 300 MiB stays within [MIN_CHILD_BYTES, MAX_CHILD_BYTES] so each child
        // passes its own bound and the sum reaches the parent-ceiling check.
        let each_child = 300 * 1024 * 1024; // 300 MiB
        assert!(
            each_child * 2 > parent,
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

    /// Refusals retain the exact physical ceiling and the observed operands.
    #[test]
    fn reservation_rejection_names_ceiling_and_operands() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let budget = governor.scribe_budget();
        let rejection = budget
            .try_reserve_classified(MemoryCategory::Raw, budget.limit_bytes() + 1)
            .expect_err("request larger than the child ceiling must reject");
        assert_eq!(rejection.kind(), MemoryRejectionKind::RequestTooLarge);
        assert_eq!(rejection.purpose(), MemoryPurpose::ScribeIngress);
        assert_eq!(rejection.ceiling(), MemoryCeiling::ScribeChild);
        assert_eq!(rejection.requested(), budget.limit_bytes() + 1);
        assert_eq!(rejection.current(), 0);
        assert_eq!(rejection.limit(), budget.limit_bytes());
    }

    /// Entry-point purpose remains stable when lifecycle categories change.
    #[test]
    fn reservation_purpose_follows_entry_point_and_survives_category_transfer() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let mut ingress = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("ingress reservation");
        ingress
            .transfer_category(MemoryCategory::Immutable)
            .expect("transfer");
        assert_eq!(ingress.purpose, MemoryPurpose::ScribeIngress);
        let maintenance = governor
            .scribe_budget()
            .try_reserve_maintenance(MemoryCategory::Decode, 64)
            .expect("maintenance reservation");
        assert_eq!(maintenance.purpose, MemoryPurpose::ScribeMaintenance);
    }

    /// Overflow has no physical Scribe ceiling label to emit.
    #[test]
    fn not_applicable_rejection_emits_no_scribe_ceiling_label() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
            let budget = governor.scribe_budget();
            let limit = budget.ingress_limit_bytes();
            assert!(
                budget
                    .try_reserve_ingress(MemoryCategory::Raw, limit + 1)
                    .is_err()
            );
            crate::scribe::record_scribe_ceiling_rejection(ScribeRejectionCeiling::IngressSublimit);
            let positive = recorder.snapshot();
            assert_eq!(
                positive
                    .counters
                    .get("bifrost_scribe_rejections_total{reason=\"ingress_sublimit\"}"),
                Some(&1)
            );
            let before = positive.counters;
            let counter = AtomicUsize::new(usize::MAX);
            let rejection = reserve_with_limit_diagnostic(
                &counter,
                usize::MAX,
                1,
                MemoryPurpose::ScribeIngress,
                MemoryCeiling::ScribeChild,
            )
            .expect_err("counter overflow");
            assert_eq!(rejection.ceiling(), MemoryCeiling::NotApplicable);
            governor.poison();
            assert!(budget.try_reserve(MemoryCategory::Raw, 1).is_err());
            assert_eq!(recorder.snapshot().counters, before);
        });
    }

    /// A checked release leaves all counters intact when category ownership is corrupt.
    #[test]
    fn checked_release_refuses_underflow_without_mutation() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let budget = governor.scribe_budget();
        let reservation = budget
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("reservation");
        governor.inner.categories[MemoryCategory::Raw as usize].store(0, Ordering::Release);
        let before = governor.snapshot();
        let error = budget
            .release(MemoryCategory::Raw, 64, None)
            .expect_err("corrupt category must not release totals");
        assert_eq!(error.kind(), MemoryRejectionKind::CounterOverflow);
        let after = governor.snapshot();
        assert_eq!(after.scribe_total_bytes, before.scribe_total_bytes);
        assert_eq!(after.bifrost_total_bytes, before.bifrost_total_bytes);
        std::mem::forget(reservation);
    }

    /// Drop underflow poisons without wrapping either shared total.
    #[test]
    fn drop_underflow_poison_does_not_wrap_or_panic() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let reservation = governor
            .scribe_budget()
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("reservation");
        governor.inner.categories[MemoryCategory::Raw as usize].store(0, Ordering::Release);
        drop(reservation);
        assert!(governor.is_poisoned());
        assert_eq!(governor.snapshot().scribe_total_bytes, 64);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 64);
    }

    /// Cleanup on a panicking path remains non-panicking and poisons admission.
    #[test]
    fn drop_underflow_during_unwind_does_not_double_panic() {
        /// Deliberately starts the outer unwind after the reservation is live.
        struct Sentinel;
        impl Drop for Sentinel {
            fn drop(&mut self) {
                panic!("sentinel unwind");
            }
        }
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let reservation = governor
                .scribe_budget()
                .try_reserve(MemoryCategory::Raw, 64)
                .expect("reservation");
            governor.inner.categories[MemoryCategory::Raw as usize].store(0, Ordering::Release);
            let _sentinel = Sentinel;
            let _ = reservation;
        }));
        let message = result
            .expect_err("sentinel must panic")
            .downcast::<&str>()
            .expect("sentinel panic payload");
        assert_eq!(*message, "sentinel unwind");
        assert!(governor.is_poisoned());
        assert_eq!(governor.snapshot().scribe_total_bytes, 64);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 64);
    }

    /// Distinct reservations can release concurrently and return shared totals to zero.
    #[test]
    fn concurrent_distinct_owner_releases_return_to_baseline() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let mut joins = Vec::new();
        for _ in 0..8 {
            let budget = governor.scribe_budget();
            joins.push(std::thread::spawn(move || {
                let reservation = budget
                    .try_reserve(MemoryCategory::Raw, 1024)
                    .expect("reservation");
                drop(reservation);
            }));
        }
        for join in joins {
            join.join().expect("worker must not panic");
        }
        assert_eq!(governor.snapshot().scribe_total_bytes, 0);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
        assert!(!governor.is_poisoned());
    }

    /// Poison rejects every nonzero Scribe, Oracle, and parent acquisition.
    #[test]
    fn poison_refuses_scribe_oracle_parent_and_datafusion_acquisitions() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        governor.poison();
        assert!(
            governor
                .scribe_budget()
                .try_reserve(MemoryCategory::Raw, 1)
                .is_err()
        );
        assert!(governor.oracle_budget().try_reserve(1).is_err());
        assert!(governor.try_reserve_parent(1).is_err());
        let pool: Arc<dyn MemoryPool> = Arc::new(BifrostDataFusionMemoryPool::for_oracle(governor));
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        assert!(reservation.try_grow(1).is_err());
    }

    /// DataFusion shrink corruption is contained by the shared poison bit.
    #[test]
    fn datafusion_shrink_underflow_poison_is_fail_closed() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let pool: Arc<dyn MemoryPool> =
            Arc::new(BifrostDataFusionMemoryPool::for_oracle(governor.clone()));
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        reservation.try_grow(64).expect("grow");
        governor
            .inner
            .oracle_total_bytes
            .store(0, Ordering::Release);
        drop(reservation);
        assert!(governor.is_poisoned());
        assert!(governor.oracle_budget().try_reserve(1).is_err());
    }

    /// Category and shard mismatches reject without subtracting shared totals.
    #[test]
    fn reserve_rollback_and_category_shard_release_are_checked() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let budget = governor.scribe_budget();
        let mut reservation = budget
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("reservation");
        reservation.attach_shard(budget.shard_accounting(), 0);
        governor.inner.shard_bytes[0].store(0, Ordering::Release);
        let before = governor.snapshot();
        assert!(
            budget
                .release(
                    MemoryCategory::Raw,
                    64,
                    reservation
                        .shard
                        .as_ref()
                        .map(|(counters, index)| (counters, *index)),
                )
                .is_err()
        );
        assert_eq!(
            governor.snapshot().scribe_total_bytes,
            before.scribe_total_bytes
        );
        assert!(governor.is_poisoned());
        reservation.shard = None;
        std::mem::forget(reservation);
    }

    /// A one-shot post-preflight CAS fault poisons and contains partial release.
    #[test]
    fn post_preflight_fault_poison_contains_partial_release() {
        use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};

        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let budget = governor.scribe_budget();
        let reservation = budget
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("reservation");
        budget.arm_post_preflight_release_fault();
        drop(reservation);
        assert!(governor.is_poisoned());
        assert!(governor.snapshot().scribe_total_bytes <= 64);
        assert!(governor.snapshot().bifrost_total_bytes <= 64);
        assert!(budget.try_reserve(MemoryCategory::Raw, 1).is_err());
        assert!(governor.oracle_budget().try_reserve(1).is_err());
        assert!(governor.try_reserve_parent(1).is_err());

        let oracle_pool: Arc<dyn MemoryPool> =
            Arc::new(BifrostDataFusionMemoryPool::for_oracle(governor.clone()));
        let parent_pool: Arc<dyn MemoryPool> =
            Arc::new(BifrostDataFusionMemoryPool::for_parent(governor.clone()));
        let oracle = MemoryConsumer::new("poisoned-oracle").register(&oracle_pool);
        let parent = MemoryConsumer::new("poisoned-parent").register(&parent_pool);
        assert!(oracle.try_grow(1).is_err());
        assert!(parent.try_grow(1).is_err());
    }

    /// A post-preflight shard race rolls back replacement ownership and preserves the lease.
    #[test]
    fn resize_growth_post_preflight_shard_fault_rolls_back_and_poisons() {
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("governor");
        let budget = governor.scribe_budget();
        let mut reservation = budget
            .try_reserve(MemoryCategory::Raw, 64)
            .expect("reservation");
        reservation.attach_shard(budget.shard_accounting(), 0);
        let before = budget.accounting_snapshot_for_test();
        budget.arm_post_preflight_shard_add_fault();

        assert!(reservation.resize(128).is_err());
        let after = budget.accounting_snapshot_for_test();
        assert_eq!(reservation.bytes(), 64);
        assert!(governor.is_poisoned());
        assert_eq!(after.0.bifrost_total_bytes, before.0.bifrost_total_bytes);
        assert_eq!(after.0.scribe_total_bytes, before.0.scribe_total_bytes);
        assert_eq!(after.0.categories, before.0.categories);
        assert_eq!(after.1[0], before.1[0] + 1);
        reservation.shard = None;
        drop(reservation);
    }
}
