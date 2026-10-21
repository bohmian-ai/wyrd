//! The application side of the benchmark: public-client `verify` and
//! `submit` against the running server, and what each step measured.
//!
//! Every step is one [`Bench::run`]: N closed-loop clients that each send the
//! next arrival as soon as the last one answers, for [`WINDOW`] after
//! [`WARMUP`]. A `submit` window stays open until its backlog drains, so the
//! server's histograms hold every queued run it accepted. Each window is
//! captured with `perf` and printed as it ends: the client latency, the
//! verification runtime's own component times from `/metrics`
//! ([`BREAKDOWN`]), the CPU Pareto by crate, and the off-CPU stacks.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_testing::capacity::{HistogramKey, Percentiles, Selection, Usage, Window};
use wyrd_testing::profile::{Capture, Profile};
use wyrd_testing::release_server::LocalServer;

use crate::verification::Result;
use crate::verification::workload::{ASSERT_VERIFIER, Tenant, context, failing};

/// Untimed load before each window, so connection setup and first-use
/// caches are not measured.
const WARMUP: Duration = Duration::from_secs(2);

/// Measured window of each step.
pub const WINDOW: Duration = Duration::from_secs(10);

/// Sampling frequency of every window, hertz: high, because one direct
/// verification spends well under a millisecond of server CPU.
const PROFILE_HERTZ: u32 = 999;

/// How long a `submit` backlog may take to drain once its clients stop.
pub const DRAIN_LIMIT: Duration = Duration::from_secs(60);

/// How often a drain reads `/metrics` again.
const POLL: Duration = Duration::from_millis(100);

/// The process outboxes a verification stages on: Scribe's and the Verifier
/// submission outbox.
const OUTBOXES: [&str; 2] = ["scribe", "verifier_submissions"];

/// The verification runtime's overhead: a whole execution less its Verifier
/// blocks. Direct p99 is what REQ-015 judges.
pub const OVERHEAD: &str = "wyrd_verification_overhead_seconds";

/// The Verifier's own execution time: the union of its block intervals.
pub const VERIFIER: &str = "wyrd_verification_verifier_seconds";

/// One whole execution on the server: a direct request's arrival until its
/// result is staged, or a queued run's claim until it settles.
pub const RUN: &str = "wyrd_verification_run_duration_seconds";

/// A `submit` request's arrival until it was staged.
pub const SUBMIT: &str = "wyrd_verification_submit_seconds";

/// A queued run's wait from claimable to claimed.
pub const QUEUE_WAIT: &str = "wyrd_verification_queue_wait_seconds";

/// A queued run's creation until it settled.
pub const TRIGGER_TO_TERMINAL: &str = "wyrd_verification_trigger_to_terminal_seconds";

/// The direct-mode series filter.
pub const DIRECT: &str = "mode=\"direct\"";

/// The queued-mode series filter.
pub const QUEUED: &str = "mode=\"queued\"";

/// The series filter of a staged submission.
pub const STAGED: &str = "outcome=\"staged\"";

/// The series filter of work that came from `submit`, which the server
/// records as a `manual` execution origin.
pub const SUBMITTED: &str = "origin=\"manual\"";

/// The server histograms a step judges or reports, as `(family, series
/// filters)`; [`wyrd_testing::capacity::Telemetry::histogram`] reads them
/// by that key.
pub const HISTOGRAMS: [HistogramKey; 8] = [
    (OVERHEAD, &[DIRECT]),
    (VERIFIER, &[DIRECT]),
    (RUN, &[DIRECT]),
    (SUBMIT, &[STAGED]),
    (QUEUE_WAIT, &[SUBMITTED]),
    (OVERHEAD, &[QUEUED]),
    (VERIFIER, &[QUEUED]),
    (TRIGGER_TO_TERMINAL, &[SUBMITTED]),
];

/// The verification runtime's component histograms a window differences,
/// each with the label that names its components: execution phases,
/// Verifier blocks, queue SQL, overhead, Verifier time, whole runs, submit
/// time, queue wait, each outbox's write (drain step) time, the Scribe
/// acknowledgement, WAL, and lane times inside that write, and the wait for
/// an app-pool connection that every tenant transaction starts with. An
/// empty label reads a family that has none as one component.
const BREAKDOWN: [(&str, &str); 15] = [
    ("wyrd_verification_phase_duration_seconds", "phase"),
    ("wyrd_verification_block_duration_seconds", "kind"),
    ("wyrd_verification_store_duration_seconds", "op"),
    (OVERHEAD, "mode"),
    (VERIFIER, "mode"),
    (RUN, "mode"),
    (SUBMIT, "outcome"),
    (QUEUE_WAIT, "origin"),
    ("outbox_write_seconds", "outbox"),
    ("bifrost_scribe_ack_seconds", ""),
    ("bifrost_scribe_queue_wait_seconds", ""),
    ("bifrost_scribe_wal_append_seconds", "outcome"),
    ("bifrost_scribe_wal_fsync_seconds", "outcome"),
    ("bifrost_scribe_lane_job_seconds", "lane"),
    ("wyrd_postgres_pool_acquire_seconds", "outcome"),
];

/// Everything a verification window reads from `/metrics`.
pub const SELECTION: Selection = Selection {
    histograms: &HISTOGRAMS,
    breakdown: &BREAKDOWN,
    prefix: "wyrd_verification_",
};

/// Component rows printed per window, largest total first.
const PARETO_ROWS: usize = 25;

/// Which public call an arrival makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `observe.verify`: direct execution, judged before it answers.
    Verify,
    /// `observe.submit`: staged for one queued execution.
    Submit,
}

/// What one step measured.
#[derive(Debug, Default, Clone)]
pub struct Measured {
    /// Concurrent clients.
    pub clients: usize,
    /// Seconds the clients sent for, the denominator of throughput.
    pub seconds: f64,
    /// Latency of every acknowledged arrival, microseconds.
    pub latencies_us: Vec<u64>,
    /// Refused arrivals by stable code.
    pub errors: BTreeMap<String, u64>,
    /// Direct judgments whose verdict was not the expected one.
    pub wrong: u64,
    /// Seconds a `submit` backlog took to drain, or `None` when it did not
    /// within [`DRAIN_LIMIT`] (and always for `verify`).
    pub drain: Option<f64>,
    /// Server and driver usage over the window.
    pub usage: Usage,
}

impl Measured {
    /// Acknowledged arrivals per second while the clients sent.
    pub fn throughput(&self) -> f64 {
        self.latencies_us.len() as f64 / self.seconds.max(f64::EPSILON)
    }

    /// Refused arrivals.
    pub fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }

    /// Nearest-rank client latency percentiles, milliseconds.
    pub fn percentiles(&self) -> Percentiles {
        Percentiles::raw(&self.latencies_us, 1e-3)
    }

    /// Mean client latency, seconds.
    pub fn mean_seconds(&self) -> Option<f64> {
        (!self.latencies_us.is_empty()).then(|| {
            self.latencies_us.iter().sum::<u64>() as f64 / self.latencies_us.len() as f64 / 1e6
        })
    }

    /// Records one arrival acknowledged after `latency_us` with a right or
    /// wrong verdict, or refused with its error.
    fn record(&mut self, latency_us: u64, outcome: std::result::Result<bool, WyrdError>) {
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
}

/// The application: one tenant's Service, driven through the public client.
pub struct Bench<'a> {
    /// The server, for `/metrics`, resource evidence, and its PID.
    server: &'a LocalServer,
    /// The provisioned tenant.
    tenant: Tenant,
    /// Where each window's `perf` capture lands.
    profiles: PathBuf,
    /// Windows captured so far; numbers each capture's directory.
    windows: AtomicUsize,
}

impl<'a> Bench<'a> {
    /// The application over `server`'s provisioned `tenant`, capturing every
    /// window under `profiles`.
    pub const fn new(server: &'a LocalServer, tenant: Tenant, profiles: PathBuf) -> Self {
        Self {
            server,
            tenant,
            profiles,
            windows: AtomicUsize::new(0),
        }
    }

    /// Runs `op` from `clients` closed-loop clients for [`WINDOW`] after
    /// [`WARMUP`], capturing the window, and prints it. A `submit` step
    /// drains its warmup before the window opens, and the window stays open
    /// until its own backlog drains or [`DRAIN_LIMIT`] passes.
    ///
    /// # Errors
    ///
    /// Returns evidence or scrape failures, or a capture that cannot start
    /// or finish.
    pub async fn run(&self, step: &str, op: Op, clients: usize) -> Result<Measured> {
        self.drive(op, clients, WARMUP).await;
        if op == Op::Submit {
            self.drain().await?;
        }
        let number = self.windows.fetch_add(1, Ordering::Relaxed);
        let pid = self.server.pid().ok_or("the profiled server has exited")?;
        let directory = self
            .profiles
            .join(format!("{number:02}-{}", step.replace(' ', "-")));
        let capture = Capture::start(pid, 0, PROFILE_HERTZ, &directory)?;
        let (window, _) = Window::open(self.server, SELECTION).await?;
        let mut measured = self.drive(op, clients, WINDOW).await;
        if op == Op::Submit {
            measured.drain = self.drain().await?;
        }
        measured.clients = clients;
        measured.usage = window.close(self.server).await?.0;
        let metadata = serde_json::json!({
            "step": step,
            "clients": clients,
            "breakdown": measured.usage.telemetry.breakdown,
        });
        let profile = capture.finish(&metadata)?;
        report_pareto(step, &measured, &profile);
        Ok(measured)
    }

    /// Runs `clients` closed-loop clients making `op` calls for `duration`,
    /// each through its own application, so client `i` presents application
    /// `i`'s key.
    ///
    /// # Panics
    ///
    /// Panics when `clients` exceeds the provisioned applications, which
    /// `main` provisions for the largest selected client count.
    async fn drive(&self, op: Op, clients: usize, duration: Duration) -> Measured {
        let started = Instant::now();
        let deadline = started + duration;
        let sequence = Arc::new(AtomicU64::new(0));
        let mut tasks = JoinSet::new();
        for state in &self.tenant.states[..clients] {
            let (state, sequence) = (state.clone(), Arc::clone(&sequence));
            tasks.spawn(async move {
                let mut measured = Measured::default();
                while Instant::now() < deadline {
                    let next = sequence.fetch_add(1, Ordering::Relaxed);
                    let sent = Instant::now();
                    let outcome = call(&state, op, next).await;
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
        total.seconds = started.elapsed().as_secs_f64();
        total
    }

    /// Waits until no queued execution is pending, running, or retrying and
    /// both [`OUTBOXES`] are empty, and returns the seconds it took, or
    /// `None` after [`DRAIN_LIMIT`].
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    async fn drain(&self) -> Result<Option<f64>> {
        let started = Instant::now();
        while started.elapsed() < DRAIN_LIMIT {
            let metrics = self.server.metrics().await?;
            let backlog = metrics.sum("wyrd_verification_queue_depth", &[])
                + OUTBOXES
                    .iter()
                    .map(|outbox| metrics.sum("outbox_pending", &[&format!("outbox=\"{outbox}\"")]))
                    .sum::<f64>();
            if backlog <= 0.0 {
                return Ok(Some(started.elapsed().as_secs_f64()));
            }
            tokio::time::sleep(POLL).await;
        }
        Ok(None)
    }
}

/// Makes arrival `sequence` as `op` through `state`, the Service's
/// application lifetime.
///
/// # Errors
///
/// Returns the client's or server's refusal; a direct verdict other than
/// the expected one is `Ok(false)`, not an error.
async fn call(state: &WyrdState, op: Op, sequence: u64) -> std::result::Result<bool, WyrdError> {
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

/// Microseconds since `sent`.
fn micros(sent: Instant) -> u64 {
    u64::try_from(sent.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Prints one window's runtime Pareto and CPU Pareto, or why the CPU capture
/// is not usable evidence.
///
/// Component times are the server's own histograms: each row is the
/// component's total time over the window divided by acknowledged arrivals,
/// beside its mean per observation. Components nest (a phase holds blocks
/// and store calls) and background work overlaps requests, so rows are read
/// beside the client latency, never summed.
fn report_pareto(step: &str, measured: &Measured, profile: &Profile) {
    let arrivals = measured.latencies_us.len().max(1) as f64;
    let latency = measured.percentiles();
    eprintln!(
        "runtime pareto, {step}: {:.0}/s, client mean {:.3} ms, p50 {:.3} ms, p99 {:.3} ms, {} errors",
        measured.throughput(),
        measured.mean_seconds().unwrap_or_default() * 1e3,
        latency.p50.unwrap_or_default(),
        latency.p99.unwrap_or_default(),
        measured.error_count(),
    );
    let mut components: Vec<(&String, &(f64, f64))> =
        measured.usage.telemetry.breakdown.iter().collect();
    components.sort_by(|left, right| right.1.0.total_cmp(&left.1.0));
    for (component, (seconds, count)) in components.iter().take(PARETO_ROWS) {
        eprintln!(
            "  {:9.4} ms/arrival {:9.4} ms mean x{:<9.0} {component}",
            seconds * 1e3 / arrivals,
            seconds * 1e3 / count.max(f64::EPSILON),
            count
        );
    }
    let cpu_ms = measured.usage.server_cpu_seconds * 1e3 / arrivals;
    eprintln!(
        "cpu pareto, {step}: {cpu_ms:.3} ms server CPU per arrival ({})",
        profile.directory.display()
    );
    if let Some(failure) = &profile.failure {
        eprintln!("  profile INCOMPLETE: {failure}");
    } else {
        for (owner, percent) in &profile.crates {
            eprintln!(
                "  {:8.4} ms {percent:5.1}%  {owner}",
                cpu_ms * percent / 100.0
            );
        }
    }
    eprintln!(
        "off-cpu pareto ({}/offcpu.folded):",
        profile.directory.display()
    );
    if let Some(failure) = &profile.off_cpu_failure {
        eprintln!("  off-cpu INCOMPLETE: {failure}");
        return;
    }
    for (stack, seconds) in &profile.off_cpu {
        eprintln!("  {:10.4} ms  {stack}", seconds * 1e3 / arrivals);
    }
}
