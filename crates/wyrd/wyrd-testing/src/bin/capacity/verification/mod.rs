//! The Verification engine's experiments (REQ-015, AC-009), run through
//! `mise run bench:l1:verification` and `mise run bench:l2:verification`.
//!
//! Both read as the production journey they measure. One release
//! `wyrd-server` starts through the local-development guide (`migrate`,
//! serve, `setup`). One tenant registers an assertion Verifier, a Model, and
//! one Service per application, and each application loads its pulled Service
//! bundle under that Service's own Card-bound key, so every call is the
//! public Rust client's `observe().verify` or `observe().submit` exactly as
//! production makes it. Evidence comes from client timings, `/metrics`, and
//! the OS's readings of the server process.
//!
//! - **L1, `direct-verify`:** closed-loop direct `verify` at 1 and 64
//!   clients. The server's verification overhead (arrival until the result is
//!   staged, less the Verifier's own blocks) p99 must be strictly under
//!   0.5 ms at 64 clients, read from the server histogram's bucket, so an
//!   unresolved tail cannot pass. Every repetition must answer with no
//!   refusal and the expected verdict.
//! - **L2, `competing`:** one stateful step whose every repetition runs open
//!   loop capacity → overload → recovery ([`Schedule`]) with direct `verify`
//!   and background `submit` together; every verification stages its result
//!   through Scribe into Bifrost, so evidence writes and the queued backlog
//!   progress in the same window.

mod run;
mod workload;

use std::time::Duration;

use serde_json::{Value, json};
use wyrd_testing::experiment::{
    Criterion, Declaration, Derive, Direction, Engine, Level, Metric, Op, Reading, Step,
};

use crate::levels::{Context, Plan};

/// Error type of the server and client steps a workload runs.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed-loop client counts of the L1 sweep, one step each.
pub const CLIENTS: [u64; 2] = [1, 64];

/// The client count the strict overhead objective applies at.
pub const OBJECTIVE_CLIENTS: u64 = 64;

/// Strict server-overhead p99 objective, milliseconds; equality fails.
pub const OVERHEAD_LIMIT_MS: f64 = 0.5;

/// Each direct execution phase the server times beside
/// `verification.direct`, as the metric id segment it fills and its
/// benchmark timing boundary. `outside` is the execution's time outside
/// every phase: HTTP decode, middleware, and waits between phases.
pub const PHASE_TIMINGS: [(&str, &str); 6] = [
    ("admit", "verification.direct.admit"),
    ("load", "verification.direct.load"),
    ("prepare", "verification.direct.prepare"),
    ("engine", "verification.direct.engine"),
    ("stage", "verification.direct.stage"),
    ("outside", "verification.direct.outside"),
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
/// only once the run admits it.
///
/// # Errors
///
/// Returns a message for an unknown flag or an invalid rate.
pub fn plan(context: &Context<'_>, base: Declaration) -> std::result::Result<Plan, String> {
    let valued: &[&str] = match context.level {
        Level::One => &[],
        Level::Two => &RATE_FLAGS,
    };
    context.known(valued, &[])?;
    let declaration = declare(context.level, context.arguments, base)?;
    let logs = context.output(Engine::Verification, &declaration.experiment);
    Ok(Plan {
        declaration,
        workload: Box::new(run::Bench::new(context.level, logs)),
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
        Level::One => direct(base),
        Level::Two => competing(base, arguments)?,
    };
    declaration.controls.insert(
        "fixture".to_owned(),
        json!("one tenant: an assertion Verifier of four `equals` checks over one 2 KiB `context` slot, a Model, and one Service per application with its own Card-bound family runtime key; every odd arrival answers check `d` wrong, so both verdicts stay in every window"),
    );
    declaration.controls.insert(
        "limits".to_owned(),
        json!("server defaults on the whole host; no resource quota"),
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
    ]
}

/// The server percentiles of direct `verify` at `scope`, and the diagnostic
/// client round-trip percentiles beside them.
fn latency_metrics(scope: &str) -> Vec<Metric> {
    let server = format!(
        "server: a direct execution's start until its result is ready (`verification.direct`, outcome `completed`), {scope}; nearest-rank bucket bounds of the server's timing histograms"
    );
    let client = format!(
        "diagnostic only: public-client `verify` call until its judged result, {scope}; never judged"
    );
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

/// The L1 declaration: closed-loop direct `verify` at each of [`CLIENTS`].
fn direct(mut base: Declaration) -> Declaration {
    let steps: Vec<String> = CLIENTS
        .iter()
        .map(|clients| format!("direct-verify-c{clients}"))
        .collect();
    base.experiment = String::from("direct-verify");
    base.question = String::from(
        "Does direct `observe().verify` keep the server's verification overhead p99 strictly under 0.5 ms at 64 closed-loop clients, answering every call with the expected verdict, and what does each verification cost the server per CPU-second at 1 and 64 clients?",
    );
    base.variables.insert("clients".to_owned(), json!(CLIENTS));
    base.controls.insert(
        "workload".to_owned(),
        json!("closed-loop public-client `observe().verify` (direct mode), one application per client, each sending its next call once the last answers"),
    );
    base.steps = CLIENTS
        .iter()
        .zip(&steps)
        .map(|(clients, id)| Step::measured(id, json!({ "clients": clients, "op": "verify" })))
        .collect();
    base.metrics = work_metrics();
    base.metrics
        .extend(latency_metrics("every acknowledged call"));
    base.metrics.extend([
        Metric::raw(
            "wrong",
            "verifications",
            "direct results whose verdict was not the expected one",
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
        Criterion::every("no-errors", "errors", &steps, Op::Le, 0.0),
        Criterion::every("correct-verdicts", "wrong", &steps, Op::Le, 0.0),
        Criterion::every("no-lost-evidence", "lost_writes", &steps, Op::Le, 0.0),
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
        assert_eq!(steps, ["direct-verify-c1", "direct-verify-c64"]);
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
            assert_eq!(criterion.steps.len(), super::CLIENTS.len());
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
