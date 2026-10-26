//! The level commands every engine shares: `l1 <engine>`, the sequential
//! `l1` aggregate, `l2 <engine>`, and `report <record.json>`.
//!
//! An engine contributes only a [`Plan`]: its declaration (question, sweep,
//! controls, metrics, criteria, steps) and the [`Workload`] that drives its
//! production calls. This module fills the declaration's source, build, host,
//! and invocation, applies `--steps`, `--allow-dirty`, `--profile`, and
//! `--smoke`, runs a full plan's smoke prepass ([`prepass`]), runs
//! [`wyrd_testing::experiment::run`], publishes the record under
//! `benchmarks/<engine>/<experiment>/` (a smoke's under
//! [`SMOKE_DIRECTORY`]), prints the report, and returns the verdict's exit
//! code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wyrd_testing::capacity::{
    binary_identity, release_binary, timer_resolution_ns, timing_declaration,
};
use wyrd_testing::experiment::{
    Declaration, Engine, HostRecord, Invocation, Level, Mode, Options, Prepass, Record,
    SMOKE_DIRECTORY, Source, Validity, Verdict, Workload, run,
};
use wyrd_testing::profile::SAMPLY;
use wyrd_testing::release_server::{Host, HostLoad};

use crate::flags;

/// Switches every level command accepts.
pub const SWITCHES: [&str; 3] = ["--profile", "--allow-dirty", "--smoke"];

/// Valued flags every level command accepts.
pub const VALUED: [&str; 1] = ["--steps"];

/// The engines `l1` runs, in order.
pub const ENGINES: [Engine; 3] = [Engine::Bifrost, Engine::Verification, Engine::Gateway];

/// Where the declared server runs, as records keep it.
const STORAGE: &str = "file:// store in a temporary directory on the local filesystem";

/// What an engine's plan reads.
pub struct Context<'a> {
    /// The level.
    pub level: Level,
    /// The command line, program name first, without the level and engine.
    pub arguments: &'a [String],
    /// The repository root that holds `benchmarks/`.
    pub root: &'a Path,
}

impl Context<'_> {
    /// Refuses any argument that is neither a shared level flag nor one of
    /// the engine's own `valued` flags and `switches`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the first unknown argument.
    pub fn known(&self, valued: &[&str], switches: &[&str]) -> Result<(), String> {
        let valued: Vec<&str> = VALUED.iter().chain(valued).copied().collect();
        let switches: Vec<&str> = SWITCHES.iter().chain(switches).copied().collect();
        flags::known(self.arguments, &valued, &switches)
    }

    /// Whether `--smoke` asks for the plan's reduced functional smoke, so
    /// the engine declares its smoke-scale fixture.
    #[must_use]
    pub fn smoke(&self) -> bool {
        self.arguments.iter().any(|argument| argument == "--smoke")
    }

    /// Where `engine`'s `experiment` keeps its server logs and artifacts:
    /// under `target/benchmarks/`, or [`SMOKE_DIRECTORY`] for a smoke.
    #[must_use]
    pub fn output(&self, engine: Engine, experiment: &str) -> PathBuf {
        let history = if self.smoke() {
            SMOKE_DIRECTORY
        } else {
            "target/benchmarks"
        };
        self.root.join(history).join(engine.name()).join(experiment)
    }
}

/// One engine experiment ready to run.
pub struct Plan {
    /// The declaration, sealed by [`execute`] before any work.
    pub declaration: Declaration,
    /// The engine's production workload.
    pub workload: Box<dyn Workload>,
}

/// The declaration every engine starts from, read from this host, this
/// checkout, and the built release server; see [`assemble`].
///
/// # Errors
///
/// Returns a message when the host or the release server cannot be read.
pub fn base(engine: Engine, context: &Context<'_>) -> Result<Declaration, String> {
    let host = Host::detect().map_err(|error| format!("cannot read the host: {error}"))?;
    let allow_dirty = context
        .arguments
        .iter()
        .any(|argument| argument == "--allow-dirty");
    let server = release_binary().map_err(|error| error.to_string())?;
    let driver = std::env::current_exe().map_err(|error| error.to_string())?;
    let compiler = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let build = json!({
        "profile": "release",
        "rustflags": "-C force-frame-pointers=yes",
        "debug": "line-tables-only",
        "compiler": compiler,
        "server": binary_identity(&server).map_err(|error| error.to_string())?,
        "driver": binary_identity(&driver).map_err(|error| error.to_string())?,
    });
    let config: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| name.starts_with("WYRD_"))
        .collect();
    let config: Vec<(&str, &str)> = config
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    Ok(assemble(
        engine,
        context,
        Source::detect(context.root, allow_dirty),
        build,
        HostRecord::new(&host, STORAGE),
        &config,
    ))
}

/// `engine` at the context's level with the level's timing, the default
/// validity thresholds, the server timing method with this host's measured
/// timer resolution as the `timing` control, and the measured `source`,
/// `build`, `host`, and invocation with its `WYRD_*` `config`. The engine fills the experiment
/// id, question, sweep, controls, metrics, criteria, and steps; a
/// declaration it leaves incomplete is refused before any work.
pub fn assemble(
    engine: Engine,
    context: &Context<'_>,
    source: Source,
    build: serde_json::Value,
    host: HostRecord,
    config: &[(&str, &str)],
) -> Declaration {
    let mut command = vec![
        format!("l{}", u8::from(context.level)),
        engine.name().to_owned(),
    ];
    command.extend(context.arguments.iter().skip(1).cloned());
    let mut timing = timing_declaration();
    timing["timer_resolution_ns"] = json!(timer_resolution_ns());
    Declaration {
        engine,
        experiment: String::new(),
        level: context.level,
        question: String::new(),
        timing: context.level.timing(),
        validity: Validity::default(),
        variables: BTreeMap::new(),
        controls: BTreeMap::from([("timing".to_owned(), timing)]),
        metrics: Vec::new(),
        criteria: Vec::new(),
        steps: Vec::new(),
        selection: None,
        source,
        build,
        command: Invocation::new(&command, config),
        host,
        refusals: Vec::new(),
        mode: Mode::Full,
        prepass: None,
    }
}

/// Applies `--steps` to `declaration`: every named step must be declared;
/// a selection records omissions and never claims full coverage.
///
/// # Errors
///
/// Returns a message for a malformed list or an undeclared step.
pub fn select(declaration: &mut Declaration, arguments: &[String]) -> Result<(), String> {
    let declared: Vec<String> = declaration
        .steps
        .iter()
        .map(|step| step.id.clone())
        .collect();
    let expected = format!("step ids from {declared:?}");
    declaration.selection = flags::list(arguments, "--steps", &expected, |item| {
        declared.iter().find(|step| *step == item).cloned()
    })?;
    Ok(())
}

/// Turns `declaration` into its smoke: [`Mode::Smoke`] with the level's
/// [`Level::smoke_timing`]. The engine already declared its smoke-scale
/// fixture from [`Context::smoke`]; every step, phase, and check stays.
pub fn smoke(declaration: &mut Declaration) {
    declaration.mode = Mode::Smoke;
    declaration.timing = declaration.level.smoke_timing();
}

/// Runs the smoke of `declaration`'s exact command (`arguments`, program
/// name first, without the level and engine) as a child under its own fresh
/// Postgres, server, and storage ([`spawn`]), outside the full run's clock,
/// and records it. `--profile` is not forwarded; `--smoke` makes the child
/// a smoke, so it never runs a prepass of its own.
pub fn prepass(root: &Path, declaration: &Declaration, arguments: &[String]) -> Prepass {
    let mut command = vec![
        format!("l{}", u8::from(declaration.level)),
        declaration.engine.name().to_owned(),
    ];
    command.extend(
        arguments
            .iter()
            .skip(1)
            .filter(|argument| *argument != "--profile")
            .cloned(),
    );
    command.push("--smoke".to_owned());
    let started = Instant::now();
    let named: Vec<&str> = command.iter().map(String::as_str).collect();
    let exit_code = spawn(root, &named, &[]);
    Prepass {
        command,
        budget_seconds: declaration.level.smoke_timing().budget_seconds,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        exit_code,
        records: root
            .join(SMOKE_DIRECTORY)
            .join(declaration.engine.name())
            .join(&declaration.experiment)
            .display()
            .to_string(),
    }
}

/// Runs a plan: seals its declaration, samples the host, runs the workload
/// under the shared lifecycle, publishes the record, prints the report, and
/// returns the verdict's exit code. A record that cannot be published exits
/// 2 and is kept under the run's artifacts directory instead.
pub async fn execute(plan: Plan, root: &Path, profile: bool) -> u8 {
    let Plan {
        declaration,
        mut workload,
    } = plan;
    let history = match declaration.mode {
        Mode::Full => "target/benchmarks",
        Mode::Smoke => SMOKE_DIRECTORY,
    };
    let artifacts = root
        .join(history)
        .join(declaration.engine.name())
        .join(&declaration.experiment)
        .join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string());
    let interrupt = CancellationToken::new();
    let interrupted = interrupt.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("interrupted: stopping admission and draining");
            interrupted.cancel();
        }
    });
    let options = Options {
        artifacts: artifacts.clone(),
        profiler: profile.then(|| PathBuf::from(SAMPLY)),
        interrupt,
    };
    let record = run(
        declaration.seal(),
        workload.as_mut(),
        HostLoad::sample,
        &options,
    )
    .await;
    println!("{}", record.render());
    match record.publish(root) {
        Ok(path) => {
            println!("record: {}", path.display());
            record.exit_code()
        }
        Err(error) => {
            eprintln!("INVALID: {error}");
            let salvage = artifacts.join("unpublished-record.json");
            let kept = std::fs::create_dir_all(&artifacts).and_then(|()| {
                std::fs::write(
                    &salvage,
                    serde_json::to_vec_pretty(&record).unwrap_or_default(),
                )
            });
            match kept {
                Ok(()) => eprintln!("the unpublished record is kept at {}", salvage.display()),
                Err(error) => eprintln!("the unpublished record could not be kept: {error}"),
            }
            Verdict::Invalid.exit_code()
        }
    }
}

/// Runs `l1` for every engine in order through `run_engine`, forwarding the
/// shared arguments, and combines their exit codes by precedence (2 over 1
/// over 0). A failed or invalid engine never stops the later engines, so each
/// keeps its own record.
pub fn aggregate(arguments: &[String], mut run_engine: impl FnMut(Engine, &[String]) -> u8) -> u8 {
    let forwarded: Vec<String> = arguments.iter().skip(1).cloned().collect();
    let codes: Vec<u8> = ENGINES
        .iter()
        .map(|engine| {
            let code = run_engine(*engine, &forwarded);
            eprintln!("l1 {}: exit {code}", engine.name());
            code
        })
        .collect();
    Verdict::combine(&codes)
}

/// Runs one engine's `l1` as a child of this binary under the repository's
/// test Postgres wrapper, so each engine starts from a fresh database;
/// Bifrost runs every experiment through [`spawn_experiments`].
pub fn spawn_engine(root: &Path, engine: Engine, forwarded: &[String]) -> u8 {
    if engine == Engine::Bifrost {
        spawn_experiments(root, &crate::query::Experiment::ALL, forwarded)
    } else {
        spawn(root, &["l1", engine.name()], forwarded)
    }
}

/// Runs each Bifrost L1 experiment of `experiments` in order as its own
/// child under a fresh database ([`spawn`]) and combines their exit codes
/// by precedence, never stopping early.
pub fn spawn_experiments(
    root: &Path,
    experiments: &[crate::query::Experiment],
    forwarded: &[String],
) -> u8 {
    let codes: Vec<u8> = experiments
        .iter()
        .map(|experiment| {
            let code = spawn(
                root,
                &["l1", Engine::Bifrost.name(), experiment.name()],
                forwarded,
            );
            eprintln!("l1 bifrost {}: exit {code}", experiment.name());
            code
        })
        .collect();
    Verdict::combine(&codes)
}

/// Runs this binary with `command` and `forwarded` as a child under the
/// repository's test Postgres wrapper, so it starts from a fresh database,
/// and returns its exit code capped at 2.
pub fn spawn(root: &Path, command: &[&str], forwarded: &[String]) -> u8 {
    let status = std::env::current_exe().and_then(|program| {
        Command::new(root.join("scripts/postgres/with-test-postgres.sh"))
            .current_dir(root)
            .arg("--")
            .arg(program)
            .args(command)
            .args(forwarded)
            .status()
    });
    match status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(Verdict::Invalid.exit_code(), |code| code.min(2)),
        Err(error) => {
            eprintln!("{}: could not start: {error}", command.join(" "));
            Verdict::Invalid.exit_code()
        }
    }
}

/// Reads a persisted record and prints its report.
///
/// # Errors
///
/// Returns a message when the record is unreadable, malformed, or of an
/// unsupported schema version.
pub fn report(path: &Path) -> Result<String, String> {
    Record::read(path)
        .map(|record| record.render())
        .map_err(|error| error.to_string())
}
