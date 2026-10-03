//! One measured step against one deployment: drive the REQ-171 mix for the
//! step window, wait for every backlog to drain, and collect client,
//! exporter, database, resource, and optional profile evidence.
//!
//! The drain watches every server-owned backlog — the run queue, Scribe,
//! the audit outbox, and Forge demand — from the moment the arrivals stop,
//! and gives up at [`DRAIN_LIMIT`], the REQ-171 saturation SLO, so a step
//! that cannot drain fails instead of stretching the run. A backlog first
//! seen empty after the limit is a miss, not a late pass.
//!
//! Replica CPU and peak memory cover one [`ResourceWindow`]: from just before
//! the arrivals start until every lane, its request tails, and its client
//! flush have ended.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use wyrd_testing::release_server::{LocalServer, MemoryPeak};

use crate::Result;
use crate::evidence::{Backlog, Overhead, Percentiles, Queue, RunTally, Scrapes};
use crate::fixture::{Kind, Tenant};
use crate::judge::Judge;
use crate::load::{Lane, MAX_IN_FLIGHT, Op, Tally, TenantClients, Work, mix};
use crate::profile::{Capture, Profile};

/// How long every backlog may take to drain after load stops (REQ-171).
pub const DRAIN_LIMIT: Duration = Duration::from_secs(60);

/// How often the drain re-reads the backlogs.
const POLL: Duration = Duration::from_millis(250);

/// The deployment every step runs against.
pub struct Deployment {
    /// Serving replicas.
    pub replicas: Vec<LocalServer>,
    /// The four identical tenants.
    pub tenants: Vec<Tenant>,
    /// Each tenant's clients, connected to every replica.
    pub clients: Vec<Arc<TenantClients>>,
    /// The durable queues.
    pub queue: Queue,
    /// The judge provider.
    pub judge: Arc<Judge>,
    /// Driver permits shared by every lane.
    pub permits: Arc<Semaphore>,
    /// Where profiles land when profiling, `None` otherwise.
    pub profiles: Option<PathBuf>,
    /// Binary identity every profile records.
    pub binary: serde_json::Value,
}

/// The closed set of REQ-171 step kinds. The verdict selects its steps by
/// variant, so a misnamed step cannot compile into missing evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Not judged; warms caches and connections.
    Warmup,
    /// One step of the one-replica ramp that finds the knee.
    Ramp,
    /// The knee held for the sustained window.
    Sustained,
    /// Twice the knee on two replicas.
    ScaleOut,
}

impl StepKind {
    /// The one label of this kind: the report's step column, the JSON
    /// `name`, and the profile directory prefix.
    pub fn label(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Ramp => "ramp",
            Self::Sustained => "sustained",
            Self::ScaleOut => "scale-out",
        }
    }
}

impl Serialize for StepKind {
    /// Serializes as [`StepKind::label`].
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

/// One step of the REQ-171 sequence.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// Step kind.
    pub name: StepKind,
    /// Load level `L`: verification executions per second.
    pub level: f64,
    /// Arrival window, seconds.
    pub seconds: f64,
    /// Whether the SLOs decide anything; the warmup is not judged.
    pub judged: bool,
    /// Whether the AC-040 sample floor applies: the one-replica sustained
    /// step.
    pub sample_floor: bool,
}

impl Plan {
    /// A step of `name` at `level` for `seconds`, judged unless it is the
    /// warmup, without the sample floor.
    pub fn new(name: StepKind, level: f64, seconds: f64) -> Self {
        Self {
            name,
            level,
            seconds,
            judged: name != StepKind::Warmup,
            sample_floor: false,
        }
    }
}

/// One operation's evidence over one step, every tenant together.
#[derive(Debug, Default, Clone, Serialize)]
pub struct OpRecord {
    /// Arrivals scheduled.
    pub offered: u64,
    /// Arrivals that found no driver permit.
    pub missed: u64,
    /// Requests that completed inside the arrival window: accepted
    /// requests, or settled runs for the queued path.
    pub completed: u64,
    /// Errors by cause: refusal codes, `lost`, `wrong judgment`, and
    /// `run <status>`.
    pub errors: BTreeMap<String, u64>,
    /// Raw client latency percentiles, milliseconds.
    pub client_ms: Percentiles,
    /// Longest tenant client-queue drain after load stopped, seconds; ingest
    /// only.
    pub drain_seconds: Option<f64>,
}

/// One replica's resource use over one step's [`ResourceWindow`].
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// Replica ordinal.
    pub replica: u16,
    /// Measured length of the window, seconds.
    pub seconds: f64,
    /// Mean cores: cgroup CPU seconds over [`Resources::seconds`].
    pub cores: f64,
    /// Peak cgroup memory over the same window, bytes.
    pub peak_memory: u64,
}

impl Resources {
    /// Replica `replica`'s use between two cgroup CPU readings, each an
    /// instant and `usage_usec`, with the peak memory read at the second.
    /// Cores divide the CPU delta by the readings' own interval, never by a
    /// planned window.
    fn between(
        replica: u16,
        opened: (Instant, u64),
        closed: (Instant, u64),
        peak_memory: u64,
    ) -> Self {
        let seconds = closed.0.saturating_duration_since(opened.0).as_secs_f64();
        let cpu_seconds = closed.1.saturating_sub(opened.1) as f64 / 1e6;
        Self {
            replica,
            seconds,
            cores: if seconds > 0.0 {
                cpu_seconds / seconds
            } else {
                0.0
            },
            peak_memory,
        }
    }
}

/// The one interval a step's replica CPU and peak memory both cover. It
/// opens with every replica's CPU reading and a fresh `memory.peak`
/// descriptor, and closes by reading both again at one instant.
struct ResourceWindow {
    /// When the CPU readings were taken.
    opened: Instant,
    /// Each replica's `usage_usec` at [`ResourceWindow::opened`].
    cpu_usec: Vec<u64>,
    /// Each replica's peak descriptor, reset at [`ResourceWindow::opened`].
    peaks: Vec<MemoryPeak>,
}

impl ResourceWindow {
    /// Opens the window over `replicas`.
    ///
    /// # Errors
    ///
    /// Returns a `memory.peak` open or reset failure.
    fn open(replicas: &[LocalServer]) -> Result<Self> {
        let mut peaks = Vec::new();
        for replica in replicas {
            peaks.push(replica.memory_peak()?);
        }
        Ok(Self {
            opened: Instant::now(),
            cpu_usec: replicas.iter().map(cpu_usec).collect(),
            peaks,
        })
    }

    /// Closes the window over the same `replicas`, in the same order.
    ///
    /// # Errors
    ///
    /// Returns a `memory.peak` read failure.
    fn close(self, replicas: &[LocalServer]) -> Result<Vec<Resources>> {
        let closed = Instant::now();
        let mut resources = Vec::new();
        for ((replica, before), peak) in replicas.iter().zip(self.cpu_usec).zip(self.peaks) {
            resources.push(Resources::between(
                replica.ordinal(),
                (self.opened, before),
                (closed, cpu_usec(replica)),
                peak.read()?,
            ));
        }
        Ok(resources)
    }
}

/// `replica`'s cgroup CPU time, microseconds.
fn cpu_usec(replica: &LocalServer) -> u64 {
    replica.cgroup_stat("cpu.stat", "usage_usec")
}

/// What one backlog read means for the drain.
#[derive(Debug, PartialEq)]
enum Drain {
    /// Not empty yet, with time left.
    Pending,
    /// Empty, seen this many seconds after load stopped, within the limit.
    Drained(f64),
    /// The limit passed before an empty read.
    Expired,
}

impl Drain {
    /// Judges a read taken `elapsed` after load stopped: empty at or before
    /// [`DRAIN_LIMIT`] drains, anything at or after it otherwise expires.
    fn judge(elapsed: Duration, empty: bool) -> Self {
        if empty && elapsed <= DRAIN_LIMIT {
            Self::Drained(elapsed.as_secs_f64())
        } else if elapsed >= DRAIN_LIMIT {
            Self::Expired
        } else {
            Self::Pending
        }
    }
}

/// Everything one step produced.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    /// Replicas serving.
    pub replicas: usize,
    /// The plan.
    pub plan: Plan,
    /// Actual arrival window, seconds.
    pub window_seconds: f64,
    /// Per operation, in [`Op::ALL`] order.
    pub ops: Vec<(Op, OpRecord)>,
    /// Direct engine overhead per kind, in [`Kind::ALL`] order.
    pub overhead: Vec<Overhead>,
    /// Seconds from load stopping until every backlog was empty; `None`
    /// when [`DRAIN_LIMIT`] passed first.
    pub backlog_drain_seconds: Option<f64>,
    /// What every backlog held when the drain ended.
    pub backlog: Backlog,
    /// Per-replica resources.
    pub resources: Vec<Resources>,
    /// Judge completions during the step.
    pub judge_calls: usize,
    /// The local judge provider's wait per completion, milliseconds:
    /// reported apart from judge engine overhead, never judged (AC-040).
    pub judge_wait_ms: Percentiles,
    /// Profiles, when captured.
    pub profiles: Vec<Profile>,
}

impl Record {
    /// The evidence of `op`.
    pub fn op(&self, op: Op) -> Option<&OpRecord> {
        self.ops
            .iter()
            .find(|(candidate, _)| *candidate == op)
            .map(|(_, record)| record)
    }
}

impl Deployment {
    /// Runs `plan` and returns its evidence.
    ///
    /// # Errors
    ///
    /// Returns a driver, scrape, query, flush, or capture failure.
    ///
    /// # Cancellation
    ///
    /// Dropping the future mid-step aborts every in-flight request task and
    /// kills any `perf` capture, but requests that already reached a replica
    /// keep their durable effects: enqueued runs, admitted observations, and
    /// audit rows stay in the shared database and store, and observations
    /// still queued in the tenants' clients are flushed or dropped by the
    /// clients' owner. No partial [`Record`] survives; a retry runs a new
    /// step whose evidence starts from that residue, so it is not comparable
    /// with an uncancelled step.
    pub async fn run(&self, plan: Plan) -> Result<Record> {
        let since = self.queue.now().await?;
        let mut before = Vec::new();
        for replica in &self.replicas {
            before.push(replica.metrics().await?);
        }
        let judge_before = self.judge.calls();
        let resources = ResourceWindow::open(&self.replicas)?;
        let captures = self.start_captures(&plan)?;

        let lanes = mix(plan.level, self.tenants.len());
        let start = Instant::now() + Duration::from_millis(100);
        let window = Duration::from_secs_f64(plan.seconds);
        let mut running = Vec::new();
        for lane in lanes.iter().copied() {
            running.push(lane.drive(
                &self.tenants[lane.tenant],
                Arc::clone(&self.clients[lane.tenant]),
                Arc::clone(&self.permits),
                start,
                window,
            ));
        }
        let tallies = futures_util::future::try_join_all(running).await?;
        let resources = resources.close(&self.replicas)?;
        let stopped = self.queue.now().await?;
        let window_seconds = window.as_secs_f64();
        let mut profiles = Vec::new();
        let metadata = serde_json::json!({ "step": plan, "binary": self.binary });
        for capture in captures {
            profiles.push(capture.finish(&metadata)?);
        }

        let activations: u64 = lanes
            .iter()
            .zip(&tallies)
            .filter(|(lane, _)| lane.work.op() == Op::Queued)
            .map(|(_, tally)| tally.accepted)
            .sum();
        let (backlog_drain_seconds, backlog, after) = self
            .drain(start + window, since, stopped, activations)
            .await?;
        let scrapes = Scrapes { before, after };
        let runs = self.queue.runs(since).await?;
        let ops = self.ops(&lanes, &tallies, &runs, window, backlog.runs == 0)?;
        Ok(Record {
            replicas: self.replicas.len(),
            plan,
            window_seconds,
            ops,
            overhead: Kind::ALL
                .into_iter()
                .map(|kind| scrapes.overhead(kind))
                .collect(),
            backlog_drain_seconds,
            backlog,
            resources,
            judge_calls: self.judge.calls().saturating_sub(judge_before),
            judge_wait_ms: Percentiles::raw(&self.judge.waits_since(judge_before), 1e-3),
            profiles,
        })
    }

    /// Waits from `stopped_at` until every backlog is empty or
    /// [`DRAIN_LIMIT`] passes, and returns the seconds it took (`None` when
    /// the limit passed first, including a first empty read after it), the
    /// last backlog read, and every replica's final scrape.
    ///
    /// # Errors
    ///
    /// Returns a scrape or query failure.
    ///
    /// # Cancellation
    ///
    /// Read-only: dropping it leaves the backlogs to drain on their own.
    async fn drain(
        &self,
        stopped_at: Instant,
        since: chrono::DateTime<chrono::Utc>,
        stopped: chrono::DateTime<chrono::Utc>,
        activations: u64,
    ) -> Result<(
        Option<f64>,
        Backlog,
        Vec<wyrd_testing::release_server::Metrics>,
    )> {
        loop {
            let (backlog, scrapes) = self
                .queue
                .poll(since, stopped, activations, async || {
                    let mut scrapes = Vec::new();
                    for replica in &self.replicas {
                        scrapes.push(replica.metrics().await?);
                    }
                    Ok(scrapes)
                })
                .await?;
            match Drain::judge(stopped_at.elapsed(), backlog.is_empty()) {
                Drain::Drained(seconds) => return Ok((Some(seconds), backlog, scrapes)),
                Drain::Expired => return Ok((None, backlog, scrapes)),
                Drain::Pending => tokio::time::sleep(POLL).await,
            }
        }
    }

    /// Stops every replica newest first, copying each log into `output` as
    /// `server-<ordinal>.log`, and returns each stop's seconds or error in
    /// ordinal order. The deployment is left without replicas.
    ///
    /// Each [`LocalServer::stop`] is synchronous and may wait out the whole
    /// [`STOP_GRACE`](wyrd_testing::release_server::STOP_GRACE) before its
    /// kill, reap, and log copy, so it runs on Tokio's blocking pool while
    /// this future awaits it; the async workers stay free for timers and
    /// other tasks during cleanup. A stop that panics is reported as that
    /// replica's error.
    ///
    /// # Cancellation
    ///
    /// Dropping the future abandons the stop in progress, which still
    /// finishes on the blocking pool; later replicas are killed by their
    /// `Drop`.
    pub async fn stop_replicas(&mut self, output: &Path) -> Vec<std::result::Result<f64, String>> {
        let mut shutdown = Vec::new();
        for replica in std::mem::take(&mut self.replicas).into_iter().rev() {
            let log = output.join(format!("server-{}.log", replica.ordinal()));
            let stopped =
                tokio::task::spawn_blocking(move || replica.stop(&log).map_err(|e| e.to_string()))
                    .await;
            shutdown.push(stopped.unwrap_or_else(|error| Err(error.to_string())));
        }
        shutdown.reverse();
        shutdown
    }

    /// Folds every lane's tally, and the queued lanes' durable runs, into
    /// one record per operation. Queued losses count only once the run
    /// queue `drained`, since an undrained run is backlog, not loss.
    ///
    /// # Errors
    ///
    /// Returns an error when a lane's tenant does not run its workload.
    fn ops(
        &self,
        lanes: &[Lane],
        tallies: &[Tally],
        runs: &BTreeMap<String, RunTally>,
        window: Duration,
        drained: bool,
    ) -> Result<Vec<(Op, OpRecord)>> {
        let mut records: BTreeMap<Op, (OpRecord, Vec<u64>)> = BTreeMap::new();
        for (lane, tally) in lanes.iter().zip(tallies) {
            let (record, samples) = records.entry(lane.work.op()).or_default();
            record.offered += tally.offered;
            record.missed += tally.missed;
            samples.extend_from_slice(&tally.latency_us);
            for (code, count) in &tally.rejected {
                *record.errors.entry(code.clone()).or_default() += count;
            }
            if tally.wrong_verdicts > 0 {
                *record
                    .errors
                    .entry("wrong judgment".to_owned())
                    .or_default() += tally.wrong_verdicts;
            }
            if let Some(drain) = tally.drain {
                let seconds = drain.as_secs_f64();
                record.drain_seconds =
                    Some(record.drain_seconds.map_or(seconds, |d| d.max(seconds)));
            }
            let Work::Queued(kind) = lane.work else {
                record.completed += within(&tally.completed_at, window);
                continue;
            };
            let (verifier, _) = self.tenants[lane.tenant].target(kind)?;
            let runs = runs.get(&verifier.to_string()).cloned().unwrap_or_default();
            record.completed += within(&runs.settled_at, window);
            if drained && runs.created < tally.accepted {
                *record.errors.entry("lost".to_owned()).or_default() +=
                    tally.accepted - runs.created;
            }
            for (status, count) in &runs.statuses {
                if !matches!(
                    status.as_str(),
                    "completed" | "pending" | "running" | "retrying"
                ) {
                    *record.errors.entry(format!("run {status}")).or_default() += count;
                }
            }
        }
        Ok(records
            .into_iter()
            .map(|(op, (mut record, samples))| {
                record.client_ms = Percentiles::raw(&samples, 1e-3);
                (op, record)
            })
            .collect())
    }

    /// Starts one capture per replica when profiling.
    ///
    /// # Errors
    ///
    /// Returns a replica without a PID or a capture that cannot start.
    fn start_captures(&self, plan: &Plan) -> Result<Vec<Capture>> {
        let Some(root) = self.profiles.as_ref() else {
            return Ok(Vec::new());
        };
        let mut captures = Vec::new();
        for replica in &self.replicas {
            let pid = replica.pid().ok_or("a profiled replica has exited")?;
            let directory = root
                .join(format!(
                    "{}-{}-replicas-{}",
                    plan.name.label(),
                    plan.level,
                    self.replicas.len()
                ))
                .join(format!("replica-{}", replica.ordinal()));
            captures.push(Capture::start(pid, replica.ordinal(), &directory)?);
        }
        Ok(captures)
    }
}

/// How many of `completions`, measured from the step start, fall inside
/// `window`.
fn within(completions: &[Duration], window: Duration) -> u64 {
    completions
        .iter()
        .filter(|completed| **completed <= window)
        .count() as u64
}

/// The shared driver permits.
pub fn permits() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(MAX_IN_FLIGHT))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::{DRAIN_LIMIT, Drain, Resources};

    /// A backlog seen empty below or exactly at the 60 s limit drains; one
    /// first seen empty after it expires, as does a non-empty read at it.
    ///
    /// # Panics
    ///
    /// Panics when a read is judged wrongly.
    #[test]
    fn a_backlog_drains_only_within_the_limit() {
        let below = DRAIN_LIMIT - Duration::from_millis(100);
        assert_eq!(
            Drain::judge(below, true),
            Drain::Drained(below.as_secs_f64())
        );
        assert_eq!(Drain::judge(below, false), Drain::Pending);
        assert_eq!(Drain::judge(DRAIN_LIMIT, true), Drain::Drained(60.0));
        assert_eq!(Drain::judge(DRAIN_LIMIT, false), Drain::Expired);
        let above = DRAIN_LIMIT + Duration::from_millis(100);
        assert_eq!(Drain::judge(above, true), Drain::Expired);
    }

    /// Cores divide the CPU delta by the interval between the two readings
    /// whose instants bound the memory peak too, not by any planned window.
    ///
    /// # Panics
    ///
    /// Panics when the arithmetic is wrong.
    #[test]
    fn resources_use_the_readings_interval() {
        let opened = Instant::now();
        let closed = opened + Duration::from_millis(2_500);
        let resources = Resources::between(1, (opened, 1_000_000), (closed, 6_000_000), 42);
        assert_eq!(resources.replica, 1);
        assert!((resources.seconds - 2.5).abs() < 1e-9);
        assert!((resources.cores - 2.0).abs() < 1e-9);
        assert_eq!(resources.peak_memory, 42);
        assert!(
            Resources::between(0, (opened, 5), (opened, 9), 0)
                .cores
                .abs()
                < f64::EPSILON
        );
    }
}
