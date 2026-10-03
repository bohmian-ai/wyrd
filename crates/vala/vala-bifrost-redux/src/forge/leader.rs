//! The elected leader's in-memory Forge compaction schedule.
//!
//! This is a port of `RisingWave`'s Iceberg compaction track
//! (`meta/src/manager/iceberg_compaction/schedule.rs` at e23ddf95). Each
//! tenant-qualified table moves among `Idle`, `PendingDispatch` and `InFlight`.
//! Successful Iceberg promotions add pending commits; a pull selects the
//! oldest due Idle tables and captures each one's pending count and observed
//! snapshot; a matching report subtracts only that captured count. Nothing
//! here is durable: a new leader starts with an empty schedule and empty
//! maintenance sets, and every decision is made under one lock without SQL,
//! catalog or object IO.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;

use super::settings::{ForgeCompactionType, ForgeTableSettings};

/// Default time a dispatched task may run before it becomes eligible again.
pub const DEFAULT_REPORT_TIMEOUT: Duration = Duration::from_mins(30);

/// Tenant-qualified physical table identity the schedule is keyed by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForgeTableKey {
    /// Tenant that owns the table.
    pub tenant: DataTenantId,
    /// Catalog, namespace and table name.
    pub table: ForgeTaskTableIdentity,
}

/// One successful Iceberg commit reported to the leader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCommitNotice {
    /// Table the commit landed on.
    pub key: ForgeTableKey,
    /// Snapshot the commit produced.
    pub snapshot_id: i64,
    /// Settings read from the committed table by the notifier.
    pub settings: ForgeTableSettings,
}

/// One table-level compaction task handed to a pulling worker.
///
/// It names the table, branch, type and settings only. The worker loads the
/// table itself and decides which files to rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCompactionDispatch {
    /// Identity a report must echo to change scheduling state.
    pub task_id: Uuid,
    /// Table to compact.
    pub key: ForgeTableKey,
    /// Iceberg branch to compact.
    pub branch: String,
    /// Physical selection the worker applies.
    pub compaction_type: ForgeCompactionType,
}

/// What a compactor reports for one dispatched task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeCompactionOutcome {
    /// The task finished, including a plan that found nothing to rewrite.
    Succeeded,
    /// The task failed; the table is due again immediately.
    Failed,
    /// The task was never started, so its delivery failed.
    NotStarted,
}

/// Lifecycle of one table's compaction track.
#[derive(Debug, Clone)]
enum TrackState {
    /// Accepting commits and checking the due rule.
    Idle {
        /// Earliest time an interval-based dispatch may happen.
        next_compaction_at: DateTime<Utc>,
    },
    /// Selected by a pull but not yet handed to the worker.
    PendingDispatch {
        /// Pending commits captured at selection.
        pending_at_dispatch: usize,
        /// Latest observed snapshot captured at selection.
        watermark: Option<i64>,
    },
    /// Handed to a worker; the deadline acts as the task's lease.
    InFlight {
        /// Task identity a report must match.
        task_id: Uuid,
        /// Pending commits a successful report subtracts.
        pending_at_dispatch: usize,
        /// Latest observed snapshot captured at selection.
        watermark: Option<i64>,
        /// Time after which the task becomes eligible again.
        report_deadline: DateTime<Utc>,
    },
}

/// One table's in-memory compaction schedule.
#[derive(Debug, Clone)]
struct CompactionTrack {
    /// Physical type dispatched for this table.
    compaction_type: ForgeCompactionType,
    /// Longest wait while commits are pending.
    interval: Duration,
    /// Pending commit count that triggers early.
    trigger_snapshot_count: usize,
    /// Successful Iceberg commits not yet consumed by a compaction.
    pending_commits: usize,
    /// Latest snapshot a commit notice reported.
    latest_snapshot: Option<i64>,
    /// Drop the track after the current task finishes (manual run while disabled).
    remove_after_finish: bool,
    /// Current lifecycle state.
    state: TrackState,
}

/// Adds a standard duration to a wall time, saturating at the far future.
fn after(now: DateTime<Utc>, duration: Duration) -> DateTime<Utc> {
    chrono::Duration::from_std(duration)
        .ok()
        .and_then(|duration| now.checked_add_signed(duration))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

impl CompactionTrack {
    /// Creates an Idle track whose first interval starts now.
    fn new(settings: &ForgeTableSettings, now: DateTime<Utc>) -> Self {
        Self {
            compaction_type: settings.compaction_type,
            interval: settings.compaction_interval,
            trigger_snapshot_count: settings.trigger_snapshot_count,
            pending_commits: 0,
            latest_snapshot: None,
            remove_after_finish: false,
            state: TrackState::Idle {
                next_compaction_at: after(now, settings.compaction_interval),
            },
        }
    }

    /// `RisingWave`'s due rule: count threshold, or elapsed interval with a commit.
    fn should_trigger(&self, now: DateTime<Utc>) -> bool {
        let TrackState::Idle { next_compaction_at } = self.state else {
            return false;
        };
        self.pending_commits >= self.trigger_snapshot_count
            || (now >= next_compaction_at && self.pending_commits > 0)
    }

    /// Applies refreshed settings; a changed interval restarts an Idle wait.
    fn refresh(&mut self, settings: &ForgeTableSettings, now: DateTime<Utc>) {
        self.compaction_type = settings.compaction_type;
        self.trigger_snapshot_count = settings.trigger_snapshot_count;
        if self.interval != settings.compaction_interval {
            self.interval = settings.compaction_interval;
            if let TrackState::Idle { next_compaction_at } = &mut self.state {
                *next_compaction_at = after(now, self.interval);
            }
        }
    }

    /// Makes an Idle track due now with at least one pending commit.
    fn force(&mut self, now: DateTime<Utc>) {
        if let TrackState::Idle { next_compaction_at } = &mut self.state {
            *next_compaction_at = now;
            self.pending_commits = self.pending_commits.max(1);
        }
    }

    /// Whether a task is selected or running for this table.
    const fn is_processing(&self) -> bool {
        !matches!(self.state, TrackState::Idle { .. })
    }

    /// Captures the pending count and watermark at selection.
    fn start_processing(&mut self) {
        self.state = TrackState::PendingDispatch {
            pending_at_dispatch: self.pending_commits,
            watermark: self.latest_snapshot,
        };
    }

    /// Moves a selected task in flight under its report deadline.
    fn mark_dispatched(&mut self, task_id: Uuid, now: DateTime<Utc>, timeout: Duration) {
        if let TrackState::PendingDispatch {
            pending_at_dispatch,
            watermark,
        } = self.state
        {
            self.state = TrackState::InFlight {
                task_id,
                pending_at_dispatch,
                watermark,
                report_deadline: after(now, timeout),
            };
        }
    }

    /// Returns a selected or undelivered task to Idle, due now, without
    /// consuming commits.
    fn revert_pre_dispatch(&mut self, now: DateTime<Utc>) {
        if self.is_processing() {
            self.state = TrackState::Idle {
                next_compaction_at: now,
            };
        }
    }

    /// Consumes only the captured commits and starts the next interval.
    fn finish_success(&mut self, now: DateTime<Utc>) {
        if let TrackState::InFlight {
            pending_at_dispatch,
            ..
        } = self.state
        {
            self.pending_commits = self.pending_commits.saturating_sub(pending_at_dispatch);
            self.state = TrackState::Idle {
                next_compaction_at: after(now, self.interval),
            };
        }
    }

    /// Returns a failed or timed-out task to Idle, due immediately.
    fn finish_failed(&mut self, now: DateTime<Utc>) {
        if matches!(self.state, TrackState::InFlight { .. }) {
            self.state = TrackState::Idle {
                next_compaction_at: now,
            };
        }
    }

    /// Whether `task_id` is this track's current in-flight task.
    fn is_task(&self, task_id: Uuid) -> bool {
        matches!(self.state, TrackState::InFlight { task_id: current, .. } if current == task_id)
    }

    /// Whether the in-flight task's deadline has passed.
    fn is_timed_out(&self, now: DateTime<Utc>) -> bool {
        matches!(self.state, TrackState::InFlight { report_deadline, .. } if now >= report_deadline)
    }
}

/// Lock-protected schedule and maintenance membership.
#[derive(Debug, Default)]
struct ScheduleInner {
    /// Compaction tracks by table.
    tracks: BTreeMap<ForgeTableKey, CompactionTrack>,
    /// Tables the leader timer expires.
    snapshot_expiration: BTreeSet<ForgeTableKey>,
    /// Tables the leader timer rewrites manifests for.
    manifest_rewrite: BTreeSet<ForgeTableKey>,
}

/// Test-visible projection of one track.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTrackView {
    /// Pending Iceberg commits.
    pub pending_commits: usize,
    /// Latest observed snapshot.
    pub latest_snapshot: Option<i64>,
    /// In-flight task, if any.
    pub in_flight: Option<Uuid>,
}

/// The elected leader's complete volatile Forge schedule.
#[derive(Debug)]
pub struct ForgeSchedule {
    /// Time a dispatched task may run before it is eligible again.
    report_timeout: Duration,
    /// Every track and maintenance set, behind one lock as in `RisingWave`.
    inner: Mutex<ScheduleInner>,
}

impl ForgeSchedule {
    /// Creates an empty schedule, the state of every newly elected leader.
    #[must_use]
    pub fn new(report_timeout: Duration) -> Self {
        Self {
            report_timeout,
            inner: Mutex::new(ScheduleInner::default()),
        }
    }

    /// Locks the schedule; a panic while held leaves state a successor discards.
    fn lock(&self) -> MutexGuard<'_, ScheduleInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records one successful Iceberg commit and the table's current settings.
    ///
    /// Maintenance membership follows the settings even when compaction is
    /// disabled. A disabled table loses an Idle track, but a running task's
    /// track survives until its report.
    pub fn notify_commit(&self, notice: ForgeCommitNotice, now: DateTime<Utc>) {
        let ForgeCommitNotice {
            key,
            snapshot_id,
            settings,
        } = notice;
        let mut inner = self.lock();
        Self::apply_membership(&mut inner, &key, &settings);
        if !settings.compaction_enabled {
            if inner
                .tracks
                .get(&key)
                .is_some_and(|track| !track.is_processing() && !track.remove_after_finish)
            {
                inner.tracks.remove(&key);
            }
            return;
        }
        tracing::debug!(table = %key.table.table, snapshot_id, "Forge commit notice recorded");
        let track = inner
            .tracks
            .entry(key)
            .or_insert_with(|| CompactionTrack::new(&settings, now));
        track.remove_after_finish = false;
        track.refresh(&settings, now);
        track.pending_commits = track.pending_commits.saturating_add(1);
        track.latest_snapshot = Some(snapshot_id);
    }

    /// Forces one table due now, even when its automatic compaction is disabled.
    ///
    /// A manual run on a disabled table creates a temporary track that is
    /// removed after its task finishes.
    pub fn request_compaction(
        &self,
        key: ForgeTableKey,
        settings: &ForgeTableSettings,
        now: DateTime<Utc>,
    ) {
        let mut inner = self.lock();
        Self::apply_membership(&mut inner, &key, settings);
        let track = inner.tracks.entry(key).or_insert_with(|| {
            let mut track = CompactionTrack::new(settings, now);
            track.remove_after_finish = !settings.compaction_enabled;
            track
        });
        track.refresh(settings, now);
        track.force(now);
    }

    /// Updates both maintenance sets from one table's settings.
    fn apply_membership(
        inner: &mut ScheduleInner,
        key: &ForgeTableKey,
        settings: &ForgeTableSettings,
    ) {
        for (set, enabled) in [
            (
                &mut inner.snapshot_expiration,
                settings.snapshot_expiration_enabled,
            ),
            (
                &mut inner.manifest_rewrite,
                settings.manifest_rewrite_enabled,
            ),
        ] {
            if enabled {
                set.insert(key.clone());
            } else {
                set.remove(key);
            }
        }
    }

    /// Re-applies one table's current settings to both maintenance sets.
    ///
    /// The timer calls this when a member's settings no longer enable the
    /// maintenance it was due, so the table leaves that set until a later
    /// commit re-enables it.
    pub fn refresh_membership(&self, key: &ForgeTableKey, settings: &ForgeTableSettings) {
        Self::apply_membership(&mut self.lock(), key, settings);
    }

    /// Selects up to `limit` oldest due tables and dispatches them.
    ///
    /// Timed-out tasks first return to Idle, due immediately, exactly as
    /// `RisingWave` reconsiders them on the next pull. Every `Idle` track is then
    /// scanned and the due ones sorted by their next compaction time.
    #[must_use]
    pub fn pull(&self, limit: usize, now: DateTime<Utc>) -> Vec<ForgeCompactionDispatch> {
        let mut inner = self.lock();
        let mut removed = Vec::new();
        for (key, track) in &mut inner.tracks {
            if track.is_timed_out(now) {
                tracing::warn!(tenant = %key.tenant, table = %key.table.table, "Forge compaction report timed out");
                track.finish_failed(now);
                if track.remove_after_finish {
                    removed.push(key.clone());
                }
            }
        }
        for key in removed {
            inner.tracks.remove(&key);
        }
        let mut due: Vec<(DateTime<Utc>, ForgeTableKey)> = inner
            .tracks
            .iter()
            .filter(|(_, track)| track.should_trigger(now))
            .filter_map(|(key, track)| match track.state {
                TrackState::Idle { next_compaction_at } => Some((next_compaction_at, key.clone())),
                _ => None,
            })
            .collect();
        due.sort_by_key(|entry| entry.0);
        due.into_iter()
            .take(limit)
            .filter_map(|(_, key)| {
                let track = inner.tracks.get_mut(&key)?;
                track.start_processing();
                let task_id = Uuid::now_v7();
                let compaction_type = track.compaction_type;
                track.mark_dispatched(task_id, now, self.report_timeout);
                tracing::debug!(table = %key.table.table, %task_id, pending_commits = track.pending_commits, "Forge compaction dispatched");
                Some(ForgeCompactionDispatch {
                    task_id,
                    key,
                    branch: super::scribe_promotion::PROMOTION_BRANCH.to_owned(),
                    compaction_type,
                })
            })
            .collect()
    }

    /// Applies one worker report; a report for any other task is ignored.
    ///
    /// [`ForgeCompactionOutcome::NotStarted`] is `RisingWave`'s failed send: the
    /// task returns to Idle, due now, with every commit intact.
    ///
    /// Returns whether the report matched the table's current task.
    pub fn report(
        &self,
        key: &ForgeTableKey,
        task_id: Uuid,
        outcome: ForgeCompactionOutcome,
        now: DateTime<Utc>,
    ) -> bool {
        let mut inner = self.lock();
        let Some(track) = inner.tracks.get_mut(key) else {
            tracing::warn!(%task_id, "Forge compaction report for an unknown table ignored");
            return false;
        };
        if !track.is_task(task_id) {
            tracing::warn!(%task_id, "stale Forge compaction report ignored");
            return false;
        }
        tracing::debug!(table = %key.table.table, %task_id, ?outcome, "Forge compaction report applied");
        match outcome {
            ForgeCompactionOutcome::Succeeded => track.finish_success(now),
            ForgeCompactionOutcome::Failed => track.finish_failed(now),
            ForgeCompactionOutcome::NotStarted => {
                track.revert_pre_dispatch(now);
                return true;
            }
        }
        if track.remove_after_finish {
            inner.tracks.remove(key);
        }
        true
    }

    /// Returns the timer's maintenance work: manifest-rewrite and expiry tables.
    #[must_use]
    pub fn maintenance_tables(&self) -> (Vec<ForgeTableKey>, Vec<ForgeTableKey>) {
        let inner = self.lock();
        (
            inner.manifest_rewrite.iter().cloned().collect(),
            inner.snapshot_expiration.iter().cloned().collect(),
        )
    }

    /// Whether this table has commits awaiting compaction or a task in flight.
    ///
    /// Lower-priority maintenance that would occupy the table's single active
    /// task slot yields while this holds, so it cannot displace the rewrite.
    #[must_use]
    pub fn owes_compaction(&self, key: &ForgeTableKey) -> bool {
        self.lock()
            .tracks
            .get(key)
            .is_some_and(|track| track.pending_commits > 0 || track.is_processing())
    }

    /// Returns the snapshot captured by a selected or running task, if any.
    ///
    /// The outer `None` means no task is processing; `Some(None)` means one is
    /// processing without an observed snapshot, which blocks expiry.
    #[must_use]
    pub fn processing_watermark(&self, key: &ForgeTableKey) -> Option<Option<i64>> {
        let inner = self.lock();
        match inner.tracks.get(key)?.state {
            TrackState::Idle { .. } => None,
            TrackState::PendingDispatch { watermark, .. }
            | TrackState::InFlight { watermark, .. } => Some(watermark),
        }
    }

    /// Projects one table's track for assertions.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn track_for_test(&self, key: &ForgeTableKey) -> Option<ForgeTrackView> {
        self.lock().tracks.get(key).map(|track| ForgeTrackView {
            pending_commits: track.pending_commits,
            latest_snapshot: track.latest_snapshot,
            in_flight: match track.state {
                TrackState::InFlight { task_id, .. } => Some(task_id),
                _ => None,
            },
        })
    }

    /// Returns how many tracks and maintenance memberships exist.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn sizes_for_test(&self) -> (usize, usize, usize) {
        let inner = self.lock();
        (
            inner.tracks.len(),
            inner.snapshot_expiration.len(),
            inner.manifest_rewrite.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a valid key for one numbered table.
    fn key(table: &str) -> ForgeTableKey {
        ForgeTableKey {
            tenant: DataTenantId::new(Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0007))
                .expect("UUIDv7 test tenant"),
            table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", table)
                .expect("identity"),
        }
    }

    /// Enabled settings with an explicit interval and count threshold.
    fn enabled(interval_secs: u64, count: usize) -> ForgeTableSettings {
        ForgeTableSettings {
            compaction_enabled: true,
            compaction_interval: Duration::from_secs(interval_secs),
            trigger_snapshot_count: count,
            ..ForgeTableSettings::default()
        }
    }

    /// Records one commit for a table.
    fn commit(
        schedule: &ForgeSchedule,
        table: &str,
        snapshot: i64,
        settings: &ForgeTableSettings,
        now: DateTime<Utc>,
    ) {
        schedule.notify_commit(
            ForgeCommitNotice {
                key: key(table),
                snapshot_id: snapshot,
                settings: settings.clone(),
            },
            now,
        );
    }

    /// The `RisingWave` due rule's exact interval, count and zero-commit boundaries.
    ///
    /// # Panics
    /// Panics when a boundary dispatches early, late or without a commit.
    #[test]
    fn due_rule_boundaries_match_risingwave() {
        let start = DateTime::<Utc>::UNIX_EPOCH;
        let schedule = ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT);
        let hourly = enabled(3600, usize::MAX);
        commit(&schedule, "interval", 1, &hourly, start);
        assert!(
            schedule
                .pull(4, start + chrono::Duration::seconds(3599))
                .is_empty(),
            "one commit before the interval waits"
        );
        let due = schedule.pull(4, start + chrono::Duration::seconds(3600));
        assert_eq!(due.len(), 1, "the interval boundary dispatches");
        assert!(schedule.report(
            &key("interval"),
            due[0].task_id,
            ForgeCompactionOutcome::Succeeded,
            start + chrono::Duration::seconds(3600)
        ));
        assert!(
            schedule
                .pull(4, start + chrono::Duration::days(30))
                .is_empty(),
            "zero pending commits past the interval stays idle"
        );

        commit(&schedule, "count", 1, &enabled(3600, 2), start);
        assert!(schedule.pull(4, start).is_empty());
        commit(&schedule, "count", 2, &enabled(3600, 2), start);
        assert_eq!(
            schedule.pull(4, start).len(),
            1,
            "the configured count dispatches early"
        );

        schedule.request_compaction(key("manual"), &ForgeTableSettings::default(), start);
        let manual = schedule.pull(4, start);
        assert_eq!(
            manual.len(),
            1,
            "a manual request forces dispatch while disabled"
        );
        assert!(schedule.report(
            &key("manual"),
            manual[0].task_id,
            ForgeCompactionOutcome::Succeeded,
            start
        ));
        assert!(
            schedule.track_for_test(&key("manual")).is_none(),
            "a disabled manual track is removed"
        );
    }

    /// Reports subtract only captured commits; stale reports and timeouts behave as `RisingWave`.
    ///
    /// # Panics
    /// Panics when a later commit is lost, a stale report applies or a timeout is not retried.
    #[test]
    fn reports_preserve_later_commits_and_ignore_stale_tasks() {
        let start = DateTime::<Utc>::UNIX_EPOCH;
        let schedule = ForgeSchedule::new(Duration::from_mins(1));
        let settings = enabled(10, 1);
        commit(&schedule, "t", 1, &settings, start);
        let first = schedule.pull(4, start).remove(0);
        assert_eq!(schedule.processing_watermark(&key("t")), Some(Some(1)));
        commit(&schedule, "t", 2, &settings, start);
        assert!(
            schedule.pull(4, start).is_empty(),
            "one current task per table"
        );
        assert!(
            !schedule.report(
                &key("t"),
                Uuid::now_v7(),
                ForgeCompactionOutcome::Succeeded,
                start
            ),
            "a stale task id is ignored"
        );
        assert!(schedule.report(
            &key("t"),
            first.task_id,
            ForgeCompactionOutcome::Succeeded,
            start
        ));
        assert_eq!(
            schedule
                .track_for_test(&key("t"))
                .expect("track")
                .pending_commits,
            1,
            "the later commit survives"
        );

        let second = schedule.pull(4, start).remove(0);
        assert!(schedule.report(
            &key("t"),
            second.task_id,
            ForgeCompactionOutcome::Failed,
            start
        ));
        let third = schedule.pull(4, start).remove(0);
        assert!(
            schedule
                .pull(4, start + chrono::Duration::seconds(59))
                .is_empty()
        );
        let retried = schedule.pull(4, start + chrono::Duration::seconds(60));
        assert_eq!(
            retried.len(),
            1,
            "a timed-out task is reconsidered on the next pull"
        );
        assert!(
            !schedule.report(
                &key("t"),
                third.task_id,
                ForgeCompactionOutcome::Succeeded,
                start
            ),
            "the timed-out task's report is stale"
        );

        assert!(schedule.report(
            &key("t"),
            retried[0].task_id,
            ForgeCompactionOutcome::NotStarted,
            start
        ));
        assert_eq!(
            schedule
                .track_for_test(&key("t"))
                .expect("track")
                .pending_commits,
            1,
            "a failed hand-over keeps its commits"
        );
        assert_eq!(
            schedule.pull(4, start).len(),
            1,
            "a failed hand-over is due immediately"
        );
    }

    /// Pulls return the oldest due tables first and honour the limit.
    ///
    /// # Panics
    /// Panics when selection order or the limit is wrong.
    #[test]
    fn pull_selects_oldest_due_tables_up_to_limit() {
        let start = DateTime::<Utc>::UNIX_EPOCH;
        let schedule = ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT);
        for (offset, table) in ["c", "a", "b"].into_iter().enumerate() {
            commit(
                &schedule,
                table,
                1,
                &enabled(10, usize::MAX),
                start + chrono::Duration::seconds(i64::try_from(offset).expect("small")),
            );
        }
        let selected = schedule.pull(2, start + chrono::Duration::seconds(100));
        let names: Vec<_> = selected
            .iter()
            .map(|dispatch| dispatch.key.table.table.as_str())
            .collect();
        assert_eq!(names, ["c", "a"]);
    }

    /// Disabling compaction keeps enabled maintenance membership.
    ///
    /// # Panics
    /// Panics when a disabled table keeps an idle track or loses maintenance.
    #[test]
    fn disabled_compaction_keeps_maintenance_membership() {
        let start = DateTime::<Utc>::UNIX_EPOCH;
        let schedule = ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT);
        commit(&schedule, "t", 1, &enabled(10, usize::MAX), start);
        let disabled = ForgeTableSettings {
            manifest_rewrite_enabled: true,
            ..ForgeTableSettings::default()
        };
        commit(&schedule, "t", 2, &disabled, start);
        assert_eq!(schedule.sizes_for_test(), (0, 1, 1));
        assert!(
            schedule
                .pull(4, start + chrono::Duration::days(1))
                .is_empty()
        );
    }
}
