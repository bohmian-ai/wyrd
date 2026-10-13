//! The host and process readings a [`super::LocalServer`] running on the
//! whole host needs, for the two operating systems the benchmarks run on.
//!
//! Linux reads `/proc`. macOS reads through safe wrappers over the kernel
//! interfaces: `sysinfo` for the host and CPU time, and `libproc`'s
//! `proc_pid_rusage` for a process's lifetime peak footprint.

#[cfg(target_os = "linux")]
pub(crate) use imp::reset_peak_memory;
pub(crate) use imp::{
    cpu_model, memory_bytes, peak_memory_bytes, process_cpu_usec, self_cpu_seconds,
};

/// `/proc` readings.
#[cfg(target_os = "linux")]
mod imp {
    use std::io;

    /// Clock ticks per second of `/proc/<pid>/stat` CPU times.
    // ponytail: Linux's fixed userspace tick on every mainstream
    // architecture; read sysconf(_SC_CLK_TCK) if that ever differs.
    const USER_HZ: u64 = 100;

    /// The first `model name` of `/proc/cpuinfo`, or `unknown`.
    pub(crate) fn cpu_model() -> String {
        std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find_map(|line| line.strip_prefix("model name")?.split_once(':'))
            .map_or_else(
                || "unknown".to_owned(),
                |(_, model)| model.trim().to_owned(),
            )
    }

    /// `MemTotal` of `/proc/meminfo`, in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is unreadable or has no `MemTotal`.
    pub(crate) fn memory_bytes() -> io::Result<u64> {
        kib_field("/proc/meminfo", "MemTotal:")
    }

    /// Microseconds of user plus system CPU process `pid` has used; zero
    /// when it is gone.
    pub(crate) fn process_cpu_usec(pid: u32) -> u64 {
        stat_ticks(&format!("/proc/{pid}/stat")).unwrap_or(0) * (1_000_000 / USER_HZ)
    }

    /// Seconds of user plus system CPU this process has used; zero when
    /// unreadable.
    pub(crate) fn self_cpu_seconds() -> f64 {
        stat_ticks("/proc/self/stat").unwrap_or(0) as f64 / USER_HZ as f64
    }

    /// Resets process `pid`'s peak resident set (`VmHWM`) to its current
    /// one by writing `5` to `/proc/<pid>/clear_refs`.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub(crate) fn reset_peak_memory(pid: u32) -> io::Result<()> {
        std::fs::write(format!("/proc/{pid}/clear_refs"), "5")
    }

    /// Process `pid`'s peak resident set since its last reset, in bytes:
    /// `VmHWM` of `/proc/<pid>/status`.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is unreadable or has no `VmHWM`.
    pub(crate) fn peak_memory_bytes(pid: u32) -> io::Result<u64> {
        kib_field(&format!("/proc/{pid}/status"), "VmHWM:")
    }

    /// Reads a `<key> <n> kB` line of `path` as bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when `path` is unreadable or has no parseable `key`.
    fn kib_field(path: &str, key: &str) -> io::Result<u64> {
        std::fs::read_to_string(path)?
            .lines()
            .find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .map(|kib| kib * 1024)
            .ok_or_else(|| io::Error::other(format!("{path} has no {key}")))
    }

    /// `utime + stime` of a `/proc/<pid>/stat` file, in clock ticks: the
    /// 12th and 13th fields after the parenthesized command name.
    fn stat_ticks(path: &str) -> Option<u64> {
        let stat = std::fs::read_to_string(path).ok()?;
        let mut fields = stat.rsplit_once(')')?.1.split_whitespace().skip(11);
        let utime: u64 = fields.next()?.parse().ok()?;
        let stime: u64 = fields.next()?.parse().ok()?;
        Some(utime + stime)
    }

    #[cfg(test)]
    mod tests {
        /// This process has a CPU model, memory, CPU time, and a peak
        /// resident set that survives a reset.
        ///
        /// # Panics
        ///
        /// Panics when a reading is missing or zero.
        #[test]
        fn reads_this_process_and_host() {
            let pid = std::process::id();
            assert_ne!(super::cpu_model(), "unknown");
            assert!(super::memory_bytes().expect("MemTotal") > 0);
            let busy: u64 = (0..20_000_000_u64).fold(0, |sum, n| sum.wrapping_add(n * n));
            std::hint::black_box(busy);
            assert!(super::process_cpu_usec(pid) > 0);
            assert!(super::self_cpu_seconds() > 0.0);
            super::reset_peak_memory(pid).expect("clear_refs");
            assert!(super::peak_memory_bytes(pid).expect("VmHWM") > 0);
        }
    }
}

/// `sysinfo` and `libproc` readings.
///
/// The workspace denies `unsafe`, so macOS reads through these safe
/// wrappers rather than calling `libc` directly.
#[cfg(target_os = "macos")]
mod imp {
    use std::io;

    use libproc::pid_rusage::{RUsageInfoV4, pidrusage};
    use sysinfo::{
        CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    };

    /// `machdep.cpu.brand_string`, or `unknown`.
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

    /// `hw.memsize`, the physical memory in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the kernel reports no physical memory.
    pub(crate) fn memory_bytes() -> io::Result<u64> {
        let mut system = System::new();
        system.refresh_memory();
        match system.total_memory() {
            0 => Err(io::Error::other("sysinfo read no physical memory")),
            bytes => Ok(bytes),
        }
    }

    /// Microseconds of user plus system CPU process `pid` has used; zero
    /// when it is gone.
    ///
    /// `sysinfo` converts Apple-silicon Mach time to wall units and reports
    /// milliseconds.
    pub(crate) fn process_cpu_usec(pid: u32) -> u64 {
        let pid = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu(),
        );
        system.process(pid).map_or(0, |process| {
            process.accumulated_cpu_time().saturating_mul(1_000)
        })
    }

    /// Seconds of user plus system CPU this process has used; zero when
    /// unreadable.
    pub(crate) fn self_cpu_seconds() -> f64 {
        process_cpu_usec(std::process::id()) as f64 / 1e6
    }

    /// Process `pid`'s lifetime peak physical footprint, in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the pid is out of range or the process cannot
    /// be read.
    pub(crate) fn peak_memory_bytes(pid: u32) -> io::Result<u64> {
        let pid = i32::try_from(pid).map_err(io::Error::other)?;
        let info = pidrusage::<RUsageInfoV4>(pid).map_err(io::Error::other)?;
        Ok(info.ri_lifetime_max_phys_footprint)
    }

    #[cfg(test)]
    mod tests {
        /// This process has a CPU model, memory, CPU time, and a peak
        /// footprint.
        ///
        /// # Panics
        ///
        /// Panics when a reading is missing or zero.
        #[test]
        fn reads_this_process_and_host() {
            let pid = std::process::id();
            assert_ne!(super::cpu_model(), "unknown");
            assert!(super::memory_bytes().expect("hw.memsize") > 0);
            let busy: u64 = (0..20_000_000_u64).fold(0, |sum, n| sum.wrapping_add(n * n));
            std::hint::black_box(busy);
            assert!(super::process_cpu_usec(pid) > 0);
            assert!(super::self_cpu_seconds() > 0.0);
            assert!(super::peak_memory_bytes(pid).expect("proc_pid_rusage") > 0);
        }
    }
}
