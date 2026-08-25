//! Prometheus metrics recorder + scrape endpoint.
//!
//! The recorder is installed once at startup; `track_metrics`
//! (`http/middleware/metrics.rs`) records per-request counters/histograms into
//! it, and `/metrics` renders the current snapshot on a dedicated listener.

use std::future::ready;
use std::sync::Arc;
#[cfg(test)]
use std::sync::OnceLock;

use axum::{Router, routing::get};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wyrd_telemetry::{TelemetryConfig, TelemetryGuard};

use crate::config::BifrostTarget;

/// Histogram buckets (seconds) for request-duration metrics.
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Forge and Bifrost-query duration buckets shared by deployed processes and
/// the read-only benchmark capture.
const BIFROST_DURATION_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
    600.0, 1800.0,
];

/// Forge spill-size buckets shared by deployed processes and capture.
const FORGE_SPILL_BUCKETS: &[f64] = &[
    65_536.0,
    1_048_576.0,
    16_777_216.0,
    67_108_864.0,
    268_435_456.0,
    1_073_741_824.0,
    2_147_483_648.0,
    4_294_967_296.0,
    8_589_934_592.0,
];

/// Wyrd metric names. Keep these stable — dashboards depend on them.
pub const HTTP_REQUESTS_TOTAL: &str = "wyrd_http_requests_total";
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "wyrd_http_request_duration_seconds";

/// Production-facing query duration metric.
pub const BIFROST_QUERY_DURATION_SECONDS: &str = "bifrost_query_duration_seconds";
/// Production-facing Forge task duration metric.
pub const BIFROST_FORGE_TASK_DURATION_SECONDS: &str = "bifrost_forge_task_duration_seconds";
/// Production-facing Forge cleanup duration metric.
pub const BIFROST_FORGE_CLEANUP_DURATION_SECONDS: &str = "bifrost_forge_cleanup_duration_seconds";
/// Production-facing Forge spill metric.
pub const BIFROST_FORGE_TASK_SPILL_BYTES: &str = "bifrost_forge_task_spill_bytes";
/// Production-facing Forge attempt resource-envelope observation metric.
pub const BIFROST_FORGE_ATTEMPT_RESOURCE_BYTES: &str = "bifrost_forge_attempt_resource_bytes";

/// Gate request latency observed at the public write/query boundary.
pub const BIFROST_GATE_REQUEST_DURATION_SECONDS: &str = "bifrost_gate_request_duration_seconds";
/// Gate query-stream lifetime from dispatch through terminal consumption.
pub const BIFROST_GATE_QUERY_STREAM_DURATION_SECONDS: &str =
    "bifrost_gate_query_stream_duration_seconds";
/// Scribe durable acknowledgement latency.
pub const BIFROST_SCRIBE_ACK_SECONDS: &str = "bifrost_scribe_ack_seconds";
/// Scribe ingress queue wait latency.
pub const BIFROST_SCRIBE_QUEUE_WAIT_SECONDS: &str = "bifrost_scribe_queue_wait_seconds";
/// Scribe execution-lane job latency.
pub const BIFROST_SCRIBE_LANE_JOB_SECONDS: &str = "bifrost_scribe_lane_job_seconds";
/// Scribe persistence publication latency.
pub const BIFROST_SCRIBE_PERSISTENCE_PUBLICATION_SECONDS: &str =
    "bifrost_scribe_persistence_publication_seconds";
/// Scribe seal-stage latency.
pub const BIFROST_SCRIBE_SEAL_STAGE_SECONDS: &str = "bifrost_scribe_seal_stage_seconds";
/// Scribe physical WAL append latency.
pub const BIFROST_SCRIBE_WAL_APPEND_SECONDS: &str = "bifrost_scribe_wal_append_seconds";
/// Scribe physical WAL fsync latency.
pub const BIFROST_SCRIBE_WAL_FSYNC_SECONDS: &str = "bifrost_scribe_wal_fsync_seconds";
/// Forge planning-scheduler pass latency.
pub const BIFROST_FORGE_SCHEDULING_DURATION_SECONDS: &str =
    "bifrost_forge_scheduling_duration_seconds";
/// Forge scheduler orchestration latency.
pub const BIFROST_FORGE_SCHEDULER_DURATION_SECONDS: &str =
    "bifrost_forge_scheduler_duration_seconds";
/// Forge stage latency, including its typed publication stage.
pub const BIFROST_FORGE_STAGE_SECONDS: &str = "bifrost_forge_stage_seconds";
/// Wyrd PostgreSQL tenant-pool acquisition latency.
pub const WYRD_POSTGRES_POOL_ACQUIRE_SECONDS: &str = "wyrd_postgres_pool_acquire_seconds";
/// Vala PostgreSQL tenant-pool acquisition latency.
pub const VALA_POSTGRES_POOL_ACQUIRE_SECONDS: &str = "vala_postgres_pool_acquire_seconds";
/// Shared storage operation latency.
pub const WYRD_STORAGE_OPERATION_DURATION_SECONDS: &str = "wyrd_storage_operation_duration_seconds";

/// Every production Bifrost duration family whose p99 is consumed by qualification.
const BIFROST_P99_DURATION_FAMILIES: &[&str] = &[
    BIFROST_QUERY_DURATION_SECONDS,
    BIFROST_GATE_REQUEST_DURATION_SECONDS,
    BIFROST_GATE_QUERY_STREAM_DURATION_SECONDS,
    BIFROST_SCRIBE_ACK_SECONDS,
    BIFROST_SCRIBE_QUEUE_WAIT_SECONDS,
    BIFROST_SCRIBE_LANE_JOB_SECONDS,
    BIFROST_SCRIBE_PERSISTENCE_PUBLICATION_SECONDS,
    BIFROST_SCRIBE_SEAL_STAGE_SECONDS,
    BIFROST_SCRIBE_WAL_APPEND_SECONDS,
    BIFROST_SCRIBE_WAL_FSYNC_SECONDS,
    BIFROST_FORGE_SCHEDULING_DURATION_SECONDS,
    BIFROST_FORGE_SCHEDULER_DURATION_SECONDS,
    BIFROST_FORGE_STAGE_SECONDS,
    BIFROST_FORGE_TASK_DURATION_SECONDS,
    BIFROST_FORGE_CLEANUP_DURATION_SECONDS,
    "oracle_admission_queue_duration_seconds",
    "oracle_query_duration_seconds",
    "oracle_query_time_to_first_batch_seconds",
    "oracle_fragment_duration_seconds",
    "oracle_audit_append_duration_seconds",
    WYRD_POSTGRES_POOL_ACQUIRE_SECONDS,
    VALA_POSTGRES_POOL_ACQUIRE_SECONDS,
    WYRD_STORAGE_OPERATION_DURATION_SECONDS,
];

/// Errors installing the Prometheus recorder.
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    /// Bucket configuration was rejected by the builder.
    #[error("failed to configure metrics buckets")]
    Buckets(#[source] metrics_exporter_prometheus::BuildError),
    /// A global recorder was already installed in this process.
    #[error("failed to install metrics recorder")]
    Install(#[source] metrics_exporter_prometheus::BuildError),
}

/// Error returned while composing Wyrd's process-wide telemetry runtime.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryRuntimeError {
    /// The production tracing provider could not be installed.
    #[error("failed to install Wyrd tracing provider")]
    Tracing(#[source] wyrd_spec::error::WyrdError),
    /// The production Prometheus recorder could not be installed.
    #[error("failed to install Wyrd Prometheus recorder")]
    Metrics(#[source] MetricsError),
}

/// Server-owned composition of canonical tracing and Prometheus backends.
///
/// The runtime is installed once before server or standalone Forge role
/// composition. `AppState` retains only [`TelemetryGuard`]; the enclosing
/// lifecycle retains this owner and its read-only render handle.
pub struct WyrdTelemetryRuntime {
    /// Guard that owns the process tracing provider lifetime.
    guard: Arc<TelemetryGuard>,
    /// Handle for the one process-global production Prometheus recorder.
    prometheus: PrometheusHandle,
}

impl WyrdTelemetryRuntime {
    /// Install the production tracing subscriber and Prometheus recorder.
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryRuntimeError`] when either process-global backend
    /// cannot be installed. Callers must retain the returned runtime until
    /// every role has stopped.
    pub fn install(config: TelemetryConfig) -> Result<Self, TelemetryRuntimeError> {
        let guard = Arc::new(wyrd_telemetry::init(config).map_err(TelemetryRuntimeError::Tracing)?);
        let prometheus = install_recorder().map_err(TelemetryRuntimeError::Metrics)?;
        Ok(Self { guard, prometheus })
    }

    /// Return the tracing-provider lifetime guard passed into application state.
    #[must_use]
    pub fn guard(&self) -> Arc<TelemetryGuard> {
        Arc::clone(&self.guard)
    }

    /// Return the production Prometheus render handle retained by lifecycle owners.
    #[must_use]
    pub fn prometheus(&self) -> PrometheusHandle {
        self.prometheus.clone()
    }
}

/// Install the same production runtime with a test-only read-only span capture.
///
/// # Errors
///
/// Returns [`TelemetryRuntimeError`] when either process-global backend cannot
/// be installed.
#[cfg(feature = "test-support")]
pub fn install_capture_runtime(
    config: TelemetryConfig,
) -> Result<(WyrdTelemetryRuntime, wyrd_telemetry::TestTraceCapture), TelemetryRuntimeError> {
    let (guard, traces) =
        wyrd_telemetry::init_capture(config).map_err(TelemetryRuntimeError::Tracing)?;
    let prometheus = install_recorder().map_err(TelemetryRuntimeError::Metrics)?;
    Ok((
        WyrdTelemetryRuntime {
            guard: Arc::new(guard),
            prometheus,
        },
        traces,
    ))
}

/// Lifecycle guard for one successfully composed Forge process role.
///
/// The active gauge describes live role concurrency, while the companion
/// counter retains replacement history without inflating that gauge.
pub(crate) struct ForgeRoleTelemetryGuard {
    /// Active-role gauge decremented only when this owner drops after drain.
    active: metrics::Gauge,
}

/// Opaque test-support owner for one production role-lifecycle observation.
#[cfg(feature = "test-support")]
pub struct TestForgeRoleTelemetryGuard {
    /// Production lifecycle guard retained until the test role drains.
    _inner: ForgeRoleTelemetryGuard,
}

impl ForgeRoleTelemetryGuard {
    /// Record that one configured Forge role completed process composition.
    #[must_use]
    pub(crate) fn started(role: BifrostTarget, node_id: uuid::Uuid) -> Self {
        let role = match role {
            BifrostTarget::All => "all",
            BifrostTarget::Server => "server",
            BifrostTarget::Oracle => "oracle",
            BifrostTarget::Scribe => "scribe",
            BifrostTarget::ForgeWorker => "forge_worker",
        };
        let active = metrics::gauge!("bifrost_forge_role_processes", "role" => role);
        active.increment(1.0);
        metrics::counter!("bifrost_forge_role_process_started_total", "role" => role).increment(1);
        metrics::counter!(
            "bifrost_forge_role_node_started_total",
            "role" => role,
            "node_id" => node_id.to_string(),
        )
        .increment(1);
        Self { active }
    }
}

/// Start one test/benchmark role against the production lifecycle instruments.
///
/// The returned guard must be retained until the represented scheduler or
/// worker role has drained. Dropping it records the same active-role shutdown
/// transition used by [`crate::app::BoundServer`].
#[cfg(feature = "test-support")]
#[must_use]
pub fn start_capture_forge_role(
    role: BifrostTarget,
    node_id: uuid::Uuid,
) -> TestForgeRoleTelemetryGuard {
    TestForgeRoleTelemetryGuard {
        _inner: ForgeRoleTelemetryGuard::started(role, node_id),
    }
}

impl Drop for ForgeRoleTelemetryGuard {
    /// Remove this process from active topology after its role has drained.
    fn drop(&mut self) {
        self.active.decrement(1.0);
    }
}

/// Install the process-global Prometheus recorder and return its render handle.
///
/// Call exactly once per process. Returns an error if a recorder is already
/// installed. The returned handle renders the current snapshot on demand.
///
/// # Errors
/// Returns [`MetricsError`] when bucket setup or global installation fails.
pub fn install_recorder() -> Result<PrometheusHandle, MetricsError> {
    let mut builder = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full(HTTP_REQUEST_DURATION_SECONDS.to_owned()),
            REQUEST_DURATION_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?;
    for family in BIFROST_P99_DURATION_FAMILIES {
        builder = builder
            .set_buckets_for_metric(
                Matcher::Full((*family).to_owned()),
                BIFROST_DURATION_BUCKETS,
            )
            .map_err(MetricsError::Buckets)?;
    }
    let handle = builder
        .set_buckets_for_metric(
            Matcher::Full(BIFROST_FORGE_TASK_SPILL_BYTES.to_owned()),
            FORGE_SPILL_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?
        .set_buckets_for_metric(
            Matcher::Full(BIFROST_FORGE_ATTEMPT_RESOURCE_BYTES.to_owned()),
            FORGE_SPILL_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?
        .install_recorder()
        .map_err(MetricsError::Install)?;
    GateRequestLifecycle::register_closed_series();
    Ok(handle)
}

/// Shared test-only Prometheus handle that installs the process recorder once.
///
/// Production composition receives its handle from [`WyrdTelemetryRuntime`].
/// Tests that exercise a real listener use this helper so they retain the same
/// one-recorder invariant without attempting a second global installation.
#[cfg(test)]
pub(crate) fn test_prometheus_handle() -> PrometheusHandle {
    /// Holds the one test-process recorder used by listener and middleware tests.
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE
        .get_or_init(|| install_recorder().expect("test recorder installs exactly once"))
        .clone()
}

/// Owns exactly one terminal Gate request observation for one public request.
///
/// Every public Bifrost transport — the native gRPC query and ingest services
/// and the HTTP `/v1/query` and ingest routes — begins this lifecycle at its
/// handler entry point and settles it exactly once. A normal return calls
/// [`GateRequestLifecycle::complete`]; a transport that drops the handler
/// future in flight settles `cancelled` through [`Drop`]. The active-request
/// gauge is drained on every terminal path, so an abandoned request cannot
/// leave the gauge permanently elevated.
///
/// For a streaming response the terminal is recorded when the handler yields
/// the admitted stream, not when the body finishes. Stream-lifetime outcomes
/// belong to the separate `bifrost_gate_query_streams_total` owner, and both
/// transports must agree on that split for the D24 ledger to reconcile.
pub(crate) struct GateRequestLifecycle {
    /// Closed D24 operation label shared by the counter and duration families.
    operation: &'static str,
    /// Monotonic request start used to derive the duration observation.
    started: std::time::Instant,
    /// Whether a normal return already recorded the terminal observation.
    completed: bool,
    /// Active-request gauge settled on every return or dropped future.
    active: metrics::Gauge,
}

impl GateRequestLifecycle {
    /// Closed D24 operation labels this owner may observe.
    const OPERATIONS: [&'static str; 2] = ["query", "write"];
    /// Closed D24 terminal outcomes this owner may observe.
    const OUTCOMES: [&'static str; 4] = ["success", "rejected", "failed", "cancelled"];

    /// Publishes every closed D24 series this owner can ever emit, at zero.
    ///
    /// A Prometheus counter only appears in a render once some handle has been
    /// created for its exact label set, so a terminal that never occurred in a
    /// window would otherwise leave a hole rather than a zero. Qualification
    /// reads the family as a closed cross product, so the absent series and the
    /// zero series must not be distinguishable. Called from
    /// [`install_recorder`] immediately after the global recorder is live —
    /// registering earlier would emit into the no-op recorder and be lost.
    fn register_closed_series() {
        metrics::describe_counter!(
            "bifrost_gate_requests_total",
            "Total Bifrost Gate requests by closed operation and terminal outcome."
        );
        metrics::describe_histogram!(
            BIFROST_GATE_REQUEST_DURATION_SECONDS,
            metrics::Unit::Seconds,
            "Bifrost Gate request duration by closed operation and terminal outcome."
        );
        metrics::describe_gauge!(
            "bifrost_gate_active_requests",
            "Current in-flight Bifrost Gate requests by closed operation."
        );
        metrics::describe_counter!(
            "bifrost_gate_rejections_total",
            "Total rejected Bifrost Gate requests by closed operation and refusal reason."
        );
        for operation in Self::OPERATIONS {
            metrics::gauge!("bifrost_gate_active_requests", "operation" => operation).set(0.0);
            for outcome in Self::OUTCOMES {
                metrics::counter!(
                    "bifrost_gate_requests_total",
                    "operation" => operation,
                    "outcome" => outcome
                )
                .increment(0);
            }
            for reason in vala_bifrost_redux::gate::error::GATE_REJECTION_REASONS {
                metrics::counter!(
                    "bifrost_gate_rejections_total",
                    "operation" => operation,
                    "reason" => reason
                )
                .increment(0);
            }
        }
    }

    /// Begins one request lifecycle at a public transport entry point.
    ///
    /// `operation` must be a closed D24 label (`"query"` or `"write"`);
    /// admitting an open value would make the family unbounded.
    pub(crate) fn begin(operation: &'static str) -> Self {
        let active = metrics::gauge!("bifrost_gate_active_requests", "operation" => operation);
        active.increment(1.0);
        Self {
            operation,
            started: std::time::Instant::now(),
            completed: false,
            active,
        }
    }

    /// Records one typed refusal against this request's closed operation.
    ///
    /// Callers pass a reason projected by the owning error type — Gate's
    /// [`vala_bifrost_redux::gate::IngestError::rejection_reason`] for writes —
    /// so the taxonomy is never restated at a transport call site. This is
    /// paired with, not a replacement for, the terminal outcome: the same
    /// request also settles `outcome="rejected"` through
    /// [`GateRequestLifecycle::complete`], which is what makes a rejection rate
    /// out of the two families reconcilable.
    ///
    /// Emitting here rather than at the refusal site keeps the operation label
    /// owned by the one lifecycle that already knows it.
    pub(crate) fn reject(&self, reason: &'static str) {
        metrics::counter!(
            "bifrost_gate_rejections_total",
            "operation" => self.operation,
            "reason" => reason
        )
        .increment(1);
    }

    /// Records the normal terminal outcome and disarms cancellation-on-drop.
    ///
    /// `outcome` must be a closed D24 label (`"success"`, `"rejected"`, or
    /// `"failed"`); `"cancelled"` is owned by [`Drop`] alone so the two paths
    /// can never both claim the same request.
    pub(crate) fn complete(mut self, outcome: &'static str) {
        self.record(outcome);
        self.completed = true;
    }

    /// Emits the paired Gate request counter and duration observation.
    fn record(&self, outcome: &'static str) {
        metrics::counter!(
            "bifrost_gate_requests_total",
            "operation" => self.operation,
            "outcome" => outcome
        )
        .increment(1);
        metrics::histogram!(
            BIFROST_GATE_REQUEST_DURATION_SECONDS,
            "operation" => self.operation,
            "outcome" => outcome
        )
        .record(self.started.elapsed().as_secs_f64());
    }
}

impl Drop for GateRequestLifecycle {
    /// Records `cancelled` when a transport drops the handler future in flight.
    fn drop(&mut self) {
        if !self.completed {
            self.record("cancelled");
        }
        self.active.decrement(1.0);
    }
}

/// Build the metrics router: `GET /metrics` renders the Prometheus snapshot.
pub fn metrics_router(handle: PrometheusHandle) -> Router {
    Router::new().route("/metrics", get(move || ready(handle.render())))
}

/// Serve the metrics router on an already-bound `listener` until `shutdown`.
///
/// The caller binds the listener (in `WyrdServer::bind`) so bind failures are
/// boot errors, not task errors — symmetric with `app/serve.rs::serve`.
///
/// # Errors
/// Returns the serve I/O error.
pub async fn serve_metrics(
    router: Router,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
}

#[cfg(test)]
mod tests {
    /// `install_recorder()` must return `Err(MetricsError::Install)` on a second
    /// call rather than panicking. This test calls the raw builder twice to pin
    /// that contract without touching the process-global `OnceLock` in
    /// `tests/metrics.rs`. Each call to `PrometheusBuilder::new().install_recorder()`
    /// attempts to set the global recorder; the second attempt returns a
    /// `BuildError` which we map to `MetricsError::Install`.
    #[test]
    fn install_recorder_second_call_errors_not_panics() {
        // Ensure the process-global recorder is installed exactly once (via the
        // shared `HANDLE`), then confirm a further install attempt returns Err
        // rather than panicking. Going through `get_handle()` keeps every
        // recorder install in this binary funnelled through the single `HANDLE`.
        let _ = get_handle();
        let second = metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder();
        assert!(
            second.is_err(),
            "second install_recorder() call must return Err, not Ok or panic"
        );
    }

    // Integration tests for the Prometheus metrics recorder and middleware.
    //
    // Process-global recorder invariant: `install_recorder()` may only be called
    // once per process. All recorder-installing tests live in this file so they
    // share one binary, one process, one recorder. `HANDLE` initialises the
    // recorder exactly once via `OnceLock`.

    use super::*;
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::middleware::from_fn;
    use axum::routing::get;
    use metrics_exporter_prometheus::PrometheusHandle;
    use tower::ServiceExt;

    use crate::http::middleware::metrics::track_metrics;

    fn get_handle() -> &'static PrometheusHandle {
        /// Holds the recorder shared by the metrics module's rendering tests.
        static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
        HANDLE.get_or_init(test_prometheus_handle)
    }

    #[test]
    fn recorder_installs_and_renders() {
        let handle = get_handle();
        let output = handle.render();
        assert!(
            output.is_empty() || output.contains('#'),
            "render must produce valid Prometheus text (empty or starting with # HELP/# TYPE): got {output:?}"
        );
    }

    /// Every qualification p99 family renders the configured histogram shape.
    #[test]
    fn bifrost_p99_families_render_bucket_count_and_sum() {
        let handle = get_handle();
        for family in BIFROST_P99_DURATION_FAMILIES {
            metrics::histogram!(*family).record(0.01);
        }
        let output = handle.render();
        for family in BIFROST_P99_DURATION_FAMILIES {
            assert!(
                output.contains(&format!("{family}_bucket")),
                "missing buckets for {family}"
            );
            assert!(
                output.contains(&format!("{family}_count")),
                "missing count for {family}"
            );
            assert!(
                output.contains(&format!("{family}_sum")),
                "missing sum for {family}"
            );
        }
    }

    /// The rejection family renders its whole closed contract before any refusal.
    ///
    /// # Panics
    ///
    /// Panics when a seeded `operation` x `reason` series is absent from the
    /// render, which would make an absent series and a zero series
    /// indistinguishable to a scrape.
    #[test]
    fn gate_rejection_family_seeds_every_closed_label_pair() {
        let handle = get_handle();
        let output = handle.render();
        for operation in GateRequestLifecycle::OPERATIONS {
            for reason in vala_bifrost_redux::gate::error::GATE_REJECTION_REASONS {
                assert!(
                    output.contains(&format!(
                        "bifrost_gate_rejections_total{{operation=\"{operation}\",reason=\"{reason}\"}}"
                    )),
                    "missing seeded rejection series for {operation}/{reason}"
                );
            }
        }
    }

    /// A typed refusal increments its own reason without touching the others.
    ///
    /// # Panics
    ///
    /// Panics when the refused series does not advance by exactly one, which
    /// would break the reconciliation between the rejection family and the
    /// `outcome="rejected"` terminal.
    #[test]
    fn gate_rejection_increments_only_the_projected_reason() {
        /// Reads one absolute counter sample out of a rendered exposition.
        fn sample(rendered: &str, series: &str) -> f64 {
            rendered
                .lines()
                .find_map(|line| line.strip_prefix(series)?.trim().parse::<f64>().ok())
                .unwrap_or_else(|| panic!("series {series} must render"))
        }
        let handle = get_handle();
        let series = "bifrost_gate_rejections_total{operation=\"write\",reason=\"catalog\"}";
        let untouched = "bifrost_gate_rejections_total{operation=\"write\",reason=\"auth\"}";
        let before = sample(&handle.render(), series);
        let before_untouched = sample(&handle.render(), untouched);

        let error = vala_bifrost_redux::gate::IngestError::TableNotFound {
            table: "vala.bifrost/absent".to_owned(),
        };
        let reason = error
            .rejection_reason()
            .expect("a table-not-found refusal is a caller-attributed rejection");
        GateRequestLifecycle::begin("write").reject(reason);

        let rendered = handle.render();
        assert!(
            (sample(&rendered, series) - before - 1.0).abs() < f64::EPSILON,
            "the projected reason must advance by exactly one"
        );
        assert!(
            (sample(&rendered, untouched) - before_untouched).abs() < f64::EPSILON,
            "an unrelated reason must not move"
        );
    }

    #[tokio::test]
    async fn metrics_router_serves_metrics() {
        let handle = get_handle();
        let router = metrics_router(handle.clone());

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("metrics router responds");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "GET /metrics must return 200"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        let text = String::from_utf8(body.to_vec()).expect("body is valid UTF-8");
        assert!(
            text.is_empty() || text.contains('#'),
            "body must be empty or valid Prometheus text format: {text:?}"
        );
    }

    #[tokio::test]
    async fn track_metrics_records_request() {
        let handle = get_handle();

        let router = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(from_fn(track_metrics));

        router
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("probe request completes");

        let output = handle.render();
        assert!(
            output.contains(HTTP_REQUESTS_TOTAL),
            "rendered output must contain the requests counter name; got:\n{output}"
        );
        assert!(
            output.contains(HTTP_REQUEST_DURATION_SECONDS),
            "rendered output must contain the duration histogram name; got:\n{output}"
        );
    }

    #[tokio::test]
    async fn track_metrics_unmatched_route_uses_bounded_label() {
        let handle = get_handle();

        let router = Router::new()
            .route("/known", get(|| async { "ok" }))
            .layer(from_fn(track_metrics));

        let raw_path = "/scanner-bait/../../etc/passwd";
        router
            .oneshot(
                Request::builder()
                    .uri(raw_path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request to unmatched route completes");

        let output = handle.render();
        assert!(
            output.contains("__unmatched__"),
            "unmatched paths must use the bounded '__unmatched__' label; got:\n{output}"
        );
        assert!(
            !output.contains("etc/passwd"),
            "raw URI path must never appear as a metric label (unbounded cardinality); got:\n{output}"
        );
    }
}
