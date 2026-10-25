//! Measurement owners shared by the opt-in capacity benchmark binaries.
//!
//! Every benchmark under `src/bin/` launches the release `wyrd-server` beside
//! itself as a plain child, reads `WYRD_LOG` for client-side tracing, windows
//! the server process's CPU and sampled peak memory, and collects the server's
//! exact per-request timings. Those pieces live here once so each workload
//! owns only its production calls; [`crate::experiment`] owns judgment and
//! history.
//!
//! Judged latency is server-side only: each [`Population`] is the
//! difference of two cumulative snapshots of the server's bounded benchmark
//! histograms, which record each request's integer nanoseconds from handler
//! entry to its result, acknowledgement, or response handoff, plus the union
//! of that request's own provider or Verifier waits and the residual between
//! them. Each percentile is the nearest-rank bucket's bounds, judged whole.
//! The coarse Prometheus histograms remain only as diagnostic means and
//! component breakdowns.
//!
//! A workload names the diagnostic histograms and runtime components it reads
//! in one [`Selection`]; a [`Window`] reads them, the server's CPU and peak
//! memory, and this process's CPU at its open and differences them into a
//! [`Usage`] at its close. A reading the OS cannot supply, a counter that went
//! backwards, or a lost process is `None`, never zero use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use num_traits::ToPrimitive;
use serde::Serialize;
use serde_json::Value;
pub use wyrd_telemetry::timing::UNLABELLED;
use wyrd_telemetry::timing::{
    Distribution, IntervalScan, IntervalSeries, TimingInterval, TimingSnapshot,
};

use crate::experiment::{Reading, Reconciled, Repetition, TimingEvidence};
use crate::release_server::{LocalServer, Metrics, PeakSampler};

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
    /// Nearest-rank percentiles of exact raw `samples`, each divided by
    /// `per`, via [`nearest_rank`].
    #[must_use]
    pub fn raw(samples: &[u64], per: f64) -> Self {
        Self {
            p50: nearest_rank(samples, 0.5, per),
            p95: nearest_rank(samples, 0.95, per),
            p99: nearest_rank(samples, 0.99, per),
        }
    }
}

/// The nearest-rank percentile `q` of exact raw `samples`: the sample at
/// rank `ceil(q * N)` in sorted order, divided by `per` (1e6 for nanoseconds
/// to milliseconds; dividing keeps 500,000 ns exactly 0.5 ms). `None` without
/// samples. The samples are copied, never interpolated or bucketed.
#[must_use]
pub fn nearest_rank(samples: &[u64], q: f64, per: f64) -> Option<f64> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (q * sorted.len() as f64).ceil().to_usize().unwrap_or(0);
    let index = rank.clamp(1, sorted.len().max(1)) - 1;
    sorted.get(index).map(|sample| *sample as f64 / per)
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
    /// Server CPU seconds, or `None` when the process was lost or a reading
    /// was unavailable or reset.
    pub server_cpu_seconds: Option<f64>,
    /// Sampled peak server resident memory over the window, or `None` when
    /// it was never sampled.
    pub peak_memory_bytes: Option<u64>,
    /// CPU seconds this benchmark process used, to show the driver was not
    /// the bottleneck; `None` when unreadable.
    pub driver_cpu_seconds: Option<f64>,
    /// Seconds the requests in flight at the window's stop took to drain
    /// after it, outside `seconds` and both CPU readings.
    pub drain_seconds: f64,
    /// The workload's selected telemetry over the window and its drain.
    pub telemetry: Telemetry,
}

/// One server's resources, this process's CPU, and a selection's telemetry
/// at a window's start, closed into a [`Usage`].
///
/// Opening and closing each scrape `/metrics` and hand that scrape back,
/// so a workload reading further series takes them from the same scrape. The
/// window's seconds and both CPU readings freeze together with the start of
/// the scrape handed back and, through [`Self::measure`], before the
/// requests in flight at the window's stop drain.
pub struct Window {
    /// Window start, taken once the opening readings are done.
    started: Instant,
    /// Server CPU and peak memory since the start.
    resources: ResourceWindow,
    /// The selection at start.
    readings: Readings,
    /// This process's CPU seconds at start.
    driver_cpu_seconds: Option<f64>,
}

impl Window {
    /// Opens a window over `server` reading `selection`, and returns the
    /// opening scrape.
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    pub async fn open(
        server: &LocalServer,
        selection: Selection,
    ) -> Result<(Self, Metrics), Error> {
        let metrics = server.metrics().await?;
        let window = Self {
            resources: ResourceWindow::open([server]),
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
    /// Returns scrape failures.
    pub async fn close(self, server: &LocalServer) -> Result<(Usage, Metrics), Error> {
        let ((), usage, metrics) = self.measure(server, Instant::now(), async {}).await?;
        Ok((usage, metrics))
    }

    /// Runs `work` in the window and stops measuring at `stop`, or when
    /// `work` ends first: the window's seconds, server CPU, and driver CPU
    /// freeze there and the stop scrape starts, so the server work counters
    /// it returns share their interval; only then do the requests `work`
    /// still has in flight drain, outside all of them. `work` must admit
    /// nothing after `stop` and counts what completes after it as late.
    /// Returns `work`'s output, the usage, whose selected telemetry also
    /// covers the drain, and the stop scrape; a check of everything the
    /// window caused, drain included, scrapes again afterwards.
    ///
    /// # Errors
    ///
    /// Returns a stop or drained scrape's failure.
    pub async fn measure<T>(
        self,
        server: &LocalServer,
        stop: Instant,
        work: impl Future<Output = T>,
    ) -> Result<(T, Usage, Metrics), Error> {
        let Self {
            started,
            mut resources,
            readings,
            driver_cpu_seconds: driver_opened,
        } = self;
        let mut driver = None;
        let (output, at_stop, stopped) = until_stop(stop, work, || {
            resources.freeze([server]);
            driver = elapsed(driver_opened, driver_cpu_seconds());
            server.metrics()
        })
        .await;
        let at_stop = at_stop?;
        let drain_seconds = stopped.elapsed().as_secs_f64();
        let seconds = stopped.duration_since(started).as_secs_f64();
        let drained = server.metrics().await?;
        let resources = resources.finish([server], seconds);
        let (server_cpu_seconds, peak_memory_bytes) = resources
            .first()
            .map_or((None, None), |used| (used.cpu_seconds, used.peak_memory));
        let usage = Usage {
            seconds,
            server_cpu_seconds,
            peak_memory_bytes,
            driver_cpu_seconds: driver,
            drain_seconds,
            telemetry: readings.since(&drained),
        };
        Ok((output, usage, at_stop))
    }
}

/// Runs `work` until `stop`, or until it ends first, then calls `freeze`
/// and only after it lets `work` drain, alongside the snapshot `freeze`
/// started, which therefore excludes nothing before the stop. Returns
/// `work`'s output, the snapshot, and when `freeze` ran, the
/// measurement's end.
async fn until_stop<T, S: Future>(
    stop: Instant,
    work: impl Future<Output = T>,
    freeze: impl FnOnce() -> S,
) -> (T, S::Output, Instant) {
    let mut work = std::pin::pin!(work);
    let early = tokio::select! {
        output = &mut work => Some(output),
        () = tokio::time::sleep_until(stop.into()) => None,
    };
    let snapshot = freeze();
    let stopped = Instant::now();
    let (output, snapshot) = if let Some(output) = early {
        (output, snapshot.await)
    } else {
        tokio::join!(work, snapshot)
    };
    (output, snapshot, stopped)
}

/// One server process's resource use over a measurement window.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// CPU seconds consumed, or `None` when the process was lost or its
    /// counter was unavailable or went backwards.
    pub cpu_seconds: Option<f64>,
    /// Mean cores: CPU seconds over window seconds, when CPU was read.
    pub cores: Option<f64>,
    /// Sampled peak resident memory, bytes, or `None` when never sampled.
    pub peak_memory: Option<u64>,
}

/// Server process CPU and sampled peak-memory readings at a window's start.
///
/// A benchmark opens the window before its workload, freezes CPU when the
/// measured interval ends (before any drain), and finishes once it no longer
/// needs peak memory, so CPU covers only the measured interval while peak
/// memory covers everything after the open.
pub struct ResourceWindow {
    /// Each process's CPU microseconds at the open and its memory sampler.
    opened: Vec<(Option<u64>, Option<PeakSampler>)>,
    /// Each process's CPU microseconds once [`Self::freeze`] ran.
    frozen: Option<Vec<Option<u64>>>,
}

impl ResourceWindow {
    /// Reads every server process's CPU counter and starts sampling its
    /// resident memory.
    pub fn open<'a>(servers: impl IntoIterator<Item = &'a LocalServer>) -> Self {
        Self {
            opened: servers
                .into_iter()
                .map(|server| (server.cpu_usec(), server.memory_peak()))
                .collect(),
            frozen: None,
        }
    }

    /// Reads every server process's CPU counter at the end of the measured
    /// interval.
    ///
    /// `servers` must be the same processes, in the same order, as the open;
    /// a later call replaces the earlier reading.
    pub fn freeze<'a>(&mut self, servers: impl IntoIterator<Item = &'a LocalServer>) {
        self.frozen = Some(servers.into_iter().map(LocalServer::cpu_usec).collect());
    }

    /// Each process's resources over `window_seconds`, using the frozen CPU
    /// reading, or a fresh one from `servers` when never frozen. A missing
    /// reading at either end, or a counter that went backwards, leaves CPU
    /// `None`.
    pub fn finish<'a>(
        mut self,
        servers: impl IntoIterator<Item = &'a LocalServer>,
        window_seconds: f64,
    ) -> Vec<Resources> {
        if self.frozen.is_none() {
            self.freeze(servers);
        }
        let frozen = self.frozen.unwrap_or_default();
        self.opened
            .into_iter()
            .zip(frozen)
            .map(|((before, sampler), after)| {
                let cpu_seconds = match (before, after) {
                    (Some(before), Some(after)) if after >= before => {
                        Some((after - before) as f64 / 1e6)
                    }
                    _ => None,
                };
                Resources {
                    cpu_seconds,
                    cores: cpu_seconds
                        .filter(|_| window_seconds > 0.0)
                        .map(|cpu| cpu / window_seconds),
                    peak_memory: sampler.and_then(|sampler| sampler.finish().ok()),
                }
            })
            .collect()
    }
}

/// CPU seconds this process has used (user plus system), or `None` when
/// unreadable.
pub fn driver_cpu_seconds() -> Option<f64> {
    crate::release_server::os::self_cpu_seconds()
}

/// The growth of a cumulative CPU-microsecond counter, in seconds, or
/// `None` when either end is missing or the counter went backwards.
#[must_use]
pub fn usec_elapsed(before: Option<u64>, after: Option<u64>) -> Option<f64> {
    match (before, after) {
        (Some(before), Some(after)) if after >= before => Some((after - before) as f64 / 1e6),
        _ => None,
    }
}

/// The growth of a cumulative CPU reading, or `None` when either end is
/// missing or the counter went backwards.
#[must_use]
pub fn elapsed(before: Option<f64>, after: Option<f64>) -> Option<f64> {
    match (before, after) {
        (Some(before), Some(after)) if after >= before => Some(after - before),
        _ => None,
    }
}

/// Nanoseconds per millisecond, the divisor from raw timings to report
/// milliseconds.
pub const NANOS_PER_MS: f64 = 1e6;

/// The public query's server timing boundary, whose queries also record
/// executed-plan scan facts.
pub const QUERY_BOUNDARY: &str = "bifrost.query";

/// How long a window's timing observations may take to land in the server's
/// histograms after its last request ended: each records at its boundary,
/// around the response handoff, so the wait covers only that race.
const COLLECTION_WAIT: Duration = Duration::from_secs(30);

/// The timing method a record declares: the server's bounded histograms,
/// read as cumulative snapshots from the loopback metrics listener and
/// differenced per window.
#[must_use]
pub fn timing_declaration() -> Value {
    let method = wyrd_telemetry::timing::Method::current();
    serde_json::json!({
        "method": "server benchmark histograms: integer nanoseconds from the owning server boundary's std::time::Instant timers, recorded into bounded HDR histograms and read as cumulative snapshots from the loopback metrics listener; each window is the bucket-count difference of two snapshots of one process",
        "clock": method.clock,
        "precision": format!("{} significant digits; each percentile is judged on its whole bucket's bounds", method.significant_digits),
        "range_ns": [method.lowest_ns, method.highest_ns],
        "scan_range": [1, method.highest_scan],
        "max_series": method.max_series,
        "max_scan_series": method.max_scan_series,
        "percentile": "nearest rank ceil(q * N) over the window's distribution",
        "server_env": { "WYRD_METRICS_BENCHMARK_SNAPSHOT": "1" },
        "tracing": "operator span export stays at its default (disabled); no span is required for timing",
        "assignment": "every request carries its closed case and the repetition or admission phase that admitted it, activated once its caller authenticated; a window takes the observations recorded between its opening snapshot and one taken after its admitted cohort drained, so a late completion stays in its admitted cohort; each cohort's judged-outcome count must equal the completions the driver saw, other outcomes (retried or refused attempts) are counted beside it; a short or extra count, a dropped or out-of-range observation, an unknown label, a missing or negative component, scans that are not the answered cohort's, or a replaced process makes it invalid",
        "measurement": "work, wall seconds, and server and driver CPU stop at the admission stop, before the requests then in flight drain; their completions count as late work beside the measured work, and their latency stays in the final cohort",
        "evidence": "each window's full opening and drained snapshots are kept under the run's target artifacts as timings/<step>/<phase>.json; the record keeps each cohort's reconciliation and each percentile's rank and bucket bounds",
    })
}

/// One cohort a window owes server timings for: the requests of `case`
/// admitted in `phase` that the driver saw complete with `outcome` at
/// `boundary`, `count` of them. Attempts with any other outcome, such as a
/// retried failure or a refusal, are counted beside it, never owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected<'a> {
    /// The server timing boundary.
    pub boundary: &'a str,
    /// The judged outcome the completions record.
    pub outcome: &'a str,
    /// The requests' closed case, or [`UNLABELLED`].
    pub case: &'a str,
    /// The closed phase that admitted them, or [`UNLABELLED`].
    pub phase: &'a str,
    /// Completions the driver saw.
    pub count: u64,
}

/// One window's server timing observations: the series and scan facts
/// recorded between two snapshots of one server process, and the
/// snapshots and reconciliation they were read from.
#[derive(Debug, Clone, Default)]
pub struct Population {
    /// Every timing series with an observation in the window.
    pub series: Vec<IntervalSeries>,
    /// Every scan series with a query in the window.
    pub scans: Vec<IntervalScan>,
    /// Why the population is incomplete or untrustworthy; empty when it is
    /// complete.
    pub invalid: Vec<String>,
    /// The snapshots the window differences, once read.
    pub snapshots: Option<(TimingSnapshot, TimingSnapshot)>,
    /// Each owed cohort's expected and recorded observations.
    pub reconciliation: Vec<Reconciled>,
}

impl Population {
    /// Waits up to [`COLLECTION_WAIT`] until `server` recorded every
    /// `expected` cohort since `before`, then takes the window; a cohort
    /// recorded short or over is invalid.
    pub async fn collect(
        server: &LocalServer,
        before: &TimingSnapshot,
        expected: &[Expected<'_>],
    ) -> Self {
        Self::wait(server, before, expected, COLLECTION_WAIT).await
    }

    /// Polls `server`'s snapshot until the counts are reached or `wait`
    /// expires, then validates the last interval.
    async fn wait(
        server: &LocalServer,
        before: &TimingSnapshot,
        expected: &[Expected<'_>],
        wait: Duration,
    ) -> Self {
        let waiting = Instant::now();
        loop {
            let mut population = match server.timings().await {
                Ok(after) => match Self::between(before, after) {
                    Ok(population) => population,
                    Err(reason) => return Self::refused(reason),
                },
                Err(error) => {
                    return Self::refused(format!("the timing snapshot is unreadable: {error}"));
                }
            };
            let reached = expected
                .iter()
                .all(|cohort| population.recorded(cohort) >= cohort.count);
            if reached || waiting.elapsed() >= wait {
                population.expect(expected);
                return population;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// The population of `after` since `before`, keeping both snapshots.
    ///
    /// # Errors
    ///
    /// Returns why the snapshots do not difference: a replaced process or a
    /// reset.
    pub fn between(before: &TimingSnapshot, after: TimingSnapshot) -> Result<Self, String> {
        let mut population = Self::from_interval(after.since(before)?);
        population.snapshots = Some((before.clone(), after));
        Ok(population)
    }

    /// Reconciles each `expected` cohort: one recorded short or over makes
    /// the population invalid.
    pub fn expect(&mut self, expected: &[Expected<'_>]) {
        for cohort in expected {
            let recorded = self.recorded(cohort);
            if recorded != cohort.count {
                self.invalid.push(format!(
                    "{recorded} `{}` timing observations of case {} in phase {} have outcome `{}` for {} completed requests",
                    cohort.boundary, cohort.case, cohort.phase, cohort.outcome, cohort.count
                ));
            }
            self.reconciliation.push(Reconciled {
                boundary: cohort.boundary.to_owned(),
                outcome: cohort.outcome.to_owned(),
                case: cohort.case.to_owned(),
                phase: cohort.phase.to_owned(),
                expected: cohort.count,
                recorded,
            });
        }
    }

    /// Observations recorded for `cohort`'s boundary, outcome, case, and
    /// phase.
    #[must_use]
    pub fn recorded(&self, cohort: &Expected<'_>) -> u64 {
        self.series
            .iter()
            .filter(|series| {
                series.boundary == cohort.boundary
                    && series.outcome == cohort.outcome
                    && series.case == cohort.case
                    && series.phase == cohort.phase
            })
            .map(|series| series.count)
            .sum()
    }

    /// The population of `interval`, invalid for every dropped,
    /// unlabelled-by-mistake, or inconsistent observation in it.
    #[must_use]
    pub fn from_interval(interval: TimingInterval) -> Self {
        let mut invalid = Vec::new();
        if interval.dropped > 0 {
            invalid.push(format!(
                "{} timing observations overflowed the server's series bound and were dropped",
                interval.dropped
            ));
        }
        if interval.invalid_labels > 0 {
            invalid.push(format!(
                "{} requests declared an unknown benchmark case or phase",
                interval.invalid_labels
            ));
        }
        for series in &interval.series {
            invalid.extend(series.problems());
        }
        for scan in &interval.scans {
            if scan.out_of_range > 0 {
                invalid.push(format!(
                    "{} scan facts of case {} are outside the trackable range",
                    scan.out_of_range, scan.case
                ));
            }
        }
        Self {
            series: interval.series,
            scans: interval.scans,
            invalid,
            ..Self::default()
        }
    }

    /// A population with no observation, invalid for `reason`.
    fn refused(reason: String) -> Self {
        Self {
            invalid: vec![reason],
            ..Self::default()
        }
    }

    /// The observations of `case` admitted in `phase` alone, keeping every
    /// invalid reason but not the window's snapshots; an L2 window judges
    /// each class's admitted cohort this way.
    #[must_use]
    pub fn only(&self, case: &str, phase: &str) -> Self {
        Self {
            series: self
                .series
                .iter()
                .filter(|series| series.case == case && series.phase == phase)
                .cloned()
                .collect(),
            scans: self
                .scans
                .iter()
                .filter(|scan| scan.case == case && scan.phase == phase)
                .cloned()
                .collect(),
            invalid: self.invalid.clone(),
            ..Self::default()
        }
    }

    /// Observations of `boundary` of any outcome, case, and phase.
    #[must_use]
    pub fn count(&self, boundary: &str) -> u64 {
        self.series
            .iter()
            .filter(|series| series.boundary == boundary)
            .map(|series| series.count)
            .sum()
    }

    /// The elapsed distribution of every `boundary` observation with
    /// `outcome`; `None` without one.
    #[must_use]
    pub fn elapsed(&self, boundary: &str, outcome: &str) -> Option<Distribution> {
        merged(
            self.of(boundary, outcome)
                .map(|series| Some(&series.elapsed)),
        )
    }

    /// The excluded-wait distribution of every `boundary` observation with
    /// `outcome`; `None` without one, or when any series has none.
    #[must_use]
    pub fn excluded(&self, boundary: &str, outcome: &str) -> Option<Distribution> {
        merged(
            self.of(boundary, outcome)
                .map(|series| series.excluded.as_ref()),
        )
    }

    /// The residual distribution, elapsed less excluded within each
    /// request, of every `boundary` observation with `outcome`.
    ///
    /// # Errors
    ///
    /// Returns why there is no residual: no observation, or a series
    /// without an excluded component.
    pub fn residuals(&self, boundary: &str, outcome: &str) -> Result<Distribution, String> {
        if self.of(boundary, outcome).next().is_none() {
            return Err(format!(
                "no `{boundary}` timing observation has outcome `{outcome}`"
            ));
        }
        merged(
            self.of(boundary, outcome)
                .map(|series| series.residual.as_ref()),
        )
        .ok_or_else(|| {
            format!("no `{boundary}` timing observation with outcome `{outcome}` has a residual")
        })
    }

    /// Observations of `boundary` whose outcome is not `outcome`.
    #[must_use]
    pub fn others(&self, boundary: &str, outcome: &str) -> u64 {
        self.series
            .iter()
            .filter(|series| series.boundary == boundary && series.outcome != outcome)
            .map(|series| series.count)
            .sum()
    }

    /// The collection-health line a repetition keeps as an observation.
    #[must_use]
    pub fn health(&self) -> String {
        format!(
            "server timing: {} observations in {} series, {} scanned queries",
            self.series.iter().map(|series| series.count).sum::<u64>(),
            self.series.len(),
            self.scans.iter().map(|scan| scan.count).sum::<u64>(),
        )
    }

    /// Keeps this population's invalid reasons, collection health, and
    /// reconciliation on `repetition`, and its snapshots as the evidence of
    /// the repetition's `window`; call once per window.
    pub fn validate(&self, repetition: &mut Repetition, window: &str) {
        repetition.invalid.extend(self.invalid.iter().cloned());
        repetition.observations.push(self.health());
        repetition
            .reconciliation
            .extend(self.reconciliation.iter().cloned());
        if let Some((before, after)) = &self.snapshots {
            repetition.timings.push(TimingEvidence {
                window: window.to_owned(),
                before: before.clone(),
                after: after.clone(),
                reconciliation: self.reconciliation.clone(),
            });
        }
    }

    /// Sets each `(id, q)` of `ids` on `repetition` to the nearest-rank
    /// percentile q, in milliseconds, of the `boundary` observations with
    /// the judged `outcome`: their residuals when `residual`, else their
    /// elapsed times, each with its rank and bucket bounds. The last id with
    /// its `pNN` segment read `max` also gets the largest observation's
    /// bucket. Other outcomes are counted by [`Self::others`], never judged.
    /// An empty judged population or an observation without a residual
    /// makes the repetition invalid.
    pub fn percentiles(
        &self,
        repetition: &mut Repetition,
        boundary: &str,
        outcome: &str,
        residual: bool,
        ids: &[(&str, f64)],
    ) {
        let distribution = if residual {
            self.residuals(boundary, outcome)
        } else {
            self.elapsed(boundary, outcome).ok_or_else(|| {
                format!("no `{boundary}` timing observation has outcome `{outcome}`")
            })
        }
        .map_err(|reason| repetition.invalid.push(reason))
        .ok();
        let maximum = ids.last().map(|(id, _)| (maximum_id(id), 1.0));
        for (id, q) in ids
            .iter()
            .map(|(id, q)| ((*id).to_owned(), *q))
            .chain(maximum)
        {
            repetition.set(
                &id,
                distribution.as_ref().map_or_else(
                    || Reading::missing("no successful sample"),
                    |distribution| Reading::quantile(distribution, q, NANOS_PER_MS),
                ),
            );
        }
    }

    /// The mean executed-plan scanned rows and bytes per query over the
    /// scan series with `outcome`, as bounds of each bucketed total; each is
    /// missing, never zero, without a query, when any query lacks the fact,
    /// or when the scanned queries are not exactly the population's
    /// [`QUERY_BOUNDARY`] timings with `outcome`, the same cohort.
    #[must_use]
    pub fn scanned_per_query(&self, outcome: &str) -> [Reading; 2] {
        let scans: Vec<&IntervalScan> = self
            .scans
            .iter()
            .filter(|scan| scan.outcome == outcome)
            .collect();
        let queries: u64 = scans.iter().map(|scan| scan.count).sum();
        let timed: u64 = self
            .of(QUERY_BOUNDARY, outcome)
            .map(|series| series.count)
            .sum();
        let mean = |fact: &str,
                    missing: fn(&IntervalScan) -> u64,
                    pick: fn(&IntervalScan) -> &Distribution| {
            if queries == 0 {
                return Reading::missing("no query was answered");
            }
            if queries != timed {
                return Reading::missing(format!(
                    "{queries} queries reported scans for {timed} `{QUERY_BOUNDARY}` timings"
                ));
            }
            if scans.iter().any(|scan| missing(scan) > 0) {
                return Reading::missing(format!(
                    "an answered query has no executed-plan scanned {fact}"
                ));
            }
            let (low, high) = scans.iter().fold((0_u64, 0_u64), |(low, high), scan| {
                let (lo, hi) = pick(scan).total_bounds();
                (low.saturating_add(lo), high.saturating_add(hi))
            });
            Reading::bounded(low as f64, high as f64, queries as f64).with_samples(queries)
        };
        [
            mean("rows", |scan| scan.missing_rows, |scan| &scan.rows),
            mean("bytes", |scan| scan.missing_bytes, |scan| &scan.bytes),
        ]
    }

    /// Series of `boundary` with `outcome`.
    fn of<'a>(
        &'a self,
        boundary: &'a str,
        outcome: &'a str,
    ) -> impl Iterator<Item = &'a IntervalSeries> {
        self.series
            .iter()
            .filter(move |series| series.boundary == boundary && series.outcome == outcome)
    }
}

/// `id` with its percentile segment, such as `p99`, read `max`:
/// `overhead_p99_ms` names `overhead_max_ms`.
fn maximum_id(id: &str) -> String {
    id.split('_')
        .map(|part| {
            let digits = part.strip_prefix('p').unwrap_or_default();
            if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                "max"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

/// The merge of `parts`; `None` when there is none or any part is `None`.
fn merged<'a>(mut parts: impl Iterator<Item = Option<&'a Distribution>>) -> Option<Distribution> {
    let mut total = parts.next()??.clone();
    for part in parts {
        total.add(part?).ok()?;
    }
    Some(total)
}

/// The smallest nonzero step this host's monotonic clock shows between two
/// successive reads, nanoseconds: its effective timer resolution, which
/// the server's timers share on this host.
#[must_use]
pub fn timer_resolution_ns() -> u64 {
    (0..10_000)
        .filter_map(|_| {
            let start = Instant::now();
            loop {
                let step = start.elapsed();
                if !step.is_zero() {
                    break u64::try_from(step.as_nanos()).ok();
                }
            }
        })
        .min()
        .unwrap_or(0)
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

/// The identity of the measured server `binary` a record keeps: its file
/// name, size, and modification time. The directory stays out, since it names
/// the contributor's checkout.
///
/// # Errors
///
/// Returns the metadata failure.
pub fn binary_identity(binary: &Path) -> Result<Value, Error> {
    let metadata = std::fs::metadata(binary)?;
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    Ok(serde_json::json!({
        "name": binary.file_name().map(|name| name.to_string_lossy().into_owned()),
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
    use super::{
        Distribution, Expected, IntervalScan, IntervalSeries, Percentiles, Population, Readings,
        Selection, TimingInterval, UNLABELLED, deltas, elapsed, until_stop,
    };
    use crate::release_server::Metrics;

    /// Work still in flight at the stop drains only after the measurement
    /// froze there, and work that ends first freezes it at its end. The
    /// stop snapshot holds the work done before the stop and none of the
    /// bytes a drained write stages after it, which the final counter still
    /// counts exactly once.
    ///
    /// # Panics
    ///
    /// Panics when the freeze follows the drain or misses the stop, or the
    /// snapshot counts drained work.
    #[tokio::test]
    async fn the_measurement_freezes_at_the_stop_before_work_drains() {
        const INSERTED: &str = "bifrost_scribe_memtable_bytes_inserted_total";
        let inserted = std::sync::atomic::AtomicU64::new(0);
        let released = tokio::sync::Notify::new();
        let stop = std::time::Instant::now() + std::time::Duration::from_millis(20);
        let (drained, at_stop, stopped) = until_stop(
            stop,
            async {
                inserted.fetch_add(10, std::sync::atomic::Ordering::SeqCst);
                released.notified().await;
                inserted.fetch_add(5, std::sync::atomic::Ordering::SeqCst);
                std::time::Instant::now()
            },
            || {
                released.notify_one();
                let scrape = format!(
                    "{INSERTED} {}",
                    inserted.load(std::sync::atomic::Ordering::SeqCst)
                );
                std::future::ready(Metrics::parse(&scrape))
            },
        )
        .await;
        assert!(stopped >= stop && drained >= stopped);
        assert_eq!(at_stop.sum(INSERTED, &[]), 10.0);
        assert_eq!(inserted.load(std::sync::atomic::Ordering::SeqCst), 15);
        let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let ((), (), early) = until_stop(later, async {}, || std::future::ready(())).await;
        assert!(early < later);
    }

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
    /// first exported mid-window from zero, give an idle selection no mean,
    /// and keep only breakdown components that gained an observation.
    ///
    /// # Panics
    ///
    /// Panics when a delta, mean, or component is wrong.
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
        assert_eq!(answer.buckets, [(0.001, 999.0), (f64::INFINITY, 1000.0)]);
        assert_eq!(answer.mean(), Some(0.0005));

        let end = telemetry.histogram(STREAM.histograms[1]);
        assert_eq!(end.count, 20.0);
        assert_eq!(end.buckets, [(0.001, 0.0), (f64::INFINITY, 20.0)]);
        assert_eq!(end.mean(), Some(0.02));

        let buffered = telemetry.histogram(STREAM.histograms[2]);
        assert_eq!(buffered.count, 0.0);
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

    /// A missing or reset cumulative reading has no growth, never zero use.
    ///
    /// # Panics
    ///
    /// Panics when a missing or reset reading yields a value.
    #[test]
    fn missing_or_reset_readings_have_no_growth() {
        assert_eq!(elapsed(Some(1.0), Some(3.5)), Some(2.5));
        assert_eq!(elapsed(Some(2.0), Some(2.0)), Some(0.0));
        assert_eq!(elapsed(None, Some(2.0)), None);
        assert_eq!(elapsed(Some(2.0), None), None);
        assert_eq!(elapsed(Some(3.0), Some(1.0)), None, "a reset counter");
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

    /// A snapshot window is invalid for every dropped, mislabelled, or
    /// inconsistent observation and for a short or extra count; one case
    /// and phase never satisfies another's count; scanned facts average as
    /// bounds of their bucketed totals and a query without one is missing,
    /// never zero.
    ///
    /// # Panics
    ///
    /// Panics when a reason, count, or reading is wrong.
    #[test]
    fn snapshot_windows_keep_classes_and_scans_apart() {
        let durations =
            |samples: &[u64]| Distribution::of_durations(samples).expect("in-range samples");
        let series = |case: &str, phase: &str, samples: &[u64], out_of_range| IntervalSeries {
            boundary: "bifrost.query".to_owned(),
            outcome: "success".to_owned(),
            case: case.to_owned(),
            phase: phase.to_owned(),
            count: samples.len() as u64 + out_of_range,
            elapsed: durations(samples),
            excluded: None,
            residual: None,
            missing_excluded: 0,
            out_of_range,
            negative_residual: 0,
        };
        let scan = |rows: &[u64], missing_rows| IntervalScan {
            outcome: "success".to_owned(),
            case: "point-lookup".to_owned(),
            phase: "steady".to_owned(),
            count: rows.len() as u64 + missing_rows,
            rows: durations(rows),
            bytes: durations(&rows.iter().map(|rows| rows * 2).collect::<Vec<_>>()),
            missing_rows,
            missing_bytes: 0,
            out_of_range: 0,
        };
        let interval = |series, scans, dropped, invalid_labels| TimingInterval {
            process: "p".to_owned(),
            nanos: 1,
            series,
            scans,
            dropped,
            invalid_labels,
        };
        let mut mixed = Population::from_interval(interval(
            vec![
                series("point-lookup", "steady", &[3, 5], 0),
                series("full-scan", "steady", &[7], 0),
                series("point-lookup", "overload", &[9], 0),
            ],
            vec![scan(&[3, 5], 0)],
            0,
            0,
        ));
        assert!(mixed.invalid.is_empty(), "{:?}", mixed.invalid);
        let point = mixed.only("point-lookup", "steady");
        let cohort = |case, phase, count| Expected {
            boundary: "bifrost.query",
            outcome: "success",
            case,
            phase,
            count,
        };
        assert_eq!(point.count("bifrost.query"), 2);
        assert_eq!(point.recorded(&cohort("full-scan", "steady", 0)), 0);
        let mut handed = point.clone();
        handed.series[0].count = 3;
        let [unmatched, _] = handed.scanned_per_query("success");
        assert_eq!(
            unmatched.reason.as_deref(),
            Some("2 queries reported scans for 3 `bifrost.query` timings"),
            "scans never average over another cohort's timings"
        );
        let [rows, bytes] = point.scanned_per_query("success");
        assert_eq!(
            (rows.value, rows.above, bytes.value),
            (Some(4.0), None, Some(8.0))
        );
        mixed.expect(&[
            cohort("point-lookup", "steady", 2),
            cohort("full-scan", "steady", 2),
        ]);
        assert_eq!(
            mixed.invalid,
            [
                "1 `bifrost.query` timing observations of case full-scan in phase steady have outcome `success` for 2 completed requests"
            ],
            "neither another class nor another phase satisfies a cohort's count"
        );
        assert_eq!(mixed.reconciliation[1].recorded, 1);

        let broken = Population::from_interval(interval(
            vec![series(UNLABELLED, UNLABELLED, &[1], 1)],
            vec![scan(&[3], 1)],
            2,
            3,
        ));
        assert_eq!(
            broken.invalid,
            [
                "2 timing observations overflowed the server's series bound and were dropped",
                "3 requests declared an unknown benchmark case or phase",
                "`bifrost.query` (success, case none, phase none) has 1 durations outside the trackable range",
                "`bifrost.query` (success, case none, phase none) recorded 1 elapsed observations for 2 requests",
            ]
        );
        assert_eq!(
            broken.scanned_per_query("success")[0].value,
            None,
            "a query without its scan fact is missing, never zero"
        );
    }
}
