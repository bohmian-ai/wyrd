//! The live deployment — one Forge leader and its dedicated workers — and the
//! measurement of one window over it.
//!
//! The leader runs `WYRD_TARGET=server`: it serves ingest, promotes sealed
//! Scribe objects into Iceberg, and schedules compaction, but executes none.
//! Every rewrite therefore runs on a `WYRD_TARGET=forge-worker` replica that
//! pulls it from the leader over the peer route. A window records, between two
//! scrapes, each worker's succeeded compaction attempts and their duration
//! histogram, the published input and output files, the leader's promotion
//! commits, every process's cgroup CPU, each worker's sampled
//! occupancy, and the shared dependencies' utilization.

use std::time::{Duration, Instant};

use serde::Serialize;
use vala_bifrost_redux::forge::ForgeWorkerConfig;
use wyrd_testing::capacity::{Percentiles, ResourceWindow, Resources, merged};
use wyrd_testing::release_server::{CPUS, LocalServer, Metrics};

use crate::Result;
use crate::dependencies::{Dependencies, DependencyUsage};
use crate::fleet::Writers;
use crate::schedule::PULL_LIMIT;

/// Compaction's task-type label: Forge's small-files strategy.
const COMPACTION: &str = "task_type=\"small_files\"";

/// Promotion's task-type label.
const PROMOTION: &str = "task_type=\"scribe_promotion\"";

/// A succeeded attempt's result label.
const SUCCEEDED: &str = "result=\"succeeded\"";

/// The leader's schedule-decision family, timed around each schedule call
/// including the wait for its lock.
const LEADER_DECISION: &str = "bifrost_forge_leader_decision_seconds";

/// How often a window samples worker occupancy and Postgres activity.
const SAMPLE_EVERY: Duration = Duration::from_millis(250);

/// One leader, its workers, and the dependencies they share.
pub struct Deployment {
    /// The Forge leader.
    pub leader: LocalServer,
    /// Dedicated Forge workers, in start order.
    pub workers: Vec<LocalServer>,
    /// Postgres and the object store.
    pub dependencies: Dependencies,
}

/// One worker's evidence over a window.
#[derive(Debug, Clone, Serialize)]
pub struct WorkerWindow {
    /// Replica ordinal.
    pub replica: u16,
    /// Succeeded compaction attempts per second.
    pub rewrites_per_second: f64,
    /// Share of samples with at least one compaction task active.
    pub occupancy: f64,
    /// Mean concurrently running compaction tasks: succeeded task seconds
    /// over window seconds.
    pub mean_active_tasks: f64,
}

/// One leader schedule operation's live latency over a window.
///
/// The leader exports the family without explicit buckets, so the Prometheus
/// recorder renders it as a summary whose quantiles cover the recorder's
/// rolling window (three 20 s buckets): read at the end of a 60 s window they
/// describe that window's operations.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LeaderDecision {
    /// Operations per second over the window.
    pub per_second: f64,
    /// Latency, microseconds; absent when no operation ran in the window.
    pub us: Percentiles,
}

/// The leader's live schedule decisions over a window.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LeaderDecisions {
    /// Commit notices applied to the schedule.
    pub commit: LeaderDecision,
    /// Compactor pulls served.
    pub pull: LeaderDecision,
    /// Compactor reports settled.
    pub report: LeaderDecision,
}

impl LeaderDecision {
    /// Reads `operation` from the leader scrapes that open and close a
    /// window `seconds` long.
    fn read(before: &Metrics, after: &Metrics, operation: &str, seconds: f64) -> Self {
        let label = format!("operation=\"{operation}\"");
        let count_family = format!("{LEADER_DECISION}_count");
        let ran = after.sum(&count_family, &[&label]) - before.sum(&count_family, &[&label]);
        let quantile = |q: &str| {
            (ran > 0.0)
                .then(|| after.sum(LEADER_DECISION, &[&label, &format!("quantile=\"{q}\"")]) * 1e6)
        };
        Self {
            per_second: ran / seconds,
            us: Percentiles {
                p50: quantile("0.5"),
                p95: quantile("0.95"),
                p99: quantile("0.99"),
            },
        }
    }
}

/// Everything one measurement window produced.
#[derive(Debug, Clone, Serialize)]
pub struct WindowRecord {
    /// Workers serving.
    pub workers: usize,
    /// Window length, seconds.
    pub seconds: f64,
    /// Succeeded compaction attempts per second, summed across workers.
    pub rewrites_per_second: f64,
    /// Compaction attempts that did not succeed, by result.
    pub unsuccessful_attempts: f64,
    /// Input files the rewrites consumed, per second.
    pub input_files_per_second: f64,
    /// Output files the rewrites published, per second.
    pub output_files_per_second: f64,
    /// Bytes the rewrites consumed, per second.
    pub input_bytes_per_second: f64,
    /// Succeeded compaction duration, bucket estimates in seconds.
    pub rewrite_seconds: Percentiles,
    /// Succeeded promotion commits per second on the leader: the Iceberg
    /// commit rate that makes tables due.
    pub promotion_commits_per_second: f64,
    /// Succeeded promotion duration on the leader, bucket estimates in
    /// seconds.
    pub promotion_seconds: Percentiles,
    /// The leader's live commit, pull, and report decisions.
    pub leader_decisions: LeaderDecisions,
    /// The most rewrites per second the workers' pull cadence can dispatch:
    /// each worker pulls at most [`PULL_LIMIT`] tasks per pull interval.
    pub pull_ceiling_per_second: f64,
    /// Client batches acknowledged per second.
    pub writes_per_second: f64,
    /// Client batches refused during the window.
    pub refused_writes: u64,
    /// Leader CPU as a share of its envelope.
    pub leader_cpu_share: f64,
    /// Per-worker evidence.
    pub per_worker: Vec<WorkerWindow>,
    /// Leader then worker resources.
    pub resources: Vec<Resources>,
    /// Postgres and object-store utilization.
    pub dependencies: DependencyUsage,
}

impl Deployment {
    /// Starts dedicated workers until `count` serve, giving each the
    /// environment `env` returns for its ordinal.
    ///
    /// # Errors
    ///
    /// Returns a worker that never becomes ready.
    pub async fn grow_to<'a>(
        &mut self,
        binary: &std::path::Path,
        count: usize,
        env: impl Fn(u16) -> Vec<(&'a str, &'a str)>,
    ) -> Result<()> {
        while self.workers.len() < count {
            let ordinal = u16::try_from(self.workers.len() + 1)?;
            let worker = self
                .leader
                .start_forge_worker(binary, ordinal, &env(ordinal))
                .await?;
            self.workers.push(worker);
        }
        Ok(())
    }

    /// Measures one window of `seconds` while `writers` keep committing.
    ///
    /// # Errors
    ///
    /// Returns a scrape, cgroup, or dependency-statistics failure.
    pub async fn measure(&self, writers: &Writers, seconds: f64) -> Result<WindowRecord> {
        let processes = || std::iter::once(&self.leader).chain(&self.workers);
        let leader_before = self.leader.metrics().await?;
        let mut workers_before = Vec::new();
        for worker in &self.workers {
            workers_before.push(worker.metrics().await?);
        }
        let dependencies_before = self.dependencies.read().await?;
        let (acked_before, refused_before) = writers.counts();
        let mut resources = ResourceWindow::open(processes())?;
        let started = Instant::now();
        let window = Duration::from_secs_f64(seconds);

        let mut busy = vec![0_u64; self.workers.len()];
        let (mut samples, mut backends) = (0_u64, Vec::new());
        let mut next = started;
        while started.elapsed() < window {
            for (worker, busy) in self.workers.iter().zip(&mut busy) {
                let active = worker
                    .metrics()
                    .await?
                    .sum("bifrost_forge_active_tasks", &[COMPACTION]);
                *busy += u64::from(active > 0.0);
            }
            backends.push(self.dependencies.active_backends().await?);
            samples += 1;
            next += SAMPLE_EVERY;
            tokio::time::sleep_until(next.into()).await;
        }

        let elapsed = started.elapsed().as_secs_f64();
        resources.freeze(processes());
        let (acked_after, refused_after) = writers.counts();
        let dependencies_after = self.dependencies.read().await?;
        let leader_after = self.leader.metrics().await?;
        let mut workers_after = Vec::new();
        for worker in &self.workers {
            workers_after.push(worker.metrics().await?);
        }
        let resources = resources.finish(processes(), elapsed)?;

        let delta = |before: &[Metrics], after: &[Metrics], family: &str, labels: &[&str]| {
            before
                .iter()
                .zip(after)
                .map(|(before, after)| after.sum(family, labels) - before.sum(family, labels))
                .sum::<f64>()
        };
        let per_worker = self
            .workers
            .iter()
            .zip(workers_before.iter().zip(&workers_after))
            .zip(&busy)
            .map(|((worker, (before, after)), busy)| {
                let attempts = delta(
                    std::slice::from_ref(before),
                    std::slice::from_ref(after),
                    "bifrost_forge_task_attempts_total",
                    &[COMPACTION, SUCCEEDED],
                );
                let task_seconds = delta(
                    std::slice::from_ref(before),
                    std::slice::from_ref(after),
                    "bifrost_forge_task_duration_seconds_sum",
                    &[COMPACTION, SUCCEEDED],
                );
                WorkerWindow {
                    replica: worker.ordinal(),
                    rewrites_per_second: attempts / elapsed,
                    occupancy: *busy as f64 / samples.max(1) as f64,
                    mean_active_tasks: task_seconds / elapsed,
                }
            })
            .collect::<Vec<_>>();
        let workers_rate = |family: &str, labels: &[&str]| {
            delta(&workers_before, &workers_after, family, labels) / elapsed
        };
        let leader = [leader_before];
        let leader_after = [leader_after];
        let leader_rate =
            |family: &str, labels: &[&str]| delta(&leader, &leader_after, family, labels) / elapsed;
        let succeeded = [COMPACTION, SUCCEEDED];
        Ok(WindowRecord {
            workers: self.workers.len(),
            seconds: elapsed,
            rewrites_per_second: per_worker.iter().map(|w| w.rewrites_per_second).sum(),
            unsuccessful_attempts: (workers_rate(
                "bifrost_forge_task_attempts_total",
                &[COMPACTION],
            ) - workers_rate(
                "bifrost_forge_task_attempts_total",
                &succeeded,
            )) * elapsed,
            input_files_per_second: workers_rate("bifrost_forge_input_files_total", &[COMPACTION]),
            output_files_per_second: workers_rate(
                "bifrost_forge_output_files_total",
                &[COMPACTION],
            ),
            input_bytes_per_second: workers_rate("bifrost_forge_input_bytes_total", &[COMPACTION]),
            rewrite_seconds: Percentiles::buckets(
                &merged(
                    &workers_before,
                    "bifrost_forge_task_duration_seconds",
                    &succeeded,
                ),
                &merged(
                    &workers_after,
                    "bifrost_forge_task_duration_seconds",
                    &succeeded,
                ),
            ),
            promotion_commits_per_second: leader_rate(
                "bifrost_forge_task_attempts_total",
                &[PROMOTION, SUCCEEDED],
            ),
            promotion_seconds: Percentiles::buckets(
                &merged(
                    &leader,
                    "bifrost_forge_task_duration_seconds",
                    &[PROMOTION, SUCCEEDED],
                ),
                &merged(
                    &leader_after,
                    "bifrost_forge_task_duration_seconds",
                    &[PROMOTION, SUCCEEDED],
                ),
            ),
            leader_decisions: LeaderDecisions {
                commit: LeaderDecision::read(&leader[0], &leader_after[0], "commit", elapsed),
                pull: LeaderDecision::read(&leader[0], &leader_after[0], "pull", elapsed),
                report: LeaderDecision::read(&leader[0], &leader_after[0], "report", elapsed),
            },
            pull_ceiling_per_second: self.workers.len() as f64 * PULL_LIMIT as f64
                / ForgeWorkerConfig::default().pull_interval.as_secs_f64(),
            writes_per_second: (acked_after - acked_before) as f64 / elapsed,
            refused_writes: refused_after - refused_before,
            leader_cpu_share: resources
                .first()
                .map_or(0.0, |leader| leader.cores / CPUS as f64),
            per_worker,
            resources,
            dependencies: self.dependencies.usage(
                dependencies_before,
                dependencies_after,
                &backends,
                elapsed,
            ),
        })
    }
}
