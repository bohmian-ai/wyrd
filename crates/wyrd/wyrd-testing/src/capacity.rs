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
//! Diagnostic histograms, counters, and gauges come from the same benchmark
//! snapshots; a benchmarked server runs no Prometheus recorder.
//!
//! A [`Window`] reads the server's CPU and peak memory and this process's
//! CPU at its open and differences them into a [`Usage`] at its close. A
//! reading the OS cannot supply, a counter that went backwards, or a lost
//! process is `None`, never zero use.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use num_traits::ToPrimitive;
use serde::Serialize;
use serde_json::Value;
pub use wyrd_benchmark::UNLABELLED;
use wyrd_benchmark::{
    BenchmarkInterval, BenchmarkSnapshot, Distribution, IntervalMetric, IntervalScan, IntervalSeries,
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
    /// Server resident memory as the window opened, or `None` when it was
    /// never sampled.
    pub opening_memory_bytes: Option<u64>,
    /// CPU seconds this benchmark process used, to show the driver was not
    /// the bottleneck; `None` when unreadable.
    pub driver_cpu_seconds: Option<f64>,
    /// Seconds the requests in flight at the window's stop took to drain
    /// after it, outside `seconds` and both CPU readings.
    pub drain_seconds: f64,
}

/// One server's resources and this process's CPU at a window's start,
/// closed into a [`Usage`].
///
/// Opening and closing each read the benchmark snapshot as [`Metrics`] and
/// hand that read back, so a workload reading further series takes them
/// from the same scrape. The
/// window's seconds and both CPU readings freeze together with the start of
/// the scrape handed back and, through [`Self::measure`], before the
/// requests in flight at the window's stop drain.
pub struct Window {
    /// Window start, taken once the opening readings are done.
    started: Instant,
    /// Server CPU and peak memory since the start.
    resources: ResourceWindow,
    /// This process's CPU seconds at start.
    driver_cpu_seconds: Option<f64>,
}

impl Window {
    /// Opens a resource window over `server`, and returns the opening
    /// scrape.
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    pub async fn open(server: &LocalServer) -> Result<(Self, Metrics), Error> {
        let metrics = server.metrics().await?;
        let window = Self {
            resources: ResourceWindow::open([server]),
            driver_cpu_seconds: driver_cpu_seconds(),
            started: Instant::now(),
        };
        Ok((window, metrics))
    }

    /// Differences `server`'s resources against the open, and returns the
    /// closing scrape.
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
    /// Returns `work`'s output, the usage, and the stop scrape; a check of
    /// everything the window caused, drain included, scrapes again
    /// afterwards.
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
        let resources = resources.finish([server], seconds);
        let used = resources.first();
        let usage = Usage {
            seconds,
            server_cpu_seconds: used.and_then(|used| used.cpu_seconds),
            peak_memory_bytes: used.and_then(|used| used.peak_memory),
            opening_memory_bytes: used.and_then(|used| used.opening_memory),
            driver_cpu_seconds: driver,
            drain_seconds,
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
    /// Resident memory as the window opened, bytes, or `None` when never
    /// sampled.
    pub opening_memory: Option<u64>,
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
                let memory = sampler.and_then(|sampler| sampler.finish().ok());
                Resources {
                    cpu_seconds,
                    cores: cpu_seconds
                        .filter(|_| window_seconds > 0.0)
                        .map(|cpu| cpu / window_seconds),
                    peak_memory: memory.map(|memory| memory.peak),
                    opening_memory: memory.map(|memory| memory.opening),
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
    let method = wyrd_benchmark::Method::current();
    serde_json::json!({
        "recorder": "wyrd-benchmark is the server's only metrics recorder: counters, gauges, and diagnostic histograms are read from the same snapshots as the timings, with no Prometheus recorder or exporter; a duration with both a timing and a production histogram is recorded only as the timing",
        "method": "server benchmark histograms: integer nanoseconds from the owning server boundary's std::time::Instant timers, recorded into bounded HDR histograms and read as cumulative snapshots from the loopback metrics listener; each window is the bucket-count difference of two snapshots of one process",
        "clock": method.clock,
        "precision": format!("{} significant digits; each percentile is judged on its whole bucket's bounds", method.significant_digits),
        "range_ns": [method.lowest_ns, method.highest_ns],
        "scan_range": [1, method.highest_scan],
        "max_series": method.max_series,
        "max_scan_series": method.max_scan_series,
        "max_metric_series": method.max_metric_series,
        "percentile": "nearest rank ceil(q * N) over the window's distribution",
        "server_env": { "WYRD_METRICS_BENCHMARK_SNAPSHOT": "1" },
        "tracing": "operator span export stays at its default (disabled); no span is required for timing",
        "assignment": "every request carries its closed case and the repetition or admission phase that admitted it, attached when its request span opens; a window takes the observations recorded between its opening snapshot and one taken after its admitted cohort drained, so a late completion stays in its admitted cohort; each cohort's judged-outcome count must equal the completions the driver saw, other outcomes (retried or refused attempts) are counted beside it; a short or extra count, a dropped or out-of-range observation, an unknown label, a missing or negative component, scans that are not the answered cohort's, or a replaced process makes it invalid",
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
    /// Every standard metric series the window's later snapshot holds.
    pub metrics: Vec<IntervalMetric>,
    /// Why the population is incomplete or untrustworthy; empty when it is
    /// complete.
    pub invalid: Vec<String>,
    /// The snapshots the window differences, once read.
    pub snapshots: Option<(BenchmarkSnapshot, BenchmarkSnapshot)>,
    /// Each owed cohort's expected and recorded observations.
    pub reconciliation: Vec<Reconciled>,
}

impl Population {
    /// Waits up to [`COLLECTION_WAIT`] until `server` recorded every
    /// `expected` cohort since `before`, then takes the window; a cohort
    /// recorded short or over is invalid.
    pub async fn collect(
        server: &LocalServer,
        before: &BenchmarkSnapshot,
        expected: &[Expected<'_>],
    ) -> Self {
        Self::wait(server, before, expected, COLLECTION_WAIT).await
    }

    /// Polls `server`'s snapshot until the counts are reached or `wait`
    /// expires, then validates the last interval.
    async fn wait(
        server: &LocalServer,
        before: &BenchmarkSnapshot,
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
    pub fn between(before: &BenchmarkSnapshot, after: BenchmarkSnapshot) -> Result<Self, String> {
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
    pub fn from_interval(interval: BenchmarkInterval) -> Self {
        let mut invalid = Vec::new();
        if interval.dropped > 0 {
            invalid.push(format!(
                "{} observations overflowed the server's series bounds and were dropped",
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
        for metric in &interval.metrics {
            if metric.rejected > 0 {
                invalid.push(format!(
                    "metric {} refused {} unrepresentable updates",
                    metric.name, metric.rejected
                ));
            }
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
            metrics: interval.metrics,
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

    /// [`IntervalMetric::counter`] over the window's metrics: `None` when
    /// the counter was never registered.
    #[must_use]
    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> Option<u64> {
        IntervalMetric::counter(&self.metrics, name, labels)
    }

    /// [`IntervalMetric::histogram`] over the window's metrics.
    ///
    /// # Errors
    ///
    /// Returns why the selection is not evidence.
    pub fn histogram(&self, name: &str, labels: &[(&str, &str)]) -> Result<Distribution, String> {
        IntervalMetric::histogram(&self.metrics, name, labels)
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

/// The release `wyrd-server` built beside the running benchmark binary, or
/// one directory up from a test binary in Cargo's `deps/`.
///
/// # Errors
///
/// Returns an error when the current executable cannot be located or the
/// server has not been built beside it.
pub fn release_binary() -> Result<PathBuf, Error> {
    let executable = std::env::current_exe()?;
    let beside = executable.with_file_name("wyrd-server");
    executable
        .ancestors()
        .skip(1)
        .take(2)
        .map(|directory| directory.join("wyrd-server"))
        .find(|binary| binary.is_file())
        .ok_or_else(|| format!("{} is not built; run through mise", beside.display()).into())
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
        BenchmarkInterval, Distribution, Expected, IntervalMetric, IntervalScan, IntervalSeries,
        Percentiles, Population, UNLABELLED, elapsed, until_stop,
    };
    use crate::release_server::Metrics;
    use wyrd_benchmark::IntervalValue;

    /// Controlled delays, waits, and CPU calibrate the shared measurement
    /// path against references measured independently of it. Each request
    /// times itself as a server boundary does, around a real sleep of a
    /// declared length, and records through the production recorder; the
    /// judged percentiles come back through the snapshot, interval, and
    /// [`Population`] the engines read. A sleep never ends early, so the
    /// declared length bounds each time from below, and the test's own
    /// outer clock, read around the whole request, bounds it from above;
    /// the judged bucket must overlap that range, and 3 significant digits
    /// keep it within 0.1% of its value. The residual of a request that
    /// excludes a known wait must lie within its own work's declared and
    /// outer bounds, never at zero. Server CPU is read by the same per-PID
    /// reader over this process: one thread computing for a known wall time
    /// uses at most that wall time plus two 10 ms accounting ticks and, on
    /// a host that preempts it, still at least half of it; sleeping uses
    /// at most two ticks.
    ///
    /// # Panics
    ///
    /// Panics when a judged time falls outside its independent reference or
    /// the CPU reader disagrees with the known work beyond its tolerance.
    #[test]
    fn known_delay_overhead_and_cpu_calibrate_the_measurement_path() {
        use std::time::{Duration, Instant};

        use crate::experiment::Reading;
        use crate::release_server::os::process_cpu_usec;

        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let request = tracing::Span::none();
        let wait = Duration::from_millis(4);
        let work = Duration::from_millis(1);
        let before = wyrd_benchmark::snapshot();
        let (mut outer_elapsed, mut outer_own) = (Duration::ZERO, Duration::ZERO);
        for _ in 0..20 {
            let outer = Instant::now();
            let started = Instant::now();
            let waited = Instant::now();
            std::thread::sleep(wait);
            let excluded = waited.elapsed();
            std::thread::sleep(work);
            wyrd_benchmark::record_timing(
                &request,
                "verification.direct",
                "completed",
                started.elapsed(),
                Some(excluded),
            );
            outer_elapsed = outer_elapsed.max(outer.elapsed());
            outer_own = outer_own.max(outer.elapsed().saturating_sub(excluded));
        }
        let interval = wyrd_benchmark::snapshot()
            .since(&before)
            .expect("one process");
        let mut population = Population::from_interval(interval);
        population.expect(&[Expected {
            boundary: "verification.direct",
            outcome: "completed",
            case: UNLABELLED,
            phase: UNLABELLED,
            count: 20,
        }]);
        let mut repetition = crate::experiment::Repetition::default();
        population.validate(&mut repetition, "measured");
        population.percentiles(
            &mut repetition,
            "verification.direct",
            "completed",
            false,
            &[("p50_ms", 0.5)],
        );
        population.percentiles(
            &mut repetition,
            "verification.direct",
            "completed",
            true,
            &[("overhead_p50_ms", 0.5)],
        );
        assert!(repetition.invalid.is_empty(), "{:?}", repetition.invalid);
        let within = |reading: &Reading, low: Duration, high: Duration| {
            let (Some(above), Some(value)) = (reading.above, reading.value) else {
                panic!("a bucketed reading: {reading:?}");
            };
            let ms = |duration: Duration| duration.as_secs_f64() * 1e3;
            assert!(
                above < ms(high) && value >= ms(low),
                "bucket ({above}, {value}] ms misses the reference [{}, {}] ms",
                ms(low),
                ms(high)
            );
            assert!(value - above <= value * 1e-3, "3 significant digits");
        };
        within(&repetition.reading("p50_ms"), wait + work, outer_elapsed);
        within(&repetition.reading("overhead_p50_ms"), work, outer_own);

        let pid = std::process::id();
        let cpu = || process_cpu_usec(pid).expect("this process's CPU");
        let tick = 20_000;
        let idle = cpu();
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            cpu() - idle <= tick,
            "sleeping uses no CPU beyond accounting ticks"
        );
        let busy = cpu();
        let started = Instant::now();
        let mut state = 0_u64;
        while started.elapsed() < Duration::from_millis(300) {
            state = std::hint::black_box(
                state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1),
            );
        }
        let wall = u64::try_from(started.elapsed().as_micros()).expect("short");
        let used = cpu() - busy;
        assert!(
            used <= wall + tick && used >= wall / 2,
            "{used} µs CPU for {wall} µs of one thread's computation"
        );
    }

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

    /// A snapshot window is invalid for every dropped, mislabelled, refused,
    /// or inconsistent observation and for a short or extra count, and keeps
    /// its standard metrics, an unregistered one missing; one case
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
        let interval = |series, scans, dropped, invalid_labels| BenchmarkInterval {
            process: "p".to_owned(),
            nanos: 1,
            series,
            scans,
            metrics: Vec::new(),
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

        let mut broken = interval(
            vec![series(UNLABELLED, UNLABELLED, &[1], 1)],
            vec![scan(&[3], 1)],
            2,
            3,
        );
        broken.metrics = vec![IntervalMetric {
            name: "outbox_items_written_total".to_owned(),
            labels: [("outbox".to_owned(), "scribe".to_owned())].into(),
            rejected: 2,
            value: IntervalValue::Counter(5),
        }];
        let broken = Population::from_interval(broken);
        assert_eq!(
            broken.invalid,
            [
                "2 observations overflowed the server's series bounds and were dropped",
                "3 requests declared an unknown benchmark case or phase",
                "`bifrost.query` (success, case none, phase none) has 1 durations outside the trackable range",
                "`bifrost.query` (success, case none, phase none) recorded 1 elapsed observations for 2 requests",
                "metric outbox_items_written_total refused 2 unrepresentable updates",
            ]
        );
        assert_eq!(
            broken.counter("outbox_items_written_total", &[("outbox", "scribe")]),
            Some(5),
            "the window keeps its standard metrics"
        );
        assert_eq!(
            broken.counter("outbox_items_written_total", &[("outbox", "audit")]),
            None,
            "an unregistered counter is missing, never zero"
        );
        assert_eq!(
            broken.scanned_per_query("success")[0].value,
            None,
            "a query without its scan fact is missing, never zero"
        );
    }
}
