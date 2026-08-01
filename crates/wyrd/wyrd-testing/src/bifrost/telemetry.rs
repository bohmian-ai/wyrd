//! Read-only parsing and validation of production Forge telemetry windows.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use metrics_exporter_prometheus::PrometheusHandle;
use wyrd_telemetry::{CapturedSpan, TestTraceCapture};

/// One parsed production Prometheus sample.
#[derive(Debug, Clone, PartialEq)]
pub struct ForgeMetricSample {
    /// Normalized production metric family.
    pub family: String,
    /// Exact fixed-cardinality labels emitted with the sample.
    pub labels: BTreeMap<String, String>,
    /// Absolute or window-delta value, depending on the enclosing artifact.
    pub value: f64,
}

/// One single-use marker for a production telemetry observation window.
#[derive(Debug, Clone)]
pub struct ForgeTelemetryCheckpoint {
    /// Unique process-local token rejected after one delta construction.
    id: u64,
    /// Absolute sample values at the start of the window.
    metrics: BTreeMap<String, f64>,
    /// Exporter-render timestamp used for Prometheus-style rates.
    timestamp_ns: u128,
    /// Production trace position before the window begins.
    spans: usize,
}

/// Captured production telemetry emitted during one observation window.
#[derive(Debug, Clone)]
pub struct ForgeTelemetryDelta {
    /// Counter and histogram deltas from the one production render handle.
    pub metrics: Vec<ForgeMetricSample>,
    /// Gauge maxima observed by the production capture while the window ran.
    pub gauge_maxima: Vec<ForgeMetricSample>,
    /// Final gauge values from the render that closed the capture window.
    pub gauge_final: Vec<ForgeMetricSample>,
    /// Finished production-provider spans after the checkpoint.
    pub spans: Vec<CapturedSpan>,
    /// Positive render-to-render duration used by counter-rate calculations.
    pub interval_seconds: f64,
}

/// Bounded background sampler for production gauges during one capture window.
pub struct ForgeGaugeSampler {
    /// Cancellation signal for the production-render polling task.
    stop: tokio_util::sync::CancellationToken,
    /// Gauge snapshots collected only from the production render handle.
    snapshots: Arc<Mutex<Vec<BTreeMap<String, f64>>>>,
    /// Task that owns bounded polling until delta construction drains it.
    task: tokio::task::JoinHandle<()>,
}

/// Failure raised when a production telemetry window cannot prove one report field.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ForgeTelemetryReportError {
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
pub struct ForgeTelemetryCapture {
    /// Production recorder render handle, never passed into Forge.
    metrics: PrometheusHandle,
    /// Same-provider trace capture installed before role composition.
    traces: TestTraceCapture,
    /// Tokens already consumed by `delta_since`.
    consumed: Arc<Mutex<BTreeSet<u64>>>,
    /// Monotonic checkpoint identity that cannot collide in this capture.
    next_checkpoint: Arc<AtomicU64>,
}

impl ForgeTelemetryCapture {
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
    /// Returns [`ForgeTelemetryReportError::Parse`] when rendered Prometheus
    /// output cannot be parsed without losing a label or numeric value.
    pub fn checkpoint(&self) -> Result<ForgeTelemetryCheckpoint, ForgeTelemetryReportError> {
        Ok(ForgeTelemetryCheckpoint {
            id: self.next_checkpoint.fetch_add(1, Ordering::AcqRel),
            metrics: rendered_values(&self.metrics.render())?,
            timestamp_ns: render_timestamp_ns()?,
            spans: self.traces.checkpoint(),
        })
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
        checkpoint: ForgeTelemetryCheckpoint,
    ) -> Result<ForgeTelemetryDelta, ForgeTelemetryReportError> {
        let mut consumed = self
            .consumed
            .lock()
            .map_err(|_| ForgeTelemetryReportError::ReusedWindow)?;
        if !consumed.insert(checkpoint.id) {
            return Err(ForgeTelemetryReportError::ReusedWindow);
        }
        drop(consumed);
        let timestamp_ns = render_timestamp_ns()?;
        let elapsed_ns = timestamp_ns
            .checked_sub(checkpoint.timestamp_ns)
            .ok_or(ForgeTelemetryReportError::InvalidInterval)?;
        if elapsed_ns == 0 {
            return Err(ForgeTelemetryReportError::InvalidInterval);
        }
        let current = rendered_values(&self.metrics.render())?;
        let mut metrics = Vec::new();
        let mut gauge_maxima = Vec::new();
        let mut gauge_final = Vec::new();
        for (series, value) in &current {
            let prior = checkpoint.metrics.get(series).copied().unwrap_or(0.0);
            if is_monotonic_series(series) {
                if *value < prior {
                    return Err(ForgeTelemetryReportError::CounterRegression {
                        series: series.clone(),
                    });
                }
                let mut sample = parse_sample(series, *value)?;
                sample.value = *value - prior;
                metrics.push(sample);
            } else {
                let baseline = parse_sample(series, prior)?;
                let end = parse_sample(series, *value)?;
                gauge_maxima.push(ForgeMetricSample {
                    family: end.family.clone(),
                    labels: end.labels.clone(),
                    value: baseline.value.max(end.value),
                });
                gauge_final.push(end);
            }
        }
        Ok(ForgeTelemetryDelta {
            metrics,
            gauge_maxima,
            gauge_final,
            spans: self.traces.finished_since(checkpoint.spans),
            interval_seconds: elapsed_ns as f64 / 1_000_000_000.0,
        })
    }

    /// Start bounded fixed-interval production-gauge sampling for one window.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeTelemetryReportError::Parse`] when the initial render is
    /// malformed. The sampler never receives fixture, SQL, or scenario values.
    pub async fn begin_gauge_sampling(
        &self,
        _checkpoint: &ForgeTelemetryCheckpoint,
    ) -> Result<ForgeGaugeSampler, ForgeTelemetryReportError> {
        let initial = rendered_values(&self.metrics.render())?;
        let snapshots = Arc::new(Mutex::new(vec![initial]));
        let stop = tokio_util::sync::CancellationToken::new();
        let sampler_stop = stop.clone();
        let sampler_snapshots = Arc::clone(&snapshots);
        let metrics = self.metrics.clone();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(25));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    () = sampler_stop.cancelled() => return,
                    _ = interval.tick() => if let Ok(snapshot) = rendered_values(&metrics.render())
                        && let Ok(mut values) = sampler_snapshots.lock() {
                        values.push(snapshot);
                    },
                }
            }
        });
        Ok(ForgeGaugeSampler {
            stop,
            snapshots,
            task,
        })
    }

    /// Build one checked production delta after stopping its gauge sampler.
    ///
    /// # Errors
    ///
    /// Returns the same errors as checkpoint/delta construction, including
    /// single-use window and counter-regression failures.
    pub async fn delta_since(
        &self,
        checkpoint: ForgeTelemetryCheckpoint,
        sampler: ForgeGaugeSampler,
    ) -> Result<ForgeTelemetryDelta, ForgeTelemetryReportError> {
        sampler.stop.cancel();
        let _ = sampler.task.await;
        let mut delta = self.delta_without_sampler(checkpoint)?;
        let snapshots = sampler
            .snapshots
            .lock()
            .map_err(|_| ForgeTelemetryReportError::ReusedWindow)?;
        let mut maxima = BTreeMap::<String, f64>::new();
        for snapshot in snapshots.iter() {
            for (series, value) in snapshot {
                if !is_monotonic_series(series) {
                    maxima
                        .entry(series.clone())
                        .and_modify(|maximum| *maximum = maximum.max(*value))
                        .or_insert(*value);
                }
            }
        }
        for sample in &delta.gauge_maxima {
            let series = rendered_series(sample);
            maxima
                .entry(series)
                .and_modify(|maximum| *maximum = maximum.max(sample.value))
                .or_insert(sample.value);
        }
        delta.gauge_maxima = maxima
            .into_iter()
            .map(|(series, value)| parse_sample(&series, value))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(delta)
    }
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
        delta: &ForgeTelemetryDelta,
        expected_roles: &BTreeMap<String, u64>,
    ) -> Result<Self, ForgeTelemetryReportError> {
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
/// Returns [`ForgeTelemetryReportError::MissingSpan`] when a required owner did
/// not emit, or [`ForgeTelemetryReportError::InvalidSpan`] for renamed spans,
/// missing/extra attributes, open values, or malformed scrubbed UUIDs.
fn validate_forge_span_contract(
    delta: &ForgeTelemetryDelta,
) -> Result<(), ForgeTelemetryReportError> {
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
                return Err(ForgeTelemetryReportError::InvalidSpan {
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
            return Err(ForgeTelemetryReportError::MissingSpan {
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
/// Returns [`ForgeTelemetryReportError::InvalidSpan`] when a required field is
/// absent or any prohibited, sensitive, or otherwise unapproved field appears.
fn validate_span_attribute_set(
    span: &CapturedSpan,
    expected: &[&str],
) -> Result<(), ForgeTelemetryReportError> {
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
        return Err(ForgeTelemetryReportError::InvalidSpan {
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
/// Returns [`ForgeTelemetryReportError::InvalidSpan`] when the attribute is
/// absent or its value is outside the approved set.
fn validate_closed_span_attribute(
    span: &CapturedSpan,
    key: &str,
    allowed: &[&str],
) -> Result<(), ForgeTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .is_some_and(|value| allowed.contains(&value.as_str()));
    if !valid {
        return Err(ForgeTelemetryReportError::InvalidSpan {
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
/// Returns [`ForgeTelemetryReportError::InvalidSpan`] when the field is absent
/// or is not a canonical UUID value.
fn validate_span_uuid(span: &CapturedSpan, key: &str) -> Result<(), ForgeTelemetryReportError> {
    let valid = span
        .attributes
        .get(key)
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .is_some();
    if !valid {
        return Err(ForgeTelemetryReportError::InvalidSpan {
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
    /// Returns [`ForgeTelemetryReportError::EmptyHistogram`] when no successful
    /// query completed in the capture window.
    pub fn from_server_delta(
        delta: &ForgeTelemetryDelta,
    ) -> Result<Self, ForgeTelemetryReportError> {
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
    delta: &ForgeTelemetryDelta,
) -> Result<(), ForgeTelemetryReportError> {
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
    delta: &ForgeTelemetryDelta,
) -> Result<(), ForgeTelemetryReportError> {
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
    sample: &ForgeMetricSample,
    allowed: &[&str],
    categorical: &[(&str, &[&str])],
) -> Result<(), ForgeTelemetryReportError> {
    if sample
        .labels
        .keys()
        .any(|label| !allowed.contains(&label.as_str()))
    {
        return Err(ForgeTelemetryReportError::Parse {
            detail: format!("unexpected label on {}", sample.family),
        });
    }
    for (key, values) in categorical {
        if let Some(value) = sample.labels.get(*key)
            && !values.contains(&value.as_str())
        {
            return Err(ForgeTelemetryReportError::Parse {
                detail: format!("unexpected {key} value on {}", sample.family),
            });
        }
    }
    Ok(())
}

/// Return a monotonic timestamp for exporter render snapshots.
fn render_timestamp_ns() -> Result<u128, ForgeTelemetryReportError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|_| ForgeTelemetryReportError::InvalidInterval)
}

/// Parse every metric value from a production render.
fn rendered_values(rendered: &str) -> Result<BTreeMap<String, f64>, ForgeTelemetryReportError> {
    let mut values = BTreeMap::new();
    for line in rendered
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (series, value) =
            line.rsplit_once(' ')
                .ok_or_else(|| ForgeTelemetryReportError::Parse {
                    detail: "metric line has no value".to_owned(),
                })?;
        let value = value
            .parse::<f64>()
            .map_err(|_| ForgeTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            })?;
        if !value.is_finite() {
            return Err(ForgeTelemetryReportError::Parse {
                detail: "metric value is not finite".to_owned(),
            });
        }
        values.insert(series.to_owned(), value);
    }
    Ok(values)
}

/// Parse one constrained Prometheus series into its family and labels.
fn parse_sample(series: &str, value: f64) -> Result<ForgeMetricSample, ForgeTelemetryReportError> {
    let (name, labels) = if let Some((name, labels)) = series.split_once('{') {
        (
            name,
            Some(
                labels
                    .strip_suffix('}')
                    .ok_or_else(|| ForgeTelemetryReportError::Parse {
                        detail: "metric labels are not closed".to_owned(),
                    })?,
            ),
        )
    } else {
        (series, None)
    };
    let family = name
        .strip_suffix("_bucket")
        .or_else(|| name.strip_suffix("_sum"))
        .or_else(|| name.strip_suffix("_count"))
        .unwrap_or(name)
        .to_owned();
    let mut parsed = BTreeMap::new();
    if let Some(labels) = labels {
        for label in labels.split(',').filter(|label| !label.is_empty()) {
            let (key, quoted) =
                label
                    .split_once('=')
                    .ok_or_else(|| ForgeTelemetryReportError::Parse {
                        detail: "metric label is malformed".to_owned(),
                    })?;
            let value = quoted
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| (!quoted.contains('"')).then_some(quoted))
                .ok_or_else(|| ForgeTelemetryReportError::Parse {
                    detail: format!("metric label {key} is not quoted: {quoted}"),
                })?;
            if parsed.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(ForgeTelemetryReportError::Parse {
                    detail: "metric label is duplicated".to_owned(),
                });
            }
        }
    }
    Ok(ForgeMetricSample {
        family,
        labels: parsed,
        value,
    })
}

/// Rebuild a normalized rendered-series identity from one parsed production sample.
fn rendered_series(sample: &ForgeMetricSample) -> String {
    if sample.labels.is_empty() {
        return sample.family.clone();
    }
    let labels = sample
        .labels
        .iter()
        .map(|(key, value)| format!(r#"{key}="{value}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}{{{labels}}}", sample.family)
}

/// Identify rendered counters and histogram internals that must never regress.
fn is_monotonic_series(series: &str) -> bool {
    series
        .split_once('{')
        .map_or(series, |(name, _)| name)
        .ends_with("_total")
        || series
            .split_once('{')
            .map_or(series, |(name, _)| name)
            .ends_with("_bucket")
        || series
            .split_once('{')
            .map_or(series, |(name, _)| name)
            .ends_with("_count")
        || series
            .split_once('{')
            .map_or(series, |(name, _)| name)
            .ends_with("_sum")
}

/// Sum one changed production family restricted by exact labels.
fn sum(
    delta: &ForgeTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, ForgeTelemetryReportError> {
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
        return Err(ForgeTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        });
    }
    Ok(samples.into_iter().map(|sample| sample.value).sum())
}

/// Return an observed production gauge maximum restricted by exact labels.
fn gauge(
    delta: &ForgeTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> Result<f64, ForgeTelemetryReportError> {
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
        .ok_or_else(|| ForgeTelemetryReportError::MissingSeries {
            family: family.to_owned(),
        })
}

/// Require that one activity counter advanced inside this telemetry window.
fn require_advanced(
    delta: &ForgeTelemetryDelta,
    family: &str,
) -> Result<(), ForgeTelemetryReportError> {
    let advanced = sum(delta, family, &[])?;
    if advanced > 0.0 {
        Ok(())
    } else {
        Err(ForgeTelemetryReportError::StaleSeries {
            family: family.to_owned(),
        })
    }
}

/// Estimate a histogram quantile from changed Prometheus cumulative buckets.
fn histogram_quantile(
    delta: &ForgeTelemetryDelta,
    family: &str,
    quantile: f64,
) -> Result<f64, ForgeTelemetryReportError> {
    histogram_quantile_for_label(delta, family, "", "", quantile)
}

/// Estimate a histogram quantile after selecting one exact closed label.
fn histogram_quantile_for_label(
    delta: &ForgeTelemetryDelta,
    family: &str,
    key: &str,
    value: &str,
    quantile: f64,
) -> Result<f64, ForgeTelemetryReportError> {
    let mut observed_buckets = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
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
    observed_buckets.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut buckets = Vec::<(f64, f64)>::new();
    for (upper, count) in observed_buckets {
        if let Some((previous_upper, aggregate)) = buckets.last_mut()
            && previous_upper.total_cmp(&upper).is_eq()
        {
            *aggregate += count;
        } else {
            buckets.push((upper, count));
        }
    }
    let total = buckets.last().map(|(_, count)| *count).ok_or_else(|| {
        ForgeTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        }
    })?;
    if total <= 0.0 {
        return Err(ForgeTelemetryReportError::EmptyHistogram {
            family: family.to_owned(),
        });
    }
    let target = total * quantile;
    let mut prior_upper = 0.0;
    let mut prior_count = 0.0;
    for (upper, count) in buckets {
        if count >= target {
            if !upper.is_finite() {
                return Ok(prior_upper);
            }
            let span = (count - prior_count).max(1.0);
            return Ok(prior_upper + (upper - prior_upper) * ((target - prior_count) / span));
        }
        prior_upper = upper;
        prior_count = count;
    }
    Err(ForgeTelemetryReportError::EmptyHistogram {
        family: family.to_owned(),
    })
}

/// Validate active-role gauges and restart activity against launched topology.
fn validate_role_topology(
    delta: &ForgeTelemetryDelta,
    expected: &BTreeMap<String, u64>,
) -> Result<BTreeMap<String, ForgeRoleTopologyReport>, ForgeTelemetryReportError> {
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
        return Err(ForgeTelemetryReportError::TopologyMismatch);
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
            .ok_or(ForgeTelemetryReportError::TopologyMismatch)?;
        let starts = sum(
            delta,
            "bifrost_forge_role_process_started_total",
            &[("role", role)],
        )?;
        if active != *count as f64 || final_active != *count as f64 || starts < *count as f64 {
            return Err(ForgeTelemetryReportError::TopologyMismatch);
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
    fn sample(family: &str, labels: &[(&str, &str)], value: f64) -> ForgeMetricSample {
        ForgeMetricSample {
            family: family.to_owned(),
            labels: labels
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            value,
        }
    }

    /// Construct one normalized captured production span for mapper tests.
    fn captured_span(name: &str, attributes: &[(&str, &str)]) -> CapturedSpan {
        CapturedSpan {
            name: name.to_owned(),
            attributes: attributes
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            duration_nanos: 1,
        }
    }

    /// Construct one complete three-worker production delta without a fixture value path.
    fn complete_delta() -> ForgeTelemetryDelta {
        ForgeTelemetryDelta {
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
        }
    }

    /// Reject missing, stale, empty, wrong-topology, and open-label report windows.
    #[test]
    fn forge_telemetry_report_rejects_incomplete_windows() {
        let expected = BTreeMap::from([("server".to_owned(), 1), ("forge_worker".to_owned(), 3)]);
        let mut missing = complete_delta();
        missing.metrics.remove(0);
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing, &expected),
            Err(ForgeTelemetryReportError::MissingSeries { .. })
        ));
        let mut stale = complete_delta();
        stale.metrics[1].value = 0.0;
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&stale, &expected),
            Err(ForgeTelemetryReportError::StaleSeries { .. })
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
            Err(ForgeTelemetryReportError::StaleSeries { family })
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
            Err(ForgeTelemetryReportError::EmptyHistogram { .. })
        ));
        let wrong = BTreeMap::from([("all".to_owned(), 1)]);
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&complete_delta(), &wrong),
            Err(ForgeTelemetryReportError::TopologyMismatch)
        );
        let mut open_label = complete_delta();
        open_label.metrics[0]
            .labels
            .insert("tenant".to_owned(), "forbidden".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_label, &expected),
            Err(ForgeTelemetryReportError::Parse { .. })
        ));
        let mut missing_span = complete_delta();
        missing_span
            .spans
            .retain(|span| span.name != "bifrost.forge.cleanup");
        assert_eq!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_span, &expected),
            Err(ForgeTelemetryReportError::MissingSpan {
                span: "bifrost.forge.cleanup".to_owned(),
            })
        );
        let mut renamed_span = complete_delta();
        renamed_span.spans[1].name = "bifrost.forge.task.renamed".to_owned();
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&renamed_span, &expected),
            Err(ForgeTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.task.renamed"
        ));
        let mut missing_attribute = complete_delta();
        missing_attribute.spans[2].attributes.remove("result");
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&missing_attribute, &expected),
            Err(ForgeTelemetryReportError::InvalidSpan { span, .. })
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
            Err(ForgeTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.cleanup"
        ));
        let mut open_attribute = complete_delta();
        open_attribute.spans[0]
            .attributes
            .insert("result".to_owned(), "unknown".to_owned());
        assert!(matches!(
            ForgeMaintenanceTelemetryReport::from_production_delta(&open_attribute, &expected),
            Err(ForgeTelemetryReportError::InvalidSpan { span, .. })
                if span == "bifrost.forge.scheduler.pass"
        ));
    }
}
