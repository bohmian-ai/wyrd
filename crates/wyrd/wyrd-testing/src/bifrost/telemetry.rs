//! Read-only parsing and validation of production Forge telemetry windows.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use metrics_exporter_prometheus::PrometheusHandle;
use wyrd_telemetry::{CapturedSpan, CapturedSpanStatus, TestTraceCapture};

/// Exact Prometheus sample kind retained across family normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostMetricKind {
    /// Monotonic counter sample.
    Counter,
    /// Point-in-time gauge sample.
    Gauge,
    /// Cumulative histogram bucket sample.
    HistogramBucket,
    /// Histogram observation-count sample.
    HistogramCount,
    /// Histogram observation-sum sample.
    HistogramSum,
}

/// Prometheus family type declared by exposition metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrometheusFamilyType {
    /// Declared counter family.
    Counter,
    /// Declared gauge family.
    Gauge,
    /// Declared histogram family.
    Histogram,
    /// Exporter summary family outside the canonical binding ledger.
    Summary,
}

/// One parsed production Prometheus sample.
#[derive(Debug, Clone, PartialEq)]
pub struct BifrostMetricSample {
    /// Normalized production metric family.
    pub family: String,
    /// Exact fixed-cardinality labels emitted with the sample.
    pub labels: BTreeMap<String, String>,
    /// Absolute or window-delta value, depending on the enclosing artifact.
    pub value: f64,
    /// Exact rendered sample kind.
    pub kind: BifrostMetricKind,
}

/// One single-use marker for a production telemetry observation window.
#[derive(Debug, Clone)]
pub struct BifrostTelemetryCheckpoint {
    /// Unique process-local token rejected after one delta construction.
    id: u64,
    /// Absolute sample values at the start of the window.
    metrics: BTreeMap<String, f64>,
    /// Exact family types declared at the window baseline.
    types: BTreeMap<String, PrometheusFamilyType>,
    /// Monotonic start instant used for window rates.
    started_at: Instant,
    /// Production trace position before the window begins.
    spans: usize,
    /// Process counters and identity at the beginning of the window.
    process: ProcessSample,
}

/// One process-scoped resource observation owned by the capture sampler.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessSample {
    /// Stable OS identity; a change starts a new counter epoch.
    pub(crate) identity: String,
    /// Explicit replacement epoch within this capture.
    pub(crate) epoch: u64,
    /// Cumulative process CPU seconds.
    pub(crate) cpu_total: f64,
    /// Point-in-time resident bytes.
    pub(crate) rss_bytes: u64,
    /// Cumulative Tokio worker busy seconds.
    pub(crate) tokio_busy_total: f64,
    /// Point-in-time Tokio global queue depth.
    pub(crate) queue_depth: u64,
}

/// Checked process resource delta and sampled peaks for one window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessWindow {
    /// Final process identity.
    pub(crate) identity: String,
    /// Final explicit replacement epoch.
    pub(crate) epoch: u64,
    /// CPU consumed within the final stable epoch.
    pub(crate) cpu_seconds: f64,
    /// Final resident bytes.
    pub(crate) current_rss_bytes: u64,
    /// Maximum sampled resident bytes.
    pub(crate) peak_rss_bytes: u64,
    /// Tokio busy time consumed within the final stable epoch.
    pub(crate) tokio_busy_seconds: f64,
    /// Maximum sampled runtime queue depth.
    pub(crate) queue_peak: u64,
}

/// Captured production telemetry emitted during one observation window.
#[derive(Debug, Clone)]
pub struct BifrostTelemetryDelta {
    /// Counter and histogram deltas from the one production render handle.
    pub metrics: Vec<BifrostMetricSample>,
    /// Gauge maxima observed by the production capture while the window ran.
    pub gauge_maxima: Vec<BifrostMetricSample>,
    /// Final gauge values from the render that closed the capture window.
    pub gauge_final: Vec<BifrostMetricSample>,
    /// Finished production-provider spans after the checkpoint.
    pub spans: Vec<CapturedSpan>,
    /// Positive render-to-render duration used by counter-rate calculations.
    pub interval_seconds: f64,
    /// Honest sampled process resource evidence for the capture window.
    pub(crate) process: ProcessWindow,
}

impl BifrostTelemetryDelta {
    /// Renders this window's samples of the named families as one evidence line.
    ///
    /// Nonzero counter and histogram samples are window deltas, `peak:` gauges are the
    /// highest value sampled while the window ran, and `final:` gauges are the
    /// value rendered when it closed. Journeys print this beside the
    /// independent fact they assert against, so a run's output records the
    /// actual production samples rather than only the assertion's verdict.
    /// Families with no sample in the window are omitted.
    #[must_use]
    pub fn evidence(&self, families: &[&str]) -> String {
        let deltas = self
            .metrics
            .iter()
            .filter(|sample| {
                sample.kind != BifrostMetricKind::HistogramBucket && sample.value != 0.0
            })
            .map(|sample| ("", sample));
        let peaks = self.gauge_maxima.iter().map(|sample| ("peak:", sample));
        let finals = self.gauge_final.iter().map(|sample| ("final:", sample));
        deltas
            .chain(peaks)
            .chain(finals)
            .filter(|(_, sample)| families.contains(&sample.family.as_str()))
            .map(|(prefix, sample)| format!("{prefix}{}={}", rendered_series(sample), sample.value))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Renders this window's elapsed time and process resources as one
    /// evidence line.
    ///
    /// CPU, Tokio busy time, and resident bytes are the whole test process —
    /// every server role and the journey itself — never one writer or query.
    /// `rss_peak_bytes` is the highest sample the window took: its endpoints
    /// alone unless the window ran a [`BifrostTelemetrySampler`].
    #[must_use]
    pub fn resources(&self) -> String {
        format!(
            "elapsed_s={} process_cpu_s={} tokio_busy_s={} rss_end_bytes={} rss_peak_bytes={}",
            self.interval_seconds,
            self.process.cpu_seconds,
            self.process.tokio_busy_seconds,
            self.process.current_rss_bytes,
            self.process.peak_rss_bytes,
        )
    }
}

/// Values retained by the single bounded gauge/process sampler.
#[derive(Debug)]
struct SamplerSnapshot {
    /// Per-series gauge maxima.
    maxima: BTreeMap<String, f64>,
    /// Most recent process observation.
    process: ProcessSample,
    /// Maximum RSS observed across all ticks in the final epoch.
    peak_rss_bytes: u64,
    /// Maximum queue depth observed across all ticks in the final epoch.
    queue_peak: u64,
}

/// Bounded background sampler for production gauges during one capture window.
pub struct BifrostTelemetrySampler {
    /// Cancellation signal for the production-render polling task.
    stop: tokio_util::sync::CancellationToken,
    /// Per-series maxima accumulated only from production-rendered gauges.
    /// Task that owns bounded polling and returns its complete snapshot.
    task: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>>,
}

/// Failure raised when a production telemetry window cannot prove one report field.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BifrostTelemetryReportError {
    /// One exact closed binding was absent or malformed.
    #[error("invalid production telemetry binding {id}: {detail}")]
    InvalidBinding {
        /// Stable private binding identifier.
        id: String,
        /// Non-sensitive validation detail.
        detail: String,
    },
    /// A closed production family or label combination was absent.
    #[error("missing production telemetry series {family}")]
    MissingSeries {
        /// Required production metric family.
        family: String,
    },
    /// A required activity counter did not advance in the capture window.
    #[error("stale production telemetry series {family}")]
    StaleSeries {
        /// Required fresh production metric family.
        family: String,
    },
    /// A monotonic counter or histogram regressed across a capture window.
    #[error("production telemetry counter regressed for {series}")]
    CounterRegression {
        /// Exact rendered series that regressed.
        series: String,
    },
    /// A required histogram received no production observations.
    #[error("production telemetry histogram {family} has no samples")]
    EmptyHistogram {
        /// Required histogram family.
        family: String,
    },
    /// A checkpoint was supplied to delta construction more than once.
    #[error("production telemetry checkpoint was already consumed")]
    ReusedWindow,
    /// A capture interval was zero, negative, or not representable.
    #[error("production telemetry capture interval is not positive")]
    InvalidInterval,
    /// The bounded sampler task panicked or was cancelled before joining.
    #[error("production telemetry sampler join failed: {detail}")]
    SamplerJoin {
        /// Non-sensitive join failure.
        detail: String,
    },
    /// A required exact-run production span was absent.
    #[error("missing production Forge span {span}")]
    MissingSpan {
        /// Exact required production instrumentation name.
        span: String,
    },
    /// A production Forge span used a renamed or malformed contract.
    #[error("invalid production Forge span {span}: {detail}")]
    InvalidSpan {
        /// Exact captured instrumentation name.
        span: String,
        /// Non-sensitive contract failure description.
        detail: String,
    },
    /// Rendered Prometheus text did not use the expected restricted grammar.
    #[error("invalid Prometheus production sample: {detail}")]
    Parse {
        /// Non-sensitive parse failure detail.
        detail: String,
    },
}

/// Read-only capture over one installed production Prometheus recorder and tracer.
#[derive(Clone)]
pub struct BifrostTelemetryCapture {
    /// Production recorder render handle, never passed into Forge.
    metrics: PrometheusHandle,
    /// Same-provider trace capture installed before role composition.
    traces: TestTraceCapture,
    /// Tokens already consumed by `delta_since`.
    consumed: Arc<Mutex<BTreeSet<u64>>>,
    /// Monotonic checkpoint identity that cannot collide in this capture.
    next_checkpoint: Arc<AtomicU64>,
}

impl BifrostTelemetryCapture {
    /// Construct a capture after the process production runtime is installed.
    #[must_use]
    pub fn new(metrics: PrometheusHandle, traces: TestTraceCapture) -> Self {
        Self {
            metrics,
            traces,
            consumed: Arc::new(Mutex::new(BTreeSet::new())),
            next_checkpoint: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a baseline from the configured production exporters.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::Parse`] when rendered Prometheus
    /// output cannot be parsed without losing a label or numeric value.
    pub fn checkpoint(&self) -> Result<BifrostTelemetryCheckpoint, BifrostTelemetryReportError> {
        let rendered = self.metrics.render();
        Ok(BifrostTelemetryCheckpoint {
            id: self.next_checkpoint.fetch_add(1, Ordering::AcqRel),
            metrics: rendered_values(&rendered)?,
            types: rendered_types(&rendered)?,
            started_at: Instant::now(),
            spans: self.traces.checkpoint(),
            process: process_sample(0)?,
        })
    }

    /// Render the current production Prometheus exposition for integration assertions.
    ///
    /// This returns the installed process recorder's text unchanged. It does not
    /// retain a scrape or add any fixture-derived metric values.
    #[must_use]
    pub fn render(&self) -> String {
        self.metrics.render()
    }

    /// Return the current production Prometheus series as absolute samples.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the recorder emits a malformed series.
    pub fn snapshot(&self) -> Result<Vec<BifrostMetricSample>, BifrostTelemetryReportError> {
        let rendered = self.metrics.render();
        let types = rendered_types(&rendered)?;
        rendered_values(&rendered)?
            .into_iter()
            .filter(|(series, _)| supported_series(series, &types))
            .map(|(series, value)| parse_sample(&series, value, &types))
            .collect()
    }

    /// Return the metric families currently rendered by the production recorder.
    #[must_use]
    pub fn families(&self) -> BTreeSet<String> {
        let rendered = self.metrics.render();
        let types = rendered_types(&rendered).unwrap_or_default();
        rendered_values(&rendered)
            .ok()
            .into_iter()
            .flat_map(std::iter::IntoIterator::into_iter)
            .filter_map(|(series, value)| parse_sample(&series, value, &types).ok())
            .map(|sample| sample.family)
            .collect()
    }

    /// Return a one-shot synchronous delta from a production checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the same parse, reuse, regression, and interval errors as the
    /// asynchronous sampler-backed capture.
    pub fn delta_since(
        &self,
        checkpoint: &BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        self.delta_without_sampler(checkpoint.clone())
    }

    /// Render what the installed production telemetry knows, for a failure report.
    ///
    /// A panicking journey has the whole production span tree and metric
    /// exposition in memory at the moment it asserts, and without this it
    /// discards all of it — leaving a bare `assert_eq!` on a number as the only
    /// evidence. This renders the part a reader needs to place the failure:
    /// every span the tracer marked failed, with its scrubbed attributes and
    /// trace id; then the span names that did finish, in order; then the
    /// non-zero production metric series.
    ///
    /// This is diagnostic output only. It parses nothing and asserts nothing,
    /// and it degrades to a partial report rather than failing, because its one
    /// caller is a panic hook where a second failure would abort the process.
    #[must_use]
    pub fn failure_diagnostics(&self) -> String {
        use std::fmt::Write as _;

        let mut report = String::new();
        let spans = self.traces.finished_since(0);

        let errored: Vec<&CapturedSpan> = spans
            .iter()
            .filter(|span| matches!(span.status, CapturedSpanStatus::Error(_)))
            .collect();
        if errored.is_empty() {
            report.push_str("failed spans: none\n");
        } else {
            let _ = writeln!(report, "failed spans ({}):", errored.len());
            for span in errored {
                let description = match &span.status {
                    CapturedSpanStatus::Error(description) => description.as_str(),
                    CapturedSpanStatus::Ok | CapturedSpanStatus::Unset => "",
                };
                let _ = writeln!(
                    report,
                    "  {} trace={} {}us: {description}",
                    span.name,
                    span.trace_id,
                    span.duration_nanos / 1_000
                );
                for (field, value) in &span.attributes {
                    let _ = writeln!(report, "      {field} = {value}");
                }
            }
        }

        let _ = writeln!(report, "finished spans ({}):", spans.len());
        for span in &spans {
            let _ = writeln!(report, "  {}", span.name);
        }

        let rendered = self.metrics.render();
        let types = rendered_types(&rendered).unwrap_or_default();
        let values = rendered_values(&rendered).unwrap_or_default();
        let nonzero: Vec<(String, f64)> = values
            .into_iter()
            .filter(|(series, value)| *value != 0.0 && supported_series(series, &types))
            .collect();
        let _ = writeln!(report, "non-zero production series ({}):", nonzero.len());
        for (series, value) in nonzero {
            let _ = writeln!(report, "  {series} = {value}");
        }

        report
    }

    /// Return the spans finished since `checkpoint` without consuming it.
    ///
    /// A streamed operation's span closes when the server drops the response
    /// stream, which can be shortly after the client has read the terminal
    /// frame. A journey polls this bounded read until the span it needs has
    /// closed, then takes its single consuming delta.
    #[must_use]
    pub fn spans_since(&self, checkpoint: &BifrostTelemetryCheckpoint) -> Vec<CapturedSpan> {
        self.traces.finished_since(checkpoint.spans)
    }

    /// Return the names of all finished spans in the shared production capture.
    #[must_use]
    pub fn span_names(&self) -> BTreeSet<String> {
        self.traces
            .finished_since(0)
            .into_iter()
            .map(|span| span.name)
            .collect()
    }

    /// Build one checked delta from the checkpointed production exporter state.
    ///
    /// Gauge maxima include the baseline and end snapshot. A caller that needs
    /// denser bounded sampling calls this method at its capture cadence and
    /// retains the maximum externally; no fixture value enters this path.
    ///
    /// # Errors
    ///
    /// Returns a parse, reused-window, counter-regression, or invalid-interval
    /// error when this capture cannot represent a trustworthy production window.
    fn delta_without_sampler(
        &self,
        checkpoint: BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        let mut consumed = self
            .consumed
            .lock()
            .map_err(|_| BifrostTelemetryReportError::ReusedWindow)?;
        if !consumed.insert(checkpoint.id) {
            return Err(BifrostTelemetryReportError::ReusedWindow);
        }
        drop(consumed);
        let elapsed = checkpoint.started_at.elapsed();
        if elapsed.is_zero() {
            return Err(BifrostTelemetryReportError::InvalidInterval);
        }
        let rendered = self.metrics.render();
        let current_types = rendered_types(&rendered)?;
        if checkpoint.types.iter().any(|(family, baseline)| {
            current_types
                .get(family)
                .is_some_and(|current| current != baseline)
        }) {
            return Err(BifrostTelemetryReportError::Parse {
                detail: "Prometheus TYPE metadata conflicted within one window".to_owned(),
            });
        }
        let current = rendered_values(&rendered)?;
        let mut metrics = Vec::new();
        let mut gauge_maxima = Vec::new();
        let mut gauge_final = Vec::new();
        for (series, value) in &current {
            if !supported_series(series, &current_types) {
                continue;
            }
            let prior = checkpoint.metrics.get(series).copied().unwrap_or(0.0);
            let parsed = parse_sample(series, *value, &current_types)?;
            if parsed.kind == BifrostMetricKind::Gauge {
                let baseline = parse_sample(series, prior, &current_types)?;
                let end = parse_sample(series, *value, &current_types)?;
                gauge_maxima.push(BifrostMetricSample {
                    family: end.family.clone(),
                    labels: end.labels.clone(),
                    value: baseline.value.max(end.value),
                    kind: end.kind,
                });
                gauge_final.push(end);
            } else {
                if *value < prior {
                    return Err(BifrostTelemetryReportError::CounterRegression {
                        series: series.clone(),
                    });
                }
                let mut sample = parsed;
                sample.value = *value - prior;
                metrics.push(sample);
            }
        }
        Ok(BifrostTelemetryDelta {
            metrics,
            gauge_maxima,
            gauge_final,
            spans: self.traces.finished_since(checkpoint.spans),
            interval_seconds: elapsed.as_secs_f64(),
            process: process_window(
                &checkpoint.process,
                process_sample(checkpoint.process.epoch)?,
                checkpoint.process.rss_bytes,
                checkpoint.process.queue_depth,
            )?,
        })
    }

    /// Start bounded fixed-interval production-gauge sampling for one window.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::Parse`] when the initial render is
    /// malformed. The sampler never receives fixture, SQL, or scenario values.
    pub fn begin_gauge_sampling(
        &self,
        checkpoint: &BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetrySampler, BifrostTelemetryReportError> {
        let initial_rendered = self.metrics.render();
        let initial = rendered_values(&initial_rendered)?;
        let initial_types = rendered_types(&initial_rendered)?;
        let mut initial_maxima = BTreeMap::new();
        merge_gauge_maxima(&mut initial_maxima, &initial, &initial_types)?;
        let initial_process = process_sample(checkpoint.process.epoch)?;
        let stop = tokio_util::sync::CancellationToken::new();
        let sampler_stop = stop.clone();
        let metrics = self.metrics.clone();
        let task = tokio::spawn(async move {
            let mut snapshot = SamplerSnapshot {
                maxima: initial_maxima,
                peak_rss_bytes: initial_process.rss_bytes,
                queue_peak: initial_process.queue_depth,
                process: initial_process,
            };
            // Forge's production reservation can cover a sub-25ms rewrite. Sample at a
            // millisecond cadence so the test's peak remains an observed exporter
            // value rather than a coincidental final zero.
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    () = sampler_stop.cancelled() => return Ok(snapshot),
                    _ = interval.tick() => {
                        let exposition = metrics.render();
                        let rendered = rendered_values(&exposition)?;
                        let types = rendered_types(&exposition)?;
                        merge_gauge_maxima(&mut snapshot.maxima, &rendered, &types)?;
                        let next = process_sample(snapshot.process.epoch)?;
                        if next.identity == snapshot.process.identity {
                            if next.cpu_total < snapshot.process.cpu_total || next.tokio_busy_total < snapshot.process.tokio_busy_total {
                                return Err(BifrostTelemetryReportError::CounterRegression { series: "process cumulative counters".to_owned() });
                            }
                            snapshot.peak_rss_bytes = snapshot.peak_rss_bytes.max(next.rss_bytes);
                            snapshot.queue_peak = snapshot.queue_peak.max(next.queue_depth);
                            snapshot.process = next;
                        } else {
                            snapshot.process = ProcessSample { epoch: snapshot.process.epoch.checked_add(1).ok_or_else(|| BifrostTelemetryReportError::Parse { detail: "process epoch overflow".to_owned() })?, ..next };
                            snapshot.peak_rss_bytes = snapshot.process.rss_bytes;
                            snapshot.queue_peak = snapshot.process.queue_depth;
                        }
                    },
                }
            }
        });
        Ok(BifrostTelemetrySampler { stop, task })
    }

    /// Build one checked production delta after stopping its gauge sampler.
    ///
    /// # Errors
    ///
    /// Returns the same errors as checkpoint/delta construction, including
    /// single-use window and counter-regression failures.
    pub async fn delta_since_with_sampler(
        &self,
        checkpoint: BifrostTelemetryCheckpoint,
        sampler: BifrostTelemetrySampler,
    ) -> Result<BifrostTelemetryDelta, BifrostTelemetryReportError> {
        sampler.stop.cancel();
        let sampled = join_sampler(sampler.task).await?;
        let current_types = checkpoint.types.clone();
        let process_start = checkpoint.process.clone();
        let mut delta = self.delta_without_sampler(checkpoint)?;
        let mut maxima = sampled.maxima;
        for sample in &delta.gauge_maxima {
            let series = rendered_series(sample);
            maxima
                .entry(series)
                .and_modify(|maximum| *maximum = maximum.max(sample.value))
                .or_insert(sample.value);
        }
        delta.gauge_maxima = maxima
            .into_iter()
            .filter(|(series, _)| supported_series(series, &current_types))
            .map(|(series, value)| parse_sample(&series, value, &current_types))
            .collect::<Result<Vec<_>, _>>()?;
        delta.process = process_window(
            &process_start,
            sampled.process,
            sampled.peak_rss_bytes,
            sampled.queue_peak,
        )?;
        Ok(delta)
    }
}

/// Join the single sampler and preserve both task and polling failures.
async fn join_sampler(
    task: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>>,
) -> Result<SamplerSnapshot, BifrostTelemetryReportError> {
    task.await
        .map_err(|error| BifrostTelemetryReportError::SamplerJoin {
            detail: error.to_string(),
        })?
}

/// Read one real process/runtime sample without inventing per-node resources.
fn process_sample(epoch: u64) -> Result<ProcessSample, BifrostTelemetryReportError> {
    let pid = std::process::id();
    let output = std::process::Command::new("ps")
        .args(["-o", "time=,rss=", "-p", &pid.to_string()])
        .output()
        .map_err(|error| BifrostTelemetryReportError::Parse {
            detail: format!("process sample failed: {error}"),
        })?;
    if !output.status.success() {
        return Err(BifrostTelemetryReportError::Parse {
            detail: "process sample command failed".to_owned(),
        });
    }
    let rendered =
        String::from_utf8(output.stdout).map_err(|_| BifrostTelemetryReportError::Parse {
            detail: "process sample is not UTF-8".to_owned(),
        })?;
    let mut fields = rendered.split_whitespace();
    let cpu = fields
        .next()
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process CPU sample is absent".to_owned(),
        })?;
    let rss_kib = fields
        .next()
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process RSS sample is absent".to_owned(),
        })?
        .parse::<u64>()
        .map_err(|_| BifrostTelemetryReportError::Parse {
            detail: "process RSS sample is invalid".to_owned(),
        })?;
    let cpu_total = parse_cpu_seconds(cpu)?;
    let runtime = tokio::runtime::Handle::current().metrics();
    let tokio_busy_total = (0..runtime.num_workers())
        .map(|worker| runtime.worker_total_busy_duration(worker).as_secs_f64())
        .sum();
    Ok(ProcessSample {
        identity: format!("pid-{pid}"),
        epoch,
        cpu_total,
        rss_bytes: rss_kib
            .checked_mul(1024)
            .ok_or_else(|| BifrostTelemetryReportError::Parse {
                detail: "process RSS overflow".to_owned(),
            })?,
        tokio_busy_total,
        queue_depth: runtime.global_queue_depth() as u64,
    })
}

/// Parse the platform process CPU clock.
fn parse_cpu_seconds(rendered: &str) -> Result<f64, BifrostTelemetryReportError> {
    let fields = rendered.split(':').collect::<Vec<_>>();
    let value = match fields.as_slice() {
        [minutes, seconds] => minutes
            .parse::<f64>()
            .ok()
            .zip(seconds.parse::<f64>().ok())
            .map(|(m, s)| m * 60.0 + s),
        [hours, minutes, seconds] => hours
            .parse::<f64>()
            .ok()
            .zip(minutes.parse::<f64>().ok())
            .zip(seconds.parse::<f64>().ok())
            .map(|((h, m), s)| h * 3_600.0 + m * 60.0 + s),
        _ => None,
    };
    value
        .filter(|value| value.is_finite())
        .ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: "process CPU sample is invalid".to_owned(),
        })
}

/// Reconcile process endpoints and sampled peaks into one checked epoch window.
fn process_window(
    start: &ProcessSample,
    end: ProcessSample,
    peak_rss_bytes: u64,
    queue_peak: u64,
) -> Result<ProcessWindow, BifrostTelemetryReportError> {
    let replaced = start.identity != end.identity || start.epoch != end.epoch;
    let (cpu_start, busy_start) = if replaced {
        (0.0, 0.0)
    } else {
        (start.cpu_total, start.tokio_busy_total)
    };
    if end.cpu_total < cpu_start || end.tokio_busy_total < busy_start {
        return Err(BifrostTelemetryReportError::CounterRegression {
            series: "process cumulative counters".to_owned(),
        });
    }
    Ok(ProcessWindow {
        identity: end.identity,
        epoch: end.epoch,
        cpu_seconds: end.cpu_total - cpu_start,
        current_rss_bytes: end.rss_bytes,
        peak_rss_bytes: peak_rss_bytes.max(end.rss_bytes),
        tokio_busy_seconds: end.tokio_busy_total - busy_start,
        queue_peak: queue_peak.max(end.queue_depth),
    })
}

/// Merge one rendered production scrape into its bounded non-monotonic maxima.
///
/// Counters and histogram buckets are excluded because their window deltas are
/// calculated from the checkpoint and final scrape. The map retains one number
/// per gauge series regardless of the number of polling ticks.
fn merge_gauge_maxima(
    maxima: &mut BTreeMap<String, f64>,
    rendered: &BTreeMap<String, f64>,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<(), BifrostTelemetryReportError> {
    for (series, value) in rendered {
        if !supported_series(series, types) {
            continue;
        }
        if parse_sample(series, *value, types)?.kind == BifrostMetricKind::Gauge {
            maxima
                .entry(series.clone())
                .and_modify(|maximum| *maximum = maximum.max(*value))
                .or_insert(*value);
        }
    }
    Ok(())
}

/// Server-only query report mapped from its production query histogram.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BifrostQueryTelemetryReport {
    /// Successful public query duration p99 in microseconds.
    pub query_latency_p99_us: f64,
}

impl BifrostQueryTelemetryReport {
    /// Map the server-integrated query field from production telemetry only.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostTelemetryReportError::EmptyHistogram`] when no successful
    /// query completed in the capture window.
    pub fn from_server_delta(
        delta: &BifrostTelemetryDelta,
    ) -> Result<Self, BifrostTelemetryReportError> {
        validate_query_label_contract(delta)?;
        Ok(Self {
            query_latency_p99_us: histogram_quantile_for_label(
                delta,
                "oracle_query_duration_seconds",
                "outcome",
                "success",
                0.99,
            )? * 1_000_000.0,
        })
    }
}

/// Reject unexpected labels or categorical values on the Oracle query histogram.
fn validate_query_label_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for sample in delta
        .metrics
        .iter()
        .filter(|sample| sample.family == "oracle_query_duration_seconds")
    {
        validate_sample_labels(
            sample,
            &["class", "outcome", "le"],
            &[
                ("class", &["interactive", "analytical"]),
                (
                    "outcome",
                    &["success", "degraded", "failed", "cancelled", "client_drop"],
                ),
            ],
        )?;
    }
    Ok(())
}

/// Validate one parsed production sample against its exact fixed-cardinality contract.
fn validate_sample_labels(
    sample: &BifrostMetricSample,
    allowed: &[&str],
    categorical: &[(&str, &[&str])],
) -> Result<(), BifrostTelemetryReportError> {
    if sample
        .labels
        .keys()
        .any(|label| !allowed.contains(&label.as_str()))
    {
        return Err(BifrostTelemetryReportError::Parse {
            detail: format!("unexpected label on {}", sample.family),
        });
    }
    for (key, values) in categorical {
        if let Some(value) = sample.labels.get(*key)
            && !values.contains(&value.as_str())
        {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("unexpected {key} value on {}", sample.family),
            });
        }
    }
    Ok(())
}

/// Parse every metric value from a production render.
fn rendered_values(rendered: &str) -> Result<BTreeMap<String, f64>, BifrostTelemetryReportError> {
    let mut values = BTreeMap::new();
    for line in rendered
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (series, value) =
            line.rsplit_once(' ')
                .ok_or_else(|| BifrostTelemetryReportError::Parse {
                    detail: "metric line has no value".to_owned(),
                })?;
        let value = value
            .parse::<f64>()
            .map_err(|_| BifrostTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            })?;
        if !value.is_finite() {
            return Err(BifrostTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            });
        }
        values.insert(series.to_owned(), value);
    }
    Ok(values)
}

/// Parse exact Prometheus family TYPE declarations, rejecting conflicts.
fn rendered_types(
    rendered: &str,
) -> Result<BTreeMap<String, PrometheusFamilyType>, BifrostTelemetryReportError> {
    let mut types = BTreeMap::new();
    for line in rendered.lines().filter(|line| line.starts_with("# TYPE ")) {
        let mut fields = line.split_whitespace();
        let _hash = fields.next();
        let _type_keyword = fields.next();
        let family = fields
            .next()
            .ok_or_else(|| BifrostTelemetryReportError::Parse {
                detail: "TYPE declaration omits family".to_owned(),
            })?;
        let family_type = match fields.next() {
            Some("counter") => PrometheusFamilyType::Counter,
            Some("gauge") => PrometheusFamilyType::Gauge,
            Some("histogram") => PrometheusFamilyType::Histogram,
            Some("summary") => PrometheusFamilyType::Summary,
            _ => {
                return Err(BifrostTelemetryReportError::Parse {
                    detail: format!("unsupported TYPE for {family}"),
                });
            }
        };
        if types
            .insert(family.to_owned(), family_type)
            .is_some_and(|prior| prior != family_type)
        {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("conflicting TYPE for {family}"),
            });
        }
    }
    Ok(types)
}

/// Return whether one rendered series belongs to a supported canonical kind.
fn supported_series(series: &str, types: &BTreeMap<String, PrometheusFamilyType>) -> bool {
    let name = series.split_once('{').map_or(series, |(name, _)| name);
    match sample_identity(name, types) {
        Ok(identity) => identity.is_some(),
        Err(_) => true,
    }
}

/// Resolve one rendered name through exact TYPE metadata and histogram bases.
fn sample_identity(
    name: &str,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<Option<(String, BifrostMetricKind)>, BifrostTelemetryReportError> {
    let histogram = [
        ("_bucket", BifrostMetricKind::HistogramBucket),
        ("_count", BifrostMetricKind::HistogramCount),
        ("_sum", BifrostMetricKind::HistogramSum),
    ]
    .into_iter()
    .find_map(|(suffix, kind)| name.strip_suffix(suffix).map(|base| (base, kind)))
    .filter(|(base, _)| types.get(*base) == Some(&PrometheusFamilyType::Histogram));
    if let Some((base, kind)) = histogram {
        if types.contains_key(name) {
            return Err(BifrostTelemetryReportError::Parse {
                detail: format!("conflicting exact and histogram TYPE for {name}"),
            });
        }
        return Ok(Some((base.to_owned(), kind)));
    }
    Ok(match types.get(name) {
        Some(PrometheusFamilyType::Counter) => Some((name.to_owned(), BifrostMetricKind::Counter)),
        Some(PrometheusFamilyType::Gauge) => Some((name.to_owned(), BifrostMetricKind::Gauge)),
        Some(PrometheusFamilyType::Histogram | PrometheusFamilyType::Summary) | None => None,
    })
}

/// Parse one constrained Prometheus series into its family and labels.
fn parse_sample(
    series: &str,
    value: f64,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<BifrostMetricSample, BifrostTelemetryReportError> {
    let (name, labels) =
        if let Some((name, labels)) = series.split_once('{') {
            (
                name,
                Some(labels.strip_suffix('}').ok_or_else(|| {
                    BifrostTelemetryReportError::Parse {
                        detail: "metric labels are not closed".to_owned(),
                    }
                })?),
            )
        } else {
            (series, None)
        };
    let (family, kind) =
        sample_identity(name, types)?.ok_or_else(|| BifrostTelemetryReportError::Parse {
            detail: format!("missing TYPE for {name}"),
        })?;
    let mut parsed = BTreeMap::new();
    if let Some(labels) = labels {
        for label in labels.split(',').filter(|label| !label.is_empty()) {
            let (key, quoted) =
                label
                    .split_once('=')
                    .ok_or_else(|| BifrostTelemetryReportError::Parse {
                        detail: "metric label is malformed".to_owned(),
                    })?;
            let value = quoted
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| (!quoted.contains('"')).then_some(quoted))
                .ok_or_else(|| BifrostTelemetryReportError::Parse {
                    detail: format!("metric label {key} is not quoted: {quoted}"),
                })?;
            if parsed.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(BifrostTelemetryReportError::Parse {
                    detail: "metric label is duplicated".to_owned(),
                });
            }
        }
    }
    Ok(BifrostMetricSample {
        family,
        labels: parsed,
        value,
        kind,
    })
}

/// Rebuild a normalized rendered-series identity from one parsed production sample.
fn rendered_series(sample: &BifrostMetricSample) -> String {
    let name = match sample.kind {
        BifrostMetricKind::HistogramBucket => format!("{}_bucket", sample.family),
        BifrostMetricKind::HistogramCount => format!("{}_count", sample.family),
        BifrostMetricKind::HistogramSum => format!("{}_sum", sample.family),
        BifrostMetricKind::Counter | BifrostMetricKind::Gauge => sample.family.clone(),
    };
    if sample.labels.is_empty() {
        return name;
    }
    let labels = sample
        .labels
        .iter()
        .map(|(key, value)| format!(r#"{key}="{value}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}{{{labels}}}")
}

/// Estimate a histogram quantile after selecting one exact closed label.
fn histogram_quantile_for_label(
    delta: &BifrostTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    histogram_quantile_state_for_label(delta, family, key, value, quantile)?.ok_or_else(|| {
        BifrostTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        }
    })
}

/// Validate every categorical histogram tuple before deriving one family quantile.
///
/// A structurally complete all-zero family has no observations and returns
/// `None`. Complete idle tuples may coexist with active tuples and contribute
/// zero to the aggregate distribution.
///
/// # Errors
/// Returns a typed histogram error for missing or duplicate components,
/// mismatched bucket boundaries, invalid values, nonmonotonic buckets,
/// count mismatches, or an overflow-only positive distribution.
fn histogram_quantile_state_for_label(
    delta: &BifrostTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<Option<f64>, BifrostTelemetryReportError> {
    type TupleKey = Vec<(String, String)>;
    type TupleParts = (Vec<f64>, Vec<f64>, BTreeMap<String, Vec<f64>>);

    let invalid = |detail: &str| BifrostTelemetryReportError::InvalidBinding {
        id: family.to_owned(),
        detail: detail.to_owned(),
    };
    let mut tuples = BTreeMap::<TupleKey, TupleParts>::new();
    for sample in delta.metrics.iter().filter(|sample| {
        sample.family == family
            && (key.is_empty() || sample.labels.get(key).map(String::as_str) == Some(value))
    }) {
        let tuple = sample
            .labels
            .iter()
            .filter(|(label, _)| label.as_str() != "le")
            .map(|(label, value)| (label.clone(), value.clone()))
            .collect::<Vec<_>>();
        let parts = tuples.entry(tuple).or_default();
        match sample.kind {
            BifrostMetricKind::HistogramBucket => {
                let upper = sample
                    .labels
                    .get("le")
                    .ok_or_else(|| invalid("histogram bucket omits le"))?;
                parts.2.entry(upper.clone()).or_default().push(sample.value);
            }
            BifrostMetricKind::HistogramCount => parts.0.push(sample.value),
            BifrostMetricKind::HistogramSum => parts.1.push(sample.value),
            _ => return Err(invalid("histogram family contains a non-histogram sample")),
        }
    }
    if tuples.is_empty() {
        return Err(BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        });
    }

    let mut expected_boundaries: Option<Vec<String>> = None;
    let mut aggregate = BTreeMap::<String, f64>::new();
    let mut aggregate_count = 0.0;
    let mut any_active = false;
    for (counts, sums, buckets) in tuples.values() {
        if counts.len() != 1 || sums.len() != 1 || buckets.is_empty() {
            return Err(invalid(
                "each histogram tuple requires exactly one count, one sum, and buckets",
            ));
        }
        if buckets.values().any(|values| values.len() != 1) {
            return Err(invalid(
                "histogram tuple contains a duplicate bucket boundary",
            ));
        }
        let boundaries = buckets.keys().cloned().collect::<Vec<_>>();
        if expected_boundaries
            .as_ref()
            .is_some_and(|expected| expected != &boundaries)
        {
            return Err(invalid(
                "histogram tuples have mismatched bucket boundaries",
            ));
        }
        expected_boundaries.get_or_insert(boundaries);
        let count = counts[0];
        let sum = sums[0];
        if !count.is_finite() || !sum.is_finite() || count < 0.0 || sum < 0.0 {
            return Err(invalid("histogram count or sum is invalid"));
        }
        let idle = count == 0.0 && sum == 0.0 && buckets.values().all(|values| values[0] == 0.0);
        if idle {
            continue;
        }
        any_active = true;
        if count <= 0.0 {
            return Err(invalid("active histogram tuple has no positive count"));
        }
        let mut ordered = buckets
            .iter()
            .map(|(upper, values)| {
                let parsed = match upper.as_str() {
                    "+Inf" | "Inf" => Ok(f64::INFINITY),
                    _ => upper
                        .parse::<f64>()
                        .map_err(|_| invalid("histogram bucket boundary is invalid")),
                }?;
                Ok((parsed, upper, values[0]))
            })
            .collect::<Result<Vec<_>, BifrostTelemetryReportError>>()?;
        ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut previous = 0.0;
        for (_, upper, cumulative) in &ordered {
            if !cumulative.is_finite() || *cumulative < previous || *cumulative < 0.0 {
                return Err(invalid("histogram buckets are nonmonotonic"));
            }
            previous = *cumulative;
            *aggregate.entry((*upper).clone()).or_default() += *cumulative;
        }
        if ordered.last().map(|(_, _, value)| *value) != Some(count) {
            return Err(invalid("histogram +Inf bucket does not equal count"));
        }
        let target = (count * quantile).ceil();
        if !ordered
            .iter()
            .any(|(upper, _, cumulative)| upper.is_finite() && *cumulative >= target)
        {
            return Err(invalid("histogram positive observations are overflow-only"));
        }
        aggregate_count += count;
    }
    if !any_active {
        return Ok(None);
    }
    let target = (aggregate_count * quantile).ceil();
    let mut ordered = aggregate
        .into_iter()
        .map(|(upper, cumulative)| {
            let parsed = match upper.as_str() {
                "+Inf" | "Inf" => Ok(f64::INFINITY),
                _ => upper
                    .parse::<f64>()
                    .map_err(|_| invalid("histogram bucket boundary is invalid")),
            }?;
            Ok((parsed, cumulative))
        })
        .collect::<Result<Vec<_>, BifrostTelemetryReportError>>()?;
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    ordered
        .into_iter()
        .find(|(upper, cumulative)| upper.is_finite() && *cumulative >= target)
        .map(|(upper, _)| Some(upper))
        .ok_or_else(|| invalid("histogram aggregate is overflow-only"))
}

#[cfg(test)]
/// Estimate a histogram quantile from changed Prometheus cumulative buckets.
pub(crate) fn histogram_quantile(
    delta: &BifrostTelemetryDelta,
    family: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    histogram_quantile_for_label(delta, family, "", "", quantile)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Active Oracle queries are an up/down gauge despite their bounded labels.
    #[test]
    fn oracle_active_queries_are_not_monotonic() {
        let types = BTreeMap::from([(
            "oracle_queries_active".to_owned(),
            PrometheusFamilyType::Gauge,
        )]);
        assert_eq!(
            parse_sample("oracle_queries_active{class=\"analytical\"}", 1.0, &types,)
                .unwrap()
                .kind,
            BifrostMetricKind::Gauge
        );
    }

    /// Proves histogram suffixes retain distinct kinds before family normalization.
    #[test]
    fn parser_preserves_exact_metric_kind() {
        let types = BTreeMap::from([
            (
                "bifrost_gate_query_stream_duration_seconds".to_owned(),
                PrometheusFamilyType::Histogram,
            ),
            (
                "bifrost_gate_requests_total".to_owned(),
                PrometheusFamilyType::Counter,
            ),
        ]);
        let bucket = parse_sample(
            "bifrost_gate_query_stream_duration_seconds_bucket{le=\"1\",outcome=\"success\"}",
            1.0,
            &types,
        )
        .expect("exact production bucket parses");
        assert_eq!(bucket.family, "bifrost_gate_query_stream_duration_seconds");
        assert_eq!(bucket.kind, BifrostMetricKind::HistogramBucket);
        let counter = parse_sample(
            "bifrost_gate_requests_total{operation=\"query\",outcome=\"success\"}",
            1.0,
            &types,
        )
        .expect("exact production counter parses");
        assert_eq!(counter.kind, BifrostMetricKind::Counter);
    }

    /// Histogram suffixes inherit only an explicit base histogram TYPE.
    #[test]
    fn parser_normalizes_histogram_type_without_guessing_suffixes() {
        let base = "bifrost_gate_resolution_seconds";
        let types = BTreeMap::from([(base.to_owned(), PrometheusFamilyType::Histogram)]);
        for (suffix, expected) in [
            ("_bucket{le=\"0.1\"}", BifrostMetricKind::HistogramBucket),
            ("_count", BifrostMetricKind::HistogramCount),
            ("_sum", BifrostMetricKind::HistogramSum),
        ] {
            let parsed = parse_sample(&format!("{base}{suffix}"), 1.0, &types)
                .expect("base histogram TYPE classifies rendered suffix");
            assert_eq!(parsed.family, base);
            assert_eq!(parsed.kind, expected);
        }

        let exact = BTreeMap::from([
            ("jobs_count".to_owned(), PrometheusFamilyType::Counter),
            ("queue_total".to_owned(), PrometheusFamilyType::Gauge),
        ]);
        assert_eq!(
            parse_sample("jobs_count", 1.0, &exact)
                .expect("exact count-named counter")
                .kind,
            BifrostMetricKind::Counter
        );
        assert_eq!(
            parse_sample("queue_total", 1.0, &exact)
                .expect("exact total-named gauge")
                .kind,
            BifrostMetricKind::Gauge
        );

        let missing = parse_sample("required_seconds_count", 1.0, &BTreeMap::new())
            .expect_err("required histogram suffix needs a base TYPE");
        assert!(
            missing
                .to_string()
                .contains("missing TYPE for required_seconds_count")
        );
        assert!(!supported_series(
            "unrelated_seconds_count",
            &BTreeMap::new()
        ));

        let conflicting = BTreeMap::from([
            (base.to_owned(), PrometheusFamilyType::Histogram),
            (format!("{base}_count"), PrometheusFamilyType::Counter),
        ]);
        let conflict = parse_sample(&format!("{base}_count"), 1.0, &conflicting)
            .expect_err("exact suffix TYPE conflicts with histogram base");
        assert!(
            conflict
                .to_string()
                .contains("conflicting exact and histogram TYPE")
        );
    }

    /// Proves names never override declared Prometheus family types.
    #[test]
    fn parser_uses_type_for_nonstandard_counter_and_gauge_names() {
        let rendered =
            "# TYPE queue_total gauge\nqueue_total 2\n# TYPE requests counter\nrequests 3\n";
        let types = rendered_types(rendered).expect("TYPE declarations parse");
        assert_eq!(
            parse_sample("queue_total", 2.0, &types).unwrap().kind,
            BifrostMetricKind::Gauge
        );
        assert_eq!(
            parse_sample("requests", 3.0, &types).unwrap().kind,
            BifrostMetricKind::Counter
        );
        assert!(parse_sample("missing", 1.0, &types).is_err());
        assert!(rendered_types("# TYPE requests counter\n# TYPE requests gauge\n").is_err());
    }

    /// Proves window buckets, rather than sum, define p99.
    #[test]
    fn histogram_window_p99_uses_bucket_delta() {
        let family = "vala_postgres_pool_acquire_seconds";
        let mut delta = BifrostTelemetryDelta {
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        for (le, value) in [("0.001", 98.0), ("0.025", 99.0), ("+Inf", 100.0)] {
            delta.metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([
                    ("le".to_owned(), le.to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                    ("pool".to_owned(), "runtime".to_owned()),
                ]),
                value,
                kind: BifrostMetricKind::HistogramBucket,
            });
        }
        delta.metrics.push(BifrostMetricSample {
            family: family.to_owned(),
            labels: BTreeMap::from([
                ("outcome".to_owned(), "success".to_owned()),
                ("pool".to_owned(), "runtime".to_owned()),
            ]),
            value: 100.0,
            kind: BifrostMetricKind::HistogramCount,
        });
        delta.metrics.push(BifrostMetricSample {
            family: family.to_owned(),
            labels: BTreeMap::from([
                ("outcome".to_owned(), "success".to_owned()),
                ("pool".to_owned(), "runtime".to_owned()),
            ]),
            value: 0.5,
            kind: BifrostMetricKind::HistogramSum,
        });
        assert_eq!(histogram_quantile(&delta, family, 0.99).unwrap(), 0.025);
    }

    /// Proves independent accepted label series aggregate by bucket boundary.
    #[test]
    fn histogram_quantile_aggregates_label_series_by_bucket() {
        let family = "wyrd_storage_operation_duration_seconds";
        let mut metrics = Vec::new();
        for (operation, buckets, count) in [
            (
                "get",
                [("0.001", 49.0), ("0.025", 50.0), ("+Inf", 50.0)],
                50.0,
            ),
            (
                "put",
                [("0.001", 49.0), ("0.025", 50.0), ("+Inf", 50.0)],
                50.0,
            ),
        ] {
            for (le, value) in buckets {
                metrics.push(BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::from([
                        ("operation".to_owned(), operation.to_owned()),
                        ("le".to_owned(), le.to_owned()),
                    ]),
                    value,
                    kind: BifrostMetricKind::HistogramBucket,
                });
            }
            metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([("operation".to_owned(), operation.to_owned())]),
                value: count,
                kind: BifrostMetricKind::HistogramCount,
            });
            metrics.push(BifrostMetricSample {
                family: family.to_owned(),
                labels: BTreeMap::from([("operation".to_owned(), operation.to_owned())]),
                value: 0.5,
                kind: BifrostMetricKind::HistogramSum,
            });
        }
        let delta = BifrostTelemetryDelta {
            metrics,
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        assert_eq!(histogram_quantile(&delta, family, 0.99).unwrap(), 0.025);
    }

    /// Proves empty, nonmonotonic, reset, and infinity-only windows are rejected.
    #[test]
    fn histogram_window_rejects_invalid_bucket_shapes() {
        let family = "test_duration_seconds";
        let make = |buckets: &[(&str, f64)], count: f64| BifrostTelemetryDelta {
            metrics: buckets
                .iter()
                .map(|(le, value)| BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::from([("le".to_owned(), (*le).to_owned())]),
                    value: *value,
                    kind: BifrostMetricKind::HistogramBucket,
                })
                .chain(std::iter::once(BifrostMetricSample {
                    family: family.to_owned(),
                    labels: BTreeMap::new(),
                    value: count,
                    kind: BifrostMetricKind::HistogramCount,
                }))
                .collect(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        assert!(histogram_quantile(&make(&[], 0.0), family, 0.99).is_err());
        assert!(
            histogram_quantile(&make(&[("1", 2.0), ("+Inf", 1.0)], 1.0), family, 0.99).is_err()
        );
        assert!(
            histogram_quantile(&make(&[("1", -1.0), ("+Inf", 1.0)], 1.0), family, 0.99).is_err()
        );
        assert!(histogram_quantile(&make(&[("+Inf", 1.0)], 1.0), family, 0.99).is_err());
    }

    /// Proves stable epochs calculate deltas, replacement starts a new epoch, and reset fails.
    #[test]
    fn process_window_handles_peaks_replacement_and_reset() {
        let start = test_process_sample();
        let stable = ProcessSample {
            cpu_total: 3.0,
            tokio_busy_total: 2.5,
            rss_bytes: 2,
            queue_depth: 1,
            ..start.clone()
        };
        let window = process_window(&start, stable, 9, 7).unwrap();
        assert_eq!(
            (
                window.cpu_seconds,
                window.tokio_busy_seconds,
                window.peak_rss_bytes,
                window.queue_peak
            ),
            (2.0, 1.5, 9, 7)
        );
        let replacement = ProcessSample {
            identity: "pid-next".to_owned(),
            epoch: 1,
            cpu_total: 0.5,
            tokio_busy_total: 0.25,
            ..start.clone()
        };
        assert_eq!(process_window(&start, replacement, 4, 3).unwrap().epoch, 1);
        let reset = ProcessSample {
            cpu_total: 0.5,
            ..start.clone()
        };
        assert!(process_window(&start, reset, 1, 1).is_err());
    }

    /// Proves the bounded sampler cancellation signal terminates and joins its owner task.
    #[tokio::test]
    async fn gauge_sampler_cancellation_cleans_up_owner_task() {
        let stop = tokio_util::sync::CancellationToken::new();
        let task_stop = stop.clone();
        let task = tokio::spawn(async move {
            task_stop.cancelled().await;
            Ok(SamplerSnapshot {
                maxima: BTreeMap::new(),
                process: test_process_sample(),
                peak_rss_bytes: 1,
                queue_peak: 1,
            })
        });
        let sampler = BifrostTelemetrySampler { stop, task };
        sampler.stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), sampler.task)
            .await
            .expect("bounded sampler joins before its cleanup deadline")
            .expect("bounded sampler task exits without panic")
            .expect("bounded sampler reports clean cancellation");
    }

    /// Proves sampler task panic and polling failures remain distinct typed errors.
    #[tokio::test]
    async fn sampler_join_propagates_panic_and_polling_failure() {
        let panic: tokio::task::JoinHandle<Result<SamplerSnapshot, BifrostTelemetryReportError>> =
            tokio::spawn(async move { panic!("injected sampler panic") });
        assert!(matches!(
            join_sampler(panic).await,
            Err(BifrostTelemetryReportError::SamplerJoin { .. })
        ));

        let polling = tokio::spawn(async move {
            Err(BifrostTelemetryReportError::Parse {
                detail: "injected malformed scrape".to_owned(),
            })
        });
        assert!(matches!(
            join_sampler(polling).await,
            Err(BifrostTelemetryReportError::Parse { .. })
        ));
    }

    /// Build deterministic process evidence for sampler-only unit fixtures.
    fn test_process_sample() -> ProcessSample {
        ProcessSample {
            identity: "pid-test".to_owned(),
            epoch: 0,
            cpu_total: 1.0,
            rss_bytes: 1,
            tokio_busy_total: 1.0,
            queue_depth: 1,
        }
    }

    /// Proves a window's elapsed time and process resources render with the
    /// whole-process scope they were measured at.
    #[test]
    fn resources_render_elapsed_and_process_scope() {
        let delta = BifrostTelemetryDelta {
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 0.25,
            process: ProcessWindow {
                cpu_seconds: 0.5,
                current_rss_bytes: 3,
                peak_rss_bytes: 7,
                tokio_busy_seconds: 0.75,
                ..test_process_window()
            },
        };
        assert_eq!(
            delta.resources(),
            "elapsed_s=0.25 process_cpu_s=0.5 tokio_busy_s=0.75 rss_end_bytes=3 rss_peak_bytes=7"
        );
    }

    /// Build deterministic process-window evidence for projection unit fixtures.
    fn test_process_window() -> ProcessWindow {
        ProcessWindow {
            identity: "pid-test".to_owned(),
            epoch: 0,
            cpu_seconds: 1.0,
            current_rss_bytes: 1,
            peak_rss_bytes: 1,
            tokio_busy_seconds: 1.0,
            queue_peak: 1,
        }
    }

    /// Retains an in-window production gauge peak in one entry per series.
    #[test]
    fn gauge_maximum_accumulator_retains_transient_peak_without_tick_history() {
        let series = "bifrost_resource_current_bytes{resource=\"memory\",role=\"forge\"}";
        let mut maxima = BTreeMap::new();
        let types = BTreeMap::from([
            (
                "bifrost_resource_current_bytes".to_owned(),
                PrometheusFamilyType::Gauge,
            ),
            (
                "bifrost_forge_operations_total".to_owned(),
                PrometheusFamilyType::Counter,
            ),
        ]);
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 4.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 32.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([(series.to_owned(), 1.0)]),
            &types,
        )
        .unwrap();
        merge_gauge_maxima(
            &mut maxima,
            &BTreeMap::from([("bifrost_forge_operations_total".to_owned(), 99.0)]),
            &types,
        )
        .unwrap();

        assert_eq!(maxima, BTreeMap::from([(series.to_owned(), 32.0)]));
    }
}
