//! Wyrd's opt-in benchmark experiments, run through `mise`:
//!
//! - `l1 <engine>`: isolated closed-loop work for `bifrost` ([`query`]),
//!   `verification` ([`verification`]), or `gateway` ([`gateway`]), through
//!   `mise run bench:l1:<engine>`; `l1` alone runs all three in order,
//!   through `mise run bench:l1`. `l1 bifrost
//!   <writes-parquet|hot|parquet>` runs one Bifrost experiment and `l1
//!   bifrost` alone all three in order.
//! - `l2 <engine>`: competing open-loop work through capacity, overload, and
//!   recovery, through `mise run bench:l2:<engine>`.
//! - `report <record.json>`: regenerates a persisted record's report.
//!
//! Every experiment shares one lifecycle ([`levels`] over
//! `wyrd_testing::experiment`): a sealed declaration, quiet-host checks,
//! warmup and three repetitions, judgment, an append-only record under
//! `benchmarks/<engine>/<experiment>/`, and exit 0 (PASS), 1 (FAIL), or 2
//! (INVALID). `--profile` adds optional samply captures, `--allow-dirty`
//! records a dirty source, `--steps` selects declared steps, and `--smoke`
//! runs the same plan at smoke scale under `target/benchmarks/smoke/`.
//! `--qualify-baseline` runs a full clean L1 that omits relative criteria
//! to qualify references, and `reference <record.json>...` prints the
//! reviewed-reference TOML two such records establish. A
//! full run first runs that smoke as an isolated prepass and stops when it
//! fails; `mise run bench:smoke` runs every level's smoke.

mod client_queue;
mod flags;
mod gateway;
mod levels;
mod query;
mod verification;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wyrd_testing::experiment::{Engine, Level};

use crate::levels::{Context, Plan};

/// Dispatches the level command the arguments name.
fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("capacity: cannot start the Tokio runtime: {error}");
            return ExitCode::from(2);
        }
    };
    wyrd_testing::capacity::install_tracing();
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ExitCode::from(runtime.block_on(Box::pin(dispatch(&arguments, &root))))
}

/// Runs the command in `arguments` (program name first) against the
/// repository at `root` and returns its exit code; a usage error exits 2
/// before any server starts.
async fn dispatch(arguments: &[String], root: &Path) -> u8 {
    let command = arguments.get(1).map(String::as_str);
    let engine = arguments.get(2).and_then(|name| engine(name));
    let rest = |skip: usize| -> Vec<String> {
        arguments
            .iter()
            .take(1)
            .chain(arguments.iter().skip(skip))
            .cloned()
            .collect()
    };
    let bare = |index: usize| {
        arguments
            .get(index)
            .is_none_or(|next| next.starts_with("--"))
    };
    let group = match arguments.get(3).map(String::as_str) {
        _ if bare(3) => Some((3, query::Experiment::ALL.to_vec())),
        _ => None,
    };
    match (command, engine, group) {
        (Some("l1"), Some(Engine::Bifrost), Some((skip, experiments))) => {
            let shared = rest(skip);
            if let Err(message) = flags::known(&shared, &levels::VALUED[..0], &levels::SWITCHES) {
                eprintln!(
                    "capacity l1 bifrost: {message}; running several experiments takes only --profile, --allow-dirty, --smoke and --qualify-baseline"
                );
                return 2;
            }
            let forwarded: Vec<String> = shared.into_iter().skip(1).collect();
            levels::spawn_experiments(root, &experiments, &forwarded)
        }
        (Some("l1"), Some(engine), _) => Box::pin(level(Level::One, engine, &rest(3), root)).await,
        (Some("l2"), Some(engine), _) => Box::pin(level(Level::Two, engine, &rest(3), root)).await,
        (Some("l1"), None, _) if bare(2) => {
            let shared = rest(2);
            if let Err(message) = flags::known(&shared, &levels::VALUED[..0], &levels::SWITCHES) {
                eprintln!(
                    "capacity l1: {message}; the aggregate takes only --profile, --allow-dirty, --smoke and --qualify-baseline"
                );
                return 2;
            }
            levels::aggregate(&shared, |engine, forwarded| {
                levels::spawn_engine(root, engine, forwarded)
            })
        }
        (Some("reference"), _, _) if arguments.len() >= 3 => {
            match levels::references(&arguments[2..]) {
                Ok(toml) => {
                    print!("{toml}");
                    0
                }
                Err(message) => {
                    eprintln!("capacity reference: {message}");
                    2
                }
            }
        }
        (Some("report"), _, _) if arguments.len() == 3 => {
            match levels::report(Path::new(&arguments[2])) {
                Ok(report) => {
                    println!("{report}");
                    0
                }
                Err(message) => {
                    eprintln!("capacity report: {message}");
                    2
                }
            }
        }
        _ => {
            eprintln!(
                "capacity: expected `l1 [bifrost [writes-parquet|hot|parquet]|verification|gateway]`, `l2 <bifrost|verification|gateway>`, `report <record.json>`, or `reference <record.json>...`, got {:?}",
                &arguments[1.min(arguments.len())..]
            );
            2
        }
    }
}

/// The engine `name` selects.
fn engine(name: &str) -> Option<Engine> {
    levels::ENGINES
        .into_iter()
        .find(|engine| engine.name() == name)
}

/// Plans `engine` at `level` from `arguments` and runs it: a `--smoke`
/// as its smoke, and anything else only after its smoke prepass, which a
/// declaration already refused skips.
async fn level(level: Level, engine: Engine, arguments: &[String], root: &Path) -> u8 {
    let context = Context {
        level,
        arguments,
        root,
    };
    let planned = plan(engine, &context).and_then(|mut plan| {
        levels::select(&mut plan.declaration, arguments)?;
        Ok(plan)
    });
    match planned {
        Ok(mut plan) => {
            if context.smoke() {
                levels::smoke(&mut plan.declaration);
            }
            levels::compare(&mut plan.declaration, root);
            if !context.smoke() && plan.declaration.refusals().is_empty() {
                plan.declaration.prepass =
                    Some(levels::prepass(root, &plan.declaration, arguments));
            }
            let profile = arguments.iter().any(|argument| argument == "--profile");
            Box::pin(levels::execute(plan, root, profile)).await
        }
        Err(message) => {
            eprintln!("capacity l{} {}: {message}", u8::from(level), engine.name());
            2
        }
    }
}

/// The engine's plan at the context's level.
fn plan(engine: Engine, context: &Context<'_>) -> Result<Plan, String> {
    let base = levels::base(engine, context)?;
    match engine {
        Engine::Bifrost => query::plan(context, base),
        Engine::Verification => verification::plan(context, base),
        Engine::Gateway => gateway::plan(context, base),
    }
}

#[cfg(test)]
mod tests {
    pub mod pg_tests;

    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use wyrd_testing::experiment::{
        Arrivals, Declaration, Derive, Engine, ExperimentError, HostRecord, Level, Mode, Op,
        Options, Phase, Reading, Record, Repetition, Rule, Source, Status, Step, StepKind, Verdict,
        Workload, run,
    };
    use wyrd_testing::release_server::{HostLoad, PEAK_SAMPLE_INTERVAL_MS};

    use super::dispatch;
    use crate::levels::{self, Context};
    use crate::{gateway, query, verification};

    /// The host the eligible Bifrost baselines were measured on.
    const BASELINE_HOST: &str = "AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux";

    /// Another host, whose comparisons are advisory.
    const OTHER_HOST: &str = "Apple M4 Max, 16 CPU / 64 GiB, macos";

    /// Adjusts a repetition the scripted workload returns.
    type Script = Box<dyn FnMut(&Step, Phase, &mut Repetition) + Send>;

    /// The repository root that holds `benchmarks/`.
    fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// A command line of `rest` after the program name.
    fn line(rest: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(rest.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// `engine`'s real declaration at `level` from `rest` on the host
    /// `fingerprint`, over a clean source and a release build, reading
    /// `root`.
    ///
    /// # Panics
    ///
    /// Panics when the engine refuses the arguments.
    fn declare(
        engine: Engine,
        level: Level,
        rest: &[&str],
        fingerprint: &str,
        root: &Path,
    ) -> Declaration {
        let arguments = line(rest);
        let context = Context {
            level,
            arguments: &arguments,
            root,
        };
        let base = levels::assemble(
            engine,
            &context,
            Source::from_diff(
                "0123456789abcdef0123456789abcdef01234567".to_owned(),
                b"",
                false,
            ),
            json!({ "profile": "release", "rustflags": "-C force-frame-pointers=yes", "compiler": "rustc test" }),
            HostRecord {
                os: "linux".to_owned(),
                os_version: "test".to_owned(),
                arch: "x86_64".to_owned(),
                cpu_model: "test".to_owned(),
                logical_cpus: 32,
                memory_bytes: 1 << 36,
                storage: "file:// test store".to_owned(),
                fingerprint: fingerprint.to_owned(),
            },
            &[],
        );
        let mut declaration = match engine {
            Engine::Bifrost => query::declare(
                (level == Level::One).then(|| query::experiment(&arguments).expect("experiment")),
                &arguments,
                base,
                root,
            ),
            Engine::Verification => verification::declare(level, &arguments, base, root),
            Engine::Gateway => gateway::declare(level, &arguments, base, root),
        }
        .expect("engine declares");
        levels::select(&mut declaration, &arguments).expect("steps select");
        declaration
    }

    /// A repetition of `step` that meets every declared absolute limit, at
    /// its inclusive boundary where it has one: every raw metric reads 1
    /// over 2,000 samples, half a server core is used, and open-loop
    /// arrivals are all on time.
    fn passing(declaration: &Declaration, step: &Step, window: Duration) -> Repetition {
        let seconds = window.as_secs_f64();
        let mut repetition = Repetition {
            seconds,
            processes: vec!["wyrd-server".to_owned()],
            server_cpu_seconds: Some(seconds / 2.0),
            peak_memory_bytes: Some(1 << 30),
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: Some(seconds / 10.0),
            arrivals: (declaration.level == Level::Two).then_some(Arrivals {
                scheduled: 10_000,
                sent: 10_000,
                completed: 10_000,
                missed: 0,
                lateness_p99_us: Some(100.0),
            }),
            ..Repetition::default()
        };
        for metric in declaration
            .metrics
            .iter()
            .filter(|metric| metric.derive.is_none())
        {
            repetition.set(&metric.id, Reading::exact(1.0).with_samples(2_000));
        }
        for criterion in &declaration.criteria {
            let raw = declaration
                .metric(&criterion.metric)
                .is_some_and(|metric| metric.derive.is_none());
            if let Rule::Limit { op, limit, .. } = criterion.rule
                && raw
                && criterion.steps.contains(&step.id)
            {
                let value = match op {
                    Op::Lt => limit - 0.1,
                    Op::Gt => limit + 1.0,
                    Op::Le | Op::Ge => limit,
                };
                repetition.set(&criterion.metric, Reading::exact(value).with_samples(2_000));
            }
        }
        repetition
    }

    /// A workload that measures [`passing`] repetitions adjusted by its
    /// script, in paused time, and remembers every call.
    struct Scripted {
        /// The declaration measured.
        declaration: Declaration,
        /// The adjustment of each repetition.
        script: Script,
        /// Each call's step and phase.
        calls: Arc<Mutex<Vec<(String, Phase)>>>,
    }

    #[async_trait]
    impl Workload for Scripted {
        async fn prepare(&mut self) -> Result<(), ExperimentError> {
            Ok(())
        }

        fn processes(&self) -> Vec<u32> {
            Vec::new()
        }

        async fn measure(
            &mut self,
            step: &Step,
            phase: Phase,
            window: Duration,
        ) -> Result<Repetition, ExperimentError> {
            tokio::time::sleep(window).await;
            self.calls
                .lock()
                .expect("calls")
                .push((step.id.clone(), phase));
            let mut repetition = passing(&self.declaration, step, window);
            (self.script)(step, phase, &mut repetition);
            Ok(repetition)
        }

        async fn finish(&mut self) -> Result<(), ExperimentError> {
            Ok(())
        }
    }

    /// Runs `declaration` through the shared lifecycle on a quiet host with
    /// `script`, returning the record and the workload's calls.
    async fn execute(declaration: Declaration, script: Script) -> (Record, Vec<(String, Phase)>) {
        let calls = Arc::default();
        let mut workload = Scripted {
            declaration: declaration.clone(),
            script,
            calls: Arc::clone(&calls),
        };
        let artifacts = tempfile::tempdir().expect("artifacts");
        let options = Options {
            artifacts: artifacts.path().to_path_buf(),
            profiler: None,
            interrupt: CancellationToken::new(),
        };
        let quiet = |_| {
            std::future::ready(Some(HostLoad {
                logical_cpus: 32,
                seconds: 10.0,
                other_cores: 1.0,
                busiest: Vec::new(),
            }))
        };
        let record = run(declaration.seal(), &mut workload, quiet, &options).await;
        let calls = calls.lock().expect("calls").clone();
        (record, calls)
    }

    /// The verdict of criterion `id` on `step`, or `None` when it did not
    /// judge there or only advised.
    fn judged(record: &Record, step: &str, id: &str) -> Option<Verdict> {
        record
            .steps
            .iter()
            .find(|candidate| candidate.id == step)
            .and_then(|step| {
                step.criteria
                    .iter()
                    .find(|judgment| judgment.criterion == id)
            })
            .and_then(|judgment| judgment.verdict)
    }

    /// The steps criterion `id` of `declaration` applies to.
    fn applies(declaration: &Declaration, id: &str) -> Vec<String> {
        declaration
            .criteria
            .iter()
            .find(|criterion| criterion.id == id)
            .map(|criterion| criterion.steps.clone())
            .unwrap_or_default()
    }

    /// The measured and derived step ids of `declaration`.
    fn steps(declaration: &Declaration, measured: bool) -> Vec<String> {
        declaration
            .steps
            .iter()
            .filter(|step| (step.kind == StepKind::Measured) == measured)
            .map(|step| step.id.clone())
            .collect()
    }

    /// `prefix-c<clients>` for each client count.
    fn sweep(prefix: &str, separator: &str, clients: &[u64]) -> Vec<String> {
        clients
            .iter()
            .map(|clients| format!("{prefix}{separator}c{clients}"))
            .collect()
    }

    /// The Bifrost L1 script: rows per server CPU-second rise to a peak at
    /// 64 clients, native twice spans and spans twice logs, and every OTLP
    /// step sends 40 wire MiB per server CPU-second; the point lookup at 64
    /// clients reads `point_p99_ms`.
    fn bifrost_script(point_p99_ms: f64) -> Script {
        Box::new(move |step, _, repetition| {
            let cpu = repetition.server_cpu_seconds.expect("cpu");
            let curve = match step.number("clients") {
                Some(1) => 100_000.0,
                _ => 300_000.0,
            };
            let factor = match step.text("op") {
                Some("native-write") => 2.0,
                Some("otlp-spans") => 1.0,
                Some("otlp-logs") => 0.5,
                _ => {
                    if step.id == "point-c64" {
                        repetition.set("p99_ms", Reading::exact(point_p99_ms).with_samples(2_000));
                    }
                    return;
                }
            };
            repetition.set("rows", Reading::exact(curve * factor * cpu));
            if factor <= 1.0 {
                repetition.set("wire_bytes", Reading::exact(40.0 * 1_048_576.0 * cpu));
            }
        })
    }

    /// V10: each engine's default L1 declaration is complete, uses the
    /// shared L1 windows, applies fixed 64-client SLOs only at 64 clients,
    /// picks sanity-floor peaks from the measured sweep, snapshots baselines
    /// by host and fixture, and records a selected subset's omissions.
    ///
    /// # Panics
    ///
    /// Panics when a declaration or its judgment departs from the matrix.
    #[tokio::test(start_paused = true)]
    async fn level_one_declares_complete_engine_coverage() {
        let root = repository();
        let timing = Level::One.timing();
        assert_eq!(
            (
                timing.warmup_seconds,
                timing.repetition_seconds,
                timing.repetitions,
                timing.budget_seconds
            ),
            (5, 5, 3, 300)
        );

        let experiment = |experiment: query::Experiment, rest: &[&str], host: &str| {
            let rest: Vec<&str> = std::iter::once(experiment.name())
                .chain(rest.iter().copied())
                .collect();
            declare(Engine::Bifrost, Level::One, &rest, host, &root)
        };
        assert_eq!(
            query::Experiment::ALL.map(query::Experiment::name),
            ["writes-parquet", "hot", "parquet"],
            "a complete Bifrost L1 suite is three records"
        );
        assert_eq!(
            query::Experiment::parse("writes-hot"),
            None,
            "the retired hot-write experiment has no alias"
        );
        let writes = experiment(query::Experiment::WritesParquet, &[], BASELINE_HOST);
        let hot = experiment(query::Experiment::Hot, &[], BASELINE_HOST);
        let parquet = experiment(query::Experiment::Parquet, &[], BASELINE_HOST);
        for bifrost in [&writes, &hot, &parquet] {
            assert!(bifrost.refusals().is_empty(), "{:?}", bifrost.refusals());
            assert_eq!(bifrost.timing, timing, "every L1 experiment fits 5 minutes");
            for control in ["fixture", "limits", "workload", "timing"] {
                assert!(bifrost.controls.contains_key(control), "{control}");
            }
            assert!(
                bifrost.controls["timing"]["timer_resolution_ns"]
                    .as_u64()
                    .is_some_and(|ns| ns > 0),
                "the measured timer resolution is a declared control"
            );
            assert!(
                !bifrost
                    .criteria
                    .iter()
                    .any(|criterion| matches!(criterion.rule, Rule::Unusable { .. })),
                "every declared baseline is in the approved data"
            );
        }
        for metric in [
            "rows_per_s_per_vcpu",
            "rows_per_cpu_s",
            "mib_per_s_per_vcpu",
            "mib_per_cpu_s",
            "memory_per_mib_s",
            "published_files",
            "memtable_backlog_mib",
            "staged_backlog_mib",
        ] {
            assert!(writes.metric(metric).is_some(), "{metric}");
        }
        for metric in [
            "queries_per_s_per_vcpu",
            "queries_per_cpu_s",
            "span_shredding_faults",
            "storage_ratio.otlp-spans",
        ] {
            assert!(parquet.metric(metric).is_some(), "{metric}");
        }
        let mut measured = Vec::new();
        for op in query::WriteOp::ALL {
            measured.extend(sweep(op.name(), "-", &query::PAIR));
        }
        assert_eq!(steps(&writes, true), measured);
        assert_eq!(
            steps(&writes, false),
            ["otlp-spans-peak", "otlp-logs-peak", "native-write-peak"]
        );
        let windows = |declaration: &Declaration| -> Vec<(String, u64)> {
            declaration
                .steps
                .iter()
                .filter(|step| step.kind == StepKind::Measured)
                .map(|step| {
                    (
                        step.id.clone(),
                        declaration.step_timing(step).repetition_seconds,
                    )
                })
                .filter(|(_, seconds)| *seconds != timing.repetition_seconds)
                .collect()
        };
        assert_eq!(
            windows(&writes),
            [
                ("otlp-spans-c1".to_owned(), 10),
                ("native-write-c1".to_owned(), 10),
                ("native-write-c64".to_owned(), 10)
            ],
            "only steps a 5 s window leaves short of their sample floor declare 10 s"
        );
        assert!(windows(&hot).is_empty());
        assert_eq!(
            windows(&parquet),
            [
                ("small-aggregate-c1".to_owned(), 10),
                ("span-trace-c1".to_owned(), 10)
            ]
        );
        assert_eq!(
            steps(&hot, true),
            [
                "point-c1",
                "point-c64",
                "small-aggregate-c1",
                "span-shredded-c1"
            ]
        );
        assert_eq!(steps(&hot, false), ["point-scaling"]);
        let reads = [
            "point-c1",
            "point-c64",
            "small-aggregate-c1",
            "full-scan-c1",
            "span-trace-c1",
            "span-trace-c64",
            "span-shredded-c1",
            "span-long-tail-c1",
        ];
        assert_eq!(steps(&parquet, true), reads);
        assert_eq!(
            steps(&parquet, false),
            ["point-scaling", "span-trace-scaling"]
        );
        assert!(
            writes.controls["fixture"]["native"]
                .as_str()
                .is_some_and(|fixture| fixture.starts_with("native-2m"))
        );
        for (declaration, hot_settings) in [(&hot, true), (&writes, false), (&parquet, false)] {
            assert_eq!(
                declaration.controls["limits"]
                    .as_str()
                    .is_some_and(|limits| limits.contains("WYRD_MAX_FILE_SIZE_IN_MEMORY=16384")),
                hot_settings,
                "{} keeps every row in a memtable only under the hot settings",
                declaration.experiment
            );
        }
        assert!(
            writes.controls["limits"]
                .as_str()
                .is_some_and(|limits| limits.contains("WYRD_MAX_FILE_RETENTION_TIME=30")),
            "writes-parquet runs the production rotation with a 30 s retention"
        );
        assert!(applies(&writes, "point-p99-slo").is_empty());
        for bifrost in [&hot, &parquet] {
            assert_eq!(applies(bifrost, "point-p99-slo"), ["point-c64"]);
            let slo = bifrost
                .criteria
                .iter()
                .find(|criterion| criterion.id == "point-p99-slo")
                .expect("point SLO");
            assert_eq!(slo.percentile, Some(99));
            assert!(
                matches!(slo.rule, Rule::Limit { op: Op::Lt, limit, .. } if limit == query::POINT_P99_MS)
            );
        }
        assert_eq!(applies(&parquet, "span-shredding"), ["span-shredded-c1"]);
        assert_eq!(applies(&parquet, "span-pruning"), ["span-trace-c1"]);
        assert_eq!(
            applies(&parquet, "scan-facts.scanned_bytes_per_query"),
            reads
        );
        assert!(applies(&hot, "scan-facts.scanned_bytes_per_query").is_empty());
        let floors: Vec<(&str, &str, f64)> = writes
            .criteria
            .iter()
            .filter_map(|criterion| match criterion.rule {
                Rule::Limit {
                    op: Op::Ge, limit, ..
                } if !criterion.metric.contains('.') => Some((
                    criterion.steps[0].as_str(),
                    criterion.metric.as_str(),
                    limit,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            floors,
            [
                ("native-write-peak", "rows_per_s", 200_000.0),
                ("otlp-spans-peak", "wire_mib_per_cpu_s", 30.0),
                ("otlp-logs-peak", "wire_mib_per_cpu_s", 30.0),
            ],
            "only the two approved sanity floors are absolute throughput limits"
        );
        let baseline = |declaration: &Declaration, id: &str| {
            declaration
                .criteria
                .iter()
                .find(|criterion| criterion.id == format!("baseline.{id}"))
                .map(|criterion| criterion.rule.clone())
        };
        match baseline(&parquet, "standard-parquet.selective.c64.cpu-us-per-query") {
            Some(Rule::Advisory { baseline, reason }) => {
                assert!((baseline.value - 3_416.199).abs() < 1e-9);
                assert!(
                    (baseline.converted - 3.416_199).abs() < 1e-9,
                    "µs/query in ms/query"
                );
                assert_eq!(baseline.fixture, "standard-10m-parquet");
                assert!(reason.contains("no method identity"), "{reason}");
            }
            other => panic!("a same-host historical baseline only advises: {other:?}"),
        }
        assert!(baseline(&hot, "standard-hot.small-aggregate.c1.queries-per-cpu-s").is_some());
        assert!(
            parquet
                .criteria
                .iter()
                .filter(|criterion| criterion.id.starts_with("baseline."))
                .all(|criterion| !criterion.steps[0].starts_with("span-")),
            "no span read has an eligible baseline"
        );
        assert!(
            matches!(
                baseline(&writes, "native-writes.write.c64.rows-per-cpu-s"),
                Some(Rule::Advisory { .. })
            ),
            "the 10M native baseline cannot judge the 2M fixture"
        );
        assert!(
            matches!(
                baseline(
                    &writes,
                    "otlp-spans-writes.otlp-spans.c64.otlp-wire-mib-per-cpu-s"
                ),
                Some(Rule::Advisory { .. })
            ),
            "historical write baselines predate the method identity"
        );
        let compared = |declaration: &Declaration| -> Vec<(String, String, Option<u8>)> {
            declaration
                .references
                .iter()
                .map(|reference| {
                    (
                        reference.step.clone(),
                        reference.metric.clone(),
                        reference.percentile,
                    )
                })
                .collect()
        };
        for declaration in [&writes, &hot, &parquet] {
            for step in declaration
                .steps
                .iter()
                .filter(|step| step.kind == StepKind::Measured)
            {
                let metrics: Vec<(String, Option<u8>)> = compared(declaration)
                    .into_iter()
                    .filter(|(id, _, _)| *id == step.id)
                    .map(|(_, metric, percentile)| (metric, percentile))
                    .collect();
                let latency = metrics
                    .iter()
                    .filter(|(_, percentile)| percentile.is_some());
                assert_eq!(
                    latency.count(),
                    usize::from(step.id != "full-scan-c1"),
                    "{} declares one latency reference; full-scan latency stays descriptive",
                    step.id
                );
                for prefix in ["cpu_", "_per_cpu_s", "_per_s_per_vcpu"] {
                    assert!(
                        metrics
                            .iter()
                            .any(|(metric, _)| metric.starts_with(prefix)
                                || metric.ends_with(prefix)),
                        "{} compares {prefix}",
                        step.id
                    );
                }
            }
        }
        assert!(compared(&writes).contains(&(
            "otlp-logs-c1".to_owned(),
            "p50_ms".to_owned(),
            Some(50)
        )));
        assert!(compared(&parquet).contains(&(
            "point-c64".to_owned(),
            "p99_ms".to_owned(),
            Some(99)
        )));
        let elsewhere = experiment(query::Experiment::Parquet, &[], OTHER_HOST);
        assert!(
            matches!(
                baseline(
                    &elsewhere,
                    "standard-parquet.selective.c64.cpu-us-per-query"
                ),
                Some(Rule::Advisory { .. })
            ),
            "a cross-host comparison is advisory"
        );

        let (record, calls) = execute(writes.clone(), bifrost_script(4.9)).await;
        assert!(
            record.validity.reasons.is_empty(),
            "{:?}",
            record.validity.reasons
        );
        assert_eq!(
            calls.len(),
            measured.len() * 4,
            "a warmup and three repetitions per step"
        );
        assert!(record.elapsed_seconds <= 300.0);
        let peak = record
            .steps
            .iter()
            .find(|step| step.id == "native-write-peak")
            .expect("native peak");
        assert_eq!(peak.status, Status::Derived);
        assert_eq!(peak.params.get("peak_of"), Some(&json!("native-write-c64")));
        assert_eq!(peak.params.get("clients"), Some(&json!(64)));
        assert_eq!(
            judged(&record, "native-write-peak", "native-peak-floor"),
            Some(Verdict::Pass)
        );
        assert_eq!(
            judged(&record, "otlp-logs-peak", "otlp-logs-peak-floor"),
            Some(Verdict::Pass)
        );
        let (record, calls) = execute(parquet.clone(), bifrost_script(4.9)).await;
        assert!(
            record.validity.reasons.is_empty(),
            "{:?}",
            record.validity.reasons
        );
        assert_eq!(calls.len(), reads.len() * 4);
        assert_eq!(
            judged(&record, "point-c64", "point-p99-slo"),
            Some(Verdict::Pass)
        );
        assert_eq!(
            judged(&record, "point-c1", "point-p99-slo"),
            None,
            "64-client SLO only at 64"
        );
        assert_eq!(
            judged(&record, "span-shredded-c1", "span-shredding"),
            Some(Verdict::Pass)
        );
        assert_eq!(
            judged(&record, "span-trace-c1", "span-pruning"),
            Some(Verdict::Pass)
        );
        let (strict, _) = execute(parquet.clone(), bifrost_script(query::POINT_P99_MS)).await;
        assert_eq!(
            judged(&strict, "point-c64", "point-p99-slo"),
            Some(Verdict::Fail),
            "equality fails"
        );
        let unscanned: Script = Box::new(|step, _, repetition| {
            if step.id == "span-trace-c64" {
                repetition.set(
                    "scanned_bytes_per_query",
                    Reading::missing("no scan on the query's trace"),
                );
            }
        });
        let (unscanned, _) = execute(parquet.clone(), unscanned).await;
        assert_eq!(
            unscanned.verdict,
            Verdict::Invalid,
            "a settled query without its scan facts is INVALID evidence"
        );

        let subset = experiment(
            query::Experiment::Parquet,
            &["--steps", "point-c64,span-trace-c1"],
            BASELINE_HOST,
        );
        assert_eq!(
            subset.selection,
            Some(vec!["point-c64".to_owned(), "span-trace-c1".to_owned()])
        );
        let (partial, calls) = execute(subset, bifrost_script(4.9)).await;
        assert_eq!(calls.len(), 8);
        assert_eq!(partial.controls.selection.as_ref().map(Vec::len), Some(2));
        let omitted = partial
            .steps
            .iter()
            .filter(|step| step.status == Status::Omitted)
            .count();
        assert_eq!(
            omitted,
            partial.steps.len() - 2,
            "every unselected step, and every derived step over an incomplete sweep, is recorded omitted"
        );
        assert!(partial.render().contains("omitted by selection"));
        let mut unknown = parquet.clone();
        assert!(levels::select(&mut unknown, &line(&["--steps", "point-c2"])).is_err());

        let verification = declare(Engine::Verification, Level::One, &[], BASELINE_HOST, &root);
        assert!(
            verification.refusals().is_empty(),
            "{:?}",
            verification.refusals()
        );
        assert_eq!(verification.timing, timing);
        assert_eq!(
            steps(&verification, true),
            sweep("direct-verify", "-", &verification::CLIENTS)
        );
        assert_eq!(
            applies(&verification, "overhead-p99"),
            ["direct-verify-c64"]
        );
        assert_eq!(
            applies(&verification, "no-errors"),
            sweep("direct-verify", "-", &verification::CLIENTS)
        );
        assert_eq!(applies(&verification, "correct-verdicts").len(), 2);
        for metric in [
            "verifications_per_s_per_vcpu",
            "verifications_per_cpu_s",
            "cpu_us_per_verification",
            "overhead_p99_ms",
            "verifier_p99_ms",
        ] {
            assert!(verification.metric(metric).is_some(), "{metric}");
        }

        let gateway = declare(Engine::Gateway, Level::One, &[], BASELINE_HOST, &root);
        assert!(gateway.refusals().is_empty(), "{:?}", gateway.refusals());
        assert_eq!(gateway.timing, timing);
        let clients: Vec<u64> = gateway::CLIENTS
            .iter()
            .map(|clients| *clients as u64)
            .collect();
        let mut shapes = sweep("chat", ".", &clients);
        shapes.extend(sweep("chat-stream", ".", &clients));
        shapes.extend(sweep("chat-capture", ".", &clients));
        assert_eq!(steps(&gateway, true), shapes);
        assert_eq!(
            applies(&gateway, "overhead-p99"),
            ["chat.c64", "chat-stream.c64", "chat-capture.c64"]
        );
        assert_eq!(applies(&gateway, "overhead-end-p99"), ["chat-stream.c64"]);
        for every in [
            "capture-lost",
            "capture-shed",
            "capture-per-arrival",
            "no-errors",
        ] {
            assert_eq!(applies(&gateway, every).len(), 6, "{every}");
        }
        for metric in [
            "requests_per_s_per_vcpu",
            "requests_per_cpu_s",
            "cpu_us_per_request",
            "capture_bytes_per_s_per_vcpu",
            "capture_bytes_per_cpu_s",
        ] {
            assert!(gateway.metric(metric).is_some(), "{metric}");
        }

        assert_eq!(
            verification.references.len(),
            2 * 4,
            "each client count compares overhead p99, CPU per verification, verifications per CPU-second and per vCPU"
        );
        assert_eq!(
            gateway.references.len(),
            6 * 4 + 2 + 2 * 2,
            "every case compares overhead p99, CPU, work and throughput; streams add end overhead, capture adds capture bytes"
        );
        for declaration in [&verification, &gateway] {
            assert!(
                declaration
                    .criteria
                    .iter()
                    .all(|criterion| matches!(criterion.rule, Rule::Limit { .. })),
                "{} references resolve only once the declaration is final",
                declaration.engine.name()
            );
            let (record, _) = execute(declaration.clone(), Box::new(|_, _, _| {})).await;
            assert_eq!(record.verdict, Verdict::Pass, "{}", record.render());
        }
    }

    /// V2: one representative case of every engine shape keeps server
    /// latency, CPU-seconds, busy cores `C/T`, provisioned `P`, `W/T`,
    /// `(W/T)/P`, `W/C`, `C/W`, peak memory and every outcome count, plus
    /// write rows and uncompressed and OTLP wire bytes, query scan facts and
    /// capture bytes; each derived value recomputes from its own operands,
    /// the report row shows them, and a repetition without server CPU leaves
    /// every CPU-derived metric missing with a reason and the run INVALID,
    /// never a favourable zero.
    ///
    /// # Panics
    ///
    /// Panics when a case loses a required metric or derives it wrongly.
    #[tokio::test(start_paused = true)]
    async fn engine_cases_preserve_latency_cpu_and_core_throughput() {
        let root = repository();
        let bifrost = |experiment: query::Experiment| {
            declare(
                Engine::Bifrost,
                Level::One,
                &[experiment.name()],
                BASELINE_HOST,
                &root,
            )
        };
        let cases = [
            (
                bifrost(query::Experiment::WritesParquet),
                "otlp-spans-c64",
                "rows",
                vec![
                    "rows_per_s_per_vcpu",
                    "rows_per_cpu_s",
                    "cpu_us_per_row",
                    "mib_per_s_per_vcpu",
                    "mib_per_cpu_s",
                    "wire_mib_per_s_per_vcpu",
                    "wire_mib_per_cpu_s",
                    "memory_per_mib_s",
                ],
                vec![
                    "requests",
                    "errors",
                    "server_failures",
                    "rows",
                    "retained_mib",
                    "wire_bytes",
                    "published_files",
                    "published_mib",
                    "memtable_backlog_mib",
                    "staged_backlog_mib",
                ],
            ),
            (
                bifrost(query::Experiment::WritesParquet),
                "native-write-c1",
                "rows",
                vec![
                    "rows_per_s_per_vcpu",
                    "rows_per_cpu_s",
                    "cpu_us_per_row",
                    "mib_per_cpu_s",
                ],
                vec![
                    "requests",
                    "errors",
                    "server_failures",
                    "rows",
                    "retained_mib",
                ],
            ),
            (
                bifrost(query::Experiment::Parquet),
                "point-c64",
                "queries",
                vec![
                    "queries_per_s_per_vcpu",
                    "queries_per_cpu_s",
                    "cpu_ms_per_query",
                ],
                vec![
                    "queries",
                    "errors",
                    "server_failures",
                    "wrong_results",
                    "scanned_rows_per_query",
                    "scanned_bytes_per_query",
                ],
            ),
            (
                bifrost(query::Experiment::Hot),
                "small-aggregate-c1",
                "queries",
                vec![
                    "queries_per_s_per_vcpu",
                    "queries_per_cpu_s",
                    "cpu_ms_per_query",
                ],
                vec!["queries", "errors", "server_failures", "wrong_results"],
            ),
            (
                declare(Engine::Verification, Level::One, &[], BASELINE_HOST, &root),
                "direct-verify-c64",
                "verifications",
                vec![
                    "verifications_per_s_per_vcpu",
                    "verifications_per_cpu_s",
                    "cpu_us_per_verification",
                ],
                vec![
                    "verifications",
                    "errors",
                    "server_failures",
                    "wrong",
                    "overhead_p99_ms",
                    "verifier_p99_ms",
                ],
            ),
            (
                declare(Engine::Gateway, Level::One, &[], BASELINE_HOST, &root),
                "chat-capture.c64",
                "requests",
                vec![
                    "requests_per_s_per_vcpu",
                    "requests_per_cpu_s",
                    "cpu_us_per_request",
                    "capture_bytes_per_s_per_vcpu",
                    "capture_bytes_per_cpu_s",
                ],
                vec![
                    "requests",
                    "arrivals",
                    "errors",
                    "server_failures",
                    "overhead_p99_ms",
                    "capture_staged",
                    "capture_dropped",
                    "capture_lost",
                    "capture_bytes",
                ],
            ),
        ];
        for (declaration, case, work, derived, raw) in cases {
            let name = format!("{} {case}", declaration.experiment);
            let (record, _) = execute(declaration.clone(), Box::new(|_, _, _| {})).await;
            let step = record
                .steps
                .iter()
                .find(|step| step.id == case)
                .unwrap_or_else(|| panic!("{name} is declared"));
            assert_eq!(step.repetitions.len(), 3, "{name}");
            for repetition in &step.repetitions {
                let read = |id: &str| {
                    repetition.reading(id).value.unwrap_or_else(|| {
                        panic!("{name} keeps `{id}`: {:?}", repetition.reading(id))
                    })
                };
                for id in ["p50_ms", "p95_ms", "p99_ms"].iter().chain(&raw) {
                    read(id);
                }
                let cpu = repetition.server_cpu_seconds.expect("server CPU");
                let vcpus = repetition.provisioned_vcpus.expect("provisioned vCPUs") as f64;
                let w = read(work);
                let t = repetition.seconds;
                assert!(
                    repetition.peak_memory_bytes.is_some(),
                    "{name} samples peak memory"
                );
                assert!(
                    repetition.driver_cpu_seconds.is_some(),
                    "{name} reads driver CPU apart"
                );
                let close = |id: &str, expected: f64| {
                    let got = read(id);
                    assert!(
                        (got - expected).abs() <= expected.abs() * 1e-9,
                        "{name} `{id}` = {got}, recomputed {expected}"
                    );
                };
                close("cores", cpu / t);
                close("rate", w / t);
                for id in &derived {
                    let metric = declaration
                        .metric(id)
                        .unwrap_or_else(|| panic!("{name} declares `{id}`"));
                    let operand = |work: &str| read(work);
                    match metric.derive.as_ref().expect("derived") {
                        Derive::PerVcpu { work, scale } => {
                            close(id, operand(work) / t / vcpus * scale);
                        }
                        Derive::PerCpu { work, scale } => close(id, operand(work) / cpu * scale),
                        Derive::CpuPer { work, scale } => close(id, cpu / operand(work) * scale),
                        Derive::MemoryPer { work } => close(
                            id,
                            repetition.peak_memory_bytes.expect("memory") as f64
                                / (operand(work) / t),
                        ),
                        other => panic!("{name} `{id}` derives {other:?}"),
                    }
                }
            }
            let report = record.render();
            let row = report
                .lines()
                .find(|line| line.starts_with(&format!("| {case} |")))
                .unwrap_or_else(|| panic!("{name} has a report row"));
            for id in derived.iter().take(3) {
                let unit = declaration.metric(id).expect("metric").unit.clone();
                assert!(row.contains(&unit), "{name} row shows {unit}: {row}");
            }
            assert!(row.contains("cores used") && row.contains("vCPU"), "{row}");

            let (unmeasured, _) = execute(
                declaration.clone(),
                Box::new(move |step, _, repetition| {
                    if step.id == case {
                        repetition.server_cpu_seconds = None;
                    }
                }),
            )
            .await;
            let step = unmeasured
                .steps
                .iter()
                .find(|step| step.id == case)
                .expect("case");
            for id in derived.iter().chain(&["cores"]) {
                let reading = step.repetitions[0].reading(id);
                if declaration.metric(id).is_some_and(|metric| {
                    !matches!(
                        metric.derive,
                        Some(
                            Derive::Rate { .. } | Derive::PerVcpu { .. } | Derive::MemoryPer { .. }
                        )
                    )
                }) {
                    assert!(
                        reading.value.is_none()
                            && reading
                                .reason
                                .as_deref()
                                .is_some_and(|reason| reason.contains("CPU")),
                        "{name} `{id}` without server CPU: {reading:?}"
                    );
                }
            }
            assert_eq!(unmeasured.verdict, Verdict::Invalid, "{name}");
        }
    }

    /// V11: L2 refuses an uncalibrated Bifrost run, calibrates from the
    /// newest compatible L1 record by the harmonic mix and never from a
    /// smoke record even moved into history, declares a fixed rate for its
    /// own smoke, runs every
    /// repetition through the whole stateful sequence, and judges
    /// concurrent SLO, backlog, drain, recovery, loss, and shedding at
    /// their boundaries even where isolated work passed.
    ///
    /// # Panics
    ///
    /// Panics when calibration or a phase judgment is wrong.
    #[tokio::test(start_paused = true)]
    async fn level_two_preserves_concurrent_work_and_recovery() {
        let timing = Level::Two.timing();
        assert_eq!(
            (
                timing.warmup_seconds,
                timing.repetition_seconds,
                timing.repetitions,
                timing.budget_seconds,
                timing.minimum_seconds
            ),
            (120, 360, 3, 1_800, 1_200)
        );
        let empty = tempfile::tempdir().expect("root");

        let uncalibrated = declare(
            Engine::Bifrost,
            Level::Two,
            &[],
            BASELINE_HOST,
            empty.path(),
        );
        let refusals: Vec<String> = uncalibrated
            .refusals()
            .into_iter()
            .map(|reason| reason.code)
            .collect();
        assert!(
            refusals.contains(&"calibration.missing".to_owned()),
            "{refusals:?}"
        );
        let (record, calls) = execute(uncalibrated, Box::new(|_, _, _| {})).await;
        assert_eq!(record.verdict, Verdict::Invalid);
        assert!(calls.is_empty(), "a refused declaration admits no work");

        let writes = declare(
            Engine::Bifrost,
            Level::One,
            &["writes-parquet"],
            BASELINE_HOST,
            &repository(),
        );
        let (l1, _) = execute(writes.clone(), bifrost_script(4.9)).await;
        let (elsewhere, _) = execute(
            declare(
                Engine::Bifrost,
                Level::One,
                &["writes-parquet"],
                OTHER_HOST,
                &repository(),
            ),
            bifrost_script(4.9),
        )
        .await;
        let base = declare(
            Engine::Bifrost,
            Level::Two,
            &[],
            BASELINE_HOST,
            empty.path(),
        );
        assert!(
            query::calibrate(std::slice::from_ref(&elsewhere), &base).is_err(),
            "another host never calibrates"
        );
        let calibration =
            query::calibrate(&[elsewhere, l1.clone()], &base).expect("compatible record");
        assert_eq!(
            calibration.rows_per_cpu_s,
            [300_000.0, 150_000.0, 600_000.0],
            "spans, logs, native peaks"
        );
        let mixed = 1.0 / (0.5 / 300_000.0 + 0.3 / 150_000.0 + 0.2 / 600_000.0);
        assert!((calibration.mixed_capacity() - mixed).abs() < 1e-6);

        let mut smoked = l1.clone();
        smoked.mode = Mode::Smoke;
        let relocated = tempfile::tempdir().expect("relocated");
        let smoke_path = smoked.publish(relocated.path()).expect("publish smoke");
        let history = relocated.path().join("benchmarks/bifrost/writes-parquet");
        std::fs::create_dir_all(&history).expect("history");
        std::fs::rename(&smoke_path, history.join("moved.json")).expect("relocate");
        let relocated_l2 = declare(
            Engine::Bifrost,
            Level::Two,
            &[],
            BASELINE_HOST,
            relocated.path(),
        );
        assert!(
            relocated_l2
                .refusals()
                .iter()
                .any(|reason| reason.code == "calibration.missing"),
            "a smoke record moved into history never calibrates"
        );
        let smoke_l2 = declare(
            Engine::Bifrost,
            Level::Two,
            &["--smoke"],
            BASELINE_HOST,
            empty.path(),
        );
        assert!(smoke_l2.refusals().is_empty(), "{:?}", smoke_l2.refusals());
        assert!(smoke_l2.controls["calibration"].get("smoke").is_some());
        assert!(smoke_l2.variables["ingest_rows_per_s"].as_f64() > Some(0.0));

        let root = tempfile::tempdir().expect("root");
        l1.publish(root.path()).expect("publish L1");
        let bifrost = declare(
            Engine::Bifrost,
            Level::Two,
            &["--server-cores", "4"],
            BASELINE_HOST,
            root.path(),
        );
        assert!(bifrost.refusals().is_empty(), "{:?}", bifrost.refusals());
        let rate = bifrost.variables["ingest_rows_per_s"]
            .as_f64()
            .expect("rate");
        assert!((rate - 0.7 * mixed * 4.0).abs() < 1e-6);
        assert_eq!(bifrost.steps.len(), 1, "one stateful step");
        assert_eq!(
            bifrost.steps[0].params.get("query_clients"),
            Some(&json!(32))
        );
        assert_eq!(
            bifrost.controls["workload"]["dashboard_split"],
            json!({ "small": 0.6, "million": 0.4 })
        );

        let (record, calls) = execute(bifrost.clone(), Box::new(|_, _, _| {})).await;
        assert_eq!(record.verdict, Verdict::Pass, "{}", record.render());
        assert_eq!(
            calls.iter().map(|(_, phase)| *phase).collect::<Vec<_>>(),
            [
                Phase::Warmup,
                Phase::Repetition(0),
                Phase::Repetition(1),
                Phase::Repetition(2)
            ]
        );
        assert!(record.elapsed_seconds >= 1_200.0);
        for repetition in &record.steps[0].repetitions {
            for phase in [
                "capacity.point_p99_ms",
                "overload.queued",
                "recovery.point_p99_ms",
            ] {
                assert!(repetition.metrics.contains_key(phase), "{phase}");
            }
        }
        let from_repetition = |n: u8, metric: &'static str, value: f64| -> Script {
            Box::new(move |_, phase, repetition| {
                if matches!(phase, Phase::Repetition(index) if index >= n) {
                    repetition.set(metric, Reading::exact(value).with_samples(2_000));
                }
            })
        };
        let step = query::COMPETING;
        let concurrent = execute(
            bifrost.clone(),
            Box::new(|_, phase, repetition: &mut Repetition| {
                if phase != Phase::Warmup {
                    repetition.set(
                        "capacity.point_p99_ms",
                        Reading::exact(6.0).with_samples(2_000),
                    );
                }
            }),
        )
        .await
        .0;
        let (parquet, _) = execute(
            declare(
                Engine::Bifrost,
                Level::One,
                &["parquet"],
                BASELINE_HOST,
                &repository(),
            ),
            bifrost_script(4.9),
        )
        .await;
        assert_eq!(
            judged(&parquet, "point-c64", "point-p99-slo"),
            Some(Verdict::Pass)
        );
        assert_eq!(
            judged(&concurrent, step, "capacity.point-p99-slo"),
            Some(Verdict::Fail),
            "isolated success cannot excuse a concurrent miss"
        );
        for (metric, criterion, value) in [
            (
                "capacity.backlog_growth_s",
                "capacity.backlog-bounded",
                1.01,
            ),
            ("overload.queue_drain_s", "overload.queue-drained", 60.5),
            ("drain_s", "drained", 60.5),
            ("queue.headroom.refusals", "queue.headroom.no-refusals", 1.0),
            (
                "queue.ack-delay.row_mismatch",
                "queue.ack-delay.exactly-once",
                1.0,
            ),
        ] {
            let failed = execute(bifrost.clone(), from_repetition(1, metric, value))
                .await
                .0;
            assert_eq!(
                judged(&failed, step, criterion),
                Some(Verdict::Fail),
                "{criterion}"
            );
        }
        let missed = execute(
            bifrost.clone(),
            Box::new(|_, phase, repetition: &mut Repetition| {
                if phase == Phase::Repetition(2)
                    && let Some(arrivals) = &mut repetition.arrivals
                {
                    arrivals.missed = 1;
                }
            }),
        )
        .await
        .0;
        assert_eq!(missed.verdict, Verdict::Invalid, "driver misses invalidate");

        let verification = declare(
            Engine::Verification,
            Level::Two,
            &[],
            BASELINE_HOST,
            empty.path(),
        );
        assert!(
            verification.refusals().is_empty(),
            "{:?}",
            verification.refusals()
        );
        let schedule = verification::Schedule::of(&verification.steps[0], Duration::from_secs(360))
            .expect("schedule");
        let stages = schedule.stages();
        assert_eq!(
            stages.map(|stage| (stage.start.as_secs(), stage.length.as_secs())),
            [(0, 150), (150, 60), (210, 150)]
        );
        assert!(
            (stages[1].direct_rate - schedule.direct_rate * schedule.overload_factor).abs() < 1e-9
        );
        assert_eq!(
            schedule.backlog_seconds(schedule.offered()).value,
            Some(1.0)
        );
        let (record, calls) = execute(verification.clone(), Box::new(|_, _, _| {})).await;
        assert_eq!(record.verdict, Verdict::Pass, "{}", record.render());
        assert_eq!(calls.len(), 4);
        let step = verification::COMPETING;
        for (metric, criterion, value) in [
            (
                "recovery.direct_overhead_p99_ms",
                "recovery-overhead-p99",
                verification::OVERHEAD_LIMIT_MS,
            ),
            ("capacity.backlog_growth_seconds", "capacity-backlog", 1.01),
            ("drain_seconds", "drain", 60.5),
            ("recovery.seconds", "recovery-time", 30.5),
            ("overload.errors", "overload-no-errors", 1.0),
            ("capacity.wrong", "capacity-correct-verdicts", 1.0),
        ] {
            let failed = execute(verification.clone(), from_repetition(0, metric, value))
                .await
                .0;
            assert_eq!(
                judged(&failed, step, criterion),
                Some(Verdict::Fail),
                "{criterion}"
            );
        }

        let gateway = declare(
            Engine::Gateway,
            Level::Two,
            &[],
            BASELINE_HOST,
            empty.path(),
        );
        assert!(gateway.refusals().is_empty(), "{:?}", gateway.refusals());
        assert_eq!(
            gateway.controls["limits"]["slo_p99_ms"],
            json!({ "completion": 200.0, "first_token": 350.0, "stream": 1100.0 })
        );
        let overridden = declare(
            Engine::Gateway,
            Level::Two,
            &["--max-completion-ms", "150"],
            BASELINE_HOST,
            empty.path(),
        );
        assert!(matches!(
            overridden.criteria.iter().find(|criterion| criterion.metric == "capacity.completion_p99_ms").map(|criterion| &criterion.rule),
            Some(Rule::Limit { limit, .. }) if (*limit - 150.0).abs() < f64::EPSILON
        ));
        let phases = gateway::capacity::Phases::of(Duration::from_secs(360));
        assert_eq!(phases.overload_end(), Duration::from_secs(300));
        assert_eq!(phases.judged(), Duration::from_secs(330));
        assert_eq!(
            phases.phase(Duration::from_secs(239)),
            gateway::capacity::Period::Capacity
        );
        assert_eq!(
            phases.phase(Duration::from_secs(329)),
            gateway::capacity::Period::Overload
        );
        assert_eq!(
            phases.phase(Duration::from_secs(330)),
            gateway::capacity::Period::Recovery
        );
        let (record, _) = execute(gateway.clone(), Box::new(|_, _, _| {})).await;
        assert_eq!(record.verdict, Verdict::Pass, "{}", record.render());
        let step = gateway::CONTENTION;
        for (metric, criterion) in [
            ("capture.lost", "capture-lost"),
            (
                "capture.shed_outside_overload",
                "capture-shed-outside-overload",
            ),
            ("overload.unexpected", "overload-refusals"),
            ("undrained", "drained"),
        ] {
            let failed = execute(gateway.clone(), from_repetition(2, metric, 1.0))
                .await
                .0;
            assert_eq!(
                judged(&failed, step, criterion),
                Some(Verdict::Fail),
                "{criterion}"
            );
        }
        let shed = execute(gateway.clone(), from_repetition(2, "overload.shed", 5.0))
            .await
            .0;
        assert_eq!(
            shed.verdict,
            Verdict::Pass,
            "accounted shedding is allowed during overload"
        );
        let completion = execute(
            gateway.clone(),
            from_repetition(0, "recovery.completion_p99_ms", 200.5),
        )
        .await
        .0;
        assert_eq!(
            judged(&completion, step, "recovery-completion"),
            Some(Verdict::Fail)
        );
    }

    /// V12: the aggregate runs every engine in order with the forwarded
    /// shared flags, keeps going after a failure or an invalid run, and
    /// combines exits 2 over 1 over 0; usage errors exit 2 before any work;
    /// and `report` regenerates a persisted record.
    ///
    /// # Panics
    ///
    /// Panics when dispatch, forwarding, precedence, or reporting is wrong.
    #[tokio::test(start_paused = true)]
    async fn level_commands_preserve_records_and_exit_precedence() {
        let root = tempfile::tempdir().expect("root");
        let mut seen = Vec::new();
        let code = levels::aggregate(&line(&["--profile"]), |engine, forwarded| {
            seen.push((engine, forwarded.to_vec()));
            match engine {
                Engine::Bifrost => 1,
                Engine::Verification => 2,
                Engine::Gateway => 0,
            }
        });
        assert_eq!(code, 2);
        assert_eq!(
            seen,
            levels::ENGINES.map(|engine| (engine, vec!["--profile".to_owned()]))
        );
        for (codes, expected) in [([0, 0, 0], 0), ([0, 1, 0], 1), ([2, 0, 1], 2)] {
            let mut next = codes.into_iter();
            assert_eq!(
                levels::aggregate(&line(&[]), |_, _| next.next().expect("engine")),
                expected
            );
        }

        let mut records = Vec::new();
        for engine in levels::ENGINES {
            let rest: &[&str] = if engine == Engine::Bifrost {
                &["writes-parquet"]
            } else {
                &[]
            };
            let declaration = declare(engine, Level::One, rest, OTHER_HOST, &repository());
            let script: Script = if engine == Engine::Verification {
                Box::new(|_, _, repetition| repetition.set("errors", Reading::exact(1.0)))
            } else {
                Box::new(|_, _, _| {})
            };
            records.push(execute(declaration, script).await.0);
        }
        let mut published = Vec::new();
        let mut pending = records.iter();
        let code = levels::aggregate(&line(&[]), |engine, _| {
            let record = pending.next().expect("engine record");
            assert_eq!(record.engine, engine);
            published.push(record.publish(root.path()).expect("publish"));
            record.exit_code()
        });
        assert_eq!(
            published.len(),
            3,
            "a failed engine never stops the later ones"
        );
        assert_eq!(code, 1, "the Verification failure decides the aggregate");
        for path in &published {
            assert!(path.starts_with(root.path().join("benchmarks")));
            let record = Record::read(path).expect("record");
            let report = levels::report(path).expect("report");
            assert_eq!(report, record.render());
            let arguments = line(&["report", path.to_str().expect("path")]);
            assert_eq!(dispatch(&arguments, root.path()).await, 0);
        }

        let malformed = root.path().join("malformed.json");
        std::fs::write(&malformed, b"{}").expect("write");
        let arguments = line(&["report", malformed.to_str().expect("path")]);
        assert_eq!(dispatch(&arguments, root.path()).await, 2);
        for usage in [
            &["l3"][..],
            &["l1", "--clients", "4"],
            &["l2"],
            &["l2", "nosuch"],
            &["l1", "bifrost", "nosuch"],
            &["l1", "bifrost", "--clients", "4"],
            &["l1", "bifrost", "writes", "--steps", "native-write-c1"],
            &["smoke", "l1", "bifrost"],
            &[],
        ] {
            assert_eq!(dispatch(&line(usage), root.path()).await, 2, "{usage:?}");
        }
    }
}
