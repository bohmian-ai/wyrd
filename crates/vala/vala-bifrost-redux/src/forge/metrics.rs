//! Public Forge production telemetry.
//!
//! Forge exposes exactly the seventeen Prometheus families operators need to
//! read demand, queue depth and age, active ownership, durable results,
//! latency, failure class, physical data flow, and compaction debt. Lease,
//! fence, catalog, reconciliation, cursor, scheduler, and resource protocol
//! detail belongs to structured traces and durable audit/task evidence, and
//! unresolved authority belongs to role readiness — none of it is duplicated
//! here.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use metrics::Gauge;
use num_traits::ToPrimitive;
use vala_sql::row_types::forge_tasks::{ForgeFailureClass, ForgeTaskStrategy};

/// Every `task_type` label Forge publishes, in durable strategy order.
///
/// The gauges are the only handles registered eagerly, so this is the exact
/// set of per-task-type series that always exports an explicit zero.
pub(super) const TASK_TYPES: [ForgeTaskStrategy; 5] = [
    ForgeTaskStrategy::ScribePromotion,
    ForgeTaskStrategy::SmallFiles,
    ForgeTaskStrategy::SnapshotExpiry,
    ForgeTaskStrategy::ExpiredCleanup,
    ForgeTaskStrategy::OrphanCleanup,
];

/// The exact public Forge family inventory, used by documentation coverage.
pub(super) const FORGE_METRIC_FAMILIES: [&str; 17] = [
    "bifrost_forge_planning_demands",
    "bifrost_forge_oldest_planning_demand_timestamp_seconds",
    "bifrost_forge_tasks_created_total",
    "bifrost_forge_pending_tasks",
    "bifrost_forge_oldest_pending_task_timestamp_seconds",
    "bifrost_forge_active_tasks",
    "bifrost_forge_task_attempts_total",
    "bifrost_forge_task_duration_seconds",
    "bifrost_forge_task_failures_total",
    "bifrost_forge_input_files_total",
    "bifrost_forge_input_bytes_total",
    "bifrost_forge_output_files_total",
    "bifrost_forge_output_bytes_total",
    "bifrost_forge_deleted_objects_total",
    "bifrost_forge_snapshots_expired_total",
    "bifrost_forge_compaction_debt_files",
    "bifrost_forge_compaction_debt_bytes",
];

/// Closed durable result of one completed Forge ownership episode.
///
/// This is the only vocabulary Forge invents for telemetry: `task_type` reuses
/// the durable [`ForgeTaskStrategy`] and `reason` reuses the durable
/// [`ForgeFailureClass`]. It exists because the durable task states do not
/// separate a capacity refusal from an ordinary retry, nor Prepared handoff
/// from an unfinished attempt, and operators need that distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeTaskResult {
    /// The attempt reached a durable success.
    Succeeded,
    /// The attempt failed and the task remains eligible for a later attempt.
    Retry,
    /// The attempt failed terminally.
    Failed,
    /// The attempt was superseded or cancelled before a durable effect.
    Cancelled,
    /// Local admission or the execution envelope refused the attempt.
    Refused,
    /// Externally accepted work retains Prepared evidence for recovery.
    Uncertain,
}

impl ForgeTaskResult {
    /// Returns the stable `result` label for this durable outcome.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Retry => "retry",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Refused => "refused",
            Self::Uncertain => "uncertain",
        }
    }
}

/// One authoritative pending-task observation for a single work type.
///
/// Produced by a complete durable scan and published together with its peers
/// so an absent work type still exports an explicit zero.
#[derive(Debug, Clone, Copy)]
pub(super) struct ForgePendingTasks {
    /// Work type this observation describes.
    pub(super) task_type: ForgeTaskStrategy,
    /// Count of rows whose state is exactly `ready` or `retryable`.
    pub(super) count: u64,
    /// Unix seconds of the oldest such row's `ready_at`, or zero when none.
    pub(super) oldest_ready_at_unix: i64,
}

/// Registered Forge metric handles retained by one Forge owner.
///
/// Only the gauges are registered here, because a gauge must export an
/// explicit zero before its first real observation. Counters and histograms
/// are obtained at their emission site so Forge never publishes a series for a
/// label combination no production event ever produced.
pub struct ForgeTelemetry {
    /// Unacknowledged planning demands after a complete fenced scan.
    planning_demands: Gauge,
    /// Unix seconds of the oldest unacknowledged planning demand.
    oldest_planning_demand: Gauge,
    /// `Ready` or `Retryable` durable tasks by work type.
    pending_tasks: BTreeMap<ForgeTaskStrategy, Gauge>,
    /// Unix seconds of the oldest pending task's `ready_at` by work type.
    oldest_pending_task: BTreeMap<ForgeTaskStrategy, Gauge>,
    /// Attempts this process currently holds unsettled, by work type.
    active_tasks: BTreeMap<ForgeTaskStrategy, Gauge>,
    /// Files the latest complete compaction inventory left outstanding.
    compaction_debt_files: Gauge,
    /// Bytes the latest complete compaction inventory left outstanding.
    compaction_debt_bytes: Gauge,
}

impl ForgeTelemetry {
    /// Registers the seven gauges that must export zero before first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            planning_demands: zeroed_gauge("bifrost_forge_planning_demands"),
            oldest_planning_demand: zeroed_gauge(
                "bifrost_forge_oldest_planning_demand_timestamp_seconds",
            ),
            pending_tasks: zeroed_task_type_gauges("bifrost_forge_pending_tasks"),
            oldest_pending_task: zeroed_task_type_gauges(
                "bifrost_forge_oldest_pending_task_timestamp_seconds",
            ),
            active_tasks: zeroed_task_type_gauges("bifrost_forge_active_tasks"),
            compaction_debt_files: zeroed_gauge("bifrost_forge_compaction_debt_files"),
            compaction_debt_bytes: zeroed_gauge("bifrost_forge_compaction_debt_bytes"),
        }
    }

    /// Counts one new durable task row after its transaction commits.
    pub(super) fn record_task_created(&self, task_type: ForgeTaskStrategy) {
        metrics::counter!(
            "bifrost_forge_tasks_created_total",
            "task_type" => task_type.as_str()
        )
        .increment(1);
    }

    /// Counts one completed ownership episode and its measured duration.
    ///
    /// Duration measures successful claim through durable result and excludes
    /// queue time, so it is recorded only once the durable state is known.
    pub(super) fn record_task_attempt(
        &self,
        task_type: ForgeTaskStrategy,
        result: ForgeTaskResult,
        elapsed: Duration,
    ) {
        metrics::counter!(
            "bifrost_forge_task_attempts_total",
            "task_type" => task_type.as_str(),
            "result" => result.as_str()
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_forge_task_duration_seconds",
            "task_type" => task_type.as_str(),
            "result" => result.as_str()
        )
        .record(elapsed.as_secs_f64());
    }

    /// Counts one durable retry, refusal, or terminal failure by its class.
    ///
    /// Recorded after the corresponding settlement transaction commits so the
    /// counter never claims a failure the durable record does not hold.
    pub(super) fn record_task_failure(
        &self,
        task_type: ForgeTaskStrategy,
        reason: ForgeFailureClass,
    ) {
        metrics::counter!(
            "bifrost_forge_task_failures_total",
            "task_type" => task_type.as_str(),
            "reason" => reason.as_str()
        )
        .increment(1);
    }

    /// Counts logical input files and bytes a committed effect consumed.
    pub(super) fn record_input(&self, task_type: ForgeTaskStrategy, files: u64, bytes: u64) {
        metrics::counter!(
            "bifrost_forge_input_files_total",
            "task_type" => task_type.as_str()
        )
        .increment(files);
        metrics::counter!(
            "bifrost_forge_input_bytes_total",
            "task_type" => task_type.as_str()
        )
        .increment(bytes);
    }

    /// Counts published output files and bytes a committed effect produced.
    pub(super) fn record_output(&self, task_type: ForgeTaskStrategy, files: u64, bytes: u64) {
        metrics::counter!(
            "bifrost_forge_output_files_total",
            "task_type" => task_type.as_str()
        )
        .increment(files);
        metrics::counter!(
            "bifrost_forge_output_bytes_total",
            "task_type" => task_type.as_str()
        )
        .increment(bytes);
    }

    /// Counts objects Forge freshly proved present and eligible, then deleted.
    ///
    /// Emitted at the delete boundary itself: pre-delete missing, refused,
    /// uncertain, and recovery-already-absent objects contribute nothing, so
    /// the storage API's idempotent success is never read as a deletion.
    pub(super) fn record_deleted_objects(&self, task_type: ForgeTaskStrategy, deleted: u64) {
        metrics::counter!(
            "bifrost_forge_deleted_objects_total",
            "task_type" => task_type.as_str()
        )
        .increment(deleted);
    }

    /// Counts individual snapshots a committed or recovered expiration removed.
    pub(super) fn record_snapshots_expired(&self, snapshots: u64) {
        metrics::counter!("bifrost_forge_snapshots_expired_total").increment(snapshots);
    }

    /// Publishes the authoritative planning demand count and oldest timestamp.
    ///
    /// The timestamp is the stored demand time in Unix seconds, never an age
    /// recomputed at publication, so `time() - timestamp` keeps growing if this
    /// producer later stalls.
    pub(super) fn publish_planning_status(&self, demands: u64, oldest_requested_at_unix: i64) {
        self.planning_demands.set(exact(demands));
        self.oldest_planning_demand
            .set(oldest_requested_at_unix.to_f64().unwrap_or(0.0));
    }

    /// Publishes pending counts and oldest `ready_at` for every work type.
    ///
    /// Work types absent from `observations` publish explicit zeroes, so an
    /// emptied queue is visible rather than a stale retained value.
    pub(super) fn publish_pending_tasks(&self, observations: &[ForgePendingTasks]) {
        for task_type in TASK_TYPES {
            let observed = observations
                .iter()
                .find(|observation| observation.task_type == task_type);
            self.pending_tasks[&task_type].set(observed.map_or(0.0, |o| exact(o.count)));
            self.oldest_pending_task[&task_type]
                .set(observed.map_or(0.0, |o| o.oldest_ready_at_unix.to_f64().unwrap_or(0.0)));
        }
    }

    /// Publishes the latest complete compaction-debt inventory.
    pub(super) fn record_compaction_debt(&self, files: u64, bytes: u64) {
        self.compaction_debt_files.set(exact(files));
        self.compaction_debt_bytes.set(exact(bytes));
    }

    /// Opens one balanced active-task guard for the duration of an attempt.
    ///
    /// The gauge is incremented here and decremented exactly once when the
    /// returned guard drops, so every task exit — success, refusal, failure,
    /// uncertainty handoff, cancellation, lease loss, takeover, shutdown, and
    /// panic unwind — balances the increment.
    pub(super) fn active_task(self: &Arc<Self>, task_type: ForgeTaskStrategy) -> ForgeActiveTask {
        self.active_tasks[&task_type].increment(1.0);
        ForgeActiveTask {
            telemetry: Arc::clone(self),
            task_type,
        }
    }
}

impl Default for ForgeTelemetry {
    /// Registers the standard Forge metric inventory.
    fn default() -> Self {
        Self::new()
    }
}

/// Balanced lifetime handle for one in-flight Forge task.
///
/// Held for exactly as long as this process owns an unsettled attempt. The
/// decrement lives in `Drop` rather than on each exit path so no new terminal
/// branch can leak the gauge.
pub(super) struct ForgeActiveTask {
    /// Registry the balanced decrement is applied to.
    telemetry: Arc<ForgeTelemetry>,
    /// Work type this guard incremented.
    task_type: ForgeTaskStrategy,
}

impl Drop for ForgeActiveTask {
    /// Returns this process's ownership of one attempt to the gauge.
    fn drop(&mut self) {
        self.telemetry.active_tasks[&self.task_type].decrement(1.0);
    }
}

/// Registers one unlabelled gauge and publishes its explicit zero.
fn zeroed_gauge(name: &'static str) -> Gauge {
    let gauge = metrics::gauge!(name);
    gauge.set(0.0);
    gauge
}

/// Registers one gauge per `task_type` and publishes each explicit zero.
fn zeroed_task_type_gauges(name: &'static str) -> BTreeMap<ForgeTaskStrategy, Gauge> {
    TASK_TYPES
        .into_iter()
        .map(|task_type| {
            let gauge = metrics::gauge!(name, "task_type" => task_type.as_str());
            gauge.set(0.0);
            (task_type, gauge)
        })
        .collect()
}

/// Converts an exact durable count to its gauge value without silent wrapping.
///
/// Counts beyond `f64`'s exact integer range saturate rather than wrap so a
/// pathological value reads as implausibly large instead of plausibly small.
fn exact(value: u64) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}
