//! Measurement owners shared by the opt-in capacity benchmark binaries.
//!
//! Every benchmark under `src/bin/` launches the release `wyrd-server` beside
//! itself, reads `WYRD_LOG` for client-side tracing, windows replica cgroup
//! CPU and peak memory, and reports nearest-rank or bucket-estimated
//! percentiles. Those pieces live here once so each binary owns only its
//! workload, verdicts, and report shape.
//!
//! Histogram percentiles are bucket estimates: the upper bound of the
//! cumulative bucket covering the quantile, computed from a window's bucket
//! deltas after summing every replica's buckets, never by averaging replica
//! percentiles. Raw percentiles come from per-sample measurements.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::release_server::{LocalServer, MemoryPeak, Metrics};

/// Error type of the capacity helpers, matching the benchmark binaries.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// p50, p95, and p99 of something.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Percentiles {
    /// Median.
    pub p50: Option<f64>,
    /// 95th percentile.
    pub p95: Option<f64>,
    /// 99th percentile.
    pub p99: Option<f64>,
}

impl Percentiles {
    /// Nearest-rank percentiles of raw `samples`, each scaled by `scale`.
    ///
    /// The samples are copied and sorted; an empty slice yields no
    /// percentiles.
    pub fn raw(samples: &[u64], scale: f64) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let rank = |q: f64| -> Option<f64> {
            if sorted.is_empty() {
                return None;
            }
            let index = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
            Some(sorted[index] as f64 * scale)
        };
        Self {
            p50: rank(0.5),
            p95: rank(0.95),
            p99: rank(0.99),
        }
    }

    /// Bucket-estimated percentiles of the observations between two
    /// cumulative bucket readings of one histogram, via [`quantile`].
    pub fn buckets(before: &[(f64, f64)], after: &[(f64, f64)]) -> Self {
        Self {
            p50: quantile(before, after, 0.5),
            p95: quantile(before, after, 0.95),
            p99: quantile(before, after, 0.99),
        }
    }
}

/// The cumulative buckets of `family` matching `labels`, summed across every
/// replica's scrape and sorted by bound.
pub fn merged(scrapes: &[Metrics], family: &str, labels: &[&str]) -> Vec<(f64, f64)> {
    let mut buckets: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
    for scrape in scrapes {
        for (bound, count) in scrape.buckets(family, labels) {
            buckets.entry(bound.to_bits()).or_insert((bound, 0.0)).1 += count;
        }
    }
    let mut sorted: Vec<(f64, f64)> = buckets.into_values().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    sorted
}

/// Per-bound increase between two cumulative bucket readings; a bound absent
/// from `before` counts from zero.
pub fn deltas(before: &[(f64, f64)], after: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let prior = |bound: f64| {
        before
            .iter()
            .find(|(b, _)| b.to_bits() == bound.to_bits())
            .map_or(0.0, |(_, count)| *count)
    };
    after
        .iter()
        .map(|(bound, count)| (*bound, count - prior(*bound)))
        .collect()
}

/// The bound of the cumulative bucket holding quantile `q` of the
/// observations between two readings of one histogram, or `None` when none
/// were recorded.
pub fn quantile(before: &[(f64, f64)], after: &[(f64, f64)], q: f64) -> Option<f64> {
    let deltas = deltas(before, after);
    let total = deltas.last().map_or(0.0, |(_, count)| *count);
    if total <= 0.0 {
        return None;
    }
    deltas
        .iter()
        .find(|(_, count)| *count >= q * total)
        .map(|(bound, _)| *bound)
}

/// One replica's resource use over a measurement window.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// Replica ordinal.
    pub replica: u16,
    /// Cgroup CPU seconds consumed.
    pub cpu_seconds: f64,
    /// Mean cores: CPU seconds over window seconds.
    pub cores: f64,
    /// Peak cgroup memory, bytes.
    pub peak_memory: u64,
}

/// Replica cgroup CPU and peak-memory readings at a window's start.
///
/// A benchmark opens the window before its workload, freezes CPU when the
/// measured interval ends (before any drain), and finishes once it no longer
/// needs peak memory, so CPU covers only the measured interval while peak
/// memory covers everything after the open.
pub struct ResourceWindow {
    /// Each replica's ordinal, `cpu.stat usage_usec` at the open, and the
    /// peak-memory descriptor reset at the open.
    opened: Vec<(u16, u64, MemoryPeak)>,
    /// Each replica's `usage_usec` once [`Self::freeze`] ran.
    frozen: Option<Vec<u64>>,
}

impl ResourceWindow {
    /// Reads every replica's CPU counter and resets its peak memory.
    ///
    /// # Errors
    ///
    /// Returns a replica whose `memory.peak` cannot be opened or reset.
    pub fn open<'a>(replicas: impl IntoIterator<Item = &'a LocalServer>) -> Result<Self, Error> {
        let mut opened = Vec::new();
        for replica in replicas {
            opened.push((
                replica.ordinal(),
                replica.cgroup_stat("cpu.stat", "usage_usec"),
                replica.memory_peak()?,
            ));
        }
        Ok(Self {
            opened,
            frozen: None,
        })
    }

    /// Reads every replica's CPU counter at the end of the measured interval.
    ///
    /// `replicas` must be the same replicas, in the same order, as the open;
    /// a later call replaces the earlier reading.
    pub fn freeze<'a>(&mut self, replicas: impl IntoIterator<Item = &'a LocalServer>) {
        self.frozen = Some(
            replicas
                .into_iter()
                .map(|replica| replica.cgroup_stat("cpu.stat", "usage_usec"))
                .collect(),
        );
    }

    /// Each replica's resources over `window_seconds`, using the frozen CPU
    /// reading, or a fresh one from `replicas` when never frozen.
    ///
    /// # Errors
    ///
    /// Returns a peak-memory read failure.
    pub fn finish<'a>(
        mut self,
        replicas: impl IntoIterator<Item = &'a LocalServer>,
        window_seconds: f64,
    ) -> Result<Vec<Resources>, Error> {
        if self.frozen.is_none() {
            self.freeze(replicas);
        }
        let frozen = self.frozen.unwrap_or_default();
        let mut resources = Vec::new();
        for ((replica, cpu_before, peak), cpu_after) in self.opened.into_iter().zip(frozen) {
            let cpu_seconds = cpu_after.saturating_sub(cpu_before) as f64 / 1e6;
            resources.push(Resources {
                replica,
                cpu_seconds,
                cores: cpu_seconds / window_seconds.max(f64::EPSILON),
                peak_memory: peak.read()?,
            });
        }
        Ok(resources)
    }
}

/// CPU seconds this process has used (user plus system), from
/// `/proc/self/stat`; zero when unreadable.
pub fn driver_cpu_seconds() -> f64 {
    // ponytail: assumes USER_HZ = 100, true on every mainstream Linux build.
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // Fields after the parenthesized command; utime and stime are the 14th
    // and 15th fields overall.
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map_or(Vec::new(), |(_, rest)| rest.split_whitespace().collect());
    let ticks = |index: usize| {
        fields
            .get(index)
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

/// The release `wyrd-server` built beside the running benchmark binary.
///
/// # Errors
///
/// Returns an error when the current executable cannot be located or the
/// server has not been built beside it.
pub fn release_binary() -> Result<PathBuf, Error> {
    let binary = std::env::current_exe()?.with_file_name("wyrd-server");
    if binary.is_file() {
        Ok(binary)
    } else {
        Err(format!("{} is not built; run through mise", binary.display()).into())
    }
}

/// The identity of the measured server `binary` a report records: its path,
/// size, and modification time.
///
/// # Errors
///
/// Returns the metadata failure.
pub fn binary_identity(binary: &Path) -> Result<Value, Error> {
    let metadata = std::fs::metadata(binary)?;
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    Ok(serde_json::json!({
        "path": binary.display().to_string(),
        "bytes": metadata.len(),
        "modified": modified,
    }))
}

/// Installs a stderr log subscriber when `WYRD_LOG`, else `RUST_LOG`, is set,
/// so client-side failures read alongside the server logs. A subscriber
/// already installed wins.
pub fn install_tracing() {
    let Ok(filter) = std::env::var("WYRD_LOG").or_else(|_| std::env::var("RUST_LOG")) else {
        return;
    };
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .finish(),
    );
}

#[cfg(test)]
mod tests {
    use super::{Percentiles, deltas, quantile};

    /// A quantile is the first cumulative bucket bound covering it among the
    /// observations between two readings, and an empty interval has none.
    ///
    /// # Panics
    ///
    /// Panics when a bound is wrong.
    #[test]
    fn quantile_reads_bucket_deltas() {
        let before = [(0.1, 10.0), (1.0, 10.0), (f64::INFINITY, 10.0)];
        let after = [(0.1, 15.0), (1.0, 105.0), (f64::INFINITY, 110.0)];
        assert_eq!(quantile(&before, &after, 0.05), Some(0.1));
        assert_eq!(quantile(&before, &after, 0.5), Some(1.0));
        assert_eq!(quantile(&before, &after, 0.99), Some(f64::INFINITY));
        assert_eq!(quantile(&after, &after, 0.5), None);
    }

    /// A bound missing from the earlier reading counts from zero.
    ///
    /// # Panics
    ///
    /// Panics when a delta is wrong.
    #[test]
    fn deltas_count_new_bounds_from_zero() {
        let after = [(0.1, 4.0), (f64::INFINITY, 6.0)];
        assert_eq!(
            deltas(&[(0.1, 1.0)], &after),
            vec![(0.1, 3.0), (f64::INFINITY, 6.0)]
        );
    }

    /// Raw percentiles use the nearest rank and an empty sample has none.
    ///
    /// # Panics
    ///
    /// Panics when a percentile is wrong.
    #[test]
    fn raw_percentiles_use_nearest_rank() {
        let samples: Vec<u64> = (1..=100).collect();
        let percentiles = Percentiles::raw(&samples, 1.0);
        assert_eq!(
            (percentiles.p50, percentiles.p95, percentiles.p99),
            (Some(50.0), Some(95.0), Some(99.0))
        );
        assert_eq!(Percentiles::raw(&[], 1.0).p50, None);
    }
}
