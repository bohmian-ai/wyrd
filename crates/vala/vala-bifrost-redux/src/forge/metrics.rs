//! Public Forge production telemetry.
//!
//! Forge exposes exactly the Prometheus families operators need to read
//! active ownership, durable results, latency, failure class, physical data
//! flow, the elected leader's in-memory decision time, the leader term's
//! acquire/renew/revoke lifecycle, and the worker's readiness, back-offs, and
//! restarts. Table lease, fence, catalog, reconciliation, cursor, and resource
//! protocol detail belongs to structured traces and durable audit/task
//! evidence — none of it is duplicated here.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use metrics::Gauge;
use vala_sql::row_types::forge_tasks::{ForgeFailureClass, ForgeTaskStrategy};

use super::error::ForgeError;

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

/// Unlabelled gauges [`ForgeTelemetry::new`] publishes as an explicit zero.
const ZEROED_GAUGES: [&str; 3] = [
    "bifrost_forge_leader_held",
    "bifrost_forge_worker_ready",
    "bifrost_forge_worker_restart_backoff_seconds",
];

/// The exact public Forge family inventory, used by documentation coverage.
#[cfg(test)]
pub(super) const FORGE_METRIC_FAMILIES: [&str; 23] = [
    "bifrost_forge_tasks_created_total",
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
    "bifrost_forge_leader_decision_seconds",
    "bifrost_forge_leader_held",
    "bifrost_forge_leader_acquisitions_total",
    "bifrost_forge_leader_renewals_total",
    "bifrost_forge_leader_renewal_seconds",
    "bifrost_forge_leader_revocations_total",
    "bifrost_forge_worker_ready",
    "bifrost_forge_worker_backoffs_total",
    "bifrost_forge_worker_restarts_total",
    "bifrost_forge_worker_restart_backoff_seconds",
    "bifrost_forge_scheduler_restarts_total",
    "bifrost_forge_expired_cleanup_refusals_total",
];

/// One leader schedule operation, the closed `operation` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeLeaderDecision {
    /// A commit notice updated one table's track.
    Commit,
    /// A compactor pull selected and dispatched due tables.
    Pull,
    /// A compactor report settled one dispatched task.
    Report,
}

impl ForgeLeaderDecision {
    /// Returns the stable `operation` label for this decision.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Pull => "pull",
            Self::Report => "report",
        }
    }
}

/// Outcome of one leader term renewal, the closed `outcome` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeLeaderRenewal {
    /// The election row extended this replica's term.
    Ok,
    /// The election row no longer names this replica's term.
    Refused,
    /// The renewal statement failed.
    Failed,
    /// The renewal was still pending at the term's local deadline.
    Timeout,
}

impl ForgeLeaderRenewal {
    /// Returns the stable `outcome` label for this renewal.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
        }
    }
}

/// Why a held leader term ended, the closed revocation `reason` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeLeaderRevocation {
    /// A renewal found the term no longer held.
    RenewalRefused,
    /// A renewal statement failed.
    RenewalFailed,
    /// A renewal outlived the term's local deadline.
    RenewalTimeout,
    /// A newly acquired term replaced it.
    Replaced,
    /// The coordinator stopped and resigned it.
    Shutdown,
    /// The scheduler exited, failed or was dropped before shutdown; the
    /// election row is left to lapse.
    SchedulerStopped,
}

impl ForgeLeaderRevocation {
    /// Returns the stable `reason` label for this revocation.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::RenewalRefused => "renewal_refused",
            Self::RenewalFailed => "renewal_failed",
            Self::RenewalTimeout => "renewal_timeout",
            Self::Replaced => "replaced",
            Self::Shutdown => "shutdown",
            Self::SchedulerStopped => "scheduler_stopped",
        }
    }
}

/// The `reason` label of a worker back-off: the database could not answer.
pub(super) const WORKER_BACKOFF_DATABASE_UNAVAILABLE: &str = "database_unavailable";

/// The `reason` label of an expired-cleanup refusal: a table read is active.
pub(super) const CLEANUP_REFUSAL_ACTIVE_READ: &str = "active_read";

/// Where an expired-cleanup refusal was observed, the closed `stage` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeCleanupRefusalStage {
    /// The durable preparation of one candidate was refused.
    Prepare,
    /// A prepared-claim replay retained its candidate after a refused preparation.
    Replay,
}

impl ForgeCleanupRefusalStage {
    /// Returns the stable `stage` label for this refusal.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Replay => "replay",
        }
    }
}

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
    /// Local compaction admission refused the attempt.
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

/// Registered Forge metric handles retained by one Forge owner.
///
/// Only the gauges are registered here, because a gauge must export an
/// explicit zero before its first real observation. Counters and histograms
/// are obtained at their emission site so Forge never publishes a series for a
/// label combination no production event ever produced.
pub struct ForgeTelemetry {
    /// Attempts this process currently holds unsettled, by work type.
    active_tasks: BTreeMap<ForgeTaskStrategy, Gauge>,
}

impl ForgeTelemetry {
    /// Registers every gauge that must export zero before its first use.
    ///
    /// That is the per-task-type active gauges plus the leader, worker
    /// readiness, and restart backoff gauges, which read zero until this
    /// process holds a term, readies its worker, or restarts it.
    #[must_use]
    pub fn new() -> Self {
        for gauge in ZEROED_GAUGES {
            metrics::gauge!(gauge).set(0.0);
        }
        Self {
            active_tasks: zeroed_task_type_gauges("bifrost_forge_active_tasks"),
        }
    }

    /// Counts one new durable task row after its transaction commits.
    pub(super) fn record_task_created(task_type: ForgeTaskStrategy) {
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
    pub(super) fn record_task_failure(task_type: ForgeTaskStrategy, reason: ForgeFailureClass) {
        metrics::counter!(
            "bifrost_forge_task_failures_total",
            "task_type" => task_type.as_str(),
            "reason" => reason.as_str()
        )
        .increment(1);
    }

    /// Counts logical input files and bytes a committed effect consumed.
    pub(super) fn record_input(task_type: ForgeTaskStrategy, files: u64, bytes: u64) {
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
    pub(super) fn record_output(task_type: ForgeTaskStrategy, files: u64, bytes: u64) {
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
    pub(super) fn record_deleted_objects(task_type: ForgeTaskStrategy, deleted: u64) {
        metrics::counter!(
            "bifrost_forge_deleted_objects_total",
            "task_type" => task_type.as_str()
        )
        .increment(deleted);
    }

    /// Counts individual snapshots a committed or recovered expiration removed.
    pub(super) fn record_snapshots_expired(snapshots: u64) {
        metrics::counter!("bifrost_forge_snapshots_expired_total").increment(snapshots);
    }

    /// Records how long one leader schedule operation held the caller.
    ///
    /// The time includes waiting for the schedule lock, so contention between
    /// concurrent pullers shows here, and excludes RPC and worker time.
    pub(super) fn record_leader_decision(operation: ForgeLeaderDecision, elapsed: Duration) {
        metrics::histogram!(
            "bifrost_forge_leader_decision_seconds",
            "operation" => operation.as_str()
        )
        .record(elapsed.as_secs_f64());
    }

    /// Counts one leader term this replica acquired.
    pub(super) fn record_leader_acquired() {
        metrics::counter!("bifrost_forge_leader_acquisitions_total").increment(1);
    }

    /// Publishes whether this replica currently holds the leader term.
    pub(super) fn record_leader_held(held: bool) {
        metrics::gauge!("bifrost_forge_leader_held").set(if held { 1.0 } else { 0.0 });
    }

    /// Counts one leader renewal and records how long the election row took.
    pub(super) fn record_leader_renewal(outcome: ForgeLeaderRenewal, elapsed: Duration) {
        metrics::counter!(
            "bifrost_forge_leader_renewals_total",
            "outcome" => outcome.as_str()
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_forge_leader_renewal_seconds",
            "outcome" => outcome.as_str()
        )
        .record(elapsed.as_secs_f64());
    }

    /// Counts one held leader term ending, by why it ended.
    pub(super) fn record_leader_revocation(reason: ForgeLeaderRevocation) {
        metrics::counter!(
            "bifrost_forge_leader_revocations_total",
            "reason" => reason.as_str()
        )
        .increment(1);
    }

    /// Publishes whether this process's Forge worker currently advertises ready.
    pub(super) fn record_worker_ready(ready: bool) {
        metrics::gauge!("bifrost_forge_worker_ready").set(if ready { 1.0 } else { 0.0 });
    }

    /// Counts one worker turn that backed off because the database could not answer.
    pub(super) fn record_worker_backoff() {
        metrics::counter!(
            "bifrost_forge_worker_backoffs_total",
            "reason" => WORKER_BACKOFF_DATABASE_UNAVAILABLE
        )
        .increment(1);
    }

    /// Counts one failed Forge worker restart and publishes its backoff.
    ///
    /// Called by the process supervisor each time it schedules a rebuild of a
    /// worker that stopped before shutdown. The `reason` label is the failure
    /// class of the error the worker returned; an exit without a worker error
    /// (`None`) has nothing to classify and is counted as an internal
    /// invariant failure. The backoff gauge holds the wait before the most
    /// recent rebuild and stays at zero until the first one.
    pub fn record_worker_restart(error: Option<&ForgeError>, backoff: Duration) {
        metrics::counter!(
            "bifrost_forge_worker_restarts_total",
            "reason" => restart_reason(error).as_str()
        )
        .increment(1);
        metrics::gauge!("bifrost_forge_worker_restart_backoff_seconds").set(backoff.as_secs_f64());
    }

    /// Counts one failed Forge maintenance scheduler restart.
    ///
    /// Called by the process supervisor each time it schedules a rebuild of a
    /// scheduler that stopped before shutdown, with the same `reason`
    /// classification as [`Self::record_worker_restart`]. The backoff is the
    /// supervisor's and is logged with the restart, so no gauge repeats it.
    pub fn record_scheduler_restart(error: Option<&ForgeError>, _backoff: Duration) {
        metrics::counter!(
            "bifrost_forge_scheduler_restarts_total",
            "reason" => restart_reason(error).as_str()
        )
        .increment(1);
    }

    /// Counts one expired-cleanup candidate an active Oracle table read refused.
    ///
    /// `stage` says where the refusal was observed: the durable preparation of
    /// a candidate, or a prepared-claim replay that retained its candidate
    /// because that preparation was refused.
    pub(super) fn record_expired_cleanup_refusal(stage: ForgeCleanupRefusalStage) {
        metrics::counter!(
            "bifrost_forge_expired_cleanup_refusals_total",
            "stage" => stage.as_str(),
            "reason" => CLEANUP_REFUSAL_ACTIVE_READ
        )
        .increment(1);
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

/// Classifies the error that stopped a restarted Forge loop.
///
/// An exit without a worker error has nothing to classify and is an internal
/// invariant failure.
fn restart_reason(error: Option<&ForgeError>) -> ForgeFailureClass {
    error.map_or(
        ForgeFailureClass::InternalInvariant,
        ForgeError::failure_class,
    )
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Every `reason` value Forge may publish, in durable class order.
    const FAILURE_CLASSES: [ForgeFailureClass; 6] = [
        ForgeFailureClass::DataRefusal,
        ForgeFailureClass::TransientObjectStore,
        ForgeFailureClass::TransientCoordination,
        ForgeFailureClass::StorageHealth,
        ForgeFailureClass::CapacityRefused,
        ForgeFailureClass::InternalInvariant,
    ];

    /// Every `result` value Forge may publish, in outcome order.
    const TASK_RESULTS: [ForgeTaskResult; 6] = [
        ForgeTaskResult::Succeeded,
        ForgeTaskResult::Retry,
        ForgeTaskResult::Failed,
        ForgeTaskResult::Cancelled,
        ForgeTaskResult::Refused,
        ForgeTaskResult::Uncertain,
    ];

    /// Every `operation` value Forge may publish.
    const LEADER_DECISIONS: [ForgeLeaderDecision; 3] = [
        ForgeLeaderDecision::Commit,
        ForgeLeaderDecision::Pull,
        ForgeLeaderDecision::Report,
    ];

    /// Every renewal `outcome` value Forge may publish.
    const LEADER_RENEWALS: [ForgeLeaderRenewal; 4] = [
        ForgeLeaderRenewal::Ok,
        ForgeLeaderRenewal::Refused,
        ForgeLeaderRenewal::Failed,
        ForgeLeaderRenewal::Timeout,
    ];

    /// Every revocation `reason` value Forge may publish.
    const LEADER_REVOCATIONS: [ForgeLeaderRevocation; 5] = [
        ForgeLeaderRevocation::RenewalRefused,
        ForgeLeaderRevocation::RenewalFailed,
        ForgeLeaderRevocation::RenewalTimeout,
        ForgeLeaderRevocation::Replaced,
        ForgeLeaderRevocation::Shutdown,
    ];

    /// Split one recorded series key into its family and its label pairs.
    fn parse_series(series: &str) -> (String, Vec<(String, String)>) {
        let Some((family, tail)) = series.split_once('{') else {
            return (series.to_owned(), Vec::new());
        };
        let labels = tail
            .trim_end_matches('}')
            .split(',')
            .filter(|pair| !pair.is_empty())
            .filter_map(|pair| pair.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.trim_matches('"').to_owned()))
            .collect();
        (family.to_owned(), labels)
    }
    /// Asserts every observed series carries only approved labels and values.
    ///
    /// Label cardinality is the operational risk in this catalog, so the check
    /// is exhaustive in both directions: an unknown label key or an unlisted
    /// value fails, and so does any ownership identity that would make a series
    /// unbounded.
    ///
    /// # Panics
    ///
    /// Panics when a series carries an unapproved label key, an unapproved
    /// value, or an ownership identity.
    fn assert_label_vocabulary_is_closed(snapshot: &wyrd_bench::BenchmarkMetricSnapshot) {
        let task_types = TASK_TYPES
            .iter()
            .map(|task_type| task_type.as_str())
            .collect::<BTreeSet<_>>();
        let results = TASK_RESULTS
            .iter()
            .map(|result| result.as_str())
            .collect::<BTreeSet<_>>();
        let reasons = FAILURE_CLASSES
            .iter()
            .map(|reason| reason.as_str())
            .chain(LEADER_REVOCATIONS.iter().map(|reason| reason.as_str()))
            .chain([
                WORKER_BACKOFF_DATABASE_UNAVAILABLE,
                CLEANUP_REFUSAL_ACTIVE_READ,
            ])
            .collect::<BTreeSet<_>>();
        let stages = [
            ForgeCleanupRefusalStage::Prepare,
            ForgeCleanupRefusalStage::Replay,
        ]
        .iter()
        .map(|stage| stage.as_str())
        .collect::<BTreeSet<_>>();
        let operations = LEADER_DECISIONS
            .iter()
            .map(|operation| operation.as_str())
            .collect::<BTreeSet<_>>();
        let outcomes = LEADER_RENEWALS
            .iter()
            .map(|outcome| outcome.as_str())
            .collect::<BTreeSet<_>>();
        for series in snapshot
            .counters
            .keys()
            .chain(snapshot.gauges.keys())
            .chain(snapshot.histograms.keys())
        {
            for (key, value) in parse_series(series).1 {
                let allowed = match key.as_str() {
                    "task_type" => &task_types,
                    "result" => &results,
                    "reason" => &reasons,
                    "operation" => &operations,
                    "outcome" => &outcomes,
                    "stage" => &stages,
                    other => panic!("{series} carries the unapproved label key {other}"),
                };
                assert!(
                    allowed.contains(value.as_str()),
                    "{series} carries the unapproved {key} value {value}"
                );
            }
            for forbidden in [
                "tenant", "table", "task_id", "attempt", "snapshot", "path", "request",
            ] {
                assert!(
                    !series.contains(&format!("{forbidden}=")),
                    "{series} carries the ownership label {forbidden}"
                );
            }
        }
    }

    /// Forge publishes exactly its approved, bounded, balanced catalog.
    ///
    /// The whole public surface is exercised through the real handle against a
    /// local recorder, so this fails if a family is added, renamed, or dropped;
    /// if a label key or value leaves its closed vocabulary; if an ownership
    /// identity reaches a label; if construction pre-registers series no
    /// production event can produce; or if an active-task guard leaks its
    /// increment on an ordinary drop or a panic unwind.
    ///
    /// # Panics
    ///
    /// Panics when the recorded catalog, labels, gauge zeroes, or active-task
    /// balance differ from the approved contract.
    #[test]
    fn forge_telemetry_is_closed_bounded_and_balanced() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let registered = metrics::with_local_recorder(&recorder, || {
            let telemetry = Arc::new(ForgeTelemetry::new());
            let registered = recorder.snapshot();

            for task_type in TASK_TYPES {
                ForgeTelemetry::record_task_created(task_type);
                ForgeTelemetry::record_input(task_type, 2, 2048);
                ForgeTelemetry::record_output(task_type, 1, 1024);
                ForgeTelemetry::record_deleted_objects(task_type, 1);
            }
            for result in TASK_RESULTS {
                ForgeTelemetry::record_task_attempt(
                    ForgeTaskStrategy::SmallFiles,
                    result,
                    Duration::from_millis(5),
                );
            }
            for reason in FAILURE_CLASSES {
                ForgeTelemetry::record_task_failure(ForgeTaskStrategy::SmallFiles, reason);
            }
            ForgeTelemetry::record_snapshots_expired(3);
            for operation in LEADER_DECISIONS {
                ForgeTelemetry::record_leader_decision(operation, Duration::from_micros(2));
            }
            ForgeTelemetry::record_leader_acquired();
            ForgeTelemetry::record_leader_held(true);
            for outcome in LEADER_RENEWALS {
                ForgeTelemetry::record_leader_renewal(outcome, Duration::from_millis(3));
            }
            for reason in LEADER_REVOCATIONS {
                ForgeTelemetry::record_leader_revocation(reason);
            }
            ForgeTelemetry::record_leader_held(false);
            ForgeTelemetry::record_worker_ready(true);
            ForgeTelemetry::record_worker_backoff();
            ForgeTelemetry::record_worker_ready(false);
            ForgeTelemetry::record_worker_restart(None, Duration::from_secs(1));
            ForgeTelemetry::record_worker_restart(
                Some(&ForgeError::Capacity {
                    detail: "the worker refused its own capacity".to_owned(),
                }),
                Duration::from_secs(2),
            );
            ForgeTelemetry::record_scheduler_restart(None, Duration::from_secs(1));
            ForgeTelemetry::record_expired_cleanup_refusal(ForgeCleanupRefusalStage::Prepare);
            ForgeTelemetry::record_expired_cleanup_refusal(ForgeCleanupRefusalStage::Replay);

            drop(telemetry.active_task(ForgeTaskStrategy::SmallFiles));
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = telemetry.active_task(ForgeTaskStrategy::OrphanCleanup);
                panic!("the attempt unwinds while it owns the gauge");
            }));
            assert!(unwound.is_err(), "the unwinding attempt really panicked");
            registered
        });

        // Construction registers only the gauges, because a gauge must export
        // zero before its first observation. A counter or histogram series here
        // would be a label combination no production event ever produced.
        assert!(
            registered.counters.is_empty() && registered.histograms.is_empty(),
            "construction pre-registered counter or histogram series: {registered:?}"
        );
        assert_eq!(
            registered.gauges.len(),
            TASK_TYPES.len() + ZEROED_GAUGES.len(),
            "construction registers exactly the per-task-type active gauges and the zeroed gauges"
        );
        for (series, value) in &registered.gauges {
            assert!(
                value.abs() < f64::EPSILON,
                "{series} did not export an explicit zero before first use"
            );
        }

        let snapshot = recorder.snapshot();
        let observed = snapshot
            .counters
            .keys()
            .chain(snapshot.gauges.keys())
            .chain(snapshot.histograms.keys())
            .map(|series| parse_series(series).0)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            observed,
            FORGE_METRIC_FAMILIES
                .iter()
                .map(|family| (*family).to_owned())
                .collect::<BTreeSet<_>>(),
            "Forge published a family outside the approved catalog"
        );

        assert_label_vocabulary_is_closed(&snapshot);

        // The guard's decrement lives in Drop, so both the ordinary exit and
        // the unwind must have returned their increment.
        for (series, value) in &snapshot.gauges {
            if series.starts_with("bifrost_forge_active_tasks{") {
                assert!(
                    value.abs() < f64::EPSILON,
                    "{series} retained {value} active attempts"
                );
            }
        }
    }

    /// Operator documentation names exactly the families Forge publishes.
    ///
    /// A family added, renamed, or removed without its documented row would
    /// otherwise reach operators as an undocumented series or a documented one
    /// that never appears on a dashboard.
    ///
    /// # Panics
    ///
    /// Panics when the documented family-name set differs from the catalog.
    #[test]
    fn forge_operator_documentation_matches_telemetry() {
        const DOCUMENTATION: &str =
            include_str!("../../../../../docs/src/content/docs/bifrost/forge.svx");

        let documented = DOCUMENTATION
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|word| word.starts_with("bifrost_forge_"))
            .map(|word| {
                // A PromQL example names the rendered histogram series, which
                // Prometheus derives from the same family.
                word.strip_suffix("_bucket").unwrap_or(word).to_owned()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            documented,
            FORGE_METRIC_FAMILIES
                .iter()
                .map(|family| (*family).to_owned())
                .collect::<BTreeSet<_>>(),
            "operator documentation and the published catalog disagree"
        );
    }
}
