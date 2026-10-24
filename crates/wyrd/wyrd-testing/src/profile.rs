//! Optional diagnostic CPU profiles of the declared server processes,
//! captured by external `samply` for a whole experiment run.
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
