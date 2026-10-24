//! The Verification workload under the shared experiment lifecycle: public
//! client `verify` and `submit` against one release server, and what each
//! repetition measured.
//!
//! An L1 repetition is closed loop: N clients each send the next direct
//! `verify` once the last answers, for the window. An L2 repetition is open
//! loop: direct and background arrivals follow the [`Schedule`] through
//! capacity, overload, and recovery while `/metrics` is read at each phase
//! boundary and every second of recovery, then the queue drains before the
//! next repetition, so each starts from the same empty backlog.
//!
//! Judged times are the server's exact `verification.direct` timings from
//! the run's [`Collector`]: each execution's elapsed time and its residual
//! after the union of its own Verifier blocks, assigned to a phase by when
//! it ended. Client round trips and the remaining histograms are
//! diagnostic.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use tokio::task::JoinSet;
use tokio::time::Instant;
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_testing::capacity::{
    Collector, Histogram, HistogramKey, NANOS_PER_MS, Population, Readings, ResourceWindow,
    SMOKE_WINDOW, Selection, Smoke, Telemetry, Timing, Window, driver_cpu_seconds, elapsed,
    release_binary, unix_nanos, usec_elapsed,
};
use wyrd_testing::experiment::{
    ExperimentError, Level, Phase, Reading, Repetition, Step, Workload,
};
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::verification::workload::{ASSERT_VERIFIER, Tenant, context, failing};
use crate::verification::{
    ADMISSION_REFUSAL, APPLICATIONS, BACKLOG_LIMIT_SECONDS, CLIENTS, OVERHEAD_LIMIT_MS,
    RECOVERY_LIMIT, STAGES, Schedule,
};

/// The declared server process whose CPU each repetition sums.
const PROCESS: &str = "wyrd-server";

/// How often recovery reads `/metrics` again.
const POLL: Duration = Duration::from_secs(1);

/// How often a drain reads `/metrics` again.
const DRAIN_POLL: Duration = Duration::from_millis(100);

/// How long a drain is watched before it is recorded as unfinished.
const DRAIN_WAIT: Duration = Duration::from_secs(90);

/// Most open-loop arrivals in flight; a scheduled arrival beyond it is never
/// sent and counts as missed, which invalidates the repetition.
const MAX_IN_FLIGHT: usize = 8_192;

/// The process outboxes a verification stages on: Scribe's and the Verifier
/// submission outbox.
const OUTBOXES: [&str; 2] = ["scribe", "verifier_submissions"];

/// The server timing boundary of one direct execution, from its start until
/// its result is ready, with the union of its Verifier blocks excluded.
const DIRECT_TIMING: &str = "verification.direct";

/// The outcome of a completed execution, whatever its verdict.
const COMPLETED: &str = "completed";

/// A `submit` request's arrival until it was staged.
const SUBMIT: &str = "wyrd_verification_submit_seconds";

/// A queued run's wait from claimable to claimed.
const QUEUE_WAIT: &str = "wyrd_verification_queue_wait_seconds";

/// Scribe's acknowledgement of staged evidence.
const SCRIBE_ACK: &str = "bifrost_scribe_ack_seconds";

/// The series filter of a staged submission.
const STAGED: &str = "outcome=\"staged\"";

/// The series filter of work that came from `submit`, which the server
/// records as a `manual` execution origin.
const SUBMITTED: &str = "origin=\"manual\"";

/// The server histograms an L2 repetition reports as diagnostic means.
const HISTOGRAMS: [HistogramKey; 3] = [
    (SUBMIT, &[STAGED]),
    (QUEUE_WAIT, &[SUBMITTED]),
    (SCRIBE_ACK, &[]),
];

/// The verification runtime's component histograms, each with the label that
/// names its components: execution phases, Verifier blocks, queue SQL, each
/// outbox's write, and the Scribe acknowledgement, WAL, and lane times.
const BREAKDOWN: [(&str, &str); 9] = [
    ("wyrd_verification_phase_duration_seconds", "phase"),
    ("wyrd_verification_block_duration_seconds", "kind"),
    ("wyrd_verification_store_duration_seconds", "op"),
    ("outbox_write_seconds", "outbox"),
    ("bifrost_scribe_queue_wait_seconds", ""),
    ("bifrost_scribe_wal_append_seconds", "outcome"),
    ("bifrost_scribe_wal_fsync_seconds", "outcome"),
    ("bifrost_scribe_lane_job_seconds", "lane"),
    ("wyrd_postgres_pool_acquire_seconds", "outcome"),
];

/// Everything a verification window reads from `/metrics`.
const SELECTION: Selection = Selection {
    histograms: &HISTOGRAMS,
    breakdown: &BREAKDOWN,
    prefix: "wyrd_verification_",
};

/// Component observations kept per L1 repetition, largest total first.
const COMPONENT_ROWS: usize = 15;

/// Which public call an arrival makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `observe.verify`: direct execution, judged before it answers.
    Verify,
    /// `observe.submit`: staged for one queued execution.
    Submit,
}

/// What a set of calls answered.
#[derive(Debug, Default, Clone)]
struct Measured {
    /// Latency of every acknowledged call, microseconds.
    latencies_us: Vec<u64>,
    /// Refused calls by stable code.
    errors: BTreeMap<String, u64>,
    /// Direct judgments whose verdict was not the expected one.
    wrong: u64,
}

impl Measured {
    /// Records one call acknowledged after `latency_us` with a right or wrong
    /// verdict, or refused with its error.
    fn record(&mut self, latency_us: u64, outcome: Result<bool, WyrdError>) {
        match outcome {
            Ok(right) => {
                self.latencies_us.push(latency_us);
                self.wrong += u64::from(!right);
            }
            Err(error) => *self.errors.entry(error.code().to_owned()).or_default() += 1,
        }
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.wrong += other.wrong;
        for (code, count) in other.errors {
            *self.errors.entry(code).or_default() += count;
        }
    }

    /// Calls that answered, acknowledged or refused.
    fn answered(&self) -> u64 {
        self.latencies_us.len() as u64 + self.errors.values().sum::<u64>()
    }

    /// Refusals other than `allowed`.
    fn unexpected(&self, allowed: Option<&str>) -> u64 {
        self.errors
            .iter()
            .filter(|(code, _)| Some(code.as_str()) != allowed)
            .map(|(_, count)| count)
            .sum()
    }

    /// Diagnostic client roundtrip percentile `q`, milliseconds.
    fn percentile(&self, q: f64) -> Reading {
        Reading::percentile(&self.latencies_us, q, 1e3)
    }
}

/// The Verification workload: one release server, one provisioned tenant,
/// and the level's arrivals.
pub struct Bench {
    /// The level, which picks closed- or open-loop work.
    level: Level,
    /// Where the tenant's bundles and the server log land.
    logs: PathBuf,
    /// The running server, from `prepare` until `finish`.
    server: Option<LocalServer>,
    /// One application per client or rotation slot.
    states: Vec<WyrdState>,
    /// The server timing collector, from `prepare`.
    collector: Option<Collector>,
}

impl Bench {
    /// A workload at `level` keeping its files under `logs`; nothing starts
    /// until [`Workload::prepare`].
    pub const fn new(level: Level, logs: PathBuf) -> Self {
        Self {
            level,
            logs,
            server: None,
            states: Vec::new(),
            collector: None,
        }
    }

    /// The running server.
    ///
    /// # Errors
    ///
    /// Returns a workload error before `prepare` or after `finish`.
    fn server(&self) -> Result<&LocalServer, ExperimentError> {
        self.server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the server is not running"))
    }

    /// The timing collector.
    ///
    /// # Errors
    ///
    /// Returns a workload error before `prepare`.
    fn collector(&self) -> Result<&Collector, ExperimentError> {
        self.collector
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the timing collector is not running"))
    }

    /// Starts the collector and the release server through the guide and
    /// provisions the tenant's applications for the level.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a collector, server, or provisioning
    /// failure.
    async fn start(&mut self) -> Result<(), ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        let binary = release_binary().map_err(ExperimentError::workload)?;
        let collector = Collector::start(&self.logs)
            .await
            .map_err(ExperimentError::workload)?;
        let tracing = collector.server_env();
        let variables: Vec<(&str, &str)> = tracing
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let server = LocalServer::start(&binary, &["bench"], &variables)
            .await
            .map_err(ExperimentError::workload)?;
        self.collector = Some(collector);
        let [setup] = server.tenants() else {
            return Err(ExperimentError::workload("setup provisioned one tenant"));
        };
        let applications = match self.level {
            Level::One => CLIENTS.iter().copied().max().unwrap_or(0),
            Level::Two => APPLICATIONS,
        };
        let applications = usize::try_from(applications).map_err(ExperimentError::workload)?;
        let tenant = Box::pin(Tenant::provision(
            setup,
            &self.logs.join("tenants"),
            applications,
        ))
        .await
        .map_err(ExperimentError::workload)?;
        self.states = tenant.states;
        self.server = Some(server);
        Ok(())
    }

    /// One L1 repetition: `step`'s clients each run closed-loop direct
    /// `verify` for `window`, judged by each acknowledged call's exact
    /// server timing.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an undeclared client count or a failed
    /// scrape.
    async fn closed(&self, step: &Step, window: Duration) -> Result<Repetition, ExperimentError> {
        let server = self.server()?;
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .filter(|clients| (1..=self.states.len()).contains(clients))
            .ok_or_else(|| {
                ExperimentError::workload(format!(
                    "step `{}` declares no client count within the {} provisioned applications",
                    step.id,
                    self.states.len()
                ))
            })?;
        let from = SystemTime::now();
        let (opened, _) = Window::open(server, self.collector()?, SELECTION)
            .await
            .map_err(ExperimentError::workload)?;
        let measured = drive(&self.states[..clients], window).await;
        let (usage, _) = opened
            .close(server)
            .await
            .map_err(ExperimentError::workload)?;
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            collector_cpu_seconds: usage.collector_cpu_seconds,
            ..Repetition::default()
        };
        let telemetry = &usage.telemetry;
        let acknowledged = measured.latencies_us.len() as u64;
        let population = self
            .collector()?
            .window(&step.id, from, &[(DIRECT_TIMING, acknowledged)])
            .await;
        population.validate(&mut repetition);
        population.percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        population.percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            true,
            &[("overhead_p99_ms", 0.99)],
        );
        let verifier = population.excluded(DIRECT_TIMING, COMPLETED);
        repetition.set(
            "overhead_mean_ms",
            mean_ms(population.residuals(DIRECT_TIMING, COMPLETED)),
        );
        repetition.set(
            "verifier_p99_ms",
            Reading::percentile(&verifier, 0.99, NANOS_PER_MS),
        );
        repetition.set("verifier_mean_ms", mean_ms(Ok(verifier)));
        repetition.set(
            "server_failures",
            Reading::count(population.others(DIRECT_TIMING, COMPLETED)),
        );
        set_client(&mut repetition, &measured);
        repetition.set("verifications", Reading::count(acknowledged));
        repetition.set("errors", Reading::count(measured.unexpected(None)));
        repetition.set("wrong", Reading::count(measured.wrong));
        repetition
            .observations
            .extend(components(telemetry, measured.latencies_us.len()));
        if !measured.errors.is_empty() {
            repetition
                .observations
                .push(format!("refusals by code: {:?}", measured.errors));
        }
        Ok(repetition)
    }

    /// One L2 repetition (or warmup): the [`Schedule`] of `step` over
    /// `window`, open loop, then the drain that returns the queue to empty
    /// and the window's direct server timings.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an undeclared rate or a failed scrape.
    async fn competing(
        &self,
        step: &Step,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let server = self.server()?;
        let schedule = Schedule::of(step, window).map_err(ExperimentError::workload)?;
        let states = Arc::new(self.states.clone());
        let tallies: Arc<[[Mutex<Measured>; 2]; 3]> = Arc::default();
        let sequence = Arc::new(AtomicU64::new(0));
        let from = SystemTime::now();
        let first = scrape(server).await?;
        let (start, start_unix) = (Instant::now(), unix_nanos(SystemTime::now()));
        let driver_before = driver_cpu_seconds();
        let collector_before = self.collector()?.cpu_usec();
        let resources = ResourceWindow::open([server]);
        let backlog_start = backlog(&first);
        let send = |op| {
            arrivals(
                op,
                schedule,
                start,
                Arc::clone(&states),
                Arc::clone(&tallies),
                Arc::clone(&sequence),
            )
        };
        let (sent, observed) = tokio::join!(
            async { tokio::join!(send(Op::Verify), send(Op::Submit)) },
            observe(server, &schedule, start, &first, resources),
        );
        let (direct, background) = sent;
        let (observed, resources) = observed?;
        let drain = drain(server, start + window).await?;
        let used = resources.finish([server], window.as_secs_f64());
        let driver = elapsed(driver_before, driver_cpu_seconds());
        let collector = usec_elapsed(collector_before, self.collector()?.cpu_usec());

        let tallies: [[Measured; 2]; 3] = std::array::from_fn(|stage| {
            std::array::from_fn(|slot| lock(&tallies[stage][slot]).clone())
        });
        let verified: u64 = tallies
            .iter()
            .map(|[verify, _]| verify.latencies_us.len() as u64)
            .sum();
        let population = self
            .collector()?
            .traced(&step.id, from, &[(DIRECT_TIMING, verified)], &[])
            .await;
        let ended = Ended {
            population: &population,
            start_unix,
        };
        let mut repetition = Repetition {
            seconds: window.as_secs_f64(),
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: used.first().and_then(|used| used.cpu_seconds),
            peak_memory_bytes: used.first().and_then(|used| used.peak_memory),
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: driver,
            collector_cpu_seconds: collector,
            ..Repetition::default()
        };
        let mut lateness = direct.lateness_us.clone();
        lateness.extend(&background.lateness_us);
        let answered: u64 = tallies.iter().flatten().map(Measured::answered).sum();
        repetition.arrivals = Some(wyrd_testing::experiment::Arrivals {
            scheduled: direct.scheduled + background.scheduled,
            sent: direct.sent + background.sent,
            completed: answered,
            missed: direct.missed + background.missed,
            lateness_p99_us: Reading::percentile(&lateness, 0.99, 1.0).value,
        });
        population.validate(&mut repetition);
        let panicked = direct.panicked + background.panicked;
        if panicked > 0 {
            repetition
                .invalid
                .push(format!("{panicked} arrivals panicked before answering"));
        }
        if backlog_start > 0.0 {
            repetition.invalid.push(format!(
                "the repetition started with {backlog_start} queued items, not the drained initial state"
            ));
        }
        let mut errors = 0;
        for ((name, [verify, submit]), telemetry) in
            STAGES.iter().zip(&tallies).zip(&observed.stages)
        {
            let allowed = (*name == "overload").then_some(ADMISSION_REFUSAL);
            let unexpected = verify.unexpected(allowed) + submit.unexpected(allowed);
            errors += unexpected;
            repetition.set(&format!("{name}.errors"), Reading::count(unexpected));
            repetition.set(&format!("{name}.wrong"), Reading::count(verify.wrong));
            repetition.set(&format!("{name}.direct_p99_ms"), verify.percentile(0.99));
            repetition.set(
                &format!("{name}.submit_mean_ms"),
                mean(&telemetry.histogram((SUBMIT, &[STAGED]))),
            );
            repetition.set(
                &format!("{name}.queue_wait_mean_ms"),
                mean(&telemetry.histogram((QUEUE_WAIT, &[SUBMITTED]))),
            );
            for tally in [verify, submit] {
                if !tally.errors.is_empty() {
                    repetition
                        .observations
                        .push(format!("{name} refusals by code: {:?}", tally.errors));
                }
            }
        }
        let [capacity_stage, overload_stage, recovery_stage] = schedule.stages();
        let settled = recovery_stage.start + RECOVERY_LIMIT;
        let recovery_end = recovery_stage.start + recovery_stage.length;
        let overhead = [
            ended.overhead_p99(
                capacity_stage.start,
                capacity_stage.start + capacity_stage.length,
            ),
            ended.overhead_p99(
                overload_stage.start,
                overload_stage.start + overload_stage.length,
            ),
            if settled < recovery_end {
                ended.overhead_p99(settled, recovery_end)
            } else {
                Reading::missing("the recovery phase is shorter than its 30 s grace")
            },
        ];
        for (name, overhead) in STAGES.iter().zip(overhead) {
            repetition.set(&format!("{name}.direct_overhead_p99_ms"), overhead);
        }
        let mut low = Duration::ZERO;
        let mut recovered = None;
        for (since, items) in &observed.ticks {
            let second =
                ended.overhead_p99(recovery_stage.start + low, recovery_stage.start + *since);
            if second.value.is_some_and(|p99| p99 < OVERHEAD_LIMIT_MS) && *items <= observed.bound {
                recovered = Some(since.as_secs_f64());
                break;
            }
            low = *since;
        }
        repetition.set(
            "recovery.seconds",
            recovered.map_or_else(
                || Reading::above(recovery_stage.length.as_secs_f64()),
                Reading::exact,
            ),
        );
        let capacity_elapsed: Vec<u64> = ended
            .between(
                capacity_stage.start,
                capacity_stage.start + capacity_stage.length,
            )
            .map(|timing| timing.elapsed_ns)
            .collect();
        for (id, q) in [("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)] {
            repetition.set(id, Reading::percentile(&capacity_elapsed, q, NANOS_PER_MS));
        }
        for (name, items) in STAGES.iter().zip(observed.backlog) {
            repetition.set(
                &format!("{name}.backlog_seconds"),
                schedule.backlog_seconds(items),
            );
        }
        let [capacity, overload, _] = &tallies;
        set_client(&mut repetition, &capacity[0]);
        repetition.set(
            "overload.admission_refusals",
            Reading::count(
                overload
                    .iter()
                    .map(|tally| tally.errors.get(ADMISSION_REFUSAL).copied().unwrap_or(0))
                    .sum(),
            ),
        );
        repetition.set("errors", Reading::count(errors));
        repetition.set(
            "verifications",
            Reading::count(
                tallies
                    .iter()
                    .flatten()
                    .map(|tally| tally.latencies_us.len() as u64)
                    .sum(),
            ),
        );
        repetition.set(
            "capacity.backlog_growth_seconds",
            schedule.backlog_seconds(observed.backlog[0] - backlog_start),
        );
        repetition.set(
            "recovery.backlog_growth_seconds",
            schedule.backlog_seconds(observed.backlog[2] - backlog_start),
        );
        repetition.set("drain_seconds", drain);
        let acks = observed.whole.histogram((SCRIBE_ACK, &[]));
        repetition.set("evidence_acks", Reading::exact(acks.count));
        repetition.set("evidence_ack_mean_ms", mean(&acks));
        Ok(repetition)
    }
}

#[async_trait]
impl Workload for Bench {
    /// Starts the collector and the release server through the guide,
    /// provisions the tenant's applications (one per L1 client at the
    /// largest declared count, or [`APPLICATIONS`] for the L2 rotation),
    /// then sends sequential direct `verify` calls from one application
    /// for [`SMOKE_WINDOW`]. Each must answer with the expected verdict and reach
    /// the collector as one completed `verification.direct` timing with its
    /// Verifier component, beside positive server CPU and component
    /// observations.
    async fn smoke(&mut self) -> Result<String, ExperimentError> {
        self.start().await?;
        let (server, collector) = (self.server()?, self.collector()?);
        let state = self
            .states
            .first()
            .ok_or_else(|| ExperimentError::workload("no application was provisioned"))?;
        let mut smoke = Smoke::new("verification");
        let from = SystemTime::now();
        let (window, _) = Window::open(server, self.collector()?, SELECTION)
            .await
            .map_err(ExperimentError::workload)?;
        let mut measured = Measured::default();
        let started = Instant::now();
        for sequence in 0.. {
            if started.elapsed() >= SMOKE_WINDOW {
                break;
            }
            let sent = Instant::now();
            let outcome = call(state, Op::Verify, sequence).await;
            measured.record(micros(sent), outcome);
        }
        let (usage, _) = window
            .close(server)
            .await
            .map_err(ExperimentError::workload)?;
        smoke.usage(&usage);
        let right = measured.latencies_us.len() as u64 - measured.wrong;
        smoke.positive(
            "direct verifications with the expected verdict",
            right as f64,
        );
        for (code, count) in &measured.errors {
            smoke.missing(format!("{count} direct calls were refused with `{code}`"));
        }
        smoke.positive(
            "verification runtime component observations",
            usage
                .telemetry
                .breakdown
                .values()
                .map(|(_, count)| count)
                .sum(),
        );
        let population = collector
            .smoke("smoke", from, &[(DIRECT_TIMING, measured.answered())])
            .await;
        smoke.timings(&population, &[(DIRECT_TIMING, COMPLETED, true)]);
        smoke.finish().map_err(ExperimentError::Workload)
    }

    /// Nothing beyond what [`Self::smoke`] started and provisioned.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        self.server().map(|_| ())
    }

    /// The server's PID, once it runs.
    fn processes(&self) -> Vec<u32> {
        self.server
            .as_ref()
            .and_then(LocalServer::pid)
            .into_iter()
            .collect()
    }

    /// Runs `step` for `window`: closed loop at L1, the whole open-loop
    /// sequence and its drain at L2. A warmup runs the same work.
    async fn measure(
        &mut self,
        step: &Step,
        _phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        match self.level {
            Level::One => self.closed(step, window).await,
            Level::Two => Box::pin(self.competing(step, window)).await,
        }
    }

    /// Stops the server and keeps its log beside the tenant's files.
    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.states.clear();
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        server
            .stop(&self.logs.join("server.log"))
            .map(|_| ())
            .map_err(ExperimentError::workload)
    }
}

/// What one L2 repetition read from `/metrics`.
struct Observed {
    /// Each phase's telemetry, in [`STAGES`] order.
    stages: [Telemetry; 3],
    /// Queued backlog items at each phase's end.
    backlog: [f64; 3],
    /// Each recovery read: its offset from recovery's start and the queued
    /// backlog items then.
    ticks: Vec<(Duration, f64)>,
    /// The recovered backlog bound: capacity's backlog plus
    /// [`BACKLOG_LIMIT_SECONDS`] of offered work.
    bound: f64,
    /// The whole window's telemetry.
    whole: Telemetry,
}

/// A window's completed direct executions, placed by when they ended.
struct Ended<'a> {
    /// The window's server timings.
    population: &'a Population,
    /// The schedule's start in Unix nanoseconds.
    start_unix: u64,
}

impl Ended<'_> {
    /// The completed direct executions that ended from `low` until before
    /// `high` after the schedule's start.
    fn between(&self, low: Duration, high: Duration) -> impl Iterator<Item = &Timing> {
        let nanos = |offset: Duration| u64::try_from(offset.as_nanos()).unwrap_or(u64::MAX);
        let (low, high) = (nanos(low), nanos(high));
        self.population.timings.iter().filter(move |timing| {
            let at = timing.end_unix_ns.checked_sub(self.start_unix);
            timing.boundary == DIRECT_TIMING
                && timing.outcome == COMPLETED
                && at.is_some_and(|at| at >= low && at < high)
        })
    }

    /// The p99, milliseconds, of the residuals of the executions that ended
    /// from `low` until before `high`; missing when one has no residual.
    fn overhead_p99(&self, low: Duration, high: Duration) -> Reading {
        match self
            .between(low, high)
            .map(Timing::residual_ns)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(residuals) => Reading::percentile(&residuals, 0.99, NANOS_PER_MS),
            Err(reason) => Reading::missing(reason),
        }
    }
}

/// Reads `/metrics` at each phase boundary of `schedule` from `start`, and
/// every [`POLL`] through recovery, keeping each recovery read's backlog for
/// the recovery judgment. Freezes server CPU at the window's end.
///
/// # Errors
///
/// Returns a workload error for a failed scrape.
async fn observe(
    server: &LocalServer,
    schedule: &Schedule,
    start: Instant,
    first: &Metrics,
    mut resources: ResourceWindow,
) -> Result<(Observed, ResourceWindow), ExperimentError> {
    let [_, overload, recovery] = schedule.stages();
    let whole = Readings::read(first, SELECTION);
    let opened = Readings::read(first, SELECTION);
    tokio::time::sleep_until(start + overload.start).await;
    let at_overload = scrape(server).await?;
    let capacity = opened.since(&at_overload);
    let opened = Readings::read(&at_overload, SELECTION);
    tokio::time::sleep_until(start + recovery.start).await;
    let at_recovery = scrape(server).await?;
    let overloaded = opened.since(&at_recovery);
    let backlog_capacity = backlog(&at_overload);
    let bound = backlog_capacity + BACKLOG_LIMIT_SECONDS * schedule.offered();
    let opened = Readings::read(&at_recovery, SELECTION);
    let end = start + schedule.window;
    let mut ticks = Vec::new();
    let mut tick = start + recovery.start;
    let last_metrics = loop {
        tick = (tick + POLL).min(end);
        tokio::time::sleep_until(tick).await;
        let metrics = scrape(server).await?;
        ticks.push((tick - (start + recovery.start), backlog(&metrics)));
        if tick >= end {
            break metrics;
        }
    };
    resources.freeze([server]);
    let recovering = opened.since(&last_metrics);
    let observed = Observed {
        backlog: [
            backlog_capacity,
            backlog(&at_recovery),
            backlog(&last_metrics),
        ],
        ticks,
        bound,
        stages: [capacity, overloaded, recovering],
        whole: whole.since(&last_metrics),
    };
    Ok((observed, resources))
}

/// What one open-loop sender scheduled and sent.
#[derive(Debug, Default)]
struct Offered {
    /// Arrivals the schedule planned.
    scheduled: u64,
    /// Arrivals sent.
    sent: u64,
    /// Planned arrivals never sent because [`MAX_IN_FLIGHT`] were pending.
    missed: u64,
    /// Arrivals that panicked before answering.
    panicked: u64,
    /// Each sent arrival's lateness behind its schedule, microseconds.
    lateness_us: Vec<u64>,
}

/// Sends `op` arrivals at each phase's rate of `schedule` from `start`,
/// rotating across `states`, recording each answer into its phase's tally,
/// and waits for every sent arrival to answer.
async fn arrivals(
    op: Op,
    schedule: Schedule,
    start: Instant,
    states: Arc<Vec<WyrdState>>,
    tallies: Arc<[[Mutex<Measured>; 2]; 3]>,
    sequence: Arc<AtomicU64>,
) -> Offered {
    let mut offered = Offered::default();
    let mut tasks = JoinSet::new();
    let slot = usize::from(op == Op::Submit);
    for (index, stage) in schedule.stages().into_iter().enumerate() {
        let rate = match op {
            Op::Verify => stage.direct_rate,
            Op::Submit => stage.submit_rate,
        };
        for arrival in 0u32.. {
            let offset = Duration::from_secs_f64(f64::from(arrival) / rate);
            if offset >= stage.length {
                break;
            }
            let at = start + stage.start + offset;
            tokio::time::sleep_until(at).await;
            offered.scheduled += 1;
            while let Some(joined) = tasks.try_join_next() {
                offered.panicked += u64::from(joined.is_err());
            }
            if tasks.len() >= MAX_IN_FLIGHT || states.is_empty() {
                offered.missed += 1;
                continue;
            }
            offered.lateness_us.push(micros(at));
            let next = sequence.fetch_add(1, Ordering::Relaxed);
            let state = states[usize::try_from(next).unwrap_or(0) % states.len()].clone();
            let tallies = Arc::clone(&tallies);
            tasks.spawn(async move {
                let called = Instant::now();
                let outcome = call(&state, op, next).await;
                lock(&tallies[index][slot]).record(micros(called), outcome);
            });
            offered.sent += 1;
        }
    }
    while let Some(joined) = tasks.join_next().await {
        offered.panicked += u64::from(joined.is_err());
    }
    offered
}

/// Waits from `stopped`, when load stopped, until no queued execution is
/// pending, running, or retrying and both [`OUTBOXES`] are empty, and
/// returns the seconds it took, or a reading above [`DRAIN_WAIT`] when it did
/// not drain by then.
///
/// # Errors
///
/// Returns a workload error for a failed scrape.
async fn drain(server: &LocalServer, stopped: Instant) -> Result<Reading, ExperimentError> {
    loop {
        let metrics = scrape(server).await?;
        let waited = stopped.elapsed();
        if backlog(&metrics) <= 0.0 {
            return Ok(Reading::exact(waited.as_secs_f64()));
        }
        if waited >= DRAIN_WAIT {
            return Ok(Reading::above(DRAIN_WAIT.as_secs_f64()));
        }
        tokio::time::sleep(DRAIN_POLL).await;
    }
}

/// Queued verification items: the queue depth plus both [`OUTBOXES`].
fn backlog(metrics: &Metrics) -> f64 {
    metrics.sum("wyrd_verification_queue_depth", &[])
        + OUTBOXES
            .iter()
            .map(|outbox| metrics.sum("outbox_pending", &[&format!("outbox=\"{outbox}\"")]))
            .sum::<f64>()
}

/// Scrapes the server's `/metrics`.
///
/// # Errors
///
/// Returns the scrape failure as a workload error.
async fn scrape(server: &LocalServer) -> Result<Metrics, ExperimentError> {
    server.metrics().await.map_err(ExperimentError::workload)
}

/// Runs one closed-loop client per state making direct `verify` calls for
/// `window`, client `i` presenting application `i`'s key.
async fn drive(states: &[WyrdState], window: Duration) -> Measured {
    let deadline = Instant::now() + window;
    let sequence = Arc::new(AtomicU64::new(0));
    let mut tasks = JoinSet::new();
    for state in states {
        let (state, sequence) = (state.clone(), Arc::clone(&sequence));
        tasks.spawn(async move {
            let mut measured = Measured::default();
            while Instant::now() < deadline {
                let next = sequence.fetch_add(1, Ordering::Relaxed);
                let sent = Instant::now();
                let outcome = call(&state, Op::Verify, next).await;
                measured.record(micros(sent), outcome);
            }
            measured
        });
    }
    let mut total = Measured::default();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(measured) => total.merge(measured),
            Err(_) => *total.errors.entry("panicked".to_owned()).or_default() += 1,
        }
    }
    total
}

/// Makes arrival `sequence` as `op` through `state`, the Service's
/// application lifetime.
///
/// # Errors
///
/// Returns the client's or server's refusal; a direct verdict other than the
/// expected one is `Ok(false)`, not an error.
async fn call(state: &WyrdState, op: Op, sequence: u64) -> Result<bool, WyrdError> {
    let run = state.run_for_card("model")?;
    let observe = run.observe();
    match op {
        Op::Submit => observe
            .submit(ASSERT_VERIFIER, context(sequence))
            .await
            .map(|_| true),
        Op::Verify => {
            let judged = observe.verify(ASSERT_VERIFIER, context(sequence)).await?;
            let expected = if failing(sequence) {
                VerificationVerdict::Failed
            } else {
                VerificationVerdict::Passed
            };
            Ok(judged.outcome.verdict == expected)
        }
    }
}

/// Sets the diagnostic client percentiles from `measured`.
fn set_client(repetition: &mut Repetition, measured: &Measured) {
    for (id, q) in [
        ("client_p50_ms", 0.5),
        ("client_p95_ms", 0.95),
        ("client_p99_ms", 0.99),
    ] {
        repetition.set(id, measured.percentile(q));
    }
}

/// The exact mean, milliseconds, of `samples` nanoseconds, or why there is
/// none.
fn mean_ms(samples: Result<Vec<u64>, String>) -> Reading {
    match samples {
        Err(reason) => Reading::missing(reason),
        Ok(samples) if samples.is_empty() => Reading::missing("no observation in the window"),
        Ok(samples) => Reading::exact(
            samples.iter().map(|sample| *sample as f64).sum::<f64>()
                / samples.len() as f64
                / NANOS_PER_MS,
        )
        .with_samples(samples.len() as u64),
    }
}

/// A server histogram's exact mean, milliseconds.
fn mean(histogram: &Histogram) -> Reading {
    histogram.mean().map_or_else(
        || Reading::missing("no observation in the window"),
        |seconds| Reading::exact(seconds * 1e3),
    )
}

/// The runtime's component times over a window, largest total first: each
/// component's milliseconds per acknowledged call beside its mean per
/// observation. Components nest and background work overlaps calls, so they
/// are read beside the client latency, never summed.
fn components(telemetry: &Telemetry, acknowledged: usize) -> Vec<String> {
    let calls = acknowledged.max(1) as f64;
    let mut components: Vec<(&String, &(f64, f64))> = telemetry.breakdown.iter().collect();
    components.sort_by(|left, right| right.1.0.total_cmp(&left.1.0));
    components
        .into_iter()
        .take(COMPONENT_ROWS)
        .map(|(component, (seconds, count))| {
            format!(
                "component {component}: {:.4} ms/call, {:.4} ms mean over {count:.0}",
                seconds * 1e3 / calls,
                seconds * 1e3 / count.max(f64::EPSILON)
            )
        })
        .collect()
}

/// Locks a tally, recovering one a panicking arrival poisoned.
fn lock(tally: &Mutex<Measured>) -> std::sync::MutexGuard<'_, Measured> {
    tally.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Microseconds since `since`.
fn micros(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{ADMISSION_REFUSAL, Measured};

    /// Answers count acknowledged and refused calls, and only undeclared
    /// refusals are unexpected.
    ///
    /// # Panics
    ///
    /// Panics when a tally miscounts.
    #[test]
    fn tallies_separate_declared_admission_refusals() {
        let mut measured = Measured {
            latencies_us: vec![100, 200],
            errors: BTreeMap::from([
                (ADMISSION_REFUSAL.to_owned(), 3),
                ("WYRD_OTHER".to_owned(), 1),
            ]),
            wrong: 1,
        };
        assert_eq!(measured.answered(), 6);
        assert_eq!(measured.unexpected(Some(ADMISSION_REFUSAL)), 1);
        assert_eq!(measured.unexpected(None), 4);
        measured.merge(Measured {
            latencies_us: vec![300],
            errors: BTreeMap::new(),
            wrong: 2,
        });
        assert_eq!(measured.wrong, 3);
        assert_eq!(measured.percentile(0.5).value, Some(0.2));
    }
}
