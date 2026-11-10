//! Optional diagnostic CPU and heap profiles of the server under test and
//! of the driver measuring it.
//!
//! `--profile [cpu|heap|both]` selects the collectors; a bare `--profile`
//! is `both` ([`ProfileMode`]). Each runs for both process roles, the
//! server and this driver, and keeps their captures apart by role.
//!
//! CPU profiles are captured by external `samply`, one capture per process
//! per measured step, started before the step's warmup and interrupted
//! after its last repetition, with the same invocation on every supported
//! OS: `samply record --save-only --output
//! <dir>/<step>/cpu-<role>-<pid>.json.gz --rate <hertz> --pid <pid>`. A
//! capture spans the step's warmup, repetitions, and the gaps between them;
//! nothing aligns its samples to one repetition. Samply samples through
//! perf events on Linux and natively on macOS; the harness neither runs
//! `perf` nor changes OS permissions or limits to obtain a profile.
//!
//! Heap profiles come from each process's own sampler,
//! [`wyrd_benchmark::heap`], which a `bench` server and this driver report
//! their allocations to: [`HeapProfiler::start`] starts both, and each
//! measured step ends with one pprof capture per role,
//! `heap-<role>-<step>.pb.gz`, of the memory the steps so far allocated and
//! of what they still hold.
//!
//! A profile is diagnostic evidence only. A missing profiler, a capture that
//! fails to start, exits early, or leaves no file yields a [`ProfileNote`]
//! naming why; it never changes a measurement, a criterion, or a verdict, and
//! never supplies the CPU denominator, which comes from the OS's process
//! readings. Captures stay under `target/`; a record keeps only their
//! relative paths.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::experiment::Role;

/// The profiler a `--profile` run invokes.
pub const SAMPLY: &str = "samply";

/// Sampling frequency of a capture, hertz: high, because a single-client
/// step spends only a few milliseconds per request.
pub const PROFILE_HERTZ: u32 = 999;

/// How long an interrupted profiler may take to save its capture before it
/// is killed and the capture is noted incomplete.
const SAVE_GRACE: Duration = Duration::from_secs(60);

/// The collectors a `--profile` run starts, for both process roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    /// CPU sampling only; no heap sampler starts.
    Cpu,
    /// Heap sampling only; no CPU profiler process starts.
    Heap,
    /// Both collectors, which a bare `--profile` selects.
    Both,
}

impl ProfileMode {
    /// Every mode, its value on the command line.
    pub const ALL: [Self; 3] = [Self::Cpu, Self::Heap, Self::Both];

    /// The mode's command-line value and record label.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Heap => "heap",
            Self::Both => "both",
        }
    }

    /// The mode `value` names, or `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == value)
    }

    /// Whether CPU captures run.
    #[must_use]
    pub const fn cpu(self) -> bool {
        matches!(self, Self::Cpu | Self::Both)
    }

    /// Whether heap captures run.
    #[must_use]
    pub const fn heap(self) -> bool {
        matches!(self, Self::Heap | Self::Both)
    }
}

/// What a run's optional profiling produced, as a record keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileNote {
    /// Whether `--profile` asked for captures.
    pub requested: bool,
    /// The collectors it asked for; `None` in an unprofiled run or a record
    /// that predates modes, whose captures were both kinds of the server.
    #[serde(default)]
    pub mode: Option<ProfileMode>,
    /// The process roles profiled; empty in a record that predates roles,
    /// which profiled the server alone.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Whether every requested capture was saved.
    pub complete: bool,
    /// Saved captures, as paths relative to the working directory.
    pub captures: Vec<String>,
    /// Why profiling was disabled or a capture is missing, one entry per
    /// cause.
    pub notes: Vec<String>,
}

impl ProfileNote {
    /// Adds `other`'s captures and notes; any note leaves this incomplete.
    pub fn absorb(&mut self, other: Self) {
        self.complete &= other.notes.is_empty();
        self.captures.extend(other.captures);
        self.notes.extend(other.notes);
    }
}

/// The running CPU captures of one measured step.
pub struct Profiler {
    /// The profiler program, [`SAMPLY`] outside tests.
    program: PathBuf,
    /// One running capture per process: its role and PID, the profiler
    /// process, and the capture it writes.
    captures: Vec<(Role, u32, Child, PathBuf)>,
    /// Causes noted so far.
    notes: Vec<String>,
    /// Whether profiling was requested.
    requested: bool,
}

impl Profiler {
    /// A profiler that was not requested: finishing it notes nothing.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            program: PathBuf::from(SAMPLY),
            captures: Vec::new(),
            notes: Vec::new(),
            requested: false,
        }
    }

    /// Starts `program` on each of `targets`, a role and PID each, writing
    /// under `directory`. A PID listed twice is captured once, under its
    /// first role.
    ///
    /// Never fails: a directory that cannot be created or a profiler that
    /// cannot spawn becomes a note, and the run continues unprofiled.
    #[must_use]
    pub fn start(program: &Path, targets: &[(Role, u32)], directory: &Path) -> Self {
        let mut profiler = Self {
            program: program.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
            requested: true,
        };
        if !targets.iter().any(|(role, _)| *role == Role::Server) {
            profiler
                .notes
                .push("no server process to profile".to_owned());
        }
        if targets.is_empty() {
            return profiler;
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            profiler.notes.push(format!(
                "profiling disabled: cannot create {}: {error}",
                directory.display()
            ));
            return profiler;
        }
        for (index, (role, pid)) in targets.iter().enumerate() {
            if targets[..index].iter().any(|(_, seen)| seen == pid) {
                continue;
            }
            let name = format!("{}-{pid}", role.name());
            let output = directory.join(format!("cpu-{name}.json.gz"));
            let spawned = std::fs::File::create(directory.join(format!("samply-{name}.log")))
                .and_then(|log| {
                    Command::new(program)
                        .args(["record", "--save-only", "--output"])
                        .arg(&output)
                        .args(["--rate", &PROFILE_HERTZ.to_string()])
                        .args(["--pid", &pid.to_string()])
                        .stdin(Stdio::null())
                        .stdout(log.try_clone()?)
                        .stderr(log)
                        .spawn()
                });
            match spawned {
                Ok(child) => profiler.captures.push((*role, *pid, child, output)),
                Err(error) => profiler.notes.push(format!(
                    "profiling disabled for {name}: {} did not start: {error}",
                    program.display()
                )),
            }
        }
        profiler
    }

    /// Interrupts every capture at once, then waits until one shared
    /// [`SAVE_GRACE`] deadline for them to save, killing any still running,
    /// and says what was captured. Blocks the calling thread while captures
    /// save; the runner calls it off the async runtime.
    #[must_use]
    pub fn finish(mut self) -> ProfileNote {
        let mut saving = Vec::new();
        for (role, pid, mut child, output) in std::mem::take(&mut self.captures) {
            let name = format!("{}-{pid}", role.name());
            if let Ok(Some(status)) = child.try_wait() {
                self.notes.push(format!(
                    "capture of {name} ended early with {status}; see samply-{name}.log"
                ));
                continue;
            }
            let _ = Command::new("kill")
                .args(["-s", "INT", &child.id().to_string()])
                .status();
            saving.push((name, child, output));
        }
        let deadline = Instant::now() + SAVE_GRACE;
        let mut captures = Vec::new();
        for (name, mut child, output) in saving {
            let mut exited = false;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if !exited {
                let _ = child.kill();
                let _ = child.wait();
                self.notes.push(format!(
                    "capture of {name} did not save within {}s",
                    SAVE_GRACE.as_secs()
                ));
                continue;
            }
            if output.is_file() {
                captures.push(output.display().to_string());
            } else {
                self.notes
                    .push(format!("capture of {name} left no {}", output.display()));
            }
        }
        if self.requested && captures.is_empty() && self.notes.is_empty() {
            self.notes
                .push(format!("{} produced no capture", self.program.display()));
        }
        let notes = std::mem::take(&mut self.notes);
        ProfileNote {
            requested: self.requested,
            complete: self.requested && notes.is_empty(),
            captures,
            notes,
            ..ProfileNote::default()
        }
    }
}

impl Drop for Profiler {
    /// Kills captures a failed run never finished.
    fn drop(&mut self) {
        for (_, _, child, _) in &mut self.captures {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The heap profiles of one experiment, read from the server under test
/// and from this driver.
pub struct HeapProfiler {
    /// The server's heap profile endpoint, or `None` when it serves none
    /// or sampling did not start.
    url: Option<String>,
    /// Whether this driver's sampler runs.
    driver: bool,
    /// Where captures are written.
    directory: PathBuf,
    /// Saved captures.
    captures: Vec<String>,
    /// Causes noted so far.
    notes: Vec<String>,
}

impl HeapProfiler {
    /// Asks the server at `url` to start sampling and, when `driver`,
    /// starts this process's sampler, writing captures under `directory`.
    /// Neither captures nothing and notes nothing.
    ///
    /// Never fails: a server that does not start sampling becomes a note,
    /// and the run continues without its heap profiles.
    pub async fn start(url: Option<String>, driver: bool, directory: &Path) -> Self {
        let mut profiler = Self {
            url: None,
            driver,
            directory: directory.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
        };
        if driver {
            wyrd_benchmark::heap::start();
        }
        let Some(url) = url else {
            return profiler;
        };
        let started = reqwest::Client::new()
            .post(&url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status);
        match started {
            Ok(_) => profiler.url = Some(url),
            Err(error) => profiler.notes.push(format!(
                "heap profiling disabled for the server: {url} did not start: {error}"
            )),
        }
        profiler
    }

    /// Saves each sampling role's current heap profile as
    /// `heap-<role>-<step>.pb.gz`, or notes why it could not.
    pub async fn capture(&mut self, step: &str) {
        if let Some(url) = &self.url {
            let profile = match reqwest::get(url)
                .await
                .and_then(reqwest::Response::error_for_status)
            {
                Ok(response) => response.bytes().await.map(|bytes| bytes.to_vec()),
                Err(error) => Err(error),
            };
            self.save(
                Role::Server,
                step,
                profile.map_err(|error| error.to_string()),
            );
        }
        if self.driver {
            let profile = tokio::task::spawn_blocking(wyrd_benchmark::heap::profile)
                .await
                .map_err(|error| error.to_string());
            self.save(Role::Driver, step, profile);
        }
    }

    /// Writes `role`'s `profile` after `step`, or notes why it is missing.
    fn save(&mut self, role: Role, step: &str, profile: Result<Vec<u8>, String>) {
        let output = self
            .directory
            .join(format!("heap-{}-{step}.pb.gz", role.name()));
        let saved = profile.and_then(|profile| {
            std::fs::create_dir_all(&self.directory)
                .and_then(|()| std::fs::write(&output, profile))
                .map_err(|error| error.to_string())
        });
        match saved {
            Ok(()) => self.captures.push(output.display().to_string()),
            Err(error) => self.notes.push(format!(
                "{} heap capture after {step} failed: {error}",
                role.name()
            )),
        }
    }

    /// Adds the captures and notes to the run's `note`; any heap note
    /// leaves it incomplete.
    pub fn finish(self, note: &mut ProfileNote) {
        note.absorb(ProfileNote {
            captures: self.captures,
            notes: self.notes,
            ..ProfileNote::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{HeapProfiler, ProfileNote, Profiler, Role};

    /// A stand-in heap endpoint that starts on `POST` and serves `profile`
    /// on `GET`; returns its URL.
    async fn heap_endpoint(profile: Vec<u8>) -> String {
        let router = axum::Router::new().route(
            "/benchmark/heap",
            axum::routing::get(|| async move { profile })
                .post(|| async { axum::http::StatusCode::NO_CONTENT }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, router).await });
        format!("http://{address}/benchmark/heap")
    }

    /// A requested, so far complete, note.
    fn complete() -> ProfileNote {
        ProfileNote {
            requested: true,
            complete: true,
            ..ProfileNote::default()
        }
    }

    /// Writes an executable stand-in profiler that saves `profile` to its
    /// `--output` when interrupted, and marks `<output>.ready` once it can
    /// be.
    fn saving_profiler(directory: &Path) -> std::path::PathBuf {
        let script = directory.join("saving-profiler");
        std::fs::write(
            &script,
            "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do [ \"$1\" = --output ] && out=\"$2\"; shift; done\n\
             trap 'echo profile > \"$out\"; exit 0' INT\n: > \"$out.ready\"\nwhile :; do sleep 0.02; done\n",
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod");
        script
    }

    /// Waits until every capture of `outputs` under `step` is ready to be
    /// interrupted.
    fn ready(step: &Path, outputs: &[String]) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for output in outputs {
            let marker = step.join(format!("{output}.ready"));
            while !marker.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{output} never started"
                );
                std::thread::yield_now();
            }
        }
    }

    /// A served profile is saved per step and listed as a capture; a server
    /// that does not start sampling leaves a note and no capture; a workload
    /// without a heap endpoint and an unsampled driver add neither.
    ///
    /// # Panics
    ///
    /// Panics when a capture or note is wrong.
    #[tokio::test]
    async fn heap_captures_and_failures_become_profile_evidence() {
        let directory = tempfile::tempdir().expect("directory");
        let url = heap_endpoint(b"profile".to_vec()).await;
        let mut served = HeapProfiler::start(Some(url), false, directory.path()).await;
        served.capture("chat.c1").await;
        let mut note = complete();
        served.finish(&mut note);
        let saved = directory.path().join("heap-server-chat.c1.pb.gz");
        assert_eq!(note.captures, [saved.display().to_string()]);
        assert!(note.complete && note.notes.is_empty(), "{note:?}");
        assert_eq!(std::fs::read(saved).expect("saved"), b"profile");

        let mut refused = HeapProfiler::start(
            Some("http://127.0.0.1:9/benchmark/heap".to_owned()),
            false,
            directory.path(),
        )
        .await;
        refused.capture("chat.c1").await;
        let mut note = complete();
        refused.finish(&mut note);
        assert!(note.captures.is_empty());
        assert!(!note.complete);
        assert!(
            note.notes[0].contains("heap profiling disabled"),
            "{note:?}"
        );

        let mut absent = HeapProfiler::start(None, false, directory.path()).await;
        absent.capture("chat.c1").await;
        let mut note = complete();
        absent.finish(&mut note);
        assert_eq!(note, complete());
    }

    /// The driver's own heap capture and the server's are both the shared
    /// sampler's gzipped pprof, written per step under their own role.
    ///
    /// # Panics
    ///
    /// Panics when a role's capture is missing or not a sampler profile.
    #[tokio::test]
    async fn driver_and_server_heap_captures_use_the_same_sample_contract() {
        let directory = tempfile::tempdir().expect("directory");
        let url = heap_endpoint(wyrd_benchmark::heap::profile()).await;
        let mut both = HeapProfiler::start(Some(url), true, directory.path()).await;
        both.capture("verify.c1").await;
        let mut note = complete();
        both.finish(&mut note);
        assert!(note.complete, "{note:?}");
        let expected: Vec<String> = ["server", "driver"]
            .map(|role| {
                directory
                    .path()
                    .join(format!("heap-{role}-verify.c1.pb.gz"))
                    .display()
                    .to_string()
            })
            .to_vec();
        assert_eq!(note.captures, expected);
        for capture in &note.captures {
            let bytes = std::fs::read(capture).expect("saved");
            assert_eq!(bytes.get(..2), Some(&[0x1f, 0x8b][..]), "{capture} is gzip");
        }
    }

    /// One CPU capture runs per process, named by its role, so the driver's
    /// capture never passes for the server's; a PID listed under both roles
    /// is captured once; a step without a server notes it.
    ///
    /// # Panics
    ///
    /// Panics when a capture's role, count, or note is wrong.
    #[test]
    fn cpu_targets_keep_driver_and_server_roles_separate() {
        let directory = tempfile::tempdir().expect("directory");
        let program = saving_profiler(directory.path());
        let step = directory.path().join("profiles").join("op-1");
        let driver = std::process::id();
        let profiler = Profiler::start(
            &program,
            &[(Role::Server, 1), (Role::Driver, driver)],
            &step,
        );
        let outputs = [
            "cpu-server-1.json.gz".to_owned(),
            format!("cpu-driver-{driver}.json.gz"),
        ];
        ready(&step, &outputs);
        let note = profiler.finish();
        assert!(note.complete, "{note:?}");
        assert_eq!(
            note.captures,
            [
                step.join("cpu-server-1.json.gz").display().to_string(),
                step.join(format!("cpu-driver-{driver}.json.gz"))
                    .display()
                    .to_string(),
            ]
        );

        let shared = Profiler::start(
            &program,
            &[(Role::Server, driver), (Role::Driver, driver)],
            &step,
        );
        ready(&step, &[format!("cpu-server-{driver}.json.gz")]);
        assert_eq!(shared.finish().captures.len(), 1, "one capture per process");

        let alone = Profiler::start(&program, &[(Role::Driver, driver)], &step);
        let note = alone.finish();
        assert!(!note.complete);
        assert!(
            note.notes.iter().any(|n| n.contains("no server process")),
            "{note:?}"
        );
    }
}
