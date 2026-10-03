//! One measured step against one deployment: drive the REQ-171 mix for the
//! step window, wait for every backlog to drain, and collect client,
//! exporter, database, resource, and optional profile evidence.
//!
//! The drain watches every server-owned backlog — the run queue, Scribe,
//! the audit outbox, and Forge demand — from the moment the arrivals stop,
//! and gives up at [`DRAIN_LIMIT`], the REQ-171 saturation SLO, so a step
//! that cannot drain fails instead of stretching the run.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use wyrd_testing::release_server::LocalServer;

use crate::Result;
use crate::evidence::{Backlog, Overhead, Percentiles, Queue, RunTally, Scrapes, scribe_backlog};
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

/// One step of the REQ-171 sequence.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// Step name: `warmup`, `ramp`, `sustained`, or `scale-out`.
    pub name: &'static str,
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
    /// Errors by cause: refusal codes, `lost`, `duplicate run`,
    /// `wrong judgment`, and `run <status>`.
    pub errors: BTreeMap<String, u64>,
    /// Raw client latency percentiles, milliseconds.
    pub client_ms: Percentiles,
    /// Longest tenant client-queue drain after load stopped, seconds; ingest
    /// only.
    pub drain_seconds: Option<f64>,
}

/// One replica's resource use over the arrival window.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// Replica ordinal.
    pub replica: u16,
    /// Mean cores: cgroup CPU seconds over window seconds.
    pub cores: f64,
    /// Peak cgroup memory, bytes.
    pub peak_memory: u64,
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
    pub judge_calls: u64,
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
    pub async fn run(&self, plan: Plan) -> Result<Record> {
        let since = self.queue.now().await?;
        let mut before = Vec::new();
        for replica in &self.replicas {
            before.push(replica.metrics().await?);
        }
        let cpu_before: Vec<u64> = self
            .replicas
            .iter()
            .map(|replica| replica.cgroup_stat("cpu.stat", "usage_usec"))
            .collect();
        let mut peaks = Vec::new();
        for replica in &self.replicas {
            peaks.push(replica.memory_peak()?);
        }
        let judge_before = self.judge.calls();
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
        let stopped = self.queue.now().await?;
        let window_seconds = window.as_secs_f64();
        let cpu_after: Vec<u64> = self
            .replicas
            .iter()
            .map(|replica| replica.cgroup_stat("cpu.stat", "usage_usec"))
            .collect();
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
        let mut resources = Vec::new();
        for (index, (replica, peak)) in self.replicas.iter().zip(peaks).enumerate() {
            let cpu_seconds = cpu_after[index].saturating_sub(cpu_before[index]) as f64 / 1e6;
            resources.push(Resources {
                replica: replica.ordinal(),
                cores: cpu_seconds / window_seconds,
                peak_memory: peak.read()?,
            });
        }
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
            judge_calls: self.judge.calls() - judge_before,
            profiles,
        })
    }

    /// Waits from `stopped_at` until every backlog is empty or
    /// [`DRAIN_LIMIT`] passes, and returns the seconds it took (`None` when
    /// the limit passed), the last backlog read, and every replica's final
    /// scrape.
    ///
    /// # Errors
    ///
    /// Returns a scrape or query failure.
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
            let mut scrapes = Vec::new();
            for replica in &self.replicas {
                scrapes.push(replica.metrics().await?);
            }
            let backlog = Backlog {
                scribe: scribe_backlog(&scrapes),
                ..self.queue.backlog(since, stopped, activations).await?
            };
            let elapsed = stopped_at.elapsed();
            if backlog.is_empty() {
                return Ok((Some(elapsed.as_secs_f64()), backlog, scrapes));
            }
            if elapsed >= DRAIN_LIMIT {
                return Ok((None, backlog, scrapes));
            }
            tokio::time::sleep(POLL).await;
        }
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
            if runs.created > tally.accepted {
                *record.errors.entry("duplicate run".to_owned()).or_default() +=
                    runs.created - tally.accepted;
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
                    plan.name,
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
