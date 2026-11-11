//! The benchmark recorder: the sole metrics backend of a benchmark-owned
//! Wyrd server, and nothing in an ordinary deployment.
//!
//! A server started for benchmarking calls [`install`] before it registers
//! any metric. That installs [`BenchmarkRecorder`] as the process's only
//! standard `metrics` recorder, so every existing counter, gauge, and
//! histogram lands here, and starts recording the explicit request timings
//! owners pass to [`record_timing`]. No Prometheus recorder or exporter runs
//! beside it, and this is unrelated to tracing and OTLP export. Ordinary
//! deployments never install it and record nothing here.
//!
//! Each owning server boundary measures one request with its own monotonic
//! timer and passes the exact durations to [`record_timing`], recorded into
//! HDR histograms of integer nanoseconds at [`SIGNIFICANT_DIGITS`]
//! significant digits over [`LOWEST_NS`]..=[`HIGHEST_NS`]: its elapsed time,
//! its excluded waits, and the residual computed within the request before
//! recording. Where an owner has both an explicit timing and a production
//! duration histogram for one observation, it records the production
//! histogram only when [`enabled`] is false, so one observation is counted
//! once. A query's executed-plan scan facts are recorded beside its query
//! series by [`record_scan`].
//!
//! Timing series are keyed by the closed boundary and outcome labels the owners
//! pass and by the request's benchmark classification ([`CASES`] and
//! [`PHASES`]), which [`label_request`] reads from the
//! [`CASE_HEADER`]/[`PHASE_HEADER`] metadata of a benchmark client onto the
//! request span. A classification confers nothing but a series; an unknown
//! value is counted and kept under [`INVALID`]. There are at most
//! [`MAX_SERIES`] timing series, [`MAX_SCAN_SERIES`] scan series, and
//! [`MAX_METRIC_SERIES`] standard metric series; an observation that would
//! open another is counted in `dropped`, never merged into an existing
//! series.
//!
//! Every series has its own lock, so unrelated series never wait for each
//! other. [`snapshot`] reads each series under its lock, so no concurrent
//! observation is lost or counted twice, and holds a grouped request's
//! timings out only while it copies the timing series, so a snapshot holds
//! all of one request's timing parts or none. There is no reset: an
//! interval is the difference of two snapshots of the same process
//! ([`BenchmarkSnapshot::since`]). A value outside the range, a negative
//! residual, or a missing component is counted, never clamped, and makes the
//! interval's series report a problem.

#[cfg(feature = "heap")]
pub mod heap;
mod recorder;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{
    LazyLock, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};
use tracing_subscriber::registry::LookupSpan as _;

pub use recorder::{
    BenchmarkRecorder, HistogramUnit, IntervalMetric, IntervalValue, MAX_METRIC_SERIES,
    MetricSnapshot, MetricValue,
};

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

/// The highest trackable standard-metric histogram value, in billionths
/// (2^62, a value of about 4.6 billion); a larger observation is rejected.
/// It holds every residual share, call cost, pool size, queue depth, and
/// outbox batch item or byte count a benchmark server records.
pub const HIGHEST_METRIC: u64 = 1 << 62;

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
    "direct-verify-model",
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

/// Whether [`install`] made this process a benchmark recorder.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The process's benchmark registry.
static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

/// Installs [`BenchmarkRecorder`] as this process's only standard `metrics`
/// recorder and starts recording benchmark timings. A benchmark server calls
/// it once at startup, before registering any metric; it cannot be turned
/// off, and a repeated call after it succeeded does nothing.
///
/// # Errors
///
/// Returns the refusal when another `metrics` recorder is already installed:
/// a process never mixes the benchmark recorder with another backend.
pub fn install() -> Result<(), metrics::SetRecorderError<BenchmarkRecorder>> {
    if enabled() {
        return Ok(());
    }
    LazyLock::force(&REGISTRY);
    metrics::set_global_recorder(BenchmarkRecorder)?;
    ENABLED.store(true, Ordering::Release);
    Ok(())
}

/// Whether [`install`] made this process a benchmark recorder.
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
/// classified [`INVALID`]. It does nothing unless the recorder is [`install`]ed or
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
    let Cohort(labels) = Cohort::of_headers(headers);
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

/// The terminal outcome of an [`Observation`] that completed, whatever
/// business result it carried.
pub const SUCCESS: &str = "success";

/// The terminal outcome of an [`Observation`] whose call failed.
pub const ERROR: &str = "error";

/// The terminal outcome of an [`Observation`] dropped before it finished.
pub const CANCELLED: &str = "cancelled";

/// The standard histogram an [`Observation`]'s components are recorded in,
/// labelled by `boundary`, `component`, `case`, and `phase`; each series'
/// count is the calls that reached that component.
pub const COMPONENT_SECONDS: &str = "wyrd_benchmark_component_seconds";

/// The standard histogram an [`Observation`]'s body sizes are recorded in,
/// in bytes, labelled by `boundary`, `body`, `case`, and `phase`; each
/// series' count is the calls that read that body.
pub const BODY_BYTES: &str = "wyrd_benchmark_body_bytes";

/// One closed benchmark classification: a case of [`CASES`] and a phase of
/// [`PHASES`], either [`UNLABELLED`] or [`INVALID`]. It selects series only
/// and carries no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cohort(Labels);

impl Cohort {
    /// No classification.
    pub const NONE: Self = Self(Labels::NONE);

    /// The cohort `case` and `phase` name, as [`label_request`] reads them
    /// from request metadata: an unknown value is [`INVALID`] and counted
    /// once in `invalid_labels` while [`install`]ed.
    #[must_use]
    pub fn of(case: Option<&str>, phase: Option<&str>) -> Self {
        let label = |value: Option<&str>, closed: &[&'static str]| {
            value.map_or(UNLABELLED, |value| {
                closed
                    .iter()
                    .find(|label| **label == value)
                    .copied()
                    .unwrap_or(INVALID)
            })
        };
        let labels = Labels {
            case: label(case, CASES),
            phase: label(phase, PHASES),
        };
        if enabled() && (labels.case == INVALID || labels.phase == INVALID) {
            REGISTRY.invalid_labels.fetch_add(1, Ordering::Relaxed);
        }
        Self(labels)
    }

    /// The cohort the benchmark metadata in `headers` declares, as
    /// [`label_request`] reads it: [`UNLABELLED`] where absent, and
    /// [`INVALID`], counted once in `invalid_labels` while [`install`]ed,
    /// where unknown.
    #[must_use]
    pub fn of_headers(headers: &http::HeaderMap) -> Self {
        let labels = Labels {
            case: Labels::label(headers.get(CASE_HEADER), CASES),
            phase: Labels::label(headers.get(PHASE_HEADER), PHASES),
        };
        if enabled() && (labels.case == INVALID || labels.phase == INVALID) {
            REGISTRY.invalid_labels.fetch_add(1, Ordering::Relaxed);
        }
        Self(labels)
    }

    /// The cohort [`label_request`] put on `span` or its nearest labelled
    /// ancestor, or [`Self::NONE`].
    #[must_use]
    pub fn of_span(span: &tracing::Span) -> Self {
        Self(labels_of(span))
    }

    /// The benchmark metadata that declares this cohort to another
    /// process, as [`label_request`] reads it: an [`UNLABELLED`] or
    /// [`INVALID`] label is not declared.
    #[must_use]
    pub fn headers(self) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        for (name, label) in [(CASE_HEADER, self.case()), (PHASE_HEADER, self.phase())] {
            if label != UNLABELLED && label != INVALID {
                headers.insert(name, http::HeaderValue::from_static(label));
            }
        }
        headers
    }

    /// The closed case label.
    #[must_use]
    pub const fn case(self) -> &'static str {
        self.0.case
    }

    /// The closed phase label.
    #[must_use]
    pub const fn phase(self) -> &'static str {
        self.0.phase
    }
}

/// One call's benchmark timing at a root `boundary`, with the components
/// it reached, recorded once when it ends.
///
/// [`Self::start`] costs one relaxed load and nothing else unless the
/// recorder is [`install`]ed: an inactive observation reads no clock,
/// allocates nothing, and records nothing. An active one records its
/// elapsed time since its start under its terminal outcome with
/// [`Self::finish`]; dropped unfinished, it records [`CANCELLED`], so a
/// cancelled call never disappears. Each component is recorded in
/// [`COMPONENT_SECONDS`] under the root's boundary and cohort, without an
/// outcome; one reached twice, such as authentication on a retry,
/// accumulates into one observation. A component never reached is absent,
/// not zero. The root and its components take only their own series'
/// locks, so a snapshot taken while a call ends may hold its root without
/// its components; a window differences snapshots taken once its cohort
/// drained, which hold both.
#[derive(Debug, Default)]
pub struct Observation(Option<Active>);

/// An [`install`]ed [`Observation`]'s state.
#[derive(Debug)]
struct Active {
    /// The root timing boundary.
    boundary: &'static str,
    /// Its classification.
    cohort: Cohort,
    /// When the call started.
    started: Instant,
    /// The excluded waits, for a boundary that has them.
    excluded: Option<Duration>,
    /// Reached components and their accumulated durations, in first-reached
    /// order.
    components: Vec<(&'static str, Duration)>,
    /// The terminal outcome, once finished.
    outcome: Option<&'static str>,
}

impl Observation {
    /// An observation of a call at `boundary` starting now in the cohort
    /// `cohort` computes; inactive, and `cohort` never called, unless
    /// [`install`]ed.
    #[must_use]
    pub fn start(boundary: &'static str, cohort: impl FnOnce() -> Cohort) -> Self {
        if !enabled() {
            return Self(None);
        }
        Self(Some(Active {
            boundary,
            cohort: cohort(),
            started: Instant::now(),
            excluded: None,
            components: Vec::new(),
            outcome: None,
        }))
    }

    /// Whether this observation records.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.0.is_some()
    }

    /// An observation of a nested call at `boundary` starting now in this
    /// one's cohort, such as one attempt of a retried call; inactive when
    /// this one is.
    #[must_use]
    pub fn child(&self, boundary: &'static str) -> Self {
        Self(self.0.as_ref().map(|active| Active {
            boundary,
            cohort: active.cohort,
            started: Instant::now(),
            excluded: None,
            components: Vec::new(),
            outcome: None,
        }))
    }

    /// Now, when active: the start of a component to pass to [`Self::since`].
    #[must_use]
    pub fn now(&self) -> Option<Instant> {
        self.0.as_ref().map(|_| Instant::now())
    }

    /// Adds `elapsed` to `component`.
    pub fn component(&mut self, component: &'static str, elapsed: Duration) {
        let Some(active) = &mut self.0 else {
            return;
        };
        match active
            .components
            .iter_mut()
            .find(|(name, _)| *name == component)
        {
            Some((_, total)) => *total += elapsed,
            None => active.components.push((component, elapsed)),
        }
    }

    /// Adds the time from `started`, a [`Self::now`] reading, until now to
    /// `component`; nothing without a start.
    pub fn since(&mut self, component: &'static str, started: Option<Instant>) {
        if let Some(started) = started {
            self.component(component, started.elapsed());
        }
    }

    /// Records `bytes` as the size of this call's `body`, such as the
    /// response body it read, in [`BODY_BYTES`] under the root's boundary
    /// and cohort; nothing when inactive.
    pub fn bytes(&self, body: &'static str, bytes: usize) {
        let Some(active) = &self.0 else {
            return;
        };
        let Labels { case, phase } = active.cohort.0;
        metrics::histogram!(
            BODY_BYTES,
            "boundary" => active.boundary,
            "body" => body,
            "case" => case,
            "phase" => phase
        )
        .record(bytes as f64);
    }

    /// Adds `wait` to the root's excluded waits, so the root records a
    /// residual.
    pub fn exclude(&mut self, wait: Duration) {
        if let Some(active) = &mut self.0 {
            *active.excluded.get_or_insert(Duration::ZERO) += wait;
        }
    }

    /// Records the call as ended now with `outcome`, [`SUCCESS`] or
    /// [`ERROR`].
    pub fn finish(mut self, outcome: &'static str) {
        if let Some(active) = &mut self.0 {
            active.outcome = Some(outcome);
        }
    }

    /// Records the call as ended now with `result`'s outcome: [`SUCCESS`]
    /// when it is `Ok`, whatever business result it carries, else
    /// [`ERROR`].
    pub fn settle<T, E>(self, result: &Result<T, E>) {
        self.finish(if result.is_ok() { SUCCESS } else { ERROR });
    }
}

impl Drop for Observation {
    /// Records the root and its components, [`CANCELLED`] when unfinished.
    fn drop(&mut self) {
        let Some(active) = self.0.take() else {
            return;
        };
        let elapsed = active.started.elapsed();
        let Labels { case, phase } = active.cohort.0;
        REGISTRY.record(
            (
                active.boundary,
                active.outcome.unwrap_or(CANCELLED),
                case,
                phase,
            ),
            elapsed,
            active.excluded,
        );
        for (component, duration) in active.components {
            metrics::histogram!(
                COMPONENT_SECONDS,
                "boundary" => active.boundary,
                "component" => component,
                "case" => case,
                "phase" => phase
            )
            .record(duration.as_secs_f64());
        }
    }
}

/// Records one request's server timing, and each of its `parts` as an
/// observation of the part's own boundary, under `parent`'s
/// classification, as one group no snapshot divides; see
/// [`record_timing_parts`]. Nothing is recorded unless [`install`]ed.
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
    let registry = &*REGISTRY;
    let key = (boundary, outcome, labels.case, labels.phase);
    if parts.is_empty() {
        registry.record(key, elapsed, excluded);
        return;
    }
    let _group = read(&registry.groups);
    registry.record(key, elapsed, excluded);
    for (part, duration) in parts {
        registry.record((part, outcome, labels.case, labels.phase), *duration, None);
    }
}

/// Records one query's executed-plan scan facts, rows and bytes when its
/// scans reported them, under `span`'s classification and the query's
/// terminal `outcome`. Nothing is recorded unless [`install`]ed; a missing
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
    REGISTRY.scan((outcome, labels.case, labels.phase), rows, bytes);
}

/// `mutex`'s guard, whatever a panicking recorder left: every update is a
/// whole increment, so a poisoned lock holds no half-written observation.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`'s shared guard, whatever a panicking recorder left; see [`locked`].
fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`'s exclusive guard, whatever a panicking recorder left; see
/// [`locked`].
fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
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

/// A new empty standard-metric value histogram, in billionths.
fn metric_values() -> Histogram<u64> {
    #[expect(
        clippy::expect_used,
        reason = "the constant bounds and precision are valid HDR parameters"
    )]
    Histogram::new_with_bounds(1, HIGHEST_METRIC, SIGNIFICANT_DIGITS)
        .expect("constant metric histogram bounds are valid")
}

/// `value` recorded into `histogram`, or `false` when it is out of range.
fn record_into(histogram: &mut Histogram<u64>, value: u64) -> bool {
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
struct Registry {
    /// When the registry started: the origin of snapshot times.
    started: Instant,
    /// This process's identity, fresh on every start.
    process: String,
    /// Held shared while a request records its timing parts and exclusively
    /// while a snapshot copies the timing series, so no snapshot divides a
    /// request's group.
    groups: RwLock<()>,
    /// Timing series.
    series: RwLock<BTreeMap<Key, Mutex<Series>>>,
    /// Scan series.
    scans: RwLock<BTreeMap<ScanKey, Mutex<Scan>>>,
    /// Observations refused because their series would exceed a bound.
    dropped: AtomicU64,
    /// Requests that declared an unknown case or phase.
    invalid_labels: AtomicU64,
    /// Standard metric series.
    metrics: RwLock<recorder::Metrics>,
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
    fn record(&self, key: Key, elapsed: Duration, excluded: Option<Duration>) {
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
    fn scan(&self, key: ScanKey, rows: Option<u64>, bytes: Option<u64>) {
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

/// Records one request's server timing at an owning boundary into the
/// bounded benchmark histograms under `parent`'s request classification.
///
/// `elapsed` is the owner's monotonic time from handler entry to `boundary`
/// (result ready, write acknowledged, or response handoff), and `excluded`
/// the union of the waits the owner does not count as its own work, such as
/// provider exchanges or Verifier blocks, when the boundary has any, with
/// the closed `boundary` and `outcome` labels. Nothing is recorded until
/// [`install`].
pub fn record_timing(
    parent: &tracing::Span,
    boundary: &'static str,
    outcome: &'static str,
    elapsed: Duration,
    excluded: Option<Duration>,
) {
    record(parent, boundary, outcome, elapsed, excluded, &[]);
}

/// Records one request's server timing as [`record_timing`] does, plus each
/// `(boundary, duration)` of `parts`: a component of the same request, such
/// as one execution phase, recorded as an observation of its own boundary
/// under the same outcome and classification.
///
/// The request and its parts are recorded as one group, so a snapshot
/// holds either all of them or none; each still takes only its own series'
/// lock, so unrelated requests never wait for each other.
pub fn record_timing_parts(
    parent: &tracing::Span,
    boundary: &'static str,
    outcome: &'static str,
    elapsed: Duration,
    excluded: Option<Duration>,
    parts: &[(&'static str, Duration)],
) {
    record(parent, boundary, outcome, elapsed, excluded, parts);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Labels, Registry, Series, durations};

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
        super::install().expect("the only recorder");
        assert!(super::enabled());
        super::install().expect("a repeated install is a no-op");
        let before = super::snapshot();
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
        super::record_timing(
            &tracing::Span::none(),
            "bench.request",
            "success",
            Duration::from_micros(300),
            Some(Duration::from_micros(100)),
        );
        let after = super::snapshot();
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
            super::IntervalValue::Gauge { value, peak }
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
                    super::IntervalValue::Histogram { unit, distribution }
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
            assert_eq!(unit, super::HistogramUnit::Billionths, "{name}");
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
        assert_eq!(super::HistogramUnit::Nanoseconds.per_unit(), 1_000_000_000);
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
            Labels::label(value("point-lookup").as_ref(), super::CASES),
            "point-lookup"
        );
        assert_eq!(
            Labels::label(value("select 1").as_ref(), super::CASES),
            super::INVALID
        );
        assert_eq!(Labels::label(None, super::PHASES), super::UNLABELLED);
    }

    /// A cohort's headers declare exactly its closed labels, which read back
    /// as the same cohort; an unlabelled or invalid label is not declared.
    ///
    /// # Panics
    ///
    /// Panics when a header is missing, extra, or reads back differently.
    #[test]
    fn cohort_headers_declare_only_closed_labels() {
        let cohort = super::Cohort::of(Some("direct-verify-model"), Some("repetition-3"));
        assert_eq!(super::Cohort::of_headers(&cohort.headers()), cohort);
        let unknown = super::Cohort::of(Some("select 1"), None);
        assert_eq!(
            (unknown.case(), unknown.phase()),
            (super::INVALID, super::UNLABELLED)
        );
        assert!(unknown.headers().is_empty());
        assert!(super::Cohort::NONE.headers().is_empty());
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
        for _ in 0..super::MAX_SERIES {
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
        assert_eq!(super::read(&registry.series).len(), super::MAX_SERIES);
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

#[cfg(test)]
mod recording_tests {
    use crate::{self as timing, CASE_HEADER, PHASE_HEADER};
    use std::time::Duration;
    /// Concurrent observations under labelled requests land exactly once in
    /// their own case and phase series, as does each recorded part under
    /// its own boundary; interval snapshots conserve counts,
    /// keep zero, bound each percentile by its bucket, and report
    /// out-of-range, negative, reset, and replaced-process evidence.
    ///
    /// # Panics
    ///
    /// Panics when the test pipeline does not initialize or any count,
    /// bound, or invalidity is wrong.
    #[test]
    fn benchmark_histograms_preserve_bounded_intervals() {
        let subscriber = tracing_subscriber::Registry::default();
        let _guard = tracing::subscriber::set_default(subscriber);
        let request = |case: &str, phase: &str| {
            let span = tracing::info_span!("request");
            let mut headers = http::HeaderMap::new();
            headers.insert(CASE_HEADER, case.parse().expect("header value"));
            headers.insert(PHASE_HEADER, phase.parse().expect("header value"));
            timing::label_request(&span, &headers);
            span
        };
        let unlabelled = tracing::info_span!("request");
        let ignored = request("point-lookup", "warmup");
        super::record_timing(&ignored, "bifrost.query", "success", Duration::ZERO, None);
        timing::install().expect("the only recorder");
        assert_eq!(
            timing::snapshot().series.len(),
            0,
            "nothing is recorded before install, and labels set then are ignored"
        );
        let started = timing::snapshot();

        let warmup = request("point-lookup", "warmup");
        for _ in 0..10 {
            super::record_timing(
                &warmup,
                "bifrost.query",
                "success",
                Duration::from_micros(3),
                None,
            );
        }
        let warm = timing::snapshot();
        let point = request("point-lookup", "repetition-1");
        let scan = request("full-scan", "repetition-1");
        std::thread::scope(|threads| {
            for thread in 0..8_u64 {
                let (point, scan, unlabelled) = (&point, &scan, &unlabelled);
                threads.spawn(move || {
                    for i in 0..1_000_u64 {
                        let elapsed = Duration::from_nanos(thread * 1_000 + i);
                        super::record_timing(point, "bifrost.query", "success", elapsed, None);
                        if i % 10 == 0 {
                            super::record_timing(scan, "bifrost.query", "success", elapsed, None);
                            super::record_timing(
                                unlabelled,
                                "bifrost.query",
                                "success",
                                elapsed,
                                None,
                            );
                        }
                    }
                });
            }
        });
        let gateway = request("buffered-1kib-capture-metadata", "repetition-1");
        super::record_timing(
            &gateway,
            "gateway.buffered",
            "success",
            Duration::from_micros(500),
            Some(Duration::from_micros(400)),
        );
        super::record_timing(
            &gateway,
            "gateway.buffered",
            "success",
            Duration::from_nanos(10),
            Some(Duration::from_nanos(11)),
        );
        super::record_timing(
            &gateway,
            "gateway.buffered",
            "success",
            Duration::from_secs(600),
            Some(Duration::ZERO),
        );
        let verify = request("direct-verify", "repetition-1");
        super::record_timing_parts(
            &verify,
            "verification.direct",
            "success",
            Duration::from_micros(300),
            Some(Duration::from_micros(10)),
            &[
                ("verification.direct.engine", Duration::from_micros(100)),
                ("verification.direct.stage", Duration::ZERO),
            ],
        );
        let _invalid = request("drop table", "repetition-1");
        let measured = timing::snapshot();

        let warmed = warm.since(&started).expect("same process");
        assert_eq!(warmed.series.len(), 1);
        assert_eq!(warmed.series[0].phase, "warmup");
        assert_eq!(warmed.series[0].count, 10);

        let interval = measured.since(&warm).expect("same process");
        let series = |case: &str, boundary: &str| {
            interval
                .series
                .iter()
                .find(|series| {
                    series.case == case
                        && series.boundary == boundary
                        && series.outcome == "success"
                })
                .unwrap_or_else(|| panic!("no {case} {boundary} series"))
        };
        let engine = series("direct-verify", "verification.direct.engine");
        let at = engine.elapsed.quantile(1.0).expect("one part");
        assert_eq!(
            engine.count, 1,
            "a part is one observation of its own boundary"
        );
        assert!(at.lowest <= 100_000 && 100_000 <= at.highest);
        assert!(engine.excluded.is_none() && engine.problems().is_empty());
        assert_eq!(
            series("direct-verify", "verification.direct.stage").count,
            1
        );
        assert_eq!(series("direct-verify", "verification.direct").count, 1);
        let point = series("point-lookup", "bifrost.query");
        assert_eq!(point.phase, "repetition-1");
        assert_eq!((point.count, point.elapsed.len()), (8_000, 8_000));
        assert!(point.problems().is_empty(), "{:?}", point.problems());
        assert_eq!(series("full-scan", "bifrost.query").count, 800);
        assert_eq!(series(timing::UNLABELLED, "bifrost.query").count, 800);
        assert!(
            interval
                .series
                .iter()
                .all(|series| series.phase != "warmup"),
            "warmup observations stay in the warmup interval"
        );
        let zero = point.elapsed.quantile(0.0).expect("observations");
        assert_eq!(
            (zero.lowest, zero.highest),
            (0, 0),
            "zero is counted exactly"
        );
        let p99 = point.elapsed.quantile(0.99).expect("observations");
        assert_eq!(p99.rank, 7_920);
        assert!(p99.lowest <= 7_919 && 7_919 <= p99.highest);
        assert!(p99.highest - p99.lowest <= 7_919 / 1_000);
        let whole = measured.since(&started).expect("same process");
        let total: u64 = whole.series.iter().map(|series| series.count).sum();
        assert_eq!(
            total,
            10 + 8_000 + 800 + 800 + 3 + 3,
            "each observation and part once"
        );

        let gateway = series("buffered-1kib-capture-metadata", "gateway.buffered");
        assert_eq!(gateway.count, 3);
        let residual = gateway.residual.as_ref().expect("residual component");
        assert_eq!(
            residual.len(),
            1,
            "only the in-range, non-negative residual"
        );
        let half = gateway.elapsed.quantile(0.0).expect("observation");
        assert!(half.lowest <= 10 && 10 <= half.highest);
        let problems = gateway.problems().join("; ");
        assert!(
            problems.contains("2 durations outside the trackable range"),
            "{problems}"
        );
        assert!(
            problems.contains("1 excluded waits exceeding elapsed time"),
            "{problems}"
        );
        assert!(
            problems.contains("2 elapsed observations for 3 requests"),
            "{problems}"
        );
        assert_eq!(
            interval.invalid_labels, 1,
            "the unknown declaration is counted once"
        );

        let mut replaced = measured.clone();
        replaced.process = "another".to_owned();
        assert!(replaced.since(&warm).is_err_and(|e| e.contains("replaced")));
        assert!(
            warm.since(&measured).is_err(),
            "a reversed interval is a reset"
        );
        let mut reset = measured.clone();
        reset.series.retain(|series| series.case != "point-lookup");
        assert!(reset.since(&warm).is_err(), "a vanished series is a reset");
    }

    /// Snapshots taken while threads record grouped request timings each
    /// hold every part of a group or none of it, and every component of a
    /// request with its elapsed time; once recording stops the totals are
    /// exact.
    ///
    /// # Panics
    ///
    /// Panics when a snapshot divides a group or a request's components, or
    /// a final count is wrong.
    #[test]
    fn snapshots_preserve_complete_timing_groups_during_recording() {
        const THREADS: u64 = 4;
        const REQUESTS: u64 = 2_000;
        timing::install().expect("the only recorder");
        let started = timing::snapshot();
        let count = |interval: &timing::BenchmarkInterval, boundary: &str| {
            interval
                .series
                .iter()
                .filter(|series| series.boundary == boundary)
                .map(|series| {
                    assert!(series.problems().is_empty(), "{:?}", series.problems());
                    series.count
                })
                .sum::<u64>()
        };
        let recording = std::sync::atomic::AtomicU64::new(THREADS);
        let cuts = std::thread::scope(|threads| {
            for thread in 0..THREADS {
                let recording = &recording;
                threads.spawn(move || {
                    for i in 0..REQUESTS {
                        let nanos = 1_000 + thread * REQUESTS + i;
                        super::record_timing_parts(
                            &tracing::Span::none(),
                            "bench.group",
                            "success",
                            Duration::from_nanos(nanos * 3),
                            Some(Duration::from_nanos(nanos)),
                            &[
                                ("bench.group.first", Duration::from_nanos(nanos)),
                                ("bench.group.second", Duration::from_nanos(nanos)),
                            ],
                        );
                    }
                    recording.fetch_sub(1, std::sync::atomic::Ordering::Release);
                });
            }
            let mut cuts = 0_u64;
            while recording.load(std::sync::atomic::Ordering::Acquire) > 0 {
                let interval = timing::snapshot().since(&started).expect("one process");
                let parent = count(&interval, "bench.group");
                assert_eq!(count(&interval, "bench.group.first"), parent);
                assert_eq!(count(&interval, "bench.group.second"), parent);
                cuts += 1;
            }
            cuts
        });
        assert!(cuts > 0, "at least one snapshot raced the recording");
        let interval = timing::snapshot().since(&started).expect("one process");
        for boundary in ["bench.group", "bench.group.first", "bench.group.second"] {
            assert_eq!(count(&interval, boundary), THREADS * REQUESTS, "{boundary}");
        }
    }
}

#[cfg(test)]
mod observation_tests {
    use std::time::Duration;

    use crate::{
        self as timing, BODY_BYTES, CANCELLED, COMPONENT_SECONDS, Cohort, ERROR, HistogramUnit,
        Observation, SUCCESS,
    };

    /// An observation records its root once under its terminal outcome,
    /// success, error, or cancellation when dropped unfinished, with its
    /// excluded wait and residual; each reached component accumulates into
    /// one observation per call under the root's boundary and cohort, and an
    /// unreached component stays absent rather than zero. An unknown label
    /// is counted invalid.
    ///
    /// # Panics
    ///
    /// Panics when a root, outcome, component count, or label is wrong.
    #[test]
    fn observations_keep_roots_outcomes_and_reached_components() {
        timing::install().expect("the only recorder");
        let before = timing::snapshot();
        let cohort = || Cohort::of(Some("direct-verify"), Some("repetition-1"));
        let mut completed = Observation::start("client.http", cohort);
        completed.component("auth", Duration::from_micros(5));
        completed.component("auth", Duration::from_micros(7));
        completed.since("decode", completed.now());
        completed.bytes("response", 1_234);
        completed.exclude(Duration::ZERO);
        completed
            .child("client.http.attempt")
            .settle(&Err::<(), ()>(()));
        completed
            .child("client.http.attempt")
            .settle(&Ok::<(), ()>(()));
        completed.finish(SUCCESS);
        let mut failed = Observation::start("client.http", cohort);
        failed.component("auth", Duration::from_micros(1));
        failed.finish(ERROR);
        drop(Observation::start("client.http", cohort));
        drop(Observation::start("client.http", || {
            Cohort::of(Some("no-such-case"), None)
        }));

        let interval = timing::snapshot().since(&before).expect("one process");
        let count = |outcome: &str, case: &str| {
            interval
                .series
                .iter()
                .filter(|s| s.boundary == "client.http" && s.outcome == outcome && s.case == case)
                .map(|s| s.count)
                .sum::<u64>()
        };
        assert_eq!(count(SUCCESS, "direct-verify"), 1);
        assert_eq!(count(ERROR, "direct-verify"), 1);
        assert_eq!(count(CANCELLED, "direct-verify"), 1);
        assert_eq!(count(CANCELLED, timing::INVALID), 1);
        assert_eq!(interval.invalid_labels, 1);
        let attempts = |outcome: &str| {
            interval
                .series
                .iter()
                .filter(|s| {
                    s.boundary == "client.http.attempt"
                        && s.outcome == outcome
                        && s.case == "direct-verify"
                })
                .map(|s| s.count)
                .sum::<u64>()
        };
        assert_eq!(
            (attempts(SUCCESS), attempts(ERROR)),
            (1, 1),
            "attempts keep their parent's cohort and their own outcomes"
        );
        let success = interval
            .series
            .iter()
            .find(|s| s.boundary == "client.http" && s.outcome == SUCCESS)
            .expect("the completed root");
        assert!(success.residual.is_some() && success.problems().is_empty());
        let component = |name: &str| {
            interval.histogram(
                COMPONENT_SECONDS,
                &[
                    ("boundary", "client.http"),
                    ("component", name),
                    ("case", "direct-verify"),
                    ("phase", "repetition-1"),
                ],
            )
        };
        let auth = component("auth").expect("auth was reached");
        assert_eq!(auth.len(), 2, "one accumulated observation per call");
        assert!(
            auth.maximum()
                .is_some_and(|bucket| bucket.highest >= 12_000 && bucket.lowest <= 12_000),
            "the retried call's auth accumulates"
        );
        assert_eq!(component("decode").expect("decode").len(), 1);
        assert!(
            component("send").is_err(),
            "an unreached component is absent"
        );
        let response = interval
            .histogram(
                BODY_BYTES,
                &[
                    ("boundary", "client.http"),
                    ("body", "response"),
                    ("case", "direct-verify"),
                    ("phase", "repetition-1"),
                ],
            )
            .expect("the read response body");
        let per = HistogramUnit::Billionths.per_unit();
        assert_eq!(response.len(), 1, "one body size per call that read it");
        assert!(
            response.maximum().is_some_and(
                |bucket| bucket.lowest <= 1_234 * per && bucket.highest >= 1_234 * per
            ),
            "the body size is recorded in bytes"
        );
    }

    /// Without the recorder an observation computes no cohort, reads no
    /// clock, and records nothing, finished or dropped.
    ///
    /// # Panics
    ///
    /// Panics when an inactive observation does any of these.
    #[test]
    fn inactive_observations_read_no_clock_and_record_nothing() {
        let before = timing::snapshot();
        let mut observation = Observation::start("client.http", || {
            panic!("an inactive observation computes no cohort")
        });
        assert!(!observation.active());
        assert_eq!(observation.now(), None);
        observation.component("auth", Duration::from_secs(1));
        observation.bytes("response", 9);
        assert!(!observation.child("client.http.attempt").active());
        observation.finish(SUCCESS);
        drop(Observation::start("client.http", || Cohort::NONE));
        let interval = timing::snapshot().since(&before).expect("one process");
        assert!(interval.series.is_empty() && interval.metrics.is_empty());
    }
}
