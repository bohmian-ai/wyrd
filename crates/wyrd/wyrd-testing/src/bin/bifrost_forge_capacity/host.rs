//! The host the benchmark runs on and the load it carries before a run.
//!
//! Every Wyrd process together stays within an 8-CPU envelope on a 16-CPU
//! host, so a run is only reported when the host had at least that many CPUs
//! free before it started. The load is sampled over a short interval from
//! `/proc/stat` (busy CPUs), `/proc/loadavg`, and every top-level cgroup's
//! `cpu.stat`, so a run that was disturbed by other work can be identified
//! and discarded rather than reported.

use std::time::Duration;

use serde::Serialize;

/// CPUs that must be free on the host for a run to count: the whole Wyrd
/// envelope.
pub const REQUIRED_FREE_CPUS: f64 = 8.0;

/// The machine the benchmark ran on.
#[derive(Debug, Clone, Serialize)]
pub struct Host {
    /// CPU model from `/proc/cpuinfo`.
    pub cpu_model: String,
    /// Logical CPUs available to the benchmark.
    pub cpus: usize,
    /// `MemTotal` from `/proc/meminfo`, bytes.
    pub memory_bytes: u64,
    /// Kernel release.
    pub kernel: String,
}

/// Host load over one sampling interval.
#[derive(Debug, Clone, Serialize)]
pub struct HostLoad {
    /// `/proc/loadavg` at the end of the sample.
    pub loadavg: String,
    /// Mean busy CPUs over the sample, from `/proc/stat`.
    pub busy_cpus: f64,
    /// CPUs left free: the host's CPUs less the larger of the busy CPUs and
    /// the one-minute load average.
    pub free_cpus: f64,
    /// Mean CPUs each top-level cgroup used over the sample, busiest first.
    pub cgroups: Vec<(String, f64)>,
}

impl Host {
    /// Reads the host description from `/proc`; unreadable fields are empty.
    pub fn read() -> Self {
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
            kernel: read("/proc/sys/kernel/osrelease").trim().to_owned(),
        }
    }
}

impl HostLoad {
    /// Samples the host's load over `interval`.
    pub async fn sample(cpus: usize, interval: Duration) -> Self {
        let (stat_before, cgroups_before) = (cpu_jiffies(), cgroup_usage());
        tokio::time::sleep(interval).await;
        let (stat_after, cgroups_after) = (cpu_jiffies(), cgroup_usage());
        let busy_cpus = match (stat_before, stat_after) {
            (Some((busy0, total0)), Some((busy1, total1))) if total1 > total0 => {
                (busy1 - busy0) as f64 / (total1 - total0) as f64 * cpus as f64
            }
            _ => 0.0,
        };
        let loadavg = read("/proc/loadavg").trim().to_owned();
        let one_minute = loadavg
            .split_whitespace()
            .next()
            .and_then(|load| load.parse::<f64>().ok())
            .unwrap_or(0.0);
        let mut cgroups: Vec<(String, f64)> = cgroups_after
            .into_iter()
            .filter_map(|(name, after)| {
                let before = cgroups_before.iter().find(|(n, _)| *n == name)?.1;
                Some((
                    name,
                    after.saturating_sub(before) as f64 / 1e6 / interval.as_secs_f64(),
                ))
            })
            .collect();
        cgroups.sort_by(|a, b| b.1.total_cmp(&a.1));
        Self {
            loadavg,
            busy_cpus,
            free_cpus: cpus as f64 - busy_cpus.max(one_minute),
            cgroups,
        }
    }

    /// Whether the host left the whole Wyrd envelope free.
    pub fn qualifies(&self) -> bool {
        self.free_cpus >= REQUIRED_FREE_CPUS
    }

    /// One line describing the sample.
    pub fn describe(&self) -> String {
        let cgroups: Vec<String> = self
            .cgroups
            .iter()
            .take(4)
            .map(|(name, cpus)| format!("{name} {cpus:.2}"))
            .collect();
        format!(
            "loadavg {} | busy {:.2} CPUs | free {:.2} CPUs | cgroups [{}]",
            self.loadavg,
            self.busy_cpus,
            self.free_cpus,
            cgroups.join(", ")
        )
    }
}

/// `path`'s contents, empty when unreadable.
fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Busy and total jiffies of the aggregate `cpu` line of `/proc/stat`; idle
/// and iowait are not busy.
fn cpu_jiffies() -> Option<(u64, u64)> {
    let stat = read("/proc/stat");
    let fields: Vec<u64> = stat
        .lines()
        .next()?
        .strip_prefix("cpu ")?
        .split_whitespace()
        .filter_map(|field| field.parse().ok())
        .collect();
    let total: u64 = fields.iter().take(8).sum();
    let idle = fields.get(3)? + fields.get(4)?;
    Some((total - idle, total))
}

/// `usage_usec` of every top-level cgroup.
fn cgroup_usage() -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir("/sys/fs/cgroup") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let usage = std::fs::read_to_string(entry.path().join("cpu.stat"))
                .ok()?
                .lines()
                .find_map(|line| line.strip_prefix("usage_usec "))?
                .trim()
                .parse()
                .ok()?;
            Some((entry.file_name().to_string_lossy().into_owned(), usage))
        })
        .collect()
}
