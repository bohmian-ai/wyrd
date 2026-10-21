//! The closed-loop overhead steps, and what every gateway window measures.
//!
//! Every overhead step is one [`Bench::run`]: N closed-loop clients, each on
//! its own keep-alive connection and key, sending the next call as soon as
//! the last one is validated, for [`WINDOW`] after [`WARMUP`]. The warmup is
//! settled before the baseline scrape, the traffic window closes when the
//! last client stops, and a separate drain interval then waits for every
//! in-flight call, capture, and outbox write before the final correctness
//! scrape, so neither warmup work nor post-answer work is misattributed.
//! Each step is profiled at [`PROFILE_HERTZ`] unless profiling is off.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use wyrd_testing::capacity::{HistogramKey, Readings, Selection, Telemetry, Usage, Window};
use wyrd_testing::profile::{Capture, Profile};
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::gateway::Result;
use crate::gateway::workload::{Outcome, Shape, Tenant, Timed, call, http};

/// Untimed load before each window, so connections, key exchanges, and
/// first-use caches are not measured.
pub const WARMUP: Duration = Duration::from_secs(2);

/// Measured traffic window of each overhead step.
pub const WINDOW: Duration = Duration::from_secs(10);

/// Sampling frequency of every profiled window, hertz.
pub const PROFILE_HERTZ: u32 = 999;

/// How long in-flight calls, captures, and outbox writes may take to settle
/// once arrivals stop.
pub const DRAIN_LIMIT: Duration = Duration::from_secs(60);

/// How often a drain reads `/metrics` again.
const POLL: Duration = Duration::from_millis(100);

/// The call's server duration, arrival to a handoff point.
pub const DURATION: &str = "wyrd_gateway_duration_seconds";

/// The server duration less the provider exchange time it waited on.
pub const OVERHEAD: &str = "wyrd_gateway_overhead_seconds";

/// The provider exchange time a call waited on until a point.
pub const UPSTREAM: &str = "wyrd_gateway_upstream_seconds";

/// Series filter of the benchmarked operation.
const CHAT: &str = "operation=\"chat_completions\"";

/// Series filter of a buffered call.
const BUFFERED: &str = "stream=\"false\"";

/// Series filter of a streamed call.
const STREAMED: &str = "stream=\"true\"";

/// Series filter of a successful call.
const SUCCEEDED: &str = "outcome=\"succeeded\"";

/// One server handoff point a step judges: its overhead, total duration,
/// and upstream wait histograms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    /// How the report names it.
    pub name: &'static str,
    /// Overhead histogram key.
    pub overhead: HistogramKey,
    /// Server duration histogram key.
    pub duration: HistogramKey,
    /// Upstream wait histogram key.
    pub upstream: HistogramKey,
}

/// A buffered call's answer handoff.
pub const BUFFERED_ANSWER: Point = Point {
    name: "answer",
    overhead: (OVERHEAD, &[CHAT, BUFFERED, "point=\"answer\"", SUCCEEDED]),
    duration: (DURATION, &[CHAT, BUFFERED, "point=\"answer\"", SUCCEEDED]),
    upstream: (UPSTREAM, &[CHAT, BUFFERED, "point=\"end\""]),
};

/// A stream's head handoff.
pub const STREAM_ANSWER: Point = Point {
    name: "stream answer",
    overhead: (OVERHEAD, &[CHAT, STREAMED, "point=\"answer\"", SUCCEEDED]),
    duration: (DURATION, &[CHAT, STREAMED, "point=\"answer\"", SUCCEEDED]),
    upstream: (UPSTREAM, &[CHAT, STREAMED, "point=\"headers\""]),
};

/// A stream's final frame handoff.
pub const STREAM_END: Point = Point {
    name: "stream end",
    overhead: (OVERHEAD, &[CHAT, STREAMED, "point=\"end\"", SUCCEEDED]),
    duration: (DURATION, &[CHAT, STREAMED, "point=\"end\"", SUCCEEDED]),
    upstream: (UPSTREAM, &[CHAT, STREAMED, "point=\"end\""]),
};

/// Every histogram a gateway window differences whole.
const HISTOGRAMS: [HistogramKey; 9] = [
    BUFFERED_ANSWER.overhead,
    BUFFERED_ANSWER.duration,
    BUFFERED_ANSWER.upstream,
    STREAM_ANSWER.overhead,
    STREAM_ANSWER.duration,
    STREAM_ANSWER.upstream,
    STREAM_END.overhead,
    STREAM_END.duration,
    STREAM_END.upstream,
];

/// The gateway's component histograms a window differences, each with the
/// label naming its components: request-path phases, overhead and duration
/// by point, the provider wait, the whole request, each attempt, first byte,
/// queue delay, each outbox's writes, the Scribe acknowledgement and WAL,
/// and the app-pool connection wait.
const BREAKDOWN: [(&str, &str); 13] = [
    ("wyrd_gateway_phase_duration_seconds", "phase"),
    (OVERHEAD, "point"),
    (DURATION, "point"),
    (UPSTREAM, "point"),
    ("wyrd_gateway_request_duration_seconds", "outcome"),
    ("wyrd_gateway_attempt_duration_seconds", "outcome"),
    ("wyrd_gateway_time_to_first_byte_seconds", "operation"),
    ("wyrd_gateway_queue_delay_seconds", "operation"),
    ("outbox_write_seconds", "outbox"),
    ("bifrost_scribe_ack_seconds", ""),
    ("bifrost_scribe_wal_append_seconds", "outcome"),
    ("bifrost_scribe_wal_fsync_seconds", "outcome"),
    ("wyrd_postgres_pool_acquire_seconds", "outcome"),
];

/// Everything a gateway window reads from `/metrics`.
pub const SELECTION: Selection = Selection {
    histograms: &HISTOGRAMS,
    breakdown: &BREAKDOWN,
    prefix: "wyrd_gateway_",
};

/// Component rows printed per window, largest total first.
const PARETO_ROWS: usize = 25;

/// Client samples of one window, microseconds from just before each send.
#[derive(Debug, Default, Clone)]
pub struct Samples {
    /// Calls sent in the window, whatever their outcome.
    pub arrivals: u64,
    /// Validated answers: until the complete answer (a stream's EOF).
    pub completion_us: Vec<u64>,
    /// Validated streams: until the response headers.
    pub headers_us: Vec<u64>,
    /// Validated streams: until the first complete data event.
    pub first_event_us: Vec<u64>,
    /// Calls that did not end validated, by outcome.
    pub errors: BTreeMap<String, u64>,
}

impl Samples {
    /// Records one call.
    pub fn record(&mut self, timed: &Timed) {
        self.arrivals += 1;
        let since = |at: Instant| micros(at.saturating_duration_since(timed.sent));
        match &timed.outcome {
            Outcome::Answered => {
                self.completion_us.push(since(timed.done));
                if let (Some(headers), Some(first)) = (timed.headers, timed.events.first()) {
                    self.headers_us.push(since(headers));
                    self.first_event_us.push(since(*first));
                }
            }
            other => *self.errors.entry(outcome_key(other)).or_default() += 1,
        }
    }

    /// Folds another client's samples into these.
    fn merge(&mut self, other: Self) {
        self.arrivals += other.arrivals;
        self.completion_us.extend(other.completion_us);
        self.headers_us.extend(other.headers_us);
        self.first_event_us.extend(other.first_event_us);
        for (key, count) in other.errors {
            *self.errors.entry(key).or_default() += count;
        }
    }

    /// Calls that did not end validated.
    pub fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }
}

/// The bounded error key of a call outcome: the refusal code, `timed_out`,
/// or the failure's class before any detail.
pub fn outcome_key(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Answered => "answered".to_owned(),
        Outcome::Refused(code) => code.clone(),
        Outcome::TimedOut => "timed_out".to_owned(),
        Outcome::Failed(reason) => reason.split(':').next().unwrap_or(reason).to_owned(),
    }
}

/// Integrity counters between a step's baseline and its final scrape.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Integrity {
    /// Events every outbox counted lost.
    pub lost: f64,
    /// Captures staged.
    pub staged: f64,
    /// Captures dropped before staging, any reason.
    pub dropped: f64,
    /// Successful Scribe outbox writes, proving the writer ran.
    pub written: f64,
    /// Whether a counter went backwards, so no delta is evidence.
    pub reset: bool,
}

impl Integrity {
    /// The integrity counters `after` added to `before`.
    pub fn between(before: &Metrics, after: &Metrics) -> Self {
        let delta =
            |family: &str, labels: &[&str]| after.sum(family, labels) - before.sum(family, labels);
        let captures = |metrics: &Metrics| metrics.sum("wyrd_gateway_capture_total", &[]);
        let staged = delta("wyrd_gateway_capture_total", &["outcome=\"staged\""]);
        let lost = delta("outbox_events_lost_total", &[]);
        let total = captures(after) - captures(before);
        let written = delta("outbox_write_seconds_count", &["outbox=\"scribe\""]);
        Self {
            lost,
            staged,
            dropped: total - staged,
            written,
            reset: [lost, staged, total, written]
                .iter()
                .any(|value| *value < 0.0)
                || ![lost, staged, total, written]
                    .iter()
                    .all(|value| value.is_finite()),
        }
    }
}

/// What one overhead step measured.
#[derive(Debug, Default, Clone)]
pub struct Step {
    /// Step name.
    pub name: String,
    /// Concurrent clients.
    pub clients: usize,
    /// Whether its calls stream.
    pub stream: bool,
    /// Client samples.
    pub samples: Samples,
    /// Server and driver usage over the traffic window alone.
    pub traffic: Usage,
    /// Seconds the drain took, or `None` when it did not settle within
    /// [`DRAIN_LIMIT`].
    pub drain: Option<f64>,
    /// Server and driver usage over the drain alone.
    pub drained: Usage,
    /// Selected telemetry from the baseline to the final scrape.
    pub telemetry: Telemetry,
    /// Integrity counters from the baseline to the final scrape.
    pub integrity: Integrity,
    /// `None` without profiling; otherwise every missing evidence class.
    pub profile_gaps: Option<Vec<String>>,
}

/// Numbers and starts each window's profile, or none without profiling.
pub struct Profiler<'a> {
    /// The profiled server, for its PID.
    server: &'a LocalServer,
    /// Where each window's profile lands, or `None` without profiling.
    profiles: Option<PathBuf>,
    /// Windows profiled so far; numbers each profile's directory.
    windows: AtomicUsize,
}

impl<'a> Profiler<'a> {
    /// Profiles `server`'s windows under `profiles`, or none when it is
    /// `None`.
    pub const fn new(server: &'a LocalServer, profiles: Option<PathBuf>) -> Self {
        Self {
            server,
            profiles,
            windows: AtomicUsize::new(0),
        }
    }

    /// Starts window `name`'s profile under `profiles/NN-<name>/`, unless
    /// profiling is off.
    ///
    /// # Errors
    ///
    /// Returns the profiler start failure or a server that has exited.
    pub fn start(&self, name: &str) -> Result<Option<Capture>> {
        let Some(profiles) = &self.profiles else {
            return Ok(None);
        };
        let number = self.windows.fetch_add(1, Ordering::Relaxed);
        let pid = self.server.pid().ok_or("the profiled server has exited")?;
        let directory = profiles.join(format!("{number:02}-{}", name.replace(' ', "-")));
        Ok(Some(Capture::start(pid, 0, PROFILE_HERTZ, &directory)?))
    }
}

/// One tenant's closed-loop calls through the real server, profiled per
/// window.
pub struct Bench<'a> {
    /// The server, for `/metrics` and resources.
    server: &'a LocalServer,
    /// The provisioned tenant, holding one key per client.
    tenant: &'a Tenant,
    /// Each window's profile.
    profiler: Profiler<'a>,
}

impl<'a> Bench<'a> {
    /// The benchmark over `server`'s provisioned `tenant`, profiling every
    /// window under `profiles` unless it is `None`.
    pub const fn new(
        server: &'a LocalServer,
        tenant: &'a Tenant,
        profiles: Option<PathBuf>,
    ) -> Self {
        Self {
            server,
            tenant,
            profiler: Profiler::new(server, profiles),
        }
    }

    /// Runs `clients` closed-loop clients making `shape` calls: warmup,
    /// settle, baseline, traffic window, drain, final scrape, each window
    /// profiled, and prints the runtime and CPU Pareto.
    ///
    /// # Errors
    ///
    /// Returns a scrape, client, or profiler start/finish failure, or a
    /// warmup that does not settle.
    pub async fn run(&self, name: &str, shape: &Shape, clients: usize) -> Result<Step> {
        if clients > self.tenant.keys.len() {
            return Err(format!("{clients} clients need as many keys").into());
        }
        self.drive(shape, clients, WARMUP).await?;
        if settle(self.server).await?.is_none() {
            return Err(format!("{name}: warmup did not settle within 60 s").into());
        }
        let capture = self.profiler.start(name)?;
        let (traffic, baseline) = Window::open(self.server, SELECTION).await?;
        let samples = self.drive(shape, clients, WINDOW).await?;
        let (traffic, _) = traffic.close(self.server).await?;
        let (draining, _) = Window::open(self.server, Selection::RESOURCES).await?;
        let drain = settle(self.server).await?;
        let (drained, last) = draining.close(self.server).await?;
        let mut step = Step {
            name: name.to_owned(),
            clients,
            stream: shape.stream,
            samples,
            traffic,
            drain,
            drained,
            telemetry: Readings::read(&baseline, SELECTION).since(&last),
            integrity: Integrity::between(&baseline, &last),
            profile_gaps: None,
        };
        let metadata = serde_json::json!({"step": name, "clients": clients});
        let profile = record(capture, &step.telemetry, &metadata)?;
        pareto(
            &Evidence {
                name,
                calls: step.samples.arrivals,
                errors: step.samples.error_count(),
                traffic: &step.traffic,
                drain: step.drain,
                telemetry: &step.telemetry,
            },
            profile.as_ref(),
        );
        step.profile_gaps = profile.as_ref().map(gaps);
        Ok(step)
    }

    /// Runs `clients` closed-loop clients, each with its own connection and
    /// key, making `shape` calls for `duration`.
    ///
    /// # Errors
    ///
    /// Returns the HTTP client build failure.
    async fn drive(&self, shape: &Shape, clients: usize, duration: Duration) -> Result<Samples> {
        let deadline = Instant::now() + duration;
        let body = shape.body("overhead");
        let mut tasks = JoinSet::new();
        for key in self.tenant.keys.iter().take(clients) {
            let (http, key, shape, body) = (http()?, key.clone(), shape.clone(), body.clone());
            tasks.spawn(async move {
                let mut samples = Samples::default();
                while Instant::now() < deadline {
                    samples.record(&call(&http, &key, &shape, body.clone()).await);
                }
                samples
            });
        }
        let mut total = Samples::default();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(samples) => total.merge(samples),
                Err(_) => *total.errors.entry("panicked".to_owned()).or_default() += 1,
            }
        }
        Ok(total)
    }
}

/// What one measured window's Pareto reads.
pub struct Evidence<'a> {
    /// Window name.
    pub name: &'a str,
    /// Calls the window sent.
    pub calls: u64,
    /// Calls that did not end validated.
    pub errors: u64,
    /// Server and driver usage over its traffic.
    pub traffic: &'a Usage,
    /// Seconds its drain took, or `None` when it did not settle.
    pub drain: Option<f64>,
    /// Server telemetry from its baseline to its final scrape.
    pub telemetry: &'a Telemetry,
}

/// Finishes `capture`, when profiling, writing `metadata` plus the
/// `telemetry` breakdown beside its files.
///
/// Blocking: it waits for `perf` to flush and report.
///
/// # Errors
///
/// Returns the profiler finish failure.
pub fn record(
    capture: Option<Capture>,
    telemetry: &Telemetry,
    metadata: &serde_json::Value,
) -> Result<Option<Profile>> {
    let Some(capture) = capture else {
        return Ok(None);
    };
    let mut metadata = metadata.clone();
    metadata["breakdown"] = serde_json::json!(telemetry.breakdown);
    Ok(Some(capture.finish(&metadata)?))
}

/// Every evidence class `profile` could not collect.
pub fn gaps(profile: &Profile) -> Vec<String> {
    [&profile.failure, &profile.off_cpu_failure]
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}

/// Waits until no gateway call is active and every outbox is empty, and
/// returns the seconds it took, or `None` after [`DRAIN_LIMIT`].
///
/// # Errors
///
/// Returns scrape failures.
pub async fn settle(server: &LocalServer) -> Result<Option<f64>> {
    let started = Instant::now();
    while started.elapsed() < DRAIN_LIMIT {
        let metrics = server.metrics().await?;
        let busy =
            metrics.sum("wyrd_gateway_active_calls", &[]) + metrics.sum("outbox_pending", &[]);
        if busy <= 0.0 {
            return Ok(Some(started.elapsed().as_secs_f64()));
        }
        tokio::time::sleep(POLL).await;
    }
    Ok(None)
}

/// Microseconds in `elapsed`.
pub fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

/// Prints one window's runtime Pareto and, when profiled, its CPU Pareto and
/// off-CPU stacks, or why each is not usable evidence.
///
/// Component rows are the server's own histograms: total time over the
/// window per call beside the mean per observation. Components nest and
/// overlap, so they are read beside client latency, never summed.
pub fn pareto(evidence: &Evidence<'_>, profile: Option<&Profile>) {
    let calls = evidence.calls.max(1) as f64;
    eprintln!(
        "runtime pareto, {}: {:.0}/s, {} errors, drain {}",
        evidence.name,
        evidence.calls as f64 / evidence.traffic.seconds.max(f64::EPSILON),
        evidence.errors,
        evidence
            .drain
            .map_or("timed out".to_owned(), |seconds| format!("{seconds:.1} s")),
    );
    let mut components: Vec<(&String, &(f64, f64))> = evidence.telemetry.breakdown.iter().collect();
    components.sort_by(|left, right| right.1.0.total_cmp(&left.1.0));
    for (component, (seconds, count)) in components.iter().take(PARETO_ROWS) {
        eprintln!(
            "  {:9.4} ms/call {:9.4} ms mean x{:<9.0} {component}",
            seconds * 1e3 / calls,
            seconds * 1e3 / count.max(f64::EPSILON),
            count
        );
    }
    let Some(profile) = profile else {
        eprintln!("cpu pareto, {}: not profiled (--no-profile)", evidence.name);
        return;
    };
    let cpu_ms = evidence.traffic.server_cpu_seconds * 1e3 / calls;
    eprintln!(
        "cpu pareto, {}: {cpu_ms:.3} ms server CPU per call ({})",
        evidence.name,
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
        eprintln!("  {:10.4} ms  {stack}", seconds * 1e3 / calls);
    }
}
