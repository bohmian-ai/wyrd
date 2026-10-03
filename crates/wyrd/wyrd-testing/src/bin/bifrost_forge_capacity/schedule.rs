//! The fixed leader scheduling case, measured in-process.
//!
//! The leader's peer routes are a term check around one [`ForgeSchedule`]
//! operation, so driving the schedule directly measures the leader decision
//! itself: the warm commit-state update a promotion's commit notice applies
//! and the in-memory pull selection a compactor's pull runs. Ten thousand
//! tables are tracked and one thousand are due; thirty-two OS threads pull and
//! report concurrently until nothing is due, which is the contention the one
//! schedule lock sees under a sustained backlog.

use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use vala_bifrost_redux::forge::{
    DEFAULT_REPORT_TIMEOUT, ForgeCommitNotice, ForgeCompactionOutcome, ForgeSchedule,
    ForgeTableKey, ForgeTableSettings,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;
use wyrd_testing::capacity::Percentiles;

use crate::Result;

/// Tasks one compactor pull requests, as a worker with free room asks.
pub const PULL_LIMIT: usize = 4;

/// The scheduling case's shape: tracked tables, due tables, and pullers.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SchedulingCase {
    /// Tables the leader tracks.
    pub tables: usize,
    /// Tracked tables whose commit makes them due.
    pub due: usize,
    /// Concurrent pulling threads.
    pub pullers: usize,
}

/// One repetition of the scheduling case.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulingRun {
    /// Warm commit-state updates applied per second, single-threaded.
    pub commit_updates_per_second: f64,
    /// Warm commit-state update latency, microseconds.
    pub commit_update_us: Percentiles,
    /// Pulls served, including the empty pulls that end each thread.
    pub pulls: usize,
    /// Pull selection latency, microseconds.
    pub pull_us: Percentiles,
    /// Due tables dispatched; equals the due count when the schedule is
    /// correct.
    pub dispatched: usize,
    /// Raw warm-update samples, nanoseconds, pooled across repetitions.
    #[serde(skip)]
    pub commit_update_ns: Vec<u64>,
    /// Raw pull samples, nanoseconds, pooled across repetitions.
    #[serde(skip)]
    pub pull_ns: Vec<u64>,
}

impl SchedulingCase {
    /// Runs the case once against a fresh schedule.
    ///
    /// Every idle table is notified once so its track exists, then notified
    /// again under a timer: the second notice is the warm update. The due
    /// tables are notified last, so the pull threads find exactly `due` due
    /// tables among `tables` tracked ones.
    ///
    /// # Errors
    ///
    /// Returns an invalid generated table identity, or a dispatch count that
    /// differs from the due count, which means the schedule dispatched a
    /// table twice or never.
    ///
    /// # Panics
    ///
    /// Panics when a puller thread panics.
    pub fn run(&self) -> Result<SchedulingRun> {
        let schedule = ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT);
        let tenant = DataTenantId::try_from(uuid::Uuid::now_v7())?;
        let keys = (0..self.tables)
            .map(|index| {
                Ok(ForgeTableKey {
                    tenant,
                    table: ForgeTaskTableIdentity::new(
                        "wyrd-redux",
                        BifrostNamespace::Datasets.as_str(),
                        format!("schedule_{index}"),
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let due = ForgeTableSettings {
            compaction_enabled: true,
            trigger_snapshot_count: 1,
            ..ForgeTableSettings::default()
        };
        let idle = ForgeTableSettings {
            compaction_enabled: true,
            ..ForgeTableSettings::default()
        };
        let notify = |index: usize, snapshot_id: i64| {
            let settings = if index < self.due { &due } else { &idle };
            schedule.notify_commit(
                ForgeCommitNotice {
                    key: keys[index].clone(),
                    snapshot_id,
                    settings: settings.clone(),
                },
                Utc::now(),
            );
        };
        for index in self.due..self.tables {
            notify(index, 1);
        }
        let started = Instant::now();
        let commit_update_ns: Vec<u64> = (self.due..self.tables)
            .map(|index| {
                let warm = Instant::now();
                notify(index, 2);
                nanos(warm.elapsed())
            })
            .collect();
        let commit_updates_per_second =
            commit_update_ns.len() as f64 / started.elapsed().as_secs_f64().max(f64::EPSILON);
        for index in 0..self.due {
            notify(index, 1);
        }
        let (pull_ns, dispatched) = self.pull_until_drained(&schedule);
        if dispatched != self.due {
            return Err(format!(
                "the schedule dispatched {dispatched} tables, not the {} due ones",
                self.due
            )
            .into());
        }
        Ok(SchedulingRun {
            commit_updates_per_second,
            commit_update_us: Percentiles::raw(&commit_update_ns, 1e-3),
            pulls: pull_ns.len(),
            pull_us: Percentiles::raw(&pull_ns, 1e-3),
            dispatched,
            commit_update_ns,
            pull_ns,
        })
    }

    /// Pulls and reports from [`Self::pullers`] OS threads until nothing is
    /// due, each pull requesting [`PULL_LIMIT`] tasks and each dispatch
    /// reported succeeded.
    ///
    /// Returns every pull's latency in nanoseconds and the number of
    /// dispatches whose own report applied.
    ///
    /// # Panics
    ///
    /// Panics when a puller thread panics.
    fn pull_until_drained(&self, schedule: &ForgeSchedule) -> (Vec<u64>, usize) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..self.pullers)
                .map(|_| {
                    scope.spawn(|| {
                        let (mut pulls, mut dispatched) = (Vec::new(), 0_usize);
                        loop {
                            let pulled = Instant::now();
                            let tasks = schedule.pull(PULL_LIMIT, Utc::now());
                            pulls.push(nanos(pulled.elapsed()));
                            if tasks.is_empty() {
                                return (pulls, dispatched);
                            }
                            for task in tasks {
                                dispatched += usize::from(schedule.report(
                                    &task.key,
                                    task.task_id,
                                    ForgeCompactionOutcome::Succeeded,
                                    Utc::now(),
                                ));
                            }
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a puller thread does not panic"))
                .fold((Vec::new(), 0), |(mut pulls, total), (more, dispatched)| {
                    pulls.extend(more);
                    (pulls, total + dispatched)
                })
        })
    }
}

/// `elapsed` in whole nanoseconds, saturating.
fn nanos(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::SchedulingCase;

    /// A small case dispatches every due table exactly once and records one
    /// warm update per idle table.
    ///
    /// # Panics
    ///
    /// Panics when the case fails or miscounts.
    #[test]
    fn small_case_dispatches_every_due_table_once() {
        let run = SchedulingCase {
            tables: 50,
            due: 10,
            pullers: 4,
        }
        .run()
        .expect("the case runs");
        assert_eq!(run.dispatched, 10);
        assert_eq!(run.commit_update_ns.len(), 40);
        assert!(run.pulls >= 4, "every puller ends on one empty pull");
    }
}
