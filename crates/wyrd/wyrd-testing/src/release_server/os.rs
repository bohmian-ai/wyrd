//! The host and process readings a [`super::LocalServer`] running on the
//! whole host needs, for the two operating systems the benchmarks run on.
//!
//! Linux reads `/proc`. macOS asks the kernel through `libc`: `sysctl` for
//! the host and `proc_pid_rusage` for a process.

pub(crate) use imp::{
    cpu_model, memory_bytes, peak_memory_bytes, process_cpu_usec, reset_peak_memory,
    self_cpu_seconds,
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

/// `sysctl` and `proc_pid_rusage` readings.
#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::CStr;
    use std::io;
    use std::mem::MaybeUninit;

    /// Ratio converting Mach absolute time to nanoseconds, which
    /// `rusage_info_v4` CPU times are counted in on Apple silicon.
    #[repr(C)]
    struct Timebase {
        /// Numerator of the ratio.
        numer: u32,
        /// Denominator of the ratio.
        denom: u32,
    }

    unsafe extern "C" {
        /// Fills `info` with the Mach timebase; `libc`'s binding is
        /// deprecated in favour of the `mach2` crate, which this one call
        /// does not warrant.
        fn mach_timebase_info(info: *mut Timebase) -> libc::c_int;
    }

    /// `machdep.cpu.brand_string`, or `unknown`.
    pub(crate) fn cpu_model() -> String {
        let mut buffer = [0_u8; 256];
        let mut len = buffer.len();
        // SAFETY: `buffer` holds `len` writable bytes and the name is
        // NUL-terminated; sysctlbyname writes at most `len` bytes.
        let status = unsafe {
            libc::sysctlbyname(
                c"machdep.cpu.brand_string".as_ptr(),
                buffer.as_mut_ptr().cast(),
                &raw mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return "unknown".to_owned();
        }
        String::from_utf8_lossy(&buffer[..len])
            .trim_end_matches('\0')
            .trim()
            .to_owned()
    }

    /// `hw.memsize`, the physical memory in bytes.
    ///
    /// # Errors
    ///
    /// Returns the OS error when the sysctl fails.
    pub(crate) fn memory_bytes() -> io::Result<u64> {
        sysctl_u64(c"hw.memsize")
    }

    /// Microseconds of user plus system CPU process `pid` has used; zero
    /// when it is gone.
    pub(crate) fn process_cpu_usec(pid: u32) -> u64 {
        let Ok(info) = rusage(pid) else {
            return 0;
        };
        let mut timebase = Timebase { numer: 0, denom: 0 };
        // SAFETY: mach_timebase_info fills the two-field struct it is given.
        if unsafe { mach_timebase_info(&raw mut timebase) } != 0 || timebase.denom == 0 {
            return 0;
        }
        let nanos = u128::from(info.ri_user_time.saturating_add(info.ri_system_time))
            * u128::from(timebase.numer)
            / u128::from(timebase.denom);
        u64::try_from(nanos / 1_000).unwrap_or(u64::MAX)
    }

    /// Seconds of user plus system CPU this process has used; zero when
    /// unreadable.
    pub(crate) fn self_cpu_seconds() -> f64 {
        let mut usage = MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage writes a complete rusage on success.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return 0.0;
        }
        // SAFETY: the call succeeded, so the value is initialized.
        let usage = unsafe { usage.assume_init() };
        let seconds = |time: libc::timeval| time.tv_sec as f64 + f64::from(time.tv_usec) / 1e6;
        seconds(usage.ru_utime) + seconds(usage.ru_stime)
    }

    /// macOS keeps only a process's lifetime peak, so there is nothing to
    /// reset; a window's peak is the peak since the process started.
    ///
    /// # Errors
    ///
    /// Never fails; fallible to match the Linux reset.
    pub(crate) fn reset_peak_memory(_pid: u32) -> io::Result<()> {
        Ok(())
    }

    /// Process `pid`'s lifetime peak physical footprint, in bytes.
    ///
    /// # Errors
    ///
    /// Returns the OS error when the process cannot be read.
    pub(crate) fn peak_memory_bytes(pid: u32) -> io::Result<u64> {
        Ok(rusage(pid)?.ri_lifetime_max_phys_footprint)
    }

    /// Reads an integer sysctl.
    ///
    /// # Errors
    ///
    /// Returns the OS error when the sysctl fails.
    fn sysctl_u64(name: &CStr) -> io::Result<u64> {
        let mut value = 0_u64;
        let mut len = size_of::<u64>();
        // SAFETY: `value` is a u64 of exactly `len` writable bytes and the
        // name is NUL-terminated.
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                (&raw mut value).cast(),
                &raw mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(value)
    }

    /// Process `pid`'s `rusage_info_v4`.
    ///
    /// # Errors
    ///
    /// Returns the OS error when the process cannot be read.
    fn rusage(pid: u32) -> io::Result<libc::rusage_info_v4> {
        let pid = libc::c_int::try_from(pid).map_err(io::Error::other)?;
        let mut info = MaybeUninit::<libc::rusage_info_v4>::uninit();
        // SAFETY: RUSAGE_INFO_V4 fills a complete rusage_info_v4.
        let status =
            unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast()) };
        if status != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the call succeeded, so the value is initialized.
        Ok(unsafe { info.assume_init() })
    }
}
