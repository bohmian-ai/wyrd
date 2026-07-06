//! Prometheus metrics recorder + scrape endpoint.
//!
//! The recorder is installed once at startup; `track_metrics`
//! (`http/middleware/metrics.rs`) records per-request counters/histograms into
//! it, and `/metrics` renders the current snapshot on a dedicated listener.

use std::future::ready;

use axum::{Router, routing::get};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Histogram buckets (seconds) for request-duration metrics.
const REQUEST_DURATION_BUCKETS: &[f64] =
    &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

/// Wyrd metric names. Keep these stable — dashboards depend on them.
pub const HTTP_REQUESTS_TOTAL: &str = "wyrd_http_requests_total";
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "wyrd_http_request_duration_seconds";

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

/// Install the process-global Prometheus recorder and return its render handle.
///
/// Call exactly once per process. Returns an error if a recorder is already
/// installed. The returned handle renders the current snapshot on demand.
///
/// # Errors
/// Returns [`MetricsError`] when bucket setup or global installation fails.
pub fn install_recorder() -> Result<PrometheusHandle, MetricsError> {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full(HTTP_REQUEST_DURATION_SECONDS.to_owned()),
            REQUEST_DURATION_BUCKETS,
        )
        .map_err(MetricsError::Buckets)?
        .install_recorder()
        .map_err(MetricsError::Install)
}

/// Build the metrics router: `GET /metrics` renders the Prometheus snapshot.
#[must_use]
pub fn metrics_router(handle: PrometheusHandle) -> Router {
    Router::new().route("/metrics", get(move || ready(handle.render())))
}

/// Serve the metrics router on an already-bound `listener` until `shutdown`.
///
/// The caller binds the listener (in `WyrdServer::serve`) so bind failures are
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
