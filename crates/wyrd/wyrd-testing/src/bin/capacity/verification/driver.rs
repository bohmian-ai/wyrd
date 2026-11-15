//! The L1 language drivers: the public SDK each step's clients call
//! through, and the child process that runs a Python or TypeScript step's
//! calls.
//!
//! A Rust step's clients are this process's async tasks. A Python or
//! TypeScript step's clients are one long-lived child per language, started
//! outside every measured window: it loads every application's pulled
//! bundle through its own public SDK, prestarts its bounded workers, and
//! then runs one closed-loop window per command. The protocol is one JSON
//! line each way on the child's stdin and stdout: the configuration (with
//! each application's key, so no key reaches disk) answered by the child's
//! identity, then one `run` per window answered by every call it made. The
//! child reports each call's two language-clock samples, its stable error
//! code, or its execution and compact outcome; this process judges the
//! outcomes after the window.
//!
//! A driver whose native module is a `bench` build installs the shared
//! benchmark recorder and answers each window with its recorder snapshots
//! around the window's calls, so the shared client's and the binding's own
//! `sdk.verify` phases are read from the process that ran them.
//!
//! Under CPU profiling the child starts with its runtime's perf map
//! (Python's `-X perf`, Node's `--perf-basic-prof`), so the external
//! `samply` capture of its process names interpreted and JIT frames beside
//! native ones. Under heap profiling it samples its managed heap with its
//! runtime's own tool (Python's `tracemalloc`, V8's sampling heap profiler
//! with a heap snapshot) and its native module's allocations with the
//! shared [`wyrd_benchmark::heap`] sampler, and answers each `heap` command
//! with the files it saved.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use wyrd_benchmark::BenchmarkSnapshot;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::execution::VerificationOutcome;
use wyrd_testing::experiment::{ExperimentError, Role, Step};
use wyrd_testing::profile::ProfileMode;

use crate::verification::workload::Application;

/// The public SDK a step's clients call through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    /// `wyrd_client` in this process's bounded async tasks.
    Rust,
    /// The `wyrd` package in one child's prestarted threads.
    Python,
    /// The `wyrd-ai` package in one child's bounded Promises.
    TypeScript,
}

impl Language {
    /// Every language, in step order.
    pub const ALL: [Self; 3] = [Self::Rust, Self::Python, Self::TypeScript];

    /// The language `step` declares in its `language` variable: Rust when
    /// it names none.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a language no driver serves.
    pub fn of(step: &Step) -> Result<Self, ExperimentError> {
        match step.text("language") {
            None | Some("rust") => Ok(Self::Rust),
            Some("python") => Ok(Self::Python),
            Some("typescript") => Ok(Self::TypeScript),
            Some(other) => Err(ExperimentError::workload(format!(
                "step `{}` declares language `{other}`, which no driver serves",
                step.id
            ))),
        }
    }

    /// The name steps and records use.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }

    /// The process role a profile files this language's clients under:
    /// Rust clients are this driver's.
    pub const fn role(self) -> Role {
        match self {
            Self::Rust => Role::Driver,
            Self::Python => Role::Python,
            Self::TypeScript => Role::TypeScript,
        }
    }

    /// The command that starts this language's driver from the repository
    /// `root`, or `None` for Rust, whose clients run in this process. When
    /// CPU-profiled it writes its runtime's perf map, and Node, which then
    /// also writes a V8 log, writes it as `v8-typescript.log` in the run's
    /// `cpu_logs` directory rather than the package directory. Python runs
    /// the package's own environment interpreter directly, so starting it
    /// never syncs or rebuilds the installed native module.
    fn command(self, root: &Path, cpu_logs: Option<&Path>) -> Option<Command> {
        let (program, arguments, directory, perf) = match self {
            Self::Rust => return None,
            Self::Python => {
                let directory = root.join("sdks/wyrd-sdk-python");
                (
                    directory.join(".venv/bin/python"),
                    vec!["-m", "tests.benchmark_driver"],
                    directory,
                    ["-X", "perf"].as_slice(),
                )
            }
            Self::TypeScript => (
                "node".into(),
                vec!["tests/profile/verification-driver.mjs"],
                root.join("sdks/wyrd-sdk-ts/wyrd"),
                ["--perf-basic-prof"].as_slice(),
            ),
        };
        let mut command = Command::new(program);
        if let Some(logs) = cpu_logs {
            command.args(perf);
            if self == Self::TypeScript {
                let mut logfile = std::ffi::OsString::from("--logfile=");
                logfile.push(logs.join("v8-typescript.log"));
                command.arg("--no-logfile-per-isolate").arg(logfile);
            }
        }
        command.args(arguments).current_dir(directory);
        Some(command)
    }
}

/// One call a language driver reports: its two samples from immediately
/// before the public `verify`, and its stable error code, or its execution
/// and compact outcome as the SDK returned them.
#[derive(Debug, Clone, Deserialize)]
pub struct Call {
    /// The arrival sequence the driver drew.
    pub sequence: u64,
    /// Nanoseconds until the call returned or raised.
    pub returned_ns: u64,
    /// Nanoseconds until its result was materialized; a refusal's equals
    /// `returned_ns`.
    pub ready_ns: u64,
    /// Whether it answered at or after the window's stop.
    pub late: bool,
    /// The refusal's stable code.
    pub error: Option<String>,
    /// The acknowledged execution.
    pub execution_id: Option<ExecutionId>,
    /// The acknowledged verdict and requirement judgments.
    pub outcome: Option<VerificationOutcome>,
    /// In a paced window, nanoseconds the call started after its due slot.
    #[serde(default)]
    pub lateness_ns: Option<u64>,
    /// In a paced window, nanoseconds its due slot fell after the window
    /// opened.
    #[serde(default)]
    pub due_ns: Option<u64>,
    /// In a paced window, nanoseconds its caller woke after the due slot,
    /// before preparing the call; `None` from a driver that does not
    /// report it.
    #[serde(default)]
    pub woke_ns: Option<u64>,
}

/// The one line a driver answers its configuration with.
#[derive(Deserialize)]
struct Ready {
    /// What the driver runs: language, runtime, SDK version, native module
    /// path, process id, and worker count.
    ready: Value,
}

/// The one line a driver answers a `run` with.
#[derive(Deserialize)]
struct Done {
    /// The window's calls and recorder readout.
    done: Window,
}

/// One window a driver ran.
#[derive(Debug, Deserialize)]
pub struct Window {
    /// Every call the window made, late ones included.
    pub calls: Vec<Call>,
    /// The driver's own benchmark recorder, read immediately before its
    /// first call and after its last returned; `None` from a driver whose
    /// native module is not a `bench` build and so records nothing.
    #[serde(default)]
    pub timings: Option<Timings>,
    /// Every garbage collection the driver's runtime ran during the
    /// window; `None` from a driver that does not report them.
    #[serde(default)]
    pub collections: Option<Vec<Collection>>,
}

/// One garbage collection a driver's runtime ran during a window.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Collection {
    /// Nanoseconds after the window opened that it started.
    pub start_ns: u64,
    /// Nanoseconds it took.
    pub duration_ns: u64,
}

/// A driver process's recorder snapshots around one window.
#[derive(Debug, Deserialize)]
pub struct Timings {
    /// The snapshot before the window's first call.
    pub before: BenchmarkSnapshot,
    /// The snapshot after its last call returned.
    pub after: BenchmarkSnapshot,
}

/// One heap capture a driver saved.
#[derive(Debug, Deserialize)]
pub struct Captured {
    /// The saved files: the managed heap's, then the native module's when
    /// it is a `bench` build.
    pub paths: Vec<String>,
}

/// A running Python or TypeScript driver with every application loaded.
pub struct ForeignDriver {
    /// The child, killed if this handle drops before [`Self::stop`].
    child: Child,
    /// The child's command stream.
    stdin: ChildStdin,
    /// The child's answer lines.
    stdout: Lines<BufReader<ChildStdout>>,
    /// What the child reported it runs.
    pub identity: Value,
    /// The child's process id.
    pub pid: u32,
    /// The applications it loaded, the most clients one window may run.
    pub applications: usize,
}

impl ForeignDriver {
    /// Starts `language`'s driver from the repository root and loads
    /// `applications` against `server_url`, every call naming `verifier`
    /// with the even or odd `arrivals` context, profiled under `profile`
    /// with any runtime log in the run's `logs` directory. Returns once the
    /// driver reports its identity: every state is loaded and every worker
    /// started.
    ///
    /// # Errors
    ///
    /// Returns a workload error for Rust, a spawn failure, or a driver that
    /// exits or answers anything but its identity.
    pub async fn start(
        language: Language,
        server_url: &str,
        verifier: &str,
        arrivals: [Value; 2],
        applications: &[Application],
        profile: Option<ProfileMode>,
        logs: &Path,
    ) -> Result<Self, ExperimentError> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let command = language
            .command(&root, profile.is_some_and(ProfileMode::cpu).then_some(logs))
            .ok_or_else(|| {
                ExperimentError::workload("Rust clients run in this process, not a driver")
            })?;
        let configuration = json!({
            "profile": { "heap": profile.is_some_and(ProfileMode::heap) },
            "server_url": server_url,
            "verifier": verifier,
            "arrivals": arrivals,
            "applications": applications
                .iter()
                .map(|application| json!({
                    "bundle": application.bundle,
                    "key": application.key.expose_secret(),
                }))
                .collect::<Vec<_>>(),
        });
        Self::spawn(command, &configuration, applications.len()).await
    }

    /// Spawns `command` as a driver of `applications` and sends it
    /// `configuration`.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a spawn failure or a driver that exits
    /// or answers anything but its identity.
    pub(super) async fn spawn(
        mut command: Command,
        configuration: &Value,
        applications: usize,
    ) -> Result<Self, ExperimentError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| ExperimentError::workload(format!("starting a driver: {error}")))?;
        let (Some(stdin), Some(stdout), Some(pid)) =
            (child.stdin.take(), child.stdout.take(), child.id())
        else {
            return Err(ExperimentError::workload("the driver has no pipes"));
        };
        let mut driver = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            identity: Value::Null,
            pid,
            applications,
        };
        driver.send(configuration).await?;
        driver.identity = driver.answer::<Ready>().await?.ready;
        Ok(driver)
    }

    /// Runs `clients` callers for `seconds` and returns every call, those
    /// in flight at the stop included, with the driver's own recorder
    /// readout around them: closed loop, or with `rate` paced so the
    /// clients together offer `rate` calls per second, each call waiting
    /// for its due slot and reporting how late it started.
    ///
    /// # Errors
    ///
    /// Returns a workload error for more clients than applications, or a
    /// driver that exits or answers anything but the window's calls.
    pub async fn run(
        &mut self,
        clients: usize,
        seconds: f64,
        rate: Option<u64>,
    ) -> Result<Window, ExperimentError> {
        if !(1..=self.applications).contains(&clients) {
            return Err(ExperimentError::workload(format!(
                "{clients} clients exceed the driver's {} applications",
                self.applications
            )));
        }
        self.send(&json!({ "run": { "clients": clients, "seconds": seconds, "rate": rate } }))
            .await?;
        Ok(self.answer::<Done>().await?.done)
    }

    /// Saves the driver's heap capture as `stem` plus each file's extension.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a driver that exits or answers anything
    /// but that capture, as one not heap-profiled does.
    pub async fn capture(&mut self, stem: &Path) -> Result<Captured, ExperimentError> {
        self.send(&json!({ "heap": { "stem": stem } })).await?;
        let mut answer = self.answer::<BTreeMap<String, Captured>>().await?;
        answer
            .remove("heap")
            .ok_or_else(|| ExperimentError::workload("the driver answered no heap capture"))
    }

    /// Closes the driver's input and waits for it to exit.
    ///
    /// # Errors
    ///
    /// Returns a workload error when it exits unsuccessfully.
    pub async fn stop(mut self) -> Result<(), ExperimentError> {
        drop(self.stdin);
        let status = self.child.wait().await.map_err(ExperimentError::workload)?;
        if status.success() {
            Ok(())
        } else {
            Err(ExperimentError::workload(format!(
                "the driver exited {status}"
            )))
        }
    }

    /// Writes one command line.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the driver's input is closed.
    async fn send(&mut self, command: &Value) -> Result<(), ExperimentError> {
        let mut line = command.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| ExperimentError::workload(format!("writing to the driver: {error}")))
    }

    /// Reads one answer line as `T`.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the driver exits first or answers
    /// something else.
    async fn answer<T: for<'de> Deserialize<'de>>(&mut self) -> Result<T, ExperimentError> {
        let line = self
            .stdout
            .next_line()
            .await
            .map_err(ExperimentError::workload)?
            .ok_or_else(|| ExperimentError::workload("the driver exited before answering"))?;
        serde_json::from_str(&line).map_err(|error| {
            ExperimentError::workload(format!("the driver answered unexpectedly: {error}"))
        })
    }
}

/// Why `calls` cannot be one window's calls: a sequence reported twice or
/// skipped, so a call was duplicated or lost. `None` when the sequences are
/// exactly `0..calls.len()`.
pub fn unsequenced(calls: &[Call]) -> Option<String> {
    let mut seen = vec![false; calls.len()];
    for call in calls {
        match usize::try_from(call.sequence)
            .ok()
            .and_then(|index| seen.get_mut(index))
        {
            Some(slot) if !*slot => *slot = true,
            _ => {
                return Some(format!(
                    "the driver reported sequence {} twice or outside its {} calls",
                    call.sequence,
                    calls.len()
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use tokio::process::Command;
    use wyrd_testing::experiment::Step;

    use super::{ForeignDriver, Language, unsequenced};

    /// A driver that answers its configuration with its identity, one
    /// `run` with a fixed acknowledged call, a refusal, and a late call, and
    /// one `heap` with its managed and native files, echoing nothing it was
    /// sent.
    fn scripted() -> Command {
        let ready = json!({ "ready": { "language": "python", "pid": 1 } });
        let done = json!({ "done": { "calls": [
            { "sequence": 1, "returned_ns": 2_000_000, "ready_ns": 2_500_000, "late": false,
              "error": null, "execution_id": "0198f3c4-6a0e-7c41-9d5e-3f7a2b8c1d00",
              "outcome": { "verdict": "failed", "requirements": [] } },
            { "sequence": 0, "returned_ns": 900_000, "ready_ns": 900_000, "late": false,
              "error": "WYRD_SERVER_503_SERVICE_UNAVAILABLE", "execution_id": null, "outcome": null },
            { "sequence": 2, "returned_ns": 3_000_000, "ready_ns": 3_100_000, "late": true,
              "error": null, "execution_id": "0198f3c4-6a0e-7c41-9d5e-3f7a2b8c1d01",
              "outcome": { "verdict": "passed", "requirements": [] } },
        ] } });
        let heap = json!({ "heap": { "paths": ["heap-python-step.tracemalloc", "heap-python-step.pb.gz"] } });
        let mut command = Command::new("sh");
        command.args([
            "-c",
            &format!(
                "read config; echo '{ready}'; read run; echo '{done}'; read heap; echo '{heap}'; cat >/dev/null"
            ),
        ]);
        command
    }

    /// A CPU-profiled driver writes its runtime's perf map, and Node writes
    /// its V8 log into the run's log directory, never the package directory;
    /// an unprofiled driver gets no profiler flags.
    ///
    /// # Panics
    ///
    /// Panics when a driver's flags are wrong.
    #[test]
    fn profiled_drivers_keep_runtime_logs_in_the_run() {
        let flags = |language: Language, logs: Option<&Path>| {
            let command = language
                .command(Path::new("/repo"), logs)
                .expect("a driver");
            command
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let logs = Path::new("/run/logs");
        assert_eq!(
            flags(Language::TypeScript, Some(logs)),
            [
                "--perf-basic-prof",
                "--no-logfile-per-isolate",
                "--logfile=/run/logs/v8-typescript.log",
                "tests/profile/verification-driver.mjs",
            ]
        );
        assert_eq!(
            flags(Language::Python, Some(logs)),
            ["-X", "perf", "-m", "tests.benchmark_driver"]
        );
        assert_eq!(
            flags(Language::TypeScript, None),
            ["tests/profile/verification-driver.mjs"]
        );
        assert!(
            Language::Rust
                .command(Path::new("/repo"), Some(logs))
                .is_none()
        );
    }

    #[test]
    fn steps_select_their_language() {
        let step = |variables| Step::measured("step", variables);
        assert_eq!(Language::of(&step(json!({}))).ok(), Some(Language::Rust));
        assert_eq!(
            Language::of(&step(json!({ "language": "python" }))).ok(),
            Some(Language::Python)
        );
        assert_eq!(
            Language::of(&step(json!({ "language": "typescript" }))).ok(),
            Some(Language::TypeScript)
        );
        assert!(Language::of(&step(json!({ "language": "go" }))).is_err());
        assert_eq!(Language::TypeScript.role().name(), "typescript");
        assert_eq!(Language::Rust.role().name(), "driver");
    }

    #[tokio::test]
    async fn drivers_report_identity_then_every_call() {
        let mut driver = ForeignDriver::spawn(scripted(), &json!({}), 64)
            .await
            .expect("the driver starts");
        assert_eq!(driver.identity["language"], "python");
        assert!(
            driver.run(65, 1.0, None).await.is_err(),
            "65 clients exceed 64 applications"
        );
        assert!(
            driver.run(0, 1.0, None).await.is_err(),
            "a window needs a client"
        );
        let window = driver.run(64, 1.0, None).await.expect("the window answers");
        assert!(
            window.timings.is_none(),
            "this driver reported no recorder window"
        );
        let calls = window.calls;
        assert_eq!(calls.len(), 3);
        assert_eq!(unsequenced(&calls), None);
        assert_eq!(
            calls[1].error.as_deref(),
            Some("WYRD_SERVER_503_SERVICE_UNAVAILABLE")
        );
        let heap = driver
            .capture(Path::new("heap-python-step"))
            .await
            .expect("the capture answers");
        assert_eq!(
            heap.paths,
            ["heap-python-step.tracemalloc", "heap-python-step.pb.gz"]
        );
        driver.stop().await.expect("the driver exits cleanly");

        let mut repeated = calls.clone();
        repeated[1].sequence = 1;
        assert!(unsequenced(&repeated).is_some(), "a duplicate is caught");
        assert!(
            unsequenced(&calls[1..]).is_some(),
            "a skipped sequence is caught"
        );
    }
}
