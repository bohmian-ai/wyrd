//! Per-request metrics middleware feeding the Prometheus recorder.

use std::time::Instant;

use axum::{extract::MatchedPath, extract::Request, middleware::Next, response::IntoResponse};
use metrics::{counter, histogram};

use crate::app::metrics::{HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL};

/// Bounded fallback label used when no route matched (404s, pre-routing
/// rejections). Never label with the raw URI — an attacker or scanner hitting
/// random paths would otherwise blow up Prometheus label cardinality.
const UNMATCHED_PATH: &str = "__unmatched__";

/// Record request count and latency labelled by method, matched route, status.
///
/// Uses `MatchedPath` (the route template, e.g. `/v1/storage/{id}`) rather than
/// the raw URI so label cardinality stays bounded. When no route matched, uses
/// the fixed `__unmatched__` label — NOT the raw URI path — so unmatched/404
/// traffic cannot create unbounded distinct label values.
pub async fn track_metrics(req: Request, next: Next) -> impl IntoResponse {
    let start = Instant::now();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned())
        .unwrap_or_else(|| UNMATCHED_PATH.to_owned());
    let method = req.method().clone();

    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();
    let labels = [
        ("method", method.to_string()),
        ("path", path),
        ("status", status),
    ];
    counter!(HTTP_REQUESTS_TOTAL, &labels).increment(1);
    histogram!(HTTP_REQUEST_DURATION_SECONDS, &labels).record(latency);

    response
}
