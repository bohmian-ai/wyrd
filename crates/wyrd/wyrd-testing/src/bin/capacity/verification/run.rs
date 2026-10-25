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
//! Judged times are the server's `verification.direct` benchmark
//! histograms: each execution's elapsed time and its residual after the
//! union of its own Verifier blocks, in the cohort of the phase that
//! admitted it, read once every request drained. Recovery's judged cohort
//! is admitted after its grace; the grace's arrivals join overload's. The
//! per-second recovery curve and the client round trips and remaining
//! histograms are diagnostic.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use tokio::task::JoinSet;
use tokio::time::Instant;
use wyrd_client::state::WyrdState;
use wyrd_client::transport::trace::with_benchmark_labels;
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_telemetry::timing::{Distribution, TimingSnapshot};
use wyrd_testing::capacity::{
    Expected, Histogram, HistogramKey, NANOS_PER_MS, Population, Readings, ResourceWindow, SMOKE,
    SMOKE_WINDOW, Selection, Smoke, Telemetry, Window, driver_cpu_seconds, elapsed, release_binary,
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

/// The benchmark case of a direct `verify`.
const DIRECT_CASE: &str = "direct-verify";

/// The benchmark case of a queued `submit`.
const QUEUED_CASE: &str = "queued-verify";

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

impl Op {
    /// The benchmark case the call declares.
    const fn case(self) -> &'static str {
        match self {
            Self::Verify => DIRECT_CASE,
            Self::Submit => QUEUED_CASE,
        }
    }
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
    /// Acknowledged calls that answered at or after the window end, outside
    /// the measured CPU; still in `latencies_us`.
    late: u64,
}

impl Measured {
    /// Records one call acknowledged after `latency_us` with a right or wrong
    /// verdict, `late` when it answered after the window end, or refused with
    /// its error.
    fn record(&mut self, latency_us: u64, outcome: Result<bool, WyrdError>, late: bool) {
        match outcome {
            Ok(right) => {
                self.latencies_us.push(latency_us);
                self.wrong += u64::from(!right);
                self.late += u64::from(late);
            }
            Err(error) => *self.errors.entry(error.code().to_owned()).or_default() += 1,
        }
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.wrong += other.wrong;
        self.late += other.late;
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

    /// Starts the release server through the guide and provisions the
    /// tenant's applications for the level.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a server or provisioning failure.
    async fn start(&mut self) -> Result<(), ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        let binary = release_binary().map_err(ExperimentError::workload)?;
        let server = LocalServer::start(&binary, &["bench"], &[])
            .await
            .map_err(ExperimentError::workload)?;
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
    /// `verify` for `window`, labelled with `phase`, judged by each
    /// acknowledged call's exact server timing. Work, seconds, and CPU stop
    /// at the window's end; calls then in flight drain into the cohort and
    /// count late.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an undeclared client count or a failed
    /// scrape.
    async fn closed(
        &self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
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
        let before = timings(server).await?;
        let (opened, _) = Window::open(server, SELECTION)
            .await
            .map_err(ExperimentError::workload)?;
        let stop = Instant::now() + window;
        let (measured, usage, _) = opened
            .measure(
                server,
                stop.into_std(),
                drive(&self.states[..clients], stop, phase.label()),
            )
            .await
            .map_err(ExperimentError::workload)?;
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            ..Repetition::default()
        };
        let telemetry = &usage.telemetry;
        let acknowledged = measured.latencies_us.len() as u64;
        let population = Population::collect(
            server,
            &before,
            &[Expected {
                boundary: DIRECT_TIMING,
                outcome: COMPLETED,
                case: DIRECT_CASE,
                phase: phase.label(),
                count: acknowledged,
            }],
        )
        .await;
        population.validate(&mut repetition, "measured");
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
        let verifier = population
            .excluded(DIRECT_TIMING, COMPLETED)
            .ok_or_else(|| "no completed direct execution has a Verifier component".to_owned());
        repetition.set(
            "overhead_mean_ms",
            mean_ms(population.residuals(DIRECT_TIMING, COMPLETED)),
        );
        repetition.set(
            "verifier_p99_ms",
            verifier.as_ref().map_or_else(Reading::missing, |verifier| {
                Reading::quantile(verifier, 0.99, NANOS_PER_MS)
            }),
        );
        repetition.set("verifier_mean_ms", mean_ms(verifier));
        repetition.set(
            "server_failures",
            Reading::count(population.others(DIRECT_TIMING, COMPLETED)),
        );
        set_client(&mut repetition, &measured);
        repetition.set(
            "verifications",
            Reading::count(acknowledged - measured.late),
        );
        repetition.set("verifications_after_window", Reading::count(measured.late));
        repetition.set("drain_seconds", Reading::exact(usage.drain_seconds));
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
        let admitted: Arc<[AtomicU64; 3]> = Arc::default();
        let sequence = Arc::new(AtomicU64::new(0));
        let before = timings(server).await?;
        let first = scrape(server).await?;
        let start = Instant::now();
        let driver_before = driver_cpu_seconds();
        let resources = ResourceWindow::open([server]);
        let backlog_start = backlog(&first);
        let send = |op| {
            arrivals(
                op,
                schedule,
                start,
                Arc::clone(&states),
                Arc::clone(&tallies),
                Arc::clone(&admitted),
                Arc::clone(&sequence),
            )
        };
        let (sent, observed) = tokio::join!(
            async { tokio::join!(send(Op::Verify), send(Op::Submit)) },
            observe(
                server,
                &schedule,
                start,
                (&first, &before),
                (resources, driver_before)
            ),
        );
        let (direct, background) = sent;
        let (observed, resources, driver) = observed?;
        let drain = drain(server, start + window).await?;
        let used = resources.finish([server], window.as_secs_f64());

        let tallies: [[Measured; 2]; 3] = std::array::from_fn(|stage| {
            std::array::from_fn(|slot| lock(&tallies[stage][slot]).clone())
        });
        let owed: Vec<Expected<'_>> = STAGES
            .iter()
            .zip(admitted.iter())
            .map(|(phase, count)| Expected {
                boundary: DIRECT_TIMING,
                outcome: COMPLETED,
                case: DIRECT_CASE,
                phase,
                count: count.load(Ordering::Relaxed),
            })
            .collect();
        let population = Population::collect(server, &before, &owed).await;
        let ended = Ended {
            snapshots: &observed.timings,
        };
        let mut repetition = Repetition {
            seconds: window.as_secs_f64(),
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: used.first().and_then(|used| used.cpu_seconds),
            peak_memory_bytes: used.first().and_then(|used| used.peak_memory),
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: driver,
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
        population.validate(&mut repetition, "measured");
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
        let [_, _, recovery_stage] = schedule.stages();
        for name in STAGES {
            repetition.set(
                &format!("{name}.direct_overhead_p99_ms"),
                overhead_p99(&population.only(DIRECT_CASE, name)),
            );
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
        population.only(DIRECT_CASE, STAGES[0]).percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
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
        let late: u64 = tallies.iter().flatten().map(|tally| tally.late).sum();
        let acknowledged: u64 = tallies
            .iter()
            .flatten()
            .map(|tally| tally.latencies_us.len() as u64)
            .sum();
        repetition.set("verifications", Reading::count(acknowledged - late));
        repetition.set("verifications_after_window", Reading::count(late));
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
    /// Starts the release server through the guide,
    /// provisions the tenant's applications (one per L1 client at the
    /// largest declared count, or [`APPLICATIONS`] for the L2 rotation),
    /// then sends sequential direct `verify` calls from one application
    /// for [`SMOKE_WINDOW`]. Each must answer with the expected verdict and
    /// land in the server's histograms as one completed `verification.direct`
    /// timing with its Verifier component and residual, beside positive
    /// server CPU and component observations.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a start or provisioning failure, a
    /// failed read, or the smoke's missing evidence. A started server is
    /// kept even when the smoke fails, so [`Workload::finish`] stops it;
    /// nothing is retried.
    async fn smoke(&mut self) -> Result<String, ExperimentError> {
        self.start().await?;
        let server = self.server()?;
        let state = self
            .states
            .first()
            .ok_or_else(|| ExperimentError::workload("no application was provisioned"))?;
        let mut smoke = Smoke::new("verification");
        let before = timings(server).await?;
        let (window, _) = Window::open(server, SELECTION)
            .await
            .map_err(ExperimentError::workload)?;
        let mut measured = Measured::default();
        let started = Instant::now();
        for sequence in 0.. {
            if started.elapsed() >= SMOKE_WINDOW {
                break;
            }
            let sent = Instant::now();
            let outcome =
                with_benchmark_labels(DIRECT_CASE, SMOKE, call(state, Op::Verify, sequence)).await;
            measured.record(micros(sent), outcome, false);
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
        let population = Population::smoke(
            server,
            &before,
            &[Expected {
                boundary: DIRECT_TIMING,
                outcome: COMPLETED,
                case: DIRECT_CASE,
                phase: SMOKE,
                count: measured.latencies_us.len() as u64,
            }],
        )
        .await;
        smoke.timings(&population, &[(DIRECT_TIMING, COMPLETED, true)]);
        smoke.finish().map_err(ExperimentError::Workload)
    }

    /// Nothing beyond what [`Self::smoke`] started and provisioned.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the smoke did not start the server.
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
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, or a failed scrape. A cancelled window aborts its client and
    /// arrival tasks with their requests; work the server already queued
    /// stays queued until [`Workload::finish`] stops it.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        match self.level {
            Level::One => self.closed(step, phase, window).await,
            Level::Two => Box::pin(self.competing(step, window)).await,
        }
    }

    /// Drops the application states, then stops the server and keeps its
    /// log beside the tenant's files. The stop runs synchronously inside
    /// this future: `SIGTERM`, a bounded wait for a clean exit, and the log
    /// copy.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the server exits unsuccessfully or
    /// outlives its stop grace (it is then killed), or its log cannot be
    /// copied. An unstarted workload finishes without error.
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
    /// The server's timing snapshot at each read, by its planned offset
    /// from the schedule's start, the start itself first.
    timings: Vec<(Duration, TimingSnapshot)>,
}

/// A window's completed direct executions, placed by when they ended: by
/// the timing snapshots taken at its phase boundaries and recovery reads.
struct Ended<'a> {
    /// Each snapshot by its offset from the schedule's start.
    snapshots: &'a [(Duration, TimingSnapshot)],
}

impl Ended<'_> {
    /// The executions recorded between the snapshots at `low` and `high`
    /// after the schedule's start.
    ///
    /// # Errors
    ///
    /// Returns why there is no such interval: no snapshot was taken at
    /// either offset, or the two cannot be differenced.
    fn between(&self, low: Duration, high: Duration) -> Result<Population, String> {
        let at = |offset: Duration| {
            self.snapshots
                .iter()
                .find(|(taken, _)| *taken == offset)
                .map(|(_, snapshot)| snapshot)
                .ok_or_else(|| format!("no timing snapshot was taken at {offset:?}"))
        };
        Ok(Population::from_interval(at(high)?.since(at(low)?)?))
    }

    /// The p99, milliseconds, of the residuals of the executions that ended
    /// between `low` and `high`; missing when one has no residual or the
    /// interval is invalid.
    fn overhead_p99(&self, low: Duration, high: Duration) -> Reading {
        match self.between(low, high) {
            Ok(population) => overhead_p99(&population),
            Err(reason) => Reading::missing(reason),
        }
    }
}

/// The p99, milliseconds, of `population`'s completed direct residuals;
/// missing when one has no residual, none completed, or the population is
/// invalid.
fn overhead_p99(population: &Population) -> Reading {
    if let Some(reason) = population.invalid.first() {
        return Reading::missing(reason.clone());
    }
    population
        .residuals(DIRECT_TIMING, COMPLETED)
        .map_or_else(Reading::missing, |residual| {
            Reading::quantile(&residual, 0.99, NANOS_PER_MS)
        })
}

/// Reads `/metrics` and the timing snapshot at each phase boundary of
/// `schedule` from `start`, and every [`POLL`] through recovery, keeping
/// each recovery read's backlog for the recovery judgment; `first` is the
/// pair read at the start. Freezes server CPU and returns the driver CPU
/// since `driver_before` at the window's end, before its closing scrape.
///
/// # Errors
///
/// Returns a workload error for a failed scrape.
async fn observe(
    server: &LocalServer,
    schedule: &Schedule,
    start: Instant,
    (first, opened_timings): (&Metrics, &TimingSnapshot),
    (mut resources, driver_before): (ResourceWindow, Option<f64>),
) -> Result<(Observed, ResourceWindow, Option<f64>), ExperimentError> {
    let [_, overload, recovery] = schedule.stages();
    let mut snapshots = vec![(Duration::ZERO, opened_timings.clone())];
    let whole = Readings::read(first, SELECTION);
    let opened = Readings::read(first, SELECTION);
    tokio::time::sleep_until(start + overload.start).await;
    let at_overload = scrape(server).await?;
    snapshots.push((overload.start, timings(server).await?));
    let capacity = opened.since(&at_overload);
    let opened = Readings::read(&at_overload, SELECTION);
    tokio::time::sleep_until(start + recovery.start).await;
    let at_recovery = scrape(server).await?;
    snapshots.push((recovery.start, timings(server).await?));
    let overloaded = opened.since(&at_recovery);
    let backlog_capacity = backlog(&at_overload);
    let bound = backlog_capacity + BACKLOG_LIMIT_SECONDS * schedule.offered();
    let opened = Readings::read(&at_recovery, SELECTION);
    let mut ticks = Vec::new();
    let mut offset = recovery.start;
    let mut driver = None;
    let last_metrics = loop {
        offset = (offset + POLL).min(schedule.window);
        tokio::time::sleep_until(start + offset).await;
        if offset >= schedule.window {
            resources.freeze([server]);
            driver = elapsed(driver_before, driver_cpu_seconds());
        }
        let metrics = scrape(server).await?;
        snapshots.push((offset, timings(server).await?));
        ticks.push((offset.saturating_sub(recovery.start), backlog(&metrics)));
        if offset >= schedule.window {
            break metrics;
        }
    };
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
        timings: snapshots,
    };
    Ok((observed, resources, driver))
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
/// rotating across `states`, each labelled with its admission phase,
/// recording each answer into its schedule phase's tally and marking one
/// after the window end late, counting each acknowledged direct call in
/// `admitted` by its admission phase, and waits for every sent arrival to
/// answer.
async fn arrivals(
    op: Op,
    schedule: Schedule,
    start: Instant,
    states: Arc<Vec<WyrdState>>,
    tallies: Arc<[[Mutex<Measured>; 2]; 3]>,
    admitted: Arc<[AtomicU64; 3]>,
    sequence: Arc<AtomicU64>,
) -> Offered {
    let mut offered = Offered::default();
    let mut tasks = JoinSet::new();
    let slot = usize::from(op == Op::Submit);
    let end = start + schedule.window;
    let recovery = schedule.stages()[2].start;
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
            let phase = admission(recovery, index, stage.start + offset);
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
            let (tallies, admitted) = (Arc::clone(&tallies), Arc::clone(&admitted));
            tasks.spawn(async move {
                let called = Instant::now();
                let outcome =
                    with_benchmark_labels(op.case(), STAGES[phase], call(&state, op, next)).await;
                let late = Instant::now() >= end;
                if op == Op::Verify && outcome.is_ok() {
                    admitted[phase].fetch_add(1, Ordering::Relaxed);
                }
                lock(&tallies[index][slot]).record(micros(called), outcome, late);
            });
            offered.sent += 1;
        }
    }
    while let Some(joined) = tasks.join_next().await {
        offered.panicked += u64::from(joined.is_err());
    }
    offered
}

/// The [`STAGES`] index of the admission phase of an arrival due `offset`
/// after the start in schedule phase `index`: recovery's arrivals within
/// [`RECOVERY_LIMIT`] of its start at `recovery` join overload's cohort.
fn admission(recovery: Duration, index: usize, offset: Duration) -> usize {
    if index == 2 && offset < recovery + RECOVERY_LIMIT {
        1
    } else {
        index
    }
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

/// Reads the server's cumulative benchmark timing snapshot.
///
/// # Errors
///
/// Returns the read failure as a workload error.
async fn timings(server: &LocalServer) -> Result<TimingSnapshot, ExperimentError> {
    server.timings().await.map_err(ExperimentError::workload)
}

/// Scrapes the server's `/metrics`.
///
/// # Errors
///
/// Returns the scrape failure as a workload error.
async fn scrape(server: &LocalServer) -> Result<Metrics, ExperimentError> {
    server.metrics().await.map_err(ExperimentError::workload)
}

/// Runs one closed-loop client per state making direct `verify` calls,
/// labelled `phase`, admitted until `stop`, client `i` presenting
/// application `i`'s key; a call answering at or after `stop` is late.
async fn drive(states: &[WyrdState], stop: Instant, phase: &'static str) -> Measured {
    let sequence = Arc::new(AtomicU64::new(0));
    let mut tasks = JoinSet::new();
    for state in states {
        let (state, sequence) = (state.clone(), Arc::clone(&sequence));
        tasks.spawn(async move {
            let mut measured = Measured::default();
            while Instant::now() < stop {
                let next = sequence.fetch_add(1, Ordering::Relaxed);
                let sent = Instant::now();
                let outcome =
                    with_benchmark_labels(DIRECT_CASE, phase, call(&state, Op::Verify, next)).await;
                measured.record(micros(sent), outcome, Instant::now() >= stop);
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

/// The mean, milliseconds, of a nanosecond `distribution` as the bounds of
/// its bucketed total, or why there is none.
fn mean_ms(distribution: Result<Distribution, String>) -> Reading {
    match distribution {
        Err(reason) => Reading::missing(reason),
        Ok(distribution) if distribution.is_empty() => {
            Reading::missing("no observation in the window")
        }
        Ok(distribution) => {
            let (low, high) = distribution.total_bounds();
            Reading::bounded(
                low as f64,
                high as f64,
                distribution.len() as f64 * NANOS_PER_MS,
            )
            .with_samples(distribution.len())
        }
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

    use std::time::Duration;

    use super::{ADMISSION_REFUSAL, Measured, admission};

    /// Arrivals keep their schedule phase's cohort except recovery's first
    /// 30 s, which join overload's, so recovery is judged after its grace
    /// however late each call completes.
    ///
    /// # Panics
    ///
    /// Panics when an arrival lands in the wrong cohort.
    #[test]
    fn recovery_grace_arrivals_join_overload() {
        let recovery = Duration::from_secs(100);
        let second = Duration::from_secs(1);
        assert_eq!(admission(recovery, 0, second), 0);
        assert_eq!(admission(recovery, 1, second * 99), 1);
        assert_eq!(admission(recovery, 2, recovery + second * 29), 1);
        assert_eq!(admission(recovery, 2, recovery + second * 30), 2);
    }

    /// Answers count acknowledged and refused calls, only undeclared
    /// refusals are unexpected, and an acknowledgement after the window end
    /// stays answered while it is counted late; merging keeps every count.
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
            late: 0,
        };
        assert_eq!(measured.answered(), 6);
        assert_eq!(measured.unexpected(Some(ADMISSION_REFUSAL)), 1);
        assert_eq!(measured.unexpected(None), 4);
        measured.merge(Measured {
            latencies_us: vec![300],
            errors: BTreeMap::new(),
            wrong: 2,
            late: 1,
        });
        measured.record(400, Ok(true), true);
        measured.record(50, Ok(true), false);
        assert_eq!(measured.wrong, 3);
        assert_eq!((measured.late, measured.answered()), (2, 9));
        assert_eq!(measured.percentile(0.5).value, Some(0.2));
        assert_eq!(measured.unexpected(None), 4);
    }
}
