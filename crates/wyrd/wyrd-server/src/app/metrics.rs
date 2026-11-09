//! The process metrics backend and its listener.
//!
//! One backend is selected and installed once at startup, before any metric
//! is registered: the production Prometheus recorder, whose `/metrics`
//! renders the current exposition, or, for an explicit benchmark on a
//! loopback listener, the [`wyrd_benchmark`] recorder, whose snapshot route
//! replaces `/metrics`. `track_metrics` (`http/middleware/metrics.rs`) and
//! every other owner record through the standard `metrics` facade into
//! whichever one was installed.

use std::future::ready;
use std::sync::Arc;
#[cfg(test)]
use std::sync::OnceLock;

use axum::{Router, routing::get};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wyrd_telemetry::{TelemetryConfig, TelemetryGuard};

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

/// Verification execution, queue, phase, and overhead buckets (seconds),
/// resolving sub-millisecond preparation, the 1 ms direct-overhead p99
/// objective (a p99 in the `le="0.001"` bucket cannot prove it is below
/// 1 ms, so finer bounds sit under it), and the 10 ms scoring objective.
const VERIFICATION_DURATION_BUCKETS: &[f64] = &[
    0.0001, 0.00025, 0.0005, 0.00075, 0.0009, 0.001, 0.0025, 0.005, 0.0075, 0.009, 0.01, 0.025,
    0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0,
];

/// Source executions pinned by one group snapshot, from a single execution to
/// a large end-of-group fan-in.
const VERIFICATION_GROUP_SOURCE_BUCKETS: &[f64] = &[
    1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
];

/// Wyrd metric names. Keep these stable — dashboards depend on them.
pub const HTTP_REQUESTS_TOTAL: &str = "wyrd_http_requests_total";
/// Histogram of HTTP request latency in seconds, labelled like [`HTTP_REQUESTS_TOTAL`].
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "wyrd_http_request_duration_seconds";

/// Production-facing Forge task duration metric.
pub const BIFROST_FORGE_TASK_DURATION_SECONDS: &str = "bifrost_forge_task_duration_seconds";
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
/// Scribe physical WAL append latency.
pub const BIFROST_SCRIBE_WAL_APPEND_SECONDS: &str = "bifrost_scribe_wal_append_seconds";
/// Scribe physical WAL fsync latency.
pub const BIFROST_SCRIBE_WAL_FSYNC_SECONDS: &str = "bifrost_scribe_wal_fsync_seconds";
/// One outbox write, from dispatch to settlement, labelled by `outbox`: the
/// drain step of a tenant's backlog, since a tenant has one write in flight.
pub const OUTBOX_WRITE_SECONDS: &str = "outbox_write_seconds";
/// Wyrd PostgreSQL tenant-pool acquisition latency.
pub const WYRD_POSTGRES_POOL_ACQUIRE_SECONDS: &str = "wyrd_postgres_pool_acquire_seconds";
/// Vala PostgreSQL tenant-pool acquisition latency.
pub const VALA_POSTGRES_POOL_ACQUIRE_SECONDS: &str = "vala_postgres_pool_acquire_seconds";
/// Shared storage operation latency.
pub const WYRD_STORAGE_OPERATION_DURATION_SECONDS: &str = "wyrd_storage_operation_duration_seconds";

/// Non-terminal Verifier runs and Operator dispatches, by `queue` and `status`.
pub const VERIFICATION_QUEUE_DEPTH: &str = "wyrd_verification_queue_depth";
/// Verifier executions currently in flight in this process, by `mode`.
pub const VERIFICATION_ACTIVE_RUNS: &str = "wyrd_verification_active_runs";
/// Settled queued attempts and admitted direct executions, by `mode`; a
/// deferred claim spends no attempt and is not counted.
pub const VERIFICATION_RUN_ATTEMPTS_TOTAL: &str = "wyrd_verification_run_attempts_total";
/// Unsuccessful execution outcomes, by `mode` and `outcome`; a
/// completed failed judgment is not one.
pub const VERIFICATION_RUN_FAILURES_TOTAL: &str = "wyrd_verification_run_failures_total";
/// Queued claim-to-settlement or direct entry-to-result latency, by
/// `mode` and `outcome`.
pub const VERIFICATION_RUN_DURATION_SECONDS: &str = "wyrd_verification_run_duration_seconds";
/// How long a claimed Verifier run had been claimable before its claim, as
/// PostgreSQL measured it, by `origin`; recorded once per attempt, never for
/// a deferred claim.
pub const VERIFICATION_QUEUE_WAIT_SECONDS: &str = "wyrd_verification_queue_wait_seconds";
/// Run creation to durable terminal settlement, by `origin` and
/// terminal `outcome`; its `_count` is the settled terminal runs.
pub const VERIFICATION_TRIGGER_TO_TERMINAL_SECONDS: &str =
    "wyrd_verification_trigger_to_terminal_seconds";
/// Overlapping phases of one execution, by `mode` and `phase`
/// (`load`, `input_read`, `prepare`, `engine`, `settlement`).
pub const VERIFICATION_PHASE_DURATION_SECONDS: &str = "wyrd_verification_phase_duration_seconds";
/// The runtime's overhead of one execution: the whole execution (a direct
/// request from its HTTP arrival, a queued attempt from its claim) less the
/// union of the Verifier's own blocks, by `mode` and `outcome`; wall time.
pub const VERIFICATION_OVERHEAD_SECONDS: &str = "wyrd_verification_overhead_seconds";
/// The Verifier's own execution: the union of the actions and checks it ran
/// in one execution, by `mode` and `outcome`; wall time.
pub const VERIFICATION_VERIFIER_SECONDS: &str = "wyrd_verification_verifier_seconds";
/// One background submission from its HTTP arrival until it is staged or
/// refused, by `outcome`; wall time.
pub const VERIFICATION_SUBMIT_SECONDS: &str = "wyrd_verification_submit_seconds";
/// Scheduler occurrences processed, by `outcome`.
pub const VERIFICATION_SCHEDULE_TICKS_TOTAL: &str = "wyrd_verification_schedule_ticks_total";
/// One Verifier block attempt inside the engine, by `block` (`action`,
/// `check`), `kind`, and `outcome` (`ok`, `error`); excludes permit waits.
pub const VERIFICATION_BLOCK_DURATION_SECONDS: &str = vala_verify::blocks::BLOCK_DURATION_SECONDS;
/// One verification queue or group-scheduling SQL operation, by `op` and
/// `outcome` (`ok`, `error`).
pub const VERIFICATION_STORE_DURATION_SECONDS: &str =
    wyrd_sql::queries::verifier_executions::VERIFICATION_STORE_DURATION_SECONDS;
/// One scheduler pass or group handoff, by `pass` (`cron`, `group`,
/// `group_handoff`) and `outcome` (`ok`, `error`).
pub const VERIFICATION_SCHEDULE_PASS_SECONDS: &str = "wyrd_verification_schedule_pass_seconds";
/// The Bifrost read pinning one group snapshot, by `outcome` (`ok`, `error`).
pub const VERIFICATION_GROUP_SNAPSHOT_SECONDS: &str = "wyrd_verification_group_snapshot_seconds";
/// Source executions pinned by one complete group snapshot.
pub const VERIFICATION_GROUP_SNAPSHOT_SOURCES: &str = "wyrd_verification_group_snapshot_sources";
/// Group handoffs left pending by a Wyrd-side blockage, by `reason`
/// (`no_owner_principal`, `no_system_principal`, `authority_unavailable`,
/// `enqueue_refused`, `snapshot_read`, `snapshot_incomplete`).
pub const VERIFICATION_GROUP_HANDOFFS_BLOCKED_TOTAL: &str =
    "wyrd_verification_group_handoffs_blocked_total";
/// Whether each verification runtime capability task is running, by `capability`.
pub const VERIFICATION_CAPABILITY_UP: &str = "wyrd_verification_capability_up";
/// Unexpected verification capability exits that were restarted, by `capability`.
pub const VERIFICATION_CAPABILITY_RESTARTS_TOTAL: &str =
    "wyrd_verification_capability_restarts_total";
/// Operator deliveries currently holding an Operator permit in this process.
pub const OPERATOR_ACTIVE_DISPATCHES: &str = "wyrd_operator_active_dispatches";
/// Settled Operator delivery attempts, by `outcome`.
pub const OPERATOR_DISPATCH_ATTEMPTS_TOTAL: &str = "wyrd_operator_dispatch_attempts_total";
/// Claim-to-settlement latency of one Operator attempt, by `outcome`.
pub const OPERATOR_DISPATCH_DURATION_SECONDS: &str = "wyrd_operator_dispatch_duration_seconds";

/// A governed gateway call's arrival until its answer, or stream end, was
/// handed to the response body.
pub const GATEWAY_DURATION_SECONDS: &str = "wyrd_gateway_duration_seconds";
/// [`GATEWAY_DURATION_SECONDS`] less the provider exchange time the call
/// waited on: the server overhead of one call.
pub const GATEWAY_OVERHEAD_SECONDS: &str = "wyrd_gateway_overhead_seconds";
/// Provider exchange time one call waited on, summed across its attempts.
pub const GATEWAY_UPSTREAM_SECONDS: &str = "wyrd_gateway_upstream_seconds";
/// One request-path phase of a governed gateway call.
pub const GATEWAY_PHASE_DURATION_SECONDS: &str = "wyrd_gateway_phase_duration_seconds";
/// A governed call's receipt until its execution task settled it.
pub const GATEWAY_REQUEST_DURATION_SECONDS: &str = "wyrd_gateway_request_duration_seconds";
/// One provider attempt's wall-clock duration.
pub const GATEWAY_ATTEMPT_DURATION_SECONDS: &str = "wyrd_gateway_attempt_duration_seconds";
/// A governed call's receipt until its answer reached the caller.
pub const GATEWAY_TIME_TO_FIRST_BYTE_SECONDS: &str = "wyrd_gateway_time_to_first_byte_seconds";
/// A governed call's receipt until its execution task started.
pub const GATEWAY_QUEUE_DELAY_SECONDS: &str = "wyrd_gateway_queue_delay_seconds";

/// Gateway latency families sharing [`VERIFICATION_DURATION_BUCKETS`], so a
/// sub-millisecond overhead p99 is resolvable.
const GATEWAY_DURATION_FAMILIES: &[&str] = &[
    GATEWAY_DURATION_SECONDS,
    GATEWAY_OVERHEAD_SECONDS,
    GATEWAY_UPSTREAM_SECONDS,
    GATEWAY_PHASE_DURATION_SECONDS,
    GATEWAY_REQUEST_DURATION_SECONDS,
    GATEWAY_ATTEMPT_DURATION_SECONDS,
    GATEWAY_TIME_TO_FIRST_BYTE_SECONDS,
    GATEWAY_QUEUE_DELAY_SECONDS,
];

/// Verification latency families sharing [`VERIFICATION_DURATION_BUCKETS`].
const VERIFICATION_DURATION_FAMILIES: &[&str] = &[
    VERIFICATION_RUN_DURATION_SECONDS,
    VERIFICATION_QUEUE_WAIT_SECONDS,
    VERIFICATION_TRIGGER_TO_TERMINAL_SECONDS,
    VERIFICATION_PHASE_DURATION_SECONDS,
    VERIFICATION_OVERHEAD_SECONDS,
    VERIFICATION_VERIFIER_SECONDS,
    VERIFICATION_SUBMIT_SECONDS,
    VERIFICATION_BLOCK_DURATION_SECONDS,
    VERIFICATION_STORE_DURATION_SECONDS,
    VERIFICATION_SCHEDULE_PASS_SECONDS,
    VERIFICATION_GROUP_SNAPSHOT_SECONDS,
];

/// Every production Bifrost duration family whose p99 is consumed by qualification.
const BIFROST_P99_DURATION_FAMILIES: &[&str] = &[
    BIFROST_GATE_REQUEST_DURATION_SECONDS,
    BIFROST_GATE_QUERY_STREAM_DURATION_SECONDS,
    BIFROST_SCRIBE_ACK_SECONDS,
    BIFROST_SCRIBE_QUEUE_WAIT_SECONDS,
    BIFROST_SCRIBE_LANE_JOB_SECONDS,
    BIFROST_SCRIBE_PERSISTENCE_PUBLICATION_SECONDS,
    BIFROST_SCRIBE_WAL_APPEND_SECONDS,
    BIFROST_SCRIBE_WAL_FSYNC_SECONDS,
    OUTBOX_WRITE_SECONDS,
    BIFROST_FORGE_TASK_DURATION_SECONDS,
    "oracle_admission_queue_duration_seconds",
    "oracle_query_duration_seconds",
    "oracle_query_time_to_first_batch_seconds",
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
    /// Another recorder was installed before the benchmark recorder.
    #[error("failed to install the benchmark recorder")]
    Benchmark(#[source] metrics::SetRecorderError<wyrd_benchmark::BenchmarkRecorder>),
}

/// The process's one metrics backend, selected once at startup.
#[derive(Clone)]
pub enum MetricsBackend {
    /// The production Prometheus recorder and its render handle.
    Prometheus(PrometheusHandle),
    /// The [`wyrd_benchmark`] recorder of an explicit benchmark server.
    Benchmark,
}

impl MetricsBackend {
    /// Installs the benchmark recorder when `benchmark`, which configuration
    /// allows only on a loopback metrics listener, and the production
    /// Prometheus recorder otherwise, then registers the Gate families into
    /// it. Call once per process, before any metric is registered.
    ///
    /// # Errors
    ///
    /// Returns [`MetricsError`] when bucket setup fails or a recorder is
    /// already installed; nothing falls back to the other backend.
    pub fn install(benchmark: bool) -> Result<Self, MetricsError> {
        if !benchmark {
            return install_recorder().map(Self::Prometheus);
        }
        wyrd_benchmark::install().map_err(MetricsError::Benchmark)?;
        vala_bifrost_redux::gate::initialize_gate_metrics();
        Ok(Self::Benchmark)
    }

    /// The Prometheus render handle; `None` for the benchmark backend.
    #[must_use]
    pub fn prometheus(&self) -> Option<&PrometheusHandle> {
        match self {
            Self::Prometheus(handle) => Some(handle),
            Self::Benchmark => None,
        }
    }
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

/// Server-owned composition of canonical tracing and the metrics backend.
///
/// The runtime is installed once before server or standalone Forge role
/// composition. `AppState` retains only [`TelemetryGuard`]; the enclosing
/// lifecycle retains this owner and its metrics backend.
pub struct WyrdTelemetryRuntime {
    /// Guard that owns the process tracing provider lifetime.
    guard: Arc<TelemetryGuard>,
    /// The one process-global metrics backend.
    metrics: MetricsBackend,
}

impl WyrdTelemetryRuntime {
    /// Install the production tracing subscriber and the metrics backend
    /// `benchmark` selects ([`MetricsBackend::install`]).
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryRuntimeError`] when either process-global backend
    /// cannot be installed. Callers must retain the returned runtime until
    /// every role has stopped.
    pub fn install(
        config: TelemetryConfig,
        benchmark: bool,
    ) -> Result<Self, TelemetryRuntimeError> {
        let metrics = MetricsBackend::install(benchmark).map_err(TelemetryRuntimeError::Metrics)?;
        let guard = Arc::new(wyrd_telemetry::init(config).map_err(TelemetryRuntimeError::Tracing)?);
        Ok(Self { guard, metrics })
    }

    /// Return the tracing-provider lifetime guard passed into application state.
    #[must_use]
    pub fn guard(&self) -> Arc<TelemetryGuard> {
        Arc::clone(&self.guard)
    }

    /// Return the metrics backend retained by lifecycle owners.
    #[must_use]
    pub fn metrics(&self) -> MetricsBackend {
        self.metrics.clone()
    }

    /// Return the production Prometheus render handle; `None` for a
    /// benchmark server.
    #[must_use]
    pub fn prometheus(&self) -> Option<PrometheusHandle> {
        self.metrics.prometheus().cloned()
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
        wyrd_telemetry::init_test_capture(config).map_err(TelemetryRuntimeError::Tracing)?;
    let prometheus = install_recorder().map_err(TelemetryRuntimeError::Metrics)?;
    Ok((
        WyrdTelemetryRuntime {
            guard: Arc::new(guard),
            metrics: MetricsBackend::Prometheus(prometheus),
        },
        traces,
    ))
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
        .map_err(MetricsError::Buckets)?
        .set_buckets_for_metric(
            Matcher::Full(OPERATOR_DISPATCH_DURATION_SECONDS.to_owned()),
            BIFROST_DURATION_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?
        .set_buckets_for_metric(
            Matcher::Full(VERIFICATION_GROUP_SNAPSHOT_SOURCES.to_owned()),
            VERIFICATION_GROUP_SOURCE_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?;
    for family in VERIFICATION_DURATION_FAMILIES {
        builder = builder
            .set_buckets_for_metric(
                Matcher::Full((*family).to_owned()),
                VERIFICATION_DURATION_BUCKETS,
            )
            .map_err(MetricsError::Buckets)?;
    }
    for family in GATEWAY_DURATION_FAMILIES {
        builder = builder
            .set_buckets_for_metric(
                Matcher::Full((*family).to_owned()),
                VERIFICATION_DURATION_BUCKETS,
            )
            .map_err(MetricsError::Buckets)?;
    }
    for family in BIFROST_P99_DURATION_FAMILIES {
        builder = builder
            .set_buckets_for_metric(
                Matcher::Full((*family).to_owned()),
                BIFROST_DURATION_BUCKETS,
            )
            .map_err(MetricsError::Buckets)?;
    }
    let handle = builder.install_recorder().map_err(MetricsError::Install)?;
    vala_bifrost_redux::gate::initialize_gate_metrics();
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

/// The benchmark metrics listener's read-only snapshot route.
pub const BENCHMARK_SNAPSHOT_PATH: &str = "/benchmark/snapshot";

/// Build the metrics router for the installed `backend`.
///
/// The Prometheus backend serves `GET /metrics`, the rendered exposition.
/// The benchmark backend, which configuration allows only on a loopback
/// listener, serves no exposition: `GET` [`BENCHMARK_SNAPSHOT_PATH`] returns
/// the cumulative [`wyrd_benchmark::BenchmarkSnapshot`] as JSON, bounded
/// aggregate timings, counters, gauges, and histograms only, with no
/// request identity, tenant, payload, query, or reset.
///
/// A benchmark `bench` build also serves [`crate::heap_profile::PATH`]:
/// `POST` starts heap sampling and `GET` returns the sampled allocations,
/// allocated and still live, as a gzipped pprof profile of sizes and code
/// locations only.
pub fn metrics_router(backend: MetricsBackend) -> Router {
    let MetricsBackend::Prometheus(handle) = backend else {
        return benchmark_router();
    };
    Router::new().route("/metrics", get(move || ready(handle.render())))
}

/// The benchmark backend's snapshot router.
fn benchmark_router() -> Router {
    let router = Router::new().route(
        BENCHMARK_SNAPSHOT_PATH,
        get(|| ready(axum::Json(wyrd_benchmark::snapshot()))),
    );
    #[cfg(feature = "bench")]
    let router = router.route(
        crate::heap_profile::PATH,
        get(heap_profile).post(|| {
            crate::heap_profile::start();
            ready(axum::http::StatusCode::NO_CONTENT)
        }),
    );
    router
}

/// The sampled heap profile, rendered on a blocking thread because
/// symbolizing takes a while.
///
/// # Errors
///
/// Answers 500 when rendering panicked.
#[cfg(feature = "bench")]
async fn heap_profile() -> Result<impl axum::response::IntoResponse, axum::http::StatusCode> {
    let profile = tokio::task::spawn_blocking(crate::heap_profile::profile)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        profile,
    ))
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

    /// Ordinary startup installs the Prometheus recorder, serves no
    /// benchmark route, and accumulates no benchmark state from either the
    /// standard metrics or an explicit request timing.
    ///
    /// # Panics
    ///
    /// Panics when the render is not exposition text, a benchmark route is
    /// served, or the benchmark registry recorded anything.
    #[tokio::test]
    async fn recorder_installs_and_renders() {
        let handle = get_handle();
        let output = handle.render();
        assert!(
            output.is_empty() || output.contains('#'),
            "render must produce valid Prometheus text (empty or starting with # HELP/# TYPE): got {output:?}"
        );
        let router = metrics_router(MetricsBackend::Prometheus(handle.clone()));
        for path in [BENCHMARK_SNAPSHOT_PATH, "/benchmark/heap"] {
            assert_eq!(fetch(&router, "GET", path).await.0, StatusCode::NOT_FOUND);
        }
        metrics::counter!(HTTP_REQUESTS_TOTAL).increment(1);
        wyrd_benchmark::record_timing(
            &tracing::Span::none(),
            "bifrost.query",
            "success",
            std::time::Duration::from_micros(1),
            None,
        );
        assert!(!wyrd_benchmark::enabled());
        let snapshot = wyrd_benchmark::snapshot();
        assert!(
            snapshot.series.is_empty() && snapshot.metrics.is_empty(),
            "an ordinary process accumulates no benchmark observation"
        );
        assert!(handle.render().contains(HTTP_REQUESTS_TOTAL));
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

    /// Every gateway duration family renders the fine verification buckets,
    /// so an overhead under 1 ms lands below the `le="0.001"` bound.
    ///
    /// # Panics
    ///
    /// Panics when a family renders without those buckets, count, or sum.
    #[test]
    fn gateway_duration_families_render_buckets() {
        let handle = get_handle();
        for family in GATEWAY_DURATION_FAMILIES {
            metrics::histogram!(*family, "operation" => "chat_completions").record(0.0004);
        }
        let output = handle.render();
        for family in GATEWAY_DURATION_FAMILIES {
            for suffix in [
                "_bucket{operation=\"chat_completions\",le=\"0.0005\"}",
                "_count",
                "_sum",
            ] {
                assert!(
                    output.contains(&format!("{family}{suffix}")),
                    "missing {suffix} for {family}"
                );
            }
        }
    }

    #[tokio::test]
    async fn metrics_router_serves_metrics() {
        let handle = get_handle();
        let router = metrics_router(MetricsBackend::Prometheus(handle.clone()));

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

    /// Fetches `path` with `method` from `router`, returning the status and
    /// body text.
    ///
    /// # Panics
    ///
    /// Panics when the request cannot build, the router does not respond, or
    /// the body is not collected UTF-8.
    async fn fetch(router: &Router, method: &str, path: &str) -> (StatusCode, String) {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("metrics router responds");
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        (
            status,
            String::from_utf8(body.to_vec()).expect("UTF-8 body"),
        )
    }

    /// The snapshot route of a benchmark router, parsed.
    ///
    /// # Panics
    ///
    /// Panics when the route does not answer 200 with a `BenchmarkSnapshot`.
    async fn timings(router: &Router) -> wyrd_benchmark::BenchmarkSnapshot {
        let (status, body) = fetch(router, "GET", BENCHMARK_SNAPSHOT_PATH).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        serde_json::from_str(&body).expect("the snapshot is a BenchmarkSnapshot")
    }

    /// This process's benchmark router, installing the benchmark backend.
    ///
    /// # Panics
    ///
    /// Panics when another recorder was installed first.
    fn benchmark() -> Router {
        let backend = MetricsBackend::install(true).expect("the benchmark backend installs");
        assert!(matches!(backend, MetricsBackend::Benchmark));
        metrics_router(backend)
    }

    /// Benchmark startup installs only the benchmark recorder, before the
    /// Gate families register: their series, and every later counter,
    /// gauge, and histogram, reach its snapshot, no Prometheus exposition is
    /// served, and the production recorder cannot be installed beside it.
    ///
    /// # Panics
    ///
    /// Panics when a Prometheus backend is installed or served, a
    /// registration is missing from the snapshot, or a fallback succeeds.
    #[tokio::test]
    async fn benchmark_mode_installs_only_benchmark_recorder() {
        let router = benchmark();
        assert!(wyrd_benchmark::enabled());
        assert!(MetricsBackend::Benchmark.prometheus().is_none());
        assert!(
            matches!(install_recorder(), Err(MetricsError::Install(_))),
            "no Prometheus recorder installs beside the benchmark recorder"
        );
        assert!(
            matches!(
                WyrdTelemetryRuntime::install(TelemetryConfig::default(), false),
                Err(TelemetryRuntimeError::Metrics(MetricsError::Install(_)))
            ),
            "there is no live switch to the production backend"
        );
        assert_eq!(
            fetch(&router, "GET", "/metrics").await.0,
            StatusCode::NOT_FOUND
        );
        let before = timings(&router).await;
        assert!(
            before
                .metrics
                .iter()
                .any(|metric| metric
                    .matches("bifrost_gate_requests_total", &[("operation", "write")])),
            "series registered at startup land in the benchmark recorder"
        );
        metrics::counter!(HTTP_REQUESTS_TOTAL, "status" => "200").increment(3);
        metrics::gauge!(VERIFICATION_ACTIVE_RUNS, "mode" => "direct").increment(2.0);
        metrics::histogram!(OUTBOX_WRITE_SECONDS, "outbox" => "scribe").record(0.002);
        let interval = timings(&router).await.since(&before).expect("one process");
        assert_eq!(interval.counter(HTTP_REQUESTS_TOTAL, &[]), Some(3));
        assert!(interval.metrics.iter().any(|metric| matches!(
            metric.value,
            wyrd_benchmark::IntervalValue::Gauge { value, .. }
                if metric.name == VERIFICATION_ACTIVE_RUNS && value.to_bits() == 2.0_f64.to_bits()
        )));
        let writes = interval
            .histogram(OUTBOX_WRITE_SECONDS, &[("outbox", "scribe")])
            .expect("the outbox write");
        assert_eq!(writes.len(), 1);
    }

    /// The server's boundary timings reach the snapshot as separate
    /// elapsed, excluded, and residual populations: the residual is
    /// computed within each request (overlapping waits already unioned by
    /// the owner, zero wait kept), the streaming head and final frame stay
    /// distinct series, and an excluded wait beyond the elapsed time is a
    /// reported problem, never a zero residual.
    ///
    /// # Panics
    ///
    /// Panics when a population, residual, or problem is wrong.
    #[tokio::test]
    async fn benchmark_timings_preserve_server_boundaries() {
        use std::time::Duration;
        let router = benchmark();
        let before = timings(&router).await;
        let record = |boundary, elapsed: u64, excluded: Option<u64>| {
            wyrd_benchmark::record_timing(
                &tracing::Span::none(),
                boundary,
                "success",
                Duration::from_nanos(elapsed),
                excluded.map(Duration::from_nanos),
            );
        };
        record("verification.direct", 900_000, Some(600_000));
        record("verification.direct", 300_000, Some(0));
        record("gateway.stream.head", 120_000_000, Some(100_000_000));
        record("gateway.stream.end", 450_000_000, Some(430_000_000));
        record("gateway.buffered", 1_000, Some(1_001));
        record("bifrost.query", 2_000_000, None);
        let interval = timings(&router)
            .await
            .since(&before)
            .expect("one process, no reset");
        let series = |boundary: &str| {
            interval
                .series
                .iter()
                .find(|series| series.boundary == boundary)
                .unwrap_or_else(|| panic!("no {boundary} series"))
        };
        let verification = series("verification.direct");
        assert!(
            verification.problems().is_empty(),
            "{:?}",
            verification.problems()
        );
        let residual = verification.residual.as_ref().expect("residual population");
        let low = residual.quantile(0.0).expect("residuals");
        let high = residual.maximum().expect("residuals");
        assert!(
            low.lowest <= 300_000 && 300_000 <= low.highest,
            "zero wait keeps the elapsed time"
        );
        assert!(
            high.lowest <= 300_000 && 300_000 <= high.highest,
            "900 µs less 600 µs"
        );
        assert_eq!(
            verification
                .excluded
                .as_ref()
                .map(wyrd_benchmark::Distribution::len),
            Some(2)
        );
        let head = series("gateway.stream.head")
            .residual
            .clone()
            .expect("head residual");
        let end = series("gateway.stream.end")
            .residual
            .clone()
            .expect("end residual");
        let head = head.maximum().expect("head");
        let end = end.maximum().expect("end");
        assert!(head.lowest <= 20_000_000 && 20_000_000 <= head.highest);
        assert!(end.lowest <= 20_000_000 && 20_000_000 <= end.highest);
        let negative = series("gateway.buffered").problems().join("; ");
        assert!(
            negative.contains("1 excluded waits exceeding elapsed time"),
            "{negative}"
        );
        assert!(
            negative.contains("0 residual observations for 1 requests"),
            "{negative}"
        );
        let query = series("bifrost.query");
        assert!(query.excluded.is_none() && query.residual.is_none());
        assert!(query.problems().is_empty());
    }

    /// The benchmark snapshot, served only by the opt-in benchmark backend
    /// (an ordinary router serves none, as [`recorder_installs_and_renders`]
    /// proves), is a GET-only, reset-free, cumulative readout of bounded
    /// aggregates from one process identity, carrying no request identity or
    /// payload.
    ///
    /// # Panics
    ///
    /// Panics when the route accepts a mutation, resets, or leaks an
    /// identity.
    #[tokio::test]
    async fn benchmark_snapshots_are_local_read_only_and_opt_in() {
        let router = benchmark();
        #[cfg(not(feature = "bench"))]
        assert_eq!(
            fetch(&router, "GET", "/benchmark/heap").await.0,
            StatusCode::NOT_FOUND,
            "only a bench build serves heap profiles"
        );
        let secret = "tenant-7f3a-trace-4bf92f3577b34da6a3ce929d0e0e4736";
        let span = tracing::info_span!("request", tenant = secret);
        wyrd_benchmark::record_timing(
            &span,
            "bifrost.gate.write",
            "success",
            std::time::Duration::from_micros(250),
            None,
        );
        let first = timings(&router).await;
        for method in ["POST", "PUT", "DELETE"] {
            assert_eq!(
                fetch(&router, method, BENCHMARK_SNAPSHOT_PATH).await.0,
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} must not reach the readout"
            );
        }
        let (status, reset) = fetch(&router, "GET", "/benchmark/snapshot?reset=true").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !reset.contains(secret),
            "no request identity in the readout"
        );
        let second = timings(&router).await;
        assert_eq!(second.process, first.process);
        assert!(second.taken_ns >= first.taken_ns);
        let interval = second.since(&first).expect("a query string resets nothing");
        assert!(interval.series.is_empty(), "repeated reads record nothing");
        assert!(second.series.len() <= wyrd_benchmark::MAX_SERIES);
        assert!(second.metrics.len() <= wyrd_benchmark::MAX_METRIC_SERIES);
        assert_eq!(second.method, wyrd_benchmark::Method::current());
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

    /// A `bench` build's heap route starts sampling on `POST`, serves a
    /// gzipped profile on `GET`, and refuses every other method.
    ///
    /// # Panics
    ///
    /// Panics when a method is answered wrongly.
    #[cfg(feature = "bench")]
    #[tokio::test]
    async fn bench_heap_profile_starts_and_serves() {
        let router = benchmark();
        let call = |method: &str| {
            let request = Request::builder()
                .method(method)
                .uri(crate::heap_profile::PATH)
                .body(Body::empty())
                .expect("request builds");
            router.clone().oneshot(request)
        };
        assert_eq!(
            call("POST").await.expect("POST").status(),
            StatusCode::NO_CONTENT
        );
        let response = call("GET").await.expect("GET");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&body[..2], [0x1f, 0x8b], "a gzip stream");
        for method in ["PUT", "DELETE"] {
            assert_eq!(
                call(method).await.expect(method).status(),
                StatusCode::METHOD_NOT_ALLOWED
            );
        }
    }
}
