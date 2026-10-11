//! Diagnostic CPU profiles of serving `wyrd-server` processes, captured with
//! external `perf` for exactly one measured step of a capacity benchmark.
//!
//! A capture attaches to a replica's own `wyrd-server` PID, never the driver
//! or the `systemd-run` wrapper, when the step's traffic starts and is
//! interrupted when it ends, so its recorded window is the step. Each
//! capture's `perf.data`, `report.txt`, and `metadata.json` land in
//! `profiles/<step>/replica-<n>/`. The report is flat self time by shared
//! object and symbol, read without inline expansion, which turns minutes of
//! DWARF resolution into seconds; the call graphs stay in `perf.data` for
//! `perf report -g`. A missing profiler, an exited server, a
//! failed record or report, an empty report, or a report without a resolved
//! Wyrd frame is reported as incomplete evidence, never a hotspot summary.
//!
//! Beside the CPU samples, a second `perf` records every context switch of
//! the server with its call chain into `offcpu.data`. Each switch-out is
//! charged the time until that thread is switched back in, so the window's
//! off-CPU time folds by the user stack that blocked: a lock wait, an idle
//! runtime park, or a socket read. The folded stacks land in
//! `offcpu.folded`, flame-graph ready. Stack-attributed switches need
//! `kernel.perf_event_paranoid` at 1 or below; above that the off-CPU half
//! reports itself as missing while the CPU half is unaffected.

use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::capacity::Error;

/// Result of a capture operation.
type Result<T> = std::result::Result<T, Error>;

/// Hot frames kept in a step's summary.
const HOTSPOTS: usize = 15;

/// Crates kept in a step's CPU Pareto.
const PARETO: usize = 20;

/// Off-CPU stacks kept in a step's summary.
const OFF_CPU_STACKS: usize = 15;

/// Innermost user frames that name one off-CPU stack.
const OFF_CPU_DEPTH: usize = 10;

/// Characters kept of each frame in a folded stack.
const FRAME_CHARS: usize = 120;

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
    /// Self CPU percent by owning crate, largest first: the CPU Pareto.
    pub crates: Vec<(String, f64)>,
    /// Why the capture is not usable evidence, when it is not.
    pub failure: Option<String>,
    /// Off-CPU seconds by blocking user stack, innermost frame first,
    /// largest first.
    pub off_cpu: Vec<(String, f64)>,
    /// Why the off-CPU capture is not usable evidence, when it is not.
    pub off_cpu_failure: Option<String>,
}

/// A running capture.
pub struct Capture {
    /// The profile this capture fills in.
    profile: Profile,
    /// The `perf record` process.
    child: Child,
    /// The context-switch `perf record` process.
    off_cpu: Child,
}

impl Capture {
    /// Starts `perf record` on `pid` for replica `replica`, sampling at
    /// `frequency` hertz and writing under `directory`.
    ///
    /// A long multi-replica step keeps the sample count bounded with a low
    /// frequency; a short single-client step needs a high one to resolve a
    /// few milliseconds of work per request.
    ///
    /// The off-CPU recorder attaches to the same PID and samples every
    /// context switch with its call chain plus the switch records that time
    /// each one.
    ///
    /// # Errors
    ///
    /// Returns the directory or spawn failure; a missing `perf` binary fails
    /// here.
    pub fn start(pid: u32, replica: u16, frequency: u32, directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let data = directory.join("perf.data");
        let args = [
            "record".to_owned(),
            "-F".to_owned(),
            frequency.to_string(),
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
        let off_cpu_log = std::fs::File::create(directory.join("offcpu-record.log"))?;
        let off_cpu = Command::new("perf")
            .args([
                "record",
                "-e",
                "context-switches",
                "-c",
                "1",
                "--switch-events",
            ])
            .args(["--call-graph", "fp", "-p", &pid.to_string(), "-o"])
            .arg(directory.join("offcpu.data"))
            .stdin(Stdio::null())
            .stdout(off_cpu_log.try_clone()?)
            .stderr(off_cpu_log)
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
                crates: Vec::new(),
                failure: None,
                off_cpu: Vec::new(),
                off_cpu_failure: None,
            },
            child,
            off_cpu,
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
        for child in [&self.child, &self.off_cpu] {
            Command::new("kill")
                .args(["-s", "INT", &child.id().to_string()])
                .status()?;
        }
        let status = self.child.wait()?;
        let off_cpu_status = self.off_cpu.wait()?;
        self.profile.stopped = Some(Utc::now());
        let directory = self.profile.directory.clone();
        // `perf` stops on the interrupt either by exiting 130 or, in newer
        // releases, by dying of SIGINT (2); both mean a flushed capture.
        let interrupted = status.code() == Some(130) || status.signal() == Some(2);
        if !status.success() && !interrupted {
            self.profile.failure =
                Some(format!("perf record exited with {status}; see record.log"));
        } else {
            let report = Command::new("perf")
                .args([
                    "report",
                    "--stdio",
                    "--no-children",
                    "--no-inline",
                    "--sort",
                    "dso,sym",
                    "-g",
                    "none",
                    "-i",
                ])
                .arg(directory.join("perf.data"))
                .output()?;
            let text = String::from_utf8_lossy(&report.stdout).into_owned();
            std::fs::write(directory.join("report.txt"), &text)?;
            let frames: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|line| line.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .collect();
            self.profile.hotspots = frames
                .iter()
                .take(HOTSPOTS)
                .map(|line| (*line).to_owned())
                .collect();
            self.profile.crates = crate_pareto(&frames);
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
        self.read_off_cpu(off_cpu_status)?;
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

impl Capture {
    /// Folds the stopped off-CPU recording into `offcpu.folded` and the
    /// profile's off-CPU summary, or records why it is not usable.
    ///
    /// # Errors
    ///
    /// Returns a `perf script` spawn or file failure.
    fn read_off_cpu(&mut self, status: std::process::ExitStatus) -> Result<()> {
        let directory = &self.profile.directory;
        if !status.success() && status.code() != Some(130) && status.signal() != Some(2) {
            self.profile.off_cpu_failure = Some(format!(
                "perf record exited with {status}; see offcpu-record.log"
            ));
            return Ok(());
        }
        let script = Command::new("perf")
            .args(["script", "--no-inline", "--show-switch-events"])
            .args(["-F", "tid,time,event,ip,sym", "-i"])
            .arg(directory.join("offcpu.data"))
            .output()?;
        let stacks = fold_off_cpu(&String::from_utf8_lossy(&script.stdout));
        let mut folded = String::new();
        for (stack, seconds) in &stacks {
            let root_first: Vec<&str> = stack.split(';').rev().collect();
            folded.push_str(&format!(
                "{} {}\n",
                root_first.join(";"),
                (seconds * 1e6).round()
            ));
        }
        std::fs::write(directory.join("offcpu.folded"), folded)?;
        let mut ranked: Vec<(String, f64)> = stacks.into_iter().collect();
        ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
        ranked.truncate(OFF_CPU_STACKS);
        self.profile.off_cpu_failure = if !script.status.success() {
            Some(format!("perf script exited with {}", script.status))
        } else if ranked.is_empty() {
            Some(
                "no stack-attributed context switch; kernel.perf_event_paranoid must be 1 or below"
                    .to_owned(),
            )
        } else {
            None
        };
        self.profile.off_cpu = ranked;
        Ok(())
    }
}

/// Charges each context switch-out the time until its thread is switched
/// back in, keyed by the innermost [`OFF_CPU_DEPTH`] user frames of the
/// switch's call chain, innermost first and `;`-separated.
///
/// Reads `perf script -F tid,time,event,ip,sym --show-switch-events`: an
/// `context-switches` sample line followed by its tab-indented frames,
/// and `PERF_RECORD_SWITCH` lines. Kernel frames (addresses from
/// `ffff`) and unresolved frames are dropped; a sample with no user frame is
/// keyed `[kernel]`. A switch-out with no later switch-in in the window is
/// not charged.
fn fold_off_cpu(script: &str) -> std::collections::BTreeMap<String, f64> {
    let mut pending: std::collections::HashMap<u32, (f64, Vec<String>)> =
        std::collections::HashMap::new();
    let mut current: Option<u32> = None;
    let mut totals = std::collections::BTreeMap::new();
    for line in script.lines() {
        if line.starts_with('\t') {
            let Some((_, frames)) = current.and_then(|tid| pending.get_mut(&tid)) else {
                continue;
            };
            let Some((address, symbol)) = line.trim().split_once(char::is_whitespace) else {
                continue;
            };
            let symbol = symbol.trim();
            if frames.len() < OFF_CPU_DEPTH
                && !address.starts_with("ffff")
                && !symbol.is_empty()
                && symbol != "[unknown]"
            {
                frames.push(
                    symbol
                        .chars()
                        .take(FRAME_CHARS)
                        .collect::<String>()
                        .replace(';', ","),
                );
            }
            continue;
        }
        current = None;
        let mut fields = line.split_whitespace();
        let (Some(tid), Some(time)) = (
            fields.next().and_then(|tid| tid.parse::<u32>().ok()),
            fields
                .next()
                .and_then(|time| time.trim_end_matches(':').parse::<f64>().ok()),
        ) else {
            continue;
        };
        if line.contains("context-switches") {
            pending.insert(tid, (time, Vec::new()));
            current = Some(tid);
        } else if line.contains("PERF_RECORD_SWITCH IN")
            && let Some((out, frames)) = pending.remove(&tid)
        {
            let stack = if frames.is_empty() {
                "[kernel]".to_owned()
            } else {
                frames.join(";")
            };
            *totals.entry(stack).or_insert(0.0) += (time - out).max(0.0);
        }
    }
    totals
}

/// Sums flat `perf report` self-time lines by the crate that owns each frame.
///
/// A line reads `<percent>%  <dso>  [.|k] <symbol>`. Kernel frames are
/// `kernel`; a Rust symbol belongs to the first path segment of its
/// demangled name, generic and trait-impl brackets stripped; any other
/// symbol, such as a statically linked C allocator or codec, is keyed by its
/// leading name segment.
fn crate_pareto(frames: &[&str]) -> Vec<(String, f64)> {
    let mut totals: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for line in frames {
        let Some((percent, rest)) = line.split_once('%') else {
            continue;
        };
        let Ok(percent) = percent.trim().parse::<f64>() else {
            continue;
        };
        let owner = if rest.contains("[k] ") {
            "kernel".to_owned()
        } else if let Some((_, symbol)) = rest.split_once("[.] ") {
            let symbol = symbol.trim_start_matches(['<', '&', '*', ' ']);
            match symbol.split_once("::") {
                Some((owner, _)) => owner.to_owned(),
                None => format!(
                    "c:{}",
                    symbol.split(['_', '(', ' ']).next().unwrap_or(symbol)
                ),
            }
        } else {
            continue;
        };
        *totals.entry(owner).or_default() += percent;
    }
    let mut pareto: Vec<(String, f64)> = totals.into_iter().collect();
    pareto.sort_by(|left, right| right.1.total_cmp(&left.1));
    pareto.truncate(PARETO);
    pareto
}

impl Drop for Capture {
    /// Stops a capture abandoned by an error or cancellation.
    fn drop(&mut self) {
        if self.profile.stopped.is_none() {
            for child in [&mut self.child, &mut self.off_cpu] {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{crate_pareto, fold_off_cpu};

    /// A switch-out is charged until its thread's switch-in, keyed by its
    /// user frames innermost first; kernel frames are dropped and a switch
    /// with no switch-in is not charged.
    #[test]
    fn fold_off_cpu_charges_switch_out_until_switch_in() {
        let script = "\
    7 100.000000: context-switches:
\t    ffffffff81000000 __schedule
\t        55aa00000001 std::sys::sync::rwlock::futex::RwLock::read_contended
\t        55aa00000002 vala::Memtable::collect_readable_batches

    7 100.000000: PERF_RECORD_SWITCH OUT
    8 100.001000: context-switches:
\t    ffffffff81000000 __schedule

    7 100.003000: PERF_RECORD_SWITCH IN
    8 100.002000: PERF_RECORD_SWITCH IN
    9 100.004000: context-switches:
\t        55aa00000003 tokio::park
";

        let folded = fold_off_cpu(script);

        let lock = folded
            .get("std::sys::sync::rwlock::futex::RwLock::read_contended;vala::Memtable::collect_readable_batches")
            .copied()
            .unwrap_or_default();
        let kernel = folded.get("[kernel]").copied().unwrap_or_default();
        assert!((lock - 0.003).abs() < 1e-9, "lock wait {lock}");
        assert!((kernel - 0.001).abs() < 1e-9, "kernel wait {kernel}");
        assert_eq!(folded.len(), 2, "an unmatched switch-out is not charged");
    }

    /// Rust frames group by crate, kernel and C frames by their own keys,
    /// largest first.
    #[test]
    fn crate_pareto_groups_frames_by_owner() {
        let frames = [
            "30.00%  wyrd-server  [.] datafusion_physical_plan::filter::FilterExec::execute",
            "10.00%  wyrd-server  [.] <parquet::arrow::ArrowReader as Iterator>::next",
            "25.00%  wyrd-server  [.] datafusion_physical_plan::aggregates::row_hash::poll",
            " 5.00%  [kernel.kallsyms]  [k] do_syscall_64",
            " 4.00%  wyrd-server  [.] mi_malloc_aligned",
        ];

        assert_eq!(
            crate_pareto(&frames),
            vec![
                ("datafusion_physical_plan".to_owned(), 55.0),
                ("parquet".to_owned(), 10.0),
                ("kernel".to_owned(), 5.0),
                ("c:mi".to_owned(), 4.0),
            ]
        );
    }
}
