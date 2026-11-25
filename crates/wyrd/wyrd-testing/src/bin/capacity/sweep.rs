//! The L1 sweep supervisor and the case process it starts.
//!
//! A sweep runs its resolved cases one at a time. Each case is a fresh
//! process of this binary (`capacity case ...`) under the repository
//! PostgreSQL wrapper, with its own named Compose project and its own
//! temporary root for server storage and WAL, so no server, database,
//! storage, client, SDK or provider state outlives it. The case process runs
//! the engine plan of exactly that case through the shared runner on the
//! case clock ([`Level::case_timing`]), announces its teardown before it
//! stops any process, and keeps its engine record. The supervisor owns the
//! case deadline and the one teardown deadline, confirms every owned process,
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
    Allowances, Declaration, HISTORY_DIRECTORY, Level, Lifecycle, Mode, Options, Owned, Profiling,
    Record, StepKind, Verdict, run,
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

/// The largest file history accepts: the pre-commit `check:file-size`
/// limit, 1 MiB.
const HISTORY_FILE_LIMIT: u64 = 1 << 20;

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
    /// The Docker client that lists and removes each case's Compose
    /// resources.
    pub docker: PathBuf,
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
            docker: PathBuf::from("docker"),
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
                    persist_seconds: None,
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

    /// The commit of a sweep that may enter history, or why it may not:
    /// a standard routine sweep (no `--cases`, `--profile` or `--pace`)
    /// whose every case is VALID and was measured from the same clean
    /// commit.
    ///
    /// # Errors
    ///
    /// Returns why the sweep stays under `target/` only.
    pub fn historical(&self, record: &SweepRecord) -> Result<String, String> {
        if self.mode != SweepMode::Standard || self.explicit {
            return Err(format!(
                "a {} sweep{} is not routine",
                self.mode.label(),
                if self.explicit {
                    " chosen by --cases"
                } else {
                    ""
                }
            ));
        }
        if self.forwarded.iter().any(|flag| flag == "--pace") {
            return Err("a --pace sweep is diagnostic".to_owned());
        }
        if record.exit_code() != 0 {
            return Err("not every case is VALID".to_owned());
        }
        let mut commits = std::collections::BTreeSet::new();
        for entry in &record.cases {
            let name = entry
                .record
                .as_deref()
                .ok_or_else(|| format!("{} kept no case record", entry.selector))?;
            let bytes = std::fs::read(self.directory.join(name))
                .map_err(|error| format!("{name}: {error}"))?;
            let case: CaseRecord =
                serde_json::from_slice(&bytes).map_err(|error| format!("{name}: {error}"))?;
            let engine = case
                .record
                .ok_or_else(|| format!("{name} holds no engine record"))?;
            if engine.dirty {
                return Err(format!("{name} was measured from a dirty tree"));
            }
            commits.insert(engine.commit);
        }
        match commits.into_iter().collect::<Vec<_>>().as_slice() {
            [commit] => Ok(commit.chars().take(9).collect()),
            [] => Err("the sweep has no cases".to_owned()),
            several => Err(format!(
                "its cases were measured from several commits: {several:?}"
            )),
        }
    }

    /// Copies the sweep record and its case records, never their artifact
    /// directories, to `l1/<category>/<stamp>-<sha9>/` under
    /// [`HISTORY_DIRECTORY`], returning that directory. Every file is
    /// checked against [`HISTORY_FILE_LIMIT`] before anything is copied.
    /// The destination is claimed by creating it, which fails atomically
    /// when it already exists, so an existing directory is never replaced;
    /// the case records are copied first and `sweep.json` last, so a
    /// directory without it is incomplete, and a failed copy removes the
    /// claim. Committing it stays manual.
    ///
    /// # Errors
    ///
    /// Returns an oversized file, an existing destination, or the
    /// directory or copy failure; history is then left unchanged.
    pub fn publish(&self, record: &SweepRecord, short: &str) -> Result<PathBuf, String> {
        let names: Vec<&str> = record
            .cases
            .iter()
            .filter_map(|entry| entry.record.as_deref())
            .chain(std::iter::once(SWEEP_FILE))
            .collect();
        for name in &names {
            let path = self.directory.join(name);
            let size = std::fs::metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .len();
            if size > HISTORY_FILE_LIMIT {
                return Err(format!(
                    "{name} is {size} bytes, over history's {HISTORY_FILE_LIMIT}-byte file limit"
                ));
            }
        }
        let parent = self
            .root
            .join(HISTORY_DIRECTORY)
            .join("l1")
            .join(self.category.name());
        let destination = parent.join(format!("{}-{short}", self.stamp));
        std::fs::create_dir_all(&parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        std::fs::create_dir(&destination).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{} already exists", destination.display())
            } else {
                format!("{}: {error}", destination.display())
            }
        })?;
        let copied = names.iter().try_for_each(|name| {
            std::fs::copy(self.directory.join(name), destination.join(name))
                .map(drop)
                .map_err(|error| format!("{name}: {error}"))
        });
        if let Err(error) = copied {
            // The directory is this sweep's own claim, so removing it on
            // failure never touches another publisher's records.
            let _ = std::fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(destination)
    }

    /// Runs case `case`, the `n`th, as its own process under the PostgreSQL
    /// wrapper and returns its case record: started only when its Compose
    /// project does not yet exist, stopped when it has not begun teardown
    /// by the 510 s cutoff, then confirmed gone (processes, containers,
    /// volumes and storage root). Teardown has one deadline, the 60 s
    /// allowance from when it began (announced by the case before it stops
    /// a process, else the cutoff, an interrupt or the process ending)
    /// within the 570 s envelope. Process stop, wrapper disposal, any
    /// forced stop and confirmation all consume it; cleanup finished after
    /// it is recorded but never confirmed.
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
        let checked = clock + Duration::from_secs(allowances.setup_seconds);
        match compose_resources(&self.docker, &project, checked).await {
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
        if std::fs::read_dir(&storage).map_or(true, |mut entries| entries.next().is_some()) {
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
        let cutoff = clock
            + Duration::from_secs(
                allowances.total() - allowances.persist_seconds - allowances.teardown_seconds,
            );
        let envelope = clock + Duration::from_secs(allowances.total() - allowances.persist_seconds);
        let allowance = Duration::from_secs(allowances.teardown_seconds);
        let ends = |began: Instant| (began + allowance).min(envelope);
        let second = |at: Instant| at.saturating_duration_since(clock).as_secs_f64();
        let announced =
            |now: Instant| Lifecycle::teardown_began(&artifacts).map(|at| (clock + at).min(now));
        // Bounds cleanup work: the teardown deadline while it lasts, then the
        // envelope, for physical cleanup that can no longer confirm anything.
        let within = |deadline: Instant| {
            let now = Instant::now();
            if now < deadline {
                deadline
            } else {
                envelope.max(now)
            }
        };
        // When teardown began: set once, by the case's announcement, the
        // cutoff, an interrupt or the case process ending, whichever is first.
        let mut began: Option<Instant> = None;
        match command.spawn() {
            Err(error) => reasons.push(format!("the case process could not start: {error}")),
            Ok(mut child) => {
                let group = child.id();
                let status = loop {
                    let wake = began.map_or_else(|| cutoff.min(Instant::now() + POLL), ends);
                    tokio::select! {
                        status = child.wait() => break Some(status),
                        () = tokio::time::sleep_until(wake) => {
                            if let Some(began) = began {
                                reasons.push(format!(
                                    "the case's teardown did not finish within its {}s allowance, from {:.1}s to {:.1}s; it was stopped",
                                    allowances.teardown_seconds,
                                    second(began),
                                    second(ends(began))
                                ));
                                break None;
                            }
                            let now = Instant::now();
                            began = announced(now);
                            if began.is_none() && now >= cutoff {
                                reasons.push(format!(
                                    "the case did not begin teardown within its {}s deadline; it was stopped",
                                    second(cutoff).round()
                                ));
                                began = Some(now);
                                break None;
                            }
                        }
                        () = interrupt.cancelled() => {
                            reasons.push("the sweep was interrupted; the case was stopped".to_owned());
                            let now = Instant::now();
                            began = began.or_else(|| announced(now)).or(Some(now));
                            break None;
                        }
                    }
                };
                let status = if let Some(status) = status {
                    status.ok()
                } else {
                    let deadline = ends(began.unwrap_or_else(Instant::now));
                    stop(group, &mut child, within(deadline)).await
                };
                match status {
                    Some(status) if status.success() || status.code() == Some(2) => {}
                    Some(status) => reasons.push(format!("the case process exited with {status}")),
                    None => reasons.push("the case process could not be reaped".to_owned()),
                }
            }
        }
        let began = began
            .or_else(|| announced(Instant::now()))
            .unwrap_or_else(Instant::now);
        let deadline = ends(began);
        let mut record = match Record::read(&record_path) {
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
        let surviving = confirm(&self.docker, &project, &owned, &storage, within(deadline)).await;
        let done = Instant::now();
        cleanup.seconds = done.saturating_duration_since(began).as_secs_f64();
        cleanup.confirmed = surviving.is_empty() && done < deadline;
        cleanup.surviving = surviving;
        if let Some(lifecycle) = record.as_mut().and_then(|record| record.lifecycle.as_mut()) {
            lifecycle.teardown_seconds = cleanup.seconds;
            for overrun in lifecycle.overruns() {
                if !record_has(&reasons, &overrun.message) {
                    reasons.push(overrun.message);
                }
            }
        } else if record.is_some() {
            reasons.push("the engine record has no case lifecycle".to_owned());
        }
        if !cleanup.confirmed {
            let left = if cleanup.surviving.is_empty() {
                format!("it finished at {:.1}s", second(done))
            } else {
                cleanup.surviving.join("; ")
            };
            reasons.push(format!(
                "cleanup was not confirmed within the {}s teardown allowance ending at {:.1}s: {left}",
                allowances.teardown_seconds,
                second(deadline)
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
            timings: std::collections::BTreeMap::new(),
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

/// Stops the case process group `group` of `child` by `deadline`: TERM,
/// so the wrapper tears its PostgreSQL down, then KILL once half the time
/// left passes. Returns its exit status, or `None` when it could not be reaped
/// by `deadline`.
async fn stop(
    group: Option<u32>,
    child: &mut tokio::process::Child,
    deadline: Instant,
) -> Option<std::process::ExitStatus> {
    let signal = |name: &str| {
        if let Some(group) = group {
            let _ = std::process::Command::new("kill")
                .args(["-s", name, "--", &format!("-{group}")])
                .status();
        }
    };
    signal("TERM");
    let term = Instant::now() + deadline.saturating_duration_since(Instant::now()) / 2;
    if let Ok(status) = tokio::time::timeout_at(term, child.wait()).await {
        return status.ok();
    }
    signal("KILL");
    tokio::time::timeout_at(deadline, child.wait())
        .await
        .ok()
        .and_then(Result::ok)
}

/// The containers, volumes and networks Compose project `project` still
/// has, asked of `docker` by `deadline`.
///
/// # Errors
///
/// Returns why Docker could not be asked, or that it did not answer by
/// `deadline`; a timeout never reads as an empty project.
async fn compose_resources(
    docker: &Path,
    project: &str,
    deadline: Instant,
) -> Result<Vec<String>, String> {
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
        (
            "network",
            vec!["network", "ls", "--quiet", "--filter", label.as_str()],
        ),
    ] {
        let output = tokio::time::timeout_at(
            deadline,
            tokio::process::Command::new(docker)
                .args(&args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| {
            format!(
                "docker {} did not answer within its allowance",
                args.join(" ")
            )
        })?
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

/// What still exists of a finished case, polled until nothing does or
/// `deadline` passes: owned processes still running with their recorded
/// start, Compose containers and volumes of `project` (removed once if the
/// wrapper left them), and the storage root, which is removed. Every Docker
/// call is bounded by `deadline`, and an answer it did not give in time
/// survives. Whether an empty answer came within the teardown allowance is
/// the caller's judgment.
async fn confirm(
    docker: &Path,
    project: &str,
    owned: &[Owned],
    storage: &Path,
    deadline: Instant,
) -> Vec<String> {
    let mut removed = false;
    loop {
        let mut surviving: Vec<String> = owned
            .iter()
            .filter(|process| process.alive())
            .map(|process| format!("{} process {} still runs", process.role.name(), process.pid))
            .collect();
        match compose_resources(docker, project, deadline).await {
            Ok(found) if !found.is_empty() && !removed => {
                removed = true;
                let _ = tokio::time::timeout_at(
                    deadline,
                    tokio::process::Command::new(docker)
                        .args([
                            "compose",
                            "--project-name",
                            project,
                            "down",
                            "--volumes",
                            "--remove-orphans",
                        ])
                        .kill_on_drop(true)
                        .output(),
                )
                .await;
                surviving.extend(found);
            }
            Ok(found) => surviving.extend(found),
            Err(error) => {
                surviving.push(format!("PostgreSQL removal could not be checked: {error}"));
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
        let now = Instant::now();
        if surviving.is_empty() || now >= deadline {
            return surviving;
        }
        tokio::time::sleep(POLL.min(deadline - now)).await;
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
    let code = print_summary(&sweep, &record);
    match sweep.historical(&record) {
        Err(reason) => {
            println!("history: not published: {reason}");
            code
        }
        Ok(short) => match sweep.publish(&record, &short) {
            Ok(directory) => {
                println!(
                    "history: {} (commit it on benchmark-records)",
                    directory.display()
                );
                code
            }
            Err(error) => {
                eprintln!(
                    "INVALID: the sweep could not enter history: {error}; its records stay under {}",
                    sweep.directory.display()
                );
                2
            }
        },
    }
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
    use std::time::Duration;

    use chrono::Utc;
    use tokio_util::sync::CancellationToken;
    use wyrd_testing::experiment::{Lifecycle, Record, Verdict};

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
            timings: std::collections::BTreeMap::new(),
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
            Category::BifrostWrites,
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

    /// The engine record a Bifrost write case process keeps: 240 s of
    /// setup, 30 s of warmup, 90 s measured, `drain` seconds of drain and
    /// capture, `teardown` seconds stopping its processes, and no owned
    /// process still running.
    fn engine_record(drain: f64, teardown: f64) -> Record {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .expect("runtime");
        let case = runtime.block_on(crate::tests::case_record(
            Category::BifrostWrites,
            "native-write-c1",
            None,
            crate::tests::case_script("rows", 30_000.0, 0.0, 300_000),
        ));
        assert_eq!(case.verdict, Verdict::Valid, "{:?}", case.reasons);
        let mut record = case.record.expect("an engine record");
        let lifecycle = record.lifecycle.as_mut().expect("a lifecycle");
        lifecycle.setup_seconds = 240.0;
        lifecycle.warmup_seconds = 30.0;
        lifecycle.measurement_seconds = 90.0;
        lifecycle.drain_seconds = drain;
        lifecycle.teardown_seconds = teardown;
        lifecycle.owned.clear();
        record
    }

    /// Writes an executable `sh` script of `body` at `path`.
    fn script(path: &std::path::Path, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("script");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    }

    /// A smoke sweep of `cases` under `root` whose PostgreSQL wrapper runs
    /// `wrapper` with `$record` and `$artifacts` naming the case's engine
    /// record and artifacts, and whose Docker client runs `docker`. Both
    /// may `gate NAME` to wait until a [`stepped`] test opens that gate,
    /// `passed NAME` to tell it they passed, and the wrapper may
    /// `announce SECOND` to announce the case's teardown as its runner does.
    fn faked(root: &std::path::Path, cases: &str, wrapper: &str, docker: &str) -> Sweep {
        let gates = root.join("gates");
        std::fs::create_dir_all(&gates).expect("gates");
        let helpers = format!(
            "gates='{}'\n\
             gate() {{ until [ -e \"$gates/$1\" ]; do sleep 0.01; done; }}\n\
             passed() {{ touch \"$gates/$1.passed\"; }}\n\
             announce() {{ echo \"$1\" > \"$artifacts/{teardown}.partial\" && mv \"$artifacts/{teardown}.partial\" \"$artifacts/{teardown}\"; }}\n",
            gates.display(),
            teardown = Lifecycle::TEARDOWN_FILE,
        );
        script(
            &root.join(super::POSTGRES),
            &format!(
                "{helpers}while [ $# -gt 0 ]; do case \"$1\" in --record) record=$2;; --artifacts) artifacts=$2;; esac; shift; done\n{wrapper}"
            ),
        );
        let docker_path = root.join("docker");
        script(&docker_path, &format!("{helpers}{docker}"));
        let mut sweep = sweep(Category::BifrostWrites, &["--cases", cases], root);
        sweep.docker = docker_path;
        sweep
    }

    /// Runs `sweep` through its real case attempts, returning the sweep
    /// record and how many cases were launched.
    async fn attempted_sweep(sweep: &Sweep) -> (summary::SweepRecord, usize) {
        let interrupt = CancellationToken::new();
        let launched = Arc::new(Mutex::new(0));
        let record = sweep
            .supervise(&interrupt, |case, n| {
                *launched.lock().expect("launched") += 1;
                let interrupt = &interrupt;
                async move { sweep.attempt(case, n, interrupt).await }
            })
            .await;
        let launched = *launched.lock().expect("launched");
        (record, launched)
    }

    /// Runs `sweep`'s real case attempts under `root` on the paused clock,
    /// held still while its processes and Docker calls run, so only
    /// `steps` move it: each moves the clock to its case second in poll
    /// advances, then, unless its gate is empty, opens that gate and waits
    /// until the fake wrapper or Docker client passed it. Returns the sweep
    /// record, how many cases launched and the case second the sweep
    /// returned at.
    async fn stepped(
        sweep: &Sweep,
        root: &std::path::Path,
        steps: &[(f64, &str)],
    ) -> (summary::SweepRecord, usize, f64) {
        let started = tokio::time::Instant::now();
        let (release, held) = std::sync::mpsc::channel::<()>();
        // While a blocking task runs, the paused clock never advances itself.
        let hold = tokio::task::spawn_blocking(move || held.recv_timeout(Duration::from_secs(60)));
        let gates = root.join("gates");
        let drive = async {
            for &(second, gate) in steps {
                let at = started + Duration::from_secs_f64(second);
                while tokio::time::Instant::now() < at {
                    tokio::time::advance(super::POLL.min(at - tokio::time::Instant::now())).await;
                }
                if gate.is_empty() {
                    continue;
                }
                std::fs::write(gates.join(gate), "").expect("gate");
                let passed = gates.join(format!("{gate}.passed"));
                tokio::task::spawn_blocking(move || {
                    let waiting = std::time::Instant::now();
                    while !passed.exists() {
                        assert!(
                            waiting.elapsed() < Duration::from_secs(10),
                            "nothing passed {}",
                            passed.display()
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                })
                .await
                .expect("the gate wait");
            }
        };
        let ((record, launched, returned), ()) = tokio::join!(
            async {
                let (record, launched) = attempted_sweep(sweep).await;
                let returned = started.elapsed().as_secs_f64();
                drop(release);
                (record, launched, returned)
            },
            drive
        );
        let _ = hold.await;
        (record, launched, returned)
    }

    /// Runs `future` to completion on a fresh paused-clock runtime.
    fn paused<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .expect("runtime")
            .block_on(future)
    }

    /// The saved case record of the first case of a native-write sweep.
    fn first_case(sweep: &Sweep) -> CaseRecord {
        summary::read_case(
            &sweep
                .directory
                .join("01-writes-parquet-native-write-c1.json"),
        )
        .expect("case record")
    }

    /// A case that announces teardown at 500 s, after 240 s of setup, 30 s
    /// of warmup, 90 s measured and 140 s of drain, is in allowed teardown
    /// past the 510 s cutoff although it has no record yet: its process stop
    /// keeps the record only at 520 s and its PostgreSQL wrapper is gone at
    /// 530 s, inside the 560 s deadline set at 500 s. The case completes at
    /// 530 s VALID, with cleanup confirmed and 30 s of teardown charged.
    #[test]
    fn an_announced_teardown_runs_past_the_cutoff_before_its_record() {
        let root = tempfile::tempdir().expect("root");
        let fixture = root.path().join("fixture.json");
        let record = engine_record(140.0, 20.0);
        std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
        let sweep = faked(
            root.path(),
            "native-write-c1",
            &format!(
                "gate teardown; announce 500; passed teardown\n\
                 gate record; cp '{}' \"$record\"; passed record\n\
                 gate exit; passed exit",
                fixture.display()
            ),
            // The ownership probe's last call passes `probe`, so the clock
            // moves past its 240 s allowance only once Docker has answered.
            "[ \"$1\" = network ] && passed probe; exit 0",
        );
        let (record, launched, returned) = paused(stepped(
            &sweep,
            root.path(),
            &[
                (0.0, "probe"),
                (500.0, "teardown"),
                (520.0, "record"),
                (530.0, "exit"),
            ],
        ));
        assert_eq!(launched, 1);
        assert_eq!(
            record.cases[0].status,
            CaseStatus::Valid,
            "{:?}",
            record.cases[0].reasons
        );
        assert!((returned - 530.0).abs() < 0.01, "{returned}");
        let case = first_case(&sweep);
        assert!(case.cleanup.confirmed);
        assert!(
            (case.cleanup.seconds - 30.0).abs() < 0.01,
            "{}",
            case.cleanup.seconds
        );
        let teardown = case
            .record
            .as_ref()
            .and_then(|record| record.lifecycle.as_ref())
            .expect("lifecycle")
            .teardown_seconds;
        assert!((teardown - 30.0).abs() < 0.01, "{teardown}");
    }

    /// A process stop that outlives its teardown is an overrun, never a
    /// reason to extend it: teardown announced at 500 s ends at 560 s, so
    /// the case still stopping then is stopped at 560 s, INVALID naming the
    /// deadline, its cleanup unconfirmed, and the next case is NOT RUN.
    #[test]
    fn a_stop_past_its_teardown_deadline_is_stopped_at_it() {
        let root = tempfile::tempdir().expect("root");
        let sweep = faked(
            root.path(),
            "native-write-c1,native-write-c64",
            "trap 'exit 0' TERM\n\
             gate teardown; announce 500; passed teardown\n\
             gate never",
            "exit 0",
        );
        let (record, launched, returned) = paused(stepped(
            &sweep,
            root.path(),
            &[(500.0, "teardown"), (560.0, "")],
        ));
        assert_eq!(launched, 1);
        assert!((returned - 560.0).abs() < 0.01, "{returned}");
        assert_eq!(
            record
                .cases
                .iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>(),
            [CaseStatus::Invalid, CaseStatus::NotRun]
        );
        let case = first_case(&sweep);
        assert!(!case.cleanup.confirmed);
        assert!(
            (case.cleanup.seconds - 60.0).abs() < 0.01,
            "{}",
            case.cleanup.seconds
        );
        for expected in [
            "teardown did not finish within its 60s allowance, from 500.0s to 560.0s",
            "kept no engine record",
            "cleanup was not confirmed within the 60s teardown allowance ending at 560.0s",
        ] {
            assert!(
                case.reasons.iter().any(|reason| reason.contains(expected)),
                "{expected}: {:?}",
                case.reasons
            );
        }
    }

    /// Teardown that expires cannot be confirmed later: announced at 490 s
    /// with the record kept at 500 s, the PostgreSQL wrapper still runs at
    /// the 550 s deadline, is stopped then and is gone at 555 s with nothing
    /// left. That later absence is kept as evidence but confirms nothing:
    /// the case is INVALID with cleanup unconfirmed at 550 s, and the next
    /// case is NOT RUN without being launched.
    #[test]
    fn an_expired_teardown_cannot_confirm_later_cleanup() {
        let root = tempfile::tempdir().expect("root");
        let fixture = root.path().join("fixture.json");
        let record = engine_record(130.0, 10.0);
        std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
        let sweep = faked(
            root.path(),
            "native-write-c1,native-write-c64",
            &format!(
                "trap 'gate exit; passed exit; exit 0' TERM\n\
                 gate teardown; announce 490; passed teardown\n\
                 gate record; cp '{}' \"$record\"; passed record\n\
                 gate never",
                fixture.display()
            ),
            "exit 0",
        );
        let (record, launched, returned) = paused(stepped(
            &sweep,
            root.path(),
            &[(490.0, "teardown"), (500.0, "record"), (555.0, "exit")],
        ));
        assert_eq!(launched, 1);
        assert!((returned - 555.0).abs() < 0.01, "{returned}");
        assert_eq!(
            record
                .cases
                .iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>(),
            [CaseStatus::Invalid, CaseStatus::NotRun]
        );
        assert!(
            record.cases[1].reasons[0].contains("could not be confirmed"),
            "{:?}",
            record.cases[1].reasons
        );
        let case = first_case(&sweep);
        assert!(!case.cleanup.confirmed);
        assert!(
            case.cleanup.surviving.is_empty(),
            "{:?}",
            case.cleanup.surviving
        );
        assert!(
            (case.cleanup.seconds - 65.0).abs() < 0.01,
            "{}",
            case.cleanup.seconds
        );
        for expected in [
            "teardown did not finish within its 60s allowance, from 490.0s to 550.0s",
            "cleanup was not confirmed within the 60s teardown allowance ending at 550.0s: it finished at 555.0s",
            "teardown took 65.0s, over its 60s allowance",
        ] {
            assert!(
                case.reasons.iter().any(|reason| reason.contains(expected)),
                "{expected}: {:?}",
                case.reasons
            );
        }
    }

    /// A case INVALID for a drain overrun whose cleanup is confirmed inside
    /// its teardown allowance still lets the next case start: both cases
    /// launch, both are INVALID, and both cleanups are confirmed.
    #[test]
    fn timely_cleanup_after_an_invalid_case_starts_the_next() {
        let root = tempfile::tempdir().expect("root");
        let fixture = root.path().join("fixture.json");
        let record = engine_record(160.0, 10.0);
        std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
        let sweep = faked(
            root.path(),
            "native-write-c1,native-write-c64",
            &format!("announce 0; cp '{}' \"$record\"", fixture.display()),
            "exit 0",
        );
        let (record, launched, _) = paused(stepped(&sweep, root.path(), &[]));
        assert_eq!(launched, 2);
        assert_eq!(
            record
                .cases
                .iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>(),
            [CaseStatus::Invalid, CaseStatus::Invalid]
        );
        let case = first_case(&sweep);
        assert!(case.cleanup.confirmed, "{:?}", case.reasons);
        assert!(
            case.reasons
                .iter()
                .any(|reason| reason.contains("drain and capture took 160.0s")),
            "{:?}",
            case.reasons
        );
    }

    /// A case's kept window snapshots (timing, scan, counter and gauge) go
    /// from its process's artifacts into the saved case record, so after the
    /// temporary artifacts are removed `capacity report` still lists every
    /// window series with its original labels and the measured persistence
    /// time, and regenerates the same sweep table.
    #[test]
    fn saved_cases_keep_window_snapshots_and_persistence_time() {
        use wyrd_testing::experiment::{Role, TimingEvidence};
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        wyrd_benchmark::record_timing(
            &tracing::Span::none(),
            "bifrost.ingest",
            "success",
            Duration::from_micros(700),
            None,
        );
        wyrd_benchmark::record_scan(&tracing::Span::none(), "success", Some(42), Some(4_096));
        metrics::counter!("l1_saved_counter_total", "lane" => "kept").increment(3);
        metrics::gauge!("l1_saved_gauge", "lane" => "kept").set(7.0);
        let after = wyrd_benchmark::snapshot();
        let evidence = vec![TimingEvidence {
            window: "measured".to_owned(),
            role: Role::Server,
            before,
            after,
            reconciliation: Vec::new(),
        }];
        let root = tempfile::tempdir().expect("root");
        let artifacts = root.path().join("artifacts");
        let mut record = engine_record(130.0, 10.0);
        record.artifacts.directory = Some(artifacts.display().to_string());
        let step = record.steps[0].id.clone();
        let kept = artifacts
            .join("timings")
            .join(&step)
            .join("repetition-1.json");
        std::fs::create_dir_all(kept.parent().expect("parent")).expect("timings");
        std::fs::write(&kept, serde_json::to_vec(&evidence).expect("json")).expect("kept");
        let fixture = root.path().join("fixture.json");
        std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
        let sweep = faked(
            root.path(),
            "native-write-c1",
            &format!("cp '{}' \"$record\"", fixture.display()),
            "exit 0",
        );
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (swept, _) = runtime.block_on(attempted_sweep(&sweep));
        assert_eq!(
            swept.cases[0].status,
            CaseStatus::Valid,
            "{:?}",
            swept.cases[0].reasons
        );
        let persisted = swept.cases[0]
            .persist_seconds
            .expect("measured persistence");
        let sweep_file = sweep.directory.join(summary::SWEEP_FILE);
        let table = summary::report(&sweep_file)
            .expect("reads")
            .expect("a sweep");
        let case_file = sweep
            .directory
            .join("01-writes-parquet-native-write-c1.json");
        let local = summary::report(&case_file).expect("reads").expect("a case");
        assert!(
            local.contains("    - timing bifrost.ingest success case"),
            "the local report loads the diagnostics beside it: {local}"
        );
        std::fs::remove_dir_all(&artifacts).expect("artifacts removed");
        assert_eq!(
            summary::report(&sweep_file)
                .expect("reads")
                .expect("a sweep"),
            table,
            "the same sweep table"
        );
        let full = summary::report(&case_file).expect("reads").expect("a case");
        assert!(
            full.contains(&format!("| persist and report | {persisted:.1} | 20 |")),
            "{full}"
        );
        assert!(
            !full.contains("timing bifrost.ingest"),
            "no diagnostics left: {full}"
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&case_file).expect("case file")).expect("json");
        assert!(
            saved.get("timings").is_none(),
            "the case record carries no snapshots"
        );

        // An older case record embedding its snapshots still renders them.
        let mut older = saved;
        older["timings"] = serde_json::json!({ step.clone(): [evidence.clone()] });
        std::fs::write(&case_file, serde_json::to_vec(&older).expect("json")).expect("older");
        let full = summary::report(&case_file).expect("reads").expect("a case");
        for line in [
            "  - repetition 1 server measured window (nearest-rank bucket bounds):",
            "    - timing bifrost.ingest success case",
            "    - scan success case",
            "l1_saved_counter_total{lane=kept}",
            "l1_saved_gauge{lane=kept}",
        ] {
            assert!(full.contains(line), "{line} in {full}");
        }
        let read = summary::read_case(&case_file).expect("case record");
        assert_eq!(
            read.record.as_ref().expect("record").steps[0].repetitions[0].timings,
            evidence
        );
    }

    /// A clean, standard, routine, all-VALID sweep publishes its sweep and
    /// case records, without artifact directories or diagnostics, under
    /// `benchmarks/history/l1/<category>/<stamp>-<sha9>/`, once; a sweep
    /// chosen by `--cases`, profiled, paced, with an INVALID case or
    /// measured dirty never does; and a file over the 1 MiB limit leaves
    /// history unchanged.
    #[test]
    fn eligible_sweeps_publish_compact_records_to_history() {
        let root = tempfile::tempdir().expect("root");
        let mut record = engine_record(130.0, 10.0);
        record.dirty = false;
        record.commit = "0123456789abcdef".to_owned();
        let fixture = root.path().join("fixture.json");
        std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
        let mut sweep = faked(
            root.path(),
            "native-write-c1",
            &format!("cp '{}' \"$record\"", fixture.display()),
            "exit 0",
        );
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (swept, _) = runtime.block_on(attempted_sweep(&sweep));
        assert_eq!(
            swept.cases[0].status,
            CaseStatus::Valid,
            "{:?}",
            swept.cases[0].reasons
        );
        assert!(
            sweep.historical(&swept).is_err(),
            "a smoke sweep chosen by --cases stays local"
        );

        sweep.mode = summary::SweepMode::Standard;
        assert!(
            sweep
                .historical(&swept)
                .expect_err("explicit")
                .contains("--cases")
        );
        sweep.explicit = false;
        let short = sweep.historical(&swept).expect("eligible");
        assert_eq!(short, "012345678");
        let mut profiled = sweep.clone();
        profiled.mode = summary::SweepMode::Diagnostic;
        assert!(profiled.historical(&swept).is_err(), "profiled");
        let mut paced = sweep.clone();
        paced.forwarded.push("--pace".to_owned());
        assert!(paced.historical(&swept).is_err(), "paced");
        let mut invalid = swept.clone();
        invalid.cases[0].status = CaseStatus::Invalid;
        assert!(sweep.historical(&invalid).is_err(), "INVALID");

        let case_file = sweep
            .directory
            .join("01-writes-parquet-native-write-c1.json");
        let saved = std::fs::read(&case_file).expect("case file");
        let mut dirty: serde_json::Value = serde_json::from_slice(&saved).expect("json");
        dirty["record"]["dirty"] = serde_json::Value::Bool(true);
        std::fs::write(&case_file, serde_json::to_vec(&dirty).expect("json")).expect("dirty");
        assert!(
            sweep
                .historical(&swept)
                .expect_err("dirty")
                .contains("dirty")
        );
        std::fs::write(&case_file, &saved).expect("clean again");

        let history = root.path().join("benchmarks/history/l1/bifrost-writes");
        std::fs::File::create(&case_file)
            .and_then(|file| file.set_len(super::HISTORY_FILE_LIMIT + 1))
            .expect("oversized");
        assert!(
            sweep
                .publish(&swept, &short)
                .expect_err("oversized")
                .contains("file limit")
        );
        assert!(!history.exists(), "nothing published");
        std::fs::write(&case_file, &saved).expect("restored");

        let claimed = history.join(format!("{}-012345678", sweep.stamp));
        std::fs::create_dir_all(&claimed).expect("another publisher's empty directory");
        assert!(
            sweep
                .publish(&swept, &short)
                .expect_err("claimed")
                .contains("already exists")
        );
        assert_eq!(
            std::fs::read_dir(&claimed).expect("kept").count(),
            0,
            "never replaced"
        );
        std::fs::remove_dir(&claimed).expect("released");
        let published = sweep.publish(&swept, &short).expect("published");
        assert_eq!(
            published,
            history.join(format!("{}-012345678", sweep.stamp))
        );
        let mut names: Vec<String> = std::fs::read_dir(&published)
            .expect("published directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "01-writes-parquet-native-write-c1.json",
                summary::SWEEP_FILE
            ]
        );
        assert_eq!(
            summary::report(&published.join(summary::SWEEP_FILE))
                .expect("reads")
                .expect("a sweep")
                .replace(&published.display().to_string(), ""),
            summary::report(&sweep.directory.join(summary::SWEEP_FILE))
                .expect("reads")
                .expect("a sweep")
                .replace(&sweep.directory.display().to_string(), ""),
            "history renders the same table"
        );
        assert!(
            sweep
                .publish(&swept, &short)
                .expect_err("once")
                .contains("already exists")
        );
    }

    /// A case process that never keeps a record and ignores TERM is
    /// stopped at the 510 s cutoff, which leaves teardown and persistence
    /// their allowances, and its stop, Docker removal and confirmation share
    /// one 60 s teardown allowance however long Docker hangs: the case is INVALID with its
    /// cleanup unconfirmed, the sweep is kept, and the next case is NOT RUN
    /// without being launched.
    #[tokio::test(start_paused = true)]
    async fn a_hung_case_is_stopped_at_its_cutoff_within_one_cleanup_allowance() {
        let root = tempfile::tempdir().expect("root");
        let sweep = faked(
            root.path(),
            "native-write-c1,native-write-c64",
            "trap '' TERM\nexec sleep 600",
            "exec sleep 600",
        );
        let started = tokio::time::Instant::now();
        let (record, launched) =
            tokio::time::timeout(Duration::from_secs(3_600), attempted_sweep(&sweep))
                .await
                .expect("the supervisor returns");
        let elapsed = started.elapsed().as_secs_f64();
        assert_eq!(launched, 1);
        assert!((510.0..=571.0).contains(&elapsed), "{elapsed}");
        let case = summary::read_case(
            &sweep
                .directory
                .join("01-writes-parquet-native-write-c1.json"),
        )
        .expect("case record");
        assert_eq!(case.verdict, Verdict::Invalid);
        assert!(!case.cleanup.confirmed);
        assert!(case.cleanup.seconds <= 60.5, "{}", case.cleanup.seconds);
        for expected in [
            "did not begin teardown within its 510s deadline",
            "kept no engine record",
            "cleanup was not confirmed",
        ] {
            assert!(
                case.reasons.iter().any(|reason| reason.contains(expected)),
                "{expected}: {:?}",
                case.reasons
            );
        }
        assert_eq!(
            record
                .cases
                .iter()
                .map(|entry| entry.status)
                .collect::<Vec<_>>(),
            [CaseStatus::Invalid, CaseStatus::NotRun]
        );
        assert!(sweep.directory.join(summary::SWEEP_FILE).exists());
    }

    /// Cleanup confirmation is bounded by what remains of the teardown
    /// allowance: with the wrapper gone at 559.5 s of a teardown that ends
    /// at 560 s, a Docker check that answers only after 2 s, or never
    /// answers, leaves cleanup unconfirmed, so the next case is NOT RUN and
    /// never launched.
    #[test]
    fn late_or_missing_cleanup_answers_stop_the_sweep() {
        for delay in ["2", "600"] {
            let root = tempfile::tempdir().expect("root");
            let fixture = root.path().join("fixture.json");
            let record = engine_record(140.0, 5.0);
            std::fs::write(&fixture, serde_json::to_vec(&record).expect("json")).expect("fixture");
            let sweep = faked(
                root.path(),
                "native-write-c1,native-write-c64",
                &format!(
                    "gate teardown; announce 500; cp '{}' \"$record\"; passed teardown\n\
                     gate exit; passed exit",
                    fixture.display()
                ),
                &format!("[ -e \"$gates/exit.passed\" ] && passed asked && sleep {delay}\nexit 0"),
            );
            let (record, launched, _) = paused(stepped(
                &sweep,
                root.path(),
                &[
                    (500.0, "teardown"),
                    (559.5, "exit"),
                    (559.5, "asked"),
                    (561.5, ""),
                ],
            ));
            assert_eq!(launched, 1, "delay {delay}");
            assert_eq!(
                record
                    .cases
                    .iter()
                    .map(|entry| entry.status)
                    .collect::<Vec<_>>(),
                [CaseStatus::Invalid, CaseStatus::NotRun],
                "delay {delay}"
            );
            let case = first_case(&sweep);
            assert!(!case.cleanup.confirmed, "delay {delay}");
            assert!(
                case.cleanup
                    .surviving
                    .iter()
                    .any(|item| item.contains("did not answer within its allowance")),
                "delay {delay}: {:?}",
                case.cleanup.surviving
            );
        }
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
        assert_eq!(unsaved.persist_seconds, None);
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
        assert_eq!(late.persist_seconds, Some(25.5));
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
