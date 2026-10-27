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
//! is admitted after its grace (at most half of recovery); the grace's
//! arrivals join overload's. The
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
    Expected, Histogram, HistogramKey, NANOS_PER_MS, Population, Readings, ResourceWindow,
    Selection, Telemetry, Window, driver_cpu_seconds, elapsed, release_binary,
};
use wyrd_testing::experiment::{
    ExperimentError, Level, Phase, Reading, Repetition, Step, Workload,
};
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::verification::workload::{ASSERT_VERIFIER, Tenant, context, failing};
use crate::verification::{
    ADMISSION_REFUSAL, APPLICATIONS, BACKLOG_LIMIT_SECONDS, CLIENTS, OVERHEAD_LIMIT_MS,
    OVERHEAD_TAILS, PHASE_TIMINGS, RECOVERY_LIMIT, STAGES, Schedule,
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
        let first = scrape(server).await?;
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
        let last = scrape(server).await?;
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
        set_tails(&mut repetition, &population);
        set_phases(&mut repetition, &population);
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
        repetition.set("lost_writes", lost_writes(&first, &last));
        if !observed_components(telemetry) {
            repetition.invalid.push(
                "the verification runtime recorded no component observation in the window"
                    .to_owned(),
            );
        }
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
            let states = Arc::clone(&states);
            let launch = move |next: u64| {
                let state = states
                    .get(usize::try_from(next).unwrap_or(0) % states.len().max(1))?
                    .clone();
                Some(async move { call(&state, op, next).await })
            };
            arrivals(
                op,
                schedule,
                start,
                launch,
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
        if !observed_components(&observed.whole) {
            repetition.invalid.push(
                "the verification runtime recorded no component observation in the window"
                    .to_owned(),
            );
        }
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
    /// Starts the release server through the guide and provisions the
    /// tenant's applications: one per L1 client at the largest declared
    /// count, or [`APPLICATIONS`] for the L2 rotation.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a start or provisioning failure. A
    /// started server is kept even when provisioning fails, so
    /// [`Workload::finish`] stops it; nothing is retried.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        self.start().await
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
    /// log beside the tenant's files. The stop runs inside
    /// this future: `SIGTERM`, a bounded wait that yields while the server
    /// drains, and the log copy.
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
            .await
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
/// each the call `launch` builds for its sequence number, labelled with its
/// admission phase, recording each answer into its schedule phase's tally
/// and marking one after the window end late, counting each acknowledged
/// direct call in `admitted` by its admission phase, and waits for every
/// sent arrival to answer. An arrival whose wake comes at or after the
/// window end, or for which `launch` has no state to call, is missed:
/// nothing is admitted once measurement stopped.
async fn arrivals<Fut>(
    op: Op,
    schedule: Schedule,
    start: Instant,
    launch: impl Fn(u64) -> Option<Fut>,
    tallies: Arc<[[Mutex<Measured>; 2]; 3]>,
    admitted: Arc<[AtomicU64; 3]>,
    sequence: Arc<AtomicU64>,
) -> Offered
where
    Fut: Future<Output = Result<bool, WyrdError>> + Send + 'static,
{
    let mut offered = Offered::default();
    let mut tasks = JoinSet::new();
    let slot = usize::from(op == Op::Submit);
    let end = start + schedule.window;
    let [_, _, recovery] = schedule.stages();
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
            let phase = admission(recovery.start, recovery.length, index, stage.start + offset);
            tokio::time::sleep_until(at).await;
            offered.scheduled += 1;
            while let Some(joined) = tasks.try_join_next() {
                offered.panicked += u64::from(joined.is_err());
            }
            if tasks.len() >= MAX_IN_FLIGHT || Instant::now() >= end {
                offered.missed += 1;
                continue;
            }
            let next = sequence.fetch_add(1, Ordering::Relaxed);
            let Some(call) = launch(next) else {
                offered.missed += 1;
                continue;
            };
            offered.lateness_us.push(micros(at));
            let (tallies, admitted) = (Arc::clone(&tallies), Arc::clone(&admitted));
            tasks.spawn(async move {
                let called = Instant::now();
                let outcome = with_benchmark_labels(op.case(), STAGES[phase], call).await;
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
/// after the start in schedule phase `index`: recovery's arrivals within its
/// grace of its start at `recovery`, [`RECOVERY_LIMIT`] or half its `length`
/// when shorter, join overload's cohort, so a short smoke recovery still
/// judges a cohort.
fn admission(recovery: Duration, length: Duration, index: usize, offset: Duration) -> usize {
    if index == 2 && offset < recovery + RECOVERY_LIMIT.min(length / 2) {
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
/// Scribe outbox writes lost for any reason between two scrapes, shed audit
/// decisions included; missing when a counter went backwards.
fn lost_writes(first: &Metrics, last: &Metrics) -> Reading {
    let labels = ["outbox=\"scribe\""];
    let lost = last.sum("outbox_events_lost_total", &labels)
        - first.sum("outbox_events_lost_total", &labels);
    if lost.is_finite() && lost >= 0.0 {
        Reading::exact(lost)
    } else {
        Reading::missing("the Scribe outbox loss counter went backwards")
    }
}

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

/// Sets each [`OVERHEAD_TAILS`] count of completed direct executions whose
/// residual exceeds its threshold, as the bounds the histogram allows; each
/// is missing when there is no residual.
fn set_tails(repetition: &mut Repetition, population: &Population) {
    let residuals = population.residuals(DIRECT_TIMING, COMPLETED);
    for (id, nanos) in OVERHEAD_TAILS {
        repetition.set(
            id,
            residuals
                .as_ref()
                .map_or_else(Reading::missing, |residuals| {
                    let (certain, possible) = residuals.count_above(nanos);
                    Reading::bounded(certain as f64, possible as f64, 1.0)
                }),
        );
    }
}

/// Sets each [`PHASE_TIMINGS`] phase's p99 and maximum, milliseconds, over
/// the completed direct executions. A phase the window did not time is
/// missing with its reason; phases are diagnostic, so that never makes the
/// repetition invalid.
fn set_phases(repetition: &mut Repetition, population: &Population) {
    for (phase, boundary) in PHASE_TIMINGS {
        let distribution = population.elapsed(boundary, COMPLETED);
        for (statistic, q) in [("p99", 0.99), ("max", 1.0)] {
            repetition.set(
                &format!("phase_{phase}_{statistic}_ms"),
                distribution.as_ref().map_or_else(
                    || Reading::missing(format!("no completed `{boundary}` time in the window")),
                    |distribution| Reading::quantile(distribution, q, NANOS_PER_MS),
                ),
            );
        }
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

/// Whether `telemetry` holds any verification runtime component
/// observation, as every executed `verify` records its phases and Verifier
/// blocks.
fn observed_components(telemetry: &Telemetry) -> bool {
    telemetry.breakdown.values().any(|(_, count)| *count > 0.0)
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

    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{ADMISSION_REFUSAL, Measured, Op, admission, arrivals};
    use crate::verification::Schedule;

    /// Once a stalled call carries the clock past the window end, every
    /// remaining due arrival is missed rather than admitted after the
    /// measurement stopped, the planned count is kept, and the earlier
    /// admission still answers into its tally.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the end or one is lost.
    #[tokio::test(start_paused = true)]
    async fn arrivals_waking_after_the_end_are_missed() {
        let schedule = Schedule {
            window: Duration::from_secs(12),
            direct_rate: 1.0,
            submit_rate: 1.0,
            overload_factor: 2.0,
        };
        let tallies: Arc<[[std::sync::Mutex<Measured>; 2]; 3]> = Arc::default();
        let admitted: Arc<[AtomicU64; 3]> = Arc::default();
        let offered = arrivals(
            Op::Verify,
            schedule,
            tokio::time::Instant::now(),
            |next| {
                Some(async move {
                    if next == 0 {
                        tokio::time::advance(Duration::from_secs(13)).await;
                    }
                    Ok(true)
                })
            },
            Arc::clone(&tallies),
            Arc::clone(&admitted),
            Arc::new(AtomicU64::new(0)),
        )
        .await;
        assert_eq!((offered.scheduled, offered.sent), (14, 1));
        assert_eq!(offered.missed, 13);
        let answered: u64 = admitted
            .iter()
            .map(|count| count.load(Ordering::SeqCst))
            .sum();
        assert_eq!(answered, 1);
    }

    /// Dropping the scheduler mid-window, as a budget overrun or an
    /// interrupt drops a repetition, admits nothing more and aborts every
    /// call it sent, so no tally changes after the stop.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the stop or a call outlives
    /// it.
    #[tokio::test(start_paused = true)]
    async fn cancelled_arrivals_admit_nothing_and_abort_their_calls() {
        let schedule = Schedule {
            window: Duration::from_secs(12),
            direct_rate: 1.0,
            submit_rate: 1.0,
            overload_factor: 2.0,
        };
        let sequence = Arc::new(AtomicU64::new(0));
        let live = Arc::new(());
        let driving = arrivals(
            Op::Verify,
            schedule,
            tokio::time::Instant::now(),
            |_| {
                let held = Arc::clone(&live);
                Some(async move {
                    std::future::pending::<()>().await;
                    drop(held);
                    Ok(true)
                })
            },
            Arc::default(),
            Arc::default(),
            Arc::clone(&sequence),
        );
        let cut = tokio::time::timeout(Duration::from_millis(2_500), driving).await;
        assert!(cut.is_err(), "the schedule outlasts the cut");
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(
            sequence.load(Ordering::SeqCst),
            3,
            "arrivals at 0, 1, and 2 s only"
        );
        assert_eq!(Arc::strong_count(&live), 1, "no call outlives the stop");
    }

    /// Arrivals keep their schedule phase's cohort except recovery's first
    /// 30 s (half of a recovery shorter than 60 s), which join overload's,
    /// so recovery is judged after its grace however late each call
    /// completes.
    ///
    /// # Panics
    ///
    /// Panics when an arrival lands in the wrong cohort.
    #[test]
    fn recovery_grace_arrivals_join_overload() {
        let recovery = Duration::from_secs(100);
        let second = Duration::from_secs(1);
        let length = second * 100;
        assert_eq!(admission(recovery, length, 0, second), 0);
        assert_eq!(admission(recovery, length, 1, second * 99), 1);
        assert_eq!(admission(recovery, length, 2, recovery + second * 29), 1);
        assert_eq!(admission(recovery, length, 2, recovery + second * 30), 2);
        let short = second * 15;
        assert_eq!(admission(recovery, short, 2, recovery + second * 7), 1);
        assert_eq!(admission(recovery, short, 2, recovery + second * 8), 2);
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

    /// The measurement path calibrated against independent references on a
    /// real release server: the shared runner, the actual [`Window`] and
    /// resource readers, the server's timing readout and the persisted record,
    /// compared with the same workload with benchmark collection turned off.
    mod calibration {
        use std::time::Duration;

        use async_trait::async_trait;
        use tokio::time::Instant;
        use tokio_util::sync::CancellationToken;
        use wyrd_testing::capacity::{Window, release_binary};
        use wyrd_testing::experiment::{
            ExperimentError, Level, Options, Phase, Record, Repetition, Step, Timing, Verdict,
            Workload, run,
        };
        use wyrd_testing::release_server::HostLoad;

        use crate::tests::pg_tests::burn;
        use crate::verification::declare;
        use crate::verification::run::{Bench, SELECTION, drive};
        use crate::verification::tests::base;

        /// The step calibrated: one closed-loop client.
        const STEP: &str = "direct-verify-c1";

        /// Each measured window, the known delay the record's seconds are read
        /// against.
        const WINDOW: Duration = Duration::from_secs(5);

        /// The window's stop fires on a millisecond timer before the stop
        /// scrape starts; 50 ms covers that and scheduling on a quiet host.
        const WINDOW_SLACK_SECONDS: f64 = 0.05;

        /// The known CPU work the second repetition's driver adds, on its own
        /// thread, inside the window.
        const SPIN: Duration = Duration::from_secs(1);

        /// A thread spinning on a quiet host is charged at least this share of
        /// its wall time; the spin starts a few milliseconds before the window
        /// opens.
        const SPIN_FLOOR: f64 = 0.8;

        /// Two 10 ms CPU accounting ticks, one at each end of a reading.
        const TICKS_SECONDS: f64 = 0.02;

        /// How much the driver's own request CPU may differ between two equal
        /// windows, as a share of it.
        const DRIVER_DRIFT: f64 = 0.25;

        /// Allowed observation cost per verification, as a share of the
        /// uncollected control: recording adds one label read and two histogram
        /// records to a request costing hundreds of microseconds, and 10% covers
        /// the difference between two sequential windows on a quiet host.
        const COST_SHARE: f64 = 0.10;

        /// Allowed observation cost per verification below which a share is
        /// meaningless, milliseconds.
        const COST_FLOOR_MS: f64 = 0.01;

        /// The real L1 workload, with [`SPIN`] of driver CPU burned beside the
        /// second repetition. It starts the server inside the runner's clock
        /// and leaves it running, so the control reuses its tenant.
        struct Calibration {
            /// The Verification L1 workload.
            bench: Bench,
        }

        #[async_trait]
        impl Workload for Calibration {
            async fn prepare(&mut self) -> Result<(), ExperimentError> {
                self.bench.start().await
            }

            fn processes(&self) -> Vec<u32> {
                self.bench.processes()
            }

            async fn measure(
                &mut self,
                step: &Step,
                phase: Phase,
                window: Duration,
            ) -> Result<Repetition, ExperimentError> {
                let spin = (phase == Phase::Repetition(1))
                    .then(|| tokio::task::spawn_blocking(|| burn(SPIN)));
                let repetition = self.bench.measure(step, phase, window).await;
                if let Some(spin) = spin {
                    spin.await.map_err(ExperimentError::workload)?;
                }
                repetition
            }

            async fn finish(&mut self) -> Result<(), ExperimentError> {
                Ok(())
            }
        }

        /// Whether `observed` lies in its reference `[low, high]`, naming
        /// `what` when it does not.
        fn within(what: &str, observed: f64, low: f64, high: f64) -> Result<(), String> {
            if (low..=high).contains(&observed) {
                Ok(())
            } else {
                Err(format!(
                    "{what} read {observed}, outside its reference [{low}, {high}]"
                ))
            }
        }

        /// One verification's wall and server CPU milliseconds over a window.
        fn per_verification(seconds: f64, cpu_seconds: f64, verifications: f64) -> [f64; 2] {
            [seconds, cpu_seconds].map(|total| total * 1e3 / verifications)
        }

        /// The release server's direct `verify` through the shared runner keeps
        /// what independent references say: each window's recorded seconds
        /// match the declared window, the driver CPU of a repetition with a
        /// known spin exceeds the plain one's by that spin, the server's p50
        /// lies under the client's own round trip and its count reconciles,
        /// and the record and its timing evidence read back. The same workload
        /// with collection off then measures what observing costs per
        /// verification in wall and server CPU, which stays within its
        /// allowance; each check rejects a materially wrong reading.
        ///
        /// Needs `mise run bench:build` and the repository Postgres wrapper on
        /// a quiet host:
        /// `RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references)'`.
        ///
        /// # Panics
        ///
        /// Panics when a reading falls outside its reference, the observation
        /// cost exceeds its allowance, or a check accepts a wrong reading.
        #[tokio::test]
        #[ignore = "needs the release server from `mise run bench:build`, the Postgres wrapper and a quiet host"]
        async fn the_measurement_path_agrees_with_independent_references() {
            let directory = tempfile::tempdir().expect("directory");
            let mut declaration = declare(Level::One, &[], base(Level::One)).expect("L1 declares");
            declaration.selection = Some(vec![STEP.to_owned()]);
            declaration.timing = Timing {
                warmup_seconds: 1,
                repetition_seconds: WINDOW.as_secs(),
                repetitions: 2,
                budget_seconds: 600,
                minimum_seconds: 0,
            };
            let options = Options {
                artifacts: directory.path().join("artifacts"),
                profiler: None,
                interrupt: CancellationToken::new(),
            };
            let mut workload = Calibration {
                bench: Bench::new(Level::One, directory.path().join("logs")),
            };
            let record = run(
                declaration.seal(),
                &mut workload,
                HostLoad::sample,
                &options,
            )
            .await;
            let path = record.publish(directory.path()).expect("publish");
            let read = Record::read(&path).expect("read back");
            assert_eq!(read.render(), record.render());
            assert_eq!(read.verdict, Verdict::Valid, "{:?}", read.reasons);
            let step = read
                .steps
                .iter()
                .find(|step| step.id == STEP)
                .expect("calibrated step");
            let [plain, spun] = step.repetitions.as_slice() else {
                panic!("two repetitions: {:?}", step.repetitions.len());
            };
            for label in ["repetition-1", "repetition-2"] {
                let kept = options
                    .artifacts
                    .join(format!("timings/{STEP}/{label}.json"));
                assert!(kept.is_file(), "{} kept", kept.display());
            }

            let window = WINDOW.as_secs_f64();
            let seconds = |repetition: &Repetition| repetition.seconds;
            for repetition in [plain, spun] {
                within(
                    "window seconds",
                    seconds(repetition),
                    window,
                    window + WINDOW_SLACK_SECONDS,
                )
                .expect("the known delay");
            }
            let driver =
                |repetition: &Repetition| repetition.driver_cpu_seconds.expect("driver CPU");
            let spin = SPIN.as_secs_f64();
            let (spin_low, spin_high) = (
                spin * SPIN_FLOOR,
                spin + TICKS_SECONDS + DRIVER_DRIFT * driver(plain),
            );
            within(
                "spin CPU",
                driver(spun) - driver(plain),
                spin_low,
                spin_high,
            )
            .expect("the known CPU");

            let server_p50 = plain.reading("p50_ms");
            let client_p50 = plain.reading("client_p50_ms");
            let (Some(above), Some(client)) = (server_p50.above, client_p50.value) else {
                panic!("bucketed server and exact client p50: {server_p50:?} {client_p50:?}");
            };
            within("server p50 lower bound", above, f64::MIN_POSITIVE, client)
                .expect("the server's time lies inside the client's round trip");
            let verifications = plain.reading("verifications").value.expect("verifications");
            let late = plain
                .reading("verifications_after_window")
                .value
                .expect("late");
            let recorded: u64 = plain
                .reconciliation
                .iter()
                .map(|cohort| cohort.recorded)
                .sum();
            assert_eq!(
                recorded as f64,
                verifications + late,
                "{:?}",
                plain.reconciliation
            );

            let bench = &mut workload.bench;
            let server = bench.server.as_mut().expect("the server still runs");
            let binary = release_binary().expect("release server");
            server
                .restart(&binary, &[("WYRD_METRICS_BENCHMARK_SNAPSHOT", "0")])
                .await
                .expect("serve without collection");
            let server = &*server;
            assert!(
                server.timings().await.is_err(),
                "collection off serves no timing readout"
            );
            let states = &bench.states[..1];
            drive(states, Instant::now() + Duration::from_secs(1), "warmup").await;
            let (opened, _) = Window::open(server, SELECTION).await.expect("open");
            let stop = Instant::now() + WINDOW;
            let (measured, usage, _) = opened
                .measure(server, stop.into_std(), drive(states, stop, "repetition-1"))
                .await
                .expect("control window");
            assert!(measured.errors.is_empty(), "{:?}", measured.errors);
            let control = per_verification(
                usage.seconds,
                usage.server_cpu_seconds.expect("control server CPU"),
                (measured.latencies_us.len() as u64 - measured.late) as f64,
            );
            let enabled = per_verification(
                plain.seconds,
                plain.server_cpu_seconds.expect("server CPU"),
                verifications,
            );
            let mut costs = Vec::new();
            for (what, index) in [("wall", 0), ("server CPU", 1)] {
                let cost = enabled[index] - control[index];
                let allowance = (control[index] * COST_SHARE).max(COST_FLOOR_MS);
                println!(
                    "{what} per verification: {:.4} ms collected, {:.4} ms control, observation cost {cost:+.4} ms, allowance ±{allowance:.4} ms",
                    enabled[index], control[index]
                );
                costs.push((what, cost, allowance));
            }
            println!(
                "window {:.4}/{:.4} s against {window} s; spin CPU {:.4} s in [{spin_low:.4}, {spin_high:.4}]; server p50 ({above}, {:?}] ms under client p50 {client} ms; residual mean {:?} ms",
                plain.seconds,
                spun.seconds,
                driver(spun) - driver(plain),
                server_p50.value,
                plain.reading("overhead_mean_ms").value,
            );
            for (what, cost, allowance) in &costs {
                within(
                    &format!("{what} observation cost"),
                    *cost,
                    -allowance,
                    *allowance,
                )
                .expect("observing stays within its allowance");
            }

            assert!(
                within(
                    "window seconds",
                    plain.seconds * 1e3,
                    window,
                    window + WINDOW_SLACK_SECONDS
                )
                .is_err(),
                "a window read in milliseconds is rejected"
            );
            assert!(
                within("spin CPU", 0.0, spin_low, spin_high).is_err(),
                "a CPU reader that missed the spin is rejected"
            );
            assert!(
                within(
                    "server p50 lower bound",
                    client * 2.0,
                    f64::MIN_POSITIVE,
                    client
                )
                .is_err(),
                "a server time beyond the client's round trip is rejected"
            );
            let (_, _, allowance) = costs[1];
            assert!(
                within("observation cost", allowance * 2.0, -allowance, allowance).is_err(),
                "an excessive observation cost is rejected"
            );
            workload.bench.finish().await.expect("stop");
        }
    }
}
