//! Measurement owners shared by the opt-in capacity benchmark binaries.
//!
//! Every benchmark under `src/bin/` launches the release `wyrd-server` beside
//! itself as a plain child, reads `WYRD_LOG` for client-side tracing, windows
//! the server process's CPU and sampled peak memory, and collects the server's
//! exact per-request timings. Those pieces live here once so each workload
//! owns only its production calls; [`crate::experiment`] owns judgment and
//! history.
//!
//! Judged latency is server-side only: the [`Collector`] receives the
//! server's `wyrd.timing` spans over OTLP, each one request's exact integer
//! nanoseconds from handler entry to its result, acknowledgement, or
//! response handoff, plus the union of that request's own provider or
//! Verifier waits; percentiles are nearest rank over those raw samples
//! ([`nearest_rank`]), never bucket estimates. Server histograms remain only
//! as diagnostic means and component breakdowns.
//!
//! A workload names the diagnostic histograms and runtime components it reads
//! in one [`Selection`]; a [`Window`] reads them, the server's CPU and peak
//! memory, and this process's CPU at its open and differences them into a
//! [`Usage`] at its close. A reading the OS cannot supply, a counter that went
//! backwards, or a lost process is `None`, never zero use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use num_traits::ToPrimitive;
use serde::Serialize;
use serde_json::Value;
use wyrd_tonic::otlp::common::v1::{KeyValue, any_value};
use wyrd_tonic::otlp::trace_service::trace_service_server::{TraceService, TraceServiceServer};
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};

use crate::experiment::{Reading, Repetition};
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
    /// CPU seconds the [`Collector`] process used, to show collection was
    /// not saturated; `None` when the process was lost or unreadable.
    pub collector_cpu_seconds: Option<f64>,
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
    driver_cpu_seconds: Option<f64>,
    /// The collector process and its CPU microseconds at start.
    collector: (u32, Option<u64>),
}

impl Window {
    /// Opens a window over `server` and the `collector` process reading
    /// `selection`, and returns the opening scrape.
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    pub async fn open(
        server: &LocalServer,
        collector: &Collector,
        selection: Selection,
    ) -> Result<(Self, Metrics), Error> {
        let metrics = server.metrics().await?;
        let window = Self {
            resources: ResourceWindow::open([server]),
            readings: Readings::read(&metrics, selection),
            driver_cpu_seconds: driver_cpu_seconds(),
            collector: (collector.pid(), collector.cpu_usec()),
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
        let seconds = self.started.elapsed().as_secs_f64();
        let metrics = server.metrics().await?;
        let resources = self.resources.finish([server], seconds);
        let (server_cpu_seconds, peak_memory_bytes) = resources
            .first()
            .map_or((None, None), |used| (used.cpu_seconds, used.peak_memory));
        let usage = Usage {
            seconds,
            server_cpu_seconds,
            peak_memory_bytes,
            driver_cpu_seconds: elapsed(self.driver_cpu_seconds, driver_cpu_seconds()),
            collector_cpu_seconds: usec_elapsed(
                self.collector.1,
                crate::release_server::os::process_cpu_usec(self.collector.0),
            ),
            telemetry: self.readings.since(&metrics),
        };
        Ok((usage, metrics))
    }
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

/// The span the server records each request's exact timing on; see
/// `wyrd_telemetry::record_timing`.
pub const TIMING_SPAN: &str = "wyrd.timing";

/// How long the server's batch export may wait before it sends, as the
/// declared `OTEL_BSP_SCHEDULE_DELAY`.
pub const EXPORT_DELAY: Duration = Duration::from_millis(100);

/// How long a window's timing observations may take to reach the collector
/// after its last request ended.
const COLLECTION_WAIT: Duration = Duration::from_secs(30);

/// How long a smoke's handful of timing observations may take to reach the
/// collector: a few export delays, so missing evidence fails promptly.
const SMOKE_WAIT: Duration = Duration::from_secs(3);

/// The least time a smoke keeps sending, so its server CPU outgrows the
/// process counters' tick resolution.
pub const SMOKE_WINDOW: Duration = Duration::from_secs(1);

/// One request's server timing at one boundary, decoded from a
/// [`TIMING_SPAN`] the server exported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Timing {
    /// The owning boundary, such as `gateway.buffered` or `bifrost.query`.
    pub boundary: String,
    /// The closed outcome the server recorded.
    pub outcome: String,
    /// Monotonic nanoseconds from handler entry to the boundary.
    pub elapsed_ns: u64,
    /// The union of excluded waits (provider exchanges, Verifier blocks),
    /// nanoseconds, when the boundary has any.
    pub excluded_ns: Option<u64>,
    /// When the span ended, Unix nanoseconds: assigns it to a window only.
    pub end_unix_ns: u64,
    /// Hex trace id: the request's trace.
    pub trace_id: String,
    /// Hex span id, unique per observation.
    pub span_id: String,
    /// The exporting server process's `service.instance.id`.
    pub instance: String,
}

impl Timing {
    /// Elapsed less excluded nanoseconds, computed within this one request.
    ///
    /// # Errors
    ///
    /// Returns why there is no residual: no excluded component, or one
    /// larger than the elapsed time. A negative residual is never clamped.
    pub fn residual_ns(&self) -> Result<u64, String> {
        let excluded = self.excluded_ns.ok_or_else(|| {
            format!(
                "a `{}` observation has no excluded component",
                self.boundary
            )
        })?;
        self.elapsed_ns.checked_sub(excluded).ok_or_else(|| {
            format!(
                "a `{}` observation's excluded {excluded} ns exceeds its elapsed {} ns",
                self.boundary, self.elapsed_ns
            )
        })
    }
}

/// Every [`TIMING_SPAN`] in one OTLP trace export, with a reason for each
/// one that cannot be decoded, and the number of spans it carried.
#[must_use]
pub fn decode_timings(request: &ExportTraceServiceRequest) -> (Vec<Timing>, Vec<String>, u64) {
    let (mut timings, mut malformed, mut spans) = (Vec::new(), Vec::new(), 0_u64);
    for resource in &request.resource_spans {
        let instance = resource
            .resource
            .as_ref()
            .and_then(|resource| text(&resource.attributes, "service.instance.id"))
            .unwrap_or_default();
        for span in resource.scope_spans.iter().flat_map(|scope| &scope.spans) {
            spans += 1;
            if span.name != TIMING_SPAN {
                continue;
            }
            let decoded = (|| {
                let boundary = text(&span.attributes, "boundary").ok_or("no boundary")?;
                let outcome = text(&span.attributes, "outcome").ok_or("no outcome")?;
                let elapsed_ns = nanos(&span.attributes, "elapsed_ns")?.ok_or("no elapsed_ns")?;
                Ok::<_, &str>(Timing {
                    boundary,
                    outcome,
                    elapsed_ns,
                    excluded_ns: nanos(&span.attributes, "excluded_ns")?,
                    end_unix_ns: span.end_time_unix_nano,
                    trace_id: hex(&span.trace_id),
                    span_id: hex(&span.span_id),
                    instance: instance.clone(),
                })
            })();
            match decoded {
                Ok(timing) => timings.push(timing),
                Err(reason) => {
                    malformed.push(format!("a `{TIMING_SPAN}` span is malformed: {reason}"));
                }
            }
        }
    }
    (timings, malformed, spans)
}

/// The string attribute `key`.
fn text(attributes: &[KeyValue], key: &str) -> Option<String> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(
            |attribute| match attribute.value.as_ref()?.value.as_ref()? {
                any_value::Value::StringValue(value) => Some(value.clone()),
                _ => None,
            },
        )
}

/// The non-negative integer attribute `key`, `None` when absent.
///
/// # Errors
///
/// Returns why a present value is not a non-negative integer.
fn nanos(attributes: &[KeyValue], key: &str) -> Result<Option<u64>, &'static str> {
    let Some(value) = attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| attribute.value.as_ref()?.value.as_ref())
    else {
        return Ok(None);
    };
    match value {
        any_value::Value::IntValue(value) => u64::try_from(*value)
            .map(Some)
            .map_err(|_| "a negative duration"),
        _ => Err("a non-integer duration"),
    }
}

/// Lower-case hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
        out
    })
}

/// The largest OTLP export request the collector decodes; a larger one is
/// refused with `RESOURCE_EXHAUSTED`, which the server's exporter counts as
/// lost spans and the waiting window reports as missing observations.
pub const MAX_EXPORT_BYTES: usize = 64 << 20;

/// The most unassigned timing observations the collector retains; any
/// beyond it are dropped and counted against their boundary.
pub const RETAINED_TIMINGS: usize = 2_000_000;

/// The most malformed-span reasons the collector keeps verbatim; any beyond
/// it are only counted.
pub const KEPT_MALFORMED: usize = 16;

/// Worker threads of the collector process: its declared CPU budget.
pub const COLLECTOR_THREADS: usize = 2;

/// One line the collector process writes to its stdout.
#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Report {
    /// The first line: where the collector listens.
    Listening {
        /// The `http://` endpoint.
        endpoint: String,
    },
    /// One decoded export request.
    Export {
        /// Its timing observations.
        timings: Vec<Timing>,
        /// Why each undecodable timing span was refused.
        malformed: Vec<String>,
        /// Spans of every name it carried.
        spans: u64,
    },
}

/// What the collector has received and not yet assigned to a window, within
/// [`RETAINED_TIMINGS`] and [`KEPT_MALFORMED`].
#[derive(Debug)]
struct Received {
    /// Unassigned timing observations, in arrival order.
    timings: Vec<Timing>,
    /// Undecodable timing spans since the last window, at most
    /// [`KEPT_MALFORMED`].
    malformed: Vec<String>,
    /// Undecodable timing spans beyond [`KEPT_MALFORMED`] since the last
    /// window.
    unkept: u64,
    /// Per boundary, observations dropped beyond [`RETAINED_TIMINGS`] and
    /// the latest end among them, Unix nanoseconds, until a window reading
    /// that boundary reports them.
    dropped: BTreeMap<String, (u64, u64)>,
    /// Why the collector process stopped reporting, once it did.
    lost: Option<String>,
    /// Export requests received in total.
    exports: u64,
    /// Spans received in total, of every name.
    spans: u64,
    /// The most unassigned observations kept: [`RETAINED_TIMINGS`].
    retain: usize,
}

impl Default for Received {
    /// Nothing received, retaining at most [`RETAINED_TIMINGS`].
    fn default() -> Self {
        Self {
            timings: Vec::new(),
            malformed: Vec::new(),
            unkept: 0,
            dropped: BTreeMap::new(),
            lost: None,
            exports: 0,
            spans: 0,
            retain: RETAINED_TIMINGS,
        }
    }
}

impl Received {
    /// Keeps one decoded export within the retention bounds.
    fn keep(&mut self, timings: Vec<Timing>, malformed: Vec<String>, spans: u64) {
        for timing in timings {
            if self.timings.len() < self.retain {
                self.timings.push(timing);
            } else {
                let (count, latest) = self.dropped.entry(timing.boundary).or_default();
                *count += 1;
                *latest = (*latest).max(timing.end_unix_ns);
            }
        }
        for reason in malformed {
            if self.malformed.len() < KEPT_MALFORMED {
                self.malformed.push(reason);
            } else {
                self.unkept += 1;
            }
        }
        self.exports += 1;
        self.spans += spans;
    }
}

/// Where the collector reports: its stdout, or a test's pipe.
type Sink = Arc<Mutex<dyn std::io::Write + Send>>;

/// The collector process's OTLP trace service.
struct Intake(Sink);

#[async_trait::async_trait]
impl TraceService for Intake {
    /// Decodes every timing span of one export, reports it as one line, and
    /// acknowledges it whole once the line is written.
    async fn export(
        &self,
        request: wyrd_tonic::tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<wyrd_tonic::tonic::Response<ExportTraceServiceResponse>, wyrd_tonic::tonic::Status>
    {
        let (timings, malformed, spans) = decode_timings(request.get_ref());
        report(
            &self.0,
            &Report::Export {
                timings,
                malformed,
                spans,
            },
        )
        .map_err(|error| wyrd_tonic::tonic::Status::unavailable(error.to_string()))?;
        Ok(wyrd_tonic::tonic::Response::new(
            ExportTraceServiceResponse::default(),
        ))
    }
}

/// Writes `line` to `sink` as one JSON line and flushes it.
///
/// # Errors
///
/// Returns the write failure, such as a driver that stopped reading.
fn report(sink: &Sink, line: &Report) -> std::io::Result<()> {
    let mut sink = sink.lock().unwrap_or_else(PoisonError::into_inner);
    let mut encoded = serde_json::to_vec(line)?;
    encoded.push(b'\n');
    sink.write_all(&encoded)?;
    sink.flush()
}

/// Listens on a free loopback port, reports it as the first line to `sink`,
/// then serves OTLP trace exports of at most [`MAX_EXPORT_BYTES`], reporting
/// each as one line, until `stop` resolves.
///
/// # Errors
///
/// Returns the bind, report, or serve failure.
async fn serve(sink: Sink, stop: impl Future<Output = ()> + Send) -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    report(
        &sink,
        &Report::Listening {
            endpoint: format!("http://{}", listener.local_addr()?),
        },
    )?;
    wyrd_tonic::tonic::transport::Server::builder()
        .add_service(
            TraceServiceServer::new(Intake(sink)).max_decoding_message_size(MAX_EXPORT_BYTES),
        )
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            stop,
        )
        .await?;
    Ok(())
}

/// Runs the collector process ([`serve`] reporting to stdout) until stdin
/// closes. [`Collector::start`] runs this as `<program> collect` on a
/// [`COLLECTOR_THREADS`]-worker runtime.
///
/// # Errors
///
/// Returns the bind, report, or serve failure.
pub async fn serve_collector() -> Result<(), Error> {
    let closed = tokio::task::spawn_blocking(|| {
        let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
    });
    let sink: Sink = Arc::new(Mutex::new(std::io::stdout()));
    serve(sink, async {
        let _ = closed.await;
    })
    .await
}

/// A local OTLP/gRPC trace collector the measured server exports to.
///
/// It is a declared benchmark helper: a plain child process of the driver
/// (`<program> collect`, [`serve_collector`]) that receives and decodes the
/// server's export, so its CPU is neither driver nor server CPU and is
/// reported on its own ([`Usage::collector_cpu_seconds`]). As a descendant
/// of the driver, quiet-host checks exclude it. The server's tracing and
/// export cost stays in server CPU. The driver keeps only [`TIMING_SPAN`]s,
/// bounded by [`RETAINED_TIMINGS`], and hands each window the observations
/// that ended inside it once every expected one arrived; every assigned
/// observation is appended to `timings.jsonl` under the run's artifacts.
pub struct Collector {
    /// The `http://` endpoint the server exports to.
    endpoint: String,
    /// What arrived and is unassigned.
    received: Arc<Mutex<Received>>,
    /// Raw observation file under the artifacts directory.
    log: PathBuf,
    /// The collector process, or `None` for a test's in-process intake;
    /// closing its stdin stops it, and dropping the collector kills it.
    child: Option<std::process::Child>,
    /// Stops a test's in-process intake on drop.
    #[cfg(test)]
    stop: Option<tokio_util::sync::DropGuard>,
}

impl Collector {
    /// Starts the collector process from this program and keeps raw
    /// observations under `artifacts`, with its stderr in
    /// `collector.stderr` there.
    ///
    /// # Errors
    ///
    /// Returns the artifacts-directory, spawn, or first-report failure.
    #[cfg(not(test))]
    pub async fn start(artifacts: &Path) -> Result<Self, Error> {
        Self::spawn(&std::env::current_exe()?, artifacts).await
    }

    /// Runs the same intake inside this test process over a pipe, since a
    /// test harness cannot be re-run as `collect`; its CPU is this process's.
    ///
    /// # Errors
    ///
    /// Returns the artifacts-directory, pipe, or first-report failure.
    #[cfg(test)]
    pub async fn start(artifacts: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(artifacts)?;
        let (reader, writer) = std::io::pipe()?;
        let stop = tokio_util::sync::CancellationToken::new();
        let sink: Sink = Arc::new(Mutex::new(writer));
        tokio::spawn(serve(sink, stop.clone().cancelled_owned()));
        let mut collector = Self::attach(reader, artifacts, None).await?;
        collector.stop = Some(stop.drop_guard());
        Ok(collector)
    }

    /// Starts `program collect` as the collector process.
    ///
    /// # Errors
    ///
    /// Returns the artifacts-directory, spawn, or first-report failure.
    pub async fn spawn(program: &Path, artifacts: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(artifacts)?;
        let mut child = std::process::Command::new(program)
            .arg("collect")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::fs::File::create(artifacts.join("collector.stderr"))?)
            .spawn()?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("the collector process has no stdout".into());
        };
        Self::attach(stdout, artifacts, Some(child)).await
    }

    /// Reads the collector's first line, its endpoint, within 10 s, then
    /// keeps every later line on a reader thread until `reports` closes or
    /// fails, which marks the collector lost.
    ///
    /// # Errors
    ///
    /// Returns the read failure or a first line that is not the endpoint;
    /// `child` is then killed.
    async fn attach(
        reports: impl std::io::Read + Send + 'static,
        artifacts: &Path,
        child: Option<std::process::Child>,
    ) -> Result<Self, Error> {
        use std::io::BufRead as _;
        let mut collector = Self {
            endpoint: String::new(),
            received: Arc::new(Mutex::new(Received::default())),
            log: artifacts.join("timings.jsonl"),
            child,
            #[cfg(test)]
            stop: None,
        };
        let mut lines = std::io::BufReader::new(reports).lines();
        let first = tokio::task::spawn_blocking(move || (lines.next(), lines));
        let (first, lines) = tokio::time::timeout(Duration::from_secs(10), first)
            .await
            .map_err(|_| "the collector did not report its endpoint within 10 s")??;
        match first.transpose()?.map(|line| serde_json::from_str(&line)) {
            Some(Ok(Report::Listening { endpoint })) => collector.endpoint = endpoint,
            other => {
                return Err(format!("the collector did not report its endpoint: {other:?}").into());
            }
        }
        let shared = Arc::clone(&collector.received);
        std::thread::spawn(move || {
            let lost = (|| -> Result<(), String> {
                for line in lines {
                    let line = line.map_err(|error| error.to_string())?;
                    match serde_json::from_str(&line) {
                        Ok(Report::Export {
                            timings,
                            malformed,
                            spans,
                        }) => shared
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .keep(timings, malformed, spans),
                        other => return Err(format!("an unexpected report {other:?}")),
                    }
                }
                Err("its output closed".to_owned())
            })();
            if let Err(reason) = lost {
                shared.lock().unwrap_or_else(PoisonError::into_inner).lost =
                    Some(format!("the timing collector stopped reporting: {reason}"));
            }
        });
        Ok(collector)
    }

    /// PID of the collector process; this process's for a test's
    /// in-process intake.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child
            .as_ref()
            .map_or_else(std::process::id, std::process::Child::id)
    }

    /// Microseconds of user plus system CPU the collector process has used,
    /// or `None` once it is gone or unreadable, so a lost collector never
    /// reads as idle.
    #[must_use]
    pub fn cpu_usec(&self) -> Option<u64> {
        crate::release_server::os::process_cpu_usec(self.pid())
    }

    /// The server environment that exports every span to this collector:
    /// the operator OTLP endpoint, a sampling ratio of one, and a batch
    /// queue large enough that a window's spans are never dropped. A
    /// record declares these as its tracing configuration.
    #[must_use]
    pub fn server_env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("WYRD_OTLP_ENDPOINT", self.endpoint.clone()),
            ("WYRD_OTLP_SAMPLE_RATIO", "1.0".to_owned()),
            ("OTEL_BSP_MAX_QUEUE_SIZE", "1048576".to_owned()),
            ("OTEL_BSP_MAX_EXPORT_BATCH_SIZE", "8192".to_owned()),
            (
                "OTEL_BSP_SCHEDULE_DELAY",
                EXPORT_DELAY.as_millis().to_string(),
            ),
        ]
    }

    /// The declared tracing configuration and collection method a record
    /// keeps, without the collector's ephemeral port.
    #[must_use]
    pub fn declaration() -> Value {
        serde_json::json!({
            "method": "server wyrd.timing spans: exact integer nanoseconds from the owning server boundary's std::time::Instant timers, exported through the operator OTLP/gRPC span exporter to a collector helper process the driver starts",
            "clock": "std::time::Instant on the server: the OS monotonic clock",
            "server_env": {
                "WYRD_OTLP_SAMPLE_RATIO": "1.0",
                "OTEL_BSP_MAX_QUEUE_SIZE": "1048576",
                "OTEL_BSP_MAX_EXPORT_BATCH_SIZE": "8192",
                "OTEL_BSP_SCHEDULE_DELAY": EXPORT_DELAY.as_millis().to_string(),
            },
            "cost": "the server's tracing and export CPU is in server CPU; the collector helper's CPU is reported separately, in neither server nor driver CPU",
            "collector": {
                "worker_threads": COLLECTOR_THREADS,
                "max_export_bytes": MAX_EXPORT_BYTES,
                "retained_timings": RETAINED_TIMINGS,
                "kept_malformed": KEPT_MALFORMED,
                "host_checks": "excluded as a descendant of the driver",
            },
            "assignment": "a window takes the observations that ended after it opened, once its expected count arrived (and, for traced requests, each request's own observation by trace id); a missing, extra, duplicate, or malformed observation makes it invalid",
        })
    }

    /// Waits until `expected` observations of each boundary ended at or
    /// after `opened`, then takes every observation of those boundaries
    /// that did and drops whatever else is unassigned: earlier windows'
    /// stragglers and boundaries this window does not read.
    ///
    /// After the counts are reached the collector waits two more export
    /// delays, so a late duplicate is counted rather than left for the next
    /// window. The population is invalid when a count is short after
    /// [`COLLECTION_WAIT`], over, or a span id repeats, or a timing span was
    /// malformed. Taken observations are appended to the raw file.
    pub async fn window(
        &self,
        label: &str,
        opened: SystemTime,
        expected: &[(&str, u64)],
    ) -> Population {
        self.collect(label, opened, expected, true, &[], COLLECTION_WAIT)
            .await
    }

    /// [`Self::window`] for a smoke's requests, waiting at most
    /// [`SMOKE_WAIT`] for a short count, and requiring each of `traces` to
    /// have exactly one observation.
    pub async fn smoke(
        &self,
        label: &str,
        opened: SystemTime,
        expected: &[(&str, u64)],
        traces: &[&str],
    ) -> Population {
        self.collect(label, opened, expected, true, traces, SMOKE_WAIT)
            .await
    }

    /// [`Self::window`] for a window whose boundaries also serve requests
    /// the driver did not send itself: waits until at least `least`
    /// observations of each boundary ended at or after `opened` and every
    /// trace id of `traces` has one, then takes every observation of those
    /// boundaries that did. The population is invalid when a count is short
    /// or a trace has no observation after [`COLLECTION_WAIT`], a trace has
    /// more than one, a span id repeats, or a timing span was malformed.
    pub async fn traced(
        &self,
        label: &str,
        opened: SystemTime,
        least: &[(&str, u64)],
        traces: &[&str],
    ) -> Population {
        self.collect(label, opened, least, false, traces, COLLECTION_WAIT)
            .await
    }

    /// Waits up to `wait` for and takes a window's observations: `expected`
    /// counts per boundary, exact when `exact` and lower bounds otherwise,
    /// and one observation at least for each of `traces`.
    async fn collect(
        &self,
        label: &str,
        opened: SystemTime,
        expected: &[(&str, u64)],
        exact: bool,
        traces: &[&str],
        wait: Duration,
    ) -> Population {
        let from = unix_nanos(opened);
        let matching = |timing: &Timing| {
            timing.end_unix_ns >= from
                && expected
                    .iter()
                    .any(|(boundary, _)| *boundary == timing.boundary)
        };
        let counts = |received: &Received| {
            expected
                .iter()
                .map(|(boundary, _)| {
                    received
                        .timings
                        .iter()
                        .filter(|timing| matching(timing) && timing.boundary == *boundary)
                        .count() as u64
                })
                .collect::<Vec<_>>()
        };
        let unseen = |received: &Received| {
            if traces.is_empty() {
                return 0;
            }
            let seen: std::collections::HashSet<&str> = received
                .timings
                .iter()
                .filter(|timing| matching(timing))
                .map(|timing| timing.trace_id.as_str())
                .collect();
            traces
                .iter()
                .filter(|trace| !seen.contains(**trace))
                .count()
        };
        let waiting = tokio::time::Instant::now();
        while waiting.elapsed() < wait {
            let reached = {
                let received = self.received.lock().unwrap_or_else(PoisonError::into_inner);
                counts(&received)
                    .iter()
                    .zip(expected)
                    .all(|(count, (_, want))| count >= want)
                    && unseen(&received) == 0
            };
            if reached {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(EXPORT_DELAY * 2).await;
        let mut received = self.received.lock().unwrap_or_else(PoisonError::into_inner);
        let mut invalid = std::mem::take(&mut received.malformed);
        let unkept = std::mem::take(&mut received.unkept);
        if unkept > 0 {
            invalid.push(format!(
                "{unkept} more `{TIMING_SPAN}` spans are malformed, beyond the {KEPT_MALFORMED} kept"
            ));
        }
        invalid.extend(received.lost.clone());
        for (boundary, _) in expected {
            if let Some((count, latest)) = received.dropped.remove(*boundary)
                && latest >= from
            {
                invalid.push(format!(
                    "{count} `{boundary}` timing observations overflowed the collector's {}-observation retention and were dropped",
                    received.retain
                ));
            }
        }
        for (count, (boundary, want)) in counts(&received).iter().zip(expected) {
            if count < want || (exact && count != want) {
                invalid.push(format!(
                    "{count} `{boundary}` timing observations arrived for {want} requests"
                ));
            }
        }
        let missing = unseen(&received);
        if missing > 0 {
            invalid.push(format!(
                "{missing} of {} traced requests have no server timing observation",
                traces.len()
            ));
        }
        let (timings, _): (Vec<Timing>, Vec<Timing>) = std::mem::take(&mut received.timings)
            .into_iter()
            .partition(|timing| matching(timing));
        let (exports, spans) = (received.exports, received.spans);
        drop(received);
        if !traces.is_empty() {
            let wanted: std::collections::HashSet<&str> = traces.iter().copied().collect();
            let mut seen: BTreeMap<&str, u64> = BTreeMap::new();
            for timing in &timings {
                if wanted.contains(timing.trace_id.as_str()) {
                    *seen.entry(timing.trace_id.as_str()).or_default() += 1;
                }
            }
            let repeated = seen.values().filter(|count| **count > 1).count();
            if repeated > 0 {
                invalid.push(format!(
                    "{repeated} of {} traced requests have more than one server timing observation",
                    traces.len()
                ));
            }
        }
        let mut ids: Vec<&str> = timings
            .iter()
            .map(|timing| timing.span_id.as_str())
            .collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        if ids.len() != unique {
            invalid.push(format!(
                "{} timing observations repeat a span id",
                unique - ids.len()
            ));
        }
        if let Err(error) = self.append(label, &timings) {
            invalid.push(format!("raw timings could not be kept: {error}"));
        }
        Population {
            timings,
            invalid,
            exports,
            spans,
        }
    }

    /// Appends `timings`, each tagged with the window `label`, to the raw
    /// file.
    ///
    /// # Errors
    ///
    /// Returns the open or write failure.
    fn append(&self, label: &str, timings: &[Timing]) -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)?;
        for timing in timings {
            let line = serde_json::json!({ "window": label, "timing": timing });
            writeln!(file, "{line}")?;
        }
        file.flush()
    }
}

impl Drop for Collector {
    /// Kills and reaps the collector process.
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// `at` as Unix nanoseconds, the clock of [`Timing::end_unix_ns`]; zero
/// before the epoch.
#[must_use]
pub fn unix_nanos(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
        })
}

/// One window's server timing observations.
#[derive(Debug, Clone, Default)]
pub struct Population {
    /// Every observation of the window's boundaries.
    pub timings: Vec<Timing>,
    /// Why the population is incomplete or untrustworthy; empty when it is
    /// complete.
    pub invalid: Vec<String>,
    /// Export requests the collector had received so far.
    pub exports: u64,
    /// Spans of every name the collector had received so far.
    pub spans: u64,
}

impl Population {
    /// The elapsed nanoseconds of every `boundary` observation with
    /// `outcome`.
    #[must_use]
    pub fn elapsed(&self, boundary: &str, outcome: &str) -> Vec<u64> {
        self.of(boundary, outcome)
            .map(|timing| timing.elapsed_ns)
            .collect()
    }

    /// The excluded-wait nanoseconds of every `boundary` observation with
    /// `outcome` that has one.
    #[must_use]
    pub fn excluded(&self, boundary: &str, outcome: &str) -> Vec<u64> {
        self.of(boundary, outcome)
            .filter_map(|timing| timing.excluded_ns)
            .collect()
    }

    /// Each `boundary` observation with `outcome`'s residual, elapsed less
    /// excluded within the request.
    ///
    /// # Errors
    ///
    /// Returns the first observation's reason when any has no residual.
    pub fn residuals(&self, boundary: &str, outcome: &str) -> Result<Vec<u64>, String> {
        self.of(boundary, outcome)
            .map(Timing::residual_ns)
            .collect()
    }

    /// Observations of `boundary` whose outcome is not `outcome`.
    #[must_use]
    pub fn others(&self, boundary: &str, outcome: &str) -> u64 {
        self.timings
            .iter()
            .filter(|timing| timing.boundary == boundary && timing.outcome != outcome)
            .count() as u64
    }

    /// The collection-health line a repetition keeps as an observation.
    #[must_use]
    pub fn health(&self) -> String {
        format!(
            "server timing: {} observations assigned; the collector had received {} spans in {} exports",
            self.timings.len(),
            self.spans,
            self.exports
        )
    }

    /// Keeps this population's invalid reasons and collection health on
    /// `repetition`; call once per window.
    pub fn validate(&self, repetition: &mut Repetition) {
        repetition.invalid.extend(self.invalid.iter().cloned());
        repetition.observations.push(self.health());
    }

    /// Sets each `(id, q)` of `ids` on `repetition` to the nearest-rank
    /// percentile q, in milliseconds, of the `boundary` observations with
    /// the judged `outcome`: their residuals when `residual`, else their
    /// elapsed times. Other outcomes stay in the raw file with their
    /// durations and are counted by [`Self::others`], never judged. An empty
    /// judged population or an observation without a residual makes the
    /// repetition invalid.
    pub fn percentiles(
        &self,
        repetition: &mut Repetition,
        boundary: &str,
        outcome: &str,
        residual: bool,
        ids: &[(&str, f64)],
    ) {
        let samples = if residual {
            self.residuals(boundary, outcome).unwrap_or_else(|reason| {
                repetition.invalid.push(reason);
                Vec::new()
            })
        } else {
            self.elapsed(boundary, outcome)
        };
        if samples.is_empty() {
            repetition.invalid.push(format!(
                "no `{boundary}` timing observation has outcome `{outcome}`"
            ));
        }
        for (id, q) in ids {
            repetition.set(id, Reading::percentile(&samples, *q, NANOS_PER_MS));
        }
    }

    /// Observations of `boundary` with `outcome`.
    fn of<'a>(&'a self, boundary: &'a str, outcome: &'a str) -> impl Iterator<Item = &'a Timing> {
        self.timings
            .iter()
            .filter(move |timing| timing.boundary == boundary && timing.outcome == outcome)
    }
}

/// What one engine's release smoke proved and found missing, judged from
/// the same windows, collector, and counters the experiment reads.
#[derive(Debug)]
pub struct Smoke {
    /// The engine smoked.
    engine: &'static str,
    /// Each proved fact, for the success line.
    proved: Vec<String>,
    /// Each missing or malformed piece of evidence.
    missing: Vec<String>,
}

impl Smoke {
    /// An empty smoke of `engine`.
    #[must_use]
    pub const fn new(engine: &'static str) -> Self {
        Self {
            engine,
            proved: Vec::new(),
            missing: Vec::new(),
        }
    }

    /// Requires `usage` to carry positive server CPU-seconds and readable
    /// driver and collector CPU, and this host to report its available logical CPUs.
    pub fn usage(&mut self, usage: &Usage) {
        match usage.server_cpu_seconds {
            Some(cpu) if cpu > 0.0 => self.proved.push(format!("server CPU {cpu:.3}s")),
            Some(_) => self.missing.push("server CPU-seconds were zero".to_owned()),
            None => self
                .missing
                .push("server CPU-seconds are unavailable".to_owned()),
        }
        if usage.driver_cpu_seconds.is_none() {
            self.missing
                .push("driver CPU-seconds are unavailable".to_owned());
        }
        match usage.collector_cpu_seconds {
            Some(cpu) => self.proved.push(format!("collector CPU {cpu:.3}s")),
            None => self
                .missing
                .push("collector CPU-seconds are unavailable".to_owned()),
        }
        match std::thread::available_parallelism() {
            Ok(cpus) => self.proved.push(format!("{cpus} logical CPUs")),
            Err(_) => self
                .missing
                .push("provisioned logical CPUs are unavailable".to_owned()),
        }
    }

    /// Requires `population` to be complete and each `(boundary, outcome,
    /// component)` to have an observation with `outcome`, and, when
    /// `component`, an excluded component no larger than its elapsed time.
    pub fn timings(&mut self, population: &Population, boundaries: &[(&str, &str, bool)]) {
        self.missing.extend(population.invalid.iter().cloned());
        for (boundary, outcome, component) in boundaries {
            let elapsed = population.elapsed(boundary, outcome);
            if elapsed.is_empty() {
                self.missing.push(format!(
                    "no `{boundary}` timing observation has outcome `{outcome}`"
                ));
                continue;
            }
            if *component {
                if population.excluded(boundary, outcome).len() != elapsed.len() {
                    self.missing.push(format!(
                        "a `{boundary}` timing observation has no `excluded_ns` component"
                    ));
                } else if let Err(reason) = population.residuals(boundary, outcome) {
                    self.missing.push(reason);
                }
            }
            self.proved
                .push(format!("{} `{boundary}` timings", elapsed.len()));
        }
    }

    /// Requires the counter or work reading `name` to be positive.
    pub fn positive(&mut self, name: &str, value: f64) {
        if value > 0.0 {
            self.proved.push(format!("{name} {value}"));
        } else {
            self.missing
                .push(format!("{name} was {value}, not positive"));
        }
    }

    /// Records `evidence` as missing, such as a refused smoke request.
    pub fn missing(&mut self, evidence: String) {
        self.missing.push(evidence);
    }

    /// The proved line, or every missing piece of evidence.
    ///
    /// # Errors
    ///
    /// Returns a message naming the engine and each missing or malformed
    /// boundary, component, resource reading, or counter.
    pub fn finish(self) -> Result<String, String> {
        if self.missing.is_empty() {
            Ok(format!("{}: {}", self.engine, self.proved.join(", ")))
        } else {
            Err(format!(
                "{} measurement is broken: {}",
                self.engine,
                self.missing.join("; ")
            ))
        }
    }
}

/// A fresh W3C trace context for one benchmark request, so its server
/// timing can be matched to it by trace id: the server's request span
/// continues it. It is unsampled, so the server never stages a tenant copy
/// of the request's spans, while the operator policy still records every
/// span.
#[must_use]
pub fn request_trace() -> wyrd_spec::trace::TraceContext {
    let id = uuid::Uuid::new_v4().simple().to_string();
    wyrd_spec::trace::TraceContext {
        span_id: id[..16].to_owned(),
        trace_id: id,
        trace_flags: 0,
        trace_state: String::new(),
    }
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
        Collector, ExportTraceServiceRequest, KEPT_MALFORMED, KeyValue, Percentiles, Population,
        Readings, Selection, Smoke, TIMING_SPAN, Telemetry, Timing, Usage, deltas, elapsed,
    };
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

    /// A real OTLP export past a small retention bound keeps only the bound,
    /// and the overflow invalidates the window reading its boundary even
    /// after a window over another boundary passed and dropped the kept
    /// stragglers; an in-limit window stays exact; offered traces match
    /// their own observations in any order, while a missing offered trace
    /// beside an unrelated same-boundary span or a repeated trace is
    /// invalid; and malformed reasons and a stopped collector stay bounded,
    /// specific invalidity.
    ///
    /// # Panics
    ///
    /// Panics when the collector cannot start or a window's evidence is
    /// wrong.
    #[tokio::test(flavor = "multi_thread")]
    async fn collector_bounds_identity_and_loss() {
        use wyrd_tonic::otlp::common::v1::{AnyValue, any_value::Value as Any};
        use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
        use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;

        let directory = tempfile::tempdir().expect("artifacts");
        let mut collector = Collector::start(directory.path()).await.expect("collector");
        collector.received.lock().expect("received").retain = 3;
        let mut client = TraceServiceClient::connect(collector.endpoint.clone())
            .await
            .expect("connect");
        let mut id = 0_u64;
        let mut export = async |boundary: &str, traces: &[u64], elapsed: Option<i64>| {
            let spans = traces
                .iter()
                .map(|trace| {
                    id += 1;
                    let attribute = |key: &str, value| KeyValue {
                        key: key.to_owned(),
                        value: Some(AnyValue { value: Some(value) }),
                    };
                    let mut attributes = vec![
                        attribute("boundary", Any::StringValue(boundary.to_owned())),
                        attribute("outcome", Any::StringValue("success".to_owned())),
                    ];
                    attributes.extend(elapsed.map(|ns| attribute("elapsed_ns", Any::IntValue(ns))));
                    Span {
                        name: TIMING_SPAN.to_owned(),
                        trace_id: [trace.to_be_bytes(), trace.to_be_bytes()].concat(),
                        span_id: id.to_be_bytes().to_vec(),
                        end_time_unix_nano: super::unix_nanos(std::time::SystemTime::now()),
                        attributes,
                        ..Span::default()
                    }
                })
                .collect();
            client
                .export(ExportTraceServiceRequest {
                    resource_spans: vec![ResourceSpans {
                        scope_spans: vec![ScopeSpans {
                            spans,
                            ..ScopeSpans::default()
                        }],
                        ..ResourceSpans::default()
                    }],
                })
                .await
                .expect("export");
        };

        let opened = std::time::SystemTime::now();
        export("a", &[0; 5], Some(10)).await;
        let other = collector.smoke("other", opened, &[("b", 0)], &[]).await;
        assert!(other.invalid.is_empty(), "{:?}", other.invalid);
        let over = collector.smoke("over", opened, &[("a", 5)], &[]).await;
        assert_eq!(
            over.invalid,
            [
                "2 `a` timing observations overflowed the collector's 3-observation retention and were dropped",
                "0 `a` timing observations arrived for 5 requests",
            ],
            "the overflow outlives a window over another boundary"
        );

        let opened = std::time::SystemTime::now();
        export("a", &[0; 2], Some(10)).await;
        let within = collector.window("within", opened, &[("a", 2)]).await;
        assert!(within.invalid.is_empty(), "{:?}", within.invalid);
        assert_eq!(within.elapsed("a", "success"), [10, 10]);

        let trace = |id: u64| format!("{id:016x}{id:016x}");
        let (x, y, z) = (trace(7), trace(8), trace(9));
        let opened = std::time::SystemTime::now();
        export("a", &[9, 7], Some(10)).await;
        let ordered = collector
            .smoke("ordered", opened, &[("a", 2)], &[&x, &z])
            .await;
        assert!(ordered.invalid.is_empty(), "{:?}", ordered.invalid);
        let opened = std::time::SystemTime::now();
        export("a", &[7, 8], Some(10)).await;
        let unrelated = collector
            .smoke("unrelated", opened, &[("a", 2)], &[&x, &z])
            .await;
        assert_eq!(
            unrelated.invalid,
            ["1 of 2 traced requests have no server timing observation"],
            "an unrelated span never stands in for an offered trace"
        );
        let opened = std::time::SystemTime::now();
        export("a", &[8, 8], Some(10)).await;
        let repeated = collector
            .smoke("repeated", opened, &[("a", 2)], &[&y])
            .await;
        assert_eq!(
            repeated.invalid,
            ["1 of 1 traced requests have more than one server timing observation"]
        );

        let opened = std::time::SystemTime::now();
        export("a", &[0; KEPT_MALFORMED + 2], None).await;
        let malformed = collector.smoke("malformed", opened, &[("a", 0)], &[]).await;
        assert_eq!(malformed.invalid.len(), KEPT_MALFORMED + 1);
        assert_eq!(
            malformed.invalid.last().map(String::as_str),
            Some("2 more `wyrd.timing` spans are malformed, beyond the 16 kept")
        );

        drop(collector.stop.take());
        let opened = std::time::SystemTime::now();
        let lost = collector.smoke("lost", opened, &[("a", 0)], &[]).await;
        assert_eq!(
            lost.invalid,
            ["the timing collector stopped reporting: its output closed"]
        );
    }

    /// A smoke passes only on complete evidence and otherwise names its
    /// engine and every missing boundary, component, resource, and counter.
    ///
    /// # Panics
    ///
    /// Panics when a complete smoke fails or a broken one omits a piece.
    #[test]
    fn smoke_names_every_missing_piece() {
        let timing = |boundary: &str, excluded_ns| Timing {
            boundary: boundary.to_owned(),
            outcome: "succeeded".to_owned(),
            elapsed_ns: 10,
            excluded_ns,
            end_unix_ns: 1,
            trace_id: "t".to_owned(),
            span_id: "s".to_owned(),
            instance: "i".to_owned(),
        };
        let usage = |server_cpu_seconds| Usage {
            seconds: 1.0,
            server_cpu_seconds,
            peak_memory_bytes: None,
            driver_cpu_seconds: Some(0.1),
            collector_cpu_seconds: Some(0.01),
            telemetry: Telemetry::default(),
        };
        let judged = [
            ("gateway.buffered", "succeeded", true),
            ("gateway.stream.end", "succeeded", true),
        ];
        let mut complete = Smoke::new("gateway");
        complete.usage(&usage(Some(0.5)));
        complete.positive("staged captures", 4.0);
        complete.timings(
            &Population {
                timings: vec![
                    timing("gateway.buffered", Some(3)),
                    timing("gateway.stream.end", Some(4)),
                ],
                ..Population::default()
            },
            &judged,
        );
        assert!(complete.finish().is_ok());

        let mut broken = Smoke::new("gateway");
        broken.usage(&usage(None));
        broken.positive("staged captures", 0.0);
        broken.timings(
            &Population {
                timings: vec![timing("gateway.buffered", None)],
                invalid: vec!["1 of 2 `gateway.buffered` timings arrived".to_owned()],
                ..Population::default()
            },
            &judged,
        );
        assert_eq!(
            broken.finish(),
            Err(
                "gateway measurement is broken: server CPU-seconds are unavailable; \
                 staged captures was 0, not positive; 1 of 2 `gateway.buffered` timings \
                 arrived; a `gateway.buffered` timing observation has no `excluded_ns` \
                 component; no `gateway.stream.end` timing observation has outcome \
                 `succeeded`"
                    .to_owned()
            )
        );
    }

    /// A timing span the server's own telemetry pipeline records and exports
    /// over OTLP/gRPC reaches the collector as one well-formed observation
    /// with its exact integer nanoseconds and residual, including a duration
    /// above `u32::MAX`, while a `u64` field, which the layer exports as a
    /// string, is refused as malformed rather than parsed.
    ///
    /// # Panics
    ///
    /// Panics when the pipeline cannot start or the collected observation
    /// is missing, malformed, or inexact.
    #[tokio::test(flavor = "multi_thread")]
    async fn exported_timing_spans_parse_exactly() {
        let directory = tempfile::tempdir().expect("artifacts");
        let collector = Collector::start(directory.path()).await.expect("collector");
        let guard = wyrd_telemetry::init(wyrd_telemetry::TelemetryConfig {
            endpoint: Some(collector.endpoint.clone()),
            sample_ratio: Some(1.0),
            ..wyrd_telemetry::TelemetryConfig::default()
        })
        .expect("server telemetry pipeline");
        let opened = std::time::SystemTime::now();
        let elapsed = std::time::Duration::from_nanos(5_000_000_123);
        let excluded = std::time::Duration::from_nanos(7);
        tracing::info_span!("request").in_scope(|| {
            wyrd_telemetry::record_timing(
                &tracing::Span::current(),
                "verification.direct",
                "completed",
                elapsed,
                Some(excluded),
            );
            tracing::info_span!(
                "wyrd.timing",
                boundary = "verification.direct",
                outcome = "completed",
                elapsed_ns = 9_u64,
            )
            .in_scope(|| {});
        });
        tokio::task::spawn_blocking(move || guard.shutdown())
            .await
            .expect("telemetry shutdown");
        let population = collector
            .window("exported", opened, &[("verification.direct", 1)])
            .await;
        assert_eq!(
            population.invalid,
            ["a `wyrd.timing` span is malformed: a non-integer duration"]
        );
        assert_eq!(
            (
                population.elapsed("verification.direct", "completed"),
                population.excluded("verification.direct", "completed"),
                population.residuals("verification.direct", "completed"),
            ),
            (vec![5_000_000_123], vec![7], Ok(vec![5_000_000_116]))
        );
    }
}
