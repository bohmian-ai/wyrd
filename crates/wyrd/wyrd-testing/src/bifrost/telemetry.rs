//! Read-only parsing and validation of production Forge telemetry windows.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use metrics_exporter_prometheus::PrometheusHandle;
use wyrd_telemetry::{CapturedSpan, TestTraceCapture};

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

/// Window aggregation required by one closed telemetry binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryAggregation {
    /// Monotonic counter difference.
    Delta,
    /// Maximum sampled gauge value.
    Peak,
    /// Final sampled gauge value.
    Final,
    /// Histogram-derived ninety-ninth percentile.
    P99,
}

/// Normative unit encoded by an exact production family convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryUnit {
    /// Dimensionless operation or row count.
    Count,
    /// Bytes.
    Bytes,
    /// Seconds.
    Seconds,
}

/// Closed requirement policy for the current representative workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TelemetryRequirement {
    /// Every canonical cluster window requires the binding.
    Always,
    /// Only windows executing the named role require the binding.
    Role(&'static str),
}

/// One exact allowed value domain for a bounded metric label.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TelemetryLabelValues {
    /// Exact label key.
    key: &'static str,
    /// Closed allowed values.
    values: &'static [&'static str],
}

/// Stable identifier for one private closed telemetry binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TelemetryBindingId(pub(crate) &'static str);

/// Exact production metric binding used by the canonical cluster projection.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TelemetryBinding {
    /// Stable report-local binding identity.
    id: TelemetryBindingId,
    /// Exact normalized production family.
    family: &'static str,
    /// Required Prometheus sample kind.
    kind: BifrostMetricKind,
    /// Normative family unit.
    unit: TelemetryUnit,
    /// Required window aggregation.
    aggregation: TelemetryAggregation,
    /// Closed workload requirement.
    requirement: TelemetryRequirement,
    /// Exact allowed label keys.
    allowed_label_keys: &'static [&'static str],
    /// Closed categorical label domains.
    allowed_label_values: &'static [TelemetryLabelValues],
}

/// Closed exact binding ledger shared by qualification and capacity projection.
const CLUSTER_BINDINGS: &[TelemetryBinding] = &[
    TelemetryBinding {
        id: TelemetryBindingId("gate.requests"),
        family: "bifrost_gate_requests_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["operation", "outcome"],
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.rows"),
        family: "bifrost_gate_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["operation", "outcome"],
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "operation",
                values: &["query", "write"],
            },
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "rejected", "failed", "cancelled"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.active"),
        family: "bifrost_gate_active_streams",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["operation"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "operation",
            values: &["query", "write"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("gate.query_streams"),
        family: "bifrost_gate_query_streams_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["outcome"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "failed", "cancelled"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.rows"),
        family: "bifrost_scribe_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_keys: &["status"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "status",
            values: &["accepted"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.backlog_peak"),
        family: "bifrost_forge_oldest_backlog_seconds",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::Peak,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_keys: &[],
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.slots_final"),
        family: "bifrost_oracle_slots_total",
        kind: BifrostMetricKind::Gauge,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Final,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_keys: &["role"],
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.wal_bytes"),
        family: "bifrost_scribe_wal_append_bytes_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Bytes,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_keys: &[],
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("scribe.seal_rows"),
        family: "bifrost_scribe_seal_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_keys: &["stage"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "stage",
            values: &["file_list_transaction"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("forge.publications"),
        family: "bifrost_forge_complete_gauge_publications_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("forge"),
        allowed_label_keys: &[],
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("oracle.rows"),
        family: "bifrost_oracle_stream_rows_total",
        kind: BifrostMetricKind::Counter,
        unit: TelemetryUnit::Count,
        aggregation: TelemetryAggregation::Delta,
        requirement: TelemetryRequirement::Role("oracle"),
        allowed_label_keys: &["outcome"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "failed", "cancelled"],
        }],
    },
    TelemetryBinding {
        id: TelemetryBindingId("postgres.acquire"),
        family: "vala_postgres_pool_acquire_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["le", "outcome", "pool"],
        allowed_label_values: &[
            TelemetryLabelValues {
                key: "outcome",
                values: &["success", "failed", "cancelled"],
            },
            TelemetryLabelValues {
                key: "pool",
                values: &["runtime"],
            },
        ],
    },
    TelemetryBinding {
        id: TelemetryBindingId("storage.duration"),
        family: "wyrd_storage_operation_duration_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Always,
        allowed_label_keys: &["backend", "le", "operation", "outcome"],
        allowed_label_values: &[],
    },
    TelemetryBinding {
        id: TelemetryBindingId("wal.fsync"),
        family: "bifrost_scribe_wal_fsync_seconds",
        kind: BifrostMetricKind::HistogramBucket,
        unit: TelemetryUnit::Seconds,
        aggregation: TelemetryAggregation::P99,
        requirement: TelemetryRequirement::Role("scribe"),
        allowed_label_keys: &["le", "outcome"],
        allowed_label_values: &[TelemetryLabelValues {
            key: "outcome",
            values: &["success", "failed", "cancelled"],
        }],
    },
];

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
    /// Active role topology diverged from immutable launched-role evidence.
    #[error("production role topology does not match executed roles")]
    TopologyMismatch,
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
            .flat_map(|values| values.into_iter())
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
        if current_types != checkpoint.types {
            return Err(BifrostTelemetryReportError::Parse {
                detail: "Prometheus TYPE metadata changed within one window".to_owned(),
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
            if parsed.kind != BifrostMetricKind::Gauge {
                if *value < prior {
                    return Err(BifrostTelemetryReportError::CounterRegression {
                        series: series.clone(),
                    });
                }
                let mut sample = parsed;
                sample.value = *value - prior;
                metrics.push(sample);
            } else {
                let baseline = parse_sample(series, prior, &current_types)?;
                let end = parse_sample(series, *value, &current_types)?;
                gauge_maxima.push(BifrostMetricSample {
                    family: end.family.clone(),
                    labels: end.labels.clone(),
                    value: baseline.value.max(end.value),
                    kind: end.kind,
                });
                gauge_final.push(end);
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
    pub async fn begin_gauge_sampling(
        &self,
        _checkpoint: &BifrostTelemetryCheckpoint,
    ) -> Result<BifrostTelemetrySampler, BifrostTelemetryReportError> {
        let initial_rendered = self.metrics.render();
        let initial = rendered_values(&initial_rendered)?;
        let initial_types = rendered_types(&initial_rendered)?;
        let mut initial_maxima = BTreeMap::new();
        merge_gauge_maxima(&mut initial_maxima, &initial, &initial_types)?;
        let initial_process = process_sample(_checkpoint.process.epoch)?;
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
            // millisecond cadence so the benchmark's peak remains an observed exporter
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
                        if next.identity != snapshot.process.identity {
                            snapshot.process = ProcessSample { epoch: snapshot.process.epoch.checked_add(1).ok_or_else(|| BifrostTelemetryReportError::Parse { detail: "process epoch overflow".to_owned() })?, ..next };
                            snapshot.peak_rss_bytes = snapshot.process.rss_bytes;
                            snapshot.queue_peak = snapshot.process.queue_depth;
                        } else {
                            if next.cpu_total < snapshot.process.cpu_total || next.tokio_busy_total < snapshot.process.tokio_busy_total {
                                return Err(BifrostTelemetryReportError::CounterRegression { series: "process cumulative counters".to_owned() });
                            }
                            snapshot.peak_rss_bytes = snapshot.peak_rss_bytes.max(next.rss_bytes);
                            snapshot.queue_peak = snapshot.queue_peak.max(next.queue_depth);
                            snapshot.process = next;
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
        let sampled =
            sampler
                .task
                .await
                .map_err(|error| BifrostTelemetryReportError::SamplerJoin {
                    detail: error.to_string(),
                })??;
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

/// Validate the canonical closed cluster binding ledger against one capture delta.
///
/// # Errors
/// Returns [`BifrostTelemetryReportError::InvalidBinding`] for a missing family,
/// wrong kind or unit convention, unknown label, open categorical value,
/// non-finite value, or empty required histogram.
pub(crate) fn validate_cluster_bindings(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for binding in CLUSTER_BINDINGS {
        let _aggregation = binding.aggregation;
        let _required = match binding.requirement {
            TelemetryRequirement::Always => true,
            TelemetryRequirement::Role(role) => !role.is_empty(),
        };
        let unit_valid = match binding.unit {
            TelemetryUnit::Count => {
                !binding.family.ends_with("_seconds") && !binding.family.ends_with("_bytes_total")
            }
            TelemetryUnit::Bytes => binding.family.ends_with("_bytes_total"),
            TelemetryUnit::Seconds => binding.family.ends_with("_seconds"),
        };
        if !unit_valid {
            return Err(invalid_binding(
                binding,
                "family violates its normative unit suffix",
            ));
        }
        let source = match binding.aggregation {
            TelemetryAggregation::Delta | TelemetryAggregation::P99 => &delta.metrics,
            TelemetryAggregation::Peak => &delta.gauge_maxima,
            TelemetryAggregation::Final => &delta.gauge_final,
        };
        let samples = source
            .iter()
            .filter(|sample| sample.family == binding.family && sample.kind == binding.kind)
            .collect::<Vec<_>>();
        if samples.is_empty() {
            return Err(invalid_binding(
                binding,
                "required family and kind are absent",
            ));
        }
        for sample in samples {
            if !sample.value.is_finite() {
                return Err(invalid_binding(binding, "sample value is not finite"));
            }
            if sample
                .labels
                .keys()
                .any(|key| !binding.allowed_label_keys.contains(&key.as_str()))
            {
                return Err(invalid_binding(binding, "sample contains an unknown label"));
            }
            if binding.kind == BifrostMetricKind::HistogramBucket
                && !sample.labels.contains_key("le")
            {
                return Err(invalid_binding(binding, "histogram bucket omits le"));
            }
            for domain in binding.allowed_label_values {
                if sample
                    .labels
                    .get(domain.key)
                    .is_some_and(|value| !domain.values.contains(&value.as_str()))
                {
                    return Err(invalid_binding(
                        binding,
                        "sample contains an open label value",
                    ));
                }
            }
        }
    }
    if delta
        .spans
        .iter()
        .any(|span| matches!(span.status, wyrd_telemetry::CapturedSpanStatus::Error(_)))
    {
        return Err(BifrostTelemetryReportError::InvalidBinding {
            id: "trace.status".to_owned(),
            detail: "capture contains an error-status span".to_owned(),
        });
    }
    Ok(())
}

/// Construct one stable binding failure without exposing metric values.
fn invalid_binding(binding: &TelemetryBinding, detail: &str) -> BifrostTelemetryReportError {
    BifrostTelemetryReportError::InvalidBinding {
        id: binding.id.0.to_owned(),
        detail: detail.to_owned(),
    }
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
        if parse_sample(series, *value, types)?.kind == BifrostMetricKind::Gauge {
            maxima
                .entry(series.clone())
                .and_modify(|maximum| *maximum = maximum.max(*value))
                .or_insert(*value);
        }
    }
    Ok(())
}

/// Forge-only report mapped solely from production capture observations.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeMaintenanceTelemetryReport {
    /// Production rewrite-byte counter rate converted to MiB/s.
    pub throughput_mib_per_sec: f64,
    /// Production task duration p99 in microseconds.
    pub task_latency_p99_us: f64,
    /// Maximum complete-pass backlog age in microseconds.
    pub backlog_age_us: f64,
    /// Peak Forge memory reservation gauge.
    pub peak_parent_memory: f64,
    /// Maximum final spill observation.
    pub spill_bytes: f64,
    /// Lease-contention counter delta.
    pub lease_contention: f64,
    /// Fence-loss counter delta.
    pub fence_lost: f64,
    /// Snapshot-change counter delta.
    pub snapshot_changed: f64,
    /// Maximum complete-pass fair-admission lag.
    pub fairness_lag_tasks: f64,
    /// Maximum cleanup duration in microseconds.
    pub cleanup_delay_us: f64,
    /// Production-observed active concurrency and start history by exact role.
    pub role_topology: BTreeMap<String, ForgeRoleTopologyReport>,
}

/// Production topology evidence for one configured process role.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgeRoleTopologyReport {
    /// Maximum active owners sampled during the observation window.
    pub max_active: u64,
    /// Active owners in the final pre-shutdown exporter snapshot.
    pub final_active: u64,
    /// Starts observed during the window, including replacements.
    pub starts: u64,
}

impl ForgeMaintenanceTelemetryReport {
    /// Map every Forge-only R13 field from one production exporter window.
    ///
    /// # Errors
    ///
    /// Returns a typed error when any required production family, activity
    /// counter, histogram, or expected active role is absent or stale.
    pub fn from_production_delta(
        delta: &BifrostTelemetryDelta,
        expected_roles: &BTreeMap<String, u64>,
    ) -> Result<Self, BifrostTelemetryReportError> {
        validate_forge_span_contract(delta)?;
        validate_forge_label_contract(delta)?;
        let role_topology = validate_role_topology(delta, expected_roles)?;
        require_advanced(delta, "bifrost_forge_complete_gauge_publications_total")?;
        require_advanced(delta, "bifrost_memory_reservations_total")?;
        let rewrite_bytes = sum(delta, "bifrost_forge_rewrite_output_bytes_total", &[])?;
        let task_latency_p99_us = histogram_quantile_for_label(
            delta,
            "bifrost_forge_task_duration_seconds",
            "result",
            "succeeded",
            0.99,
        )? * 1_000_000.0;
        let spill_bytes = histogram_quantile(delta, "bifrost_forge_task_spill_bytes", 1.0)?;
        let cleanup_delay_us =
            histogram_quantile(delta, "bifrost_forge_cleanup_duration_seconds", 1.0)? * 1_000_000.0;
        let backlog_age_us =
            gauge(delta, "bifrost_forge_oldest_backlog_seconds", &[])? * 1_000_000.0;
        let peak_parent_memory = gauge(
            delta,
            "bifrost_memory_reserved_bytes",
            &[("consumer", "forge")],
        )?;
        let fairness_lag_tasks = gauge(delta, "bifrost_forge_fairness_lag_tasks", &[])?;
        Ok(Self {
            throughput_mib_per_sec: rewrite_bytes / (1024.0 * 1024.0 * delta.interval_seconds),
            task_latency_p99_us,
            backlog_age_us,
            peak_parent_memory,
            spill_bytes,
            lease_contention: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "lease_contention")],
            )?,
            fence_lost: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "fence_lost")],
            )?,
            snapshot_changed: sum(
                delta,
                "bifrost_forge_conflicts_total",
                &[("kind", "snapshot_changed")],
            )?,
            fairness_lag_tasks,
            cleanup_delay_us,
            role_topology,
        })
    }
}

/// Validate all exact-run Forge spans against their closed production schemas.
///
/// Every captured Forge-prefixed span must be one of the four normative names,
/// and every instance must carry its required owner-authored attributes. Known
/// tracing-provider semantic attributes are tolerated because they are added
/// after owner instrumentation; task and attempt UUIDs remain the sole
/// permitted owner-authored high-cardinality values.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::MissingSpan`] when a required owner did
/// not emit, or [`BifrostTelemetryReportError::InvalidSpan`] for renamed spans,
/// missing/extra attributes, open values, or malformed scrubbed UUIDs.
fn validate_forge_span_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    let required = [
        "bifrost.forge.scheduler.pass",
        "bifrost.forge.task.execute",
        "bifrost.forge.catalog.commit",
        "bifrost.forge.cleanup",
    ];
    let mut seen = BTreeSet::new();
    for span in &delta.spans {
        match span.name.as_str() {
            "bifrost.forge.scheduler.pass" => {
                validate_span_attribute_set(span, &["result", "role"])?;
                validate_closed_span_attribute(span, "result", &["succeeded", "failed"])?;
                validate_closed_span_attribute(span, "role", &["server"])?;
            }
            "bifrost.forge.task.execute" => {
                validate_span_attribute_set(
                    span,
                    &["attempt_id", "result", "role", "strategy", "task_id"],
                )?;
                validate_closed_span_attribute(
                    span,
                    "strategy",
                    &["staging_fold", "small_files", "snapshot_expiry"],
                )?;
                validate_closed_span_attribute(span, "result", &["succeeded", "failed"])?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            "bifrost.forge.catalog.commit" => {
                validate_span_attribute_set(
                    span,
                    &["attempt_id", "result", "role", "strategy", "task_id"],
                )?;
                validate_closed_span_attribute(span, "strategy", &["staging_fold", "small_files"])?;
                validate_closed_span_attribute(
                    span,
                    "result",
                    &["succeeded", "failed", "timed_out", "cancelled"],
                )?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            "bifrost.forge.cleanup" => {
                validate_span_attribute_set(
                    span,
                    &[
                        "attempt_id",
                        "kind",
                        "result",
                        "role",
                        "strategy",
                        "task_id",
                    ],
                )?;
                validate_closed_span_attribute(span, "kind", &["expired"])?;
                validate_closed_span_attribute(span, "strategy", &["snapshot_expiry"])?;
                validate_closed_span_attribute(span, "result", &["succeeded", "failed"])?;
                validate_closed_span_attribute(span, "role", &["forge_worker"])?;
                validate_span_uuid(span, "task_id")?;
                validate_span_uuid(span, "attempt_id")?;
            }
            name if name.starts_with("bifrost.forge.") => {
                return Err(BifrostTelemetryReportError::InvalidSpan {
                    span: name.to_owned(),
                    detail: "unexpected Forge instrumentation name".to_owned(),
                });
            }
            _ => continue,
        }
        seen.insert(span.name.as_str());
    }
    for name in required {
        if !seen.contains(name) {
            return Err(BifrostTelemetryReportError::MissingSpan {
                span: name.to_owned(),
            });
        }
    }
    Ok(())
}

/// Require one captured span to expose its approved owner-authored attributes.
///
/// The tracing provider may append its standard source, thread, and timing
/// attributes. All other additions remain invalid so owner instrumentation
/// cannot leak tenant data, object paths, SQL text, or error text.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when a required field is
/// absent or any prohibited, sensitive, or otherwise unapproved field appears.
fn validate_span_attribute_set(
    span: &CapturedSpan,
    expected: &[&str],
) -> Result<(), BifrostTelemetryReportError> {
    let actual = span
        .attributes
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let provider = [
        "busy_ns",
        "code.filepath",
        "code.lineno",
        "code.namespace",
        "idle_ns",
        "thread.id",
        "thread.name",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
    let unexpected = actual
        .difference(&expected)
        .filter(|key| !provider.contains(**key))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() || !unexpected.is_empty() {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!(
                "closed attribute contract has missing keys {missing:?} and unexpected keys {unexpected:?}"
            ),
        });
    }
    Ok(())
}

/// Validate one fixed-cardinality span attribute against its closed values.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when the attribute is
/// absent or its value is outside the approved set.
fn validate_closed_span_attribute(
    span: &CapturedSpan,
    key: &str,
    allowed: &[&str],
) -> Result<(), BifrostTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .is_some_and(|value| allowed.contains(&value.as_str()));
    if !valid {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!("attribute {key} is absent or outside its closed values"),
        });
    }
    Ok(())
}

/// Validate one permitted high-cardinality field as a scrubbed UUID.
///
/// # Errors
///
/// Returns [`BifrostTelemetryReportError::InvalidSpan`] when the field is absent
/// or is not a canonical UUID value.
fn validate_span_uuid(span: &CapturedSpan, key: &str) -> Result<(), BifrostTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .is_some();
    if !valid {
        return Err(BifrostTelemetryReportError::InvalidSpan {
            span: span.name.clone(),
            detail: format!("attribute {key} is not a scrubbed UUID"),
        });
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
                "bifrost_query_duration_seconds",
                "result",
                "success",
                0.99,
            )? * 1_000_000.0,
        })
    }
}

/// Reject unexpected or open-cardinality labels on the closed Forge report families.
fn validate_forge_label_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for sample in delta
        .metrics
        .iter()
        .chain(&delta.gauge_maxima)
        .chain(&delta.gauge_final)
    {
        let (allowed, categorical): (&[&str], &[(&str, &[&str])]) = match sample.family.as_str() {
            "bifrost_forge_rewrite_output_bytes_total" => {
                (&["source"], &[("source", &["staging", "iceberg"])])
            }
            "bifrost_forge_task_duration_seconds" => (
                &["strategy", "result", "le"],
                &[
                    (
                        "strategy",
                        &["staging_fold", "small_files", "snapshot_expiry"],
                    ),
                    (
                        "result",
                        &[
                            "succeeded",
                            "retryable",
                            "failed",
                            "cancelled",
                            "unschedulable",
                        ],
                    ),
                ],
            ),
            "bifrost_forge_oldest_backlog_seconds"
            | "bifrost_forge_fairness_lag_tasks"
            | "bifrost_forge_complete_gauge_publications_total" => (&[], &[]),
            "bifrost_memory_reserved_bytes" => (&["consumer"], &[]),
            "bifrost_memory_reservations_total" => (
                &["consumer", "outcome"],
                &[("outcome", &["accepted", "rejected"])],
            ),
            "bifrost_forge_task_spill_bytes" => (
                &["strategy", "le"],
                &[(
                    "strategy",
                    &["staging_fold", "small_files", "snapshot_expiry"],
                )],
            ),
            "bifrost_forge_conflicts_total" => (
                &["kind"],
                &[(
                    "kind",
                    &["lease_contention", "fence_lost", "snapshot_changed"],
                )],
            ),
            "bifrost_forge_cleanup_duration_seconds" => (
                &["kind", "le"],
                &[("kind", &["expired", "orphan", "spill"])],
            ),
            "bifrost_forge_role_processes" | "bifrost_forge_role_process_started_total" => {
                (&["role"], &[("role", &["all", "server", "forge_worker"])])
            }
            _ => continue,
        };
        validate_sample_labels(sample, allowed, categorical)?;
    }
    Ok(())
}

/// Reject unexpected labels or categorical values on the server query histogram.
fn validate_query_label_contract(
    delta: &BifrostTelemetryDelta,
) -> Result<(), BifrostTelemetryReportError> {
    for sample in delta
        .metrics
        .iter()
        .filter(|sample| sample.family == "bifrost_query_duration_seconds")
    {
        validate_sample_labels(
            sample,
            &["result", "le"],
            &[("result", &["success", "rejected", "failed"])],
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
    let normalized = name
        .strip_suffix("_bucket")
        .or_else(|| name.strip_suffix("_count"))
        .or_else(|| name.strip_suffix("_sum"));
    !matches!(
        types
            .get(name)
            .or_else(|| normalized.and_then(|family| types.get(family))),
        Some(PrometheusFamilyType::Summary)
    )
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
    let histogram_family = name
        .strip_suffix("_bucket")
        .or_else(|| name.strip_suffix("_sum"))
        .or_else(|| name.strip_suffix("_count"))
        .filter(|family| types.get(*family) == Some(&PrometheusFamilyType::Histogram));
    let family = histogram_family.unwrap_or(name).to_owned();
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
    let kind = metric_kind(name, &family, types)?;
    Ok(BifrostMetricSample {
        family,
        labels: parsed,
        value,
        kind,
    })
}

/// Classify a rendered Prometheus series before normalizing histogram suffixes.
#[must_use]
fn metric_kind(
    name: &str,
    family: &str,
    types: &BTreeMap<String, PrometheusFamilyType>,
) -> Result<BifrostMetricKind, BifrostTelemetryReportError> {
    match types.get(family) {
        Some(PrometheusFamilyType::Histogram) if name.ends_with("_bucket") => {
            Ok(BifrostMetricKind::HistogramBucket)
        }
        Some(PrometheusFamilyType::Histogram) if name.ends_with("_count") => {
            Ok(BifrostMetricKind::HistogramCount)
        }
        Some(PrometheusFamilyType::Histogram) if name.ends_with("_sum") => {
            Ok(BifrostMetricKind::HistogramSum)
        }
        Some(PrometheusFamilyType::Counter) if name == family => Ok(BifrostMetricKind::Counter),
        Some(PrometheusFamilyType::Gauge) if name == family => Ok(BifrostMetricKind::Gauge),
        Some(_) => Err(BifrostTelemetryReportError::Parse {
            detail: format!("sample kind conflicts with TYPE for {family}"),
        }),
        None => Err(BifrostTelemetryReportError::Parse {
            detail: format!("missing TYPE for {family}"),
        }),
    }
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

/// Sum one changed production family restricted by exact labels.
fn sum(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, BifrostTelemetryReportError> {
    let samples = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Err(BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        });
    }
    Ok(samples.into_iter().map(|sample| sample.value).sum())
}

/// Return an observed production gauge maximum restricted by exact labels.
fn gauge(
    delta: &BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, BifrostTelemetryReportError> {
    delta
        .gauge_maxima
        .iter()
        .filter(|sample| {
            sample.family == family
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .map(|sample| sample.value)
        .reduce(f64::max)
        .ok_or_else(|| BifrostTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        })
}

/// Require that one activity counter advanced inside this telemetry window.
fn require_advanced(
    delta: &BifrostTelemetryDelta,
    family: &str,
) -> Result<(), BifrostTelemetryReportError> {
    let advanced = sum(delta, family, &[])?;
    if advanced > 0.0 {
        Ok(())
    } else {
        Err(BifrostTelemetryReportError::StaleSeries {
            family: family.to_owned(),
        })
    }
}

/// Estimate a histogram quantile from changed Prometheus cumulative buckets.
pub(crate) fn histogram_quantile(
    delta: &BifrostTelemetryDelta,
    family: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    histogram_quantile_for_label(delta, family, "", "", quantile)
}

/// Convert a finite nonnegative duration in seconds to rounded microseconds.
///
/// # Errors
/// Returns invalid binding when multiplication or rounding cannot fit `u64`.
pub(crate) fn seconds_to_micros(
    binding_id: &str,
    seconds: f64,
) -> Result<u64, BifrostTelemetryReportError> {
    let micros = seconds * 1_000_000.0;
    if !micros.is_finite() || micros < 0.0 || micros.round() > u64::MAX as f64 {
        return Err(BifrostTelemetryReportError::InvalidBinding {
            id: binding_id.to_owned(),
            detail: "duration cannot be represented in microseconds".to_owned(),
        });
    }
    Ok(micros.round() as u64)
}

/// Estimate a histogram quantile after selecting one exact closed label.
fn histogram_quantile_for_label(
    delta: &BifrostTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<f64, BifrostTelemetryReportError> {
    let mut buckets = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && sample.kind == BifrostMetricKind::HistogramBucket
                && (key.is_empty() || sample.labels.get(key).map(String::as_str) == Some(value))
        })
        .filter_map(|sample| {
            sample
                .labels
                .get("le")
                .and_then(|upper| match upper.as_str() {
                    "+Inf" | "Inf" => Some(f64::INFINITY),
                    _ => upper.parse::<f64>().ok(),
                })
                .map(|upper| (upper, sample.value))
        })
        .collect::<Vec<_>>();
    buckets.sort_by(|left, right| left.0.total_cmp(&right.0));
    let count = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && sample.kind == BifrostMetricKind::HistogramCount
                && (key.is_empty() || sample.labels.get(key).map(String::as_str) == Some(value))
        })
        .map(|sample| sample.value)
        .sum::<f64>();
    if !count.is_finite() || count <= 0.0 {
        return Err(BifrostTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        });
    }
    let mut previous = 0.0;
    for (_, cumulative) in &buckets {
        if !cumulative.is_finite() || *cumulative < previous || *cumulative < 0.0 {
            return Err(BifrostTelemetryReportError::InvalidBinding {
                id: family.to_owned(),
                detail: "histogram buckets are nonmonotonic".to_owned(),
            });
        }
        previous = *cumulative;
    }
    let infinity_count = buckets.last().map(|(_, count)| *count).ok_or_else(|| {
        BifrostTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        }
    })?;
    if infinity_count != count {
        return Err(BifrostTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        });
    }
    let target = (count * quantile).ceil();
    for (upper, count) in buckets {
        if count >= target {
            if !upper.is_finite() {
                return Err(BifrostTelemetryReportError::EmptyHistogram {
                    family: family.to_owned(),
                });
            }
            return Ok(upper);
        }
    }
    Err(BifrostTelemetryReportError::EmptyHistogram {
        family: family.to_owned(),
    })
}

/// Validate active-role gauges and restart activity against launched topology.
fn validate_role_topology(
    delta: &BifrostTelemetryDelta,
    expected: &BTreeMap<String, u64>,
) -> Result<BTreeMap<String, ForgeRoleTopologyReport>, BifrostTelemetryReportError> {
    let active_roles = delta
        .gauge_maxima
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_role_processes" && sample.value > 0.0)
        .filter_map(|sample| sample.labels.get("role").cloned())
        .collect::<BTreeSet<_>>();
    let final_roles = delta
        .gauge_final
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_role_processes" && sample.value > 0.0)
        .filter_map(|sample| sample.labels.get("role").cloned())
        .collect::<BTreeSet<_>>();
    let expected_roles = expected.keys().cloned().collect::<BTreeSet<_>>();
    if active_roles != expected_roles || final_roles != expected_roles {
        return Err(BifrostTelemetryReportError::TopologyMismatch);
    }
    let mut topology = BTreeMap::new();
    for (role, count) in expected {
        let active = gauge(delta, "bifrost_forge_role_processes", &[("role", role)])?;
        let final_active = delta
            .gauge_final
            .iter()
            .find(|sample| {
                sample.family == "bifrost_forge_role_processes"
                    && sample.labels.get("role").map(String::as_str) == Some(role)
            })
            .map(|sample| sample.value)
            .ok_or(BifrostTelemetryReportError::TopologyMismatch)?;
        let starts = sum(
            delta,
            "bifrost_forge_role_process_started_total",
            &[("role", role)],
        )?;
        if active != *count as f64 || final_active != *count as f64 || starts < *count as f64 {
            return Err(BifrostTelemetryReportError::TopologyMismatch);
        }
        let max_active = active as u64;
        let final_active = final_active as u64;
        let starts = starts as u64;
        topology.insert(
            role.clone(),
            ForgeRoleTopologyReport {
                max_active,
                final_active,
                starts,
            },
        );
    }
    Ok(topology)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct one normalized production sample for mapper contract tests.
    fn sample(family: &str, labels: &[(&str, &str)], value: f64) -> BifrostMetricSample {
        BifrostMetricSample {
            family: family.to_owned(),
            labels: labels
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            value,
            kind: if labels.iter().any(|(key, _)| *key == "le") {
                BifrostMetricKind::HistogramBucket
            } else if family.ends_with("_total") && family != "bifrost_oracle_slots_total" {
                BifrostMetricKind::Counter
            } else {
                BifrostMetricKind::Gauge
            },
        }
    }

    /// Construct one normalized histogram count sample for mapper tests.
    fn histogram_count(family: &str, labels: &[(&str, &str)], value: f64) -> BifrostMetricSample {
        let mut sample = sample(family, labels, value);
        sample.kind = BifrostMetricKind::HistogramCount;
        sample
    }

    /// Construct one normalized captured production span for mapper tests.
    fn captured_span(name: &str, attributes: &[(&str, &str)]) -> CapturedSpan {
        CapturedSpan {
            trace_id: "00000000000000000000000000000001".to_owned(),
            name: name.to_owned(),
            attributes: attributes
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            duration_nanos: 1,
            status: wyrd_telemetry::CapturedSpanStatus::Unset,
        }
    }

    /// Slot capacity is an up/down gauge even though its legacy family ends in `_total`.
    #[test]
    fn oracle_slot_capacity_is_not_monotonic() {
        let types = BTreeMap::from([(
            "bifrost_oracle_slots_total".to_owned(),
            PrometheusFamilyType::Gauge,
        )]);
        assert_eq!(
            parse_sample("bifrost_oracle_slots_total{role=\"leader\"}", 1.0, &types,)
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

    /// Proves checked window buckets, rather than sum, define p99 microseconds.
    #[test]
    fn histogram_window_p99_uses_bucket_delta_and_checked_microseconds() {
        let family = "vala_postgres_pool_acquire_seconds";
        let mut delta = canonical_binding_delta();
        delta.metrics.retain(|sample| sample.family != family);
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
        let p99 = histogram_quantile(&delta, family, 0.99).unwrap();
        assert_eq!(seconds_to_micros("postgres.acquire", p99).unwrap(), 25_000);
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

    /// Proves every required closed binding fails independently when removed.
    #[test]
    fn canonical_projection_rejects_each_missing_binding() {
        let complete = canonical_binding_delta();
        validate_cluster_bindings(&complete).expect("closed binding fixture is complete");
        for binding in CLUSTER_BINDINGS {
            let mut removed = complete.clone();
            removed
                .metrics
                .retain(|sample| sample.family != binding.family);
            removed
                .gauge_maxima
                .retain(|sample| sample.family != binding.family);
            removed
                .gauge_final
                .retain(|sample| sample.family != binding.family);
            assert!(matches!(
                validate_cluster_bindings(&removed),
                Err(BifrostTelemetryReportError::InvalidBinding { ref id, .. })
                    if id == binding.id.0
            ));
        }
    }

    /// Proves wrong kinds, open labels, and error spans invalidate clean evidence.
    #[test]
    fn canonical_projection_rejects_malformed_and_error_evidence() {
        let mut wrong_kind = canonical_binding_delta();
        wrong_kind.metrics[0].kind = BifrostMetricKind::Gauge;
        assert!(validate_cluster_bindings(&wrong_kind).is_err());

        let mut open_label = canonical_binding_delta();
        open_label.metrics[0]
            .labels
            .insert("tenant".to_owned(), "forbidden".to_owned());
        assert!(validate_cluster_bindings(&open_label).is_err());

        let mut error_span = canonical_binding_delta();
        let mut span = captured_span("bifrost.oracle.query", &[]);
        span.status = wyrd_telemetry::CapturedSpanStatus::Error("failed".to_owned());
        error_span.spans.push(span);
        assert!(validate_cluster_bindings(&error_span).is_err());
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

    /// Build one complete exact binding fixture from the closed compile-time ledger.
    fn canonical_binding_delta() -> BifrostTelemetryDelta {
        let mut delta = BifrostTelemetryDelta {
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: test_process_window(),
        };
        for binding in CLUSTER_BINDINGS {
            let mut labels = binding
                .allowed_label_values
                .iter()
                .map(|domain| (domain.key.to_owned(), domain.values[0].to_owned()))
                .collect::<BTreeMap<_, _>>();
            if binding.kind == BifrostMetricKind::HistogramBucket {
                labels.insert("le".to_owned(), "+Inf".to_owned());
            }
            if binding.family == "bifrost_oracle_slots_total" {
                labels.insert("role".to_owned(), "leader".to_owned());
            }
            let sample = BifrostMetricSample {
                family: binding.family.to_owned(),
                labels,
                value: 1.0,
                kind: binding.kind,
            };
            match binding.aggregation {
                TelemetryAggregation::Delta | TelemetryAggregation::P99 => {
                    delta.metrics.push(sample);
                }
                TelemetryAggregation::Peak => delta.gauge_maxima.push(sample),
                TelemetryAggregation::Final => delta.gauge_final.push(sample),
            }
        }
        delta
    }

    /// Construct one complete three-worker production delta without a fixture value path.
    fn complete_delta() -> BifrostTelemetryDelta {
        BifrostTelemetryDelta {
            metrics: vec![
                sample(
                    "bifrost_forge_rewrite_output_bytes_total",
                    &[("source", "staging")],
                    1_048_576.0,
                ),
                sample("bifrost_forge_complete_gauge_publications_total", &[], 1.0),
                sample(
                    "bifrost_memory_reservations_total",
                    &[("consumer", "forge"), ("outcome", "accepted")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_duration_seconds",
                    &[
                        ("strategy", "staging_fold"),
                        ("result", "succeeded"),
                        ("le", "0.1"),
                    ],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_task_duration_seconds",
                    &[("strategy", "staging_fold"), ("result", "succeeded")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_duration_seconds",
                    &[
                        ("strategy", "staging_fold"),
                        ("result", "succeeded"),
                        ("le", "+Inf"),
                    ],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold"), ("le", "65536")],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_task_spill_bytes",
                    &[("strategy", "staging_fold"), ("le", "+Inf")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired"), ("le", "0.025")],
                    1.0,
                ),
                histogram_count(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_cleanup_duration_seconds",
                    &[("kind", "expired"), ("le", "+Inf")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "lease_contention")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "fence_lost")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_conflicts_total",
                    &[("kind", "snapshot_changed")],
                    0.0,
                ),
                sample(
                    "bifrost_forge_role_process_started_total",
                    &[("role", "server")],
                    1.0,
                ),
                sample(
                    "bifrost_forge_role_process_started_total",
                    &[("role", "forge_worker")],
                    4.0,
                ),
            ],
            gauge_maxima: vec![
                sample("bifrost_forge_oldest_backlog_seconds", &[], 0.01),
                sample(
                    "bifrost_memory_reserved_bytes",
                    &[("consumer", "forge")],
                    1024.0,
                ),
                sample("bifrost_forge_fairness_lag_tasks", &[], 1.0),
                sample("bifrost_forge_role_processes", &[("role", "server")], 1.0),
                sample(
                    "bifrost_forge_role_processes",
                    &[("role", "forge_worker")],
                    3.0,
                ),
            ],
            gauge_final: vec![
                sample("bifrost_forge_oldest_backlog_seconds", &[], 0.0),
                sample(
                    "bifrost_memory_reserved_bytes",
                    &[("consumer", "forge")],
                    0.0,
                ),
                sample("bifrost_forge_fairness_lag_tasks", &[], 0.0),
                sample("bifrost_forge_role_processes", &[("role", "server")], 1.0),
                sample(
                    "bifrost_forge_role_processes",
                    &[("role", "forge_worker")],
                    3.0,
                ),
            ],
            spans: vec![
                captured_span(
                    "bifrost.forge.scheduler.pass",
                    &[("result", "succeeded"), ("role", "server")],
                ),
                captured_span(
                    "bifrost.forge.task.execute",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b02"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "staging_fold"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"),
                    ],
                ),
                captured_span(
                    "bifrost.forge.catalog.commit",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b04"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "small_files"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b03"),
                    ],
                ),
                captured_span(
                    "bifrost.forge.cleanup",
                    &[
                        ("attempt_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b06"),
                        ("kind", "expired"),
                        ("result", "succeeded"),
                        ("role", "forge_worker"),
                        ("strategy", "snapshot_expiry"),
                        ("task_id", "01890f28-7c4a-7000-98e7-4f4a3c2d1b05"),
                    ],
                ),
            ],
            interval_seconds: 2.0,
            process: test_process_window(),
        }
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
        let series = "bifrost_memory_reserved_bytes{consumer=\"forge\"}";
        let mut maxima = BTreeMap::new();
        let types = BTreeMap::from([
            (
                "bifrost_memory_reserved_bytes".to_owned(),
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

    /// Prove every report projection responds only to its production delta field.
    #[test]
    fn forge_telemetry_report_uses_production_delta() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let baseline =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("complete production delta maps");
        macro_rules! assert_projection {
            ($delta:expr, $field:ident) => {{
                let changed =
                    ForgeMaintenanceTelemetryReport::from_production_delta(&$delta, &expected)
                        .expect("changed production delta maps");
                assert_ne!(changed.$field, baseline.$field);
                let mut normalized = changed;
                normalized.$field = baseline.$field.clone();
                assert_eq!(normalized, baseline);
            }};
        }

        let mut delta = complete_delta();
        delta.metrics[0].value *= 2.0;
        assert_projection!(delta, throughput_mib_per_sec);

        let mut delta = complete_delta();
        delta.metrics[3]
            .labels
            .insert("le".to_owned(), "0.2".to_owned());
        assert_projection!(delta, task_latency_p99_us);

        let mut delta = complete_delta();
        delta.gauge_maxima[0].value = 0.02;
        assert_projection!(delta, backlog_age_us);

        let mut delta = complete_delta();
        delta.gauge_maxima[1].value = 2048.0;
        assert_projection!(delta, peak_parent_memory);

        let mut delta = complete_delta();
        delta.metrics[6]
            .labels
            .insert("le".to_owned(), "131072".to_owned());
        assert_projection!(delta, spill_bytes);

        for (index, field) in [
            (12, "lease_contention"),
            (13, "fence_lost"),
            (14, "snapshot_changed"),
        ] {
            let mut delta = complete_delta();
            delta.metrics[index].value = 1.0;
            let changed = ForgeMaintenanceTelemetryReport::from_production_delta(&delta, &expected)
                .expect("changed production conflict maps");
            let mut normalized = changed.clone();
            match field {
                "lease_contention" => normalized.lease_contention = baseline.lease_contention,
                "fence_lost" => normalized.fence_lost = baseline.fence_lost,
                "snapshot_changed" => normalized.snapshot_changed = baseline.snapshot_changed,
                _ => unreachable!("closed conflict projection"),
            }
            assert_ne!(changed, baseline);
            assert_eq!(normalized, baseline);
        }

        let mut delta = complete_delta();
        delta.gauge_maxima[2].value = 2.0;
        assert_projection!(delta, fairness_lag_tasks);

        let mut delta = complete_delta();
        delta.metrics[9]
            .labels
            .insert("le".to_owned(), "0.05".to_owned());
        assert_projection!(delta, cleanup_delay_us);

        let mut delta = complete_delta();
        delta.metrics[16].value = 5.0;
        assert_projection!(delta, role_topology);
    }

    /// Reject benchmark-local derivation paths outside the production capture mapper.
    #[test]
    fn forge_benchmark_has_no_parallel_metric_derivation() {
        let source = include_str!("bench_forge.rs");
        for prohibited in [
            "Instant::elapsed",
            "query_latency",
            "schedule_once",
            "execute_one_for_test",
        ] {
            assert!(!source.contains(prohibited));
        }
        assert!(source.contains("ForgeMaintenanceTelemetryReport::from_production_delta"));
    }

    /// Prove replacement starts remain distinct from maximum and final concurrency.
    #[test]
    fn forge_role_topology_survives_worker_replacement() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let report =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("production replacement topology maps");
        assert_eq!(report.role_topology["forge_worker"].starts, 4);
        assert_eq!(report.role_topology["forge_worker"].max_active, 3);
        assert_eq!(report.role_topology["forge_worker"].final_active, 3);
    }

    /// Prove throughput uses captured output bytes divided by the capture interval.
    #[test]
    fn forge_throughput_uses_production_counter_rate() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let report =
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &expected)
                .expect("production counter-rate delta maps");
        assert_eq!(report.throughput_mib_per_sec, 0.5);
    }

    /// Reject missing, stale, empty, wrong-topology, and open-label report windows.
    #[test]
    fn forge_telemetry_report_rejects_incomplete_windows() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let mut missing = complete_delta();
        missing.metrics.remove(0);
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing, &expected),
            Err(BifrostTelemetryReportError::MissingSeries { .. })
        ));
        let mut stale = complete_delta();
        stale.metrics[1].value = 0.0;
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&stale, &expected),
            Err(BifrostTelemetryReportError::StaleSeries { .. })
        ));
        let mut stale_memory = complete_delta();
        stale_memory
            .metrics
            .iter_mut()
            .find(|sample| sample.family == "bifrost_memory_reservations_total")
            .expect("memory reservation activity series")
            .value = 0.0;
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&stale_memory, &expected),
            Err(BifrostTelemetryReportError::StaleSeries { family })
                if family == "bifrost_memory_reservations_total"
        ));
        let mut empty = complete_delta();
        empty
            .metrics
            .iter_mut()
            .filter(|sample| sample.family == "bifrost_forge_task_spill_bytes")
            .for_each(|sample| sample.value = 0.0);
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&empty, &expected),
            Err(BifrostTelemetryReportError::EmptyHistogram { .. })
        ));
        let wrong = BTreeMap::from([("all".to_owned(), 1)]);
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &wrong),
            Err(BifrostTelemetryReportError::TopologyMismatch)
        );
        let mut open_label = complete_delta();
        open_label.metrics[0]
            .labels
            .insert("tenant".to_owned(), "forbidden".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_label, &expected),
            Err(BifrostTelemetryReportError::Parse { .. })
        ));
        let mut missing_span = complete_delta();
        missing_span
            .spans
            .retain(|span| span.name != "bifrost.forge.cleanup");
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_span, &expected),
            Err(BifrostTelemetryReportError::MissingSpan {
                span: "bifrost.forge.cleanup".to_owned(),
            })
        );
        let mut renamed_span = complete_delta();
        renamed_span.spans[1].name = "bifrost.forge.task.renamed".to_owned();
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&renamed_span, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.task.renamed"
        ));
        let mut missing_attribute = complete_delta();
        missing_attribute.spans[2].attributes.remove("result");
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_attribute, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.catalog.commit"
        ));
        let mut prohibited_attribute = complete_delta();
        prohibited_attribute.spans[3]
            .attributes
            .insert("tenant_id".to_owned(), "forbidden".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(
                &prohibited_attribute,
                &expected,
            ),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.cleanup"
        ));
        let mut open_attribute = complete_delta();
        open_attribute.spans[0]
            .attributes
            .insert("result".to_owned(), "unknown".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_attribute, &expected),
            Err(BifrostTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.scheduler.pass"
        ));
    }
}
