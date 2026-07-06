//! Integration tests for the Prometheus metrics recorder and middleware.
//!
//! Process-global recorder invariant: `install_recorder()` may only be called
//! once per process. All recorder-installing tests live in this file so they
//! share one binary, one process, one recorder. `HANDLE` initialises the
//! recorder exactly once via `OnceLock`.

use std::sync::OnceLock;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::middleware::from_fn;
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;
use tower::ServiceExt;
use wyrd_server::app::metrics::{
    HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL, install_recorder, metrics_router,
};
use wyrd_server::http::middleware::metrics::track_metrics;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

fn get_handle() -> &'static PrometheusHandle {
    HANDLE.get_or_init(|| install_recorder().expect("recorder installs exactly once per process"))
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
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .expect("metrics router responds");

    assert_eq!(response.status(), StatusCode::OK, "GET /metrics must return 200");
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
        .oneshot(Request::builder().uri("/probe").body(Body::empty()).unwrap())
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
        .oneshot(Request::builder().uri(raw_path).body(Body::empty()).unwrap())
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
