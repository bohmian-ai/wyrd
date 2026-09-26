//! Wyrd-owned request body size limiter.
//!
//! Returns `WyrdError::PayloadTooLarge` rendered as `application/problem+json`
//! when the body exceeds `max_bytes`. Audio transcription and translation
//! uploads pass through unbuffered: their handlers stream them under
//! `limits.audio_upload_bytes`. Uses bounded buffering: `to_bytes` reads
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
    /// Default maximum body bytes for non-Scribe HTTP routes.
    max_bytes: usize,
    /// Optional route-local maximum for enabled Scribe OTLP HTTP endpoints.
    scribe_max_bytes: Option<usize>,
    /// Process-root transport reservation owner used only by Scribe routes.
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
}

impl WyrdBodyLimitLayer {
    /// Create a new body-limit layer.
    #[must_use]
    pub fn new(
        max_bytes: usize,
        scribe_max_bytes: Option<usize>,
        admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
    ) -> Self {
        Self {
            max_bytes,
            scribe_max_bytes,
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
            scribe_max_bytes: self.scribe_max_bytes,
            admission: self.admission.clone(),
        }
    }
}

/// Tower service that enforces the body size limit.
#[derive(Clone)]
pub struct WyrdBodyLimitService<S> {
    /// Wrapped route service invoked after bounded admission succeeds.
    inner: S,
    /// Default maximum body bytes for non-Scribe HTTP routes.
    max_bytes: usize,
    /// Optional route-local maximum for enabled Scribe OTLP HTTP endpoints.
    scribe_max_bytes: Option<usize>,
    /// Process-root transport reservation owner used only by Scribe routes.
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
        if matches!(
            request.uri().path(),
            "/v1/audio/transcriptions" | "/v1/audio/translations"
        ) {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(request).await.map_err(Into::into) });
        }
        let scribe_route = matches!(
            request.uri().path(),
            "/v1/traces" | "/v1/metrics" | "/v1/logs"
        );
        let max_bytes = if scribe_route {
            self.scribe_max_bytes.unwrap_or(self.max_bytes)
        } else {
            self.max_bytes
        };
        let admission = scribe_route.then(|| self.admission.clone()).flatten();
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
    scribe_max_bytes: Option<usize>,
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
) -> WyrdBodyLimitLayer {
    WyrdBodyLimitLayer::new(max_bytes, scribe_max_bytes, admission)
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
        let admission = BifrostTransportAdmission::for_tests();
        let layer = wyrd_body_limit(
            1024,
            Some(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES),
            Some(admission.clone()),
        );
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
        let admission = BifrostTransportAdmission::for_tests();
        let first = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("first maximum message");
        let second = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("exact aggregate boundary");
        let invoked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&invoked);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(
                1024,
                Some(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES),
                Some(admission.clone()),
            ))
            .service(service_fn(move |_request: Request<Body>| {
                observed.store(true, Ordering::Release);
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let request = Request::builder()
            .version(axum::http::Version::HTTP_2)
            .uri("/v1/traces")
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

    /// Only the three mounted Scribe HTTP routes receive the selected encoded
    /// request limit; unrelated protected routes retain the general 413 floor.
    #[tokio::test]
    async fn scribe_body_limit_is_route_local() {
        let admission = BifrostTransportAdmission::for_tests();
        let invoked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&invoked);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(
                16,
                Some(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES),
                Some(admission),
            ))
            .service(service_fn(move |_request: Request<Body>| {
                observed.store(true, Ordering::Release);
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let scribe = Request::builder()
            .uri("/v1/traces")
            .header(axum::http::header::CONTENT_LENGTH, "17")
            .body(Body::from(vec![0_u8; 17]))
            .expect("Scribe request");
        assert_eq!(
            service
                .clone()
                .oneshot(scribe)
                .await
                .expect("response")
                .status(),
            axum::http::StatusCode::OK
        );
        let protected = Request::builder()
            .uri("/v1/cards")
            .header(axum::http::header::CONTENT_LENGTH, "17")
            .body(Body::empty())
            .expect("protected request");
        assert_eq!(
            service.oneshot(protected).await.expect("response").status(),
            axum::http::StatusCode::PAYLOAD_TOO_LARGE
        );
        assert!(invoked.load(Ordering::Acquire));
    }

    /// Audio transcription and translation uploads reach their handler
    /// unbuffered past the general limit, which still refuses every other
    /// route, including Audio speech.
    ///
    /// # Panics
    ///
    /// Panics when a request is refused or admitted differently.
    #[tokio::test]
    async fn audio_uploads_pass_the_body_limit_unbuffered() {
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(16, None, None))
            .service(service_fn(|_request: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::empty()))
            }));
        for (path, status) in [
            ("/v1/audio/transcriptions", axum::http::StatusCode::OK),
            ("/v1/audio/translations", axum::http::StatusCode::OK),
            (
                "/v1/audio/speech",
                axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            let request = Request::builder()
                .uri(path)
                .header(axum::http::header::CONTENT_LENGTH, "17")
                .body(Body::from(vec![0_u8; 17]))
                .expect("request");
            let response = service.clone().oneshot(request).await.expect("response");
            assert_eq!(response.status(), status, "{path}");
        }
    }
}
