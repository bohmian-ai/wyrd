//! The benchmark registry: every timing, scan, and standard metric series a
//! benchmark process accumulates, its cumulative [`snapshot`], and the
//! interval arithmetic reports difference snapshots with. It exists only
//! with the `bench` feature, so an ordinary build holds no registry, series,
//! histogram, or snapshot.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{
    LazyLock, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};

use crate::recorder::{self, IntervalMetric, MAX_METRIC_SERIES, MetricSnapshot};
use crate::{
    CLOCK, HIGHEST_METRIC, HIGHEST_NS, HIGHEST_SCAN, LOWEST_NS, MAX_SCAN_SERIES, MAX_SERIES,
    SIGNIFICANT_DIGITS,
};

/// The process's benchmark registry.
pub(crate) static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

/// `mutex`'s guard, whatever a panicking recorder left: every update is a
/// whole increment, so a poisoned lock holds no half-written observation.
pub(crate) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`'s shared guard, whatever a panicking recorder left; see [`locked`].
pub(crate) fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`'s exclusive guard, whatever a panicking recorder left; see
/// [`locked`].
pub(crate) fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

/// Applies `update` to `key`'s series of `map` under that series' own lock,
/// opening it with `open` on first sight unless `bound` series are kept;
/// `false` when the bound refused it.
fn update<K: Ord + Copy, V>(
    map: &RwLock<BTreeMap<K, Mutex<V>>>,
    key: K,
    bound: usize,
    open: impl FnOnce() -> V,
    update: impl FnOnce(&mut V),
) -> bool {
    if let Some(series) = read(map).get(&key) {
        update(&mut locked(series));
        return true;
    }
    let mut map = write(map);
    if !map.contains_key(&key) && map.len() >= bound {
        return false;
    }
    let series = map.entry(key).or_insert_with(|| Mutex::new(open()));
    update(series.get_mut().unwrap_or_else(PoisonError::into_inner));
    true
}

/// A timing series' key: boundary, outcome, case, phase.
type Key = (&'static str, &'static str, &'static str, &'static str);

/// A scan series' key: outcome, case, phase.
type ScanKey = (&'static str, &'static str, &'static str);

/// A new empty duration histogram.
pub(crate) fn durations() -> Histogram<u64> {
    #[expect(
        clippy::expect_used,
        reason = "the constant bounds and precision are valid HDR parameters"
    )]
    Histogram::new_with_bounds(LOWEST_NS, HIGHEST_NS, SIGNIFICANT_DIGITS)
        .expect("constant duration histogram bounds are valid")
}

/// A new empty scan-fact histogram.
fn facts() -> Histogram<u64> {
    #[expect(
        clippy::expect_used,
        reason = "the constant bounds and precision are valid HDR parameters"
    )]
    Histogram::new_with_bounds(1, HIGHEST_SCAN, SIGNIFICANT_DIGITS)
        .expect("constant scan histogram bounds are valid")
}

/// A new empty standard-metric value histogram, in billionths.
pub(crate) fn metric_values() -> Histogram<u64> {
    #[expect(
        clippy::expect_used,
        reason = "the constant bounds and precision are valid HDR parameters"
    )]
    Histogram::new_with_bounds(1, HIGHEST_METRIC, SIGNIFICANT_DIGITS)
        .expect("constant metric histogram bounds are valid")
}

/// `value` recorded into `histogram`, or `false` when it is out of range.
pub(crate) fn record_into(histogram: &mut Histogram<u64>, value: u64) -> bool {
    histogram.record(value).is_ok()
}

/// One timing series' cumulative observations.
#[derive(Clone)]
struct Series {
    /// `record_timing` calls.
    count: u64,
    /// Elapsed durations.
    elapsed: Histogram<u64>,
    /// Excluded-wait unions, for a boundary that has them.
    excluded: Option<Histogram<u64>>,
    /// Elapsed less excluded, computed within each request.
    residual: Option<Histogram<u64>>,
    /// Calls lacking the excluded component an earlier call carried, or
    /// carrying one an earlier call lacked.
    missing_excluded: u64,
    /// Durations outside the trackable range, of any component.
    out_of_range: u64,
    /// Calls whose excluded waits exceeded their elapsed time.
    negative_residual: u64,
}

/// One scan series' cumulative observations.
struct Scan {
    /// `record_scan` calls.
    count: u64,
    /// Scanned rows.
    rows: Histogram<u64>,
    /// Scanned bytes.
    bytes: Histogram<u64>,
    /// Calls without a rows fact.
    missing_rows: u64,
    /// Calls without a bytes fact.
    missing_bytes: u64,
    /// Facts outside the trackable range.
    out_of_range: u64,
}

/// The process's cumulative benchmark timings and standard metrics, each
/// series under its own lock.
pub(crate) struct Registry {
    /// When the registry started: the origin of snapshot times.
    started: Instant,
    /// This process's identity, fresh on every start.
    process: String,
    /// Held shared while a request records its timing parts and exclusively
    /// while a snapshot copies the timing series, so no snapshot divides a
    /// request's group.
    pub(crate) groups: RwLock<()>,
    /// Timing series.
    series: RwLock<BTreeMap<Key, Mutex<Series>>>,
    /// Scan series.
    scans: RwLock<BTreeMap<ScanKey, Mutex<Scan>>>,
    /// Observations refused because their series would exceed a bound.
    pub(crate) dropped: AtomicU64,
    /// Requests that declared an unknown case or phase.
    pub(crate) invalid_labels: AtomicU64,
    /// Standard metric series.
    pub(crate) metrics: RwLock<recorder::Metrics>,
}

impl Registry {
    /// An empty registry of a fresh process identity.
    fn new() -> Self {
        Self {
            started: Instant::now(),
            process: ulid::Ulid::new().to_string(),
            groups: RwLock::new(()),
            series: RwLock::new(BTreeMap::new()),
            scans: RwLock::new(BTreeMap::new()),
            dropped: AtomicU64::new(0),
            invalid_labels: AtomicU64::new(0),
            metrics: RwLock::new(recorder::Metrics::default()),
        }
    }

    /// Records one observation into series `key`.
    pub(crate) fn record(&self, key: Key, elapsed: Duration, excluded: Option<Duration>) {
        let open = || Series {
            count: 0,
            elapsed: durations(),
            excluded: excluded.map(|_| durations()),
            residual: excluded.map(|_| durations()),
            missing_excluded: 0,
            out_of_range: 0,
            negative_residual: 0,
        };
        if !update(&self.series, key, MAX_SERIES, open, |series| {
            series.observe(elapsed, excluded);
        }) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records one query's scan facts into scan series `key`.
    pub(crate) fn scan(&self, key: ScanKey, rows: Option<u64>, bytes: Option<u64>) {
        let open = || Scan {
            count: 0,
            rows: facts(),
            bytes: facts(),
            missing_rows: 0,
            missing_bytes: 0,
            out_of_range: 0,
        };
        if !update(&self.scans, key, MAX_SCAN_SERIES, open, |scan| {
            scan.observe(rows, bytes);
        }) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// The cumulative snapshot of everything recorded so far.
    ///
    /// The timing series are copied while request groups are held out,
    /// then converted with every producer free; each scan and standard
    /// metric series is read under its own lock alone.
    fn snapshot(&self) -> BenchmarkSnapshot {
        let copied: Vec<(Key, Series)> = {
            let _cut = write(&self.groups);
            read(&self.series)
                .iter()
                .map(|(key, series)| (*key, locked(series).clone()))
                .collect()
        };
        let series = copied
            .iter()
            .map(
                |((boundary, outcome, case, phase), series)| SeriesSnapshot {
                    boundary: (*boundary).to_owned(),
                    outcome: (*outcome).to_owned(),
                    case: (*case).to_owned(),
                    phase: (*phase).to_owned(),
                    count: series.count,
                    elapsed: Buckets::of(&series.elapsed),
                    excluded: series.excluded.as_ref().map(Buckets::of),
                    residual: series.residual.as_ref().map(Buckets::of),
                    missing_excluded: series.missing_excluded,
                    out_of_range: series.out_of_range,
                    negative_residual: series.negative_residual,
                },
            )
            .collect();
        let scans = read(&self.scans)
            .iter()
            .map(|((outcome, case, phase), scan)| {
                let scan = locked(scan);
                ScanSnapshot {
                    outcome: (*outcome).to_owned(),
                    case: (*case).to_owned(),
                    phase: (*phase).to_owned(),
                    count: scan.count,
                    rows: Buckets::of(&scan.rows),
                    bytes: Buckets::of(&scan.bytes),
                    missing_rows: scan.missing_rows,
                    missing_bytes: scan.missing_bytes,
                    out_of_range: scan.out_of_range,
                }
            })
            .collect();
        let metrics = recorder::Metrics::snapshots(&self.metrics);
        BenchmarkSnapshot {
            process: self.process.clone(),
            pid: std::process::id(),
            taken_ns: u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            method: Method::current(),
            series,
            scans,
            metrics,
            dropped: self.dropped.load(Ordering::Relaxed),
            invalid_labels: self.invalid_labels.load(Ordering::Relaxed),
        }
    }
}

impl Series {
    /// Records one request's `elapsed` and `excluded` components.
    fn observe(&mut self, elapsed: Duration, excluded: Option<Duration>) {
        self.count += 1;
        let nanos = |duration: Duration| u64::try_from(duration.as_nanos()).ok();
        let elapsed = nanos(elapsed);
        match elapsed {
            Some(value) if record_into(&mut self.elapsed, value) => {}
            _ => self.out_of_range += 1,
        }
        match (excluded, &mut self.excluded, &mut self.residual) {
            (None, None, None) => {}
            (Some(excluded), Some(excluded_histogram), Some(residual_histogram)) => {
                let excluded = nanos(excluded);
                match excluded {
                    Some(value) if record_into(excluded_histogram, value) => {}
                    _ => self.out_of_range += 1,
                }
                match elapsed.zip(excluded).map(|(e, x)| e.checked_sub(x)) {
                    Some(Some(residual)) => {
                        if !record_into(residual_histogram, residual) {
                            self.out_of_range += 1;
                        }
                    }
                    Some(None) => self.negative_residual += 1,
                    None => self.out_of_range += 1,
                }
            }
            _ => self.missing_excluded += 1,
        }
    }
}

impl Scan {
    /// Records one query's scan facts.
    fn observe(&mut self, rows: Option<u64>, bytes: Option<u64>) {
        self.count += 1;
        for (fact, histogram, missing) in [
            (rows, &mut self.rows, &mut self.missing_rows),
            (bytes, &mut self.bytes, &mut self.missing_bytes),
        ] {
            match fact {
                Some(value) => {
                    if !record_into(histogram, value) {
                        self.out_of_range += 1;
                    }
                }
                None => *missing += 1,
            }
        }
    }
}

/// The cumulative snapshot of this process's benchmark timings and
/// standard metrics, each series read under its own lock and no request's
/// timing group divided.
#[must_use]
pub fn snapshot() -> BenchmarkSnapshot {
    REGISTRY.snapshot()
}

/// The histogram parameters every series of a snapshot shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
    /// [`CLOCK`].
    pub clock: String,
    /// [`SIGNIFICANT_DIGITS`].
    pub significant_digits: u8,
    /// [`LOWEST_NS`].
    pub lowest_ns: u64,
    /// [`HIGHEST_NS`].
    pub highest_ns: u64,
    /// [`HIGHEST_SCAN`].
    pub highest_scan: u64,
    /// [`MAX_SERIES`].
    pub max_series: usize,
    /// [`MAX_SCAN_SERIES`].
    pub max_scan_series: usize,
    /// [`MAX_METRIC_SERIES`]; zero in a record that predates standard
    /// metrics.
    #[serde(default)]
    pub max_metric_series: usize,
    /// [`HIGHEST_METRIC`]; zero in a record that predates standard metrics.
    #[serde(default)]
    pub highest_metric: u64,
}

impl Method {
    /// This build's parameters.
    #[must_use]
    pub fn current() -> Self {
        Self {
            clock: CLOCK.to_owned(),
            significant_digits: SIGNIFICANT_DIGITS,
            lowest_ns: LOWEST_NS,
            highest_ns: HIGHEST_NS,
            highest_scan: HIGHEST_SCAN,
            max_series: MAX_SERIES,
            max_scan_series: MAX_SCAN_SERIES,
            max_metric_series: MAX_METRIC_SERIES,
            highest_metric: HIGHEST_METRIC,
        }
    }
}

/// One histogram's nonzero buckets as `[highest equivalent value, count]`
/// pairs in ascending value order: re-recording each pair rebuilds the
/// exact bucket counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Buckets(
    /// `[highest equivalent value, count]` per nonzero bucket, ascending.
    pub Vec<[u64; 2]>,
);

impl Buckets {
    /// The nonzero buckets of `histogram`.
    pub(crate) fn of(histogram: &Histogram<u64>) -> Self {
        Self(
            histogram
                .iter_recorded()
                .map(|bucket| [bucket.value_iterated_to(), bucket.count_at_value()])
                .collect(),
        )
    }
}

/// One timing series in a snapshot, cumulative since the process started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesSnapshot {
    /// The owning boundary, such as `bifrost.query`.
    pub boundary: String,
    /// The closed outcome the owner recorded.
    pub outcome: String,
    /// The request's case, [`UNLABELLED`], or [`INVALID`].
    pub case: String,
    /// The request's phase, [`UNLABELLED`], or [`INVALID`].
    pub phase: String,
    /// Observations recorded.
    pub count: u64,
    /// Elapsed nanoseconds.
    pub elapsed: Buckets,
    /// Excluded-wait nanoseconds, for a boundary that has them.
    pub excluded: Option<Buckets>,
    /// Residual nanoseconds, for a boundary that has excluded waits.
    pub residual: Option<Buckets>,
    /// Observations whose excluded component was inconsistent with the
    /// series'.
    pub missing_excluded: u64,
    /// Durations outside the trackable range.
    pub out_of_range: u64,
    /// Observations whose excluded waits exceeded their elapsed time.
    pub negative_residual: u64,
}

/// One scan series in a snapshot, cumulative since the process started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanSnapshot {
    /// The query's terminal outcome as Oracle recorded it.
    pub outcome: String,
    /// The request's case, [`UNLABELLED`], or [`INVALID`].
    pub case: String,
    /// The request's phase, [`UNLABELLED`], or [`INVALID`].
    pub phase: String,
    /// Queries recorded.
    pub count: u64,
    /// Scanned rows per query.
    pub rows: Buckets,
    /// Scanned bytes per query.
    pub bytes: Buckets,
    /// Queries without a rows fact.
    pub missing_rows: u64,
    /// Queries without a bytes fact.
    pub missing_bytes: u64,
    /// Facts outside the trackable range.
    pub out_of_range: u64,
}

/// A cumulative read of one process's benchmark recorder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSnapshot {
    /// The process identity, fresh on every start: a replaced process
    /// cannot pass for the same one.
    pub process: String,
    /// The process id.
    pub pid: u32,
    /// Monotonic nanoseconds since the registry started.
    pub taken_ns: u64,
    /// Histogram parameters.
    pub method: Method,
    /// Every timing series.
    pub series: Vec<SeriesSnapshot>,
    /// Every scan series.
    pub scans: Vec<ScanSnapshot>,
    /// Every standard metric series; empty in a record that predates them.
    #[serde(default)]
    pub metrics: Vec<MetricSnapshot>,
    /// Observations refused by a series bound.
    pub dropped: u64,
    /// Requests that declared an unknown case or phase.
    pub invalid_labels: u64,
}

/// A rebuilt duration or scan-fact distribution of one interval.
#[derive(Debug, Clone)]
pub struct Distribution(
    /// The interval's bucket counts, in the parameters of its owning kind.
    Histogram<u64>,
);

/// One percentile of a [`Distribution`]: its nearest rank and the
/// inclusive bounds of the bucket that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantile {
    /// The rank `ceil(q * N)`, at least 1.
    pub rank: u64,
    /// The bucket's lowest equivalent value.
    pub lowest: u64,
    /// The bucket's highest equivalent value.
    pub highest: u64,
}

impl Distribution {
    /// Rebuilds a distribution of `buckets` in a histogram of `empty`'s
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns which bucket value is outside `empty`'s range.
    fn rebuild(buckets: &Buckets, mut empty: Histogram<u64>) -> Result<Self, String> {
        for [value, count] in &buckets.0 {
            empty
                .record_n(*value, *count)
                .map_err(|_| format!("a bucket at {value} is outside the declared range"))?;
        }
        Ok(Self(empty))
    }

    /// The bucket counts `self` added to `earlier`.
    ///
    /// # Errors
    ///
    /// Returns a reset message when any bucket of `earlier` exceeds `self`'s.
    fn since(&self, earlier: &Self) -> Result<Self, String> {
        let mut interval = self.0.clone();
        interval
            .subtract(&earlier.0)
            .map_err(|_| "a bucket count went backwards: the histogram was reset".to_owned())?;
        Ok(Self(interval))
    }

    /// A duration distribution of exact `samples` nanoseconds, as a client
    /// test builds one.
    ///
    /// # Errors
    ///
    /// Returns which sample is outside the duration range.
    pub fn of_durations(samples: &[u64]) -> Result<Self, String> {
        let mut histogram = durations();
        for sample in samples {
            histogram
                .record(*sample)
                .map_err(|_| format!("{sample} ns is outside the duration range"))?;
        }
        Ok(Self(histogram))
    }

    /// Adds `other`'s observations, merging two series of one population.
    ///
    /// # Errors
    ///
    /// Returns a message when `other` holds a value outside this
    /// distribution's range: a duration merged with a scan fact.
    pub fn add(&mut self, other: &Self) -> Result<(), String> {
        self.0
            .add(&other.0)
            .map_err(|_| "two distributions of different ranges cannot merge".to_owned())
    }

    /// Observations.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.0.len()
    }

    /// Whether there is no observation.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The nearest-rank percentile `q` in `0.0..=1.0` and its bucket
    /// bounds; `None` without observations.
    #[must_use]
    pub fn quantile(&self, q: f64) -> Option<Quantile> {
        if self.0.is_empty() {
            return None;
        }
        let value = self.0.value_at_quantile(q);
        let len = self.0.len();
        let rank = len
            .to_f64()
            .and_then(|n| (q * n).ceil().to_u64())
            .unwrap_or(len)
            .clamp(1, len);
        Some(Quantile {
            rank,
            lowest: self.0.lowest_equivalent(value),
            highest: self.0.highest_equivalent(value),
        })
    }

    /// The bounds of the largest observation's bucket; `None` without
    /// observations.
    #[must_use]
    pub fn maximum(&self) -> Option<Quantile> {
        self.quantile(1.0)
    }

    /// The exact sum of each bucket's lowest and highest equivalent values
    /// times its count: bounds of the interval's total.
    #[must_use]
    pub fn total_bounds(&self) -> (u64, u64) {
        self.0.iter_recorded().fold((0, 0), |(lo, hi), bucket| {
            let value = bucket.value_iterated_to();
            let count = bucket.count_at_value();
            (
                lo.saturating_add(self.0.lowest_equivalent(value).saturating_mul(count)),
                hi.saturating_add(self.0.highest_equivalent(value).saturating_mul(count)),
            )
        })
    }

    /// Observations above `threshold`, as the bounds a bucketed count
    /// allows: those whose whole bucket lies above it, and those whose
    /// bucket reaches above it, which adds the one bucket that straddles
    /// `threshold`.
    #[must_use]
    pub fn count_above(&self, threshold: u64) -> (u64, u64) {
        self.0
            .iter_recorded()
            .fold((0, 0), |(certain, possible), bucket| {
                let value = bucket.value_iterated_to();
                let count = bucket.count_at_value();
                (
                    certain + count * u64::from(self.0.lowest_equivalent(value) > threshold),
                    possible + count * u64::from(self.0.highest_equivalent(value) > threshold),
                )
            })
    }

    /// The nonzero buckets, for keeping the full interval distribution.
    #[must_use]
    pub fn buckets(&self) -> Buckets {
        Buckets::of(&self.0)
    }
}

/// One timing series over an interval.
#[derive(Debug, Clone)]
pub struct IntervalSeries {
    /// The owning boundary.
    pub boundary: String,
    /// The recorded outcome.
    pub outcome: String,
    /// The request's case.
    pub case: String,
    /// The request's phase.
    pub phase: String,
    /// Observations recorded in the interval.
    pub count: u64,
    /// Elapsed nanoseconds.
    pub elapsed: Distribution,
    /// Excluded-wait nanoseconds, for a boundary that has them.
    pub excluded: Option<Distribution>,
    /// Residual nanoseconds, for a boundary that has excluded waits.
    pub residual: Option<Distribution>,
    /// Inconsistent excluded components in the interval.
    pub missing_excluded: u64,
    /// Out-of-range durations in the interval.
    pub out_of_range: u64,
    /// Negative residuals in the interval.
    pub negative_residual: u64,
}

impl IntervalSeries {
    /// Why this series' interval cannot be judged: an out-of-range,
    /// negative, or missing component, or a component population that does
    /// not reconcile with the recorded count. Empty when it is complete.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let name = format!(
            "`{}` ({}, case {}, phase {})",
            self.boundary, self.outcome, self.case, self.phase
        );
        let mut problems = Vec::new();
        for (count, what) in [
            (self.out_of_range, "durations outside the trackable range"),
            (
                self.negative_residual,
                "excluded waits exceeding elapsed time",
            ),
            (self.missing_excluded, "inconsistent excluded components"),
        ] {
            if count > 0 {
                problems.push(format!("{name} has {count} {what}"));
            }
        }
        if self.elapsed.len() != self.count {
            problems.push(format!(
                "{name} recorded {} elapsed observations for {} requests",
                self.elapsed.len(),
                self.count
            ));
        }
        for (component, label) in [(&self.excluded, "excluded"), (&self.residual, "residual")] {
            if let Some(component) = component
                && component.len() != self.count
            {
                problems.push(format!(
                    "{name} recorded {} {label} observations for {} requests",
                    component.len(),
                    self.count
                ));
            }
        }
        problems
    }
}

/// One scan series over an interval.
#[derive(Debug, Clone)]
pub struct IntervalScan {
    /// The query's terminal outcome as Oracle recorded it.
    pub outcome: String,
    /// The request's case.
    pub case: String,
    /// The request's phase.
    pub phase: String,
    /// Queries recorded in the interval.
    pub count: u64,
    /// Scanned rows per query.
    pub rows: Distribution,
    /// Scanned bytes per query.
    pub bytes: Distribution,
    /// Queries without a rows fact.
    pub missing_rows: u64,
    /// Queries without a bytes fact.
    pub missing_bytes: u64,
    /// Out-of-range facts.
    pub out_of_range: u64,
}

/// Everything one process recorded between two snapshots.
#[derive(Debug, Clone)]
pub struct BenchmarkInterval {
    /// The process both snapshots read.
    pub process: String,
    /// Monotonic nanoseconds between the snapshots.
    pub nanos: u64,
    /// Timing series with at least one observation in the interval.
    pub series: Vec<IntervalSeries>,
    /// Scan series with at least one query in the interval.
    pub scans: Vec<IntervalScan>,
    /// Every standard metric series the later snapshot holds.
    pub metrics: Vec<IntervalMetric>,
    /// Observations refused by a series bound in the interval.
    pub dropped: u64,
    /// Requests that declared an unknown case or phase in the interval.
    pub invalid_labels: u64,
}

impl BenchmarkSnapshot {
    /// What this snapshot recorded after `earlier`, by bucket-count
    /// difference.
    ///
    /// # Errors
    ///
    /// Returns why the two cannot form an interval: another process or
    /// method, a later `earlier`, or a series or counter that went
    /// backwards or disappeared (a reset).
    pub fn since(&self, earlier: &Self) -> Result<BenchmarkInterval, String> {
        if self.process != earlier.process || self.pid != earlier.pid {
            return Err(format!(
                "the timing snapshots come from different processes ({} pid {}, then {} pid {}): the server was replaced",
                earlier.process, earlier.pid, self.process, self.pid
            ));
        }
        if self.method != earlier.method {
            return Err("the timing snapshots declare different histogram methods".to_owned());
        }
        let nanos = self
            .taken_ns
            .checked_sub(earlier.taken_ns)
            .ok_or("the later timing snapshot was taken first")?;
        let backwards = |what: &str| format!("{what} went backwards: the timings were reset");
        let dropped = self
            .dropped
            .checked_sub(earlier.dropped)
            .ok_or_else(|| backwards("the dropped count"))?;
        let invalid_labels = self
            .invalid_labels
            .checked_sub(earlier.invalid_labels)
            .ok_or_else(|| backwards("the invalid-label count"))?;
        let mut series = Vec::new();
        for later in &self.series {
            let prior = earlier.series.iter().find(|prior| {
                (&prior.boundary, &prior.outcome, &prior.case, &prior.phase)
                    == (&later.boundary, &later.outcome, &later.case, &later.phase)
            });
            if let Some(interval) = IntervalSeries::between(later, prior)? {
                series.push(interval);
            }
        }
        let mut scans = Vec::new();
        for later in &self.scans {
            let prior = earlier.scans.iter().find(|prior| {
                (&prior.outcome, &prior.case, &prior.phase)
                    == (&later.outcome, &later.case, &later.phase)
            });
            if let Some(interval) = IntervalScan::between(later, prior)? {
                scans.push(interval);
            }
        }
        let vanished = earlier.series.iter().any(|prior| {
            !self.series.iter().any(|later| {
                (&prior.boundary, &prior.outcome, &prior.case, &prior.phase)
                    == (&later.boundary, &later.outcome, &later.case, &later.phase)
            })
        }) || earlier.scans.iter().any(|prior| {
            !self.scans.iter().any(|later| {
                (&prior.outcome, &prior.case, &prior.phase)
                    == (&later.outcome, &later.case, &later.phase)
            })
        });
        if vanished {
            return Err(backwards("a series"));
        }
        let metrics = IntervalMetric::between_all(&self.metrics, &earlier.metrics)?;
        Ok(BenchmarkInterval {
            process: self.process.clone(),
            nanos,
            series,
            scans,
            metrics,
            dropped,
            invalid_labels,
        })
    }
}

/// `later - earlier` of the counter named `what`.
///
/// # Errors
///
/// Returns a reset message naming `what` when `later` is below `earlier`.
pub(crate) fn counted(later: u64, earlier: u64, what: &str) -> Result<u64, String> {
    later
        .checked_sub(earlier)
        .ok_or_else(|| format!("{what} went backwards: the timings were reset"))
}

/// The interval distribution of `later` since `earlier`, `earlier` absent
/// counting from empty.
///
/// # Errors
///
/// Returns why either side cannot rebuild in `empty`'s range, or a reset
/// when a bucket count went backwards.
pub(crate) fn interval_of(
    later: &Buckets,
    earlier: Option<&Buckets>,
    empty: impl Fn() -> Histogram<u64>,
) -> Result<Distribution, String> {
    let later = Distribution::rebuild(later, empty())?;
    match earlier {
        Some(earlier) => later.since(&Distribution::rebuild(earlier, empty())?),
        None => Ok(later),
    }
}

impl IntervalSeries {
    /// `later` since `earlier`, or `None` when nothing was recorded in
    /// between.
    ///
    /// # Errors
    ///
    /// Returns a reset message when any count or bucket went backwards, or a
    /// range message when a bucket cannot rebuild.
    fn between(
        later: &SeriesSnapshot,
        earlier: Option<&SeriesSnapshot>,
    ) -> Result<Option<Self>, String> {
        let zero = |pick: fn(&SeriesSnapshot) -> u64| earlier.map_or(0, pick);
        let count = counted(later.count, zero(|s| s.count), "a series count")?;
        if count == 0 {
            return Ok(None);
        }
        let component = |pick: fn(&SeriesSnapshot) -> Option<&Buckets>| {
            pick(later)
                .map(|buckets| interval_of(buckets, earlier.and_then(pick), durations))
                .transpose()
        };
        Ok(Some(Self {
            boundary: later.boundary.clone(),
            outcome: later.outcome.clone(),
            case: later.case.clone(),
            phase: later.phase.clone(),
            count,
            elapsed: interval_of(&later.elapsed, earlier.map(|s| &s.elapsed), durations)?,
            excluded: component(|s| s.excluded.as_ref())?,
            residual: component(|s| s.residual.as_ref())?,
            missing_excluded: counted(
                later.missing_excluded,
                zero(|s| s.missing_excluded),
                "a missing-component count",
            )?,
            out_of_range: counted(
                later.out_of_range,
                zero(|s| s.out_of_range),
                "an out-of-range count",
            )?,
            negative_residual: counted(
                later.negative_residual,
                zero(|s| s.negative_residual),
                "a negative-residual count",
            )?,
        }))
    }
}

impl IntervalScan {
    /// `later` since `earlier`, or `None` when nothing was recorded in
    /// between.
    ///
    /// # Errors
    ///
    /// Returns a reset message when any count or bucket went backwards, or a
    /// range message when a bucket cannot rebuild.
    fn between(
        later: &ScanSnapshot,
        earlier: Option<&ScanSnapshot>,
    ) -> Result<Option<Self>, String> {
        let zero = |pick: fn(&ScanSnapshot) -> u64| earlier.map_or(0, pick);
        let count = counted(later.count, zero(|s| s.count), "a scan count")?;
        if count == 0 {
            return Ok(None);
        }
        Ok(Some(Self {
            outcome: later.outcome.clone(),
            case: later.case.clone(),
            phase: later.phase.clone(),
            count,
            rows: interval_of(&later.rows, earlier.map(|s| &s.rows), facts)?,
            bytes: interval_of(&later.bytes, earlier.map(|s| &s.bytes), facts)?,
            missing_rows: counted(
                later.missing_rows,
                zero(|s| s.missing_rows),
                "a missing-rows count",
            )?,
            missing_bytes: counted(
                later.missing_bytes,
                zero(|s| s.missing_bytes),
                "a missing-bytes count",
            )?,
            out_of_range: counted(
                later.out_of_range,
                zero(|s| s.out_of_range),
                "an out-of-range count",
            )?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Registry, Series, durations};
    use crate::Labels;

    /// Records `elapsed`/`excluded` nanoseconds into `registry` under
    /// `case` and `phase` at `boundary`.
    fn observe(
        registry: &Registry,
        boundary: &'static str,
        case: &'static str,
        phase: &'static str,
        elapsed: u64,
        excluded: Option<u64>,
    ) {
        registry.record(
            (boundary, "success", case, phase),
            Duration::from_nanos(elapsed),
            excluded.map(Duration::from_nanos),
        );
    }

    /// The existing standard `metrics` macros and the explicit request
    /// timing land in one benchmark snapshot: counters and label-order
    /// variants share a series, a gauge keeps its decrements and peak, a
    /// `_seconds` histogram records nanoseconds while any other records
    /// billionths of its values, so whole counts, a fractional ratio, and a
    /// decimal cost all keep their value; an unrepresentable value is
    /// refused, a request keeps its residual, and an absent series is
    /// missing rather than zero.
    ///
    /// # Panics
    ///
    /// Panics when the recorder does not install or any series, unit, or
    /// count is wrong.
    #[test]
    fn standard_metrics_and_request_timings_share_one_snapshot() {
        crate::install().expect("the only recorder");
        assert!(crate::enabled());
        crate::install().expect("a repeated install is a no-op");
        let before = crate::snapshot();
        metrics::counter!("bench_test_total", "lane" => "a", "status" => "ok").increment(2);
        metrics::counter!("bench_test_total", "status" => "ok", "lane" => "a").increment(1);
        metrics::counter!("bench_test_total", "lane" => "b", "status" => "ok").increment(4);
        let pending = metrics::gauge!("bench_test_pending");
        pending.increment(5.0);
        pending.decrement(3.0);
        metrics::histogram!("bench_test_duration_seconds").record(0.000_25);
        let rows = metrics::histogram!("bench_test_rows");
        rows.record(4.0);
        rows.record(-1.0);
        rows.record(f64::NAN);
        metrics::histogram!("bifrost_variant_residual_share", "writer" => "scribe").record(0.25);
        metrics::histogram!("wyrd_gateway_call_cost", "currency" => "USD").record(0.000_123);
        crate::record_timing(
            &tracing::Span::none(),
            "bench.request",
            "success",
            Duration::from_micros(300),
            Some(Duration::from_micros(100)),
        );
        let after = crate::snapshot();
        let interval = after.since(&before).expect("one process");

        assert_eq!(
            interval.counter("bench_test_total", &[("lane", "a")]),
            Some(3)
        );
        assert_eq!(interval.counter("bench_test_total", &[]), Some(7));
        assert_eq!(interval.counter("bench_test_absent_total", &[]), None);
        assert_eq!(
            after
                .metrics
                .iter()
                .filter(|metric| metric.name == "bench_test_total")
                .count(),
            2,
            "label order does not open a series"
        );
        let gauge = interval
            .metrics
            .iter()
            .find(|metric| metric.name == "bench_test_pending")
            .expect("the gauge");
        assert!(matches!(
            gauge.value,
            crate::IntervalValue::Gauge { value, peak }
                if value.to_bits() == 2.0_f64.to_bits() && peak.to_bits() == 5.0_f64.to_bits()
        ));
        let duration = interval
            .histogram("bench_test_duration_seconds", &[])
            .expect("one duration");
        let at = duration.quantile(1.0).expect("one observation");
        assert_eq!(duration.len(), 1);
        assert!(at.lowest <= 250_000 && 250_000 <= at.highest, "{at:?}");
        let refused = interval.histogram("bench_test_rows", &[]);
        assert!(
            refused.is_err_and(|error| error.contains("2 unrepresentable")),
            "a negative or non-finite value is refused, never clamped"
        );
        let histogram = |name: &str| {
            interval
                .metrics
                .iter()
                .find_map(|metric| match &metric.value {
                    crate::IntervalValue::Histogram { unit, distribution }
                        if metric.name == name =>
                    {
                        Some((metric.rejected, *unit, distribution.clone()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the {name} histogram"))
        };
        for (name, value) in [
            ("bench_test_rows", 4.0),
            ("bifrost_variant_residual_share", 0.25),
            ("wyrd_gateway_call_cost", 0.000_123),
        ] {
            let (rejected, unit, values) = histogram(name);
            assert_eq!(unit, crate::HistogramUnit::Billionths, "{name}");
            let at = values.quantile(1.0).expect("one value");
            let original = |scaled: u64| {
                num_traits::ToPrimitive::to_f64(&scaled).expect("finite")
                    / num_traits::ToPrimitive::to_f64(&unit.per_unit()).expect("finite")
            };
            assert!(
                original(at.lowest) <= value && value <= original(at.highest),
                "{name} keeps {value} in original units: {at:?}"
            );
            if name != "bench_test_rows" {
                assert_eq!((rejected, values.len()), (0, 1), "{name}");
                assert!(interval.histogram(name, &[]).is_ok(), "{name}");
            }
        }
        assert_eq!(crate::HistogramUnit::Nanoseconds.per_unit(), 1_000_000_000);
        assert!(
            interval
                .histogram("bench_test_absent_seconds", &[])
                .is_err()
        );

        let request = interval
            .series
            .iter()
            .find(|series| series.boundary == "bench.request")
            .expect("the request timing");
        assert_eq!(request.count, 1);
        let residual = request.residual.as_ref().expect("a residual");
        let own = residual.quantile(1.0).expect("one residual");
        assert!(own.lowest <= 200_000 && 200_000 <= own.highest);
        assert!(
            !interval
                .metrics
                .iter()
                .any(|metric| metric.name.contains("request")),
            "an explicit timing opens no metric series"
        );
        let mut reset = after.clone();
        reset
            .metrics
            .retain(|metric| metric.name != "bench_test_pending");
        assert!(reset.since(&after).is_err(), "a vanished metric is a reset");
    }

    /// Unknown labels are classified invalid, absent ones unlabelled.
    ///
    /// # Panics
    ///
    /// Panics when a label is classified other than declared.
    #[test]
    fn labels_are_closed() {
        let value = |text: &str| http::HeaderValue::from_str(text).ok();
        assert_eq!(
            Labels::label(value("point-lookup").as_ref(), crate::CASES),
            "point-lookup"
        );
        assert_eq!(
            Labels::label(value("select 1").as_ref(), crate::CASES),
            crate::INVALID
        );
        assert_eq!(Labels::label(None, crate::PHASES), crate::UNLABELLED);
    }

    /// A cohort's headers declare exactly its closed labels, which read back
    /// as the same cohort; an unlabelled or invalid label is not declared.
    ///
    /// # Panics
    ///
    /// Panics when a header is missing, extra, or reads back differently.
    #[test]
    fn cohort_headers_declare_only_closed_labels() {
        let cohort = crate::Cohort::of(Some("direct-verify-model"), Some("repetition-3"));
        assert_eq!(crate::Cohort::of_headers(&cohort.headers()), cohort);
        let unknown = crate::Cohort::of(Some("select 1"), None);
        assert_eq!(
            (unknown.case(), unknown.phase()),
            (crate::INVALID, crate::UNLABELLED)
        );
        assert!(unknown.headers().is_empty());
        assert!(crate::Cohort::NONE.headers().is_empty());
    }

    /// A series opened beyond the bound is dropped and counted, never
    /// merged into another series.
    ///
    /// # Panics
    ///
    /// Panics when the extra series is kept or not counted dropped.
    #[test]
    fn series_beyond_the_bound_are_dropped() {
        let registry = Registry::new();
        for _ in 0..crate::MAX_SERIES {
            super::write(&registry.series).insert(
                (
                    "bifrost.query",
                    "success",
                    Box::leak(ulid::Ulid::new().to_string().into_boxed_str()),
                    "none",
                ),
                std::sync::Mutex::new(Series {
                    count: 0,
                    elapsed: durations(),
                    excluded: None,
                    residual: None,
                    missing_excluded: 0,
                    out_of_range: 0,
                    negative_residual: 0,
                }),
            );
        }
        observe(&registry, "bifrost.query", "full-scan", "none", 5, None);
        assert_eq!(registry.snapshot().dropped, 1);
        assert_eq!(crate::read(&registry.series).len(), crate::MAX_SERIES);
    }

    /// A count above a threshold is exact away from it and widens by only
    /// the one bucket that straddles it.
    ///
    /// # Panics
    ///
    /// Panics when a count or its bounds are wrong.
    #[test]
    fn counts_above_a_threshold_bound_its_bucket() {
        let samples = [100_000, 499_000, 500_100, 2_000_000, 12_000_000];
        let distribution = super::Distribution::of_durations(&samples).expect("in range");
        assert_eq!(distribution.count_above(1_000_000), (2, 2));
        assert_eq!(distribution.count_above(0), (5, 5));
        assert_eq!(distribution.count_above(20_000_000), (0, 0));
        let (certain, possible) = distribution.count_above(500_000);
        assert!(
            certain <= 3 && 3 <= possible && possible - certain <= 1,
            "{certain}..{possible}"
        );
    }
}
