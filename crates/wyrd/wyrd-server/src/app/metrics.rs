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
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

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
    use std::sync::OnceLock;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::middleware::from_fn;
    use axum::routing::get;
    use metrics_exporter_prometheus::PrometheusHandle;
    use tower::ServiceExt;

    use crate::http::middleware::metrics::track_metrics;

    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

    fn get_handle() -> &'static PrometheusHandle {
        HANDLE
            .get_or_init(|| install_recorder().expect("recorder installs exactly once per process"))
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
