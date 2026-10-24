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

/// What the collector has received and not yet assigned to a window.
#[derive(Debug, Default)]
struct Received {
    /// Unassigned timing observations, in arrival order.
    timings: Vec<Timing>,
    /// Undecodable timing spans since the last window.
    malformed: Vec<String>,
    /// Export requests received in total.
    exports: u64,
    /// Spans received in total, of every name.
    spans: u64,
}

/// The collector's OTLP trace service.
#[derive(Clone)]
struct Intake(Arc<Mutex<Received>>);

#[async_trait::async_trait]
impl TraceService for Intake {
    /// Keeps every timing span of one export and acknowledges it whole.
    async fn export(
        &self,
        request: wyrd_tonic::tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<wyrd_tonic::tonic::Response<ExportTraceServiceResponse>, wyrd_tonic::tonic::Status>
    {
        let (timings, malformed, spans) = decode_timings(request.get_ref());
        let mut received = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        received.timings.extend(timings);
        received.malformed.extend(malformed);
        received.exports += 1;
        received.spans += spans;
        Ok(wyrd_tonic::tonic::Response::new(
            ExportTraceServiceResponse::default(),
        ))
    }
}

/// A local OTLP/gRPC trace collector the measured server exports to.
///
/// It runs inside the benchmark driver, so its CPU is part of driver CPU and
/// never server CPU; the server's tracing and export cost stays in server
/// CPU. It keeps only [`TIMING_SPAN`]s, decodes them into [`Timing`]s, and
/// hands each window the observations that ended inside it once every
/// expected one arrived; every assigned observation is appended to
/// `timings.jsonl` under the run's artifacts.
pub struct Collector {
    /// The `http://` endpoint the server exports to.
    endpoint: String,
    /// What arrived and is unassigned.
    received: Arc<Mutex<Received>>,
    /// Raw observation file under the artifacts directory.
    log: PathBuf,
    /// Stops the gRPC server on drop.
    _stop: tokio_util::sync::DropGuard,
}

impl Collector {
    /// Listens on a free loopback port and keeps raw observations under
    /// `artifacts`.
    ///
    /// # Errors
    ///
    /// Returns the bind or artifacts-directory failure.
    pub async fn start(artifacts: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(artifacts)?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let received = Arc::new(Mutex::new(Received::default()));
        let stop = tokio_util::sync::CancellationToken::new();
        let service = TraceServiceServer::new(Intake(Arc::clone(&received)))
            .max_decoding_message_size(usize::MAX);
        let stopped = stop.clone();
        tokio::spawn(async move {
            let served = wyrd_tonic::tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    stopped.cancelled_owned(),
                )
                .await;
            if let Err(error) = served {
                tracing::error!(%error, "the timing collector stopped");
            }
        });
        Ok(Self {
            endpoint,
            received,
            log: artifacts.join("timings.jsonl"),
            _stop: stop.drop_guard(),
        })
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
            "method": "server wyrd.timing spans: exact integer nanoseconds from the owning server boundary's std::time::Instant timers, exported through the operator OTLP/gRPC span exporter to a collector inside the benchmark driver",
            "clock": "std::time::Instant on the server: the OS monotonic clock",
            "server_env": {
                "WYRD_OTLP_SAMPLE_RATIO": "1.0",
                "OTEL_BSP_MAX_QUEUE_SIZE": "1048576",
                "OTEL_BSP_MAX_EXPORT_BATCH_SIZE": "8192",
                "OTEL_BSP_SCHEDULE_DELAY": EXPORT_DELAY.as_millis().to_string(),
            },
            "cost": "the server's tracing and export CPU is in server CPU; the collector's CPU is in driver CPU",
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
        self.collect(label, opened, expected, true, &[]).await
    }

    /// [`Self::window`] for a window whose boundaries also serve requests
    /// the driver did not send itself: waits until at least `least`
    /// observations of each boundary ended at or after `opened` and every
    /// trace id of `traces` has one, then takes every observation of those
    /// boundaries that did. The population is invalid when a count is short
    /// or a trace has no observation after [`COLLECTION_WAIT`], a span id
    /// repeats, or a timing span was malformed.
    pub async fn traced(
        &self,
        label: &str,
        opened: SystemTime,
        least: &[(&str, u64)],
        traces: &[&str],
    ) -> Population {
        self.collect(label, opened, least, false, traces).await
    }

    /// Waits for and takes a window's observations: `expected` counts per
    /// boundary, exact when `exact` and lower bounds otherwise, and one
    /// observation at least for each of `traces`.
    async fn collect(
        &self,
        label: &str,
        opened: SystemTime,
        expected: &[(&str, u64)],
        exact: bool,
        traces: &[&str],
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
        while waiting.elapsed() < COLLECTION_WAIT {
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
    use super::{Collector, Percentiles, Readings, Selection, deltas, elapsed};
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
