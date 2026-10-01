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
use bytes::Bytes;
use http_body_util::{BodyExt, Collected, LengthLimitError, Limited};
use tower::{Layer, Service};
use vala_bifrost_redux::resources::BifrostResourceError;
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

            // A declared length is charged before a byte is buffered; an
            // undeclared body is charged frame by frame as it is buffered.
            let transport_lease = match (&admission, declared_bytes) {
                (Some(admission), Some(bytes)) => match admission.try_acquire(bytes) {
                    Ok(lease) => Some(lease),
                    Err(error) => return Ok(transport_occupied(&error)),
                },
                _ => None,
            };

            // Collect body into memory, capping at max_bytes + 1 via Limited.
            // Using http_body_util::Limited directly lets us distinguish
            // length-limit failures from stream/IO failures by error type.
            let (parts, body) = request.into_parts();
            let mut limited = Limited::new(body, max_bytes + 1);
            let mut frame_leases = Vec::new();
            let collected = match (&admission, declared_bytes) {
                (Some(admission), None) => {
                    let mut buffered = Vec::new();
                    loop {
                        match limited.frame().await {
                            None => break Ok(Bytes::from(buffered)),
                            Some(Err(error)) => break Err(error),
                            Some(Ok(frame)) => {
                                if let Some(data) = frame.data_ref() {
                                    match admission.try_acquire(data.len()) {
                                        Ok(lease) => frame_leases.push(lease),
                                        Err(error) => return Ok(transport_occupied(&error)),
                                    }
                                    buffered.extend_from_slice(data);
                                }
                            }
                        }
                    }
                }
                _ => limited.collect().await.map(Collected::to_bytes),
            };
            match collected {
                Ok(bytes) => {
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
                    let response = inner.call(request).await.map_err(Into::into);
                    drop((transport_lease, frame_leases));
                    response
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

/// Maps a refused transport charge to the typed occupied-capacity response.
fn transport_occupied(error: &BifrostResourceError) -> Response {
    let error = WyrdError::ServiceUnavailable {
        message: "request body capacity is occupied".to_owned(),
        details: serde_json::json!({ "reason": error.to_string() }),
    };
    WyrdErrorResponse::from(error).into_response()
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
        BIFROST_INGEST_REQUEST_LIMIT_BYTES, BifrostTransportAdmission,
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
            Some(BIFROST_INGEST_REQUEST_LIMIT_BYTES),
            Some(admission.clone()),
        );
        assert_eq!(layer.max_bytes, 1024);
        let first = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("first maximum message");
        let second = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("second maximum message reaches the shared cap");
        assert!(admission.try_acquire(1).is_err());
        drop((first, second));
        assert_eq!(admission.used_bytes(), 0);
    }

    /// An HTTP/2-style body without a length is charged as it is buffered,
    /// and a full shared cap refuses it before the handler runs.
    ///
    /// # Panics
    ///
    /// Panics when the in-memory request service unexpectedly errors.
    #[tokio::test]
    async fn bifrost_transport_admission_charges_collected_unknown_http2_body() {
        let admission = BifrostTransportAdmission::for_tests();
        let first = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("first maximum message");
        let second = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("exact aggregate boundary");
        let invoked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&invoked);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(
                1024,
                Some(BIFROST_INGEST_REQUEST_LIMIT_BYTES),
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
            2 * BIFROST_INGEST_REQUEST_LIMIT_BYTES
        );
        drop((first, second));
    }

    /// An undeclared-length Scribe body holds a governed charge for every
    /// buffered frame while it is still arriving, a nearly full root refuses
    /// it before buffering past the remaining capacity, and dropping the
    /// in-flight request returns every byte.
    ///
    /// # Panics
    ///
    /// Panics when the charge, refusal, or release differs from that.
    #[tokio::test]
    async fn undeclared_body_is_charged_while_collected_and_released_on_drop() {
        let admission = BifrostTransportAdmission::for_tests();
        let invoked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&invoked);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(
                1024,
                Some(BIFROST_INGEST_REQUEST_LIMIT_BYTES),
                Some(admission.clone()),
            ))
            .service(service_fn(move |_request: Request<Body>| {
                observed.store(true, Ordering::Release);
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let streamed = |rx| {
            Request::builder()
                .version(axum::http::Version::HTTP_2)
                .uri("/v1/traces")
                .body(Body::from_stream(
                    tokio_stream::wrappers::ReceiverStream::new(rx),
                ))
                .expect("HTTP/2 request")
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(1);
        let in_flight = tokio::spawn(service.clone().oneshot(streamed(rx)));
        tx.send(Ok(Bytes::from(vec![0_u8; 100])))
            .await
            .expect("body chunk");
        while admission.used_bytes() == 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(admission.used_bytes(), 100, "the buffered chunk is charged");
        in_flight.abort();
        assert!(in_flight.await.is_err(), "the request was dropped mid-body");
        assert_eq!(
            admission.used_bytes(),
            0,
            "a dropped request returns its charge"
        );

        let headroom = 10;
        let occupants = (
            admission
                .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
                .expect("first occupant"),
            admission
                .try_acquire(
                    admission.limit_bytes() - BIFROST_INGEST_REQUEST_LIMIT_BYTES - headroom,
                )
                .expect("second occupant fills all but the headroom"),
        );
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(1);
        tx.send(Ok(Bytes::from(vec![0_u8; 100])))
            .await
            .expect("body chunk");
        drop(tx);
        let response = service
            .oneshot(streamed(rx))
            .await
            .expect("infallible service");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(!invoked.load(Ordering::Acquire), "the handler never ran");
        assert_eq!(admission.used_bytes(), admission.limit_bytes() - headroom);
        drop(occupants);
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
                Some(BIFROST_INGEST_REQUEST_LIMIT_BYTES),
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
