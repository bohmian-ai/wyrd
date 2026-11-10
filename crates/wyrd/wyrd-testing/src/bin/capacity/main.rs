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
//! - `results`: writes every recorded run of `benchmarks/history/` into one HTML
//!   page at `target/benchmarks/results.html`, through
//!   `mise run bench:results`.
//!
//! Every experiment shares one lifecycle ([`levels`] over
//! `wyrd_testing::experiment`): a sealed declaration, quiet-host checks,
//! warmup and three repetitions, judgment, an append-only record under
//! `benchmarks/history/<engine>/<experiment>/` (a `--steps`, `--profile`,
//! or `--allow-dirty` run's under `target/benchmarks/`), and exit 0 (VALID)
//! or 2 (INVALID);
//! a missed performance target is reported and never changes the exit.
//! `--profile` adds optional samply CPU captures and server heap profiles,
//! `--allow-dirty` records a dirty source, `--steps` selects declared steps,
//! and `--smoke` runs the same plan at smoke scale under
//! `target/benchmarks/smoke/`. A full run never runs a smoke first;
//! `mise run bench:smoke` runs every level's smoke.

mod client_queue;
mod flags;
mod gateway;
mod levels;
mod query;
mod verification;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wyrd_testing::experiment::{DRIVER_CORES, Engine, Level};

use crate::levels::{Context, Plan};

/// Where `results` writes the results page, under the repository root.
const RESULTS: &str = "target/benchmarks/results.html";

/// Dispatches the level command the arguments name.
///
/// The driver's runtime has one worker per core of its declared
/// [`DRIVER_CORES`] budget rather than one per host CPU, so the budget every
/// run is judged against is also the parallelism the driver schedules over.
fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(usize::from(DRIVER_CORES))
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("capacity: cannot start the Tokio runtime: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = wyrd_testing::capacity::install_tracing() {
        eprintln!("capacity: cannot install the tracing registry: {error}");
        return ExitCode::from(2);
    }
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
                    "capacity l1 bifrost: {message}; running several experiments takes only --profile, --allow-dirty and --smoke"
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
                    "capacity l1: {message}; the aggregate takes only --profile, --allow-dirty and --smoke"
                );
                return 2;
            }
            levels::aggregate(&shared, |engine, forwarded| {
                levels::spawn_engine(root, engine, forwarded)
            })
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
        (Some("results"), _, _) if arguments.len() == 2 => {
            let out = root.join(RESULTS);
            match levels::results(root, &out) {
                Ok(records) => {
                    println!("{records} records: {}", out.display());
                    0
                }
                Err(message) => {
                    eprintln!("capacity results: {message}");
                    2
                }
            }
        }
        _ => {
            eprintln!(
                "capacity: expected `l1 [bifrost [writes-parquet|hot|parquet]|verification|gateway]`, `l2 <bifrost|verification|gateway>`, `report <record.json>`, or `results`, got {:?}",
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

/// Plans `engine` at `level` from `arguments` and runs it, as its smoke
/// under `--smoke`.
async fn level(level: Level, engine: Engine, arguments: &[String], root: &Path) -> u8 {
    if let Err(error) = start_benchmark() {
        eprintln!("capacity l{} {}: {error}", u8::from(level), engine.name());
        return 2;
    }
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
            let profile = arguments.iter().any(|argument| argument == "--profile");
            Box::pin(levels::execute(plan, root, profile)).await
        }
        Err(message) => {
            eprintln!("capacity l{} {}: {message}", u8::from(level), engine.name());
            2
        }
    }
}

/// Makes this driver process a benchmark recorder before it creates any
/// client, as `WYRD_METRICS_BENCHMARK_SNAPSHOT=1` makes its child server one,
/// so its own client, attempt, and fixture observations are kept; only
/// `l1`/`l2` start it, never `report` or `results`.
///
/// # Errors
///
/// Returns the refusal when another `metrics` recorder is already installed.
fn start_benchmark() -> Result<(), String> {
    wyrd_benchmark::install()
        .map_err(|error| format!("cannot start the benchmark recorder: {error}"))
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
        Options, Outcome, Over, Phase, Reading, Record, Repetition, Rule, Source, Status, Step,
        StepKind, Timing, Verdict, Workload, run,
    };
    use wyrd_testing::release_server::{HostLoad, PEAK_SAMPLE_INTERVAL_MS};

    use super::dispatch;
    use crate::levels::{self, Context};
    use crate::{gateway, query, verification};

    /// The host the test declarations measure on.
    const HOST: &str = "AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux";

    /// Another host, whose records never calibrate an L2 rate on [`HOST`].
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
            Engine::Verification => verification::declare(level, &arguments, base),
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
            let Rule { op, limit, .. } = criterion.rule;
            if raw && criterion.steps.contains(&step.id) {
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

    /// The outcome of criterion `id` on `step`, or `None` when it did not
    /// judge there.
    fn judged(record: &Record, step: &str, id: &str) -> Option<Outcome> {
        record
            .steps
            .iter()
            .find(|candidate| candidate.id == step)
            .and_then(|step| {
                step.criteria
                    .iter()
                    .find(|judgment| judgment.criterion == id)
            })
            .and_then(|judgment| judgment.outcome)
    }

    /// The outcome and run verdict a failing reading of criterion `id` of
    /// `declaration` gives: a median target is missed and the run stays
    /// VALID; an every-repetition criterion invalidates it.
    fn failing(declaration: &Declaration, id: &str) -> (Option<Outcome>, Verdict) {
        let criterion = declaration
            .criteria
            .iter()
            .find(|criterion| criterion.id == id)
            .expect("declared criterion");
        match criterion.rule.over {
            Over::Median => (Some(Outcome::Missed), Verdict::Valid),
            Over::Every => (Some(Outcome::Invalid), Verdict::Invalid),
        }
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
                Some("otlp-spans" | "otlp-spans-1k") => 1.0,
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
    /// picks sanity-floor peaks from the measured sweep, and records a
    /// selected subset's omissions.
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
        let writes = experiment(query::Experiment::WritesParquet, &[], HOST);
        let hot = experiment(query::Experiment::Hot, &[], HOST);
        let parquet = experiment(query::Experiment::Parquet, &[], HOST);
        for (bifrost, budget) in [(&writes, 480), (&hot, 300), (&parquet, 300)] {
            assert!(bifrost.refusals().is_empty(), "{:?}", bifrost.refusals());
            assert_eq!(
                bifrost.timing,
                Timing {
                    budget_seconds: budget,
                    ..timing
                },
                "writes-parquet fits 480 s and every other L1 experiment 300 s"
            );
            for control in ["fixture", "limits", "workload", "timing"] {
                assert!(bifrost.controls.contains_key(control), "{control}");
            }
            assert!(
                bifrost.controls["timing"]["timer_resolution_ns"]
                    .as_u64()
                    .is_some_and(|ns| ns > 0),
                "the measured timer resolution is a declared control"
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
            [
                "otlp-spans-peak",
                "otlp-spans-1k-peak",
                "otlp-logs-peak",
                "native-write-peak"
            ]
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
                ("otlp-spans-1k-c1".to_owned(), 10),
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
                "point-memtable-c1",
                "point-memtable-c64",
                "point-staged-c1",
                "small-aggregate-memtable-c1",
                "small-aggregate-staged-c1",
                "span-shredded-memtable-c1",
                "span-shredded-staged-c1",
                "span-long-tail-memtable-c1",
                "span-long-tail-staged-c1"
            ]
        );
        assert_eq!(steps(&hot, false), ["point-memtable-scaling"]);
        assert!(
            hot.controls["limits"]
                .as_str()
                .is_some_and(|limits| limits.contains("WYRD_MAX_FILE_RETENTION_TIME=86400")),
            "under the hot settings nothing ages out; only the benchmark control moves rows"
        );
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
        for (bifrost, point) in [(&hot, "point-memtable-c64"), (&parquet, "point-c64")] {
            assert_eq!(applies(bifrost, "point-p99-slo"), [point]);
            let slo = bifrost
                .criteria
                .iter()
                .find(|criterion| criterion.id == "point-p99-slo")
                .expect("point SLO");
            assert_eq!(slo.percentile, Some(99));
            assert!(
                matches!(slo.rule, Rule { op: Op::Lt, limit, .. } if limit == query::POINT_P99_MS)
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
                Rule {
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
        assert!(record.elapsed_seconds <= 480.0);
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
            Some(Outcome::Met)
        );
        assert_eq!(
            judged(&record, "otlp-logs-peak", "otlp-logs-peak-floor"),
            Some(Outcome::Met)
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
            Some(Outcome::Met)
        );
        assert_eq!(
            judged(&record, "point-c1", "point-p99-slo"),
            None,
            "64-client SLO only at 64"
        );
        assert_eq!(
            judged(&record, "span-shredded-c1", "span-shredding"),
            Some(Outcome::Met)
        );
        assert_eq!(
            judged(&record, "span-trace-c1", "span-pruning"),
            Some(Outcome::Met)
        );
        let (strict, _) = execute(parquet.clone(), bifrost_script(query::POINT_P99_MS)).await;
        assert_eq!(
            judged(&strict, "point-c64", "point-p99-slo"),
            Some(Outcome::Missed),
            "equality misses"
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
            HOST,
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

        let verification = declare(Engine::Verification, Level::One, &[], HOST, &root);
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

        let gateway = declare(Engine::Gateway, Level::One, &[], HOST, &root);
        assert!(gateway.refusals().is_empty(), "{:?}", gateway.refusals());
        assert_eq!(gateway.timing, timing);
        let clients: Vec<u64> = gateway::CLIENTS
            .iter()
            .map(|clients| *clients as u64)
            .collect();
        let mut shapes = sweep("buffered-1kib-capture-metadata", ".", &clients);
        shapes.extend(sweep("streamed-1kib-capture-metadata", ".", &clients));
        shapes.extend(sweep("buffered-64kib-capture-metadata", ".", &clients));
        shapes.extend(sweep("buffered-64kib-capture-payload", ".", &clients));
        assert_eq!(steps(&gateway, true), shapes);
        assert_eq!(
            applies(&gateway, "overhead-p99"),
            [
                "buffered-1kib-capture-metadata.c64",
                "streamed-1kib-capture-metadata.c64",
                "buffered-64kib-capture-metadata.c64",
                "buffered-64kib-capture-payload.c64"
            ]
        );
        assert_eq!(
            applies(&gateway, "overhead-end-p99"),
            ["streamed-1kib-capture-metadata.c64"]
        );
        for every in [
            "capture-lost",
            "capture-shed",
            "capture-per-arrival",
            "no-errors",
        ] {
            assert_eq!(applies(&gateway, every).len(), 8, "{every}");
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

        for declaration in [&verification, &gateway] {
            let (record, _) = execute(declaration.clone(), Box::new(|_, _, _| {})).await;
            assert_eq!(record.verdict, Verdict::Valid, "{}", record.render());
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
                HOST,
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
                    "wire_bytes_per_row",
                    "published_files",
                    "published_mib",
                    "wal_bytes_per_row",
                    "staged_bytes_per_row",
                    "published_bytes_per_row",
                    "wal_syncs",
                    "stage_syncs",
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
                "small-aggregate-memtable-c1",
                "queries",
                vec![
                    "queries_per_s_per_vcpu",
                    "queries_per_cpu_s",
                    "cpu_ms_per_query",
                ],
                vec!["queries", "errors", "server_failures", "wrong_results"],
            ),
            (
                declare(Engine::Verification, Level::One, &[], HOST, &root),
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
                declare(Engine::Gateway, Level::One, &[], HOST, &root),
                "buffered-64kib-capture-payload.c64",
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

        let uncalibrated = declare(Engine::Bifrost, Level::Two, &[], HOST, empty.path());
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
            HOST,
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
        let base = declare(Engine::Bifrost, Level::Two, &[], HOST, empty.path());
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
        let history = relocated
            .path()
            .join("benchmarks/history/bifrost/writes-parquet");
        std::fs::create_dir_all(&history).expect("history");
        std::fs::rename(&smoke_path, history.join("moved.json")).expect("relocate");
        let relocated_l2 = declare(Engine::Bifrost, Level::Two, &[], HOST, relocated.path());
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
            HOST,
            empty.path(),
        );
        assert!(smoke_l2.refusals().is_empty(), "{:?}", smoke_l2.refusals());
        assert!(smoke_l2.controls["calibration"].get("smoke").is_some());
        assert!(smoke_l2.variables["ingest_rows_per_s"].as_f64() > Some(0.0));

        let old = tempfile::tempdir().expect("old layout");
        let moved = old.path().join("benchmarks/bifrost/writes-parquet");
        std::fs::create_dir_all(&moved).expect("old history");
        std::fs::write(
            moved.join("old.json"),
            serde_json::to_vec(&l1).expect("record json"),
        )
        .expect("old record");
        assert!(
            declare(Engine::Bifrost, Level::Two, &[], HOST, old.path())
                .refusals()
                .iter()
                .any(|reason| reason.code == "calibration.missing"),
            "calibration never falls back to benchmarks/<engine>/<experiment>/"
        );

        let root = tempfile::tempdir().expect("root");
        let published = l1.publish(root.path()).expect("publish L1");
        assert!(
            published.starts_with(
                root.path()
                    .join("benchmarks/history/bifrost/writes-parquet")
            )
        );
        let bifrost = declare(
            Engine::Bifrost,
            Level::Two,
            &["--server-cores", "4"],
            HOST,
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
        assert_eq!(record.verdict, Verdict::Valid, "{}", record.render());
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
                HOST,
                &repository(),
            ),
            bifrost_script(4.9),
        )
        .await;
        assert_eq!(
            judged(&parquet, "point-c64", "point-p99-slo"),
            Some(Outcome::Met)
        );
        assert_eq!(
            (
                judged(&concurrent, step, "capacity.point-p99-slo"),
                concurrent.verdict
            ),
            (Some(Outcome::Missed), Verdict::Valid),
            "isolated success cannot excuse a concurrent miss"
        );
        for target in [
            "capacity.backlog-bounded",
            "queue.headroom.bytes-bounded",
            "queue.ack-delay.bytes-bounded",
        ] {
            assert_eq!(
                failing(&bifrost, target),
                (Some(Outcome::Missed), Verdict::Valid),
                "{target} is a target"
            );
        }
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
                (judged(&failed, step, criterion), failed.verdict),
                failing(&bifrost, criterion),
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

        let verification = declare(Engine::Verification, Level::Two, &[], HOST, empty.path());
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
        assert_eq!(record.verdict, Verdict::Valid, "{}", record.render());
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
                (judged(&failed, step, criterion), failed.verdict),
                failing(&verification, criterion),
                "{criterion}"
            );
        }

        let gateway = declare(Engine::Gateway, Level::Two, &[], HOST, empty.path());
        assert!(gateway.refusals().is_empty(), "{:?}", gateway.refusals());
        assert_eq!(
            gateway.controls["limits"]["slo_p99_ms"],
            json!({ "completion": 200.0, "first_token": 350.0, "stream": 1100.0 })
        );
        let overridden = declare(
            Engine::Gateway,
            Level::Two,
            &["--max-completion-ms", "150"],
            HOST,
            empty.path(),
        );
        assert!(matches!(
            overridden.criteria.iter().find(|criterion| criterion.metric == "capacity.completion_p99_ms").map(|criterion| &criterion.rule),
            Some(Rule { limit, .. }) if (*limit - 150.0).abs() < f64::EPSILON
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
        assert_eq!(record.verdict, Verdict::Valid, "{}", record.render());
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
                (judged(&failed, step, criterion), failed.verdict),
                failing(&gateway, criterion),
                "{criterion}"
            );
        }
        let shed = execute(gateway.clone(), from_repetition(2, "overload.shed", 5.0))
            .await
            .0;
        assert_eq!(
            shed.verdict,
            Verdict::Valid,
            "accounted shedding is allowed during overload"
        );
        let completion = execute(
            gateway.clone(),
            from_repetition(0, "recovery.completion_p99_ms", 200.5),
        )
        .await
        .0;
        assert_eq!(
            (
                judged(&completion, step, "recovery-completion"),
                completion.verdict
            ),
            (Some(Outcome::Missed), Verdict::Valid)
        );
    }

    /// V12: the aggregate runs every engine in order with the forwarded
    /// shared flags, keeps going after an invalid run, and combines exits
    /// to 0 only when every engine exits 0, else 2; usage errors exit 2
    /// before any work;
    /// `report` regenerates a persisted record; and `results` writes every
    /// history file into one page, marking one that is not a current record
    /// and escaping embedded text.
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
        for (codes, expected) in [([0, 0, 0], 0), ([0, 1, 0], 2), ([2, 0, 1], 2)] {
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
            "an invalid engine never stops the later ones"
        );
        assert_eq!(
            code, 2,
            "the invalid Verification run decides the aggregate"
        );
        for path in &published {
            assert!(path.starts_with(root.path().join("benchmarks/history")));
            let record = Record::read(path).expect("record");
            let report = levels::report(path).expect("report");
            assert_eq!(report, record.render());
            let arguments = line(&["report", path.to_str().expect("path")]);
            assert_eq!(dispatch(&arguments, root.path()).await, 0);
        }

        let stale = root
            .path()
            .join("benchmarks/history/verification/direct-verify/19990101T000000Z-000000000.json");
        std::fs::write(
            &stale,
            br#"{"schema_version": 2, "engine": "verification", "note": "</script><b>"}"#,
        )
        .expect("write");
        let old = root.path().join("benchmarks/verification/direct-verify");
        std::fs::create_dir_all(&old).expect("old layout");
        std::fs::copy(&published[1], old.join("old.json")).expect("old record");
        let page = root.path().join("results.html");
        assert_eq!(
            levels::results(root.path(), &page),
            Ok(4),
            "results read benchmarks/history only, never the old layout"
        );
        let html = std::fs::read_to_string(&page).expect("page");
        assert!(!html.contains("/*RECORDS*/"), "the records fill the slot");
        assert_eq!(
            html.matches("</script>").count(),
            2,
            "embedded text cannot close the page's scripts"
        );
        assert_eq!(
            html.matches(r#""problem":null"#).count(),
            3,
            "every published record reads as current"
        );
        assert_eq!(
            html.matches(r#""problem":""#).count(),
            1,
            "a file that is not a current record is kept and marked"
        );
        assert_eq!(dispatch(&line(&["results"]), root.path()).await, 0);
        assert!(root.path().join(super::RESULTS).is_file());

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

    /// A transport over a local server answering every request `{}`.
    async fn answered() -> wyrd_client::transport::HttpTransport {
        serving(|_| true).await
    }

    /// A transport over a local server answering a request `{}` when
    /// `answers` accepts its request line, and a non-retryable 400 problem
    /// otherwise.
    async fn serving(answers: fn(&str) -> bool) -> wyrd_client::transport::HttpTransport {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0_u8; 8192];
                let read = socket.read(&mut request).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&request[..read]);
                let response: &[u8] = if answers(line.lines().next().unwrap_or_default()) {
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}"
                } else {
                    b"HTTP/1.1 400 Bad Request\r\ncontent-type: application/problem+json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}"
                };
                let _ = socket.write_all(response).await;
            }
        });
        let config = wyrd_client::config::ClientConfig::default();
        let auth = wyrd_client::auth::AuthMiddleware::new(
            &config,
            wyrd_client::transport::credential::ResolvedCredential::BearerToken(
                "token".to_owned().into(),
            ),
        )
        .expect("auth");
        wyrd_client::transport::HttpTransport::new(
            &wyrd_client::transport::config::HttpConfig {
                base_url: format!("http://{address}"),
                ..wyrd_client::transport::config::HttpConfig::default()
            },
            auth,
        )
        .expect("transport")
    }

    /// Driver startup installs the classification registry whether or not a
    /// log filter is set, and the recorder, before any client exists: a span
    /// labelled from benchmark metadata keeps its cohort, and a labelled
    /// client call records its logical call and attempt in that cohort.
    /// Starting again is not a second installation.
    ///
    /// # Panics
    ///
    /// Panics when startup fails, a label is lost, or the call is not
    /// recorded in its cohort.
    #[tokio::test]
    async fn client_cohorts_record_without_log_environment() {
        wyrd_testing::capacity::install_tracing().expect("the only subscriber");
        super::start_benchmark().expect("the only recorder");
        super::start_benchmark().expect("starting again changes nothing");
        assert!(wyrd_benchmark::enabled());

        let span = tracing::info_span!("fixture.request");
        let mut headers = http::HeaderMap::new();
        headers.insert(
            wyrd_benchmark::CASE_HEADER,
            http::HeaderValue::from_static("direct-verify"),
        );
        headers.insert(
            wyrd_benchmark::PHASE_HEADER,
            http::HeaderValue::from_static("repetition-2"),
        );
        wyrd_benchmark::label_request(&span, &headers);
        let cohort = wyrd_benchmark::Cohort::of_span(&span);
        assert_eq!(
            (cohort.case(), cohort.phase()),
            ("direct-verify", "repetition-2"),
            "the registry keeps span labels without a log layer"
        );

        let transport = answered().await;
        let before = wyrd_benchmark::snapshot();
        wyrd_client::transport::trace::with_benchmark_labels(
            "direct-verify",
            "repetition-1",
            transport.request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                "/v1/probe",
                None,
            ),
        )
        .await
        .expect("the call succeeds");
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        for boundary in ["client.http", "client.http.attempt"] {
            assert!(
                interval
                    .series
                    .iter()
                    .any(|series| series.boundary == boundary
                        && series.outcome == wyrd_benchmark::SUCCESS
                        && series.case == "direct-verify"
                        && series.phase == "repetition-1"
                        && series.count == 1),
                "{boundary} keeps its cohort"
            );
        }
        assert_eq!(interval.dropped, 0);
    }

    /// Calls `path` on `transport` as `case` admitted in `phase`, returning
    /// whether it succeeded.
    async fn labelled(
        transport: &wyrd_client::transport::HttpTransport,
        (case, phase): (&'static str, &'static str),
        path: &str,
    ) -> bool {
        wyrd_client::transport::trace::with_benchmark_labels(
            case,
            phase,
            transport.request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                path,
                None,
            ),
        )
        .await
        .is_ok()
    }

    /// A driver population is read from the driver's own recorder and kept
    /// as [`Role::Driver`] evidence beside the server's [`Role::Server`]
    /// window, and a driver snapshot never differences against a server
    /// snapshot, so neither process's observations pass for the other's.
    ///
    /// # Panics
    ///
    /// Panics when a population loses its role, a cross-process difference
    /// is accepted, or the driver call is not reconciled.
    #[tokio::test]
    async fn driver_and_server_snapshots_keep_distinct_process_populations() {
        use wyrd_testing::capacity::{Expected, Population};
        use wyrd_testing::experiment::Role;

        wyrd_testing::capacity::install_tracing().expect("the only subscriber");
        super::start_benchmark().expect("the only recorder");
        let transport = answered().await;
        let cohort = ("direct-verify", "repetition-1");
        let before = wyrd_benchmark::snapshot();
        assert!(labelled(&transport, cohort, "/v1/probe").await);
        let owed = [Expected {
            boundary: "client.http",
            outcome: wyrd_benchmark::SUCCESS,
            case: cohort.0,
            phase: cohort.1,
            count: 1,
        }];
        let driver = Population::collect_driver(&before, &owed).await;
        assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
        assert_eq!(driver.role, Role::Driver);
        assert_eq!(driver.only(cohort.0, cohort.1).role, Role::Driver);

        let mut server_before = before.clone();
        server_before.process = format!("{}-server", before.process);
        let mut server_after = server_before.clone();
        server_after.taken_ns += 1;
        assert!(
            Population::between(&before, server_after.clone()).is_err(),
            "a driver snapshot never differences against a server's"
        );
        let server = Population::between(&server_before, server_after).expect("one process");
        assert_eq!(server.role, Role::Server);

        let mut repetition = Repetition::default();
        server.validate(&mut repetition, "window");
        driver.validate(&mut repetition, "window");
        let roles: Vec<_> = repetition
            .timings
            .iter()
            .map(|timing| timing.role)
            .collect();
        assert_eq!(roles, [Role::Server, Role::Driver]);
        assert!(
            repetition
                .reconciliation
                .iter()
                .all(|reconciled| reconciled.role == Role::Driver && reconciled.recorded == 1)
        );
        assert!(repetition.observations[0].starts_with("server timing"));
        assert!(repetition.observations[1].starts_with("driver timing"));
    }

    /// A driver window owes each phase its own completed calls: failed calls
    /// are counted beside the owed success count rather than inside it, and a
    /// cohort owed more or fewer calls than recorded makes the population
    /// invalid with its recorded count kept.
    ///
    /// # Panics
    ///
    /// Panics when a phase's completions or failures are miscounted, or a
    /// mismatched cohort is accepted.
    #[tokio::test]
    async fn driver_phase_counts_reconcile_completed_and_failed_calls() {
        use wyrd_testing::capacity::{Expected, Population};

        wyrd_testing::capacity::install_tracing().expect("the only subscriber");
        super::start_benchmark().expect("the only recorder");
        let transport = serving(|line| !line.contains("/v1/refused")).await;
        let (case, first, second) = ("direct-verify", "repetition-1", "repetition-2");
        let before = wyrd_benchmark::snapshot();
        for (path, succeeds) in [
            ("/v1/probe", true),
            ("/v1/refused", false),
            ("/v1/probe", true),
        ] {
            assert_eq!(labelled(&transport, (case, first), path).await, succeeds);
        }
        assert!(labelled(&transport, (case, second), "/v1/probe").await);
        let owed = |phase, count| Expected {
            boundary: "client.http",
            outcome: wyrd_benchmark::SUCCESS,
            case,
            phase,
            count,
        };
        let driver = Population::collect_driver(&before, &[owed(first, 2), owed(second, 1)]).await;
        assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
        for (phase, completed, failed) in [(first, 2, 1), (second, 1, 0)] {
            let cohort = driver.only(case, phase);
            assert_eq!(
                cohort
                    .elapsed("client.http", wyrd_benchmark::SUCCESS)
                    .map(|d| d.len()),
                Some(completed),
                "{phase}"
            );
            assert_eq!(
                cohort.others("client.http", wyrd_benchmark::SUCCESS),
                failed,
                "{phase}"
            );
        }

        let (earlier, later) = driver.snapshots.clone().expect("the window's snapshots");
        let mut overdue = Population::between(&earlier, later).expect("one process");
        overdue.expect(&[owed(first, 3)]);
        assert_eq!(overdue.invalid.len(), 1, "{:?}", overdue.invalid);
        assert_eq!(overdue.reconciliation[0].recorded, 2);
    }

    /// Building and calling an ordinary client installs neither a recorder
    /// nor a subscriber and records nothing, so benchmark startup afterwards
    /// still finds no other backend.
    ///
    /// # Panics
    ///
    /// Panics when the client installs or records anything.
    #[tokio::test]
    async fn ordinary_clients_do_not_install_or_record_benchmarks() {
        let before = wyrd_benchmark::snapshot();
        let transport = answered().await;
        let _clone = transport.clone();
        let _: serde_json::Value = transport
            .request_json(
                reqwest::Method::GET,
                "/v1/probe",
                None::<&serde_json::Value>,
            )
            .await
            .expect("the call succeeds");
        assert!(!wyrd_benchmark::enabled());
        assert!(
            !tracing::dispatcher::has_been_set(),
            "no subscriber installed"
        );
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        assert!(interval.series.is_empty() && interval.metrics.is_empty());
        super::start_benchmark().expect("no other recorder was installed");
    }

    /// Every engine's L1 and L2 consumer reads its judged timings, scan
    /// facts, diagnostic histograms, counters, gauges, and backlog from one
    /// pair of benchmark snapshots with only the benchmark recorder
    /// installed: no Prometheus recorder can install beside it, and no
    /// exposition is rendered or parsed.
    ///
    /// # Panics
    ///
    /// Panics when a Prometheus recorder installs, or a consumer reads a
    /// timing, count, gauge, or diagnostic other than the one recorded.
    #[test]
    fn all_engine_consumers_read_benchmark_snapshots_without_prometheus() {
        use std::time::Duration;

        use wyrd_testing::capacity::{Population, QUERY_BOUNDARY};
        use wyrd_testing::release_server::Metrics;

        use crate::gateway::run::{Durable, Integrity};
        use crate::verification::run as verify;

        wyrd_benchmark::install().expect("the benchmark recorder installs");
        assert!(
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .install_recorder()
                .is_err(),
            "the benchmark recorder is the only one"
        );
        let before = wyrd_benchmark::snapshot();
        let first = Metrics::of_snapshot(&before).expect("a complete snapshot");
        let span = tracing::Span::none();
        let ms = Duration::from_millis;

        // Bifrost: query and write timings, scan facts, ingest counters, and
        // memtable and Oracle slot gauges.
        wyrd_benchmark::record_timing(&span, QUERY_BOUNDARY, "success", ms(2), None);
        wyrd_benchmark::record_scan(&span, "success", Some(10), Some(100));
        metrics::counter!("bifrost_scribe_memtable_bytes_inserted_total").increment(64);
        metrics::counter!("oracle_query_files_scanned_total").increment(3);
        metrics::gauge!("bifrost_scribe_active_memtable_bytes").set(5.0);
        metrics::gauge!("bifrost_oracle_local_slot_units", "kind" => "limit").set(8.0);
        // Verification: direct timings and phases, submission and queue-wait
        // histograms, Scribe acknowledgements, backlog gauges, and losses.
        wyrd_benchmark::record_timing_parts(
            &span,
            "verification.direct",
            "completed",
            ms(3),
            Some(ms(1)),
            &[("verification.direct.engine", ms(2))],
        );
        metrics::histogram!(verify::SUBMIT, "outcome" => "staged").record(0.001);
        metrics::histogram!(verify::QUEUE_WAIT, "origin" => "manual").record(0.002);
        metrics::histogram!(verify::SCRIBE_ACK).record(0.003);
        wyrd_benchmark::record_timing(&span, verify::SCRIBE_WRITE, "success", ms(3), None);
        metrics::gauge!("wyrd_verification_queue_depth").set(3.0);
        metrics::gauge!("wyrd_verification_queue_depth").decrement(1.0);
        metrics::gauge!("outbox_pending", "outbox" => "scribe").set(1.0);
        metrics::counter!("outbox_events_lost_total", "outbox" => "scribe", "reason" => "shed")
            .increment(1);
        // Gateway: handoff timings, capture and outbox write counts, the
        // accounting phase's observation count, and the active-call gauge.
        wyrd_benchmark::record_timing(&span, "gateway.buffered", "succeeded", ms(4), Some(ms(1)));
        metrics::counter!("wyrd_gateway_capture_total", "outcome" => "staged").increment(2);
        metrics::counter!("wyrd_gateway_capture_total", "outcome" => "dropped").increment(1);
        metrics::histogram!("outbox_write_seconds", "outbox" => "scribe").record(0.000_5);
        metrics::counter!("outbox_items_written_total", "outbox" => "scribe").increment(2);
        metrics::counter!("outbox_bytes_written_total", "outbox" => "scribe").increment(256);
        metrics::histogram!("wyrd_gateway_phase_duration_seconds", "phase" => "account")
            .record(0.000_1);
        metrics::gauge!("wyrd_gateway_active_calls").set(0.0);

        let after = wyrd_benchmark::snapshot();
        let last = Metrics::of_snapshot(&after).expect("a complete snapshot");
        let population = Population::between(&before, after).expect("one process");
        assert!(population.invalid.is_empty(), "{:?}", population.invalid);
        let grew =
            |family: &str, labels: &[&str]| last.sum(family, labels) - first.sum(family, labels);

        assert_eq!(population.count(QUERY_BOUNDARY), 1);
        assert_eq!(population.scans.len(), 1);
        assert_eq!(
            grew("bifrost_scribe_memtable_bytes_inserted_total", &[]),
            64.0
        );
        assert_eq!(grew("oracle_query_files_scanned_total", &[]), 3.0);
        assert_eq!(last.sum("bifrost_scribe_active_memtable_bytes", &[]), 5.0);
        assert_eq!(
            last.sum("bifrost_oracle_local_slot_units", &["kind=\"limit\""]),
            8.0
        );

        assert!(
            population
                .residuals("verification.direct", "completed")
                .is_ok()
        );
        assert!(verify::observed_components(&population));
        assert_eq!(verify::components(&population, 1).len(), 2);
        for histogram in [
            population.histogram(verify::SUBMIT, &[verify::STAGED]),
            population.histogram(verify::QUEUE_WAIT, &[verify::SUBMITTED]),
        ] {
            assert_eq!(verify::mean_ms(histogram).samples, Some(1));
        }
        assert_eq!(
            verify::mean_ms(verify::acknowledgements(&population)).samples,
            Some(2)
        );
        assert_eq!(verify::backlog(&last), 3.0, "two queued and one pending");
        assert_eq!(verify::lost_writes(&first, &last).value, Some(1.0));

        assert_eq!(
            population
                .residuals("gateway.buffered", "succeeded")
                .map(|residuals| residuals.len()),
            Ok(1)
        );
        let integrity = Integrity::between(&first, &last);
        assert_eq!(
            (
                integrity.staged,
                integrity.dropped,
                integrity.written,
                integrity.shed
            ),
            (2.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(integrity.unwritten(), None);
        assert_eq!(
            Durable::between(&first, &last),
            Durable {
                items: 2.0,
                bytes: 256.0
            }
        );
        assert_eq!(
            grew(
                "wyrd_gateway_phase_duration_seconds_count",
                &["phase=\"account\""]
            ),
            1.0,
            "admitted calls are the accounting phase's observations"
        );
        assert_eq!(last.sum("wyrd_gateway_active_calls", &[]), 0.0);
    }

    /// Timing boundaries a group of requests reaches, each with every closed
    /// outcome its owner records.
    type Reach = &'static [(&'static str, &'static [&'static str])];

    /// `bifrost.scribe.write` and the part boundaries every acknowledged
    /// write records beside it, always `success` (`scribe/profile.rs`).
    const SCRIBE: Reach = &[
        ("bifrost.scribe.write", &["success"]),
        ("bifrost.scribe.write.admission", &["success"]),
        ("bifrost.scribe.write.decode", &["success"]),
        ("bifrost.scribe.write.preprocess", &["success"]),
        ("bifrost.scribe.write.shard_wait", &["success"]),
        ("bifrost.scribe.write.wal_append", &["success"]),
        ("bifrost.scribe.write.wal_sync", &["success"]),
        ("bifrost.scribe.write.visibility", &["success"]),
        ("bifrost.scribe.write.ack", &["success"]),
    ];

    /// A public query: `QueryTiming` records its terminal, or `cancelled`
    /// when it drops first (`query/routes.rs`).
    const QUERY: Reach = &[("bifrost.query", &["success", "failed", "cancelled"])];

    /// A native write's Gate lifecycle outcomes (`gate/mod.rs`).
    const GATE: Reach = &[(
        "bifrost.gate.write",
        &["success", "rejected", "failed", "cancelled"],
    )];

    /// An OTLP traces export and its decode (`grpc/otlp.rs`).
    const TRACES: Reach = &[
        ("bifrost.otlp.traces", &["success", "failed"]),
        ("bifrost.otlp.traces.decode", &["success"]),
    ];

    /// An OTLP logs export and its decode (`grpc/otlp.rs`).
    const LOGS: Reach = &[
        ("bifrost.otlp.logs", &["success", "failed"]),
        ("bifrost.otlp.logs.decode", &["success"]),
    ];

    /// A direct Verification execution and each phase part it records,
    /// `completed` or `cancelled` on drop (`verification/telemetry.rs`).
    const DIRECT: Reach = &[
        ("verification.direct", &["completed", "cancelled"]),
        ("verification.direct.admit", &["completed", "cancelled"]),
        ("verification.direct.load", &["completed", "cancelled"]),
        (
            "verification.direct.input_read",
            &["completed", "cancelled"],
        ),
        ("verification.direct.prepare", &["completed", "cancelled"]),
        ("verification.direct.engine", &["completed", "cancelled"]),
        ("verification.direct.stage", &["completed", "cancelled"]),
        (
            "verification.direct.settlement",
            &["completed", "cancelled"],
        ),
        ("verification.direct.outside", &["completed", "cancelled"]),
    ];

    /// Every Gateway call outcome (`wyrd_gateway::outcome_name`).
    const CALL: &[&str] = &["succeeded", "failed", "cancelled", "timed_out"];

    /// A buffered Gateway call's handoff.
    const BUFFERED: Reach = &[("gateway.buffered", CALL)];

    /// A streamed Gateway call's head and end handoffs.
    const STREAMED: Reach = &[("gateway.stream.head", CALL), ("gateway.stream.end", CALL)];

    /// Every driver observation's terminal outcome (`wyrd_benchmark`).
    const OBSERVED: &[&str] = &["success", "error", "cancelled"];

    /// An SDK buffered call and its attempts (`wyrd-client` `transport/http.rs`).
    const SDK: Reach = &[("client.http", OBSERVED), ("client.http.attempt", OBSERVED)];

    /// An SDK `verify` flow (`wyrd-client` `observe/mod.rs`).
    const VERIFY: Reach = &[("client.verify", OBSERVED)];

    /// A raw Gateway call and its one attempt (`gateway/workload.rs`).
    const GATEWAY_CALL: Reach = &[
        ("client.gateway", OBSERVED),
        ("client.http.attempt", OBSERVED),
    ];

    /// A raw streamed Gateway call's head (`gateway/workload.rs`).
    const HEAD: Reach = &[("client.gateway.head", OBSERVED)];

    /// A local provider fixture answer (`gateway_capture.rs`).
    const FIXTURE: Reach = &[("fixture.upstream", OBSERVED)];

    /// Driver roots and the components each reaches, recorded per cohort in
    /// [`wyrd_benchmark::COMPONENT_SECONDS`].
    type Parts = &'static [(&'static str, &'static [&'static str])];

    /// An SDK buffered call's components.
    const SDK_PARTS: Parts = &[(
        "client.http",
        &["serialize", "auth", "send", "backoff", "body", "decode"],
    )];

    /// An SDK `verify` flow's components, with its request's.
    const VERIFY_PARTS: Parts = &[
        ("client.verify", &["prepare"]),
        (
            "client.http",
            &["serialize", "auth", "send", "backoff", "body", "decode"],
        ),
    ];

    /// A raw Gateway call's components.
    const GATEWAY_PARTS: Parts = &[("client.gateway", &["send", "body", "decode"])];

    /// A fixture answer's components.
    const FIXTURE_PARTS: Parts = &[("fixture.upstream", &["parse", "answer"])];

    /// The only Oracle outcome that records scan facts (`oracle/mod.rs`).
    const SCANNED: &[&str] = &["success"];

    /// Every L1 repetition phase ([`Phase::label`]).
    const L1: &[&str] = &["warmup", "repetition-1", "repetition-2", "repetition-3"];

    /// Every L2 admission phase.
    const L2: &[&str] = &["capacity", "overload", "recovery"];

    /// The case label of each Bifrost read.
    const READS: &[&str] = &[
        "point-lookup",
        "small-aggregate",
        "million-aggregate",
        "table-aggregate",
        "full-scan",
        "trace-lookup",
        "shredded-filter",
        "long-tail-filter",
    ];

    /// The case label of each competing L2 read, its probe included.
    const COMPETING_READS: &[&str] = &[
        "point-lookup",
        "small-aggregate",
        "million-aggregate",
        "table-aggregate",
        "full-scan",
        "probe",
    ];

    /// One benchmark server process's reachable series: each labelled group
    /// of cases and the boundaries its requests reach, in every phase; the
    /// unlabelled setup, fixture, and background work; and the cases whose
    /// queries record scan facts.
    struct Lifetime {
        /// The engine, level, and experiment it serves.
        name: &'static str,
        /// The phases its driver labels requests with.
        phases: &'static [&'static str],
        /// Each group of cases and the boundaries they reach.
        labelled: Vec<(&'static [&'static str], Vec<Reach>)>,
        /// Boundaries unlabelled work reaches.
        unlabelled: Vec<Reach>,
        /// The cases whose queries record scan facts.
        scanned: &'static [&'static str],
        /// Each group of cases and the root components they reach.
        parts: Vec<(&'static [&'static str], Vec<Parts>)>,
        /// Root components unlabelled work reaches.
        unlabelled_parts: Vec<Parts>,
    }

    impl Lifetime {
        /// Every distinct timing series key it can open.
        fn timings(&self) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
            let keys = |reaches: &[Reach]| -> Vec<(&'static str, &'static str)> {
                reaches
                    .iter()
                    .flat_map(|reach| reach.iter())
                    .flat_map(|(boundary, outcomes)| {
                        outcomes.iter().map(move |outcome| (*boundary, *outcome))
                    })
                    .collect()
            };
            let mut series: Vec<_> = keys(&self.unlabelled)
                .into_iter()
                .map(|(boundary, outcome)| {
                    (
                        boundary,
                        outcome,
                        wyrd_benchmark::UNLABELLED,
                        wyrd_benchmark::UNLABELLED,
                    )
                })
                .collect();
            for (cases, reaches) in &self.labelled {
                let keys = keys(reaches);
                for case in *cases {
                    for phase in self.phases {
                        series.extend(
                            keys.iter()
                                .map(|(boundary, outcome)| (*boundary, *outcome, *case, *phase)),
                        );
                    }
                }
            }
            series.sort_unstable();
            series.dedup();
            series
        }

        /// Every distinct component histogram key it can open: boundary,
        /// component, case, and phase.
        fn components(&self) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
            let keys = |parts: &[Parts]| -> Vec<(&'static str, &'static str)> {
                parts
                    .iter()
                    .flat_map(|parts| parts.iter())
                    .flat_map(|(boundary, components)| {
                        components
                            .iter()
                            .map(move |component| (*boundary, *component))
                    })
                    .collect()
            };
            let mut series: Vec<_> = keys(&self.unlabelled_parts)
                .into_iter()
                .map(|(boundary, component)| {
                    (
                        boundary,
                        component,
                        wyrd_benchmark::UNLABELLED,
                        wyrd_benchmark::UNLABELLED,
                    )
                })
                .collect();
            for (cases, parts) in &self.parts {
                let keys = keys(parts);
                for case in *cases {
                    for phase in self.phases {
                        series.extend(
                            keys.iter().map(|(boundary, component)| {
                                (*boundary, *component, *case, *phase)
                            }),
                        );
                    }
                }
            }
            series.sort_unstable();
            series.dedup();
            series
        }

        /// Every distinct scan series key it can open, unlabelled queries
        /// included.
        fn scans(&self) -> Vec<(&'static str, &'static str, &'static str)> {
            let mut scans: Vec<_> = SCANNED
                .iter()
                .map(|outcome| {
                    (
                        *outcome,
                        wyrd_benchmark::UNLABELLED,
                        wyrd_benchmark::UNLABELLED,
                    )
                })
                .collect();
            for case in self.scanned {
                for phase in self.phases {
                    scans.extend(SCANNED.iter().map(|outcome| (*outcome, *case, *phase)));
                }
            }
            scans
        }
    }

    /// Every declared engine and level's server process: each Bifrost L1
    /// experiment starts its own server, as do Bifrost L2, each
    /// Verification level, and each Gateway level.
    fn lifetimes() -> Vec<Lifetime> {
        let bifrost: Vec<Reach> = vec![QUERY, GATE, SCRIBE, TRACES, LOGS];
        let writes = || {
            vec![
                (&["native-write"][..], vec![GATE, SCRIBE]),
                (&["otlp-spans", "otlp-spans-1k"][..], vec![TRACES, SCRIBE]),
                (&["otlp-logs"][..], vec![LOGS, SCRIBE]),
            ]
        };
        let mut competing = writes();
        competing.push((COMPETING_READS, vec![QUERY]));
        let lifetime = |name, phases, labelled, unlabelled, scanned| Lifetime {
            name,
            phases,
            labelled,
            unlabelled,
            scanned,
            parts: Vec::new(),
            unlabelled_parts: Vec::new(),
        };
        let driver = |name, phases, labelled, unlabelled, parts| Lifetime {
            name,
            phases,
            labelled,
            unlabelled,
            scanned: &[],
            parts,
            unlabelled_parts: vec![SDK_PARTS, FIXTURE_PARTS],
        };
        let reads = |cases: &'static [&'static str]| vec![(cases, vec![SDK])];
        let mut writes_reads: Vec<(&'static [&'static str], Vec<Reach>)> = vec![(
            &["native-write", "otlp-spans", "otlp-spans-1k", "otlp-logs"][..],
            vec![SDK],
        )];
        writes_reads.push((COMPETING_READS, vec![SDK]));
        let verifications: &'static [&'static str] = &["direct-verify", "direct-verify-model"];
        let gateway_cases: &'static [&'static str] = &[
            "buffered-1kib-capture-metadata",
            "buffered-64kib-capture-metadata",
            "buffered-64kib-capture-payload",
        ];
        let streamed: &'static [&'static str] = &["streamed-1kib-capture-metadata"];
        let gateway_driver = |name, phases, buffered: &'static [&'static str]| {
            driver(
                name,
                phases,
                vec![
                    (buffered, vec![GATEWAY_CALL, FIXTURE]),
                    (streamed, vec![GATEWAY_CALL, HEAD, FIXTURE]),
                ],
                vec![SDK, FIXTURE],
                vec![
                    (buffered, vec![GATEWAY_PARTS, FIXTURE_PARTS]),
                    (streamed, vec![GATEWAY_PARTS, FIXTURE_PARTS]),
                ],
            )
        };
        let drivers = vec![
            driver(
                "l1 bifrost writes-parquet driver",
                L1,
                reads(&["native-write", "otlp-spans", "otlp-spans-1k", "otlp-logs"]),
                vec![SDK],
                vec![(
                    &["native-write", "otlp-spans", "otlp-spans-1k", "otlp-logs"][..],
                    vec![SDK_PARTS],
                )],
            ),
            driver(
                "l1 bifrost hot or parquet driver",
                L1,
                reads(READS),
                vec![SDK],
                vec![(READS, vec![SDK_PARTS])],
            ),
            driver(
                "l2 bifrost driver",
                L2,
                writes_reads.clone(),
                vec![SDK],
                writes_reads
                    .iter()
                    .map(|(cases, _)| (*cases, vec![SDK_PARTS]))
                    .collect(),
            ),
            driver(
                "l1 verification driver",
                L1,
                vec![(verifications, vec![VERIFY, SDK, FIXTURE])],
                vec![SDK, FIXTURE],
                vec![(verifications, vec![VERIFY_PARTS, FIXTURE_PARTS])],
            ),
            driver(
                "l2 verification driver",
                L2,
                vec![(verifications, vec![VERIFY, SDK, FIXTURE])],
                vec![SDK, FIXTURE],
                vec![(verifications, vec![VERIFY_PARTS, FIXTURE_PARTS])],
            ),
            gateway_driver("l1 gateway driver", L1, gateway_cases),
            gateway_driver("l2 gateway driver", L2, &["buffered-1kib-capture-metadata"]),
        ];
        let mut lifetimes = vec![
            lifetime(
                "l1 bifrost writes-parquet",
                L1,
                writes(),
                bifrost.clone(),
                &[],
            ),
            lifetime(
                "l1 bifrost hot",
                L1,
                vec![(READS, vec![QUERY])],
                bifrost.clone(),
                READS,
            ),
            lifetime(
                "l1 bifrost parquet",
                L1,
                vec![(READS, vec![QUERY])],
                bifrost.clone(),
                READS,
            ),
            lifetime("l2 bifrost", L2, competing, bifrost, COMPETING_READS),
            lifetime(
                "l1 verification",
                L1,
                vec![
                    (&["direct-verify"][..], vec![DIRECT]),
                    (&["direct-verify-model"][..], vec![DIRECT, BUFFERED]),
                ],
                vec![DIRECT, QUERY, SCRIBE, BUFFERED],
                &[],
            ),
            lifetime(
                "l2 verification",
                L2,
                vec![
                    (&["direct-verify"][..], vec![DIRECT]),
                    (&["direct-verify-model"][..], vec![DIRECT, BUFFERED]),
                ],
                vec![DIRECT, QUERY, SCRIBE, BUFFERED],
                &[],
            ),
            lifetime(
                "l1 gateway",
                L1,
                vec![
                    (
                        &[
                            "buffered-1kib-capture-metadata",
                            "buffered-64kib-capture-metadata",
                            "buffered-64kib-capture-payload",
                        ][..],
                        vec![BUFFERED],
                    ),
                    (&["streamed-1kib-capture-metadata"][..], vec![STREAMED]),
                ],
                vec![BUFFERED, STREAMED, QUERY, SCRIBE],
                &[],
            ),
            lifetime(
                "l2 gateway",
                L2,
                vec![
                    (&["buffered-1kib-capture-metadata"][..], vec![BUFFERED]),
                    (&["streamed-1kib-capture-metadata"][..], vec![STREAMED]),
                ],
                vec![BUFFERED, STREAMED, QUERY, SCRIBE],
                &[],
            ),
        ];
        lifetimes.extend(drivers);
        lifetimes
    }

    /// Every declared engine and level's server and driver process fits the
    /// timing, scan, and component series bounds: each lifetime's reachable
    /// cases, phases, outcomes, parts, and root components, setup and
    /// background work included, are at most the bound, and the largest are
    /// all retained with nothing dropped. Lifetimes are separate processes,
    /// so each is held against the bounds alone; the largest timing, scan,
    /// and component footprints are recorded together, since they fill
    /// separate bounds.
    ///
    /// # Panics
    ///
    /// Panics when a lifetime exceeds a bound, or a recorded series is
    /// dropped or not retained.
    #[test]
    fn declared_workloads_fit_recording_bounds() {
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let lifetimes = lifetimes();
        for lifetime in &lifetimes {
            let (timings, scans, components) = (
                lifetime.timings().len(),
                lifetime.scans().len(),
                lifetime.components().len(),
            );
            assert!(
                timings <= wyrd_benchmark::MAX_SERIES
                    && scans <= wyrd_benchmark::MAX_SCAN_SERIES
                    && components <= wyrd_benchmark::MAX_METRIC_SERIES,
                "{}: {timings} timing, {scans} scan, and {components} component series",
                lifetime.name
            );
        }
        let widest = |footprint: fn(&Lifetime) -> usize| {
            lifetimes
                .iter()
                .max_by_key(|lifetime| footprint(lifetime))
                .expect("declared lifetimes")
        };
        let timings = widest(|lifetime| lifetime.timings().len()).timings();
        let scans = widest(|lifetime| lifetime.scans().len()).scans();
        let components = widest(|lifetime| lifetime.components().len()).components();

        let _registry = tracing::subscriber::set_default(tracing_subscriber::registry());
        let span = |case: &str, phase: &str| {
            let span = tracing::info_span!("request");
            if case != wyrd_benchmark::UNLABELLED {
                let mut headers = http::HeaderMap::new();
                for (header, value) in [
                    (wyrd_benchmark::CASE_HEADER, case),
                    (wyrd_benchmark::PHASE_HEADER, phase),
                ] {
                    headers.insert(header, http::HeaderValue::from_str(value).expect("a label"));
                }
                wyrd_benchmark::label_request(&span, &headers);
            }
            span
        };
        for (boundary, outcome, case, phase) in &timings {
            wyrd_benchmark::record_timing(
                &span(case, phase),
                boundary,
                outcome,
                Duration::from_micros(1),
                None,
            );
        }
        for (outcome, case, phase) in &scans {
            wyrd_benchmark::record_scan(&span(case, phase), outcome, Some(1), Some(1));
        }
        let snapshot = wyrd_benchmark::snapshot();
        assert_eq!(snapshot.dropped, 0, "every reachable series is kept");
        assert_eq!(snapshot.invalid_labels, 0);
        assert_eq!(snapshot.series.len(), timings.len());
        assert_eq!(snapshot.scans.len(), scans.len());

        // Components fill the standard metric bound, apart from the timing
        // series their roots also open, which the lifetime's timings count.
        for (boundary, component, case, phase) in components.iter().copied() {
            metrics::histogram!(
                wyrd_benchmark::COMPONENT_SECONDS,
                "boundary" => boundary,
                "component" => component,
                "case" => case,
                "phase" => phase
            )
            .record(1e-6);
        }
        let snapshot = wyrd_benchmark::snapshot();
        assert_eq!(snapshot.dropped, 0, "every reachable component is kept");
        assert_eq!(snapshot.invalid_labels, 0);
        assert_eq!(
            snapshot
                .metrics
                .iter()
                .filter(|metric| metric.name == wyrd_benchmark::COMPONENT_SECONDS)
                .count(),
            components.len()
        );
    }
}
