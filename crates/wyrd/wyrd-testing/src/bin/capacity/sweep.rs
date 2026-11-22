//! The L1 sweep supervisor and the case process it starts.
//!
//! A sweep runs its resolved cases one at a time. Each case is a fresh
//! process of this binary (`capacity case ...`) under the repository
//! PostgreSQL wrapper, with its own named Compose project and its own
//! temporary root for server storage and WAL, so no server, database,
//! storage, client, SDK or provider state outlives it. The case process runs
//! the engine plan of exactly that case through the shared runner on the
//! case clock ([`Level::case_timing`]) and keeps its engine record. The
//! supervisor owns the case deadline, confirms every owned process,
//! container, volume and storage root is gone, keeps the [`CaseRecord`],
//! and records the outcome in the [`SweepRecord`]. A case whose cleanup
//! cannot be confirmed within its teardown allowance stops the sweep: every
//! later case is recorded NOT RUN.

use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_testing::experiment::{
    Allowances, Declaration, Level, Mode, Options, Owned, Profiling, Record, StepKind, Verdict, run,
};
use wyrd_testing::profile::{ProfileMode, SAMPLY, SYMBOLS};
use wyrd_testing::release_server::HostLoad;

use crate::cases::{Case, Category, Selection};
use crate::flags;
use crate::levels::{self, Context};
use crate::summary::{
    self, CASE_PROTOCOL, CaseRecord, CaseStatus, Cleanup, PROTOCOL_VERSION, SWEEP_FILE,
    SWEEP_PROTOCOL, SweepEntry, SweepMode, SweepRecord,
};

/// Where sweeps keep their records, under the repository root.
const SWEEPS: &str = "target/benchmarks/l1";

/// Where harness smoke sweeps keep their records.
const SMOKE_SWEEPS: &str = "target/benchmarks/smoke/l1";

/// The wrapper that owns each case's PostgreSQL.
const POSTGRES: &str = "scripts/postgres/with-test-postgres.sh";

/// The variable naming the Compose project the wrapper creates.
const PROJECT: &str = "WYRD_TEST_POSTGRES_PROJECT";

/// How often cleanup confirmation polls.
const POLL: Duration = Duration::from_millis(250);

/// One sweep: its resolved cases, mode and record directory.
#[derive(Debug, Clone)]
pub struct Sweep {
    /// The category.
    pub category: Category,
    /// The cases, in run order.
    pub cases: Vec<Case>,
    /// Whether `--cases` chose them.
    pub explicit: bool,
    /// The profile mode, when profiling.
    pub profile: Option<ProfileMode>,
    /// The mode.
    pub mode: SweepMode,
    /// Flags every case receives unchanged.
    pub forwarded: Vec<String>,
    /// The invocation, program name first.
    pub invocation: Vec<String>,
    /// The sweep's record directory.
    pub directory: PathBuf,
    /// The repository root.
    pub root: PathBuf,
    /// The sweep's start stamp, which names its Compose projects.
    pub stamp: String,
    /// The `capacity` program each case process runs: this binary.
    pub program: PathBuf,
}

impl Sweep {
    /// The sweep of `selection` from `invocation` at `root`, keeping its
    /// records in a new stamped directory; `smoke` runs the harness smoke.
    #[must_use]
    pub fn new(selection: Selection, invocation: &[String], root: &Path, smoke: bool) -> Self {
        let stamp = Utc::now().format("%Y%m%dt%H%M%S%3fz").to_string();
        let mode = match (smoke, selection.profile) {
            (true, _) => SweepMode::Smoke,
            (false, Some(_)) => SweepMode::Diagnostic,
            (false, None) => SweepMode::Standard,
        };
        let base = if smoke { SMOKE_SWEEPS } else { SWEEPS };
        Self {
            directory: root.join(base).join(selection.category.name()).join(&stamp),
            category: selection.category,
            cases: selection.cases,
            explicit: selection.explicit,
            profile: selection.profile,
            mode,
            forwarded: selection.forwarded,
            invocation: invocation.to_vec(),
            root: root.to_path_buf(),
            stamp,
            program: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("capacity")),
        }
    }

    /// The file stem of the `n`th case (from 1): `NN-<family>-<id>`.
    #[must_use]
    pub fn name(n: usize, case: &Case) -> String {
        format!("{n:02}-{}-{}", case.family, case.id)
    }

    /// The sweep record with `entries` so far.
    fn record(&self, started_at: chrono::DateTime<Utc>, entries: Vec<SweepEntry>) -> SweepRecord {
        SweepRecord {
            protocol: SWEEP_PROTOCOL.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            category: self.category.name().to_owned(),
            invocation: self.invocation.clone(),
            mode: self.mode,
            profile: self.profile,
            explicit: self.explicit,
            started_at,
            finished_at: Utc::now(),
            cases: entries,
        }
    }

    /// Runs every case through `launch` in order, keeping each case record
    /// and the sweep record after every case, and returns the sweep record.
    /// A case whose cleanup is not confirmed stops admission: every later
    /// case is NOT RUN with the reason. An interrupt does the same.
    pub async fn supervise<F, Fut>(
        &self,
        interrupt: &CancellationToken,
        mut launch: F,
    ) -> SweepRecord
    where
        F: FnMut(Case, usize) -> Fut,
        Fut: Future<Output = CaseRecord>,
    {
        let started_at = Utc::now();
        let mut entries: Vec<SweepEntry> = Vec::new();
        let mut stopped: Option<String> = None;
        for (index, case) in self.cases.iter().enumerate() {
            let n = index + 1;
            if stopped.is_none() && interrupt.is_cancelled() {
                stopped = Some("the sweep was interrupted before it started".to_owned());
            }
            if let Some(reason) = &stopped {
                entries.push(SweepEntry {
                    selector: case.selector(),
                    family: case.family.to_owned(),
                    case: case.id.to_owned(),
                    status: CaseStatus::NotRun,
                    record: None,
                    reasons: vec![reason.clone()],
                });
                continue;
            }
            eprintln!(
                "l1 {}: case {n}/{} {}",
                self.category.name(),
                self.cases.len(),
                case.selector()
            );
            let record = launch(*case, n).await;
            let kept = self.keep_case(&format!("{}.json", Self::name(n, case)), &record);
            let entry = summary::entry(case, &record, kept, Allowances::L1.persist_seconds);
            eprintln!(
                "l1 {}: {} {}",
                self.category.name(),
                case.selector(),
                entry.status.label()
            );
            if !record.cleanup.confirmed {
                stopped = Some(format!(
                    "not started: cleanup of `{}` could not be confirmed within its teardown allowance",
                    case.selector()
                ));
            }
            entries.push(entry);
            if let Err(error) = self.keep_sweep(&self.record(started_at, entries.clone())) {
                eprintln!(
                    "l1 {}: the sweep record could not be kept: {error}",
                    self.category.name()
                );
            }
        }
        let record = self.record(started_at, entries);
        if let Err(error) = self.keep_sweep(&record) {
            eprintln!(
                "l1 {}: the sweep record could not be kept: {error}",
                self.category.name()
            );
        }
        record
    }

    /// Keeps `record` as `name` beside the sweep record without replacing
    /// an existing file, returning the name and the seconds it took.
    ///
    /// # Errors
    ///
    /// Returns why it could not be encoded, written, synced or linked.
    pub fn keep_case(&self, name: &str, record: &CaseRecord) -> Result<(String, f64), String> {
        let started = std::time::Instant::now();
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
        let path = self.directory.join(name);
        keep(&self.directory, &bytes)?
            .persist_noclobber(&path)
            .map_err(|error| format!("{}: {}", path.display(), error.error))?;
        Ok((name.to_owned(), started.elapsed().as_secs_f64()))
    }

    /// Atomically replaces the sweep record, returning its path.
    ///
    /// # Errors
    ///
    /// Returns why it could not be encoded, written, synced or renamed.
    pub fn keep_sweep(&self, record: &SweepRecord) -> Result<PathBuf, String> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
        let path = self.directory.join(SWEEP_FILE);
        keep(&self.directory, &bytes)?
            .persist(&path)
            .map_err(|error| format!("{}: {}", path.display(), error.error))?;
        Ok(path)
    }

    /// Runs case `case`, the `n`th, as its own process under the PostgreSQL
    /// wrapper and returns its case record: started only when its Compose
    /// project does not yet exist, stopped at the case deadline, then
    /// confirmed gone (processes, containers, volumes and storage root)
    /// within the teardown allowance.
    pub async fn attempt(&self, case: Case, n: usize, interrupt: &CancellationToken) -> CaseRecord {
        let allowances = Allowances::L1;
        let name = Self::name(n, &case);
        let artifacts = self.directory.join(&name);
        let storage = artifacts.join("tmp");
        let record_path = artifacts.join("engine-record.json");
        let project = format!("wyrd-l1-{}-{n}", self.stamp);
        let started_at = Utc::now();
        let clock = Instant::now();
        let mut reasons: Vec<String> = Vec::new();
        let mut cleanup = Cleanup {
            project: project.clone(),
            storage_root: storage.display().to_string(),
            ..Cleanup::default()
        };
        match compose_resources(&project).await {
            Ok(found) => cleanup.preexisting = found,
            Err(error) => reasons.push(format!(
                "PostgreSQL ownership could not be checked: {error}"
            )),
        }
        if let Err(error) = std::fs::create_dir_all(&storage) {
            reasons.push(format!(
                "the case storage root could not be created: {error}"
            ));
        }
        if std::fs::read_dir(&storage)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true)
        {
            cleanup
                .preexisting
                .push(format!("storage root {} is not empty", storage.display()));
        }
        if !cleanup.preexisting.is_empty() {
            reasons.push("the case's resources existed before it started".to_owned());
            cleanup.confirmed = false;
            return self.finished(case, started_at, None, cleanup, reasons);
        }
        let mut command = tokio::process::Command::new(self.root.join(POSTGRES));
        command
            .current_dir(&self.root)
            .arg("--")
            .arg(&self.program)
            .args([
                "case",
                self.category.name(),
                &format!("{}/{}", case.family, case.id),
            ])
            .arg("--record")
            .arg(&record_path)
            .arg("--artifacts")
            .arg(&artifacts)
            .args(["--started", &started_at.timestamp_millis().to_string()])
            .args(&self.forwarded)
            .env(PROJECT, &project)
            .env("TMPDIR", &storage)
            .process_group(0)
            .kill_on_drop(true);
        if let Some(profile) = self.profile {
            command.args(["--profile", profile.name()]);
        }
        if self.mode == SweepMode::Smoke {
            command.arg("--smoke");
        }
        let deadline = clock
            + Duration::from_secs(
                allowances.total() - allowances.persist_seconds - allowances.teardown_seconds,
            );
        let teardown = Duration::from_secs(allowances.teardown_seconds);
        match command.spawn() {
            Err(error) => reasons.push(format!("the case process could not start: {error}")),
            Ok(mut child) => {
                let group = child.id();
                let status = tokio::select! {
                    status = child.wait() => Some(status),
                    () = tokio::time::sleep_until(deadline) => {
                        reasons.push(format!(
                            "the case did not finish within its {}s deadline; it was stopped",
                            deadline.duration_since(clock).as_secs()
                        ));
                        None
                    }
                    () = interrupt.cancelled() => {
                        reasons.push("the sweep was interrupted; the case was stopped".to_owned());
                        None
                    }
                };
                let status = match status {
                    Some(status) => status.ok(),
                    None => stop(group, &mut child, teardown).await,
                };
                match status {
                    Some(status) if status.success() || status.code() == Some(2) => {}
                    Some(status) => reasons.push(format!("the case process exited with {status}")),
                    None => reasons.push("the case process could not be reaped".to_owned()),
                }
            }
        }
        let exited = Utc::now();
        let record = std::fs::read(&record_path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                Record::parse(&String::from_utf8_lossy(&bytes)).map_err(|error| error.to_string())
            });
        let mut record = match record {
            Ok(record) => Some(record),
            Err(error) => {
                reasons.push(format!("the case process kept no engine record: {error}"));
                None
            }
        };
        let owned: Vec<Owned> = record
            .as_ref()
            .and_then(|record| record.lifecycle.as_ref())
            .map(|lifecycle| lifecycle.owned.clone())
            .unwrap_or_default();
        let confirming = Instant::now();
        let child_teardown = record.as_ref().map_or(0.0, |record| {
            (exited - record.finished_at).num_milliseconds().max(0) as f64 / 1e3
        });
        let left = record
            .as_ref()
            .and_then(|record| record.lifecycle.as_ref())
            .map_or(teardown, |lifecycle| {
                let used = lifecycle.teardown_seconds + child_teardown;
                Duration::from_secs_f64((allowances.teardown_seconds as f64 - used).max(0.0))
            });
        let surviving = confirm(&project, &owned, &storage, left).await;
        cleanup.seconds = child_teardown + confirming.elapsed().as_secs_f64();
        cleanup.confirmed = surviving.is_empty();
        cleanup.surviving = surviving;
        if let Some(lifecycle) = record.as_mut().and_then(|record| record.lifecycle.as_mut()) {
            lifecycle.teardown_seconds += cleanup.seconds;
            for overrun in lifecycle.overruns() {
                if !record_has(&reasons, &overrun.message) {
                    reasons.push(overrun.message);
                }
            }
        } else if record.is_some() {
            reasons.push("the engine record has no case lifecycle".to_owned());
        }
        if !cleanup.confirmed {
            reasons.push(format!(
                "cleanup was not confirmed within the {}s teardown allowance: {}",
                allowances.teardown_seconds,
                cleanup.surviving.join("; ")
            ));
        }
        self.finished(case, started_at, record, cleanup, reasons)
    }

    /// The case record of `case` from what its attempt observed.
    fn finished(
        &self,
        case: Case,
        started_at: chrono::DateTime<Utc>,
        record: Option<Record>,
        cleanup: Cleanup,
        mut reasons: Vec<String>,
    ) -> CaseRecord {
        if let Some(record) = &record {
            let mut engine: Vec<String> = record
                .reasons
                .iter()
                .map(|reason| reason.message.clone())
                .collect();
            engine.append(&mut reasons);
            reasons = engine;
        }
        let mut unique = Vec::new();
        for reason in reasons {
            if !unique.contains(&reason) {
                unique.push(reason);
            }
        }
        CaseRecord {
            protocol: CASE_PROTOCOL.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            category: self.category.name().to_owned(),
            family: case.family.to_owned(),
            case: case.id.to_owned(),
            selector: case.selector(),
            mode: self.mode,
            profile: self.profile,
            started_at,
            record,
            verdict: if unique.is_empty() {
                Verdict::Valid
            } else {
                Verdict::Invalid
            },
            reasons: unique,
            cleanup,
        }
    }
}

/// Whether `reasons` already holds `message`.
fn record_has(reasons: &[String], message: &str) -> bool {
    reasons.iter().any(|reason| reason == message)
}

/// A synced temporary file in `directory` holding `bytes`.
///
/// # Errors
///
/// Returns why the directory, write or sync failed.
fn keep(directory: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile, String> {
    let fail = |error: std::io::Error| format!("{}: {error}", directory.display());
    std::fs::create_dir_all(directory).map_err(fail)?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(fail)?;
    file.write_all(bytes).map_err(fail)?;
    file.write_all(b"\n").map_err(fail)?;
    file.as_file().sync_all().map_err(fail)?;
    Ok(file)
}

/// Stops the case process group `group` of `child`: TERM, so the wrapper
/// tears its PostgreSQL down, then KILL once `grace` passes. Returns its
/// exit status, or `None` when it could not be reaped.
async fn stop(
    group: Option<u32>,
    child: &mut tokio::process::Child,
    grace: Duration,
) -> Option<std::process::ExitStatus> {
    let signal = |name: &str| {
        if let Some(group) = group {
            let _ = std::process::Command::new("kill")
                .args(["-s", name, "--", &format!("-{group}")])
                .status();
        }
    };
    signal("TERM");
    if let Ok(status) = tokio::time::timeout(grace, child.wait()).await {
        return status.ok();
    }
    signal("KILL");
    tokio::time::timeout(grace, child.wait())
        .await
        .ok()
        .and_then(Result::ok)
}

/// The containers and volumes Compose project `project` still has.
///
/// # Errors
///
/// Returns why Docker could not be asked.
async fn compose_resources(project: &str) -> Result<Vec<String>, String> {
    let label = format!("label=com.docker.compose.project={project}");
    let mut found = Vec::new();
    for (what, args) in [
        (
            "container",
            vec!["ps", "--all", "--quiet", "--filter", label.as_str()],
        ),
        (
            "volume",
            vec!["volume", "ls", "--quiet", "--filter", label.as_str()],
        ),
    ] {
        let output = tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|error| format!("docker could not run: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "docker {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        found.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| format!("{what} {} of project {project}", line.trim())),
        );
    }
    Ok(found)
}

/// What still exists of a finished case within `left`: owned processes
/// still running with their recorded start, Compose containers and volumes
/// of `project` (removed once if the wrapper left them), and the storage
/// root, which is removed. Empty when cleanup is confirmed.
async fn confirm(project: &str, owned: &[Owned], storage: &Path, left: Duration) -> Vec<String> {
    let deadline = Instant::now() + left;
    let mut removed = false;
    loop {
        let mut surviving: Vec<String> = owned
            .iter()
            .filter(|process| process.alive())
            .map(|process| format!("{} process {} still runs", process.role.name(), process.pid))
            .collect();
        match compose_resources(project).await {
            Ok(found) if !found.is_empty() && !removed => {
                removed = true;
                let _ = tokio::process::Command::new("docker")
                    .args([
                        "compose",
                        "--project-name",
                        project,
                        "down",
                        "--volumes",
                        "--remove-orphans",
                    ])
                    .output()
                    .await;
                surviving.extend(found);
            }
            Ok(found) => surviving.extend(found),
            Err(error) => {
                surviving.push(format!("PostgreSQL removal could not be checked: {error}"))
            }
        }
        if storage.exists() {
            if let Err(error) = std::fs::remove_dir_all(storage) {
                surviving.push(format!(
                    "storage root {} could not be removed: {error}",
                    storage.display()
                ));
            } else if storage.exists() {
                surviving.push(format!("storage root {} still exists", storage.display()));
            }
        }
        if surviving.is_empty() || Instant::now() >= deadline {
            return surviving;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Narrows the engine's `declaration` to exactly case step `id`: only that
/// measured step and the criteria that judge it, full coverage of it, on
/// the case clock, or the harness smoke clock when `smoke`.
///
/// # Errors
///
/// Returns a message when the engine declares no measured step `id`.
pub fn narrow(declaration: &mut Declaration, id: &str, smoke: bool) -> Result<(), String> {
    declaration
        .steps
        .retain(|step| step.id == id && step.kind == StepKind::Measured);
    if declaration.steps.is_empty() {
        return Err(format!(
            "the {} `{}` declaration has no measured step `{id}`",
            declaration.engine.name(),
            declaration.experiment
        ));
    }
    declaration
        .criteria
        .retain(|criterion| criterion.steps.iter().any(|step| step == id));
    for criterion in &mut declaration.criteria {
        criterion.steps = vec![id.to_owned()];
    }
    declaration.selection = None;
    declaration.timing = if smoke {
        Level::case_smoke_timing()
    } else {
        Level::case_timing()
    };
    declaration.mode = if smoke { Mode::Smoke } else { Mode::Full };
    Ok(())
}

/// Switches `capacity case` accepts.
const CASE_SWITCHES: [&str; 2] = ["--smoke", "--allow-dirty"];

/// Valued flags `capacity case` accepts.
const CASE_VALUED: [&str; 4] = ["--record", "--artifacts", "--started", "--pace"];

/// `capacity case <category> <family/id> --record <path> --artifacts <dir>
/// --started <unix-ms> [--profile <mode>] [--smoke] [--allow-dirty] [--pace
/// <rate>]`: runs exactly one case's engine plan on the case clock, keeps
/// its engine record at `--record`, and exits with its verdict. Only the
/// supervisor starts it, under the PostgreSQL wrapper.
pub async fn case(arguments: &[String], root: &Path) -> u8 {
    match Box::pin(run_case(arguments, root)).await {
        Ok(code) => code,
        Err(message) => {
            eprintln!("capacity case: {message}");
            2
        }
    }
}

/// [`case`]'s work.
///
/// # Errors
///
/// Returns a message for a malformed command line, an undeclared case, or
/// a record that cannot be kept.
async fn run_case(arguments: &[String], root: &Path) -> Result<u8, String> {
    let category = arguments
        .get(2)
        .and_then(|name| Category::parse(name))
        .ok_or("expected a category")?;
    let case = category.case(arguments.get(3).ok_or("expected a case selector")?)?;
    let rest: Vec<String> = arguments
        .iter()
        .take(1)
        .chain(arguments.iter().skip(4))
        .cloned()
        .collect();
    levels::known(&rest, &CASE_VALUED, &CASE_SWITCHES)?;
    let record_path =
        PathBuf::from(flags::value(&rest, "--record")?.ok_or("--record is required")?);
    let artifacts =
        PathBuf::from(flags::value(&rest, "--artifacts")?.ok_or("--artifacts is required")?);
    let started = flags::value(&rest, "--started")?
        .and_then(|millis| millis.parse::<i64>().ok())
        .and_then(chrono::DateTime::<Utc>::from_timestamp_millis)
        .ok_or("--started takes the case start in Unix milliseconds")?;
    let smoke = rest.iter().any(|argument| argument == "--smoke");
    let profile = levels::profile(&rest)?;
    let mut forwarded: Vec<String> = Vec::new();
    for switch in CASE_SWITCHES {
        if rest.iter().any(|argument| argument == switch) {
            forwarded.push(switch.to_owned());
        }
    }
    if let Some(pace) = flags::value(&rest, "--pace")? {
        forwarded.extend(["--pace".to_owned(), pace.to_owned()]);
    }
    crate::start_benchmark()?;
    let plan_arguments = case.plan_arguments(&forwarded);
    let context = Context {
        level: Level::One,
        arguments: &plan_arguments,
        root,
    };
    let mut plan = crate::plan(category.engine(), &context)?;
    levels::select(&mut plan.declaration, &plan_arguments)?;
    narrow(&mut plan.declaration, case.id, smoke)?;
    let interrupt = CancellationToken::new();
    let interrupted = interrupt.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupted.cancel();
        }
    });
    let options = Options {
        artifacts,
        profile: profile.map(|mode| Profiling {
            mode,
            program: PathBuf::from(SAMPLY),
            symbols: root.join(SYMBOLS),
        }),
        interrupt,
        setup_spent: (Utc::now() - started).to_std().unwrap_or_default(),
    };
    let record = run(
        plan.declaration.seal(),
        plan.workload.as_mut(),
        HostLoad::sample,
        &options,
    )
    .await;
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    if let Some(parent) = record_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&record_path, bytes)
        .map_err(|error| format!("{}: {error}", record_path.display()))?;
    eprintln!(
        "l1 {} {}: {}",
        category.name(),
        case.selector(),
        record.verdict.label()
    );
    Ok(record.exit_code())
}

/// `capacity l1 <category> [--list-cases | --cases a,b] [--profile mode]
/// [--allow-dirty]`: lists the category's cases, or runs its sweep, prints
/// the summary table and exits 0 when every case is VALID, else 2. A usage
/// error exits 2 before any build artifact is read or case launched.
pub async fn command(category: Category, arguments: &[String], root: &Path) -> u8 {
    let selection = match crate::cases::resolve(category, arguments) {
        Ok(crate::cases::Request::List) => {
            print!("{}", category.list());
            return 0;
        }
        Ok(crate::cases::Request::Sweep(selection)) => selection,
        Err(message) => {
            eprintln!("capacity l1 {}: {message}", category.name());
            return 2;
        }
    };
    if let Err(error) = wyrd_testing::capacity::release_binary() {
        eprintln!(
            "capacity l1 {}: {error}; prepare the build first through `mise run benchmark:l1:{}`",
            category.name(),
            category.name()
        );
        return 2;
    }
    let mut invocation = vec!["l1".to_owned(), category.name().to_owned()];
    invocation.extend(arguments.iter().skip(1).cloned());
    let sweep = Sweep::new(selection, &invocation, root, false);
    let interrupt = CancellationToken::new();
    let interrupted = interrupt.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("interrupted: stopping the current case and starting no other");
            interrupted.cancel();
        }
    });
    let record = sweep
        .supervise(&interrupt, |case, n| {
            let sweep = &sweep;
            let interrupt = &interrupt;
            async move { sweep.attempt(case, n, interrupt).await }
        })
        .await;
    print_summary(&sweep, &record)
}

/// Prints the sweep's summary table, rendered from the saved files as
/// `capacity report` renders them, and returns the sweep's exit code.
pub fn print_summary(sweep: &Sweep, record: &SweepRecord) -> u8 {
    let path = sweep.directory.join(SWEEP_FILE);
    match summary::report(&path) {
        Ok(Some(table)) => println!("{table}"),
        Ok(None) | Err(_) => {
            println!(
                "{}",
                summary::render_sweep(record, &path.display().to_string(), &|name| {
                    summary::read_case(&sweep.directory.join(name))
                })
            );
        }
    }
    record.exit_code()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use tokio_util::sync::CancellationToken;
    use wyrd_testing::experiment::Verdict;

    use super::Sweep;
    use crate::cases::{Case, Category, Request, resolve};
    use crate::summary::{self, CASE_PROTOCOL, CaseRecord, CaseStatus, Cleanup, PROTOCOL_VERSION};

    /// A sweep of `rest` for `category` under a temporary root.
    fn sweep(category: Category, rest: &[&str], root: &std::path::Path) -> Sweep {
        let line: Vec<String> = std::iter::once("capacity")
            .chain(rest.iter().copied())
            .map(str::to_owned)
            .collect();
        let Ok(Request::Sweep(selection)) = resolve(category, &line) else {
            panic!("a sweep");
        };
        Sweep::new(selection, &line, root, true)
    }

    /// A case record of `case` with `verdict`, `reasons` and whether its
    /// cleanup was confirmed, and no engine record.
    fn attempted(case: &Case, verdict: Verdict, reasons: &[&str], confirmed: bool) -> CaseRecord {
        CaseRecord {
            protocol: CASE_PROTOCOL.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            category: case.category.name().to_owned(),
            family: case.family.to_owned(),
            case: case.id.to_owned(),
            selector: case.selector(),
            mode: summary::SweepMode::Smoke,
            profile: None,
            started_at: Utc::now(),
            record: None,
            cleanup: Cleanup {
                project: "p".to_owned(),
                confirmed,
                ..Cleanup::default()
            },
            verdict,
            reasons: reasons.iter().map(|reason| (*reason).to_owned()).collect(),
        }
    }

    /// An invalid case whose cleanup is confirmed lets the next case start;
    /// one whose cleanup is not confirmed stops the sweep and records every
    /// later case NOT RUN with the reason, without launching it.
    #[tokio::test]
    async fn unconfirmed_cleanup_stops_later_launches() {
        let root = tempfile::tempdir().expect("root");
        let sweep = sweep(
            Category::Bifrost,
            &[
                "--cases",
                "otlp-logs-c1,otlp-logs-c64,native-write-c1,native-write-c64",
            ],
            root.path(),
        );
        let launched = Arc::new(Mutex::new(Vec::new()));
        let record = sweep
            .supervise(&CancellationToken::new(), |case, n| {
                let launched = Arc::clone(&launched);
                async move {
                    launched.lock().expect("launched").push(case.id);
                    match n {
                        1 => attempted(&case, Verdict::Invalid, &["setup failed"], true),
                        2 => attempted(&case, Verdict::Invalid, &["teardown hung"], false),
                        _ => attempted(&case, Verdict::Valid, &[], true),
                    }
                }
            })
            .await;
        assert_eq!(
            *launched.lock().expect("launched"),
            ["otlp-logs-c1", "otlp-logs-c64"]
        );
        let statuses: Vec<CaseStatus> = record.cases.iter().map(|entry| entry.status).collect();
        assert_eq!(
            statuses,
            [
                CaseStatus::Invalid,
                CaseStatus::Invalid,
                CaseStatus::NotRun,
                CaseStatus::NotRun
            ]
        );
        assert_eq!(record.cases[0].reasons, ["setup failed"]);
        assert!(
            record.cases[2].reasons[0]
                .contains("cleanup of `otlp-logs-c64` could not be confirmed")
        );
        assert!(record.cases[2].record.is_none());
        assert_eq!(
            record.cases[1].record.as_deref(),
            Some("02-writes-parquet-otlp-logs-c64.json")
        );
        assert_eq!(record.exit_code(), 2);
        let saved =
            std::fs::read_to_string(sweep.directory.join(summary::SWEEP_FILE)).expect("saved");
        assert!(
            matches!(summary::parse(&saved), Ok(Some(summary::Saved::Sweep(saved))) if saved == record)
        );
        let table = summary::report(&sweep.directory.join(summary::SWEEP_FILE))
            .expect("reads")
            .expect("a protocol record");
        assert!(table.contains("| native-write-c1 | NOT RUN: not started: cleanup of `otlp-logs-c64` could not be confirmed within its teardown allowance | — | — | — | — | — | unavailable |"), "{table}");
        assert!(table.contains("| otlp-logs-c1 | INVALID: setup failed | — | — | — | — | — | 01-writes-parquet-otlp-logs-c1.json |"), "{table}");
    }

    /// An interrupt before a case starts records it and every later case
    /// NOT RUN.
    #[tokio::test]
    async fn an_interrupt_starts_no_further_case() {
        let root = tempfile::tempdir().expect("root");
        let sweep = sweep(Category::Gateway, &[], root.path());
        let interrupt = CancellationToken::new();
        interrupt.cancel();
        let record = sweep
            .supervise(&interrupt, |case, _| async move {
                attempted(&case, Verdict::Valid, &[], true)
            })
            .await;
        assert!(
            record
                .cases
                .iter()
                .all(|entry| entry.status == CaseStatus::NotRun)
        );
        assert_eq!(record.cases.len(), 5);
    }

    /// A record that cannot be kept is INVALID with no reference, and one
    /// kept after its persist allowance is INVALID with its reference.
    #[test]
    fn persistence_failures_invalidate_the_row() {
        let case = Category::Gateway.routine()[0];
        let valid = attempted(&case, Verdict::Valid, &[], true);
        let unsaved = summary::entry(&case, &valid, Err("disk full".to_owned()), 20);
        assert_eq!(unsaved.status, CaseStatus::Invalid);
        assert_eq!(unsaved.record, None);
        assert_eq!(
            unsaved.reasons,
            ["the case record could not be kept: disk full"]
        );
        let late = summary::entry(
            &case,
            &valid,
            Ok(("01-overhead-x.json".to_owned(), 25.5)),
            20,
        );
        assert_eq!(late.status, CaseStatus::Invalid);
        assert_eq!(late.record.as_deref(), Some("01-overhead-x.json"));
        assert_eq!(
            late.reasons,
            ["persisting the case record took 25.5s, over its 20s allowance"]
        );
        let kept = summary::entry(
            &case,
            &valid,
            Ok(("01-overhead-x.json".to_owned(), 0.1)),
            20,
        );
        assert_eq!(kept.status, CaseStatus::Valid);
        let root = tempfile::tempdir().expect("root");
        let sweep = sweep(Category::Gateway, &[], root.path());
        std::fs::create_dir_all(&sweep.directory).expect("directory");
        sweep.keep_case("01.json", &valid).expect("kept");
        assert!(
            sweep.keep_case("01.json", &valid).is_err(),
            "never overwritten"
        );
    }
}
