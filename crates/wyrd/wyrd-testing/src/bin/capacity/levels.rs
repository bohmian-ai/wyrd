//! The level plumbing every engine shares: the declaration base, flags,
//! the L2 `l2 <engine>` run, and `report <record.json>`; the L1 sweep and
//! case process live in [`crate::sweep`].
//!
//! An engine contributes only a [`Plan`]: its declaration (question, sweep,
//! controls, metrics, criteria, steps) and the [`Workload`] that drives its
//! production calls. This module fills the declaration's source, build, host,
//! and invocation, applies `--steps`, `--allow-dirty`, `--profile`,
//! and `--smoke`, runs
//! [`wyrd_testing::experiment::run`], publishes the record
//! ([`wyrd_testing::experiment::Record::publish`]), prints the report, and
//! returns the verdict's exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wyrd_testing::capacity::{
    binary_identity, release_binary, timer_resolution_ns, timing_declaration,
};
use wyrd_testing::experiment::Profiling;
use wyrd_testing::experiment::{
    Declaration, Engine, HISTORY_DIRECTORY, HostRecord, Invocation, LOCAL_DIRECTORY, Level, Mode,
    Options, Record, SMOKE_DIRECTORY, Source, Validity, Verdict, Workload, run,
};
use wyrd_testing::profile::{ProfileMode, SAMPLY, SYMBOLS, SymbolizedCapture};
use wyrd_testing::release_server::{Host, HostLoad};

use crate::flags;

/// Switches every level command accepts.
pub const SWITCHES: [&str; 3] = ["--profile", "--allow-dirty", "--smoke"];

/// Valued flags every level command accepts.
pub const VALUED: [&str; 1] = ["--steps"];

/// The engines, in documentation order.
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
        known(self.arguments, &valued, switches)
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
            LOCAL_DIRECTORY
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
/// id, question, sweep, controls, metrics, criteria, and steps;
/// a declaration it leaves incomplete is refused before any work.
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

/// The profile mode `--profile` selects: `None` without it, [`ProfileMode::Both`]
/// when bare, else the `cpu`, `heap`, or `both` that follows it.
///
/// # Errors
///
/// Returns a message when `--profile` is given twice or followed by
/// anything but a mode or another flag.
pub fn profile(arguments: &[String]) -> Result<Option<ProfileMode>, String> {
    let mut found = None;
    for (index, argument) in arguments.iter().enumerate() {
        if argument != "--profile" {
            continue;
        }
        if found.is_some() {
            return Err("--profile is given twice".to_owned());
        }
        let mode = match arguments
            .get(index + 1)
            .filter(|next| !next.starts_with("--"))
        {
            None => ProfileMode::Both,
            Some(value) => ProfileMode::parse(value)
                .ok_or_else(|| format!("--profile takes cpu, heap, or both, got `{value}`"))?,
        };
        found = Some(mode);
    }
    Ok(found)
}

/// Refuses any argument that is neither a shared level flag, with
/// `--profile`'s optional mode, nor one of `valued` and `switches`.
///
/// # Errors
///
/// Returns a message naming an invalid `--profile` or the first unknown
/// argument.
pub fn known(arguments: &[String], valued: &[&str], switches: &[&str]) -> Result<(), String> {
    let mode = profile(arguments)?.and_then(|_| {
        let index = arguments
            .iter()
            .position(|argument| argument == "--profile")?;
        arguments
            .get(index + 1)
            .filter(|next| !next.starts_with("--"))
            .map(|_| index + 1)
    });
    let rest: Vec<String> = arguments
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != mode)
        .map(|(_, argument)| argument.clone())
        .collect();
    let switches: Vec<&str> = SWITCHES.iter().chain(switches).copied().collect();
    flags::known(&rest, valued, &switches)
}

/// Runs a plan: seals its declaration, samples the host, runs the workload
/// under the shared lifecycle, publishes the record, prints the report, and
/// returns the verdict's exit code. A record that cannot be published exits
/// 2 and is kept under the run's artifacts directory instead.
pub async fn execute(plan: Plan, root: &Path, profile: Option<ProfileMode>) -> u8 {
    let Plan {
        declaration,
        mut workload,
    } = plan;
    let history = match declaration.mode {
        Mode::Full => LOCAL_DIRECTORY,
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
        profile: profile.map(|mode| Profiling {
            mode,
            program: PathBuf::from(SAMPLY),
            symbols: root.join(SYMBOLS),
        }),
        interrupt,
        setup_spent: std::time::Duration::ZERO,
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

/// Reads a persisted record and returns its report: an L1 sweep record's
/// category table, an L1 case record's full report, or an experiment
/// record's report.
///
/// # Errors
///
/// Returns a message when the record is unreadable, malformed, or of an
/// unsupported protocol or schema version.
pub fn report(path: &Path) -> Result<String, String> {
    if let Some(report) = crate::summary::report(path)? {
        return Ok(report);
    }
    Record::read_with_timings(path)
        .map(|record| record.render())
        .map_err(|error| error.to_string())
}

/// Reads two persisted records of one experiment and returns
/// [`Record::compare`] of `before` against `after`.
///
/// # Errors
///
/// Returns a message when a record is unreadable, malformed, or of an
/// unsupported schema version, or when the two do not compare.
pub fn compare(before: &Path, after: &Path) -> Result<String, String> {
    let before = Record::read(before).map_err(|error| error.to_string())?;
    let after = Record::read(after).map_err(|error| error.to_string())?;
    before.compare(&after)
}

/// Functions each sampled thread group of a CPU capture spends its samples
/// in, from the capture's Samply sidecar: [`SymbolizedCapture::report`] of
/// the threads whose name contains `threads`, the top [`PROFILE_TOP`] each.
///
/// # Errors
///
/// Returns a message when the capture or its sidecar cannot be read.
pub fn cpu_report(capture: &Path, threads: &str) -> Result<String, String> {
    SymbolizedCapture::read(capture)
        .map(|read| read.report(threads, PROFILE_TOP))
        .map_err(|error| error.to_string())
}

/// Functions a `profile` report lists per thread group and ordering.
const PROFILE_TOP: usize = 40;

/// The results page; [`results`] replaces its [`RESULTS_SLOT`] with the
/// records it shows.
const RESULTS_PAGE: &str = include_str!("results.html");

/// The placeholder in [`RESULTS_PAGE`] the records replace.
const RESULTS_SLOT: &str = "/*RECORDS*/[]";

/// Writes one self-contained HTML page of every record in `root`'s
/// [`HISTORY_DIRECTORY`]`/<engine>/<experiment>/` to `out` and returns how
/// many it holds. The page renders each record and compares the runs of
/// one experiment in chronological order; it reads nothing else.
///
/// Each record is embedded as the recorder reads it. A file that does not
/// read as a current record is embedded as stored with the reason, so the
/// page shows it marked rather than hiding history.
///
/// # Errors
///
/// Returns a message when the history cannot be listed, a file cannot be
/// read or is not JSON, or the page cannot be written.
pub fn results(root: &Path, out: &Path) -> Result<usize, String> {
    let history = root.join(HISTORY_DIRECTORY);
    let mut paths = Vec::new();
    for engine in children(&history)? {
        for experiment in children(&engine)? {
            paths.extend(
                children(&experiment)?
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|ext| ext == "json")),
            );
        }
    }
    paths.sort();
    let entries = paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let (record, problem) = match Record::parse(&text) {
                Ok(record) => (
                    serde_json::to_value(record).map_err(|error| error.to_string())?,
                    None,
                ),
                Err(error) => (
                    serde_json::from_str(&text)
                        .map_err(|json| format!("{}: {json}", path.display()))?,
                    Some(error.to_string()),
                ),
            };
            let path = path.strip_prefix(root).unwrap_or(path);
            Ok(json!({ "path": path, "record": record, "problem": problem }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let data = serde_json::to_string(&entries)
        .map_err(|error| error.to_string())?
        .replace('<', "\\u003c");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(out, RESULTS_PAGE.replacen(RESULTS_SLOT, &data, 1))
        .map_err(|error| format!("{}: {error}", out.display()))?;
    Ok(entries.len())
}

/// The entries of directory `path`, sorted; none when it does not exist.
///
/// # Errors
///
/// Returns a message when an existing directory cannot be listed.
fn children(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = std::fs::read_dir(path)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()
        })
        .map_err(|error| format!("{}: {error}", path.display()))?;
    entries.sort();
    Ok(entries)
}
