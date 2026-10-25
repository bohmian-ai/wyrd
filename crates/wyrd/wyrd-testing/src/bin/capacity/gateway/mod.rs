//! AI gateway benchmark experiments: L1 `overhead` and L2 `capacity`.
//!
//! Both read as the production journey they measure. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) as a plain child process. The tenant administrator
//! configures `openai_compatible` deployments against a local upstream, an
//! explicit capture policy, and one machine principal per caller; every call
//! is raw HTTP/1.1 keep-alive `POST /v1/chat/completions` with a `wyrd_sk_`
//! key, timed until the complete answer is validated. All evidence comes from
//! client timings, `/metrics`, tenant capture reads, and the OS's readings of
//! the server process.
//!
//! - L1 `overhead` ([`run`]): closed-loop clients at 1/4/16/64 for each
//!   [`Case`] against an instant upstream. The server's added overhead
//!   (arrival to handoff, less the provider exchange) p99 must be strictly
//!   under 0.5 ms at 64 clients at every applicable handoff point; every
//!   step needs zero refused or invalid answers, one staged capture per
//!   arrival, no drop, no loss, no shedding, and a bounded drain.
//! - L2 `capacity` ([`capacity`]): one open-loop step whose every repetition
//!   runs capacity → overload → recovery against a paced upstream with four
//!   contending tenants, judging completion, first token, chunk lag, and full
//!   stream p99 against declared SLOs in capacity and recovery, only declared
//!   admission refusals in overload, bounded backlog and drain, and capture
//!   accounting throughout.
//!
//! Each module declares its experiment here and drives it through the shared
//! lifecycle in `wyrd_testing::experiment`, which owns repetitions, validity,
//! judgment, the record, and the report.

pub mod capacity;
pub mod run;
pub mod workload;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wyrd_testing::experiment::{
    Criterion, Declaration, Derive, Direction, Level, Metric, Op, Step,
};
use wyrd_testing::release_server::LocalServer;

use crate::levels::{Context, Plan};
use capacity::{
    ADMISSION_REFUSALS, KEYS_PER_TENANT, MAX_IN_FLIGHT, MISS_AFTER, OVERLOAD_FACTOR, PACING,
    RECOVERY_GRACE, Slo, TENANTS,
};
use workload::CapturePolicy;

/// Error type of every workload step; the shared lifecycle records it.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Every client count an L1 case runs at.
pub const CLIENTS: [usize; 4] = [1, 4, 16, 64];

/// The client count the fixed overhead SLO applies at.
pub const SLO_CLIENTS: usize = 64;

/// The strict L1 overhead p99 limit, milliseconds; equality fails.
pub const OVERHEAD_LIMIT_MS: f64 = 0.5;

/// The declared server process whose CPU every repetition sums.
pub const PROCESS: &str = "wyrd-server";

/// Data events of every streamed L1 answer.
pub const STREAM_CHUNKS: usize = 32;

/// Request content, and buffered answer content, of the small L1 cases.
pub const SMALL_BYTES: usize = 1024;

/// Request and answer content of the L1 payload-capture case.
pub const LARGE_BYTES: usize = 64 * 1024;

/// The L2 aggregate offered rate when `--rate` is not given, requests/s.
pub const DEFAULT_RATE: f64 = 250.0;

/// The L2 experiment's only step.
pub const CONTENTION: &str = "contention";

/// L2 flags that take a value.
pub const CAPACITY_FLAGS: [&str; 4] = [
    "--rate",
    "--max-completion-ms",
    "--max-first-token-ms",
    "--max-stream-ms",
];

/// One L1 overhead case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    /// Buffered 1 KiB chat with metadata capture.
    Chat,
    /// Streamed 1 KiB chat with metadata capture.
    ChatStream,
    /// Buffered 64 KiB chat with request and response payload capture.
    ChatCapture,
}

impl Case {
    /// Every case, in run order.
    pub const ALL: [Self; 3] = [Self::Chat, Self::ChatStream, Self::ChatCapture];

    /// The case's stable id, the first part of its step ids.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::ChatStream => "chat-stream",
            Self::ChatCapture => "chat-capture",
        }
    }

    /// The case whose [`Self::id`] is `id`.
    #[must_use]
    pub fn named(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|case| case.id() == id)
    }

    /// The upstream path prefix whose deployment answers it.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::ChatStream => "stream",
            Self::ChatCapture => "capture",
        }
    }

    /// What its tenant captures.
    #[must_use]
    pub const fn capture(self) -> CapturePolicy {
        match self {
            Self::Chat | Self::ChatStream => CapturePolicy::Metadata,
            Self::ChatCapture => CapturePolicy::Payload,
        }
    }

    /// What each of its calls asks for and expects back.
    #[must_use]
    pub fn shape(self) -> workload::Shape {
        let (stream, request_bytes, expected) = match self {
            Self::Chat => (false, SMALL_BYTES, SMALL_BYTES),
            Self::ChatStream => (true, SMALL_BYTES, STREAM_CHUNKS),
            Self::ChatCapture => (false, LARGE_BYTES, LARGE_BYTES),
        };
        workload::Shape {
            model: format!("bench-{}/gpt-4o", self.prefix()),
            stream,
            request_bytes,
            expected,
        }
    }

    /// The step id of this case at `clients`.
    #[must_use]
    pub fn step(self, clients: usize) -> String {
        format!("{}.c{clients}", self.id())
    }
}

/// Plans the Gateway experiment at the context's level: refuses unknown
/// flags, declares it, and builds its workload, which starts nothing until
/// the lifecycle prepares it.
///
/// # Errors
///
/// Returns a message for an unknown argument or an invalid declared value.
pub fn plan(context: &Context<'_>, base: Declaration) -> std::result::Result<Plan, String> {
    let valued: &[&str] = match context.level {
        Level::One => &[],
        Level::Two => &CAPACITY_FLAGS,
    };
    context.known(valued, &[])?;
    let declaration = declare(context.level, context.arguments, base, context.root)?;
    let log = server_log(context.root, &declaration.experiment);
    let workload: Box<dyn wyrd_testing::experiment::Workload> = match context.level {
        Level::One => Box::new(run::Overhead::new(log)),
        Level::Two => Box::new(capacity::Capacity::new(log)),
    };
    Ok(Plan {
        declaration,
        workload,
    })
}

/// Fills `base` with the Gateway experiment at `level`: its id, question,
/// sweep, controls, metrics, criteria, and steps. Pure: it reads only
/// `arguments`, so tests judge the real declaration without a server. No
/// eligible Gateway baseline exists, so no criterion compares one.
///
/// # Errors
///
/// Returns a message for an invalid `--rate` or SLO override.
pub fn declare(
    level: Level,
    arguments: &[String],
    base: Declaration,
    _root: &Path,
) -> std::result::Result<Declaration, String> {
    match level {
        Level::One => Ok(overhead(base)),
        Level::Two => {
            let rate = crate::flags::value(arguments, "--rate")?
                .map(|value| {
                    value
                        .parse::<f64>()
                        .ok()
                        .filter(|rate| rate.is_finite() && *rate > 0.0)
                        .ok_or_else(|| format!("--rate takes positive requests/s, got `{value}`"))
                })
                .transpose()?;
            Ok(capacity_declaration(base, rate, Slo::parse(arguments)?))
        }
    }
}

/// The L1 `overhead` declaration over `base`.
fn overhead(mut base: Declaration) -> Declaration {
    "overhead".clone_into(&mut base.experiment);
    base.question = format!(
        "What overhead does the gateway add to buffered, streamed, and payload-captured chat at 1, 4, 16, and 64 closed-loop clients, and is its p99 strictly under {OVERHEAD_LIMIT_MS} ms at {SLO_CLIENTS} clients at every handoff point with every call answered and captured exactly once?"
    );
    base.variables = BTreeMap::from([
        ("case".to_owned(), json!(Case::ALL.map(Case::id))),
        ("clients".to_owned(), json!(CLIENTS)),
    ]);
    base.controls.extend([
        (
            "fixture".to_owned(),
            json!(
                "stub-provider: an instant local upstream answering a 1 KiB buffered completion, a 32-event stream, and a 64 KiB buffered completion"
            ),
        ),
        (
            "workload".to_owned(),
            json!(format!(
                "one tenant, one wyrd_sk_ key and keep-alive connection per closed-loop client; each client sends its next call when the last is validated; {}",
                Case::ALL
                    .map(|case| format!(
                        "{}: {} capture",
                        case.id(),
                        match case.capture() {
                            CapturePolicy::Metadata => "metadata",
                            CapturePolicy::Payload => "request and response payload",
                        }
                    ))
                    .join(", ")
            )),
        ),
        (
            "limits".to_owned(),
            json!({
                "overhead_p99_ms_lt": OVERHEAD_LIMIT_MS,
                "at_clients": SLO_CLIENTS,
                "errors": 0,
                "capture": "one staged capture per arrival; no drop, loss, or shedding",
                "drain_seconds": run::DRAIN_LIMIT.as_secs(),
            }),
        ),
        ("server_processes".to_owned(), json!([PROCESS])),
    ]);
    base.metrics = overhead_metrics();
    let mut steps = Vec::new();
    for case in Case::ALL {
        for clients in CLIENTS {
            let shape = case.shape();
            steps.push(Step::measured(
                &case.step(clients),
                json!({
                    "case": case.id(),
                    "clients": clients,
                    "stream": shape.stream,
                    "request_bytes": shape.request_bytes,
                }),
            ));
        }
    }
    let every: Vec<String> = steps.iter().map(|step| step.id.clone()).collect();
    let at_slo: Vec<String> = Case::ALL.map(|case| case.step(SLO_CLIENTS)).to_vec();
    let stream_at_slo = vec![Case::ChatStream.step(SLO_CLIENTS)];
    base.criteria = vec![
        Criterion::limit(
            "overhead-p99",
            "overhead_p99_ms",
            &at_slo,
            Op::Lt,
            OVERHEAD_LIMIT_MS,
        )
        .percentile(99),
        Criterion::limit(
            "overhead-end-p99",
            "overhead_end_p99_ms",
            &stream_at_slo,
            Op::Lt,
            OVERHEAD_LIMIT_MS,
        )
        .percentile(99),
        Criterion::every("no-errors", "errors", &every, Op::Le, 0.0),
        Criterion::every(
            "capture-per-arrival",
            "capture_unstaged",
            &every,
            Op::Le,
            0.0,
        ),
        Criterion::every("capture-dropped", "capture_dropped", &every, Op::Le, 0.0),
        Criterion::every("capture-lost", "capture_lost", &every, Op::Le, 0.0),
        Criterion::every("capture-shed", "capture_shed", &every, Op::Le, 0.0),
        Criterion::every("drained", "undrained", &every, Op::Le, 0.0),
    ];
    base.steps = steps;
    base
}

/// Every metric an L1 repetition reads or derives.
fn overhead_metrics() -> Vec<Metric> {
    let mut metrics = vec![
        Metric::raw(
            "requests",
            "requests",
            "client: calls that ended with a complete validated answer in the window",
        ),
        Metric::raw(
            "arrivals",
            "requests",
            "client: calls sent in the window, whatever their outcome",
        ),
        Metric::raw(
            "errors",
            "requests",
            "client: calls refused, timed out, failed, or answered invalidly in the window",
        ),
        Metric::raw(
            "server_failures",
            "requests",
            "server: answer-handoff timings whose outcome is not `succeeded`, counted in their own outcome series and never judged",
        ),
        Metric::raw(
            "first_event_p99_ms",
            "ms",
            "diagnostic only: client send until a stream's first complete data event; streams only",
        ),
        Metric::raw(
            "overhead_p99_ms",
            "ms",
            "server: per call, handler entry until the answer handoff (`gateway.buffered`, or a stream's head at `gateway.stream.head`) less the union of that call's own provider waits; nearest-rank bucket bounds of the server's timing histograms",
        ),
        Metric::raw(
            "overhead_end_p99_ms",
            "ms",
            "server: per stream, handler entry until its final frame handoff (`gateway.stream.end`) less the union of its own provider waits; nearest-rank bucket bounds of the server's timing histograms; streams only",
        ),
        Metric::raw(
            "upstream_p99_ms",
            "ms",
            "server: the union of a call's own provider waits until the answer handoff; nearest-rank bucket bounds of the server's timing histograms",
        ),
        Metric::raw("client_p50_ms", "ms", CLIENT_TIME),
        Metric::raw("client_p95_ms", "ms", CLIENT_TIME),
        Metric::raw("client_p99_ms", "ms", CLIENT_TIME),
        Metric::raw(
            "capture_staged",
            "captures",
            "server: wyrd_gateway_capture_total{outcome=staged}, baseline to post-drain scrape",
        ),
        Metric::raw(
            "capture_unstaged",
            "captures",
            "server: |staged captures − arrivals|, arrivals without exactly one staged capture",
        ),
        Metric::raw(
            "capture_dropped",
            "captures",
            "server: captures dropped before staging, any reason",
        ),
        Metric::raw(
            "capture_lost",
            "items",
            "server: outbox_events_lost_total for every reason but accounted shedding",
        ),
        Metric::raw(
            "capture_shed",
            "items",
            "server: outbox_events_lost_total{reason=shed}, items shed at an outbox's staging limit, accounted separately from loss",
        ),
        Metric::raw(
            "capture_bytes",
            "bytes",
            "server: accounted bytes the Scribe outbox wrote during the traffic window",
        ),
        Metric::raw(
            "drain_s",
            "s",
            "server: arrivals stopped until no gateway call is active and every outbox is empty",
        ),
        Metric::raw(
            "undrained",
            "items",
            "server: active gateway calls plus pending outbox items still present when the drain limit passed",
        ),
    ];
    metrics.extend(latency_metrics(
        "server: handler entry until the answer handoff (`gateway.buffered`, or a stream's head at `gateway.stream.head`), successful calls; nearest-rank bucket bounds of the server's timing histograms",
    ));
    metrics.extend(efficiency_metrics());
    metrics
}

/// The boundary of the diagnostic client round trip, which never judges.
const CLIENT_TIME: &str = "diagnostic only: client send until the complete validated answer (a stream's EOF), never judged";

/// The common report's `p50_ms`, `p95_ms`, and `p99_ms` over `boundary`.
fn latency_metrics(boundary: &str) -> Vec<Metric> {
    ["p50_ms", "p95_ms", "p99_ms"]
        .map(|id| Metric::raw(id, "ms", boundary))
        .to_vec()
}

/// The derived rate and per-core metrics every Gateway repetition reports.
fn efficiency_metrics() -> Vec<Metric> {
    vec![
        Metric::derived(
            "rate",
            "requests/s",
            "answered requests per measured second",
            Direction::Higher,
            Derive::Rate {
                work: "requests".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "requests_per_s_per_vcpu",
            "requests/s/vCPU",
            "(W/T)/P: answered requests per wall second per provisioned vCPU",
            Direction::Higher,
            Derive::PerVcpu {
                work: "requests".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "requests_per_cpu_s",
            "requests/cpu-s",
            "W/C: answered requests per server CPU-second used",
            Direction::Higher,
            Derive::PerCpu {
                work: "requests".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "cpu_us_per_request",
            "µs/request",
            "server CPU microseconds per answered request",
            Direction::Lower,
            Derive::CpuPer {
                work: "requests".to_owned(),
                scale: 1e6,
            },
        ),
        Metric::derived(
            "capture_bytes_per_s_per_vcpu",
            "B/s/vCPU",
            "(W/T)/P: accounted capture bytes the Scribe outbox wrote per wall second per provisioned vCPU",
            Direction::Higher,
            Derive::PerVcpu {
                work: "capture_bytes".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "capture_bytes_per_cpu_s",
            "B/cpu-s",
            "W/C: accounted capture bytes the Scribe outbox wrote per server CPU-second used",
            Direction::Higher,
            Derive::PerCpu {
                work: "capture_bytes".to_owned(),
                scale: 1.0,
            },
        ),
    ]
}

/// The L2 `capacity` declaration over `base` at `rate` (or
/// [`DEFAULT_RATE`]) with `slo`.
fn capacity_declaration(mut base: Declaration, rate: Option<f64>, slo: Slo) -> Declaration {
    let rate = rate.unwrap_or(DEFAULT_RATE);
    let phases = capacity::Phases::of(std::time::Duration::from_secs(
        base.timing.repetition_seconds,
    ));
    "capacity".clone_into(&mut base.experiment);
    base.question = format!(
        "At {rate} requests/s offered open-loop by four contending tenants, does the gateway meet its server completion, first-token handoff, and final-frame handoff SLOs with bounded backlog, refuse only by declared admission limits under {OVERLOAD_FACTOR}x overload, recover within {} s, drain within {} s, and account for every capture?",
        RECOVERY_GRACE.as_secs(),
        run::DRAIN_LIMIT.as_secs()
    );
    base.variables = BTreeMap::from([(
        "phase".to_owned(),
        json!(["capacity", "overload", "recovery"]),
    )]);
    base.controls.extend([
        (
            "fixture".to_owned(),
            json!({
                "upstream": "paced local upstream",
                "buffered_delay_ms": PACING.buffered_delay.as_millis(),
                "buffered_bytes": PACING.buffered_bytes,
                "stream_delay_ms": PACING.stream_delay.as_millis(),
                "chunks": PACING.chunks,
                "chunk_interval_ms": PACING.chunk_interval.as_millis(),
            }),
        ),
        (
            "workload".to_owned(),
            json!({
                "tenants": TENANTS,
                "shares": [70, 10, 10, 10],
                "modes": "every tenant alternates buffered and streamed calls",
                "keys_per_tenant": KEYS_PER_TENANT,
                "capture": "metadata",
                "rate": rate,
                "overload_rate": rate * OVERLOAD_FACTOR,
                "phases_of_repetition_seconds": {
                    "capacity": phases.capacity.as_secs(),
                    "overload": phases.overload.as_secs(),
                    "recovery": phases.recovery.as_secs(),
                    "recovery_grace": phases.grace.as_secs(),
                },
                "max_in_flight": MAX_IN_FLIGHT,
                "miss_after_ms": MISS_AFTER.as_millis(),
            }),
        ),
        (
            "limits".to_owned(),
            json!({
                "slo_p99_ms": slo.limits(),
                "slo_overrides": slo.overrides,
                "backlog_growth_requests": rate,
                "admission_refusals": ADMISSION_REFUSALS,
                "drain_seconds": run::DRAIN_LIMIT.as_secs(),
                "recovery_seconds": RECOVERY_GRACE.as_secs(),
                "capture": "lost always zero; shedding only in overload",
            }),
        ),
        ("server_processes".to_owned(), json!([PROCESS])),
    ]);
    base.metrics = capacity_metrics();
    base.steps = vec![Step::measured(
        CONTENTION,
        json!({
            "case": "tenant-contention",
            "tenants": TENANTS.len(),
            "rate": rate,
            "overload_rate": rate * OVERLOAD_FACTOR,
        }),
    )];
    let step = [CONTENTION.to_owned()];
    let mut criteria = Vec::new();
    for phase in ["capacity", "recovery"] {
        for (name, bound) in [
            ("completion", slo.completion),
            ("first_token", slo.first_token),
            ("stream", slo.stream),
        ] {
            criteria.push(
                Criterion::limit(
                    &format!("{phase}-{}", name.replace('_', "-")),
                    &format!("{phase}.{name}_p99_ms"),
                    &step,
                    Op::Le,
                    bound,
                )
                .percentile(99),
            );
        }
        criteria.push(Criterion::every(
            &format!("{phase}-errors"),
            &format!("{phase}.errors"),
            &step,
            Op::Le,
            0.0,
        ));
    }
    criteria.extend([
        Criterion::every(
            "capacity-backlog",
            "capacity.backlog_growth",
            &step,
            Op::Le,
            rate,
        ),
        Criterion::every(
            "overload-refusals",
            "overload.unexpected",
            &step,
            Op::Le,
            0.0,
        ),
        Criterion::every("drained", "undrained", &step, Op::Le, 0.0),
        Criterion::every("capture-lost", "capture.lost", &step, Op::Le, 0.0),
        Criterion::every("capture-dropped", "capture.dropped", &step, Op::Le, 0.0),
        Criterion::every(
            "capture-per-admitted",
            "capture.unstaged",
            &step,
            Op::Le,
            0.0,
        ),
        Criterion::every(
            "capture-shed-outside-overload",
            "capture.shed_outside_overload",
            &step,
            Op::Le,
            0.0,
        ),
        Criterion::every(
            "capture-ownership",
            "capture.misattributed",
            &step,
            Op::Le,
            0.0,
        ),
    ]);
    base.criteria = criteria;
    base
}

/// Every metric an L2 repetition reads or derives, phase metrics flattened
/// as `<phase>.<metric>`.
fn capacity_metrics() -> Vec<Metric> {
    let from = "server: handler entry until";
    let mut metrics = Vec::new();
    for (phase, scope, client) in [
        (
            "capacity",
            "observations completed in the capacity phase, every tenant together",
            "calls scheduled in the capacity phase; the worst tenant's p99",
        ),
        (
            "recovery",
            "observations completed after the recovery grace, every tenant together",
            "calls scheduled after the recovery grace; every tenant together",
        ),
    ] {
        for (name, boundary) in [
            (
                "completion",
                "the buffered answer's handoff (`gateway.buffered`)",
            ),
            (
                "first_token",
                "a stream's head handoff (`gateway.stream.head`)",
            ),
            (
                "chunk_lag",
                "each stream data event's receipt, measured behind its upstream send",
            ),
            (
                "stream",
                "a stream's final frame handoff (`gateway.stream.end`)",
            ),
        ] {
            let boundary = if name == "chunk_lag" {
                format!("diagnostic only, client: {boundary}, {client}; never judged")
            } else {
                format!(
                    "{from} {boundary}, {scope}; successful calls' nearest-rank bucket bounds of the server's timing histograms"
                )
            };
            metrics.push(Metric::raw(
                &format!("{phase}.{name}_p99_ms"),
                "ms",
                &boundary,
            ));
        }
        metrics.push(Metric::raw(
            &format!("{phase}.errors"),
            "requests",
            &format!(
                "client: {phase} calls that did not end with a validated answer, refusals included"
            ),
        ));
    }
    metrics.extend([
        Metric::raw(
            "capacity.backlog_growth",
            "requests",
            "driver in-flight calls, server active calls, and outbox items: the largest growth of a series' mean from the first to the last 3 s of the capacity phase",
        ),
        Metric::raw(
            "capacity.overhead_p99_ms",
            "ms",
            "server: diagnostic per-call buffered completion less the union of the call's own provider waits, calls completed in the capacity phase; nearest-rank bucket bounds of the server's timing histograms",
        ),
        Metric::raw(
            "overload.refused",
            "requests",
            "client: overload and recovery-grace calls refused with a declared admission code",
        ),
        Metric::raw(
            "overload.unexpected",
            "requests",
            "client: overload and recovery-grace calls that ended unanswered other than by a declared admission refusal",
        ),
        Metric::raw(
            "overload.shed",
            "items",
            "server: outbox items shed at the staging limit from the overload start to the recovery grace end",
        ),
        Metric::raw(
            "requests",
            "requests",
            "client: calls answered with a validated answer over the whole sequence",
        ),
        Metric::raw(
            "errors",
            "requests",
            "client: calls that ended unanswered other than by a declared overload admission refusal",
        ),
        Metric::raw(
            "capture.staged",
            "captures",
            "server: staged captures over the sequence and its drain",
        ),
        Metric::raw(
            "capture.unstaged",
            "captures",
            "server: |staged captures − calls the server admitted and settled|",
        ),
        Metric::raw("capture.dropped", "captures", "server: captures dropped before staging"),
        Metric::raw(
            "capture.lost",
            "items",
            "server: outbox items lost for every reason but accounted shedding",
        ),
        Metric::raw(
            "capture.shed_outside_overload",
            "items",
            "server: outbox items shed in the capacity phase, the judged recovery, or the drain",
        ),
        Metric::raw(
            "capture.misattributed",
            "tenants",
            "tenant reads: tenants whose readable capture rows fall outside their own answered-to-admissible calls, plus one when rows do not sum to the staged captures",
        ),
        Metric::raw(
            "capture_bytes",
            "bytes",
            "server: accounted bytes the Scribe outbox wrote while arrivals ran",
        ),
        Metric::raw(
            "drain_s",
            "s",
            "server: arrivals completed until no gateway call is active and every outbox is empty",
        ),
        Metric::raw(
            "undrained",
            "items",
            "server: active gateway calls plus pending outbox items still present when the drain limit passed",
        ),
    ]);
    metrics.extend(latency_metrics(
        "client: scheduled arrival until the complete validated buffered answer, every tenant, capacity phase",
    ));
    metrics.extend(efficiency_metrics());
    metrics
}

/// Where a run of `experiment` keeps the server log it stops with.
fn server_log(root: &Path, experiment: &str) -> PathBuf {
    root.join("target/benchmarks/gateway")
        .join(experiment)
        .join("server.log")
}

/// Server log lines that report an outbox item lost.
const LOSS_MARKERS: [&str; 3] = ["was lost", "items unwritten", "write task failed"];

/// Settles `server`, stops it cleanly, and copies its log to `log`.
///
/// # Errors
///
/// Returns every reason the stop was not loss-free: active or pending work
/// at the drain limit, an outbox item lost other than shed, an unclean
/// exit, or a log that is unreadable or reports a lost item.
pub async fn shutdown(server: LocalServer, log: &Path) -> Result<()> {
    let drain = run::settle(&server).await?;
    let metrics = server.metrics().await?;
    let lost = metrics.sum("outbox_events_lost_total", &[])
        - metrics.sum("outbox_events_lost_total", &["reason=\"shed\""]);
    if let Some(directory) = log.parent() {
        std::fs::create_dir_all(directory)?;
    }
    let mut problems = Vec::new();
    if drain.remaining > 0.0 {
        problems.push(format!(
            "{} active calls or pending outbox items before stop",
            drain.remaining
        ));
    }
    if lost != 0.0 {
        problems.push(format!(
            "{lost} outbox items lost other than shed over the run"
        ));
    }
    if let Err(error) = server.stop(log) {
        problems.push(format!("unclean exit: {error}"));
    }
    match std::fs::read_to_string(log) {
        Ok(text) => {
            let lines = text
                .lines()
                .filter(|line| LOSS_MARKERS.iter().any(|marker| line.contains(marker)))
                .count();
            if lines > 0 {
                problems.push(format!("{lines} server log lines report lost items"));
            }
        }
        Err(error) => problems.push(format!("server log unreadable: {error}")),
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; ").into())
    }
}

/// A declared step's float parameter `name`.
fn param(step: &Step, name: &str) -> Option<f64> {
    step.params.get(name).and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use serde_json::{Value, json};
    use wyrd_testing::experiment::{
        Declaration, Engine, HostRecord, Invocation, Level, Op, Rule, Source, Validity,
    };
    use wyrd_testing::release_server::Host;

    use super::{CLIENTS, CONTENTION, Case, DEFAULT_RATE, declare};

    /// A command line of `arguments` after the program name.
    fn line(arguments: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(arguments.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// A base declaration at `level` that needs no release binary, with a
    /// shared `timing` control as the runner assembles one.
    ///
    /// # Panics
    ///
    /// Panics when the host cannot be read.
    fn base(level: Level) -> Declaration {
        let host = Host::detect().expect("the host");
        Declaration {
            engine: Engine::Gateway,
            experiment: String::new(),
            level,
            question: String::new(),
            timing: level.timing(),
            validity: Validity::default(),
            variables: BTreeMap::new(),
            controls: BTreeMap::from([("timing".to_owned(), json!({ "method": "shared" }))]),
            metrics: Vec::new(),
            criteria: Vec::new(),
            steps: Vec::new(),
            selection: None,
            source: Source::from_diff("0".repeat(40), b"", false),
            build: Value::Null,
            command: Invocation::new(&[], &[]),
            host: HostRecord::new(&host, "test"),
            refusals: Vec::new(),
        }
    }

    /// L1 declares all twelve case/client steps, the strict 0.5 ms overhead
    /// p99 only at 64 clients with p99 sample sufficiency, and every
    /// correctness and capture criterion on every step, keeping the shared
    /// timing control; it is admissible.
    ///
    /// # Panics
    ///
    /// Panics when a step or criterion is missing or misdeclared.
    #[test]
    fn overhead_declares_every_case_at_every_client_count() {
        let declared = declare(Level::One, &line(&[]), base(Level::One), Path::new("."))
            .expect("the L1 declaration");
        assert_eq!(declared.experiment, "overhead");
        assert_eq!(declared.steps.len(), Case::ALL.len() * CLIENTS.len());
        assert!(
            declared
                .steps
                .iter()
                .any(|step| step.id == "chat-capture.c1")
        );
        let overhead = declared
            .criteria
            .iter()
            .find(|criterion| criterion.id == "overhead-p99")
            .expect("the overhead criterion");
        assert_eq!(
            overhead.steps,
            vec!["chat.c64", "chat-stream.c64", "chat-capture.c64"]
        );
        assert_eq!(overhead.percentile, Some(99));
        assert!(matches!(
            overhead.rule,
            Rule::Limit { op: Op::Lt, limit, .. } if (limit - 0.5).abs() < f64::EPSILON
        ));
        for id in ["no-errors", "capture-lost", "capture-shed", "drained"] {
            let criterion = declared
                .criteria
                .iter()
                .find(|criterion| criterion.id == id)
                .expect("a correctness criterion");
            assert_eq!(criterion.steps.len(), 12, "{id}");
        }
        assert!(declared.refusals().is_empty(), "{:?}", declared.refusals());
        assert_eq!(declared.controls["timing"], json!({ "method": "shared" }));
        let shape = Case::ChatCapture.shape();
        assert_eq!(
            (shape.request_bytes, shape.expected, shape.stream),
            (65_536, 65_536, false)
        );
        assert_eq!(Case::ChatStream.shape().expected, 32);
    }

    /// L2 declares one contention step at the default rate with the SLO
    /// defaults, keeps the shared timing control, fixes explicit overrides before execution in its controls,
    /// and refuses invalid values.
    ///
    /// # Panics
    ///
    /// Panics when a default, override, or refusal is wrong.
    #[test]
    fn capacity_declares_default_slos_and_recorded_overrides() {
        let declared = declare(Level::Two, &line(&[]), base(Level::Two), Path::new("."))
            .expect("the L2 declaration");
        assert_eq!(declared.experiment, "capacity");
        assert_eq!(declared.steps.len(), 1);
        assert_eq!(declared.steps[0].id, CONTENTION);
        assert_eq!(declared.steps[0].params["rate"], json!(DEFAULT_RATE));
        assert_eq!(declared.controls["timing"], json!({ "method": "shared" }));
        let limit = |declared: &super::Declaration, id: &str| {
            declared
                .criteria
                .iter()
                .find(|criterion| criterion.id == id)
                .and_then(|criterion| match criterion.rule {
                    Rule::Limit { limit, .. } => Some(limit),
                    _ => None,
                })
        };
        for (id, bound) in [
            ("capacity-completion", 200.0),
            ("capacity-first-token", 350.0),
            ("capacity-stream", 1_100.0),
            ("recovery-completion", 200.0),
            ("capacity-backlog", DEFAULT_RATE),
        ] {
            assert_eq!(limit(&declared, id), Some(bound), "{id}");
        }
        assert!(declared.refusals().is_empty(), "{:?}", declared.refusals());
        let overridden = declare(
            Level::Two,
            &line(&["--rate", "100", "--max-completion-ms", "150"]),
            base(Level::Two),
            Path::new("."),
        )
        .expect("an overridden declaration");
        assert_eq!(limit(&overridden, "capacity-completion"), Some(150.0));
        assert_eq!(limit(&overridden, "capacity-backlog"), Some(100.0));
        assert_eq!(
            overridden.controls["limits"]["slo_overrides"],
            json!(["--max-completion-ms"])
        );
        for refused in [
            &["--rate", "0"][..],
            &["--rate", "-5"],
            &["--max-stream-ms", "inf"],
            &["--max-first-token-ms", "0"],
            &["--rate", "1", "--rate", "2"],
        ] {
            assert!(
                declare(Level::Two, &line(refused), base(Level::Two), Path::new(".")).is_err(),
                "{refused:?}"
            );
        }
    }
}
