//! Wyrd-owned request body size limiter.
//!
//! Returns `WyrdError::PayloadTooLarge` rendered as `application/problem+json`
//! when the body exceeds `max_bytes`. Uses bounded buffering: `to_bytes` reads
//! the full body into memory up to `max_bytes + 1`; the worst-case process
//! footprint is `max_bytes * concurrency`.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use tower::{Layer, Service};
use wyrd_spec::error::WyrdError;

use crate::http::error::WyrdErrorResponse;

/// Tower layer that wraps a service with the Wyrd-owned body size limiter.
#[derive(Clone)]
pub struct WyrdBodyLimitLayer {
    max_bytes: usize,
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
}

impl WyrdBodyLimitLayer {
    /// Create a new body-limit layer.
    #[must_use]
    pub fn new(
        max_bytes: usize,
        admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
    ) -> Self {
        Self {
            max_bytes,
            admission,
        }
    }
}

impl<S> Layer<S> for WyrdBodyLimitLayer {
    type Service = WyrdBodyLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        WyrdBodyLimitService {
            inner,
            max_bytes: self.max_bytes,
            admission: self.admission.clone(),
        }
    }
}

/// Tower service that enforces the body size limit.
#[derive(Clone)]
pub struct WyrdBodyLimitService<S> {
    inner: S,
    max_bytes: usize,
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
}

type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

impl<S> Service<Request<Body>> for WyrdBodyLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<std::convert::Infallible>,
{
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = BoxFut<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let max_bytes = self.max_bytes;
        let admission = self.admission.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let declared_bytes = request
                .headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<usize>().ok());
            // Fast path: Content-Length header declares a size that exceeds the limit.
            if let Some(content_length) = request.headers().get(axum::http::header::CONTENT_LENGTH)
                && let Ok(s) = content_length.to_str()
                && let Ok(len) = s.parse::<usize>()
                && len > max_bytes
            {
                let error = WyrdError::PayloadTooLarge {
                    message: format!("request body exceeds the {max_bytes}-byte limit"),
                    details: serde_json::json!({
                        "max_bytes": max_bytes,
                        "declared_bytes": len,
                    }),
                };
                return Ok(WyrdErrorResponse::from(error).into_response());
            }

            let _transport_lease = if let Some(admission) = admission {
                let acquired = declared_bytes.map_or_else(
                    || admission.try_acquire_unknown(),
                    |bytes| admission.try_acquire(bytes),
                );
                match acquired {
                    Ok(lease) => Some(lease),
                    Err(error) => {
                        let error = WyrdError::ServiceUnavailable {
                            message: "request body capacity is occupied".to_owned(),
                            details: serde_json::json!({ "reason": error.to_string() }),
                        };
                        return Ok(WyrdErrorResponse::from(error).into_response());
                    }
                }
            } else {
                None
            };

            // Collect body into memory, capping at max_bytes + 1 via Limited.
            // Using http_body_util::Limited directly lets us distinguish
            // length-limit failures from stream/IO failures by error type.
            let (parts, body) = request.into_parts();
            let limited = Limited::new(body, max_bytes + 1);
            match limited.collect().await {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    if bytes.len() > max_bytes {
                        let error = WyrdError::PayloadTooLarge {
                            message: format!("request body exceeds the {max_bytes}-byte limit"),
                            details: serde_json::json!({
                                "max_bytes": max_bytes,
                            }),
                        };
                        return Ok(WyrdErrorResponse::from(error).into_response());
                    }
                    let request = Request::from_parts(parts, Body::from(bytes));
                    inner.call(request).await.map_err(Into::into)
                }
                Err(error) if error.is::<LengthLimitError>() => {
                    let error = WyrdError::PayloadTooLarge {
                        message: format!("request body exceeds the {max_bytes}-byte limit"),
                        details: serde_json::json!({
                            "max_bytes": max_bytes,
                        }),
                    };
                    Ok(WyrdErrorResponse::from(error).into_response())
                }
                Err(error) => {
                    tracing::debug!(
                        error = ?error,
                        "failed to read request body (stream/io error)"
                    );
                    let error = WyrdError::ServiceUnavailable {
                        message: "failed to read request body".to_owned(),
                        details: serde_json::json!({}),
                    };
                    Ok(WyrdErrorResponse::from(error).into_response())
                }
            }
        })
    }
}

/// Convenience constructor for use in `Router::layer` composition.
pub fn wyrd_body_limit(
    max_bytes: usize,
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
) -> WyrdBodyLimitLayer {
    WyrdBodyLimitLayer::new(max_bytes, admission)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tower::{ServiceBuilder, ServiceExt, service_fn};
    use vala_bifrost_redux::gate::limits::{
        BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES, BifrostTransportAdmission,
    };

    /// Server edge and tonic peers can share the same exact transport owner.
    ///
    /// # Panics
    ///
    /// Panics when exact-boundary admission unexpectedly fails.
    #[test]
    fn bifrost_transport_admission_is_process_wide_at_server_edge() {
        let admission = BifrostTransportAdmission::default();
        let layer = wyrd_body_limit(1024, Some(admission.clone()));
        assert_eq!(layer.max_bytes, 1024);
        let first = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("first maximum message");
        let second = admission
            .try_acquire_unknown()
            .expect("unknown HTTP body reaches exact aggregate boundary");
        assert!(admission.try_acquire(1).is_err());
        drop((first, second));
        assert_eq!(admission.used_bytes(), 0);
    }

    /// An HTTP/2-style body without a length reserves pessimistically before collection.
    ///
    /// # Panics
    ///
    /// Panics when the in-memory request service unexpectedly errors.
    #[tokio::test]
    async fn bifrost_transport_admission_pessimistically_rejects_unknown_http2_body() {
        let admission = BifrostTransportAdmission::default();
        let first = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("first maximum message");
        let second = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("exact aggregate boundary");
        let invoked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&invoked);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(1024, Some(admission.clone())))
            .service(service_fn(move |_request: Request<Body>| {
                observed.store(true, Ordering::Release);
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let request = Request::builder()
            .version(axum::http::Version::HTTP_2)
            .body(Body::from("unknown-length"))
            .expect("HTTP/2 request");
        let response = service.oneshot(request).await.expect("infallible service");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(!invoked.load(Ordering::Acquire));
        assert_eq!(
            admission.used_bytes(),
            2 * BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES
        );
        drop((first, second));
    }
}
