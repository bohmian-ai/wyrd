//! Gateway L1 `overhead`: closed-loop calls through the real server, and
//! what every gateway window measures.
//!
//! Every repetition of a step is N closed-loop clients, each on its own
//! keep-alive connection and key, sending the next call as soon as the last
//! one is validated, for the window the lifecycle asks. The traffic window
//! closes when the last client stops; a separate drain then waits for every
//! in-flight call, capture, and outbox write before the final correctness
//! scrape, so neither warmup work nor post-answer work is misattributed. The
//! server's exact per-call timings arrive at the shared
//! [`Collector`]; overhead is each call's elapsed time less the union of its
//! own provider waits, before any percentile. The warmup runs the same calls
//! and is discarded; each repetition starts from the settled server the
//! previous one left.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use tokio::task::JoinSet;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_testing::capacity::{
    Collector, Population, SMOKE_WINDOW, Selection, Smoke, Usage, Window, release_binary,
};
use wyrd_testing::experiment::{ExperimentError, Phase, Reading, Repetition, Step, Workload};
use wyrd_testing::gateway_capture::GatewayCapture;
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::gateway::workload::{CapturePolicy, Outcome, Shape, Tenant, Timed, call, http};
use crate::gateway::{
    CLIENTS, Case, LARGE_BYTES, PROCESS, Result, SMALL_BYTES, STREAM_CHUNKS, shutdown,
};

/// How long in-flight calls, captures, and outbox writes may take to settle
/// once arrivals stop.
pub const DRAIN_LIMIT: Duration = Duration::from_secs(60);

/// How often a drain reads `/metrics` again.
const POLL: Duration = Duration::from_millis(100);

/// The server timing of a buffered call: arrival until its answer handoff.
pub const BUFFERED: &str = "gateway.buffered";

/// The server timing of a stream's head: arrival until it is handed to the
/// response body.
pub const STREAM_HEAD: &str = "gateway.stream.head";

/// The server timing of a stream's end: arrival until its consumer took the
/// final frame.
pub const STREAM_END: &str = "gateway.stream.end";

/// The outcome of every judged gateway timing.
pub const SUCCEEDED: &str = "succeeded";

/// The server timings each call records, its answer handoff first.
#[must_use]
pub const fn boundaries(stream: bool) -> &'static [&'static str] {
    if stream {
        &[STREAM_HEAD, STREAM_END]
    } else {
        &[BUFFERED]
    }
}

/// Client samples of one window, microseconds from just before each send.
#[derive(Debug, Default, Clone)]
pub struct Samples {
    /// Calls sent in the window, whatever their outcome.
    pub arrivals: u64,
    /// Validated answers: until the complete answer (a stream's EOF).
    pub completion_us: Vec<u64>,
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
                if let Some(first) = timed.events.first() {
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
        self.first_event_us.extend(other.first_event_us);
        for (key, count) in other.errors {
            *self.errors.entry(key).or_default() += count;
        }
    }

    /// Calls that did not end validated.
    #[must_use]
    pub fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }
}

/// The bounded error key of a call outcome: the refusal code, `timed_out`,
/// or the failure's class before any detail.
#[must_use]
pub fn outcome_key(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Answered => "answered".to_owned(),
        Outcome::Refused(code) => code.clone(),
        Outcome::TimedOut => "timed_out".to_owned(),
        Outcome::Failed(reason) => reason.split(':').next().unwrap_or(reason).to_owned(),
    }
}

/// Integrity counters between two scrapes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Integrity {
    /// Events every outbox counted lost, for every reason.
    pub lost: f64,
    /// The part of `lost` shed at an outbox's unreserved staging limit.
    pub shed: f64,
    /// Items the Scribe outbox wrote.
    pub written_items: f64,
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
    #[must_use]
    pub fn between(before: &Metrics, after: &Metrics) -> Self {
        let delta =
            |family: &str, labels: &[&str]| after.sum(family, labels) - before.sum(family, labels);
        let staged = delta("wyrd_gateway_capture_total", &["outcome=\"staged\""]);
        let total = delta("wyrd_gateway_capture_total", &[]);
        let lost = delta("outbox_events_lost_total", &[]);
        let shed = delta("outbox_events_lost_total", &["reason=\"shed\""]);
        let written = delta("outbox_write_seconds_count", &["outbox=\"scribe\""]);
        let written_items = delta("outbox_items_written_total", &["outbox=\"scribe\""]);
        let counters = [lost, shed, staged, total, written, written_items];
        Self {
            lost,
            shed,
            written_items,
            staged,
            dropped: total - staged,
            written,
            reset: counters.iter().any(|value| *value < 0.0)
                || !counters.iter().all(|value| value.is_finite()),
        }
    }

    /// Items lost for any reason but accounted shedding.
    #[must_use]
    pub fn unshed_loss(&self) -> f64 {
        self.lost - self.shed
    }

    /// The fraction of items handed to the outbox that were shed: shed
    /// items over written and lost ones, or zero when nothing was handed
    /// over.
    #[must_use]
    pub fn shed_fraction(&self) -> f64 {
        let handed = self.written_items + self.lost;
        if handed > 0.0 {
            self.shed / handed
        } else {
            0.0
        }
    }

    /// Why these counters cannot prove the Scribe writer ran for what was
    /// staged, or `None` when they can.
    #[must_use]
    pub fn unwritten(&self) -> Option<String> {
        (!self.reset && self.staged > 0.0 && self.written <= 0.0).then(|| {
            format!(
                "{} captures were staged but no Scribe outbox write completed",
                self.staged
            )
        })
    }
}

/// Items and accounted bytes the Scribe outbox wrote during a traffic
/// window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Durable {
    /// Items written.
    pub items: f64,
    /// Accounted bytes written.
    pub bytes: f64,
}

impl Durable {
    /// What the Scribe outbox wrote between `before` and `after`.
    #[must_use]
    pub fn between(before: &Metrics, after: &Metrics) -> Self {
        let delta = |family: &str| {
            after.sum(family, &["outbox=\"scribe\""]) - before.sum(family, &["outbox=\"scribe\""])
        };
        Self {
            items: delta("outbox_items_written_total"),
            bytes: delta("outbox_bytes_written_total"),
        }
    }
}

/// What settling after arrivals stopped found.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Drain {
    /// Seconds it waited.
    pub seconds: f64,
    /// Active calls plus pending outbox items left when it ended: zero when
    /// it settled, positive when [`DRAIN_LIMIT`] passed first.
    pub remaining: f64,
}

/// Waits until no gateway call is active and every outbox is empty, or
/// [`DRAIN_LIMIT`] passes.
///
/// # Errors
///
/// Returns scrape failures.
pub async fn settle(server: &LocalServer) -> Result<Drain> {
    let started = Instant::now();
    loop {
        let metrics = server.metrics().await?;
        let busy =
            metrics.sum("wyrd_gateway_active_calls", &[]) + metrics.sum("outbox_pending", &[]);
        if busy <= 0.0 || started.elapsed() >= DRAIN_LIMIT {
            return Ok(Drain {
                seconds: started.elapsed().as_secs_f64(),
                remaining: busy.max(0.0),
            });
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Microseconds in `elapsed`.
#[must_use]
pub fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

/// What one L1 repetition observed, before it becomes a [`Repetition`].
#[derive(Debug, Clone, Default)]
pub struct Observed {
    /// Whether its calls streamed.
    pub stream: bool,
    /// Client samples.
    pub samples: Samples,
    /// Server and driver usage over the traffic window alone.
    pub usage: Usage,
    /// The server timings of every call the window sent.
    pub population: Population,
    /// Integrity counters from the baseline to the post-drain scrape.
    pub integrity: Integrity,
    /// What the Scribe outbox wrote during the traffic window alone.
    pub durable: Durable,
    /// The drain after arrivals stopped.
    pub drain: Drain,
}

impl Observed {
    /// The repetition's readings by declared metric id. Every reading is a
    /// measurement or carries why it is missing or inapplicable; a reset
    /// counter leaves the capture readings missing, so their criteria are
    /// INVALID rather than a favorable zero.
    #[must_use]
    pub fn repetition(&self) -> Repetition {
        let usage = &self.usage;
        let samples = &self.samples;
        let integrity = &self.integrity;
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
        repetition.set(
            "requests",
            Reading::count(samples.completion_us.len() as u64),
        );
        repetition.set("arrivals", Reading::count(samples.arrivals));
        repetition.set("errors", Reading::count(samples.error_count()));
        for (id, q) in [
            ("client_p50_ms", 0.5),
            ("client_p95_ms", 0.95),
            ("client_p99_ms", 0.99),
        ] {
            repetition.set(id, Reading::percentile(&samples.completion_us, q, 1e3));
        }
        let population = &self.population;
        population.validate(&mut repetition);
        let answer = boundaries(self.stream)[0];
        population.percentiles(
            &mut repetition,
            answer,
            SUCCEEDED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        population.percentiles(
            &mut repetition,
            answer,
            SUCCEEDED,
            true,
            &[("overhead_p99_ms", 0.99)],
        );
        repetition.set(
            "upstream_p99_ms",
            Reading::percentile(
                &population.excluded(answer, SUCCEEDED),
                0.99,
                wyrd_testing::capacity::NANOS_PER_MS,
            ),
        );
        let failures: u64 = boundaries(self.stream)
            .iter()
            .map(|boundary| population.others(boundary, SUCCEEDED))
            .sum();
        repetition.set("server_failures", Reading::count(failures));
        if self.stream {
            population.percentiles(
                &mut repetition,
                STREAM_END,
                SUCCEEDED,
                true,
                &[("overhead_end_p99_ms", 0.99)],
            );
            repetition.set(
                "first_event_p99_ms",
                Reading::percentile(&samples.first_event_us, 0.99, 1e3),
            );
        } else {
            repetition.set(
                "overhead_end_p99_ms",
                Reading::missing("inapplicable: a buffered call has no final stream frame"),
            );
            repetition.set(
                "first_event_p99_ms",
                Reading::missing("inapplicable: a buffered call has no stream events"),
            );
        }
        let capture = |value: f64| {
            if integrity.reset {
                Reading::missing("a capture or outbox counter reset during the window")
            } else {
                Reading::exact(value)
            }
        };
        repetition.set("capture_staged", capture(integrity.staged));
        repetition.set(
            "capture_unstaged",
            capture((integrity.staged - samples.arrivals as f64).abs()),
        );
        repetition.set("capture_dropped", capture(integrity.dropped));
        repetition.set("capture_lost", capture(integrity.unshed_loss()));
        repetition.set("capture_shed", capture(integrity.shed));
        repetition.set(
            "capture_bytes",
            if integrity.reset || self.durable.bytes < 0.0 {
                Reading::missing("an outbox counter reset during the window")
            } else {
                Reading::exact(self.durable.bytes)
            },
        );
        repetition.set("drain_s", Reading::exact(self.drain.seconds));
        repetition.set("undrained", Reading::exact(self.drain.remaining));
        if samples.arrivals == 0 {
            repetition
                .invalid
                .push("no call was sent in the window".to_owned());
        }
        repetition.invalid.extend(integrity.unwritten());
        if integrity.shed > 0.0 && !integrity.reset {
            let seconds = usage.seconds.max(f64::EPSILON);
            repetition.observations.push(format!(
                "capture saturated: {} items shed at the outbox limit ({:.1}% of outbox items); durable capture {:.0} items/s, {:.1} MiB/s",
                integrity.shed,
                integrity.shed_fraction() * 100.0,
                self.durable.items / seconds,
                self.durable.bytes / seconds / 1_048_576.0,
            ));
        }
        repetition
    }
}

/// The Gateway release smoke: sequential calls of every `(shape, capture)`
/// pair, for an equal share of [`SMOKE_WINDOW`] each, from `tenant`'s first key, each pair under its capture
/// policy, inside one window that then settles. Every call must be
/// validated and reach `collector` as one succeeded timing at each of its
/// boundaries with its provider wait excluded, beside positive server CPU,
/// staged captures, and Scribe outbox writes.
///
/// # Errors
///
/// Returns a client, capture-policy, or scrape failure, or the [`Smoke`]
/// message naming every missing piece of evidence.
pub async fn smoke(
    server: &LocalServer,
    collector: &Collector,
    tenant: &Tenant,
    shapes: &[(Shape, CapturePolicy)],
) -> Result<String> {
    let mut smoke = Smoke::new("gateway");
    let http = http()?;
    let key = tenant
        .keys
        .first()
        .ok_or("the tenant has no invoking key")?;
    let mut expected: BTreeMap<&str, u64> = BTreeMap::new();
    let from = SystemTime::now();
    let (window, baseline) = Window::open(server, collector, Selection::RESOURCES).await?;
    let mut samples = Samples::default();
    for (shape, policy) in shapes {
        tenant.capture(*policy).await?;
        let share = SMOKE_WINDOW / u32::try_from(shapes.len()).unwrap_or(u32::MAX);
        let started = Instant::now();
        let mut calls = 0;
        while calls == 0 || started.elapsed() < share {
            samples.record(&call(&http, key, shape, shape.body("smoke")).await);
            calls += 1;
        }
        for boundary in boundaries(shape.stream) {
            *expected.entry(boundary).or_default() += calls;
        }
    }
    let (usage, _) = window.close(server).await?;
    settle(server).await?;
    let integrity = Integrity::between(&baseline, &server.metrics().await?);
    smoke.usage(&usage);
    smoke.positive("validated calls", samples.completion_us.len() as f64);
    for (outcome, count) in &samples.errors {
        smoke.missing(format!("{count} calls ended `{outcome}`"));
    }
    smoke.positive("staged captures", integrity.staged);
    smoke.positive("Scribe outbox writes", integrity.written);
    let expected: Vec<(&str, u64)> = expected.into_iter().collect();
    let population = collector.smoke("smoke", from, &expected).await;
    let judged: Vec<(&str, &str, bool)> = expected
        .iter()
        .map(|(boundary, _)| (*boundary, SUCCEEDED, true))
        .collect();
    smoke.timings(&population, &judged);
    Ok(smoke.finish()?)
}

/// The L1 workload: one server, one tenant holding a key per client, and
/// the instant upstream.
pub struct Overhead {
    /// The server, once prepared.
    server: Option<LocalServer>,
    /// The instant upstream, once prepared; held so it keeps answering.
    upstream: Option<MockServer>,
    /// The provisioned tenant, once prepared.
    tenant: Option<Tenant>,
    /// The capture policy the tenant has now.
    policy: Option<CapturePolicy>,
    /// The collector the server exports its timings to, once prepared.
    collector: Option<Collector>,
    /// Where the server log is copied when it stops; its directory keeps
    /// the raw timings.
    log: PathBuf,
}

impl Overhead {
    /// An unprepared workload that will copy the server log to `log`.
    #[must_use]
    pub const fn new(log: PathBuf) -> Self {
        Self {
            server: None,
            upstream: None,
            tenant: None,
            policy: None,
            collector: None,
            log,
        }
    }

    /// Starts the timing collector, then the release server exporting to it
    /// with one tenant, the instant upstream, and a key per client of the
    /// largest step.
    ///
    /// # Errors
    ///
    /// Returns a collector, server, upstream, or provisioning failure.
    async fn start(&mut self) -> Result<()> {
        let binary = release_binary()?;
        let artifacts = self.log.parent().unwrap_or(&self.log).to_path_buf();
        let collector = Collector::start(&artifacts).await?;
        let tracing = collector.server_env();
        let variables: Vec<(&str, &str)> = tracing
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let server = LocalServer::start(&binary, &["bench"], &variables).await?;
        self.collector = Some(collector);
        let [setup] = server.tenants() else {
            return Err("setup provisioned one tenant".into());
        };
        let upstream = upstream().await;
        let keys = CLIENTS.iter().copied().max().unwrap_or(1);
        self.tenant = Some(Tenant::provision(setup, &upstream.uri(), keys).await?);
        self.upstream = Some(upstream);
        self.server = Some(server);
        Ok(())
    }

    /// Runs `phase` of `step` for `window`.
    ///
    /// # Errors
    ///
    /// Returns an undeclared step, a capture policy, scrape, or client
    /// failure.
    async fn step(&mut self, step: &Step, phase: Phase, window: Duration) -> Result<Repetition> {
        let case = step
            .text("case")
            .and_then(Case::named)
            .ok_or_else(|| format!("step `{}` declares no known case", step.id))?;
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .ok_or_else(|| format!("step `{}` declares no client count", step.id))?;
        let (Some(server), Some(tenant), Some(collector)) =
            (&self.server, &self.tenant, &self.collector)
        else {
            return Err("the workload was not prepared".into());
        };
        if clients > tenant.keys.len() {
            return Err(format!("{clients} clients need as many keys").into());
        }
        if self.policy != Some(case.capture()) {
            tenant.capture(case.capture()).await?;
            self.policy = Some(case.capture());
        }
        let shape = case.shape();
        if phase == Phase::Warmup {
            drive(tenant, &shape, clients, window).await?;
            settle(server).await?;
            return Ok(Repetition::default());
        }
        let from = SystemTime::now();
        let (traffic, baseline) = Window::open(server, collector, Selection::RESOURCES).await?;
        let samples = drive(tenant, &shape, clients, window).await?;
        let (usage, ended) = traffic.close(server).await?;
        let drain = settle(server).await?;
        let last = server.metrics().await?;
        let expected: Vec<(&str, u64)> = boundaries(shape.stream)
            .iter()
            .map(|boundary| (*boundary, samples.arrivals))
            .collect();
        let population = collector
            .window(&format!("{}/{phase:?}", step.id), from, &expected)
            .await;
        let observed = Observed {
            stream: shape.stream,
            samples,
            usage,
            population,
            integrity: Integrity::between(&baseline, &last),
            durable: Durable::between(&baseline, &ended),
            drain,
        };
        Ok(observed.repetition())
    }
}

#[async_trait]
impl Workload for Overhead {
    /// Starts the server, upstream, and tenant, and smokes every case's
    /// shape under its capture policy ([`smoke`]).
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, or provisioning failure, or the smoke's
    /// missing evidence.
    async fn smoke(&mut self) -> std::result::Result<String, ExperimentError> {
        self.start().await.map_err(ExperimentError::workload)?;
        let (Some(server), Some(tenant), Some(collector)) =
            (&self.server, &self.tenant, &self.collector)
        else {
            return Err(ExperimentError::workload("the workload did not start"));
        };
        let shapes: Vec<(Shape, CapturePolicy)> = Case::ALL
            .iter()
            .map(|case| (case.shape(), case.capture()))
            .collect();
        self.policy = shapes.last().map(|(_, policy)| *policy);
        smoke(server, collector, tenant, &shapes)
            .await
            .map_err(ExperimentError::workload)
    }

    /// Nothing beyond what [`Self::smoke`] started.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the smoke did not start the server.
    async fn prepare(&mut self) -> std::result::Result<(), ExperimentError> {
        self.server
            .as_ref()
            .map(|_| ())
            .ok_or_else(|| ExperimentError::workload("the workload was not smoked"))
    }

    /// The server's PID while it runs.
    fn processes(&self) -> Vec<u32> {
        self.server.iter().filter_map(LocalServer::pid).collect()
    }

    /// Runs `phase` of `step` for `window`.
    ///
    /// # Errors
    ///
    /// Returns an undeclared step, a capture policy, scrape, or client
    /// failure.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> std::result::Result<Repetition, ExperimentError> {
        self.step(step, phase, window)
            .await
            .map_err(ExperimentError::workload)
    }

    /// Settles and stops the server loss-free.
    ///
    /// # Errors
    ///
    /// Returns every reason the stop was not loss-free.
    async fn finish(&mut self) -> std::result::Result<(), ExperimentError> {
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        shutdown(server, &self.log)
            .await
            .map_err(ExperimentError::workload)
    }
}

/// Runs `clients` closed-loop clients of `tenant`, each with its own
/// connection and key, making `shape` calls for `duration`.
///
/// # Errors
///
/// Returns the HTTP client build failure.
async fn drive(
    tenant: &Tenant,
    shape: &Shape,
    clients: usize,
    duration: Duration,
) -> Result<Samples> {
    let deadline = Instant::now() + duration;
    let body = shape.body("overhead");
    let mut tasks = JoinSet::new();
    for key in tenant.keys.iter().take(clients) {
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

/// The instant L1 upstream: one route per [`Case::prefix`], answering its
/// case's buffered answer or whole stream, recording no requests so its
/// memory stays bounded.
async fn upstream() -> MockServer {
    let upstream = MockServer::builder()
        .disable_request_recording()
        .start()
        .await;
    let answers = [
        (
            Case::Chat.prefix(),
            ResponseTemplate::new(200).set_body_json(GatewayCapture::sized_completion(SMALL_BYTES)),
        ),
        (
            Case::ChatStream.prefix(),
            ResponseTemplate::new(200).set_body_raw(
                GatewayCapture::chat_events("chatcmpl-bench", STREAM_CHUNKS),
                "text/event-stream",
            ),
        ),
        (
            Case::ChatCapture.prefix(),
            ResponseTemplate::new(200).set_body_json(GatewayCapture::sized_completion(LARGE_BYTES)),
        ),
    ];
    for (prefix, answer) in answers {
        Mock::given(method("POST"))
            .and(path(format!("/{prefix}/v1/chat/completions")))
            .respond_with(answer)
            .mount(&upstream)
            .await;
    }
    upstream
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wyrd_testing::capacity::{Population, Timing, Usage};
    use wyrd_testing::experiment::Reading;

    use super::{BUFFERED, Drain, Integrity, Observed, STREAM_END, STREAM_HEAD, Samples};

    /// One successful server timing at `boundary` of `elapsed_ns` with
    /// `excluded_ns` of provider wait.
    fn timing(boundary: &str, index: usize, elapsed_ns: u64, excluded_ns: u64) -> Timing {
        Timing {
            boundary: boundary.to_owned(),
            outcome: "succeeded".to_owned(),
            elapsed_ns,
            excluded_ns: Some(excluded_ns),
            end_unix_ns: 1,
            trace_id: format!("{index:032x}"),
            span_id: format!("{index:016x}"),
            instance: "server".to_owned(),
        }
    }

    /// A clean repetition of `arrivals` calls, each with one server timing
    /// per handoff of 1 ms waiting `wait_ns` on the provider, `slow` of
    /// them adding `slow_ns` of overhead.
    fn observed(stream: bool, arrivals: u64, wait_ns: u64, slow: usize, slow_ns: u64) -> Observed {
        let calls = usize::try_from(arrivals).unwrap_or(0);
        let boundaries: &[&str] = if stream {
            &[STREAM_HEAD, STREAM_END]
        } else {
            &[BUFFERED]
        };
        let timings = boundaries
            .iter()
            .flat_map(|boundary| {
                (0..calls).map(move |index| {
                    let extra = if index < slow { slow_ns } else { 0 };
                    timing(boundary, index, wait_ns + 100_000 + extra, wait_ns)
                })
            })
            .collect();
        Observed {
            stream,
            samples: Samples {
                arrivals,
                completion_us: vec![400; calls],
                first_event_us: if stream { vec![200; calls] } else { Vec::new() },
                errors: BTreeMap::new(),
            },
            usage: Usage {
                seconds: 10.0,
                server_cpu_seconds: Some(5.0),
                peak_memory_bytes: Some(1 << 30),
                driver_cpu_seconds: Some(2.0),
                collector_cpu_seconds: Some(0.1),
                ..Usage::default()
            },
            population: Population {
                timings,
                ..Population::default()
            },
            integrity: Integrity {
                staged: arrivals as f64,
                written: 1.0,
                written_items: arrivals as f64,
                ..Integrity::default()
            },
            drain: Drain {
                seconds: 0.2,
                remaining: 0.0,
            },
            ..Observed::default()
        }
    }

    /// A reading's exact value.
    fn exact(reading: &Reading) -> Option<f64> {
        reading.above.is_none().then_some(reading.value).flatten()
    }

    /// An L1 repetition reads overhead as each call's server elapsed time
    /// less its own provider wait, at the exact nearest-rank p99; server
    /// latency comes from server timings, never the client round trip; it
    /// counts errors and every capture class separately (shedding apart
    /// from loss), leaves inapplicable and reset readings missing with a
    /// reason, and observes saturation without judging it.
    ///
    /// # Panics
    ///
    /// Panics when a reading is wrong.
    #[test]
    fn overhead_repetition_subtracts_waits_per_call_and_counts_capture() {
        let clean = observed(false, 1_000, 2_000_000, 5, 400_000).repetition();
        let overhead = clean.reading("overhead_p99_ms");
        assert_eq!(
            (overhead.above, overhead.value, overhead.samples),
            (None, Some(0.1), Some(1_000)),
            "rank 990 of 1,000 residuals is a fast call's 100 µs"
        );
        assert_eq!(exact(&clean.reading("upstream_p99_ms")), Some(2.0));
        assert_eq!(exact(&clean.reading("p99_ms")), Some(2.1));
        assert_eq!(exact(&clean.reading("client_p99_ms")), Some(0.4));
        let slow = observed(false, 1_000, 2_000_000, 20, 400_000).repetition();
        assert_eq!(exact(&slow.reading("overhead_p99_ms")), Some(0.5));
        assert_eq!(exact(&clean.reading("requests")), Some(1_000.0));
        assert_eq!(exact(&clean.reading("capture_unstaged")), Some(0.0));
        assert!(clean.reading("overhead_end_p99_ms").reason.is_some());
        assert!(clean.invalid.is_empty(), "{:?}", clean.invalid);
        assert_eq!(
            clean.observations,
            [observed(false, 1_000, 2_000_000, 5, 400_000)
                .population
                .health()],
            "a clean window observes only its collection health"
        );
        let stream = observed(true, 1_000, 2_000_000, 5, 400_000).repetition();
        assert_eq!(exact(&stream.reading("overhead_end_p99_ms")), Some(0.1));
        assert_eq!(exact(&stream.reading("first_event_p99_ms")), Some(0.2));

        let mut broken = observed(false, 1_000, 2_000_000, 5, 400_000);
        broken.samples.errors.insert("timed_out".to_owned(), 2);
        broken.integrity.staged = 998.0;
        broken.integrity.dropped = 1.0;
        broken.integrity.lost = 7.0;
        broken.integrity.shed = 4.0;
        broken.drain.remaining = 3.0;
        let broken = broken.repetition();
        for (id, value) in [
            ("errors", 2.0),
            ("capture_unstaged", 2.0),
            ("capture_dropped", 1.0),
            ("capture_lost", 3.0),
            ("capture_shed", 4.0),
            ("undrained", 3.0),
        ] {
            assert_eq!(exact(&broken.reading(id)), Some(value), "{id}");
        }
        assert_eq!(broken.observations.len(), 2, "{:?}", broken.observations);
        assert!(broken.observations[1].starts_with("capture saturated"));

        let mut reset = observed(false, 1_000, 2_000_000, 5, 400_000);
        reset.integrity.reset = true;
        let reset = reset.repetition();
        assert!(!reset.reading("capture_lost").is_present());
        assert!(!reset.reading("capture_bytes").is_present());
        let mut unwritten = observed(false, 1_000, 2_000_000, 5, 400_000);
        unwritten.integrity.written = 0.0;
        assert_eq!(unwritten.repetition().invalid.len(), 1);
        assert_eq!(
            observed(false, 0, 0, 0, 0).repetition().invalid,
            [
                "no `gateway.buffered` timing observation has outcome `succeeded`",
                "no `gateway.buffered` timing observation has outcome `succeeded`",
                "no call was sent in the window",
            ]
        );
        let mut missing = observed(false, 1_000, 2_000_000, 5, 400_000);
        missing.population.timings.clear();
        missing
            .population
            .invalid
            .push("0 `gateway.buffered` timing observations arrived for 1000 requests".to_owned());
        let missing = missing.repetition();
        assert!(!missing.reading("overhead_p99_ms").is_present());
        assert!(missing.invalid[0].contains("arrived for 1000 requests"));
        let mut negative = observed(false, 1_000, 2_000_000, 5, 400_000);
        negative.population.timings[0].excluded_ns = Some(u64::MAX);
        let negative = negative.repetition();
        assert!(!negative.reading("overhead_p99_ms").is_present());
        assert!(
            negative.invalid[0].contains("exceeds its elapsed"),
            "{:?}",
            negative.invalid
        );
    }
}
