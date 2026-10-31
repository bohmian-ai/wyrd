//! Bounded high-resolution server timing histograms for benchmark-owned
//! servers.
//!
//! Each owning server boundary already measures one request with its own
//! monotonic timer and passes the exact durations to
//! [`crate::record_timing`]. When [`enable`] has been called, which only a
//! server whose metrics listener serves the benchmark snapshot does, every
//! such observation is recorded here into HDR histograms of integer
//! nanoseconds at [`SIGNIFICANT_DIGITS`] significant digits over
//! [`LOWEST_NS`]..=[`HIGHEST_NS`]: its elapsed time, its excluded waits, and
//! the residual computed within the request before recording. A query's
//! executed-plan scan facts are recorded beside its query series by
//! [`record_scan`]. Ordinary deployments never enable it and record nothing.
//!
//! Series are keyed by the closed boundary and outcome labels the owners
//! pass and by the request's benchmark classification ([`CASES`] and
//! [`PHASES`]), which [`label_request`] reads from the
//! [`CASE_HEADER`]/[`PHASE_HEADER`] metadata of a benchmark client onto the
//! request span. A classification confers nothing but a series; an unknown
//! value is counted and kept under [`INVALID`]. There are at most
//! [`MAX_SERIES`] timing series and [`MAX_SCAN_SERIES`] scan series; an
//! observation that would open another is counted in `dropped`, never
//! merged into an existing series.
//!
//! [`snapshot`] reads every cumulative distribution and counter under the
//! same lock that records them, so a snapshot is consistent and no
//! concurrent observation is lost or counted twice. There is no reset: an
//! interval is the bucket-count difference of two snapshots of the same
//! process ([`TimingSnapshot::since`]). A value outside the range, a
//! negative residual, or a missing component is counted, never clamped,
//! and makes the interval's series report a problem.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};
use tracing_subscriber::registry::LookupSpan as _;

/// Significant decimal digits every histogram keeps.
pub const SIGNIFICANT_DIGITS: u8 = 3;

/// The lowest discernible duration, nanoseconds; zero is still recorded
/// and counted exactly.
pub const LOWEST_NS: u64 = 1;

/// The highest trackable duration, nanoseconds (2^37 ns, about 137 s); a
/// longer observation is counted out of range.
pub const HIGHEST_NS: u64 = 1 << 37;

/// The highest trackable scanned-row or scanned-byte count of one query
/// (2^50); a larger fact is counted out of range.
pub const HIGHEST_SCAN: u64 = 1 << 50;

/// The most timing series one process keeps.
pub const MAX_SERIES: usize = 256;

/// The most scan series one process keeps.
pub const MAX_SCAN_SERIES: usize = 64;

/// The request metadata carrying a benchmark request's case.
pub const CASE_HEADER: &str = "x-wyrd-benchmark-case";

/// The request metadata carrying a benchmark request's phase.
pub const PHASE_HEADER: &str = "x-wyrd-benchmark-phase";

/// The closed benchmark cases and query classes a request may declare.
/// `probe` is an incidental operation of the harness itself, which never
/// counts as offered work.
pub const CASES: &[&str] = &[
    "point-lookup",
    "small-aggregate",
    "million-aggregate",
    "table-aggregate",
    "full-scan",
    "trace-lookup",
    "shredded-filter",
    "long-tail-filter",
    "native-write",
    "otlp-spans",
    "otlp-spans-1k",
    "otlp-logs",
    "direct-verify",
    "queued-verify",
    "evidence-write",
    "buffered-1kib-capture-metadata",
    "streamed-1kib-capture-metadata",
    "buffered-64kib-capture-metadata",
    "buffered-64kib-capture-payload",
    "probe",
];

/// The closed benchmark phases a request may declare.
pub const PHASES: &[&str] = &[
    "warmup",
    "repetition-1",
    "repetition-2",
    "repetition-3",
    "steady",
    "capacity",
    "overload",
    "recovery",
];

/// The label of a request that declared no case or phase.
pub const UNLABELLED: &str = "none";

/// The label of a request whose declared case or phase is not in
/// [`CASES`] or [`PHASES`].
pub const INVALID: &str = "invalid";

/// The clock every recorded duration comes from.
pub const CLOCK: &str = "std::time::Instant: the OS monotonic clock";

/// Whether this process records benchmark timings.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The process's timing registry.
// ponytail: one process-wide lock; per-series locks if recording contention ever shows in server CPU.
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::new()));

/// Starts recording benchmark timings in this process. Only a server that
/// serves the benchmark snapshot calls it; it cannot be turned off.
pub fn enable() {
    LazyLock::force(&REGISTRY);
    ENABLED.store(true, Ordering::Release);
}

/// Whether [`enable`] was called in this process.
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// One request's benchmark classification, kept in its span's extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Labels {
    /// One of [`CASES`], [`UNLABELLED`], or [`INVALID`].
    case: &'static str,
    /// One of [`PHASES`], [`UNLABELLED`], or [`INVALID`].
    phase: &'static str,
}

impl Labels {
    /// No classification.
    const NONE: Self = Self {
        case: UNLABELLED,
        phase: UNLABELLED,
    };

    /// The label `value` names in `closed`: [`UNLABELLED`] when absent and
    /// [`INVALID`] when unknown.
    fn label(value: Option<&http::HeaderValue>, closed: &[&'static str]) -> &'static str {
        let Some(value) = value else {
            return UNLABELLED;
        };
        value
            .to_str()
            .ok()
            .and_then(|value| closed.iter().find(|label| **label == value))
            .copied()
            .unwrap_or(INVALID)
    }
}

/// Reads the benchmark metadata in `headers` onto the request `span`
/// opens, so every timing recorded under it lands in its case and phase
/// series; an unknown value is counted once in `invalid_labels` and
/// classified [`INVALID`]. It does nothing unless timing is [`enable`]d or
/// when the request declares neither. The classification only selects a
/// series: it carries no authority, tenant, or sampling choice.
pub fn label_request(span: &tracing::Span, headers: &http::HeaderMap) {
    if !enabled() {
        return;
    }
    let (case, phase) = (headers.get(CASE_HEADER), headers.get(PHASE_HEADER));
    if case.is_none() && phase.is_none() {
        return;
    }
    let labels = Labels {
        case: Labels::label(case, CASES),
        phase: Labels::label(phase, PHASES),
    };
    if labels.case == INVALID || labels.phase == INVALID {
        lock().invalid_labels += 1;
    }
    span.with_subscriber(|(id, dispatch)| {
        if let Some(span) = dispatch
            .downcast_ref::<tracing_subscriber::Registry>()
            .and_then(|registry| registry.span(id))
        {
            span.extensions_mut().replace(labels);
        }
    });
}

/// The classification of the nearest span from `span` to its root that
/// [`label_request`] labelled, or none.
fn labels_of(span: &tracing::Span) -> Labels {
    span.with_subscriber(|(id, dispatch)| {
        let registry = dispatch.downcast_ref::<tracing_subscriber::Registry>()?;
        registry
            .span(id)?
            .scope()
            .find_map(|span| span.extensions().get::<Labels>().copied())
    })
    .flatten()
    .unwrap_or(Labels::NONE)
}

/// Records one request's server timing, and each of its `parts` as an
/// observation of the part's own boundary, under `parent`'s
/// classification and one lock acquisition; see
/// [`crate::record_timing_parts`]. Nothing is recorded unless [`enable`]d.
pub(crate) fn record(
    parent: &tracing::Span,
    boundary: &'static str,
    outcome: &'static str,
    elapsed: Duration,
    excluded: Option<Duration>,
    parts: &[(&'static str, Duration)],
) {
    if !enabled() {
        return;
    }
    let labels = labels_of(parent);
    let mut registry = lock();
    registry.record(
        (boundary, outcome, labels.case, labels.phase),
        elapsed,
        excluded,
    );
    for (part, duration) in parts {
        registry.record((part, outcome, labels.case, labels.phase), *duration, None);
    }
}

/// Records one query's executed-plan scan facts, rows and bytes when its
/// scans reported them, under `span`'s classification and the query's
/// terminal `outcome`. Nothing is recorded unless [`enable`]d; a missing
/// fact is counted, never recorded as zero.
pub fn record_scan(
    span: &tracing::Span,
    outcome: &'static str,
    rows: Option<u64>,
    bytes: Option<u64>,
) {
    if !enabled() {
        return;
    }
    let labels = labels_of(span);
    lock().scan((outcome, labels.case, labels.phase), rows, bytes);
}

/// The registry, whatever a panicking recorder left: every update is a
/// whole increment, so a poisoned lock holds no half-written observation.
fn lock() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A timing series' key: boundary, outcome, case, phase.
type Key = (&'static str, &'static str, &'static str, &'static str);

/// A scan series' key: outcome, case, phase.
type ScanKey = (&'static str, &'static str, &'static str);

/// A new empty duration histogram.
fn durations() -> Histogram<u64> {
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

/// `value` recorded into `histogram`, or `false` when it is out of range.
fn record_into(histogram: &mut Histogram<u64>, value: u64) -> bool {
    histogram.record(value).is_ok()
}

/// One timing series' cumulative observations.
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

/// The process's cumulative benchmark timings.
struct Registry {
    /// When the registry started: the origin of snapshot times.
    started: Instant,
    /// This process's identity, fresh on every start.
    process: String,
    /// Timing series.
    series: BTreeMap<Key, Series>,
    /// Scan series.
    scans: BTreeMap<ScanKey, Scan>,
    /// Observations refused because their series would exceed a bound.
    dropped: u64,
    /// Requests that declared an unknown case or phase.
    invalid_labels: u64,
}

impl Registry {
    /// An empty registry of a fresh process identity.
    fn new() -> Self {
        Self {
            started: Instant::now(),
            process: ulid::Ulid::new().to_string(),
            series: BTreeMap::new(),
            scans: BTreeMap::new(),
            dropped: 0,
            invalid_labels: 0,
        }
    }

    /// Records one observation into series `key`.
    fn record(&mut self, key: Key, elapsed: Duration, excluded: Option<Duration>) {
        if !self.series.contains_key(&key) && self.series.len() >= MAX_SERIES {
            self.dropped += 1;
            return;
        }
        let series = self.series.entry(key).or_insert_with(|| Series {
            count: 0,
            elapsed: durations(),
            excluded: excluded.map(|_| durations()),
            residual: excluded.map(|_| durations()),
            missing_excluded: 0,
            out_of_range: 0,
            negative_residual: 0,
        });
        series.count += 1;
        let nanos = |duration: Duration| u64::try_from(duration.as_nanos()).ok();
        let elapsed = nanos(elapsed);
        match elapsed {
            Some(value) if record_into(&mut series.elapsed, value) => {}
            _ => series.out_of_range += 1,
        }
        match (excluded, &mut series.excluded, &mut series.residual) {
            (None, None, None) => {}
            (Some(excluded), Some(excluded_histogram), Some(residual_histogram)) => {
                let excluded = nanos(excluded);
                match excluded {
                    Some(value) if record_into(excluded_histogram, value) => {}
                    _ => series.out_of_range += 1,
                }
                match elapsed.zip(excluded).map(|(e, x)| e.checked_sub(x)) {
                    Some(Some(residual)) => {
                        if !record_into(residual_histogram, residual) {
                            series.out_of_range += 1;
                        }
                    }
                    Some(None) => series.negative_residual += 1,
                    None => series.out_of_range += 1,
                }
            }
            _ => series.missing_excluded += 1,
        }
    }

    /// Records one query's scan facts into scan series `key`.
    fn scan(&mut self, key: ScanKey, rows: Option<u64>, bytes: Option<u64>) {
        if !self.scans.contains_key(&key) && self.scans.len() >= MAX_SCAN_SERIES {
            self.dropped += 1;
            return;
        }
        let scan = self.scans.entry(key).or_insert_with(|| Scan {
            count: 0,
            rows: facts(),
            bytes: facts(),
            missing_rows: 0,
            missing_bytes: 0,
            out_of_range: 0,
        });
        scan.count += 1;
        for (fact, histogram, missing) in [
            (rows, &mut scan.rows, &mut scan.missing_rows),
            (bytes, &mut scan.bytes, &mut scan.missing_bytes),
        ] {
            match fact {
                Some(value) => {
                    if !record_into(histogram, value) {
                        scan.out_of_range += 1;
                    }
                }
                None => *missing += 1,
            }
        }
    }

    /// The cumulative snapshot of everything recorded so far.
    fn snapshot(&self) -> TimingSnapshot {
        TimingSnapshot {
            process: self.process.clone(),
            pid: std::process::id(),
            taken_ns: u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            method: Method::current(),
            series: self
                .series
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
                .collect(),
            scans: self
                .scans
                .iter()
                .map(|((outcome, case, phase), scan)| ScanSnapshot {
                    outcome: (*outcome).to_owned(),
                    case: (*case).to_owned(),
                    phase: (*phase).to_owned(),
                    count: scan.count,
                    rows: Buckets::of(&scan.rows),
                    bytes: Buckets::of(&scan.bytes),
                    missing_rows: scan.missing_rows,
                    missing_bytes: scan.missing_bytes,
                    out_of_range: scan.out_of_range,
                })
                .collect(),
            dropped: self.dropped,
            invalid_labels: self.invalid_labels,
        }
    }
}

/// The cumulative snapshot of this process's benchmark timings, read
/// consistently under the recording lock.
#[must_use]
pub fn snapshot() -> TimingSnapshot {
    lock().snapshot()
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
    fn of(histogram: &Histogram<u64>) -> Self {
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

/// A cumulative read of one process's benchmark timings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimingSnapshot {
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
pub struct TimingInterval {
    /// The process both snapshots read.
    pub process: String,
    /// Monotonic nanoseconds between the snapshots.
    pub nanos: u64,
    /// Timing series with at least one observation in the interval.
    pub series: Vec<IntervalSeries>,
    /// Scan series with at least one query in the interval.
    pub scans: Vec<IntervalScan>,
    /// Observations refused by a series bound in the interval.
    pub dropped: u64,
    /// Requests that declared an unknown case or phase in the interval.
    pub invalid_labels: u64,
}

impl TimingSnapshot {
    /// What this snapshot recorded after `earlier`, by bucket-count
    /// difference.
    ///
    /// # Errors
    ///
    /// Returns why the two cannot form an interval: another process or
    /// method, a later `earlier`, or a series or counter that went
    /// backwards or disappeared (a reset).
    pub fn since(&self, earlier: &Self) -> Result<TimingInterval, String> {
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
        Ok(TimingInterval {
            process: self.process.clone(),
            nanos,
            series,
            scans,
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
fn counted(later: u64, earlier: u64, what: &str) -> Result<u64, String> {
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
fn interval_of(
    later: &Buckets,
    earlier: Option<&Buckets>,
    empty: fn() -> Histogram<u64>,
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

    use super::{Labels, Registry, Series, durations};

    /// Records `elapsed`/`excluded` nanoseconds into `registry` under
    /// `case` and `phase` at `boundary`.
    fn observe(
        registry: &mut Registry,
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

    /// Unknown labels are classified invalid, absent ones unlabelled.
    ///
    /// # Panics
    ///
    /// Panics when a label is classified other than declared.
    #[test]
    fn labels_are_closed() {
        let value = |text: &str| http::HeaderValue::from_str(text).ok();
        assert_eq!(
            Labels::label(value("point-lookup").as_ref(), super::CASES),
            "point-lookup"
        );
        assert_eq!(
            Labels::label(value("select 1").as_ref(), super::CASES),
            super::INVALID
        );
        assert_eq!(Labels::label(None, super::PHASES), super::UNLABELLED);
    }

    /// A series opened beyond the bound is dropped and counted, never
    /// merged into another series.
    ///
    /// # Panics
    ///
    /// Panics when the extra series is kept or not counted dropped.
    #[test]
    fn series_beyond_the_bound_are_dropped() {
        let mut registry = Registry::new();
        for _ in 0..super::MAX_SERIES {
            registry.series.insert(
                (
                    "bifrost.query",
                    "success",
                    Box::leak(ulid::Ulid::new().to_string().into_boxed_str()),
                    "none",
                ),
                Series {
                    count: 0,
                    elapsed: durations(),
                    excluded: None,
                    residual: None,
                    missing_excluded: 0,
                    out_of_range: 0,
                    negative_residual: 0,
                },
            );
        }
        observe(&mut registry, "bifrost.query", "full-scan", "none", 5, None);
        assert_eq!(registry.dropped, 1);
        assert_eq!(registry.series.len(), super::MAX_SERIES);
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
