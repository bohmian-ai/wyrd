//! gRPC scaffold re-exports plus the application-level ingest mount.
//!
//! Boot wiring imports via `wyrd_server::grpc::*`. Most symbols here are a
//! `pub use` that forwards to the owning `wyrd_tonic` crate. The one body this
//! module owns is [`build_app_grpc`]: the single code path that mounts the
//! Gate-served ingest service, shared by `main.rs` and the `wyrd-testing`
//! harness. The ingest service itself is `vala_bifrost_redux::gate::Gate`; this
//! module binds the socket and wraps it in transport admission, nothing more.
pub use wyrd_tonic::error;
pub use wyrd_tonic::health::WyrdHealthSentinel;
pub use wyrd_tonic::server::*;

mod otlp;
#[cfg(debug_assertions)]
#[doc(hidden)]
pub use otlp::{OtlpCodecActivity, reset_otlp_codec_activity, snapshot_otlp_codec_activity};
mod peer_auth;
pub(crate) mod query;
mod scribe_tail;

pub use peer_auth::{PeerWorkloadAuth, PeerWorkloadAuthLayer, PeerWorkloadIdentity};

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

/// The head of a gRPC request body, read far enough to bound its first message.
///
/// A gRPC length-prefixed message is five header bytes followed by the declared
/// payload, but HTTP/2 frames do not align to that boundary: a header can be
/// split across two DATA frames, and several messages can be coalesced into
/// one. This owner reads only as far as the first complete header, retains
/// every byte it consumed, and replays all of them ahead of the untouched
/// remainder — so the bound applies to the first message alone while later
/// coalesced messages reach the codec intact.
struct GrpcFirstFrame {
    /// Frames already taken from the body, replayed to the inner service in order.
    buffered: Vec<http_body::Frame<wyrd_tonic::tonic::codegen::Bytes>>,
    /// Header bytes observed so far.
    header: [u8; GRPC_FRAME_HEADER_BYTES],
    /// How many of `header` are filled.
    filled: usize,
}

/// Why a request body's first frame cannot be admitted.
///
/// Distinguished from a transport error because the body is well formed at the
/// HTTP layer and malformed at the gRPC layer; the caller answers with a
/// status rather than dropping the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstFrameError {
    /// The compression flag is neither 0 nor 1.
    MalformedFlag,
}

impl GrpcFirstFrame {
    /// Creates an empty reader.
    const fn new() -> Self {
        Self {
            buffered: Vec::new(),
            header: [0; GRPC_FRAME_HEADER_BYTES],
            filled: 0,
        }
    }

    /// Reads `body` until the first message header is complete or it ends.
    ///
    /// Stops at the header, never at the payload: a large first message is
    /// bounded by its declared length, not by buffering it.
    ///
    /// # Errors
    ///
    /// Returns the body's own transport error unchanged. A malformed
    /// compression flag is reported through [`Self::declared`] instead, so the
    /// consumed bytes are still available to the caller for a clean refusal.
    async fn read(body: &mut Body) -> Result<Self, Status> {
        let mut reader = Self::new();
        while reader.filled < GRPC_FRAME_HEADER_BYTES {
            let Some(frame) = body.frame().await.transpose()? else {
                break;
            };
            if let Some(data) = frame.data_ref() {
                let wanted = GRPC_FRAME_HEADER_BYTES - reader.filled;
                let taken = wanted.min(data.len());
                reader.header[reader.filled..reader.filled + taken].copy_from_slice(&data[..taken]);
                reader.filled += taken;
            }
            reader.buffered.push(frame);
        }
        Ok(reader)
    }

    /// Returns the first message's declared payload size, when it is knowable.
    ///
    /// `None` means the body ended before a complete header, which is a legal
    /// empty body, or the message is compressed. A compressed frame declares
    /// its *compressed* length, which would under-bound the decompressed body,
    /// so it is treated as unknown rather than admitted against a lease that is
    /// too small.
    ///
    /// # Errors
    ///
    /// Returns [`FirstFrameError::MalformedFlag`] when the compression flag is
    /// neither 0 nor 1, which no conforming gRPC client emits.
    fn declared(&self) -> Result<Option<usize>, FirstFrameError> {
        if self.filled < GRPC_FRAME_HEADER_BYTES {
            return Ok(None);
        }
        match self.header[0] {
            0 => {}
            1 => return Ok(None),
            _ => return Err(FirstFrameError::MalformedFlag),
        }
        let length: [u8; 4] = self.header[1..GRPC_FRAME_HEADER_BYTES]
            .try_into()
            .unwrap_or([0; 4]);
        Ok(usize::try_from(u32::from_be_bytes(length)).ok())
    }

    /// Rebuilds one body from the consumed frames followed by the remainder.
    fn replay(self, rest: Body) -> Body {
        let buffered = futures_util::stream::iter(self.buffered.into_iter().map(Ok));
        Body::new(StreamBody::new(buffered.chain(BodyStream::new(rest))))
    }
}

/// Which trust plane a transport-admission wrapper serves.
///
/// The two planes share one byte-weighted owner but not one boundary: only the
/// private plane sits behind peer workload authentication, so only its first
/// body poll is evidence that authentication already ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportPlane {
    /// The public client-facing listener.
    Public,
    /// The private mutually authenticated Bifrost peer listener.
    Peer,
}

impl TransportPlane {
    /// Records one private-plane body poll for multi-process probes.
    ///
    /// The counter exists only in test-support builds; a production binary
    /// keeps this a no-op so the private path pays nothing for the evidence.
    fn record_body_poll(self) {
        #[cfg(feature = "test-support")]
        if self == Self::Peer {
            PEER_BODY_POLLS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

/// Request bodies the private peer plane has polled since process start.
#[cfg(feature = "test-support")]
static PEER_BODY_POLLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Reads how many request bodies this process's peer plane has polled.
///
/// The private plane authenticates before it touches a body, so a refused peer
/// request must leave this unchanged while an admitted one advances it. A
/// multi-process journey reads it to prove that ordering from outside the
/// process rather than by inspecting the layer stack.
#[cfg(feature = "test-support")]
#[must_use]
pub fn peer_body_polls() -> u64 {
    PEER_BODY_POLLS.load(std::sync::atomic::Ordering::Acquire)
}

/// Service wrapper that acquires encoded gRPC body capacity before decoding.
#[derive(Clone)]
struct GrpcTransportAdmissionService<S> {
    /// Generated tonic service invoked only after transport admission.
    inner: S,
    /// Process-wide byte-weighted body owner shared with HTTP.
    admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    /// Trust plane this wrapper serves.
    plane: TransportPlane,
}

impl<S> GrpcTransportAdmissionService<S> {
    /// Wraps one public-plane service with the process transport owner.
    fn new(
        inner: S,
        admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    ) -> Self {
        Self {
            inner,
            admission,
            plane: TransportPlane::Public,
        }
    }

    /// Wraps one private peer-plane service with the process transport owner.
    fn new_peer(
        inner: S,
        admission: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    ) -> Self {
        Self {
            inner,
            admission,
            plane: TransportPlane::Peer,
        }
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
        let plane = self.plane;
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let (parts, mut body) = request.into_parts();
            plane.record_body_poll();
            let head = match GrpcFirstFrame::read(&mut body).await {
                Ok(head) => head,
                Err(error) => return Ok(error.into_http()),
            };
            let declared = match head.declared() {
                Ok(declared) => declared,
                // Refused with the consumed bytes still in hand and never
                // forwarded, so a malformed frame reaches no codec.
                Err(FirstFrameError::MalformedFlag) => {
                    return Ok(
                        Status::invalid_argument("gRPC message framing is malformed").into_http(),
                    );
                }
            };
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
            inner
                .call(Request::from_parts(parts, head.replay(body)))
                .await
        })
    }
}

/// Build the **public** application gRPC router.
///
/// The public listener carries client-facing traffic only: health, optional
/// reflection, Scribe ingest, OTLP, and the Vala/Bifrost query services. Every
/// private peer service lives on the separate peer router built by
/// [`build_peer_grpc`], so a peer RPC is unreachable on this listener even with
/// a valid client token.
///
/// A role-absent topology returns the health-only router and does not mount
/// Scribe ingest, OTLP, or query services. Fallible: ingest is never mounted
/// unauthenticated, so a `None` `AppState.token_verifier` is a hard
/// [`GrpcError::MissingTokenVerifier`].
///
/// # Errors
/// Returns [`GrpcError::MissingTokenVerifier`] when no token verifier is
/// configured, or a router-assembly error from [`build_grpc_router`].
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
    if !state.bifrost.serves_api() {
        return Ok(router);
    }
    let bifrost = Arc::clone(&state.bifrost);
    let traces = otlp::TraceOtlpGrpcService::new(Arc::clone(&bifrost));
    let metrics = otlp::MetricsOtlpGrpcService::new(Arc::clone(&bifrost));
    let logs = otlp::LogsOtlpGrpcService::new(Arc::clone(&bifrost));
    let query = crate::vala_query::grpc::ValaQueryGrpc::new(state.clone());
    let bifrost_query = query::BifrostQueryGrpc::new(state.clone());
    let transport = state.bifrost.transport_admission();
    Ok(router
        .add_service(GrpcTransportAdmissionService::new(
            state.bifrost.gate().clone().into_server(),
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
            transport,
        )))
}

/// Selects the role-owned peer security audit for this process.
///
/// Deliberately not an aggregate: a process owns exactly one peer plane, and
/// whichever role composed it owns the record of what that plane refused.
fn peer_security_audit(
    state: &AppState,
) -> Option<Arc<dyn vala_bifrost_redux::oracle::peer::PeerSecurityAudit>> {
    if let Some(oracle) = state.bifrost.oracle() {
        return Some(oracle.peer().security_audit());
    }
    state
        .bifrost
        .scribe()
        .map(|scribe| scribe.fragment_security_audit())
}

/// Build the **private** Bifrost peer router served on the mutually
/// authenticated peer listener.
///
/// This router mounts exactly the closed set of private services this target's
/// selected roles own: the Oracle peer service, the Scribe tail service, the
/// upstream DataFusion worker adapter, and the Oracle lifecycle service. It
/// deliberately mounts neither health nor reflection — a private listener
/// advertises nothing to an unauthenticated caller.
///
/// Returns `Ok(None)` when this target selects no private service, which is the
/// Forge-worker case: it keeps using its durable assignment path and opens no
/// peer socket.
///
/// # Errors
/// Returns [`GrpcError::MissingTokenVerifier`] when the Oracle lifecycle
/// service is selected without a configured token verifier, or
/// [`GrpcError::Transport`] when tonic rejects the peer TLS material.
pub fn build_peer_grpc(
    state: &AppState,
    tls: wyrd_tonic::server::MutualTlsServerConfig,
    denial_audit_concurrency: usize,
) -> Result<Option<TonicRouter>, GrpcError> {
    let serves_ingest = state.bifrost_ingest().is_some();
    let serves_query = state.bifrost_query().is_some();
    if !state.bifrost.serves_api() || !(serves_ingest || serves_query) {
        return Ok(None);
    }
    let transport = state.bifrost.transport_admission();
    // One boundary, composed once and cloned into every mounted service, so no
    // private adapter can acquire an authentication path of its own.
    let auth = PeerWorkloadAuthLayer::new(
        state.bifrost.shared_token_verifier(),
        state
            .bifrost
            .peer_identity()
            .cloned()
            .ok_or(GrpcError::MissingPeerIdentity)?,
        peer_security_audit(state).ok_or(GrpcError::MissingPeerIdentity)?,
        denial_audit_concurrency,
    );
    // `OraclePeerService` is the one adapter every peer-bearing target mounts:
    // a Scribe answers fragment operations on it and an Oracle answers query
    // control, so it anchors the router and later services extend it. The
    // protobuf service is never forked by role; an operation whose local
    // capability is absent fails closed after authentication instead.
    let router = wyrd_tonic::server::mutual_tls_server(tls)?.add_service(auth.wrap(
        GrpcTransportAdmissionService::new_peer(
            crate::oracle::OraclePeerGrpc::new(Arc::clone(&state.bifrost)).into_server(),
            transport.clone(),
        ),
    ));
    let router = match state.bifrost_ingest() {
        Some(scribe) => router.add_service(
            auth.wrap(GrpcTransportAdmissionService::new_peer(
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
            )),
        ),
        None => router,
    };
    let router = match state
        .bifrost_query()
        .and_then(|query| query.engine().analytical_worker())
    {
        Some(ingress) => {
            let worker = ingress.worker().clone().into_worker_server();
            router.add_service(auth.wrap(GrpcTransportAdmissionService::new_peer(
                vala_bifrost_redux::oracle::analytical_transport::AnalyticalStageAuthLayer::new(
                    ingress,
                )
                .layer_service(worker),
                transport.clone(),
            )))
        }
        None => router,
    };
    let router = match state.bifrost_query() {
        Some(query) => router.add_service(
            auth.wrap(GrpcTransportAdmissionService::new_peer(
                crate::oracle::OracleLifecycleGrpc::new(
                    state
                        .auth
                        .token_verifier
                        .clone()
                        .ok_or(GrpcError::MissingTokenVerifier)?,
                    Arc::clone(query),
                )
                .into_server(),
                transport,
            )),
        ),
        None => router,
    };
    Ok(Some(router))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use http_body_util::{BodyExt, Full, StreamBody};
    use tower::ServiceExt;
    use wyrd_tonic::tonic::codegen::Bytes;

    use vala_bifrost_redux::gate::limits::{
        BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES, BifrostTransportAdmission,
    };

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

    /// Builds one body delivering `chunks` as separate DATA frames.
    ///
    /// HTTP/2 frame boundaries are the whole point of these cases: a gRPC
    /// header can arrive split across two frames, and several messages can
    /// arrive coalesced into one.
    fn chunked_body(chunks: Vec<Vec<u8>>) -> Body {
        let frames = futures_util::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok::<_, Infallible>(http_body::Frame::data(Bytes::from(chunk)))),
        );
        Body::new(StreamBody::new(frames))
    }

    /// Builds the five header bytes declaring an uncompressed `payload` length.
    ///
    /// # Panics
    ///
    /// Panics when a test requests a length outside the gRPC `u32` envelope.
    fn header_for(payload: usize) -> Vec<u8> {
        let length = u32::try_from(payload).expect("test frame length fits the gRPC u32 envelope");
        let mut header = vec![0_u8];
        header.extend_from_slice(&length.to_be_bytes());
        header
    }

    /// Drains a body into the bytes an inner service would have received.
    ///
    /// # Panics
    ///
    /// Panics when the body yields a transport error, which no test body does.
    async fn drain(mut body: Body) -> Vec<u8> {
        let mut collected = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.expect("test body never errors");
            if let Some(data) = frame.data_ref() {
                collected.extend_from_slice(data);
            }
        }
        collected
    }

    /// A header split across DATA frames is still read, and every byte replays.
    ///
    /// # Panics
    ///
    /// Panics when the split header is not reassembled or a byte is lost.
    #[tokio::test]
    async fn a_split_first_header_is_reassembled_and_fully_replayed() {
        let mut header = header_for(9);
        let tail = header.split_off(2);
        let mut body = chunked_body(vec![header.clone(), tail.clone(), vec![7_u8; 9]]);

        let head = GrpcFirstFrame::read(&mut body)
            .await
            .expect("test body never errors");

        assert_eq!(head.declared(), Ok(Some(9)));
        let mut expected = header;
        expected.extend_from_slice(&tail);
        expected.extend_from_slice(&[7_u8; 9]);
        assert_eq!(drain(head.replay(body)).await, expected);
    }

    /// A first payload split across frames is bounded once and replayed whole.
    ///
    /// # Panics
    ///
    /// Panics when the declared bound or the replayed bytes differ.
    #[tokio::test]
    async fn a_split_first_payload_is_bounded_once_and_replayed_whole() {
        let mut first = header_for(6);
        first.extend_from_slice(&[1_u8, 2, 3]);
        let mut body = chunked_body(vec![first.clone(), vec![4_u8, 5, 6]]);

        let head = GrpcFirstFrame::read(&mut body)
            .await
            .expect("test body never errors");

        assert_eq!(head.declared(), Ok(Some(6)));
        let mut expected = first;
        expected.extend_from_slice(&[4_u8, 5, 6]);
        assert_eq!(drain(head.replay(body)).await, expected);
    }

    /// Messages coalesced behind the first one are bounded by the first alone.
    ///
    /// This is the defect the bound previously had: the following messages must
    /// reach the codec untouched rather than being counted or dropped.
    ///
    /// # Panics
    ///
    /// Panics when the bound counts a later message or a byte is lost.
    #[tokio::test]
    async fn coalesced_later_messages_are_neither_counted_nor_lost() {
        let mut coalesced = header_for(4);
        coalesced.extend_from_slice(&[1_u8, 2, 3, 4]);
        coalesced.extend_from_slice(&header_for(3));
        coalesced.extend_from_slice(&[5_u8, 6, 7]);
        let mut body = chunked_body(vec![coalesced.clone()]);

        let head = GrpcFirstFrame::read(&mut body)
            .await
            .expect("test body never errors");

        assert_eq!(head.declared(), Ok(Some(4)));
        assert_eq!(drain(head.replay(body)).await, coalesced);
    }

    /// A malformed compression flag is refused before any byte is forwarded.
    ///
    /// # Panics
    ///
    /// Panics when the flag is accepted or the inner service is invoked.
    #[tokio::test]
    async fn a_malformed_compression_flag_never_reaches_the_codec() {
        let admission = BifrostTransportAdmission::for_tests();
        let invoked = Arc::new(AtomicBool::new(false));
        let probe = AdmissionProbe {
            invoked: Arc::clone(&invoked),
            observed_bytes: Arc::new(AtomicUsize::new(0)),
            admission: admission.clone(),
        };
        let mut malformed = vec![9_u8];
        malformed.extend_from_slice(&4_u32.to_be_bytes());
        let service = GrpcTransportAdmissionService::new(probe, admission.clone());

        let response = service
            .oneshot(Request::new(chunked_body(vec![malformed])))
            .await
            .expect("infallible refusal response");

        assert_eq!(
            response
                .headers()
                .get("grpc-status")
                .and_then(|value| value.to_str().ok()),
            Some("3")
        );
        assert!(!invoked.load(Ordering::Acquire));
        assert_eq!(admission.used_bytes(), 0);
    }

    /// A compressed first frame declares a compressed length, so it is unknown.
    ///
    /// Admitting it against its declared length would lease less capacity than
    /// the decompressed message occupies.
    ///
    /// # Panics
    ///
    /// Panics when a compressed frame is bounded by its compressed length.
    #[tokio::test]
    async fn a_compressed_first_frame_is_bounded_as_unknown() {
        let mut compressed = vec![1_u8];
        compressed.extend_from_slice(&4_u32.to_be_bytes());
        let mut body = chunked_body(vec![compressed]);

        let head = GrpcFirstFrame::read(&mut body)
            .await
            .expect("test body never errors");

        assert_eq!(head.declared(), Ok(None));
    }

    /// An empty body has no header to read and is admitted as unknown.
    ///
    /// # Panics
    ///
    /// Panics when an empty body is treated as a declared length.
    #[tokio::test]
    async fn an_empty_body_declares_nothing() {
        let mut body = Body::empty();

        let head = GrpcFirstFrame::read(&mut body)
            .await
            .expect("test body never errors");

        assert_eq!(head.declared(), Ok(None));
    }

    /// Actual gRPC frame length admits the exact cap and refuses one byte over.
    ///
    /// # Panics
    ///
    /// Panics when the in-memory service unexpectedly errors or assertions fail.
    #[tokio::test]
    async fn bifrost_transport_admission_precedes_tonic_decode_at_frame_boundary() {
        let admission = BifrostTransportAdmission::for_tests();
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
