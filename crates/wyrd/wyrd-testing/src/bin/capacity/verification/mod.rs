//! The Verification engine's experiments (REQ-015, AC-009), run through
//! `mise run benchmark:l1:verification` and `mise run bench:l2:verification`.
//!
//! Both read as the production journey they measure. One release
//! `wyrd-server` starts through the local-development guide (`migrate`,
//! serve, `setup`). One tenant registers an assertion Verifier, a Model, and
//! one Service per application, and each application loads its pulled Service
//! bundle under that Service's own Card-bound key, so every call is the
//! public Rust client's `observe().verify` or `observe().submit` exactly as
//! production makes it. Evidence comes from client timings, the benchmark snapshot, and
//! the OS's readings of the server process.
//!
//! - **L1, `direct-verify`:** direct `verify` of the canonical Verifier (one
//!   Assert and two Agent checks, each Agent running one `code.run` between
//!   two model exchanges with a local provider) through the Rust, Python and
//!   TypeScript SDKs: closed loop at 1 and 64 clients, and paced at
//!   [`PACED_RATE`] verifications/s from [`PACED_CLIENTS`] clients, the one
//!   load every SDK presents the server equally. The server's verification
//!   overhead (arrival until the result is staged, less the Verifier's own
//!   blocks) p99 must be strictly under 0.5 ms at 64 closed-loop Rust
//!   clients, read from the server histogram's bucket, so an unresolved tail
//!   cannot pass. Every repetition must answer with no
//!   refusal and the expected verdict, and reconcile every acknowledged
//!   call's provider exchanges and code output. Diagnostic `direct-verify-model-c1`
//!   and `-c64`, declared only when `--steps` names one, verify a judge
//!   Verifier whose one Agent check calls a local Gateway deployment backed
//!   by a local provider; they share the correctness criteria and are never
//!   judged by the overhead objective.
//! - **L2, `competing`:** one stateful step whose every repetition runs open
//!   loop capacity → overload → recovery ([`Schedule`]) with direct `verify`
//!   and background `submit` together; every verification stages its result
//!   through Scribe into Bifrost, so evidence writes and the queued backlog
//!   progress in the same window.

mod driver;
pub(crate) mod run;

use driver::Language;
pub(crate) mod workload;

use std::time::Duration;

use serde_json::{Value, json};
use wyrd_server::config::VerificationConfig;
use wyrd_testing::capacity::Population;
use wyrd_testing::experiment::{
    Criterion, Declaration, Derive, Direction, Engine, Level, Metric, Op, Reading, Role, Step,
};

use crate::levels::{Context, Plan};

/// Error type of the server and client steps a workload runs.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed-loop client counts of the L1 sweep, one step each.
pub const CLIENTS: [u64; 2] = [1, 64];

/// Diagnostic Rust closed-loop client counts between [`CLIENTS`], declared
/// only when `--steps` names one, so a run can read throughput and CPU per
/// verification against server concurrency as load rises.
pub const SWEEP_CLIENTS: [u64; 3] = [4, 16, 32];

/// The prefix of the Rust closed-loop step ids, one per client count.
const RUST_STEP: &str = "direct-verify-c";

/// The arrival rate, verifications per second, every language's paced
/// diagnostic step offers through [`PACED_CLIENTS`] clients, so the server
/// sees the same load whichever SDK calls it; below every population's
/// closed-loop capacity on the reference host.
pub const PACED_RATE: u64 = 3_000;

/// The clients a paced step spreads [`PACED_RATE`] across, each calling at
/// its own fixed slots.
pub const PACED_CLIENTS: u64 = 64;

/// The prefix of the diagnostic model step ids, one per [`CLIENTS`] count.
pub const MODEL_STEP: &str = "direct-verify-model-c";

/// The version of the L1 canonical workload each record carries, so an
/// Assert-only record is never read as a comparable canonical baseline.
pub const CANONICAL_VERSION: &str = "canonical-1";

/// The client count the strict overhead objective applies at.
pub const OBJECTIVE_CLIENTS: u64 = 64;

/// Strict server-overhead p99 objective, milliseconds; equality misses.
pub const OVERHEAD_LIMIT_MS: f64 = 0.5;

/// Each direct execution phase the server times beside
/// `verification.direct`, as the metric id segment it fills and its
/// benchmark timing boundary. `outside` is the execution's time outside
/// every phase, and `outside_before`, `outside_between` and `outside_after`
/// split it into the time before the first phase (arrival, middleware,
/// decode), the gaps between phases, and the time after the last phase
/// (result construction and teardown).
pub const PHASE_TIMINGS: [(&str, &str); 10] = [
    ("admit", "verification.direct.admit"),
    ("load", "verification.direct.load"),
    ("setup", "verification.direct.setup"),
    ("prepare", "verification.direct.prepare"),
    ("engine", "verification.direct.engine"),
    ("stage", "verification.direct.stage"),
    ("outside", "verification.direct.outside"),
    ("outside_before", "verification.direct.outside.before"),
    ("outside_between", "verification.direct.outside.between"),
    ("outside_after", "verification.direct.outside.after"),
];

/// The overhead thresholds each L1 repetition counts executions over, as
/// the metric id and nanoseconds.
pub const OVERHEAD_TAILS: [(&str, u64); 4] = [
    ("overhead_over_500us", 500_000),
    ("overhead_over_1ms", 1_000_000),
    ("overhead_over_5ms", 5_000_000),
    ("overhead_over_10ms", 10_000_000),
];

/// The L2 stateful step.
pub const COMPETING: &str = "competing";

/// Applications the L2 open-loop arrivals rotate through.
pub const APPLICATIONS: u64 = 64;

/// Declared default direct `verify` arrivals per second at capacity.
pub const DIRECT_RATE: f64 = 400.0;

/// Declared default background `submit` arrivals per second at capacity.
pub const SUBMIT_RATE: f64 = 100.0;

/// Declared default overload multiple of both capacity rates.
pub const OVERLOAD_FACTOR: f64 = 4.0;

/// Most backlog growth, in seconds of offered capacity work, a capacity or
/// recovery phase may add.
pub const BACKLOG_LIMIT_SECONDS: f64 = 1.0;

/// Longest drain after load stops, seconds.
pub const DRAIN_LIMIT_SECONDS: f64 = 60.0;

/// Longest time after overload ends before the SLOs hold again, seconds;
/// recovery's overhead p99 is read only after it.
pub const RECOVERY_LIMIT: Duration = Duration::from_secs(30);

/// The refusal a deliberate overload may answer with; anywhere else it is an
/// unexpected error.
pub const ADMISSION_REFUSAL: &str = "WYRD_VERIFICATION_429_STAGING_FULL";

/// The L2 rate flags, each a positive number recorded in the controls.
const RATE_FLAGS: [&str; 3] = ["--direct-rate", "--submit-rate", "--overload-factor"];

/// The phases of one L2 repetition, in order.
pub const STAGES: [&str; 3] = ["capacity", "overload", "recovery"];

/// One phase of an L2 repetition: when it starts, how long it runs, and the
/// arrival rates it offers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    /// The phase's name, one of [`STAGES`].
    pub name: &'static str,
    /// Offset from the repetition's start.
    pub start: Duration,
    /// Length.
    pub length: Duration,
    /// Direct `verify` arrivals per second.
    pub direct_rate: f64,
    /// Background `submit` arrivals per second.
    pub submit_rate: f64,
}

/// The capacity → overload → recovery sequence one L2 repetition (or its
/// warmup) runs over its window: capacity 5/12, overload 2/12 at
/// `overload_factor` times both rates, recovery 5/12. The phases tile the
/// window, so a warmup of any length runs the whole sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schedule {
    /// The repetition's window.
    pub window: Duration,
    /// Direct arrivals per second at capacity.
    pub direct_rate: f64,
    /// Background arrivals per second at capacity.
    pub submit_rate: f64,
    /// Overload multiple of both rates.
    pub overload_factor: f64,
}

impl Schedule {
    /// The schedule a declared L2 step runs over `window`.
    ///
    /// # Errors
    ///
    /// Returns a message when the step lacks a positive declared rate.
    pub fn of(step: &Step, window: Duration) -> std::result::Result<Self, String> {
        let rate = |name: &str| {
            step.params
                .get(name)
                .and_then(Value::as_f64)
                .filter(|rate| rate.is_finite() && *rate > 0.0)
                .ok_or_else(|| format!("step `{}` declares no positive `{name}`", step.id))
        };
        Ok(Self {
            window,
            direct_rate: rate("direct_rate")?,
            submit_rate: rate("submit_rate")?,
            overload_factor: rate("overload_factor")?,
        })
    }

    /// The three phases, tiling the window in [`STAGES`] order.
    #[must_use]
    pub fn stages(&self) -> [Stage; 3] {
        let twelfth = self.window / 12;
        let capacity = twelfth * 5;
        let overload = twelfth * 2;
        let stage = |index: usize, start: Duration, length: Duration, factor: f64| Stage {
            name: STAGES[index],
            start,
            length,
            direct_rate: self.direct_rate * factor,
            submit_rate: self.submit_rate * factor,
        };
        [
            stage(0, Duration::ZERO, capacity, 1.0),
            stage(1, capacity, overload, self.overload_factor),
            stage(
                2,
                capacity + overload,
                self.window.saturating_sub(capacity + overload),
                1.0,
            ),
        ]
    }

    /// Arrivals per second offered at capacity, the unit backlog is counted
    /// in.
    #[must_use]
    pub fn offered(&self) -> f64 {
        self.direct_rate + self.submit_rate
    }

    /// Backlog `items` as seconds of offered capacity work.
    #[must_use]
    pub fn backlog_seconds(&self, items: f64) -> Reading {
        Reading::exact(items / self.offered())
    }
}

/// Plans the Verification experiment at the context's level: refuses unknown
/// flags, declares it, and prepares the workload, which starts the server
/// only once the run admits it and profiles its language drivers under
/// `--profile`.
///
/// # Errors
///
/// Returns a message for an unknown flag, an invalid rate, or a malformed
/// `--profile`.
pub fn plan(context: &Context<'_>, base: Declaration) -> std::result::Result<Plan, String> {
    let valued: &[&str] = match context.level {
        Level::One => &[],
        Level::Two => &RATE_FLAGS,
    };
    context.known(valued, &[])?;
    let declaration = declare(context.level, context.arguments, base)?;
    let logs = context.output(Engine::Verification, &declaration.experiment);
    let model = declaration
        .steps
        .iter()
        .any(|step| step.id.starts_with(MODEL_STEP));
    Ok(Plan {
        declaration,
        workload: Box::new(run::Bench::new(
            context.level,
            logs,
            model,
            crate::levels::profile(context.arguments)?,
        )),
    })
}

/// The Verification declaration at `level` over `base`, reading the L2 rate
/// flags from `arguments` (program name first). Pure: it starts nothing.
///
/// # Errors
///
/// Returns a message for an L2 rate flag that is not a positive number.
pub fn declare(
    level: Level,
    arguments: &[String],
    base: Declaration,
) -> std::result::Result<Declaration, String> {
    let mut declaration = match level {
        Level::One => direct(base, models(arguments)?, &sweeps(arguments)?),
        Level::Two => competing(base, arguments)?,
    };
    let fixture = match level {
        Level::One => format!("one tenant: canonical Verifier `{}` of one Assert and two Agent checks over one 2 KiB `context` slot, all three eligible at once; each Agent, routed through local Gateway deployment `{}` with `code.run` granted by tenant Role `bench_code`, makes one model exchange that asks for one `code.run` of a fixed program reading `/inputs/context.json`, then a second whose cited Verdict carries that output as data, so a verification is four governed provider exchanges and two `code.run` operations; the loopback provider answers each conversation from its own messages and keeps a bounded per-arrival ledger the driver reconciles; a Model and one Service per application with its own Card-bound family runtime key; every odd arrival answers the Assert wrong, so both verdicts stay in every window; L2 keeps the Assert-only Verifier", workload::CANONICAL_VERIFIER, workload::JUDGE_DEPLOYMENT),
        Level::Two => "one tenant: an assertion Verifier of four `equals` checks over one 2 KiB `context` slot, a Model, and one Service per application with its own Card-bound family runtime key; every odd arrival answers check `d` wrong, so both verdicts stay in every window".to_owned(),
    };
    declaration
        .controls
        .insert("fixture".to_owned(), json!(fixture));
    let mut server = VerificationConfig::default();
    server
        .resolve_limits()
        .map_err(|error| format!("reading the server's verification limits: {error}"))?;
    let limits = server.limits;
    declaration.controls.insert(
        "limits".to_owned(),
        json!(format!(
            "server defaults on the whole host; no resource quota; verification: {} prewarmed Python workers, {} MiB Python budget per operation, {} ms per code operation; Python sessions wait fairly per tenant for shared process memory and reuse reset workers; workspaces charge shared process memory and scratch; an Agent check holds one workspace for its invocation, and a refused workspace or operation is a counted refusal, never a pool resize",
            limits.monty_min_workers,
            limits.monty_memory_bytes / (1024 * 1024),
            limits.code_timeout.as_millis(),
        )),
    );
    Ok(declaration)
}

/// The metrics both levels read for direct `verify` and its work.
fn work_metrics() -> Vec<Metric> {
    vec![
        Metric::cores(),
        Metric::derived(
            "rate",
            "verifications/s",
            "acknowledged verifications per wall second of the window",
            Direction::Higher,
            Derive::Rate {
                work: "verifications".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::raw(
            "verifications",
            "verifications",
            "acknowledged direct and background calls in the window",
        ),
        Metric::raw(
            "errors",
            "errors",
            "refused calls outside a declared overload admission refusal",
        ),
        Metric::derived(
            "verifications_per_s_per_vcpu",
            "verifications/s/vCPU",
            "(W/T)/P: acknowledged verifications per wall second per provisioned vCPU",
            Direction::Higher,
            Derive::PerVcpu {
                work: "verifications".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "verifications_per_cpu_s",
            "verifications/cpu-s",
            "W/C: acknowledged verifications per server CPU-second used",
            Direction::Higher,
            Derive::PerCpu {
                work: "verifications".to_owned(),
                scale: 1.0,
            },
        ),
        Metric::derived(
            "cpu_us_per_verification",
            "µs/verification",
            "server CPU per acknowledged verification",
            Direction::Lower,
            Derive::CpuPer {
                work: "verifications".to_owned(),
                scale: 1e6,
            },
        ),
        Metric::raw(
            "worker_cpu_seconds",
            "s",
            "user plus system CPU seconds of the server's Python worker children inside the measured window, including workers started or exited in it; apart from server CPU, which stays the server process's own, and from client and provider CPU",
        ),
        Metric::raw(
            "worker_peak_memory_bytes",
            "bytes",
            "sampled peak of the server's Python worker children's summed resident memory from the window's open until its calls drained; apart from server memory",
        ),
        Metric::raw(
            "worker_spawns",
            "workers",
            "Python worker processes the server started inside the measured window",
        ),
        Metric::raw(
            "worker_resets",
            "workers",
            "Python worker sessions reset for reuse inside the measured window",
        ),
        Metric::raw(
            "worker_reaps",
            "workers",
            "exited Python worker processes the server reaped inside the measured window",
        ),
    ]
}

/// The server percentiles of direct `verify` at `scope`, and the client
/// percentiles, of the boundary `client` describes, beside them.
fn latency_metrics(scope: &str, client: &str) -> Vec<Metric> {
    let server = format!(
        "server: a direct execution's start until its result is ready (`verification.direct`, outcome `completed`), {scope}; nearest-rank bucket bounds of the server's timing histograms"
    );
    let client = format!("{client}, {scope}");
    ["p50_ms", "p95_ms", "p99_ms"]
        .into_iter()
        .map(|id| Metric::raw(id, "ms", &server))
        .chain(
            ["client_p50_ms", "client_p95_ms", "client_p99_ms"]
                .into_iter()
                .map(|id| Metric::raw(id, "ms", &client)),
        )
        .collect()
}

/// Whether `--steps` names a [`MODEL_STEP`], so L1 declares the model steps.
///
/// # Errors
///
/// Returns the `--steps` value refusal.
fn models(arguments: &[String]) -> std::result::Result<bool, String> {
    Ok(
        crate::flags::value(arguments, "--steps")?.is_some_and(|steps| {
            steps
                .split(',')
                .any(|step| step.trim().starts_with(MODEL_STEP))
        }),
    )
}

/// The [`SWEEP_CLIENTS`] counts `--steps` names as Rust closed-loop steps.
///
/// # Errors
///
/// Returns the `--steps` value refusal.
fn sweeps(arguments: &[String]) -> std::result::Result<Vec<u64>, String> {
    let named = crate::flags::value(arguments, "--steps")?.unwrap_or_default();
    Ok(SWEEP_CLIENTS
        .into_iter()
        .filter(|clients| {
            named
                .split(',')
                .any(|step| step.trim() == format!("{RUST_STEP}{clients}"))
        })
        .collect())
}

/// The L1 declaration: closed-loop direct `verify` at each of [`CLIENTS`],
/// with `model` the diagnostic model steps at the same counts, and the Rust
/// diagnostic closed-loop steps at each of `sweep` in client order.
fn direct(mut base: Declaration, model: bool, sweep: &[u64]) -> Declaration {
    let populations: Vec<(Language, u64, Option<u64>, String)> = Language::ALL
        .iter()
        .flat_map(|language| {
            let prefix = match language {
                Language::Rust => String::from("direct-verify"),
                other => format!("direct-verify-{}", other.name()),
            };
            let mut closed = CLIENTS.to_vec();
            if *language == Language::Rust {
                closed.extend_from_slice(sweep);
                closed.sort_unstable();
            }
            closed
                .into_iter()
                .map(move |clients| (clients, None))
                .chain([(PACED_CLIENTS, Some(PACED_RATE))])
                .map(move |(clients, rate)| {
                    let id = match rate {
                        None => format!("{prefix}-c{clients}"),
                        Some(rate) => format!("{prefix}-r{rate}"),
                    };
                    (*language, clients, rate, id)
                })
        })
        .collect();
    let steps: Vec<String> = populations.iter().map(|(_, _, _, id)| id.clone()).collect();
    base.experiment = String::from("direct-verify");
    base.question = String::from(
        "Does direct `observe().verify` keep the server's verification overhead p99 strictly under 0.5 ms at 64 closed-loop clients, answering every call with the expected verdict, and what does each verification cost the server per CPU-second at 1 and 64 clients?",
    );
    base.variables.insert("clients".to_owned(), json!(CLIENTS));
    base.variables.insert(
        "languages".to_owned(),
        json!(Language::ALL.map(Language::name)),
    );
    base.controls.insert(
        "processes".to_owned(),
        json!("rust: the clients are bounded async tasks in this controller, which also hosts the local provider on its own runtime; python: one child process calling the `wyrd` package's synchronous `observe.verify` from prestarted threads, one per client; typescript: one child Node process awaiting the `wyrd-ai` package's `observe.verify` in bounded Promise loops, one per client. Each child loads every application's pulled bundle and starts its workers before its first window, and times every call with its own language clock; client CPU and memory are the client process's, and only the Rust client's include the controller and provider"),
    );
    base.controls.insert(
        "workload".to_owned(),
        json!("closed-loop public-client `observe().verify` (direct mode) of the canonical Verifier, one application per client, each sending its next call once the last answers; the client time starts immediately before `verify` and stops when its decoded result returns, with Run and context preparation before it; a second client sample continues until the returned result's compact fields are materialized, and result checks and details reads come after both"),
    );
    base.controls
        .insert("workload_version".to_owned(), json!(CANONICAL_VERSION));
    base.controls.insert(
        "paced".to_owned(),
        json!(format!("diagnostic `-r{PACED_RATE}` steps offer {PACED_RATE} verifications/s from {PACED_CLIENTS} clients in every language: client i's k-th call is due at (i / {PACED_CLIENTS} + k) x {PACED_CLIENTS} / {PACED_RATE} s after the window opens and waits for it, so every SDK presents the server the same arrival rate; a call starts late when its client is still busy, and calls never sent by the window end are missed. Closed-loop `-c` steps hold the client count, not the server load: a client that spends longer in its own SDK keeps fewer executions on the server, so their server readings compare only within a language")),
    );
    base.controls.insert(
        "compare".to_owned(),
        json!({
            "by": "language",
            "metrics": [
                "rate",
                "server_concurrency",
                "overhead_p99_ms",
                "p50_ms",
                "p99_ms",
                "client_p50_ms",
                "client_p99_ms",
            ],
            "notes": {
                "clients=1": "One client, one call at a time: the cost of a call with no load. Compare languages here.",
                "clients=64": "64 clients calling back to back: the server is past saturation, and each language keeps a different number of executions on it (server_concurrency). This shows how the server degrades under overload. Do not compare languages here.",
                format!("clients={PACED_CLIENTS} rate={PACED_RATE}"): format!("{PACED_RATE} verifications/s from every language: the same server load. Compare languages here: server overhead should match, and client latency less server latency is the SDK and transport cost."),
            },
        }),
    );
    base.steps = populations
        .iter()
        .map(|(language, clients, rate, id)| {
            let mut params = json!({ "clients": clients, "op": "verify", "check": "canonical", "language": language.name() });
            if let Some(rate) = rate {
                params["rate"] = json!(rate);
            }
            Step::measured(id, params)
        })
        .collect();
    let mut correct = steps.clone();
    if model {
        for clients in CLIENTS {
            let id = format!("{MODEL_STEP}{clients}");
            base.steps.push(Step::measured(
                &id,
                json!({ "clients": clients, "op": "verify", "check": "model" }),
            ));
            correct.push(id);
        }
        base.controls.insert(
            "model".to_owned(),
            json!(format!("diagnostic model steps: `{}` has one Agent check routed through local Gateway deployment `{}` for one iteration; the deployment calls a loopback plaintext HTTP/1.1 OpenAI-compatible provider in the driver process that answers every call at once with a passing Verdict citing slot `{}` and fixed usage (11 prompt, 4 completion tokens); unexercised: TLS, provider authentication, retries, fallbacks, streaming, tool calls, and remote provider latency, none of which is measured or zero", workload::JUDGE_VERIFIER, workload::JUDGE_DEPLOYMENT, workload::JUDGE_SLOT)),
        );
    }
    base.metrics = work_metrics();
    base.metrics.extend(latency_metrics(
        "every acknowledged call",
        "client: end-to-end client–server–client call-return latency, from immediately before the public `verify` until its decoded result returns, with all elapsed work in between (SDK serialization, transport, server, provider, sandbox, pool and host waits, decoding) and never reduced by any phase; a total with no target",
    ));
    base.metrics.extend(
        ["client_ready_p50_ms", "client_ready_p95_ms", "client_ready_p99_ms"].map(|id| {
            Metric::raw(
                id,
                "ms",
                "client: end-to-end result-ready latency, from the same start until the verdict and every requirement's outcome, citations, and data are materialized from the returned view, so deferred conversion is included and judgment is not; a total with no target, every acknowledged call",
            )
        }),
    );
    base.metrics.extend(run::MATERIALIZE_STATS.map(|(id, _)| {
        Metric::raw(
            id,
            "ms",
            "client diagnostic: per acknowledged call, its result-ready sample less its own call-return sample on the same language clock, the deferred verdict and requirement conversion after `verify` returned; computed per call before the percentile, never from the totals' percentiles",
        )
    }));
    base.metrics.extend([
        Metric::raw(
            "client_cpu_seconds",
            "s",
            "client: user plus system CPU seconds of the process making the step's calls over the measured window; for Rust it is this controller, including the local provider",
        ),
        Metric::raw(
            "client_peak_memory_bytes",
            "bytes",
            "client: sampled peak resident memory of the process making the step's calls from the window's open until its calls drained",
        ),
        Metric::raw(
            "wrong",
            "verifications",
            "direct results whose verdict, requirement outcomes, citations, or Agent data were not the expected ones",
        ),
        Metric::raw(
            "unreconciled",
            "problems",
            "canonical steps: acknowledged calls without exactly one `code.run` turn and one own-output final turn per Agent in the provider ledger, mismatched final turns, conversations nobody acknowledged in an error-free window, a ledger overflow, and sampled executions whose details do not record one clean model-recipient `code.run` per Agent check; inapplicable elsewhere",
        ),
        Metric::raw(
            "lost_writes",
            "writes",
            "server: `outbox_events_lost_total{outbox=\"scribe\"}` added over the window and its drain, every reason including shed audit decisions",
        ),
        Metric::raw(
            "overhead_p99_ms",
            "ms",
            "server: per execution, `verification.direct` elapsed less the union of its own Verifier blocks; nearest-rank bucket bounds of the server's timing histograms",
        ),
        Metric::raw(
            "overhead_mean_ms",
            "ms",
            "server: exact mean of the per-execution residuals",
        ),
        Metric::raw(
            "server_concurrency",
            "executions",
            "server: the time-averaged count of direct executions in progress over the window, the completed executions' summed `verification.direct` elapsed time over the window's seconds (Little's law; executions draining after the window add their whole time); the load the server actually carried, which a closed-loop step's client count does not fix",
        ),
        Metric::raw(
            "arrivals_late",
            "arrivals",
            "client, paced steps: calls that started more than 5 ms behind their due slot, measured immediately before the public `verify`; diagnostic, the send-lateness p99 judges",
        ),
        Metric::raw(
            "arrivals_late_first_ms",
            "ms",
            "client, paced steps: the due time, after the window opened, of the first call that started more than 5 ms late; missing when none did",
        ),
        Metric::raw(
            "arrivals_late_last_ms",
            "ms",
            "client, paced steps: the due time, after the window opened, of the last call that started more than 5 ms late; missing when none did",
        ),
        Metric::raw(
            "arrivals_wake_p99_us",
            "µs",
            "client, paced steps: p99 of how late each caller woke after its due slot, before preparing its call; start lateness well above it is the caller's own preparation",
        ),
        Metric::raw(
            "driver_gc_collections",
            "collections",
            "client, Python steps: garbage collections the driver's interpreter ran during the window (`gc.callbacks`); missing for a driver that reports none",
        ),
        Metric::raw(
            "driver_gc_max_ms",
            "ms",
            "client, Python steps: the longest garbage collection the driver's interpreter ran during the window",
        ),
        Metric::raw(
            "arrivals_late_in_gc",
            "arrivals",
            "client, paced Python steps: late calls whose wait, from due slot to start, overlapped a garbage collection; equal to `arrivals_late` when collections explain every late start",
        ),
        Metric::raw(
            "overhead_max_ms",
            "ms",
            "server: the largest per-execution residual; its histogram bucket's bounds",
        ),
        Metric::raw(
            "verifier_p99_ms",
            "ms",
            "server: the union of an execution's own Verifier blocks; nearest-rank bucket bounds of the server's timing histograms",
        ),
        Metric::raw(
            "verifier_mean_ms",
            "ms",
            "server: exact mean of the Verifier block unions",
        ),
        Metric::raw(
            "server_failures",
            "verifications",
            "server: `verification.direct` timings whose outcome is not `completed`, counted in their own outcome series and never judged",
        ),
    ]);
    base.metrics.extend(OVERHEAD_TAILS.iter().map(|(id, nanos)| {
        Metric::raw(
            id,
            "verifications",
            &format!(
                "server: executions whose residual exceeds {} ms, bounded by the one histogram bucket that straddles it",
                *nanos as f64 / 1e6
            ),
        )
    }));
    for (phase, boundary) in PHASE_TIMINGS {
        for (statistic, what) in [("p99", "nearest-rank p99"), ("max", "largest value")] {
            base.metrics.push(Metric::raw(
                &format!("phase_{phase}_{statistic}_ms"),
                "ms",
                &format!(
                    "server diagnostic: the {what} of each execution's `{boundary}` time, its histogram bucket's bounds; phases overlap (engine holds prepare) and are never summed"
                ),
            ));
        }
    }
    for language in driver::Language::ALL {
        base.metrics.extend(Population::root_metrics(
            language.role(),
            run::client_roots(language),
        ));
    }
    base.metrics.extend(Population::response_byte_metrics(
        Role::Driver,
        run::HTTP_ROOT,
    ));
    base.metrics
        .extend(Population::root_metrics(Role::Driver, &run::FIXTURE_ROOTS));
    for phase in run::GATEWAY_PHASES {
        base.metrics.extend(Metric::diagnostic(
            &format!("gateway.phase.{phase}"),
            &format!("server: `wyrd_gateway_phase_duration_seconds{{phase={phase}}}` of an Agent's in-process model calls, measured window; nested in the Verifier block and never summed"),
        ));
    }
    for wait in crate::gateway::run::WAITS {
        base.metrics.extend(Metric::diagnostic(
            &format!("gateway.wait.{wait}"),
            &format!("server: `wyrd_gateway_wait_seconds{{wait={wait}}}` of an Agent's in-process model calls, measured window; overlaps the phases and is never summed"),
        ));
    }
    for (observation, boundary, _) in run::ENVELOPES {
        base.metrics.extend(Metric::diagnostic(
            observation,
            &format!("server: per completed direct execution, the union of its `{boundary}` intervals, measured window; inclusive (a tool call holds its sandbox operation, a model call its Gateway phases and provider waits), nested in the engine and the Verifier blocks, overlapping across concurrent Agents, and never summed or subtracted; fewer samples than completed executions leaves attribution unfinished"),
        ));
    }
    let objective = format!("direct-verify-c{OBJECTIVE_CLIENTS}");
    base.criteria = vec![
        Criterion::limit(
            "overhead-p99",
            "overhead_p99_ms",
            &[objective],
            Op::Lt,
            OVERHEAD_LIMIT_MS,
        )
        .percentile(99),
        Criterion::every("no-errors", "errors", &correct, Op::Le, 0.0),
        Criterion::every("correct-verdicts", "wrong", &correct, Op::Le, 0.0),
        Criterion::every("no-lost-evidence", "lost_writes", &correct, Op::Le, 0.0),
        Criterion::every("reconciled", "unreconciled", &steps, Op::Le, 0.0),
    ];
    base
}

/// The L2 declaration: one [`COMPETING`] step at the declared rates, with
/// every phase's metrics flattened as `<phase>.<metric>`.
///
/// # Errors
///
/// Returns a message for a rate flag that is not a positive number.
fn competing(
    mut base: Declaration,
    arguments: &[String],
) -> std::result::Result<Declaration, String> {
    let rate = |flag: &str, default: f64| -> std::result::Result<f64, String> {
        crate::flags::value(arguments, flag)?.map_or(Ok(default), |value| {
            value
                .parse::<f64>()
                .ok()
                .filter(|rate| rate.is_finite() && *rate > 0.0)
                .ok_or_else(|| format!("{flag} takes a positive number, got `{value}`"))
        })
    };
    let direct_rate = rate("--direct-rate", DIRECT_RATE)?;
    let submit_rate = rate("--submit-rate", SUBMIT_RATE)?;
    let overload_factor = rate("--overload-factor", OVERLOAD_FACTOR)?;
    let rates = json!({
        "direct_rate": direct_rate,
        "submit_rate": submit_rate,
        "overload_factor": overload_factor,
        "applications": APPLICATIONS,
    });
    let step = vec![COMPETING.to_owned()];
    base.experiment = String::from(COMPETING);
    base.question = String::from(
        "Under open-loop direct `verify` and background `submit` together, with their evidence written through Scribe into Bifrost, does Verification keep direct overhead p99 under 0.5 ms at capacity, bound backlog growth to one second of offered work, drain within 60 s after load stops, and meet its SLOs again within 30 s of a deliberate overload, with no unexpected refusal or wrong verdict?",
    );
    base.variables.insert("rates".to_owned(), rates.clone());
    base.controls.insert("rates".to_owned(), rates.clone());
    base.controls.insert(
        "workload".to_owned(),
        json!(format!(
            "open-loop public-client `observe().verify` and `observe().submit` rotating across {APPLICATIONS} applications; each repetition and its warmup run capacity (5/12 of the window) → overload (2/12, both rates × overload_factor) → recovery (5/12) from a drained queue; every result is staged through Scribe into Bifrost; `{ADMISSION_REFUSAL}` is the declared overload admission refusal"
        )),
    );
    base.steps = vec![Step::measured(COMPETING, rates)];
    base.metrics = work_metrics();
    base.metrics.extend(latency_metrics(
        "executions that ended in the capacity phase",
        "diagnostic only: public-client `verify` call until its judged result; never judged",
    ));
    base.metrics.extend([
        Metric::raw(
            "verifications_after_window",
            "verifications",
            "calls in flight at the window end, acknowledged after it and outside every work-per-CPU metric",
        ),
        Metric::raw(
            "drain_seconds",
            "s",
            "load stopping until the verification queue and the Scribe and Verifier submission outboxes are empty",
        ),
        Metric::raw(
            "evidence_acks",
            "acks",
            "Scribe acknowledgements of staged evidence in the window (bifrost_scribe_ack_seconds count)",
        ),
        Metric::raw(
            "evidence_ack_mean_ms",
            "ms",
            "diagnostic only: exact mean of bifrost_scribe_ack_seconds over the window",
        ),
        Metric::raw(
            "capacity.backlog_growth_seconds",
            "s",
            "queued backlog growth over the capacity phase, in seconds of offered capacity work",
        ),
        Metric::raw(
            "recovery.backlog_growth_seconds",
            "s",
            "queued backlog at the end of recovery less at the repetition's start, in seconds of offered capacity work",
        ),
        Metric::raw(
            "overload.admission_refusals",
            "calls",
            "declared admission refusals during the deliberate overload",
        ),
        Metric::raw(
            "recovery.seconds",
            "s",
            "overload ending until backlog is within one second of offered work of capacity's and the exact direct overhead p99 of the executions that ended in the second before a read is under 0.5 ms",
        ),
    ]);
    for stage in STAGES {
        let overhead = if stage == "recovery" {
            "from 30 s after overload ends to the window's end"
        } else {
            "over the phase"
        };
        base.metrics.extend([
            Metric::raw(
                &format!("{stage}.direct_overhead_p99_ms"),
                "ms",
                &format!(
                    "server: per direct execution that ended {overhead}, `verification.direct` elapsed less the union of its own Verifier blocks; nearest-rank bucket bounds of the server's timing histograms"
                ),
            ),
            Metric::raw(
                &format!("{stage}.direct_p99_ms"),
                "ms",
                "diagnostic only: public-client direct `verify` roundtrip p99 over the phase; never judged",
            ),
            Metric::raw(
                &format!("{stage}.submit_mean_ms"),
                "ms",
                "diagnostic only: exact mean of server `submit` arrival until staged over the phase",
            ),
            Metric::raw(
                &format!("{stage}.queue_wait_mean_ms"),
                "ms",
                "diagnostic only: exact mean of a queued run's wait from claimable until claimed over the phase",
            ),
            Metric::raw(
                &format!("{stage}.backlog_seconds"),
                "s",
                "queued backlog at the phase's end, in seconds of offered capacity work",
            ),
            Metric::raw(
                &format!("{stage}.errors"),
                "calls",
                "refusals over the phase outside the declared overload admission refusal",
            ),
            Metric::raw(
                &format!("{stage}.wrong"),
                "verifications",
                "direct results over the phase whose verdict was not the expected one",
            ),
        ]);
    }
    base.criteria = vec![
        Criterion::limit(
            "capacity-overhead-p99",
            "capacity.direct_overhead_p99_ms",
            &step,
            Op::Lt,
            OVERHEAD_LIMIT_MS,
        )
        .percentile(99),
        Criterion::limit(
            "recovery-overhead-p99",
            "recovery.direct_overhead_p99_ms",
            &step,
            Op::Lt,
            OVERHEAD_LIMIT_MS,
        )
        .percentile(99),
        Criterion::limit(
            "capacity-backlog",
            "capacity.backlog_growth_seconds",
            &step,
            Op::Le,
            BACKLOG_LIMIT_SECONDS,
        ),
        Criterion::limit(
            "recovery-backlog",
            "recovery.backlog_growth_seconds",
            &step,
            Op::Le,
            BACKLOG_LIMIT_SECONDS,
        ),
        Criterion::limit("drain", "drain_seconds", &step, Op::Le, DRAIN_LIMIT_SECONDS),
        Criterion::limit(
            "recovery-time",
            "recovery.seconds",
            &step,
            Op::Le,
            RECOVERY_LIMIT.as_secs_f64(),
        ),
    ];
    for stage in STAGES {
        base.criteria.extend([
            Criterion::every(
                &format!("{stage}-no-errors"),
                &format!("{stage}.errors"),
                &step,
                Op::Le,
                0.0,
            ),
            Criterion::every(
                &format!("{stage}-correct-verdicts"),
                &format!("{stage}.wrong"),
                &step,
                Op::Le,
                0.0,
            ),
        ]);
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use wyrd_testing::experiment::{
        Declaration, Engine, HostRecord, Invocation, Level, Mode, Rule, Source, Validity,
    };

    use super::{COMPETING, OVERHEAD_LIMIT_MS, STAGES, Schedule, declare};

    /// The default L1 populations: Rust, Python, and TypeScript at 1 and
    /// 64 clients.
    const STEPS: [&str; 9] = [
        "direct-verify-c1",
        "direct-verify-c64",
        "direct-verify-r3000",
        "direct-verify-python-c1",
        "direct-verify-python-c64",
        "direct-verify-python-r3000",
        "direct-verify-typescript-c1",
        "direct-verify-typescript-c64",
        "direct-verify-typescript-r3000",
    ];

    /// A clean base declaration at `level` with no release binary.
    pub(super) fn base(level: Level) -> Declaration {
        Declaration {
            engine: Engine::Verification,
            experiment: String::new(),
            level,
            question: String::new(),
            timing: level.timing(),
            validity: Validity::default(),
            variables: BTreeMap::new(),
            controls: BTreeMap::new(),
            metrics: Vec::new(),
            criteria: Vec::new(),
            steps: Vec::new(),
            selection: None,
            source: Source::from_diff("0123456789abcdef".to_owned(), b"", false),
            build: serde_json::Value::Null,
            command: Invocation::new(&[], &[]),
            host: HostRecord {
                os: "linux".to_owned(),
                os_version: "test".to_owned(),
                arch: "x86_64".to_owned(),
                cpu_model: "test".to_owned(),
                logical_cpus: 8,
                memory_bytes: 1 << 34,
                storage: "file://".to_owned(),
                fingerprint: "test".to_owned(),
            },
            refusals: Vec::new(),
            mode: Mode::Full,
        }
    }

    /// The command line `arguments` after the program name.
    fn line(arguments: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(arguments.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// L1 declares the full direct sweep, judges the strict overhead only at
    /// 64 clients with enough p99 samples, keeps correctness in every
    /// repetition, and admits its workload.
    ///
    /// # Panics
    ///
    /// Panics when the declaration drifts from the approved L1 matrix.
    #[test]
    fn level_one_declares_direct_sweep_and_strict_objective() {
        let declaration =
            declare(Level::One, &line(&[]), base(Level::One)).expect("L1 declares without flags");
        let steps: Vec<&str> = declaration
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect();
        assert_eq!(steps, STEPS);
        let expected = [
            ("rust", 1, None),
            ("rust", 64, None),
            ("rust", 64, Some(3_000)),
            ("python", 1, None),
            ("python", 64, None),
            ("python", 64, Some(3_000)),
            ("typescript", 1, None),
            ("typescript", 64, None),
            ("typescript", 64, Some(3_000)),
        ];
        for (step, (language, clients, rate)) in declaration.steps.iter().zip(expected) {
            assert_eq!(step.text("op"), Some("verify"), "{}", step.id);
            assert_eq!(step.text("check"), Some("canonical"), "{}", step.id);
            assert_eq!(step.text("language"), Some(language), "{}", step.id);
            assert_eq!(step.number("clients"), Some(clients), "{}", step.id);
            assert_eq!(step.number("rate"), rate, "{}", step.id);
        }
        let processes = declaration.controls["processes"]
            .as_str()
            .expect("the process model is recorded");
        for named in [
            "prestarted threads",
            "bounded Promise",
            "include the controller and provider",
        ] {
            assert!(processes.contains(named), "{named}: {processes}");
        }
        for id in ["client_cpu_seconds", "client_peak_memory_bytes"] {
            assert!(
                declaration.metrics.iter().any(|metric| metric.id == id),
                "{id} is declared"
            );
        }
        assert_eq!(
            declaration.controls["workload_version"],
            super::CANONICAL_VERSION
        );
        let fixture = declaration.controls["fixture"]
            .as_str()
            .expect("the fixture is recorded");
        for named in [
            super::workload::CANONICAL_VERIFIER,
            "one Assert and two Agent checks",
            "four governed provider exchanges and two `code.run` operations",
            "L2",
        ] {
            assert!(fixture.contains(named), "{named}: {fixture}");
        }
        let limits = declaration.controls["limits"]
            .as_str()
            .expect("the limits are recorded");
        for named in [
            "workspaces",
            "Python workers",
            "wait fairly per tenant for shared process memory",
            "never a pool resize",
        ] {
            assert!(limits.contains(named), "{named}: {limits}");
        }
        assert!(
            !limits.contains("waiters"),
            "no fixed waiter count: {limits}"
        );
        for id in [
            "client_p50_ms",
            "client_p95_ms",
            "client_p99_ms",
            "client_ready_p50_ms",
            "client_ready_p95_ms",
            "client_ready_p99_ms",
        ] {
            let metric = declaration
                .metric(id)
                .unwrap_or_else(|| panic!("total client field {id} is declared"));
            assert!(
                metric.boundary.starts_with("client: end-to-end"),
                "{id} is a total: {}",
                metric.boundary
            );
        }
        assert!(
            declaration
                .metrics
                .iter()
                .all(|metric| !metric.id.contains("adjusted")),
            "no client metric subtracts work"
        );
        assert!(
            declaration
                .criteria
                .iter()
                .all(|criterion| !criterion.metric.starts_with("client")),
            "no client target is judged"
        );
        for (observation, _, _) in super::run::ENVELOPES {
            assert!(
                declaration
                    .metrics
                    .iter()
                    .any(|metric| metric.id.contains(&format!("{observation}."))),
                "{observation} attributes engine time"
            );
        }
        let reconciled = declaration
            .criteria
            .iter()
            .find(|criterion| criterion.id == "reconciled")
            .expect("per-request reconciliation is judged");
        assert_eq!(reconciled.steps, steps);
        assert!(declaration.metric("unreconciled").is_some());
        for diagnostic in ["driver.fixture.upstream.", "gateway.phase.route."] {
            assert!(
                declaration
                    .metrics
                    .iter()
                    .any(|metric| metric.id.contains(diagnostic)),
                "{diagnostic} is declared: every canonical exchange is a governed model call"
            );
        }
        let overhead = declaration
            .criteria
            .iter()
            .find(|criterion| criterion.id == "overhead-p99")
            .expect("the overhead objective is declared");
        assert_eq!(overhead.steps, ["direct-verify-c64"]);
        assert_eq!(overhead.percentile, Some(99));
        assert!(matches!(
            overhead.rule,
            Rule { op: wyrd_testing::experiment::Op::Lt, limit, .. } if (limit - OVERHEAD_LIMIT_MS).abs() < f64::EPSILON
        ));
        for id in ["no-errors", "correct-verdicts"] {
            let criterion = declaration
                .criteria
                .iter()
                .find(|c| c.id == id)
                .expect("declared");
            assert_eq!(criterion.steps, STEPS, "{id} covers every language");
            assert!(matches!(
                criterion.rule,
                Rule {
                    over: wyrd_testing::experiment::Over::Every,
                    ..
                }
            ));
        }
        for id in [
            "rate",
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "errors",
            "verifications_per_cpu_s",
            "cpu_us_per_verification",
        ] {
            assert!(declaration.metric(id).is_some(), "{id} is declared");
        }
        assert_eq!(declaration.refusals(), Vec::new());
    }

    /// A named sweep count adds one Rust closed-loop step in client order
    /// and leaves the default declaration and other languages unchanged.
    ///
    /// # Panics
    ///
    /// Panics when a sweep step is missing, misordered, or not a Rust
    /// closed-loop step.
    #[test]
    fn sweep_steps_are_rust_only_and_declared_when_named() {
        let selected = line(&["--steps", "direct-verify-c16,direct-verify-c4"]);
        let declaration =
            declare(Level::One, &selected, base(Level::One)).expect("L1 declares sweep steps");
        let ids: Vec<&str> = declaration
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect();
        assert_eq!(
            &ids[..5],
            [
                "direct-verify-c1",
                "direct-verify-c4",
                "direct-verify-c16",
                "direct-verify-c64",
                "direct-verify-r3000"
            ]
        );
        assert_eq!(&ids[5..], &STEPS[3..]);
        let c16 = &declaration.steps[2];
        assert_eq!(c16.number("clients"), Some(16));
        assert_eq!(c16.text("language"), Some("rust"));
        assert_eq!(c16.number("rate"), None);
    }

    /// The model steps exist only when `--steps` names one: the default L1
    /// declaration is unchanged, and a model run adds both counts with the
    /// `model` check over the same SDK `verify` flow, records the local
    /// fixture and what it leaves unexercised, owes the provider's answers
    /// as driver diagnostics, keeps the overhead objective on the assertion
    /// step alone, and refuses an undeclared model step.
    ///
    /// # Panics
    ///
    /// Panics when a model step leaks into the default run or is judged by
    /// the overhead objective.
    #[test]
    fn model_steps_declare_the_existing_sdk_flow_and_local_fixture() {
        let default =
            declare(Level::One, &line(&[]), base(Level::One)).expect("L1 declares by default");
        let selected = line(&["--steps", "direct-verify-c1,direct-verify-model-c64"]);
        let mut model =
            declare(Level::One, &selected, base(Level::One)).expect("L1 declares model steps");
        let ids = |declaration: &Declaration| -> Vec<String> {
            declaration
                .steps
                .iter()
                .map(|step| step.id.clone())
                .collect()
        };
        assert_eq!(ids(&default), STEPS);
        assert!(!default.controls.contains_key("model"));
        assert_eq!(
            ids(&model),
            [
                &STEPS[..],
                &["direct-verify-model-c1", "direct-verify-model-c64"]
            ]
            .concat()
        );
        for step in &model.steps[STEPS.len()..] {
            assert_eq!(step.text("check"), Some("model"));
            assert_eq!(step.text("op"), Some("verify"));
        }
        let fixture = model.controls["model"]
            .as_str()
            .expect("the fixture is recorded");
        for named in [
            super::workload::JUDGE_DEPLOYMENT,
            "loopback plaintext HTTP/1.1",
            "unexercised: TLS",
        ] {
            assert!(fixture.contains(named), "{named}: {fixture}");
        }
        assert!(
            model
                .metrics
                .iter()
                .any(|metric| metric.id.contains("driver.fixture.upstream")),
            "the provider's answers are driver diagnostics"
        );
        let criterion = |id: &str| {
            model
                .criteria
                .iter()
                .find(|criterion| criterion.id == id)
                .expect("declared")
                .steps
                .clone()
        };
        assert_eq!(criterion("overhead-p99"), ["direct-verify-c64"]);
        for id in ["no-errors", "correct-verdicts", "no-lost-evidence"] {
            assert_eq!(
                criterion(id).len(),
                STEPS.len() + 2,
                "{id} covers the model steps"
            );
        }
        assert_eq!(model.refusals(), Vec::new());
        crate::levels::select(&mut model, &selected).expect("declared steps select");
        assert_eq!(
            model.selection.as_deref(),
            Some(
                &[
                    "direct-verify-c1".to_owned(),
                    "direct-verify-model-c64".to_owned()
                ][..]
            )
        );
        let unknown = line(&["--steps", "direct-verify-model-c2"]);
        let mut refused = declare(Level::One, &unknown, base(Level::One)).expect("declares");
        assert!(crate::levels::select(&mut refused, &unknown).is_err());
    }

    /// L2 declares one stateful step with every phase's metrics, judges
    /// overhead in capacity and recovery only, bounds backlog, drain, and
    /// recovery, keeps correctness in every phase and repetition, records
    /// rate overrides before execution, and refuses a malformed rate.
    ///
    /// # Panics
    ///
    /// Panics when the declaration drifts from the approved L2 contract.
    #[test]
    fn level_two_declares_phases_and_records_rate_overrides() {
        let declaration = declare(
            Level::Two,
            &line(&["--direct-rate", "250", "--overload-factor", "3"]),
            base(Level::Two),
        )
        .expect("L2 declares with valid rates");
        assert_eq!(declaration.steps.len(), 1);
        assert_eq!(declaration.steps[0].id, COMPETING);
        assert_eq!(declaration.controls["rates"]["direct_rate"], 250.0);
        assert_eq!(declaration.controls["rates"]["submit_rate"], 100.0);
        assert_eq!(declaration.controls["rates"]["overload_factor"], 3.0);
        let ids: Vec<&str> = declaration.criteria.iter().map(|c| c.id.as_str()).collect();
        for id in [
            "capacity-overhead-p99",
            "recovery-overhead-p99",
            "capacity-backlog",
            "recovery-backlog",
            "drain",
            "recovery-time",
        ] {
            assert!(ids.contains(&id), "{id} is declared");
        }
        assert!(!ids.iter().any(|id| id.starts_with("overload-overhead")));
        for stage in STAGES {
            for suffix in [
                "errors",
                "wrong",
                "direct_overhead_p99_ms",
                "backlog_seconds",
            ] {
                assert!(declaration.metric(&format!("{stage}.{suffix}")).is_some());
            }
            assert!(ids.contains(&format!("{stage}-no-errors").as_str()));
        }
        assert_eq!(declaration.refusals(), Vec::new());
        for bad in [
            &["--submit-rate", "0"][..],
            &["--direct-rate", "fast"],
            &["--overload-factor", "NaN"],
        ] {
            assert!(
                declare(Level::Two, &line(bad), base(Level::Two)).is_err(),
                "{bad:?}"
            );
        }
    }

    /// The schedule's phases tile any window in order, overload scales both
    /// rates, and backlog counts in seconds of offered capacity work.
    ///
    /// # Panics
    ///
    /// Panics when the phase arithmetic drifts.
    #[test]
    fn schedule_tiles_the_window_and_counts_backlog_in_offered_seconds() {
        let declaration =
            declare(Level::Two, &line(&[]), base(Level::Two)).expect("L2 declares by default");
        for seconds in [120, 360] {
            let schedule = Schedule::of(&declaration.steps[0], Duration::from_secs(seconds))
                .expect("the step declares its rates");
            let stages = schedule.stages();
            assert_eq!(stages.map(|stage| stage.name), STAGES);
            assert_eq!(stages[0].start, Duration::ZERO);
            for pair in stages.windows(2) {
                assert_eq!(pair[0].start + pair[0].length, pair[1].start);
            }
            assert_eq!(
                stages[2].start + stages[2].length,
                Duration::from_secs(seconds)
            );
            assert!(stages[2].length >= super::RECOVERY_LIMIT);
            assert!((stages[1].direct_rate - 1_600.0).abs() < f64::EPSILON);
            assert!((stages[1].submit_rate - 400.0).abs() < f64::EPSILON);
            assert!((stages[2].direct_rate - 400.0).abs() < f64::EPSILON);
            assert_eq!(schedule.backlog_seconds(500.0).value, Some(1.0));
        }
        let mut step = declaration.steps[0].clone();
        step.params.remove("submit_rate");
        assert!(Schedule::of(&step, Duration::from_secs(360)).is_err());
    }
}
