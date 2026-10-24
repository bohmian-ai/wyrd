//! The level commands every engine shares: `l1 <engine>`, the sequential
//! `l1` aggregate, `l2 <engine>`, `smoke [l1|l2 <engine>]`, and
//! `report <record.json>`.
//!
//! An engine contributes only a [`Plan`]: its declaration (question, sweep,
//! controls, metrics, criteria, steps) and the [`Workload`] that drives its
//! production calls. This module fills the declaration's source, build, host,
//! and invocation, applies `--steps`, `--allow-dirty`, and `--profile`, runs
//! [`wyrd_testing::experiment::run`], publishes the record under
//! `benchmarks/<engine>/<experiment>/`, prints the report, and returns the
//! verdict's exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wyrd_testing::capacity::{Collector, binary_identity, release_binary, timer_resolution_ns};
use wyrd_testing::experiment::{
    Declaration, Engine, HostRecord, Invocation, Level, Options, Record, Source, Validity, Verdict,
    Workload, run,
};
use wyrd_testing::profile::SAMPLY;
use wyrd_testing::release_server::{Host, HostLoad};

use crate::flags;

/// Switches every level command accepts.
pub const SWITCHES: [&str; 2] = ["--profile", "--allow-dirty"];

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
    let mut timing = Collector::declaration();
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

/// Runs a plan: seals its declaration, samples the host, runs the workload
/// under the shared lifecycle, publishes the record, prints the report, and
/// returns the verdict's exit code. A record that cannot be published exits
/// 2 and is kept under the run's artifacts directory instead.
pub async fn execute(plan: Plan, root: &Path, profile: bool) -> u8 {
    let Plan {
        declaration,
        mut workload,
    } = plan;
    let artifacts = root
        .join("target/benchmarks")
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
/// test Postgres wrapper, so each engine starts from a fresh database.
pub fn spawn_engine(root: &Path, engine: Engine, forwarded: &[String]) -> u8 {
    spawn(root, &["l1", engine.name()], forwarded)
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

/// How long a standalone smoke may take before it is a failure.
const SMOKE_BUDGET: Duration = Duration::from_secs(120);

/// Runs only a plan's release smoke ([`Workload::smoke`]) within
/// [`SMOKE_BUDGET`], then stops its servers. Nothing is recorded or
/// published; diagnostics stay under `target/benchmarks/`. Exits 0 when
/// every boundary, component, resource, and counter was observed, and 2
/// with the engine's missing evidence otherwise.
pub async fn smoke(plan: Plan) -> u8 {
    let Plan {
        declaration,
        mut workload,
    } = plan;
    let label = format!(
        "smoke l{} {}",
        u8::from(declaration.level),
        declaration.engine.name()
    );
    let smoked = tokio::time::timeout(SMOKE_BUDGET, workload.smoke()).await;
    let finished = workload.finish().await;
    let code = match smoked {
        Ok(Ok(proved)) => {
            println!("{label}: PASS: {proved}");
            0
        }
        Ok(Err(error)) => {
            eprintln!("{label}: INVALID: {error}");
            Verdict::Invalid.exit_code()
        }
        Err(_) => {
            eprintln!("{label}: INVALID: the smoke overran {SMOKE_BUDGET:?}");
            Verdict::Invalid.exit_code()
        }
    };
    if let Err(error) = finished {
        eprintln!("{label}: INVALID: the servers did not stop: {error}");
        return Verdict::Invalid.exit_code();
    }
    code
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
