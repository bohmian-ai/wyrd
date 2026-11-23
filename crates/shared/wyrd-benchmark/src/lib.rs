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

#[cfg(feature = "bench")]
pub mod heap;
#[cfg(feature = "bench")]
mod recorder;
#[cfg(feature = "bench")]
mod registry;

#[cfg(feature = "bench")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tracing_subscriber::registry::LookupSpan as _;

#[cfg(feature = "bench")]
pub use recorder::{
    BenchmarkRecorder, HistogramUnit, IntervalMetric, IntervalValue, MAX_METRIC_SERIES,
    MetricSnapshot, MetricValue,
};
#[cfg(feature = "bench")]
pub use registry::{
    BenchmarkInterval, BenchmarkSnapshot, Buckets, Distribution, IntervalScan, IntervalSeries,
    Method, Quantile, ScanSnapshot, SeriesSnapshot, snapshot,
};
#[cfg(feature = "bench")]
use registry::{REGISTRY, read};

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
    "ui-sql",
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
#[cfg(feature = "bench")]
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Installs [`BenchmarkRecorder`] as this process's only standard `metrics`
/// recorder and starts recording benchmark timings. A benchmark server calls
/// it once at startup, before registering any metric; it cannot be turned
/// off, and a repeated call after it succeeded does nothing. It exists only
/// with the `bench` feature, so a build without it cannot record.
///
/// # Errors
///
/// Returns the refusal when another `metrics` recorder is already installed:
/// a process never mixes the benchmark recorder with another backend.
#[cfg(feature = "bench")]
pub fn install() -> Result<(), metrics::SetRecorderError<BenchmarkRecorder>> {
    if enabled() {
        return Ok(());
    }
    std::sync::LazyLock::force(&REGISTRY);
    metrics::set_global_recorder(BenchmarkRecorder)?;
    ENABLED.store(true, Ordering::Release);
    Ok(())
}

/// Whether [`install`] made this process a benchmark recorder. Without the
/// `bench` feature it is a constant `false`, so every site it guards compiles
/// away.
#[must_use]
#[inline]
pub fn enabled() -> bool {
    #[cfg(feature = "bench")]
    return ENABLED.load(Ordering::Acquire);
    #[cfg(not(feature = "bench"))]
    false
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
    #[cfg(not(feature = "bench"))]
    let _ = (span, headers);
    #[cfg(feature = "bench")]
    label_bench_request(span, headers);
}

/// [`label_request`] of a `bench` build.
#[cfg(feature = "bench")]
fn label_bench_request(span: &tracing::Span, headers: &http::HeaderMap) {
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
        Self::counted(labels)
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
        Self::counted(labels)
    }

    /// `labels` as a cohort, an invalid one counted in `invalid_labels`
    /// while [`install`]ed.
    fn counted(labels: Labels) -> Self {
        #[cfg(feature = "bench")]
        if enabled() && (labels.case == INVALID || labels.phase == INVALID) {
            REGISTRY
                .invalid_labels
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
///
/// Without the `bench` feature it is a stateless no-op of the same
/// methods, so a shipped build holds no observation state, clock, or lock.
#[derive(Debug, Default)]
pub struct Observation(#[cfg(feature = "bench")] Option<Active>);

/// An [`install`]ed [`Observation`]'s state.
#[cfg(feature = "bench")]
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

#[cfg(feature = "bench")]
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

#[cfg(feature = "bench")]
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

/// The stateless observation of a build without the `bench` feature: every
/// method does nothing and reads no clock.
#[cfg(not(feature = "bench"))]
impl Observation {
    /// No observation: `cohort` is never called.
    #[must_use]
    pub fn start(_boundary: &'static str, _cohort: impl FnOnce() -> Cohort) -> Self {
        Self()
    }

    /// Always `false`.
    #[must_use]
    pub const fn active(&self) -> bool {
        false
    }

    /// No observation.
    #[must_use]
    pub const fn child(&self, _boundary: &'static str) -> Self {
        Self()
    }

    /// Always `None`.
    #[must_use]
    pub const fn now(&self) -> Option<Instant> {
        None
    }

    /// Does nothing.
    pub const fn component(&mut self, _component: &'static str, _elapsed: Duration) {}

    /// Does nothing.
    pub const fn since(&mut self, _component: &'static str, _started: Option<Instant>) {}

    /// Does nothing.
    pub const fn bytes(&self, _body: &'static str, _bytes: usize) {}

    /// Does nothing.
    pub const fn exclude(&mut self, _wait: Duration) {}

    /// Does nothing.
    pub const fn finish(self, _outcome: &'static str) {}

    /// Does nothing.
    pub const fn settle<T, E>(self, _result: &Result<T, E>) {}
}

/// Records one request's server timing, and each of its `parts` as an
/// observation of the part's own boundary, under `parent`'s
/// classification, as one group no snapshot divides; see
/// [`record_timing_parts`]. Nothing is recorded unless [`install`]ed.
#[cfg(feature = "bench")]
fn record(
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
    #[cfg(not(feature = "bench"))]
    let _ = (span, outcome, rows, bytes);
    #[cfg(feature = "bench")]
    if enabled() {
        let labels = labels_of(span);
        REGISTRY.scan((outcome, labels.case, labels.phase), rows, bytes);
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
    record_timing_parts(parent, boundary, outcome, elapsed, excluded, &[]);
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
    #[cfg(not(feature = "bench"))]
    let _ = (parent, boundary, outcome, elapsed, excluded, parts);
    #[cfg(feature = "bench")]
    record(parent, boundary, outcome, elapsed, excluded, parts);
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
