//! Gateway L1 `overhead`: closed-loop calls through the real server, and
//! what every gateway window measures.
//!
//! Every repetition of a step is N closed-loop clients, each on its own
//! keep-alive connection and key, sending the next call as soon as the last
//! one is validated, for the window the lifecycle asks, each labelled with
//! its case and phase. Work, seconds, and CPU stop at the window's end;
//! calls then in flight drain into the cohort and count late, and a
//! separate drain then waits for every capture and outbox write before the
//! final correctness scrape, so neither warmup work nor post-answer work is
//! misattributed. The
//! server's per-call timings land in its benchmark histograms, read as a
//! [`Population`] between two snapshots; overhead is each call's elapsed
//! time less the union of its own provider waits, before any percentile. The warmup runs the same calls
//! and is discarded; each repetition starts from the settled server the
//! previous one left.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use tokio::task::JoinSet;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::transport::trace::with_benchmark_labels;
use wyrd_testing::capacity::{
    Expected, NANOS_PER_MS, Population, Selection, Usage, Window, release_binary,
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
    /// Validated answers that ended at or after the admission stop; still
    /// in `completion_us`.
    pub late: u64,
}

impl Samples {
    /// Records one call, late when it was validated at or after `stop`.
    pub fn record(&mut self, timed: &Timed, stop: Option<Instant>) {
        self.arrivals += 1;
        let since = |at: Instant| micros(at.saturating_duration_since(timed.sent));
        match &timed.outcome {
            Outcome::Answered => {
                self.completion_us.push(since(timed.done));
                self.late += u64::from(stop.is_some_and(|stop| timed.done >= stop));
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
        self.late += other.late;
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

    /// Why these counters cannot prove capture staged work and the Scribe
    /// writer ran for it, or `None` when they can; every case captures, so
    /// a window that staged nothing measured no capture.
    #[must_use]
    pub fn unwritten(&self) -> Option<String> {
        if self.reset {
            None
        } else if self.staged <= 0.0 {
            Some("no capture was staged in the window".to_owned())
        } else {
            (self.written <= 0.0).then(|| {
                format!(
                    "{} captures were staged but no Scribe outbox write completed",
                    self.staged
                )
            })
        }
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
            ..Repetition::default()
        };
        repetition.set(
            "requests",
            Reading::count(samples.completion_us.len() as u64 - samples.late),
        );
        repetition.set("late_requests", Reading::count(samples.late));
        repetition.set("drain_seconds", Reading::exact(usage.drain_seconds));
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
        population.validate(&mut repetition, "measured");
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
            population.excluded(answer, SUCCEEDED).map_or_else(
                || Reading::missing("no succeeded call has a provider wait"),
                |upstream| Reading::quantile(&upstream, 0.99, NANOS_PER_MS),
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
    /// Where the server log is copied when it stops.
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
            log,
        }
    }

    /// Starts the release server with one tenant, the instant upstream, and
    /// a key per client of the largest step.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, or provisioning failure.
    async fn start(&mut self) -> Result<()> {
        let binary = release_binary()?;
        let server = LocalServer::start(&binary, &["bench"], &[]).await?;
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
        let (Some(server), Some(tenant)) = (&self.server, &self.tenant) else {
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
        let labels = (case.label(), phase.label());
        if phase == Phase::Warmup {
            drive(tenant, &shape, clients, Instant::now() + window, labels).await?;
            settle(server).await?;
            return Ok(Repetition::default());
        }
        let before = server.timings().await?;
        let (traffic, baseline) = Window::open(server, Selection::RESOURCES).await?;
        let stop = Instant::now() + window;
        let (samples, usage, stopped) = traffic
            .measure(server, stop, drive(tenant, &shape, clients, stop, labels))
            .await?;
        let samples = samples?;
        let drain = settle(server).await?;
        let last = server.metrics().await?;
        let expected: Vec<Expected<'_>> = boundaries(shape.stream)
            .iter()
            .map(|boundary| Expected {
                boundary,
                outcome: SUCCEEDED,
                case: labels.0,
                phase: labels.1,
                count: samples.completion_us.len() as u64,
            })
            .collect();
        let population = Population::collect(server, &before, &expected).await;
        let observed = Observed {
            stream: shape.stream,
            samples,
            usage,
            population,
            integrity: Integrity::between(&baseline, &last),
            durable: Durable::between(&baseline, &stopped),
            drain,
        };
        Ok(observed.repetition())
    }
}

#[async_trait]
impl Workload for Overhead {
    /// Starts the server, upstream, and tenant.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, or provisioning failure.
    async fn prepare(&mut self) -> std::result::Result<(), ExperimentError> {
        self.start().await.map_err(ExperimentError::workload)
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
/// connection and key, making `shape` calls labelled `(case, phase)`
/// admitted until `stop`; a call validated at or after `stop` is late.
///
/// # Errors
///
/// Returns the HTTP client build failure.
async fn drive(
    tenant: &Tenant,
    shape: &Shape,
    clients: usize,
    stop: Instant,
    (case, phase): (&'static str, &'static str),
) -> Result<Samples> {
    let body = shape.body("overhead");
    let mut tasks = JoinSet::new();
    for key in tenant.keys.iter().take(clients) {
        let (http, key, shape, body) = (http()?, key.clone(), shape.clone(), body.clone());
        tasks.spawn(async move {
            let mut samples = Samples::default();
            while Instant::now() < stop {
                let timed =
                    with_benchmark_labels(case, phase, call(&http, &key, &shape, body.clone()))
                        .await;
                samples.record(&timed, Some(stop));
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

    use wyrd_telemetry::timing::{Distribution, IntervalSeries};
    use wyrd_testing::capacity::{Population, UNLABELLED, Usage};
    use wyrd_testing::experiment::Reading;

    use super::{BUFFERED, Drain, Integrity, Observed, STREAM_END, STREAM_HEAD, Samples};

    /// The successful server timings at `boundary` of `calls`, each
    /// `(elapsed_ns, excluded_ns)` of provider wait.
    ///
    /// # Panics
    ///
    /// Panics when a duration is outside the histogram's trackable range or
    /// a wait exceeds its elapsed time.
    fn timing(boundary: &str, calls: &[(u64, u64)]) -> IntervalSeries {
        let durations =
            |samples: Vec<u64>| Distribution::of_durations(&samples).expect("in-range samples");
        IntervalSeries {
            boundary: boundary.to_owned(),
            outcome: "succeeded".to_owned(),
            case: UNLABELLED.to_owned(),
            phase: UNLABELLED.to_owned(),
            count: calls.len() as u64,
            elapsed: durations(calls.iter().map(|(elapsed, _)| *elapsed).collect()),
            excluded: Some(durations(calls.iter().map(|(_, wait)| *wait).collect())),
            residual: Some(durations(
                calls.iter().map(|(elapsed, wait)| elapsed - wait).collect(),
            )),
            missing_excluded: 0,
            out_of_range: 0,
            negative_residual: 0,
        }
    }

    /// A clean repetition of `arrivals` calls, each with one server timing
    /// per handoff of 1 ms waiting `wait_ns` on the provider, `slow` of
    /// them adding `slow_ns` of overhead.
    ///
    /// # Panics
    ///
    /// Panics when [`timing`] does.
    fn observed(stream: bool, arrivals: u64, wait_ns: u64, slow: usize, slow_ns: u64) -> Observed {
        let calls = usize::try_from(arrivals).unwrap_or(0);
        let boundaries: &[&str] = if stream {
            &[STREAM_HEAD, STREAM_END]
        } else {
            &[BUFFERED]
        };
        let series = if calls == 0 {
            Vec::new()
        } else {
            boundaries
                .iter()
                .map(|boundary| {
                    let calls: Vec<(u64, u64)> = (0..calls)
                        .map(|index| {
                            let extra = if index < slow { slow_ns } else { 0 };
                            (wait_ns + 100_000 + extra, wait_ns)
                        })
                        .collect();
                    timing(boundary, &calls)
                })
                .collect()
        };
        Observed {
            stream,
            samples: Samples {
                arrivals,
                completion_us: vec![400; calls],
                first_event_us: if stream { vec![200; calls] } else { Vec::new() },
                errors: BTreeMap::new(),
                late: 0,
            },
            usage: Usage {
                seconds: 10.0,
                server_cpu_seconds: Some(5.0),
                peak_memory_bytes: Some(1 << 30),
                driver_cpu_seconds: Some(2.0),
                ..Usage::default()
            },
            population: Population {
                series,
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

    /// Whether `value` lies in a reading's bucket bounds: its exclusive
    /// lower bound and inclusive upper bound, or its exact value.
    fn holds(reading: &Reading, value: f64) -> bool {
        match (reading.above, reading.value) {
            (Some(above), Some(upper)) => above < value && value <= upper,
            (None, Some(exact)) => (exact - value).abs() < 1e-9,
            _ => false,
        }
    }

    /// An L1 repetition reads overhead as each call's server elapsed time
    /// less its own provider wait, at the nearest-rank p99's bucket; server
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
        assert!(
            holds(&overhead, 0.1) && overhead.samples == Some(1_000),
            "rank 990 of 1,000 residuals is a fast call's 100 µs: {overhead:?}"
        );
        assert!(holds(&clean.reading("upstream_p99_ms"), 2.0));
        assert!(holds(&clean.reading("p99_ms"), 2.1));
        assert_eq!(exact(&clean.reading("client_p99_ms")), Some(0.4));
        let slow = observed(false, 1_000, 2_000_000, 20, 400_000).repetition();
        assert!(holds(&slow.reading("overhead_p99_ms"), 0.5));
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
        assert!(holds(&stream.reading("overhead_end_p99_ms"), 0.1));
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
                "no capture was staged in the window",
            ]
        );
        let mut missing = observed(false, 1_000, 2_000_000, 5, 400_000);
        missing.population.series.clear();
        missing.population.invalid.push(
            "0 `gateway.buffered` timing observations of case none were recorded for 1000 requests"
                .to_owned(),
        );
        let missing = missing.repetition();
        assert!(!missing.reading("overhead_p99_ms").is_present());
        assert!(missing.invalid[0].contains("recorded for 1000 requests"));
    }
}
