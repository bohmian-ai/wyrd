//! gRPC scaffold re-exports plus the application-level ingest mount.
//!
//! Boot wiring imports via `wyrd_server::grpc::*`. Most symbols here are a
//! `pub use` that forwards to the owning `wyrd_tonic` crate. The one body this
//! module owns is [`build_app_grpc`]: the single code path that mounts the C1
//! ingest service, shared by `main.rs` and the `wyrd-testing` harness.
pub use wyrd_tonic::error;
pub use wyrd_tonic::health::WyrdHealthSentinel;
pub use wyrd_tonic::server::*;

mod otlp;
#[cfg(debug_assertions)]
#[doc(hidden)]
pub use otlp::{OtlpCodecActivity, reset_otlp_codec_activity, snapshot_otlp_codec_activity};
pub(crate) mod query;
mod scribe_tail;

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::StreamExt;
use http_body_util::{BodyExt, BodyStream, StreamBody};
use tower::Service;
use wyrd_tonic::tonic::Status;
use wyrd_tonic::tonic::body::Body;
use wyrd_tonic::tonic::codegen::http::{Request, Response};
use wyrd_tonic::tonic::server::NamedService;
use wyrd_tonic::tonic::transport::server::Router as TonicRouter;
use wyrd_tonic::tonic_health::pb::health_server::{Health, HealthServer};

use crate::AppState;

/// Encoded bytes occupied by the gRPC compression flag and big-endian length.
const GRPC_FRAME_HEADER_BYTES: usize = 5;

/// Reads the actual first gRPC envelope without decoding its protobuf payload.
fn grpc_frame_message_bytes(data: &[u8]) -> Option<usize> {
    let length = data.get(1..GRPC_FRAME_HEADER_BYTES)?.try_into().ok()?;
    usize::try_from(u32::from_be_bytes(length)).ok()
}

/// Service wrapper that acquires encoded gRPC body capacity before decoding.
#[derive(Clone)]
struct GrpcTransportAdmissionService<S> {
    /// Generated tonic service invoked only after transport admission.
    inner: S,
    /// Process-wide byte-weighted body owner shared with HTTP.
    admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
}

impl<S> GrpcTransportAdmissionService<S> {
    /// Wraps one generated service with the process transport owner.
    fn new(
        inner: S,
        admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    ) -> Self {
        Self { inner, admission }
    }
}

impl<S> NamedService for GrpcTransportAdmissionService<S>
where
    S: NamedService,
{
    const NAME: &'static str = S::NAME;
}

impl<S> Service<Request<Body>> for GrpcTransportAdmissionService<S>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let admission = self.admission.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let (parts, mut body) = request.into_parts();
            let first = match body.frame().await.transpose() {
                Ok(first) => first,
                Err(error) => return Ok(error.into_http()),
            };
            let declared = first
                .as_ref()
                .and_then(|frame| frame.data_ref())
                .and_then(|data| grpc_frame_message_bytes(data));
            let lease = declared.map_or_else(
                || admission.try_acquire_unknown(),
                |bytes| admission.try_acquire(bytes),
            );
            let _lease = match lease {
                Ok(lease) => lease,
                Err(_) => {
                    return Ok(Status::resource_exhausted(
                        "gRPC encoded-body capacity is occupied",
                    )
                    .into_http());
                }
            };
            let first = futures_util::stream::iter(first.into_iter().map(Ok));
            let body = Body::new(StreamBody::new(first.chain(BodyStream::new(body))));
            inner.call(Request::from_parts(parts, body)).await
        })
    }
}

/// Build the application gRPC router: health (unauthenticated) plus the C1
/// ingest service with auth completed in the handler.
///
/// A role-absent topology returns the health-only router and does not mount
/// Scribe ingest, OTLP, query, or tail services. Fallible: ingest is never mounted unauthenticated, so a `None`
/// `AppState.token_verifier` is a hard [`GrpcError::MissingTokenVerifier`]. The
/// verifier is the same `TokenVerifier` the HTTP `AuthenticatedPrincipal`
/// extractor uses; `build_grpc_router` stays unchanged (health only,
/// `NoopInterceptor`, no `.layer`) and ingest is attached via `add_service`.
///
/// # Errors
/// Returns [`GrpcError::MissingTokenVerifier`] when no token verifier is
/// configured, [`GrpcError::MissingScribe`] when only one of the paired Scribe
/// gate/resource capabilities is present, or a router-assembly error from
/// [`build_grpc_router`].
pub fn build_app_grpc<H>(
    state: &AppState,
    health_service: HealthServer<H>,
    cfg: GrpcRouterConfig,
) -> Result<TonicRouter, GrpcError>
where
    H: Health,
{
    if state.auth.token_verifier.is_none() {
        return Err(GrpcError::MissingTokenVerifier);
    }
    let router = build_grpc_router(health_service, NoopInterceptor, cfg)?;
    if state.bifrost_gate.is_none() && state.bifrost_resources.is_none() {
        return Ok(router);
    }
    let ingest = state
        .bifrost_gate
        .as_ref()
        .ok_or(GrpcError::MissingScribe)?;
    let traces = otlp::TraceOtlpGrpcService::new(Arc::clone(ingest));
    let metrics = otlp::MetricsOtlpGrpcService::new(Arc::clone(ingest));
    let logs = otlp::LogsOtlpGrpcService::new(Arc::clone(ingest));
    let query = crate::vala_query::grpc::ValaQueryGrpc::new(state.clone());
    let bifrost_query = query::BifrostQueryGrpc::new(state.clone());
    let transport = state
        .bifrost_resources
        .as_ref()
        .map(vala_bifrost_redux::resources::BifrostRoleResources::transport_admission)
        .ok_or(GrpcError::MissingScribe)?;
    let router = router
        .add_service(GrpcTransportAdmissionService::new(
            (**ingest).clone().into_server(),
            transport.clone(),
        ))
        .add_service(GrpcTransportAdmissionService::new(
            traces,
            transport.clone(),
        ))
        .add_service(GrpcTransportAdmissionService::new(
            metrics,
            transport.clone(),
        ))
        .add_service(GrpcTransportAdmissionService::new(logs, transport.clone()))
        .add_service(GrpcTransportAdmissionService::new(
            query.into_server(),
            transport.clone(),
        ))
        .add_service(GrpcTransportAdmissionService::new(
            bifrost_query.into_server(),
            transport.clone(),
        ));
    let router = if let Some(peer) = state.oracle_peer.as_ref() {
        let scribe = peer.scribe();
        router.add_service(GrpcTransportAdmissionService::new(
            match scribe.tail_authority() {
                Some(authority) => scribe_tail::ScribeTailGrpc::new_with_authority(
                    state.clone(),
                    scribe.tail_reader(),
                    authority,
                )
                .into_server(),
                None => scribe_tail::ScribeTailGrpc::new(state.clone(), scribe.tail_reader())
                    .into_server(),
            },
            transport.clone(),
        ))
    } else {
        router
    };
    let router = if let Some(peer) = state.oracle_peer.as_ref() {
        let oracle = peer.oracle();
        router.add_service(GrpcTransportAdmissionService::new(
            crate::oracle::OraclePeerGrpc::new(
                state.clone(),
                oracle.worker(),
                oracle.cluster(),
                oracle.security_audit(),
                peer.scribe().tail_service(),
            )
            .into_server(),
            transport.clone(),
        ))
    } else {
        router
    };
    let router = if let Some(query) = state.bifrost_query() {
        router.add_service(GrpcTransportAdmissionService::new(
            crate::oracle::OracleLifecycleGrpc::new(state.clone(), Arc::clone(query)).into_server(),
            transport,
        ))
    } else {
        router
    };
    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use http_body_util::Full;
    use tower::ServiceExt;
    use wyrd_tonic::tonic::codegen::Bytes;

    use vala_bifrost_redux::gate::limits::{
        BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES, BifrostTransportAdmission,
    };

    /// Mounted peer construction receives the exact retained follower source.
    #[tokio::test]
    async fn mounted_peer_receives_retained_follower_tail_source() {
        let (mut state, _) = crate::oracle::pg_tests::real_api_serving_state(
            crate::config::ForgeProcessRole::Server,
        )
        .await;
        let peer = state.oracle_peer.as_ref().expect("combined peer");
        let oracle = peer.oracle();
        let mounted = crate::oracle::OraclePeerGrpc::new(
            state.clone(),
            oracle.worker(),
            oracle.cluster(),
            oracle.security_audit(),
            peer.scribe().tail_service(),
        );
        assert!(Arc::ptr_eq(
            mounted.tail_service(),
            &peer.scribe().tail_service(),
        ));
        state
            .bifrost_query
            .take()
            .expect("query runtime")
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
            .await;
    }

    /// Minimal tonic-shaped service recording admission state before body decode.
    #[derive(Clone)]
    struct AdmissionProbe {
        /// Whether the generated-service boundary was invoked.
        invoked: Arc<AtomicBool>,
        /// Live transport bytes observed at the boundary.
        observed_bytes: Arc<AtomicUsize>,
        /// Shared transport owner inspected at invocation.
        admission: BifrostTransportAdmission,
    }

    impl NamedService for AdmissionProbe {
        const NAME: &'static str = "wyrd.test.AdmissionProbe";
    }

    impl Service<Request<Body>> for AdmissionProbe {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

        /// The probe is always ready because it performs no I/O.
        fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        /// Records that frame admission preceded the generated-service boundary.
        fn call(&mut self, _request: Request<Body>) -> Self::Future {
            self.invoked.store(true, Ordering::Release);
            self.observed_bytes
                .store(self.admission.used_bytes(), Ordering::Release);
            std::future::ready(Ok(Response::new(Body::empty())))
        }
    }

    /// Builds one encoded gRPC envelope declaring `message_bytes`.
    ///
    /// # Panics
    ///
    /// Panics when a test requests a length outside the gRPC `u32` envelope.
    fn framed_body(message_bytes: usize) -> Body {
        let length =
            u32::try_from(message_bytes).expect("test frame length fits the gRPC u32 envelope");
        let mut header = vec![0_u8];
        header.extend_from_slice(&length.to_be_bytes());
        Body::new(Full::new(Bytes::from(header)))
    }

    /// Actual gRPC frame length admits the exact cap and refuses one byte over.
    ///
    /// # Panics
    ///
    /// Panics when the in-memory service unexpectedly errors or assertions fail.
    #[tokio::test]
    async fn bifrost_transport_admission_precedes_tonic_decode_at_frame_boundary() {
        let admission = BifrostTransportAdmission::default();
        let invoked = Arc::new(AtomicBool::new(false));
        let observed_bytes = Arc::new(AtomicUsize::new(0));
        let probe = AdmissionProbe {
            invoked: Arc::clone(&invoked),
            observed_bytes: Arc::clone(&observed_bytes),
            admission: admission.clone(),
        };
        let service = GrpcTransportAdmissionService::new(probe.clone(), admission.clone());
        let request = Request::new(framed_body(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES));
        service
            .oneshot(request)
            .await
            .expect("infallible exact-boundary request");
        assert!(invoked.load(Ordering::Acquire));
        assert_eq!(
            observed_bytes.load(Ordering::Acquire),
            BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES
        );
        assert_eq!(admission.used_bytes(), 0);

        invoked.store(false, Ordering::Release);
        let service = GrpcTransportAdmissionService::new(probe, admission.clone());
        let request = Request::new(framed_body(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES + 1));
        let response = service
            .oneshot(request)
            .await
            .expect("infallible refusal response");
        assert_eq!(
            response
                .headers()
                .get("grpc-status")
                .and_then(|value| value.to_str().ok()),
            Some("8")
        );
        assert!(!invoked.load(Ordering::Acquire));
        assert_eq!(admission.used_bytes(), 0);
    }
}
