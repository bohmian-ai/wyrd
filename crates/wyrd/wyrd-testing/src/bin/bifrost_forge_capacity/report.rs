//! The benchmark's report: host, settings, the scheduling case, every
//! measured window, per-step summaries, and the gating checks.
//!
//! A check fails the run when its measurement misses the threshold or was not
//! measured at all. Thresholds are the Forge capacity scenario's and are never
//! relaxed here.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use wyrd_testing::capacity::Percentiles;

use crate::Result;
use crate::deployment::WindowRecord;
use crate::schedule::{SchedulingCase, SchedulingRun};

/// p99 warm commit-state update and p99 pull selection bound, microseconds.
const DECISION_P99_US: f64 = 1_000.0;
/// Required 2-worker over 1-worker completion-rate ratio.
const TWO_WORKER_SCALING: f64 = 1.7;
/// Required 4-worker over 1-worker completion-rate ratio.
const FOUR_WORKER_SCALING: f64 = 3.0;
/// Leader CPU share of its envelope that must not be reached.
const LEADER_CPU_SHARE: f64 = 0.70;
/// Minimum worker occupancy.
const WORKER_OCCUPANCY: f64 = 0.85;

/// The machine the benchmark ran on.
#[derive(Debug, Clone, Serialize)]
pub struct Host {
    /// CPU model from `/proc/cpuinfo`.
    pub cpu_model: String,
    /// Logical CPUs available to the benchmark.
    pub cpus: usize,
    /// `MemTotal` from `/proc/meminfo`, bytes.
    pub memory_bytes: u64,
    /// `/proc/loadavg` when the benchmark started.
    pub load_at_start: String,
    /// Kernel release.
    pub kernel: String,
}

impl Host {
    /// Reads the host description from `/proc`; unreadable fields are empty.
    pub fn read() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let cpu_model = read("/proc/cpuinfo")
            .lines()
            .find_map(|line| line.strip_prefix("model name"))
            .and_then(|rest| rest.split_once(':'))
            .map(|(_, model)| model.trim().to_owned())
            .unwrap_or_default();
        let memory_bytes = read("/proc/meminfo")
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))
            .and_then(|rest| {
                rest.trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map_or(0, |kib| kib * 1024);
        Self {
            cpu_model,
            cpus: std::thread::available_parallelism().map_or(0, usize::from),
            memory_bytes,
            load_at_start: read("/proc/loadavg").trim().to_owned(),
            kernel: read("/proc/sys/kernel/osrelease").trim().to_owned(),
        }
    }
}

/// Workload settings the report was measured under.
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// Independent tables committing and compacting.
    pub tables: usize,
    /// Tenants owning them.
    pub tenants: usize,
    /// Scribe seal age, seconds (`WYRD_MAX_FILE_RETENTION_TIME`).
    pub seal_seconds: u64,
    /// Interval between one table's writes, milliseconds.
    pub write_interval_ms: u64,
    /// Rows per write.
    pub rows_per_write: usize,
    /// Warmup before a step's first window, seconds.
    pub warmup_seconds: f64,
    /// Measured window length, seconds.
    pub window_seconds: f64,
    /// Windows measured per step.
    pub windows: usize,
}

/// The scheduling case's repetitions and their pooled percentiles.
#[derive(Debug, Clone, Serialize)]
pub struct Scheduling {
    /// The case's shape.
    pub case: SchedulingCase,
    /// Each repetition.
    pub runs: Vec<SchedulingRun>,
    /// Warm commit-state update, microseconds, pooled across repetitions.
    pub commit_update_us: Percentiles,
    /// Pull selection, microseconds, pooled across repetitions.
    pub pull_us: Percentiles,
}

impl Scheduling {
    /// Pools `runs` of `case`.
    pub fn new(case: SchedulingCase, runs: Vec<SchedulingRun>) -> Self {
        let pooled = |samples: fn(&SchedulingRun) -> &[u64]| {
            let all: Vec<u64> = runs
                .iter()
                .flat_map(|run| samples(run).iter().copied())
                .collect();
            Percentiles::raw(&all, 1e-3)
        };
        Self {
            case,
            commit_update_us: pooled(|run| run.commit_update_ns.as_slice()),
            pull_us: pooled(|run| run.pull_ns.as_slice()),
            runs,
        }
    }
}

/// One worker count's windows, summarized.
#[derive(Debug, Clone, Serialize)]
pub struct StepSummary {
    /// Workers serving.
    pub workers: usize,
    /// Median window completion rate, rewrites per second.
    pub rewrites_per_second: f64,
    /// Lowest and highest window completion rates.
    pub rewrites_per_second_range: (f64, f64),
    /// Highest window leader CPU share.
    pub leader_cpu_share: f64,
    /// Mean worker occupancy across every window and worker.
    pub occupancy: f64,
}

impl StepSummary {
    /// Summarizes the windows measured with `workers` workers; `None` when
    /// none were.
    fn of(windows: &[WindowRecord], workers: usize) -> Option<Self> {
        let step: Vec<&WindowRecord> = windows.iter().filter(|w| w.workers == workers).collect();
        if step.is_empty() {
            return None;
        }
        let mut rates: Vec<f64> = step.iter().map(|w| w.rewrites_per_second).collect();
        rates.sort_by(f64::total_cmp);
        let occupancies: Vec<f64> = step
            .iter()
            .flat_map(|w| w.per_worker.iter().map(|worker| worker.occupancy))
            .collect();
        Some(Self {
            workers,
            rewrites_per_second: rates[(rates.len() - 1) / 2],
            rewrites_per_second_range: (rates[0], rates[rates.len() - 1]),
            leader_cpu_share: step.iter().map(|w| w.leader_cpu_share).fold(0.0, f64::max),
            occupancy: occupancies.iter().sum::<f64>() / occupancies.len().max(1) as f64,
        })
    }
}

/// One gating check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// What is checked.
    pub name: String,
    /// The measurement, when there was one.
    pub measured: Option<f64>,
    /// The bound, in words.
    pub threshold: String,
    /// Whether the measurement met the bound.
    pub pass: bool,
}

impl Check {
    /// A check that passes when `measured` is present and `within` holds.
    fn new(
        name: &str,
        measured: Option<f64>,
        threshold: &str,
        within: impl Fn(f64) -> bool,
    ) -> Self {
        Self {
            name: name.to_owned(),
            pass: measured.is_some_and(within),
            measured,
            threshold: threshold.to_owned(),
        }
    }
}

/// The complete report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The machine.
    pub host: Host,
    /// The measured server binary's identity.
    pub binary: serde_json::Value,
    /// Workload settings.
    pub settings: Settings,
    /// The in-process scheduling case.
    pub scheduling: Scheduling,
    /// Every measured window, in order.
    pub windows: Vec<WindowRecord>,
    /// One summary per worker count.
    pub steps: Vec<StepSummary>,
    /// Every gating check.
    pub checks: Vec<Check>,
}

impl Report {
    /// Summarizes `windows` for each of `ladder` and evaluates every check.
    pub fn new(
        host: Host,
        binary: serde_json::Value,
        settings: Settings,
        scheduling: Scheduling,
        windows: Vec<WindowRecord>,
        ladder: &[usize],
    ) -> Self {
        let steps: Vec<StepSummary> = ladder
            .iter()
            .filter_map(|workers| StepSummary::of(&windows, *workers))
            .collect();
        let rate = |workers: usize| {
            steps
                .iter()
                .find(|step| step.workers == workers)
                .map(|step| step.rewrites_per_second)
        };
        let ratio = |workers: usize| {
            let base = rate(1).filter(|base| *base > 0.0)?;
            Some(rate(workers)? / base)
        };
        let checks = vec![
            Check::new(
                "p99 warm commit-state update (µs)",
                scheduling.commit_update_us.p99,
                "< 1000",
                |us| us < DECISION_P99_US,
            ),
            Check::new(
                "p99 pull selection (µs)",
                scheduling.pull_us.p99,
                "< 1000",
                |us| us < DECISION_P99_US,
            ),
            Check::new(
                "2-worker / 1-worker completion rate",
                ratio(2),
                ">= 1.7",
                |ratio| ratio >= TWO_WORKER_SCALING,
            ),
            Check::new(
                "4-worker / 1-worker completion rate",
                ratio(4),
                ">= 3.0",
                |ratio| ratio >= FOUR_WORKER_SCALING,
            ),
            Check::new(
                "highest leader CPU share",
                (!steps.is_empty()).then(|| {
                    steps
                        .iter()
                        .map(|step| step.leader_cpu_share)
                        .fold(0.0, f64::max)
                }),
                "< 0.70",
                |share| share < LEADER_CPU_SHARE,
            ),
            Check::new(
                "lowest step worker occupancy",
                (!steps.is_empty()).then(|| {
                    steps
                        .iter()
                        .map(|step| step.occupancy)
                        .fold(f64::INFINITY, f64::min)
                }),
                ">= 0.85",
                |occupancy| occupancy >= WORKER_OCCUPANCY,
            ),
        ];
        Self {
            host,
            binary,
            settings,
            scheduling,
            windows,
            steps,
            checks,
        }
    }

    /// Whether every check passed.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.pass)
    }

    /// Writes `report.json` and `report.txt` into `directory`.
    ///
    /// # Errors
    ///
    /// Returns a serialization or write failure.
    pub fn write(&self, directory: &Path) -> Result<()> {
        std::fs::write(
            directory.join("report.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        std::fs::write(directory.join("report.txt"), self.text())?;
        Ok(())
    }

    /// The human-readable report.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let host = &self.host;
        let _ = writeln!(out, "Forge capacity benchmark");
        let _ = writeln!(
            out,
            "host: {} | {} CPUs | {:.1} GiB | kernel {} | load at start {}",
            host.cpu_model,
            host.cpus,
            host.memory_bytes as f64 / f64::from(1 << 30),
            host.kernel,
            host.load_at_start
        );
        let settings = &self.settings;
        let _ = writeln!(
            out,
            "workload: {} tables, {} tenant, seal {} s, one {}-row write per table every {} ms; \
             warmup {} s, {} windows of {} s per step",
            settings.tables,
            settings.tenants,
            settings.seal_seconds,
            settings.rows_per_write,
            settings.write_interval_ms,
            settings.warmup_seconds,
            settings.windows,
            settings.window_seconds
        );

        let case = &self.scheduling.case;
        let _ = writeln!(
            out,
            "\nscheduling case (in-process ForgeSchedule): {} tracked, {} due, {} pullers",
            case.tables, case.due, case.pullers
        );
        for (index, run) in self.scheduling.runs.iter().enumerate() {
            let _ = writeln!(
                out,
                "  run {index}: commit update p50/p99 {} µs, {:.0}/s | pull p50/p99 {} µs over {} pulls | dispatched {}",
                pair(run.commit_update_us, 1.0),
                run.commit_updates_per_second,
                pair(run.pull_us, 1.0),
                run.pulls,
                run.dispatched
            );
        }
        let _ = writeln!(
            out,
            "  pooled: commit update p50/p99 {} µs | pull p50/p99 {} µs",
            pair(self.scheduling.commit_update_us, 1.0),
            pair(self.scheduling.pull_us, 1.0)
        );

        let _ = writeln!(out, "\nwindows:");
        for window in &self.windows {
            let deps = &window.dependencies;
            let _ = writeln!(
                out,
                "  {} worker(s), {:.0} s: {:.2} rewrites/s (in {:.1} files/s, {:.0} KiB/s; out {:.1} files/s; {} unsuccessful) \
                 rewrite p50/p99 {} s | promotions {:.2}/s p50/p99 {} s | pull ceiling {:.2}/s | writes {:.0}/s ({} refused)",
                window.workers,
                window.seconds,
                window.rewrites_per_second,
                window.input_files_per_second,
                window.input_bytes_per_second / 1024.0,
                window.output_files_per_second,
                window.unsuccessful_attempts,
                pair(window.rewrite_seconds, 1.0),
                window.promotion_commits_per_second,
                pair(window.promotion_seconds, 1.0),
                window.pull_ceiling_per_second,
                window.writes_per_second,
                window.refused_writes
            );
            let decisions = &window.leader_decisions;
            let _ = writeln!(
                out,
                "    leader decisions p50/p99 µs: commit {} ({:.1}/s) | pull {} ({:.2}/s) | report {} ({:.2}/s)",
                pair(decisions.commit.us, 1.0),
                decisions.commit.per_second,
                pair(decisions.pull.us, 1.0),
                decisions.pull.per_second,
                pair(decisions.report.us, 1.0),
                decisions.report.per_second
            );
            let workers: Vec<String> = window
                .per_worker
                .iter()
                .map(|worker| {
                    format!(
                        "#{} {:.2}/s occ {:.0}% active {:.2}",
                        worker.replica,
                        worker.rewrites_per_second,
                        worker.occupancy * 100.0,
                        worker.mean_active_tasks
                    )
                })
                .collect();
            let cpu: Vec<String> = window
                .resources
                .iter()
                .map(|resource| format!("#{} {:.2}", resource.replica, resource.cores))
                .collect();
            let _ = writeln!(
                out,
                "    leader CPU {:.1}% | workers [{}] | cores [{}]",
                window.leader_cpu_share * 100.0,
                workers.join(", "),
                cpu.join(", ")
            );
            let _ = writeln!(
                out,
                "    postgres {:.0} xact/s, {:.0} blk read/s, {:.0} tup written/s, {:.1} active backends, {} cores | rustfs {} cores | docker VM {} CPUs",
                deps.postgres_transactions_per_second,
                deps.postgres_blocks_read_per_second,
                deps.postgres_tuples_written_per_second,
                deps.postgres_active_backends,
                optional(deps.postgres_cores),
                optional(deps.rustfs_cores),
                optional(deps.docker_vm_cpus)
            );
        }

        let _ = writeln!(out, "\nsteps:");
        for step in &self.steps {
            let _ = writeln!(
                out,
                "  {} worker(s): median {:.2} rewrites/s (range {:.2}-{:.2}), leader CPU max {:.1}%, occupancy {:.0}%",
                step.workers,
                step.rewrites_per_second,
                step.rewrites_per_second_range.0,
                step.rewrites_per_second_range.1,
                step.leader_cpu_share * 100.0,
                step.occupancy * 100.0
            );
        }

        let _ = writeln!(out, "\nchecks:");
        for check in &self.checks {
            let _ = writeln!(
                out,
                "  {} {}: {} (needs {})",
                if check.pass { "PASS" } else { "FAIL" },
                check.name,
                optional(check.measured),
                check.threshold
            );
        }
        out
    }
}

/// `p50/p99` of `percentiles` scaled by `scale`, `-` where absent.
fn pair(percentiles: Percentiles, scale: f64) -> String {
    let one = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{:.3}", v * scale));
    format!("{}/{}", one(percentiles.p50), one(percentiles.p99))
}

/// A measurement to three decimals, `-` when absent.
fn optional(value: Option<f64>) -> String {
    value.map_or("-".to_owned(), |v| format!("{v:.3}"))
}
