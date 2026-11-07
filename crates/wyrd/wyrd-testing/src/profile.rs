//! Optional diagnostic CPU and heap profiles of the server under test.
//!
//! CPU profiles are captured by external `samply` for a whole experiment
//! run. Heap profiles come from the server itself: a `bench` build samples
//! its live allocations once [`HeapProfiler::start`] asks it to, and each
//! measured step ends with one pprof capture, `heap-<step>.pb.gz`, of the
//! memory the steps so far allocated and still hold.
//!
//! `--profile` asks for one capture per server PID, started once the
//! servers are ready and interrupted before they stop, with the same
//! invocation on every supported OS: `samply record --save-only --output
//! <dir>/profile-<pid>.json.gz --rate <hertz> --pid <pid>`. Samply samples
//! through perf events on Linux and natively on macOS; the harness neither
//! runs `perf` nor changes OS permissions or limits to obtain a profile.
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

/// The profiler a `--profile` run invokes.
pub const SAMPLY: &str = "samply";

/// Sampling frequency of a capture, hertz: high, because a single-client
/// step spends only a few milliseconds per request.
pub const PROFILE_HERTZ: u32 = 999;

/// How long an interrupted profiler may take to save its capture before it
/// is killed and the capture is noted incomplete.
const SAVE_GRACE: Duration = Duration::from_secs(60);

/// What a run's optional profiling produced, as a record keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileNote {
    /// Whether `--profile` asked for captures.
    pub requested: bool,
    /// Whether every requested capture was saved.
    pub complete: bool,
    /// Saved captures, as paths relative to the working directory.
    pub captures: Vec<String>,
    /// Why profiling was disabled or a capture is missing, one entry per
    /// cause.
    pub notes: Vec<String>,
}

/// The running captures of one experiment.
pub struct Profiler {
    /// The profiler program, [`SAMPLY`] outside tests.
    program: PathBuf,
    /// One running capture per server PID: the PID, the profiler process,
    /// and the capture it writes.
    captures: Vec<(u32, Child, PathBuf)>,
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

    /// Starts `program` on each of `pids`, writing under `directory`.
    ///
    /// Never fails: a directory that cannot be created or a profiler that
    /// cannot spawn becomes a note, and the run continues unprofiled.
    #[must_use]
    pub fn start(program: &Path, pids: &[u32], directory: &Path) -> Self {
        let mut profiler = Self {
            program: program.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
            requested: true,
        };
        if pids.is_empty() {
            profiler
                .notes
                .push("no server process to profile".to_owned());
            return profiler;
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            profiler.notes.push(format!(
                "profiling disabled: cannot create {}: {error}",
                directory.display()
            ));
            return profiler;
        }
        for pid in pids {
            let output = directory.join(format!("profile-{pid}.json.gz"));
            let spawned = std::fs::File::create(directory.join(format!("samply-{pid}.log")))
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
                Ok(child) => profiler.captures.push((*pid, child, output)),
                Err(error) => profiler.notes.push(format!(
                    "profiling disabled for pid {pid}: {} did not start: {error}",
                    program.display()
                )),
            }
        }
        profiler
    }

    /// Interrupts every capture at once, then waits until one shared
    /// [`SAVE_GRACE`] deadline for them to save, killing any still running,
    /// and says what was captured. Blocks the calling thread while captures
    /// save; the runner calls it beside the workload's drain, never before.
    #[must_use]
    pub fn finish(mut self) -> ProfileNote {
        let mut saving = Vec::new();
        for (pid, mut child, output) in std::mem::take(&mut self.captures) {
            if let Ok(Some(status)) = child.try_wait() {
                self.notes.push(format!(
                    "capture of pid {pid} ended early with {status}; see samply-{pid}.log"
                ));
                continue;
            }
            let _ = Command::new("kill")
                .args(["-s", "INT", &child.id().to_string()])
                .status();
            saving.push((pid, child, output));
        }
        let deadline = Instant::now() + SAVE_GRACE;
        let mut captures = Vec::new();
        for (pid, mut child, output) in saving {
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
                    "capture of pid {pid} did not save within {}s",
                    SAVE_GRACE.as_secs()
                ));
                continue;
            }
            if output.is_file() {
                captures.push(output.display().to_string());
            } else {
                self.notes
                    .push(format!("capture of pid {pid} left no {}", output.display()));
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
        }
    }
}

impl Drop for Profiler {
    /// Kills captures a failed run never finished.
    fn drop(&mut self) {
        for (_, child, _) in &mut self.captures {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The heap profiles of one experiment, read from the server under test.
pub struct HeapProfiler {
    /// The server's heap profile endpoint, or `None` when it serves none
    /// or sampling did not start.
    url: Option<String>,
    /// Where captures are written.
    directory: PathBuf,
    /// Saved captures.
    captures: Vec<String>,
    /// Causes noted so far.
    notes: Vec<String>,
}

impl HeapProfiler {
    /// Asks the server at `url` to start sampling, writing captures under
    /// `directory`. `None` captures nothing and notes nothing.
    ///
    /// Never fails: a server that does not start sampling becomes a note,
    /// and the run continues without heap profiles.
    pub async fn start(url: Option<String>, directory: &Path) -> Self {
        let mut profiler = Self {
            url: None,
            directory: directory.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
        };
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
                "heap profiling disabled: {url} did not start: {error}"
            )),
        }
        profiler
    }

    /// Saves the server's current heap profile as `heap-<step>.pb.gz`, or
    /// notes why it could not.
    pub async fn capture(&mut self, step: &str) {
        let Some(url) = &self.url else {
            return;
        };
        let profile = match reqwest::get(url)
            .await
            .and_then(reqwest::Response::error_for_status)
        {
            Ok(response) => response.bytes().await,
            Err(error) => Err(error),
        };
        let output = self.directory.join(format!("heap-{step}.pb.gz"));
        let saved = match profile {
            Ok(profile) => std::fs::create_dir_all(&self.directory)
                .and_then(|()| std::fs::write(&output, profile))
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        match saved {
            Ok(()) => self.captures.push(output.display().to_string()),
            Err(error) => self
                .notes
                .push(format!("heap capture after {step} failed: {error}")),
        }
    }

    /// Adds the captures and notes to the CPU profiles' `note`; any heap
    /// note leaves it incomplete.
    pub fn finish(self, note: &mut ProfileNote) {
        note.complete &= self.notes.is_empty();
        note.captures.extend(self.captures);
        note.notes.extend(self.notes);
    }
}

#[cfg(test)]
mod tests {
    use super::{HeapProfiler, ProfileNote};

    /// A stand-in heap endpoint that starts on `POST` and serves `profile`
    /// on `GET`; returns its URL.
    async fn heap_endpoint() -> String {
        let router = axum::Router::new().route(
            "/benchmark/heap",
            axum::routing::get(|| async { "profile" })
                .post(|| async { axum::http::StatusCode::NO_CONTENT }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, router).await });
        format!("http://{address}/benchmark/heap")
    }

    /// A served profile is saved per step and listed as a capture; a server
    /// that does not start sampling leaves a note and no capture; a workload
    /// without a heap endpoint adds neither.
    ///
    /// # Panics
    ///
    /// Panics when a capture or note is wrong.
    #[tokio::test]
    async fn heap_captures_and_failures_become_profile_evidence() {
        let directory = tempfile::tempdir().expect("directory");
        let complete = || ProfileNote {
            requested: true,
            complete: true,
            ..ProfileNote::default()
        };

        let mut served = HeapProfiler::start(Some(heap_endpoint().await), directory.path()).await;
        served.capture("chat.c1").await;
        let mut note = complete();
        served.finish(&mut note);
        let saved = directory.path().join("heap-chat.c1.pb.gz");
        assert_eq!(note.captures, [saved.display().to_string()]);
        assert!(note.complete && note.notes.is_empty(), "{note:?}");
        assert_eq!(std::fs::read(saved).expect("saved"), b"profile");

        let mut refused = HeapProfiler::start(
            Some("http://127.0.0.1:9/benchmark/heap".to_owned()),
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

        let mut absent = HeapProfiler::start(None, directory.path()).await;
        absent.capture("chat.c1").await;
        let mut note = complete();
        absent.finish(&mut note);
        assert_eq!(note, complete());
    }
}
