//! The host and process readings a [`super::LocalServer`] running on the
//! whole host needs, read through `sysinfo` so every supported OS shares one
//! implementation.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind,
    System,
};

/// How often a [`PeakSampler`] reads the process's resident memory.
// ponytail: a spike shorter than this can be missed; shorten it if a
// benchmark's peak ever depends on one.
const PEAK_SAMPLE_INTERVAL: Duration = Duration::from_millis(10);

/// The first CPU's brand string, or `unknown`.
pub(crate) fn cpu_model() -> String {
    let system =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
    system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .filter(|brand| !brand.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The host's physical memory, in bytes.
///
/// # Errors
///
/// Returns an error when the OS reports no physical memory.
pub(crate) fn memory_bytes() -> io::Result<u64> {
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    match system.total_memory() {
        0 => Err(io::Error::other("sysinfo read no physical memory")),
        bytes => Ok(bytes),
    }
}

/// Refreshes `system`'s view of process `pid` for `kind`, returning whether
/// the process still exists.
fn refresh_process(system: &mut System, pid: Pid, kind: ProcessRefreshKind) -> bool {
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
    system.process(pid).is_some()
}

/// Microseconds of user plus system CPU process `pid` has used; zero when it
/// is gone. `sysinfo` reports milliseconds.
pub(crate) fn process_cpu_usec(pid: u32) -> u64 {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    refresh_process(&mut system, pid, ProcessRefreshKind::nothing().with_cpu());
    system.process(pid).map_or(0, |process| {
        process.accumulated_cpu_time().saturating_mul(1_000)
    })
}

/// Seconds of user plus system CPU this process has used; zero when
/// unreadable.
pub(crate) fn self_cpu_seconds() -> f64 {
    process_cpu_usec(std::process::id()) as f64 / 1e6
}

/// A background thread recording the highest resident memory one process
/// reaches from [`PeakSampler::start`] until [`PeakSampler::finish`].
///
/// The OS keeps no portable resettable peak, so the window's peak is the
/// largest of readings taken every [`PEAK_SAMPLE_INTERVAL`].
pub struct PeakSampler {
    /// Set to end the sampling loop.
    stop: Arc<AtomicBool>,
    /// The sampling thread, returning the peak it saw; taken by `finish`.
    thread: Option<JoinHandle<u64>>,
}

impl PeakSampler {
    /// Starts sampling process `pid`'s resident memory on a new thread.
    ///
    /// The thread reads until stopped or the process exits, keeping the
    /// largest reading.
    pub(crate) fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let pid = Pid::from_u32(pid);
            let mut system = System::new();
            let mut peak = 0;
            while refresh_process(
                &mut system,
                pid,
                ProcessRefreshKind::nothing().with_memory(),
            ) {
                peak = system
                    .process(pid)
                    .map_or(peak, |process| peak.max(process.memory()));
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(PEAK_SAMPLE_INTERVAL);
            }
            peak
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }

    /// Stops sampling and returns the peak resident memory seen, in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the sampling thread panicked or never read the
    /// process.
    pub(crate) fn finish(mut self) -> io::Result<u64> {
        self.stop.store(true, Ordering::Relaxed);
        let thread = self.thread.take().expect("a sampler is finished once");
        match thread.join() {
            Ok(0) => Err(io::Error::other("the process was never sampled")),
            Ok(peak) => Ok(peak),
            Err(_) => Err(io::Error::other("the memory sampler panicked")),
        }
    }
}

impl Drop for PeakSampler {
    /// Stops an unfinished sampler so its thread exits.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    /// This process has a CPU model, memory, CPU time, and a sampled peak.
    ///
    /// # Panics
    ///
    /// Panics when a reading is missing or zero.
    #[test]
    fn reads_this_process_and_host() {
        let pid = std::process::id();
        let sampler = super::PeakSampler::start(pid);
        assert_ne!(super::cpu_model(), "unknown");
        assert!(super::memory_bytes().expect("physical memory") > 0);
        let busy: u64 = (0..20_000_000_u64).fold(0, |sum, n| sum.wrapping_add(n * n));
        std::hint::black_box(busy);
        assert!(super::process_cpu_usec(pid) > 0);
        assert!(super::self_cpu_seconds() > 0.0);
        assert!(sampler.finish().expect("sampled peak") > 0);
    }
}
