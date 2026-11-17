//! Wyrd-owned request body size limiter.
//!
//! Returns `WyrdError::PayloadTooLarge` rendered as `application/problem+json`
//! when the body exceeds `max_bytes`. Audio transcription and translation
//! uploads pass through unbuffered: their handlers stream them under
//! `limits.audio_upload_bytes`. Verification execute, enqueue, and evidence
//! requests and local storage uploads pass through too: their handlers read
//! under their own contract bound (a local upload under its declared size).
//! Every other body is buffered up to `max_bytes + 1` and charged before it is
//! buffered: Scribe OTLP bodies to Bifrost transport admission, the rest to the
//! process memory ledger. A body the owner cannot hold is refused with `503`,
//! and the charge returns when the handler does.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use http_body_util::{BodyExt, LengthLimitError, Limited};
use tower::{Layer, Service};
use wyrd_runtime::resources::{ByteCharge, ByteLedger};
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
    /// Process memory ledger every other buffered body is charged to.
    memory: ByteLedger,
}

impl WyrdBodyLimitLayer {
    /// Create a new body-limit layer.
    #[must_use]
    pub fn new(
        max_bytes: usize,
        scribe_max_bytes: Option<usize>,
        admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
        memory: ByteLedger,
    ) -> Self {
        Self {
            max_bytes,
            scribe_max_bytes,
            admission,
            memory,
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
            memory: self.memory.clone(),
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
    /// Process memory ledger every other buffered body is charged to.
    memory: ByteLedger,
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

    /// Apply the global request-body limit, or pass the request through
    /// unbounded to a route that owns its own bound.
    ///
    /// Audio, verification request, evidence-planning, and evidence
    /// publication routes, and the
    /// local upload route (`/v1/cards/upload/local/{id}`) bound their bodies
    /// in their handlers — the local upload against its upload's declared
    /// size — so this layer never caps them below the storage object limit.
    /// Every other body is capped, with OTLP routes using their own limit
    /// and admission.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let path = request.uri().path();
        if matches!(
            path,
            "/v1/audio/transcriptions"
                | "/v1/audio/translations"
                | "/v1/verification/execute"
                | "/v1/verification/enqueue"
                | "/v1/verification/evidence"
                | "/v1/verification/evidence/publish"
        ) || path.starts_with("/v1/cards/upload/local/")
        {
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
        let memory = self.memory.clone();
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
            let mut charge = match admission {
                Some(admission) => BodyCharge::Transport {
                    admission,
                    leases: Vec::new(),
                },
                None => BodyCharge::Memory(memory.empty_charge()),
            };
            let mut charged = declared_bytes.unwrap_or(0);
            if let Err(reason) = charge.try_add(charged) {
                return Ok(occupied(&reason));
            }

            // Collect body into memory, capping at max_bytes + 1 via Limited.
            // Using http_body_util::Limited directly lets us distinguish
            // length-limit failures from stream/IO failures by error type.
            let (parts, body) = request.into_parts();
            let mut limited = Limited::new(body, max_bytes + 1);
            let mut frames = Vec::new();
            let mut buffered = 0;
            let collected = loop {
                match limited.frame().await {
                    None => break Ok(join(frames)),
                    Some(Err(error)) => break Err(error),
                    Some(Ok(frame)) => {
                        if let Ok(data) = frame.into_data() {
                            buffered += data.len();
                            if buffered > charged {
                                if let Err(reason) = charge.try_add(buffered - charged) {
                                    return Ok(occupied(&reason));
                                }
                                charged = buffered;
                            }
                            frames.push(data);
                        }
                    }
                }
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
                    let mut request = Request::from_parts(parts, Body::from(bytes));
                    if let BodyCharge::Memory(charge) = charge {
                        request.extensions_mut().insert(BodyCustody::new(charge));
                        return inner.call(request).await.map_err(Into::into);
                    }
                    let response = inner.call(request).await.map_err(Into::into);
                    drop(charge);
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

/// The process memory charge of a buffered request body, offered to its
/// handler.
///
/// A handler whose work outlives its response, such as a streamed gateway
/// call, takes the charge so the body stays counted, once, until that work
/// settles; an untaken charge returns with the response.
#[derive(Clone, Debug)]
pub(crate) struct BodyCustody(Arc<Mutex<Option<ByteCharge>>>);

impl BodyCustody {
    /// Offers `charge` to the handler.
    pub(crate) fn new(charge: ByteCharge) -> Self {
        Self(Arc::new(Mutex::new(Some(charge))))
    }

    /// Takes the body's charge; `None` once taken.
    pub(crate) fn take(&self) -> Option<ByteCharge> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

/// The memory a buffered request body holds until its handler returns.
///
/// Scribe routes charge Bifrost transport admission, which draws on the
/// process memory ledger through the shared Bifrost cap; every other route
/// charges the process memory ledger directly. Dropping it returns every
/// byte, on success, error, or cancellation.
enum BodyCharge {
    /// Exact transport leases for a Scribe body.
    Transport {
        /// The shared transport admission owner.
        admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
        /// One lease per charged span of the body.
        leases: Vec<vala_bifrost_redux::gate::limits::BifrostTransportLease>,
    },
    /// One growing process memory charge for any other body.
    Memory(ByteCharge),
}

impl BodyCharge {
    /// Charge `bytes` more of the body before they are buffered.
    ///
    /// # Errors
    ///
    /// Returns the owner's refusal reason when it cannot hold them.
    fn try_add(&mut self, bytes: usize) -> Result<(), String> {
        if bytes == 0 {
            return Ok(());
        }
        match self {
            Self::Transport { admission, leases } => {
                leases.push(
                    admission
                        .try_acquire(bytes)
                        .map_err(|error| error.to_string())?,
                );
            }
            Self::Memory(charge) => charge
                .try_grow(u64::try_from(bytes).unwrap_or(u64::MAX))
                .map_err(|refusal| refusal.to_string())?,
        }
        Ok(())
    }
}

/// Joins buffered frames, passing a single frame on without a copy.
fn join(mut frames: Vec<Bytes>) -> Bytes {
    match frames.len() {
        0 => Bytes::new(),
        1 => frames.pop().unwrap_or_default(),
        _ => Bytes::from(frames.concat()),
    }
}

/// Maps a refused body charge to the typed occupied-capacity response.
fn occupied(reason: &str) -> Response {
    let error = WyrdError::ServiceUnavailable {
        message: "request body capacity is occupied".to_owned(),
        details: serde_json::json!({ "reason": reason }),
    };
    WyrdErrorResponse::from(error).into_response()
}

/// Convenience constructor for use in `Router::layer` composition.
pub fn wyrd_body_limit(
    max_bytes: usize,
    scribe_max_bytes: Option<usize>,
    admission: Option<vala_bifrost_redux::gate::limits::BifrostTransportAdmission>,
    memory: ByteLedger,
) -> WyrdBodyLimitLayer {
    WyrdBodyLimitLayer::new(max_bytes, scribe_max_bytes, admission, memory)
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

    /// A process memory ledger no test body can fill.
    fn unbounded() -> ByteLedger {
        ByteLedger::new(u64::MAX / 4)
    }

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
            unbounded(),
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
                unbounded(),
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
                unbounded(),
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

    /// A generic buffered body charges process memory while its handler
    /// runs and returns the charge with the response; a declared or
    /// undeclared body the ledger cannot hold is refused busy before the
    /// handler runs.
    ///
    /// # Panics
    ///
    /// Panics when a charge, refusal, or release differs from that.
    #[tokio::test]
    async fn generic_bodies_charge_process_memory() {
        let memory = ByteLedger::new(64);
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&observed);
        let ledger = memory.clone();
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(1024, None, None, memory.clone()))
            .service(service_fn(move |_request: Request<Body>| {
                seen.lock().expect("observations").push(ledger.used());
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let declared = |bytes: usize| {
            Request::builder()
                .uri("/v1/cards")
                .header(axum::http::header::CONTENT_LENGTH, bytes.to_string())
                .body(Body::from(vec![0_u8; bytes]))
                .expect("declared request")
        };
        let undeclared = |bytes: usize| {
            Request::builder()
                .version(axum::http::Version::HTTP_2)
                .uri("/v1/cards")
                .body(Body::from(vec![0_u8; bytes]))
                .expect("undeclared request")
        };

        for request in [declared(40), undeclared(40)] {
            let response = service.clone().oneshot(request).await.expect("response");
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(memory.used(), 0, "the response returns the charge");
        }
        assert_eq!(
            *observed.lock().expect("observations"),
            [40, 40],
            "the handler runs while the body is charged"
        );

        let occupant = memory.try_charge(30).expect("occupant");
        for request in [declared(40), undeclared(40)] {
            let response = service.clone().oneshot(request).await.expect("response");
            assert_eq!(
                response.status(),
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(memory.used(), 30, "a refused body charges nothing");
        }
        assert_eq!(observed.lock().expect("observations").len(), 2);
        drop(occupant);
    }

    /// A handler that takes its body's charge keeps it past the response,
    /// so the body stays counted, once, until the handler's work settles;
    /// an untaken charge returns with the response.
    ///
    /// # Panics
    ///
    /// Panics when the body is charged twice or released early or late.
    #[tokio::test]
    async fn a_taken_body_charge_outlives_the_response() {
        let memory = ByteLedger::new(64);
        let kept = Arc::new(Mutex::new(None));
        let keep = Arc::clone(&kept);
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(1024, None, None, memory.clone()))
            .service(service_fn(move |request: Request<Body>| {
                let custody = request
                    .extensions()
                    .get::<BodyCustody>()
                    .expect("the body limit offers the charge");
                *keep.lock().expect("kept") = custody.take();
                async { Ok::<_, Infallible>(Response::new(Body::empty())) }
            }));
        let request = Request::builder()
            .uri("/v1/chat/completions")
            .body(Body::from(vec![0_u8; 40]))
            .expect("request");

        let response = service.oneshot(request).await.expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            memory.used(),
            40,
            "the taken charge outlives the response, once"
        );
        drop(kept.lock().expect("kept").take());
        assert_eq!(
            memory.used(),
            0,
            "dropping the taken charge returns the body"
        );
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
                unbounded(),
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

    /// Audio transcription and translation uploads and verification execute,
    /// enqueue, and evidence requests reach their handler unbuffered past the
    /// general limit,
    /// which still refuses every other route, including Audio speech.
    ///
    /// # Panics
    ///
    /// Panics when a request is refused or admitted differently.
    #[tokio::test]
    async fn audio_uploads_pass_the_body_limit_unbuffered() {
        let service = ServiceBuilder::new()
            .layer(wyrd_body_limit(16, None, None, unbounded()))
            .service(service_fn(|_request: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::empty()))
            }));
        for (path, status) in [
            ("/v1/audio/transcriptions", axum::http::StatusCode::OK),
            ("/v1/audio/translations", axum::http::StatusCode::OK),
            ("/v1/verification/execute", axum::http::StatusCode::OK),
            ("/v1/verification/enqueue", axum::http::StatusCode::OK),
            ("/v1/verification/evidence", axum::http::StatusCode::OK),
            (
                "/v1/verification/evidence/publish",
                axum::http::StatusCode::OK,
            ),
            ("/v1/cards/upload/local/00", axum::http::StatusCode::OK),
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
