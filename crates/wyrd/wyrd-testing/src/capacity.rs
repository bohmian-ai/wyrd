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
//!
//! A workload names the server histograms and runtime components it reads in
//! one [`Selection`]; a [`Window`] reads them, the server's CPU and peak
//! memory, and this process's CPU at its open and differences them into a
//! [`Usage`] at its close. Workload verdicts and reports stay with each
//! workload.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use num_traits::ToPrimitive;
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
            let rank = (q * sorted.len() as f64).ceil().to_usize().unwrap_or(0);
            let index = rank.clamp(1, sorted.len()) - 1;
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

/// One server histogram's observations, cumulative in one scrape or over a
/// window.
#[derive(Debug, Default, Clone)]
pub struct Histogram {
    /// Cumulative `le` buckets.
    pub buckets: Vec<(f64, f64)>,
    /// Observed seconds.
    pub sum: f64,
    /// Observations.
    pub count: f64,
}

impl Histogram {
    /// The histogram of every `family` series whose labels contain each of
    /// `labels` in one scrape, summed across those series.
    #[must_use]
    pub fn read(metrics: &Metrics, family: &str, labels: &[&str]) -> Self {
        Self {
            buckets: metrics.buckets(family, labels),
            sum: metrics.sum(&format!("{family}_sum"), labels),
            count: metrics.sum(&format!("{family}_count"), labels),
        }
    }

    /// The observations `after` added to `self`; a bucket bound or series
    /// absent from `self` counts from zero.
    #[must_use]
    pub fn since(&self, after: &Self) -> Self {
        Self {
            buckets: deltas(&self.buckets, &after.buckets),
            sum: after.sum - self.sum,
            count: after.count - self.count,
        }
    }

    /// Bucket-bound p50/p95/p99, seconds, of these observations.
    #[must_use]
    pub fn percentiles(&self) -> Percentiles {
        let zero: Vec<(f64, f64)> = self
            .buckets
            .iter()
            .map(|(bound, _)| (*bound, 0.0))
            .collect();
        Percentiles::buckets(&zero, &self.buckets)
    }

    /// Exact mean, seconds, from `_sum` / `_count`, or `None` without
    /// observations.
    #[must_use]
    pub fn mean(&self) -> Option<f64> {
        (self.count > 0.0).then(|| self.sum / self.count)
    }
}

/// One histogram a workload judges or reports: its family and the label
/// matchers (`name="value"`) every selected series must contain.
pub type HistogramKey = (&'static str, &'static [&'static str]);

/// The server telemetry one workload's windows read.
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    /// Histograms differenced whole, each by its [`HistogramKey`].
    pub histograms: &'static [HistogramKey],
    /// Runtime component histograms, each with the label naming its
    /// components; an empty label reads a family without labels as one
    /// component.
    pub breakdown: &'static [(&'static str, &'static str)],
    /// Common family prefix dropped from breakdown component names.
    pub prefix: &'static str,
}

impl Selection {
    /// A selection reading no histogram and no component, for a workload
    /// that keeps only the window's resources.
    pub const RESOURCES: Self = Self {
        histograms: &[],
        breakdown: &[],
        prefix: "",
    };

    /// Every selected histogram in one scrape, in selection order.
    fn histograms(&self, metrics: &Metrics) -> Vec<Histogram> {
        self.histograms
            .iter()
            .map(|(family, labels)| Histogram::read(metrics, family, labels))
            .collect()
    }

    /// Every breakdown component's total seconds and observations in one
    /// scrape, keyed `family{label=value}` with [`Self::prefix`] and the
    /// `_seconds` unit dropped; a family with an empty label is the one
    /// component `family`.
    fn breakdown(&self, metrics: &Metrics) -> BTreeMap<String, (f64, f64)> {
        let mut components = BTreeMap::new();
        for (family, label) in self.breakdown {
            let name = family
                .trim_start_matches(self.prefix)
                .trim_end_matches("_seconds");
            if label.is_empty() {
                let sum = metrics.sum(&format!("{family}_sum"), &[]);
                let count = metrics.sum(&format!("{family}_count"), &[]);
                components.insert(name.to_owned(), (sum, count));
                continue;
            }
            let counts = metrics.by_label(&format!("{family}_count"), label);
            for (value, sum) in metrics.by_label(&format!("{family}_sum"), label) {
                let count = counts.get(&value).copied().unwrap_or(0.0);
                components.insert(format!("{name}{{{label}={value}}}"), (sum, count));
            }
        }
        components
    }
}

/// A selection's telemetry in one scrape, differenced against a later one.
#[derive(Debug, Clone)]
pub struct Readings {
    /// What was read.
    selection: Selection,
    /// Each selected histogram, in selection order.
    histograms: Vec<Histogram>,
    /// Each breakdown component's seconds and observations.
    breakdown: BTreeMap<String, (f64, f64)>,
}

impl Readings {
    /// Reads `selection` from one scrape.
    #[must_use]
    pub fn read(metrics: &Metrics, selection: Selection) -> Self {
        Self {
            selection,
            histograms: selection.histograms(metrics),
            breakdown: selection.breakdown(metrics),
        }
    }

    /// What `after`, a later scrape, added: every selected histogram's
    /// observations, and every breakdown component that gained an
    /// observation. A series first exported after these readings counts
    /// from zero.
    #[must_use]
    pub fn since(&self, after: &Metrics) -> Telemetry {
        Telemetry {
            selection: self.selection,
            histograms: self
                .histograms
                .iter()
                .zip(self.selection.histograms(after))
                .map(|(before, after)| before.since(&after))
                .collect(),
            breakdown: self
                .selection
                .breakdown(after)
                .into_iter()
                .map(|(key, (sum, count))| {
                    let (sum0, count0) = self.breakdown.get(&key).copied().unwrap_or_default();
                    (key, (sum - sum0, count - count0))
                })
                .filter(|(_, (_, count))| *count > 0.0)
                .collect(),
        }
    }
}

/// A selection's telemetry over one window.
#[derive(Debug, Clone)]
pub struct Telemetry {
    /// What was read.
    pub selection: Selection,
    /// Each selected histogram over the window, in selection order.
    pub histograms: Vec<Histogram>,
    /// New seconds and observations of each breakdown component, keyed
    /// `family{label=value}`.
    pub breakdown: BTreeMap<String, (f64, f64)>,
}

impl Default for Telemetry {
    /// No telemetry: an unmeasured window.
    fn default() -> Self {
        Self {
            selection: Selection::RESOURCES,
            histograms: Vec::new(),
            breakdown: BTreeMap::new(),
        }
    }
}

impl Telemetry {
    /// The window's histogram under `key`; empty when the selection did not
    /// read it.
    #[must_use]
    pub fn histogram(&self, key: HistogramKey) -> Histogram {
        self.selection
            .histograms
            .iter()
            .position(|selected| *selected == key)
            .and_then(|index| self.histograms.get(index))
            .cloned()
            .unwrap_or_default()
    }
}

/// Server and driver usage over one window.
#[derive(Debug, Default, Clone)]
pub struct Usage {
    /// Wall-clock window length.
    pub seconds: f64,
    /// Server CPU seconds.
    pub server_cpu_seconds: f64,
    /// Peak server memory over the window.
    pub peak_memory_bytes: u64,
    /// CPU seconds this benchmark process used, to show the driver was not
    /// the bottleneck.
    pub driver_cpu_seconds: f64,
    /// The workload's selected telemetry over the window.
    pub telemetry: Telemetry,
}

/// One server's resources, this process's CPU, and a selection's telemetry
/// at a window's start, closed into a [`Usage`].
///
/// Opening and closing each scrape `/metrics` once and hand that scrape back,
/// so a workload reading further series takes them from the same scrape. The
/// window's seconds end before the closing scrape; its CPU readings follow it.
pub struct Window {
    /// Window start, taken once the opening readings are done.
    started: Instant,
    /// Server CPU and peak memory since the start.
    resources: ResourceWindow,
    /// The selection at start.
    readings: Readings,
    /// This process's CPU seconds at start.
    driver_cpu_seconds: f64,
}

impl Window {
    /// Opens a window over `server` reading `selection`, and returns the
    /// opening scrape.
    ///
    /// # Errors
    ///
    /// Returns scrape or memory-peak failures.
    pub async fn open(
        server: &LocalServer,
        selection: Selection,
    ) -> Result<(Self, Metrics), Error> {
        let metrics = server.metrics().await?;
        let window = Self {
            resources: ResourceWindow::open([server])?,
            readings: Readings::read(&metrics, selection),
            driver_cpu_seconds: driver_cpu_seconds(),
            started: Instant::now(),
        };
        Ok((window, metrics))
    }

    /// Differences `server`'s resources and the selection against the open,
    /// and returns the closing scrape.
    ///
    /// # Errors
    ///
    /// Returns scrape or memory-peak failures.
    pub async fn close(self, server: &LocalServer) -> Result<(Usage, Metrics), Error> {
        let seconds = self.started.elapsed().as_secs_f64();
        let metrics = server.metrics().await?;
        let resources = self.resources.finish([server], seconds)?;
        let (server_cpu_seconds, peak_memory_bytes) = resources
            .first()
            .map_or((0.0, 0), |used| (used.cpu_seconds, used.peak_memory));
        let usage = Usage {
            seconds,
            server_cpu_seconds,
            peak_memory_bytes,
            driver_cpu_seconds: driver_cpu_seconds() - self.driver_cpu_seconds,
            telemetry: self.readings.since(&metrics),
        };
        Ok((usage, metrics))
    }
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
                replica.cpu_usec(),
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
        self.frozen = Some(replicas.into_iter().map(LocalServer::cpu_usec).collect());
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

/// CPU seconds this process has used (user plus system); zero when
/// unreadable.
pub fn driver_cpu_seconds() -> f64 {
    crate::release_server::os::self_cpu_seconds()
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
    use super::{Percentiles, Readings, Selection, deltas, quantile};
    use crate::release_server::Metrics;

    /// A two-point streaming selection over one family and a breakdown by
    /// `point`, as a gateway or verification workload names them.
    const STREAM: Selection = Selection {
        histograms: &[
            (
                "wyrd_gateway_overhead_seconds",
                &["stream=\"true\"", "point=\"answer\""],
            ),
            (
                "wyrd_gateway_overhead_seconds",
                &["stream=\"true\"", "point=\"end\""],
            ),
            ("wyrd_gateway_overhead_seconds", &["stream=\"false\""]),
        ],
        breakdown: &[
            ("wyrd_gateway_overhead_seconds", "point"),
            ("outbox_write_seconds", ""),
        ],
        prefix: "wyrd_gateway_",
    };

    /// One `wyrd_gateway_overhead_seconds` series of `stream` and `point`
    /// with cumulative bucket counts at bounds 0.001 and `+Inf`, and its sum.
    fn series(stream: &str, point: &str, fast: f64, total: f64, sum: f64) -> String {
        let labels = format!("stream=\"{stream}\",point=\"{point}\"");
        format!(
            "wyrd_gateway_overhead_seconds_bucket{{{labels},le=\"0.001\"}} {fast}\n\
             wyrd_gateway_overhead_seconds_bucket{{{labels},le=\"+Inf\"}} {total}\n\
             wyrd_gateway_overhead_seconds_sum{{{labels}}} {sum}\n\
             wyrd_gateway_overhead_seconds_count{{{labels}}} {total}\n"
        )
    }

    /// Shared readings keep each label selection independent, count a series
    /// first exported mid-window from zero, give an idle selection no
    /// percentiles, and keep only breakdown components that gained an
    /// observation.
    ///
    /// # Panics
    ///
    /// Panics when a delta, percentile, or component is wrong.
    #[test]
    fn shared_measurements_preserve_workload_deltas() {
        let before = Metrics::parse(&format!(
            "{}{}outbox_write_seconds_sum 2\noutbox_write_seconds_count 4\n",
            series("true", "answer", 10.0, 10.0, 0.005),
            series("false", "answer", 3.0, 3.0, 0.001),
        ));
        let after = Metrics::parse(&format!(
            "{}{}{}outbox_write_seconds_sum 2\noutbox_write_seconds_count 4\n",
            series("true", "answer", 1009.0, 1010.0, 0.505),
            series("true", "end", 0.0, 20.0, 0.4),
            series("false", "answer", 3.0, 3.0, 0.001),
        ));
        let telemetry = Readings::read(&before, STREAM).since(&after);

        let answer = telemetry.histogram(STREAM.histograms[0]);
        assert_eq!((answer.count, answer.sum), (1000.0, 0.5));
        assert_eq!(answer.percentiles().p50, Some(0.001));
        assert_eq!(answer.percentiles().p99, Some(0.001));
        assert_eq!(answer.mean(), Some(0.0005));

        let end = telemetry.histogram(STREAM.histograms[1]);
        assert_eq!(end.count, 20.0);
        assert_eq!(end.percentiles().p50, Some(f64::INFINITY));

        let buffered = telemetry.histogram(STREAM.histograms[2]);
        assert_eq!(buffered.count, 0.0);
        assert_eq!(buffered.percentiles().p99, None);
        assert_eq!(buffered.mean(), None);

        assert_eq!(
            telemetry
                .histogram(("wyrd_gateway_overhead_seconds", &[]))
                .count,
            0.0,
            "an unselected key reads empty"
        );
        let components: Vec<&String> = telemetry.breakdown.keys().collect();
        assert_eq!(
            components,
            ["overhead{point=answer}", "overhead{point=end}"],
            "an idle component is dropped and the prefix and unit trimmed"
        );
        let (seconds, count) = telemetry.breakdown["overhead{point=answer}"];
        assert!((seconds - 0.5).abs() < 1e-9 && (count - 1000.0).abs() < f64::EPSILON);
    }

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
