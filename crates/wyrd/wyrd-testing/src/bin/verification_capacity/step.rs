//! One measured step against one deployment: drive its lanes for the step
//! window, drain, and collect client, exporter, database, resource, trace,
//! and optional profile evidence.
//!
//! Every step drains before the next starts, so no step's work lands in
//! another's evidence. Drain waits for every sent request, flushes the client
//! queues, then waits until every run created since the step start is
//! terminal and every accepted queued Eval observation has activated its
//! run. Work still outstanding one step length after the arrivals stop marks
//! the step unsustainable; work still outstanding at [`DRAIN_LIMIT`] is
//! reported as backlog and contaminates nothing only because the ladder for
//! that path stops there.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use wyrd_testing::capacity::{ResourceWindow, Resources, driver_cpu_seconds};
use wyrd_testing::release_server::LocalServer;

use crate::Result;
use crate::collector::Collector;
use crate::evidence::{Queue, RunTally, Scrapes, ServerStats};
use crate::fixture::{Kind, Tenant};
use crate::judge::Judge;
use crate::load::{Lane, MAX_IN_FLIGHT, Mode, Tally, TenantClients};
use crate::profile::{Capture, Profile};

/// Longest a step may take to drain before its backlog is reported.
const DRAIN_LIMIT: Duration = Duration::from_secs(300);

/// Width of a progress slice: a mixed step must complete every kind in each.
const SLICE: Duration = Duration::from_secs(5);

/// The deployment every step runs against.
pub struct Deployment {
    /// Serving replicas.
    pub replicas: Vec<LocalServer>,
    /// Every tenant, measured ones first.
    pub tenants: Vec<Tenant>,
    /// Each tenant's clients, connected to every replica.
    pub clients: Vec<Arc<TenantClients>>,
    /// The durable run queue.
    pub queue: Queue,
    /// The server's trace collector.
    pub collector: Arc<Collector>,
    /// The judge provider.
    pub judge: Arc<Judge>,
    /// Driver permits shared by every lane.
    pub permits: Arc<Semaphore>,
    /// Where profiles land when profiling, `None` otherwise.
    pub profiles: Option<PathBuf>,
    /// Binary identity every profile records.
    pub binary: serde_json::Value,
}

/// What a step measures.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// Path name: `queued`, `direct`, `combined`, `fairness`, or `warmup`.
    pub path: &'static str,
    /// Case name: a workload label or `mixed`.
    pub case: String,
    /// Offered executions per second per path, across every kind.
    pub rate: f64,
    /// Arrival window, seconds.
    pub seconds: f64,
    /// The lanes.
    #[serde(skip)]
    pub lanes: Vec<Lane>,
    /// Whether this step is captured when profiling.
    pub profiled: bool,
}

impl Plan {
    /// Stable name of the step, used for artifact directories.
    pub fn name(&self) -> String {
        format!("{}-{}-{}", self.path, self.case, self.rate)
    }
}

/// One lane's client tally plus its durable runs and exporter evidence.
#[derive(Debug, Clone, Serialize)]
pub struct LaneRecord {
    /// Tenant slug.
    pub tenant: String,
    /// Workload label.
    pub kind: &'static str,
    /// Path label.
    pub mode: &'static str,
    /// Offered arrivals per second.
    pub rate: f64,
    /// Client tally.
    #[serde(flatten)]
    pub tally: TallyRecord,
    /// Durable runs, queued lanes only.
    pub runs: Option<RunTally>,
    /// Raw queued creation-to-terminal latency percentiles, milliseconds.
    pub terminal_ms: crate::evidence::Percentiles,
    /// Exporter evidence for this kind and path, all tenants together.
    pub server: ServerStats,
    /// Sampled queued task-start delays, microseconds percentiles.
    pub task_start_us: crate::evidence::Percentiles,
    /// Executions completed per [`SLICE`] of the arrival window.
    pub progress: Vec<u64>,
}

/// The serializable part of a [`Tally`].
#[derive(Debug, Default, Clone, Serialize)]
pub struct TallyRecord {
    /// Arrivals scheduled.
    pub offered: u64,
    /// Arrivals without a driver permit.
    pub missed: u64,
    /// Requests sent.
    pub started: u64,
    /// Requests accepted.
    pub accepted: u64,
    /// Accepted requests built to fail.
    pub failing_accepted: u64,
    /// Refusals by code.
    pub rejected: BTreeMap<String, u64>,
    /// Direct verdicts.
    pub verdicts: BTreeMap<String, u64>,
    /// Direct verdicts that differ from the input's.
    pub wrong_verdicts: u64,
    /// Raw client latency percentiles, milliseconds.
    pub client_ms: crate::evidence::Percentiles,
    /// Raw client latency samples, microseconds.
    pub client_us: Vec<u64>,
}

impl From<&Tally> for TallyRecord {
    /// Keeps every count and the raw samples.
    fn from(tally: &Tally) -> Self {
        Self {
            offered: tally.offered,
            missed: tally.missed,
            started: tally.started,
            accepted: tally.accepted,
            failing_accepted: tally.failing_accepted,
            rejected: tally.rejected.clone(),
            verdicts: tally.verdicts.clone(),
            wrong_verdicts: tally.wrong_verdicts,
            client_ms: crate::evidence::Percentiles::raw(&tally.latency_us, 1e-3),
            client_us: tally.latency_us.clone(),
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
    /// Seconds from the window's end until drained, or the limit.
    pub drain_seconds: f64,
    /// Runs outstanding one step length after the arrivals stopped.
    pub outstanding_at_deadline: u64,
    /// Runs outstanding when the drain gave up; zero once drained.
    pub outstanding_final: u64,
    /// Lanes.
    pub lanes: Vec<LaneRecord>,
    /// Per-replica resources.
    pub resources: Vec<Resources>,
    /// Driver process CPU cores over the window.
    pub driver_cores: f64,
    /// Judge completions during the step.
    pub judge_calls: u64,
    /// Profiles, when captured.
    pub profiles: Vec<Profile>,
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
        let mut resources = ResourceWindow::open(&self.replicas)?;
        let driver_before = driver_cpu_seconds();
        let judge_before = self.judge.calls();
        let captures = self.start_captures(&plan)?;
        let unix_start = unix_nanos();

        let start = Instant::now() + Duration::from_millis(100);
        let window = Duration::from_secs_f64(plan.seconds);
        let mut running = Vec::new();
        for lane in plan.lanes.iter().copied() {
            let tenant = &self.tenants[lane.tenant];
            running.push(lane.drive(
                tenant,
                Arc::clone(&self.clients[lane.tenant]),
                Arc::clone(&self.permits),
                start,
                window,
            ));
        }
        let tallies = futures_util::future::try_join_all(running).await?;
        let window_seconds = start
            .elapsed()
            .as_secs_f64()
            .min(plan.seconds)
            .max(f64::EPSILON);
        resources.freeze(&self.replicas);
        let driver_cores = (driver_cpu_seconds() - driver_before) / window_seconds;
        let mut profiles = Vec::new();
        let metadata = serde_json::json!({ "step": plan, "binary": self.binary });
        for capture in captures {
            profiles.push(capture.finish(&metadata)?);
        }
        let unix_end = unix_nanos();

        let drained = Instant::now();
        let activations: u64 = plan
            .lanes
            .iter()
            .zip(&tallies)
            .filter(|(lane, _)| lane.mode == Mode::Queued)
            .map(|(_, tally)| tally.accepted)
            .sum();
        for clients in &self.clients {
            clients.flush().await?;
        }
        let deadline = drained + window;
        let mut outstanding_at_deadline = None;
        let outstanding_final = loop {
            let (outstanding, created) = self.queue.progress(since).await?;
            let missing = activations.saturating_sub(created);
            if outstanding + missing == 0 {
                break 0;
            }
            if outstanding_at_deadline.is_none() && Instant::now() >= deadline {
                outstanding_at_deadline = Some(outstanding + missing);
            }
            if drained.elapsed() >= DRAIN_LIMIT {
                break outstanding + missing;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let drain_seconds = drained.elapsed().as_secs_f64();

        let mut after = Vec::new();
        for replica in &self.replicas {
            after.push(replica.metrics().await?);
        }
        let scrapes = Scrapes { before, after };
        let runs = self.queue.runs(since).await?;
        let resources = resources.finish(&self.replicas, window_seconds)?;
        let lanes = plan
            .lanes
            .iter()
            .zip(&tallies)
            .map(|(lane, tally)| {
                self.lane_record(lane, tally, &runs, &scrapes, (unix_start, unix_end), window)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Record {
            replicas: self.replicas.len(),
            plan,
            window_seconds,
            drain_seconds,
            outstanding_at_deadline: outstanding_at_deadline.unwrap_or(0),
            outstanding_final,
            lanes,
            resources,
            driver_cores,
            judge_calls: self.judge.calls() - judge_before,
            profiles,
        })
    }

    /// Starts one capture per replica when profiling and `plan` is profiled.
    ///
    /// # Errors
    ///
    /// Returns a replica without a PID or a capture that cannot start.
    fn start_captures(&self, plan: &Plan) -> Result<Vec<Capture>> {
        let Some(root) = self.profiles.as_ref().filter(|_| plan.profiled) else {
            return Ok(Vec::new());
        };
        let mut captures = Vec::new();
        for replica in &self.replicas {
            let pid = replica.pid().ok_or("a profiled replica has exited")?;
            let directory = root
                .join(format!("{}-replicas-{}", plan.name(), self.replicas.len()))
                .join(format!("replica-{}", replica.ordinal()));
            captures.push(Capture::start(pid, replica.ordinal(), &directory)?);
        }
        Ok(captures)
    }

    /// Joins one lane's tally with its runs, exporter stats, and traces.
    ///
    /// # Errors
    ///
    /// Returns an error when the lane's tenant does not run its workload.
    fn lane_record(
        &self,
        lane: &Lane,
        tally: &Tally,
        runs: &BTreeMap<String, RunTally>,
        scrapes: &Scrapes,
        (unix_start, unix_end): (u64, u64),
        window: Duration,
    ) -> Result<LaneRecord> {
        let tenant = &self.tenants[lane.tenant];
        let (verifier, _) = tenant.target(lane.kind)?;
        let runs = (lane.mode == Mode::Queued)
            .then(|| runs.get(&verifier.to_string()).cloned().unwrap_or_default());
        let completions: Vec<Duration> = match &runs {
            Some(runs) => runs.settled_at.clone(),
            None => tally.completed_at.clone(),
        };
        let slices = (window.as_secs_f64() / SLICE.as_secs_f64()).floor() as usize;
        let mut progress = vec![0_u64; slices];
        for completed in completions {
            if let Some(slot) =
                progress.get_mut((completed.as_secs_f64() / SLICE.as_secs_f64()) as usize)
            {
                *slot += 1;
            }
        }
        Ok(LaneRecord {
            tenant: tenant.slug.clone(),
            kind: lane.kind.label(),
            mode: lane.mode.label(),
            rate: lane.rate,
            tally: TallyRecord::from(tally),
            terminal_ms: runs
                .as_ref()
                .map(|runs| crate::evidence::Percentiles::raw(&runs.latency_us, 1e-3))
                .unwrap_or_default(),
            runs,
            server: scrapes.stats(lane.kind, lane.mode),
            task_start_us: if lane.mode == Mode::Queued {
                crate::evidence::Percentiles::raw(
                    &self
                        .collector
                        .task_start_delays(lane.kind.label(), unix_start, unix_end),
                    1.0,
                )
            } else {
                crate::evidence::Percentiles::default()
            },
            progress,
        })
    }
}

/// Lanes of `kinds` for `tenant` on `mode`, each at `per_kind` arrivals per
/// second.
pub fn lanes(tenant: usize, kinds: &[Kind], mode: Mode, per_kind: f64) -> Vec<Lane> {
    kinds
        .iter()
        .map(|kind| Lane {
            tenant,
            kind: *kind,
            mode,
            rate: per_kind,
        })
        .collect()
}

/// Nanoseconds since the Unix epoch.
fn unix_nanos() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}

/// The shared driver permits.
pub fn permits() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(MAX_IN_FLIGHT))
}
