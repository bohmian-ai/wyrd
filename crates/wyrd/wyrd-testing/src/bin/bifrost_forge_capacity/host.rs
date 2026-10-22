//! The host the benchmark runs on and the load it carries before a run.
//!
//! Every Wyrd process together stays within an 8-CPU envelope on a 16-CPU
//! host, so a run is only reported when the host had at least that many CPUs
//! free before it started. The load is sampled over a short interval through
//! `sysinfo` (busy CPUs, load average, and every process's CPU use), so a run
//! that was disturbed by other work can be identified and discarded rather
//! than reported.

use std::time::Duration;

use serde::Serialize;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
};

/// CPUs that must be free on the host for a run to count: the whole Wyrd
/// envelope.
pub const REQUIRED_FREE_CPUS: f64 = 8.0;

/// The machine the benchmark ran on.
#[derive(Debug, Clone, Serialize)]
pub struct Host {
    /// CPU model, empty when the OS reports none.
    pub cpu_model: String,
    /// Logical CPUs available to the benchmark.
    pub cpus: usize,
    /// Physical memory, bytes.
    pub memory_bytes: u64,
    /// Kernel version.
    pub kernel: String,
}

/// Host load over one sampling interval.
#[derive(Debug, Clone, Serialize)]
pub struct HostLoad {
    /// One, five, and fifteen minute load averages at the end of the sample.
    pub loadavg: String,
    /// Mean busy CPUs over the sample.
    pub busy_cpus: f64,
    /// CPUs left free: the host's CPUs less the larger of the busy CPUs and
    /// the one-minute load average.
    pub free_cpus: f64,
    /// Mean CPUs each process used over the sample, busiest first.
    pub processes: Vec<(String, f64)>,
}

impl Host {
    /// Reads the host description; unreadable fields are empty.
    pub fn read() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        Self {
            cpu_model: system
                .cpus()
                .first()
                .map(|cpu| cpu.brand().trim().to_owned())
                .unwrap_or_default(),
            cpus: std::thread::available_parallelism().map_or(0, usize::from),
            memory_bytes: system.total_memory(),
            kernel: System::kernel_version().unwrap_or_default(),
        }
    }
}

impl HostLoad {
    /// Samples the host's load over `interval`.
    ///
    /// CPU use is the difference between two refreshes `interval` apart.
    pub async fn sample(cpus: usize, interval: Duration) -> Self {
        let mut system = System::new();
        let processes = ProcessRefreshKind::nothing().with_cpu();
        system.refresh_cpu_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, processes);
        tokio::time::sleep(interval).await;
        system.refresh_cpu_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, processes);

        let busy_cpus = f64::from(system.global_cpu_usage()) / 100.0 * cpus as f64;
        let load = System::load_average();
        let mut processes: Vec<(String, f64)> = system
            .processes()
            .values()
            .map(|process| {
                (
                    process.name().to_string_lossy().into_owned(),
                    f64::from(process.cpu_usage()) / 100.0,
                )
            })
            .filter(|(_, cpus)| *cpus > 0.0)
            .collect();
        processes.sort_by(|a, b| b.1.total_cmp(&a.1));
        Self {
            loadavg: format!("{:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
            busy_cpus,
            free_cpus: cpus as f64 - busy_cpus.max(load.one),
            processes,
        }
    }

    /// Whether the host left the whole Wyrd envelope free.
    pub fn qualifies(&self) -> bool {
        self.free_cpus >= REQUIRED_FREE_CPUS
    }

    /// One line describing the sample.
    pub fn describe(&self) -> String {
        let processes: Vec<String> = self
            .processes
            .iter()
            .take(4)
            .map(|(name, cpus)| format!("{name} {cpus:.2}"))
            .collect();
        format!(
            "loadavg {} | busy {:.2} CPUs | free {:.2} CPUs | processes [{}]",
            self.loadavg,
            self.busy_cpus,
            self.free_cpus,
            processes.join(", ")
        )
    }
}
