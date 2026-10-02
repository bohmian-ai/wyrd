//! Diagnostic CPU profiles of the serving replicas, captured with external
//! `perf` for exactly one measured step.
//!
//! A capture attaches to a replica's own `wyrd-server` PID, never the driver
//! or the `systemd-run` wrapper, when the step's traffic starts and is
//! interrupted when it ends, so its recorded window is the step. Each
//! capture's `perf.data`, `report.txt`, and `metadata.json` land in
//! `profiles/<step>/replica-<n>/`. A missing profiler, an exited server, a
//! failed record or report, an empty report, or a report without a resolved
//! Wyrd frame is reported as incomplete evidence, never a hotspot summary.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::Result;

/// Sampling frequency, hertz.
const FREQUENCY: &str = "99";

/// Hot frames kept in a step's summary.
const HOTSPOTS: usize = 15;

/// One replica's capture of one step.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    /// Replica ordinal.
    pub replica: u16,
    /// Serving PID the capture attached to.
    pub pid: u32,
    /// Where the artifacts are.
    pub directory: PathBuf,
    /// When `perf record` started.
    pub started: DateTime<Utc>,
    /// When it was interrupted.
    pub stopped: Option<DateTime<Utc>>,
    /// The `perf record` command line.
    pub command: String,
    /// The hottest self-time frames, as `perf report` prints them.
    pub hotspots: Vec<String>,
    /// Why the capture is not usable evidence, when it is not.
    pub failure: Option<String>,
}

/// A running capture.
pub struct Capture {
    /// The profile this capture fills in.
    profile: Profile,
    /// The `perf record` process.
    child: Child,
}

impl Capture {
    /// Starts `perf record` on `pid` for replica `replica`, writing under
    /// `directory`.
    ///
    /// # Errors
    ///
    /// Returns the directory or spawn failure; a missing `perf` binary fails
    /// here.
    pub fn start(pid: u32, replica: u16, directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let data = directory.join("perf.data");
        let args = [
            "record".to_owned(),
            "-F".to_owned(),
            FREQUENCY.to_owned(),
            "-g".to_owned(),
            "--call-graph".to_owned(),
            "fp".to_owned(),
            "-p".to_owned(),
            pid.to_string(),
            "-o".to_owned(),
            data.display().to_string(),
        ];
        let log = std::fs::File::create(directory.join("record.log"))?;
        let child = Command::new("perf")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        Ok(Self {
            profile: Profile {
                replica,
                pid,
                directory: directory.to_path_buf(),
                started: Utc::now(),
                stopped: None,
                command: format!("perf {}", args.join(" ")),
                hotspots: Vec::new(),
                failure: None,
            },
            child,
        })
    }

    /// Interrupts the capture, waits for `perf` to flush, writes the report
    /// and metadata, and checks the evidence.
    ///
    /// # Errors
    ///
    /// Returns a signal, wait, report, or file failure; an unusable capture
    /// is returned with its failure recorded instead.
    pub fn finish(mut self, metadata: &serde_json::Value) -> Result<Profile> {
        Command::new("kill")
            .args(["-s", "INT", &self.child.id().to_string()])
            .status()?;
        let status = self.child.wait()?;
        self.profile.stopped = Some(Utc::now());
        let directory = self.profile.directory.clone();
        if !status.success() && status.code() != Some(130) {
            self.profile.failure =
                Some(format!("perf record exited with {status}; see record.log"));
        } else {
            let report = Command::new("perf")
                .args([
                    "report",
                    "--stdio",
                    "--no-children",
                    "--percent-limit",
                    "0.5",
                    "-i",
                ])
                .arg(directory.join("perf.data"))
                .output()?;
            let text = String::from_utf8_lossy(&report.stdout).into_owned();
            std::fs::write(directory.join("report.txt"), &text)?;
            self.profile.hotspots = text
                .lines()
                .filter(|line| {
                    line.trim_start()
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit())
                })
                .take(HOTSPOTS)
                .map(|line| line.trim().to_owned())
                .collect();
            self.profile.failure = if !report.status.success() {
                Some(format!("perf report exited with {}", report.status))
            } else if self.profile.hotspots.is_empty() {
                Some("the capture holds no samples".to_owned())
            } else if !text.contains("wyrd") && !text.contains("vala") {
                Some("no Wyrd frame resolved; symbols are missing".to_owned())
            } else {
                None
            };
        }
        let mut meta = metadata.clone();
        if let serde_json::Value::Object(map) = &mut meta {
            map.insert("profile".to_owned(), serde_json::to_value(&self.profile)?);
        }
        std::fs::write(
            directory.join("metadata.json"),
            serde_json::to_vec_pretty(&meta)?,
        )?;
        Ok(self.profile.clone())
    }
}

impl Drop for Capture {
    /// Stops a capture abandoned by an error or cancellation.
    fn drop(&mut self) {
        if self.profile.stopped.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
