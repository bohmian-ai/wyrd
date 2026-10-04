//! The live deployment — one Forge leader and its dedicated workers — and the
//! measurement of one window over it.
//!
//! The leader runs `WYRD_TARGET=server`: it serves ingest, promotes sealed
//! Scribe objects into Iceberg, and schedules compaction, but executes none.
//! Every rewrite therefore runs on a `WYRD_TARGET=forge-worker` replica that
//! pulls it from the leader over the peer route. A window records, between two
//! scrapes, each worker's succeeded compaction attempts and their duration
//! histogram, the published input and output files, the leader's promotion
//! commits and live decisions, every process's cgroup CPU, the shared
//! dependencies' utilization, and — from the leader's debug log — every pull
//! it served, how many tasks it returned, and whether a due table remained.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use vala_bifrost_redux::forge::ForgeWorkerConfig;
use wyrd_testing::capacity::{Percentiles, ResourceWindow, Resources, merged};
use wyrd_testing::release_server::{Envelope, LocalServer, Metrics};

use crate::Result;
use crate::dependencies::{Dependencies, DependencyUsage};
use crate::fleet::Writers;
use crate::schedule::PULL_LIMIT;

/// Compaction's task-type metric label. Forge labels a compaction attempt
/// `small_files` whatever its compaction type, including the default `full`.
const COMPACTION: &str = "task_type=\"small_files\"";

/// Promotion's task-type label.
const PROMOTION: &str = "task_type=\"scribe_promotion\"";

/// A succeeded attempt's result label.
const SUCCEEDED: &str = "result=\"succeeded\"";

/// The leader's schedule-decision family, timed around each schedule call
/// including the wait for its lock.
const LEADER_DECISION: &str = "bifrost_forge_leader_decision_seconds";

/// The leader's debug line for one served pull.
const PULL_SERVED: &str = "Forge compaction pull served";

/// How often a window samples Postgres activity.
const SAMPLE_EVERY: Duration = Duration::from_millis(250);

/// One leader, its workers, and the dependencies they share.
pub struct Deployment {
    /// The Forge leader.
    pub leader: LocalServer,
    /// Dedicated Forge workers, in start order.
    pub workers: Vec<LocalServer>,
    /// Postgres and the object store.
    pub dependencies: Dependencies,
    /// Read position in the leader's log, so each window parses only the
    /// pulls served during it.
    pub leader_log: LogCursor,
}

/// A read position in a growing log file.
pub struct LogCursor {
    /// The log.
    path: PathBuf,
    /// Bytes already read.
    offset: u64,
}

impl LogCursor {
    /// A cursor at the current end of `path`, so only later lines are read.
    pub fn at_end(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            offset: std::fs::metadata(path).map_or(0, |meta| meta.len()),
        }
    }

    /// A cursor at the start of `path`, so the whole log is read.
    pub fn at_start(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            offset: 0,
        }
    }

    /// The complete lines appended since the last read, ANSI styling
    /// removed; a trailing partial line is left for the next read.
    ///
    /// # Errors
    ///
    /// Returns an unreadable log.
    pub fn read_new(&mut self) -> Result<String> {
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let complete = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |end| end + 1);
        self.offset += u64::try_from(complete)?;
        Ok(strip_ansi(&String::from_utf8_lossy(&bytes[..complete])))
    }
}

/// One worker's evidence over a window.
#[derive(Debug, Clone, Serialize)]
pub struct WorkerWindow {
    /// Replica ordinal.
    pub replica: u16,
    /// Succeeded compaction attempts per second.
    pub rewrites_per_second: f64,
    /// Mean concurrently running compaction tasks: succeeded task seconds
    /// over window seconds.
    pub mean_active_tasks: f64,
    /// CPU as a share of the worker's envelope.
    pub cpu_share: f64,
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

impl LeaderDecisions {
    /// Reads all three operations from the leader scrapes that open and close
    /// a window `seconds` long.
    pub fn read(before: &Metrics, after: &Metrics, seconds: f64) -> Self {
        Self {
            commit: LeaderDecision::read(before, after, "commit", seconds),
            pull: LeaderDecision::read(before, after, "pull", seconds),
            report: LeaderDecision::read(before, after, "report", seconds),
        }
    }
}

/// Every pull the leader served over a window, from its debug log.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PullEvidence {
    /// Pulls served.
    pub pulls: u64,
    /// Pulls that found at least one due table.
    pub pulls_with_due: u64,
    /// Pulls answered with fewer tasks than requested.
    pub short: u64,
    /// Pulls answered with fewer tasks than requested while a due table
    /// remained: the full-answer violation.
    pub short_while_due: u64,
    /// Tasks requested.
    pub requested: u64,
    /// Tasks returned.
    pub returned: u64,
}

impl PullEvidence {
    /// Tallies every [`PULL_SERVED`] line in `log`.
    pub fn parse(log: &str) -> Self {
        let mut evidence = Self::default();
        for line in log.lines().filter(|line| line.contains(PULL_SERVED)) {
            let field = |name: &str| {
                line.split_whitespace()
                    .find_map(|token| token.strip_prefix(name)?.strip_prefix('='))
            };
            let (Some(requested), Some(returned)) = (
                field("requested").and_then(|v| v.parse::<u64>().ok()),
                field("returned").and_then(|v| v.parse::<u64>().ok()),
            ) else {
                continue;
            };
            let still_due = field("still_due") == Some("true");
            evidence.pulls += 1;
            evidence.requested += requested;
            evidence.returned += returned;
            evidence.pulls_with_due += u64::from(returned > 0 || still_due);
            if returned < requested {
                evidence.short += 1;
                evidence.short_while_due += u64::from(still_due);
            }
        }
        evidence
    }

    /// Adds `other`'s tallies.
    pub fn add(&mut self, other: Self) {
        self.pulls += other.pulls;
        self.pulls_with_due += other.pulls_with_due;
        self.short += other.short;
        self.short_while_due += other.short_while_due;
        self.requested += other.requested;
        self.returned += other.returned;
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
    /// Compaction attempts that did not succeed.
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
    /// Every pull the leader served.
    pub pulls: PullEvidence,
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
    /// Starts dedicated workers in `envelope` until `count` serve, giving
    /// each the environment `env` returns for its ordinal.
    ///
    /// # Errors
    ///
    /// Returns a worker that never becomes ready.
    pub async fn grow_to<'a>(
        &mut self,
        binary: &Path,
        count: usize,
        envelope: Envelope,
        env: impl Fn(u16) -> Vec<(&'a str, &'a str)>,
    ) -> Result<()> {
        while self.workers.len() < count {
            let ordinal = u16::try_from(self.workers.len() + 1)?;
            let worker = self
                .leader
                .start_forge_worker(binary, ordinal, &env(ordinal), envelope)
                .await?;
            self.workers.push(worker);
        }
        Ok(())
    }

    /// Measures one window of `seconds` while `writers` keep committing.
    ///
    /// # Errors
    ///
    /// Returns a scrape, log, cgroup, or dependency-statistics failure.
    pub async fn measure(&mut self, writers: &Writers, seconds: f64) -> Result<WindowRecord> {
        let leader_before = self.leader.metrics().await?;
        let mut workers_before = Vec::new();
        for worker in &self.workers {
            workers_before.push(worker.metrics().await?);
        }
        let dependencies_before = self.dependencies.read().await?;
        let (acked_before, refused_before) = writers.counts();
        self.leader_log.read_new()?;
        let processes = || std::iter::once(&self.leader).chain(&self.workers);
        let mut resources = ResourceWindow::open(processes())?;
        let started = Instant::now();
        let window = Duration::from_secs_f64(seconds);

        let mut backends = Vec::new();
        let mut next = started;
        while started.elapsed() < window {
            backends.push(self.dependencies.active_backends().await?);
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
        let pulls = PullEvidence::parse(&self.leader_log.read_new()?);

        let delta = |before: &[Metrics], after: &[Metrics], family: &str, labels: &[&str]| {
            before
                .iter()
                .zip(after)
                .map(|(before, after)| after.sum(family, labels) - before.sum(family, labels))
                .sum::<f64>()
        };
        let succeeded = [COMPACTION, SUCCEEDED];
        let per_worker = self
            .workers
            .iter()
            .zip(workers_before.iter().zip(&workers_after))
            .zip(resources.iter().skip(1))
            .map(|((worker, (before, after)), resource)| {
                let one = |family: &str| {
                    delta(
                        std::slice::from_ref(before),
                        std::slice::from_ref(after),
                        family,
                        &succeeded,
                    )
                };
                WorkerWindow {
                    replica: worker.ordinal(),
                    rewrites_per_second: one("bifrost_forge_task_attempts_total") / elapsed,
                    mean_active_tasks: one("bifrost_forge_task_duration_seconds_sum") / elapsed,
                    cpu_share: resource.cores / worker.envelope().cpus(),
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
            leader_decisions: LeaderDecisions::read(&leader[0], &leader_after[0], elapsed),
            pulls,
            pull_ceiling_per_second: self.workers.len() as f64 * PULL_LIMIT as f64
                / ForgeWorkerConfig::default().pull_interval.as_secs_f64(),
            writes_per_second: (acked_after - acked_before) as f64 / elapsed,
            refused_writes: refused_after - refused_before,
            leader_cpu_share: resources
                .first()
                .map_or(0.0, |leader| leader.cores / self.leader.envelope().cpus()),
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

/// `text` with ANSI escape sequences (`ESC [ ... letter`) removed.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{PullEvidence, strip_ansi};

    /// Styled leader pull lines parse into requested and returned counts, and
    /// only a short answer with a due table left counts as a violation.
    ///
    /// # Panics
    ///
    /// Panics when a line is miscounted.
    #[test]
    fn pull_evidence_counts_short_answers_while_due() {
        let styled = "\u{1b}[2m2026-10-04T01:08:21Z\u{1b}[0m \u{1b}[34mDEBUG\u{1b}[0m x: Forge compaction pull served \
                      \u{1b}[3mrequested\u{1b}[0m\u{1b}[2m=\u{1b}[0m4 \u{1b}[3mreturned\u{1b}[0m\u{1b}[2m=\u{1b}[0m2 \
                      \u{1b}[3mstill_due\u{1b}[0m\u{1b}[2m=\u{1b}[0mfalse\n";
        let log = format!(
            "{}x: Forge compaction pull served requested=4 returned=4 still_due=true\n\
             x: Forge compaction pull served requested=4 returned=1 still_due=true\n\
             x: Forge compaction pull served requested=4 returned=0 still_due=false\n\
             x: Forge compaction dispatched table=t\n",
            strip_ansi(styled)
        );
        let evidence = PullEvidence::parse(&log);
        assert_eq!(evidence.pulls, 4);
        assert_eq!(evidence.short, 3);
        assert_eq!(evidence.short_while_due, 1);
        assert_eq!(evidence.pulls_with_due, 3);
        assert_eq!((evidence.requested, evidence.returned), (16, 7));
    }
}
