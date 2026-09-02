//! East-west stage-operation authority for Oracle's Analytical transport.
//!
//! Upstream ships its own gRPC worker service and client, and Wyrd keeps both
//! unchanged: introducing a second wire format for the same messages would be a
//! duplicate serialization bridge. What Wyrd adds is a pair of method-scoped
//! Tower layers around them.
//!
//! * The **mint** layer runs on the coordinator's channel. It reads the exact
//!   bytes about to go on the wire, signs a single-use stage ticket over them,
//!   and attaches the ticket as metadata.
//! * The **auth** layer runs in front of the follower's service. It reads the
//!   same bytes, re-derives the binding from its own node identity, and refuses
//!   before tonic decodes any protobuf, before the task cache is consulted,
//!   before a provider is constructed, and before any resource or storage I/O.
//!
//! This sits *on top of* peer mTLS and workload authentication, which is what
//! establishes who is calling. The buffered message proves only that the caller
//! signed these exact bytes; it never establishes peer identity, and it is not
//! first-frame authentication. Existing typed-handler checks run after tonic has
//! already decoded the protobuf, which is precisely why they cannot carry the
//! bind-before-decode guarantee this layer exists for.
//!
//! # Framing and what the digest covers
//!
//! A gRPC message on the wire is a five-byte prefix — one compression flag byte
//! and a four-byte big-endian length — followed by that many payload bytes. The
//! digest covers **the complete framed message, prefix included**, so the
//! compression flag is bound along with the payload and neither end has to agree
//! on a decompressed form. Both layers use [`stage_body_digest`] over exactly
//! those bytes; there is one representation, not two.
//!
//! # Why only the first message of a stream
//!
//! `ExecuteTask` is unary: its one request message is the whole body. The
//! coordinator channel is bidirectional and stays open for the query's life
//! while work units stream over it, so collecting its body would deadlock the
//! protocol. Only the initial `SetPlan` message is buffered and bound; it is
//! replayed byte-exact and the remaining live stream is forwarded untouched,
//! preserving frame independence, trailers, body errors, cancellation, flow
//! control, and backpressure.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use http::{HeaderMap, HeaderName, HeaderValue, Request, Response};
use http_body::{Body, Frame, SizeHint};
use wyrd_tonic::tonic::body::Body as TonicBody;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use datafusion::arrow::array::RecordBatch;
use datafusion::common::DataFusionError;
use datafusion::execution::TaskContext;
use datafusion::physical_plan::metrics::ExecutionPlanMetricsSet;
use datafusion_distributed::grpc::{BoxCloneSyncChannel, create_worker_client};
use datafusion_distributed::{
    ChannelResolver, CoordinatorToWorkerMsg, ExecuteTaskRequest, GetWorkerInfoRequest,
    GetWorkerInfoResponse, SetPlanRequest, TaskKey, WorkerChannel, WorkerToCoordinatorMsg,
};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use sha2::{Digest as _, Sha256};
use tower::Layer as _;
use url::Url;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{NodeId, SignedPeerTicket};

use super::analytical::{
    AnalyticalConnectionLease, AnalyticalGraphKey, AnalyticalStageIngress,
    DATAFUSION_QUERY_ID_HEADER, PUBLIC_QUERY_ID_HEADER,
};
use super::dispatcher::{BifrostPeerTls, OraclePeerCredentials};
use super::peer::{
    MAX_STAGE_BODY_BYTES, MAX_STAGE_PARTICIPANTS, OracleStageAuthority, PeerSecurityError,
    StageBinding, StageOperationV1, StageParticipantV1, StageTicketClaims, stage_body_digest,
};
use super::telemetry::record_exchange_transfer;

/// gRPC path of the bidirectional coordinator channel this layer governs.
///
/// Only its initial `SetPlan` message is bound; see the module documentation.
pub(crate) const COORDINATOR_CHANNEL_PATH: &str = "/worker.WorkerService/CoordinatorChannel";

/// gRPC path of the unary task execution call this layer governs.
pub(crate) const EXECUTE_TASK_PATH: &str = "/worker.WorkerService/ExecuteTask";

/// Bytes of the gRPC length-prefix that precede every message payload.
const GRPC_PREFIX_BYTES: usize = 5;

/// The stage operation a governed gRPC path performs, if any.
///
/// Returning `None` for an unrecognized path is what makes the auth layer fail
/// closed: a method this layer does not understand cannot be authorized, so it
/// is refused rather than forwarded unchecked.
#[must_use]
pub(crate) fn governed_operation(path: &str) -> Option<StageOperationV1> {
    match path {
        COORDINATOR_CHANNEL_PATH => Some(StageOperationV1::SetPlan),
        EXECUTE_TASK_PATH => Some(StageOperationV1::ExecuteTask),
        // Every other path, worker version discovery included, carries no
        // stage identity and is refused rather than forwarded: Wyrd pins its
        // own worker build, so a follower has nothing to tell a coordinator
        // that the coordinator does not already know.
        _ => None,
    }
}

/// Reasons the first message of a governed request could not be bound.
///
/// Every variant is terminal and fails the request closed. None of them are
/// retried: a caller that framed its message wrongly, or oversized it, does not
/// get a second attempt against the same single-use nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StageFramingError {
    /// The declared or accumulated message length exceeds the hard bound.
    ///
    /// Detected while reading rather than after collecting, so an oversized
    /// message never causes an unbounded allocation.
    #[error("stage message exceeds the maximum bound")]
    Oversized,
    /// The body ended before a complete message had been read.
    #[error("stage message framing is incomplete")]
    Incomplete,
    /// The underlying body failed before a complete message had been read.
    #[error("stage message body failed")]
    BodyFailed,
}

impl From<StageFramingError> for PeerSecurityError {
    /// Projects a framing failure onto the closed peer-security refusal.
    ///
    /// Framing failures are body failures: the receiver could not establish
    /// which bytes the signature was supposed to cover, which is the same
    /// refusal as a body that does not match its digest.
    fn from(_: StageFramingError) -> Self {
        Self::Body
    }
}

/// Incremental reader for exactly one complete gRPC message.
///
/// The reader enforces [`MAX_STAGE_BODY_BYTES`] as it accumulates rather than
/// afterwards: the declared length is checked the moment the five-byte prefix is
/// complete, and the accumulated total is checked on every chunk. An attacker
/// therefore cannot make the follower allocate an unbounded buffer by declaring
/// or streaming a large message.
///
/// Bytes that arrive after the first complete message — a second message packed
/// into the same HTTP/2 data frame — are retained separately so they can be
/// replayed in order rather than dropped.
#[derive(Debug)]
pub(crate) struct StageMessageReader {
    /// Bytes accumulated toward the first complete framed message.
    buffer: BytesMut,
    /// Total framed length once the five-byte prefix has been read.
    framed_len: Option<usize>,
    /// Hard bound on the complete framed message.
    limit: usize,
}

impl Default for StageMessageReader {
    /// Creates a reader bounded by [`MAX_STAGE_BODY_BYTES`].
    fn default() -> Self {
        Self::new(MAX_STAGE_BODY_BYTES)
    }
}

impl StageMessageReader {
    /// Creates a reader that refuses any framed message above `limit`.
    #[must_use]
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            buffer: BytesMut::new(),
            framed_len: None,
            limit,
        }
    }

    /// Accepts one chunk, returning the first complete framed message when done.
    ///
    /// The returned pair is the complete framed message — prefix included, which
    /// is exactly what the digest covers — and any trailing bytes from the same
    /// chunk that belong to later messages.
    ///
    /// # Errors
    ///
    /// Returns [`StageFramingError::Oversized`] as soon as the declared or
    /// accumulated length passes the bound, before the offending bytes are
    /// retained.
    pub(crate) fn push(
        &mut self,
        chunk: &[u8],
    ) -> Result<Option<(Bytes, Bytes)>, StageFramingError> {
        if self.buffer.len().saturating_add(chunk.len()) > self.limit {
            return Err(StageFramingError::Oversized);
        }
        self.buffer.extend_from_slice(chunk);
        if self.framed_len.is_none() && self.buffer.len() >= GRPC_PREFIX_BYTES {
            let mut length = [0_u8; 4];
            length.copy_from_slice(&self.buffer[1..GRPC_PREFIX_BYTES]);
            let payload = usize::try_from(u32::from_be_bytes(length))
                .map_err(|_| StageFramingError::Oversized)?;
            let framed = payload
                .checked_add(GRPC_PREFIX_BYTES)
                .ok_or(StageFramingError::Oversized)?;
            if framed > self.limit {
                return Err(StageFramingError::Oversized);
            }
            self.framed_len = Some(framed);
        }
        let Some(framed_len) = self.framed_len else {
            return Ok(None);
        };
        if self.buffer.len() < framed_len {
            return Ok(None);
        }
        let mut complete = std::mem::take(&mut self.buffer);
        let leftover = complete.split_off(framed_len);
        Ok(Some((complete.freeze(), leftover.freeze())))
    }
}

/// A body that re-emits already-read bytes before delegating to its source.
///
/// The auth and mint layers must consume the head of a request body to bind it,
/// then hand the *same* request onward. This body replays the exact chunks that
/// were consumed — the framed message and any trailing bytes from the same data
/// frame — and then forwards the untouched remainder, including its trailers,
/// errors, end-of-stream, and backpressure. Chunk boundaries are not preserved
/// across the replay seam, which HTTP/2 does not require: message framing is
/// independent of data-frame boundaries.
#[derive(Debug)]
pub struct ReplayBody<B> {
    /// Consumed chunks awaiting replay, in wire order.
    replay: VecDeque<Bytes>,
    /// The source body, resumed once every replayed chunk is delivered.
    inner: B,
    /// Follower ownership held for exactly as long as this request body lives.
    ///
    /// Present only on the bidirectional coordinator channel, whose request
    /// stream tonic holds for the whole call. Dropping this body is therefore
    /// the single honest signal that the coordinator is gone, whatever removed
    /// it, and it is what releases the graph this call admitted.
    lease: Option<AnalyticalConnectionLease>,
}

impl<B> ReplayBody<B> {
    /// Wraps `inner`, replaying `replay` in order before resuming it.
    #[must_use]
    pub fn new(replay: VecDeque<Bytes>, inner: B) -> Self {
        Self {
            replay,
            inner,
            lease: None,
        }
    }

    /// Attaches follower graph ownership to this body's lifetime.
    #[must_use]
    pub fn with_lease(mut self, lease: AnalyticalConnectionLease) -> Self {
        self.lease = Some(lease);
        self
    }
}

impl<B> Body for ReplayBody<B>
where
    B: Body<Data = Bytes> + Unpin,
{
    /// Replayed and forwarded chunks share the source body's data type.
    type Data = Bytes;
    /// Replay never fails, so every error observed is the source body's own.
    type Error = B::Error;

    /// Delivers the replayed chunks in order, then polls the source body.
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if let Some(chunk) = self.replay.pop_front() {
            return Poll::Ready(Some(Ok(Frame::data(chunk))));
        }
        Pin::new(&mut self.inner).poll_frame(cx)
    }

    /// Reports end-of-stream only once no replayed chunk remains.
    fn is_end_stream(&self) -> bool {
        self.replay.is_empty() && self.inner.is_end_stream()
    }

    /// Adds the retained replay bytes to the source body's own hint.
    fn size_hint(&self) -> SizeHint {
        let replayed = self.replay.iter().map(|chunk| chunk.len() as u64).sum();
        let inner = self.inner.size_hint();
        let mut hint = SizeHint::new();
        hint.set_lower(inner.lower().saturating_add(replayed));
        if let Some(upper) = inner.upper() {
            hint.set_upper(upper.saturating_add(replayed));
        }
        hint
    }
}

/// Reads the first complete framed message from `body`, retaining what it read.
///
/// Returns the framed message to digest and the chunks to replay, so the caller
/// can rebuild an identical request. Cancellation while this future is pending
/// simply drops the partially read body; nothing has been forwarded, no ticket
/// has been consumed, and the follower has done no work.
///
/// # Errors
///
/// Returns [`StageFramingError::Oversized`] when the bound is passed while
/// reading, [`StageFramingError::Incomplete`] when the body ends first, and
/// [`StageFramingError::BodyFailed`] when the source body errors first.
pub(crate) async fn read_first_message<B>(
    body: &mut B,
    limit: usize,
) -> Result<(Bytes, VecDeque<Bytes>), StageFramingError>
where
    B: Body<Data = Bytes> + Unpin,
{
    let mut reader = StageMessageReader::new(limit);
    let mut replay = VecDeque::new();
    loop {
        let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut *body).poll_frame(cx)).await
        else {
            return Err(StageFramingError::Incomplete);
        };
        let frame = frame.map_err(|_| StageFramingError::BodyFailed)?;
        let Ok(data) = frame.into_data() else {
            // A trailers frame before a complete message means the caller ended
            // the request without ever sending one.
            return Err(StageFramingError::Incomplete);
        };
        if let Some((framed, leftover)) = reader.push(&data)? {
            replay.push_back(framed.clone());
            if !leftover.is_empty() {
                replay.push_back(leftover);
            }
            return Ok((framed, replay));
        }
    }
}

/// Names one stage metadata key on the transport.
///
/// The keys are grouped here so the mint and auth layers cannot drift apart on
/// spelling; both sides read this one list.
pub(crate) struct StageHeaderNames;

impl StageHeaderNames {
    /// Signing key identifier of the ticket.
    pub(crate) const KEY_ID: &'static str = "wyrd-oracle-stage-key-id";
    /// Base64 of the signed claims.
    pub(crate) const CLAIMS: &'static str = "wyrd-oracle-stage-claims";
    /// Base64 of the ticket signature.
    pub(crate) const SIGNATURE: &'static str = "wyrd-oracle-stage-signature";
    /// Coordinator node identity the follower must expect.
    pub(crate) const SOURCE_NODE: &'static str = "wyrd-oracle-stage-source-node";
    /// Coordinator role fence the follower must expect.
    pub(crate) const SOURCE_FENCE: &'static str = "wyrd-oracle-stage-source-fence";
    /// Authenticated data tenant of the graph.
    pub(crate) const TENANT: &'static str = "wyrd-oracle-stage-tenant";
    /// Pinned snapshot digest of the graph's cut.
    pub(crate) const SNAPSHOT_DIGEST: &'static str = "wyrd-oracle-stage-snapshot";
    /// Graph-local stage ordinal.
    pub(crate) const STAGE_ID: &'static str = "wyrd-oracle-stage-id";
    /// Stage-local task ordinal, absent only for a stage-scoped operation
    /// that addresses no single task.
    pub(crate) const TASK_ID: &'static str = "wyrd-oracle-stage-task-id";
    /// Attempt ordinal within the graph.
    pub(crate) const ATTEMPT: &'static str = "wyrd-oracle-stage-attempt";
    /// Reservation the follower resolved for the graph.
    pub(crate) const RESERVATION_ID: &'static str = "wyrd-oracle-stage-reservation";
    /// Permission digest the follower resolved for the query.
    pub(crate) const PERMISSION_DIGEST: &'static str = "wyrd-oracle-stage-permission";
}

/// Reads one required metadata value as UTF-8.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] when the key is absent or not valid
/// header-safe UTF-8, which fails the operation closed rather than binding a
/// partially derived expectation.
pub(crate) fn required_header<'headers>(
    headers: &'headers HeaderMap,
    name: &str,
) -> Result<&'headers str, PeerSecurityError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or(PeerSecurityError::Claims)
}

/// Builds a header value from an ASCII-safe string.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Encoding`] when the value cannot be represented
/// on the transport, which refuses to mint rather than send a truncated binding.
pub(crate) fn header_value(value: &str) -> Result<HeaderValue, PeerSecurityError> {
    HeaderValue::from_str(value).map_err(|_| PeerSecurityError::Encoding)
}

/// Rebuilds `request` with `replay` prepended to its body.
///
/// The method, URI, version, extensions, and every header are preserved exactly;
/// only the body is wrapped, so the inner service observes the request it would
/// have observed had nothing read from it.
pub(crate) fn replay_request<B>(
    request: Request<B>,
    replay: VecDeque<Bytes>,
    lease: Option<AnalyticalConnectionLease>,
) -> Request<ReplayBody<B>> {
    let (parts, body) = request.into_parts();
    let body = ReplayBody::new(replay, body);
    let body = match lease {
        Some(lease) => body.with_lease(lease),
        None => body,
    };
    Request::from_parts(parts, body)
}

/// Builds the closed gRPC refusal returned for any stage authority failure.
///
/// Every refusal is `PermissionDenied` with no detail beyond the class, so a
/// caller cannot use the response to distinguish an unknown key from a bad
/// signature from a consumed nonce.
pub(crate) fn refusal<B: Default>() -> Response<B> {
    wyrd_tonic::tonic::Status::permission_denied("Oracle analytical stage operation refused")
        .into_http()
}

/// Encodes ticket bytes for transport metadata.
#[must_use]
pub(crate) fn encode_ticket_bytes(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decodes ticket bytes from transport metadata.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] when the value is not valid base64.
pub(crate) fn decode_ticket_bytes(value: &str) -> Result<Vec<u8>, PeerSecurityError> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| PeerSecurityError::Claims)
}

/// The stage identity that travels beside a governed message.
///
/// Every field here is also covered by the ticket signature, so the wire copy is
/// an *echo*: the follower re-derives its expectation from these values and the
/// signed claims must match field for field. Tampering with any of them
/// therefore produces a binding refusal rather than a different authorization.
///
/// The two fields that are deliberately *not* here are the destination node
/// identity and its role fence. The receiver supplies those from itself, which
/// is what stops a ticket minted for one follower from being replayed at
/// another, or a ticket minted before a restart from being accepted after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StageWireIdentity {
    /// Coordinator node that minted the ticket.
    pub(crate) source_node_id: NodeId,
    /// Coordinator role fence at mint time.
    pub(crate) source_fence: u64,
    /// Authenticated data tenant of the graph.
    pub(crate) tenant_id: DataTenantId,
    /// The two-identity graph the operation belongs to.
    pub(crate) graph: AnalyticalGraphKey,
    /// Pinned snapshot digest of the graph's cut.
    pub(crate) snapshot_digest: String,
    /// Graph-local stage ordinal.
    pub(crate) stage_id: u32,
    /// Stage-local task ordinal, absent for plan installation.
    pub(crate) task_id: Option<u32>,
    /// Attempt ordinal within the graph.
    pub(crate) attempt: u32,
    /// Reservation resolved for the graph.
    pub(crate) reservation_id: String,
    /// Permission digest resolved for the query.
    pub(crate) permission_digest: String,
}

impl StageWireIdentity {
    /// Writes the identity onto one governed request's metadata.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Encoding`] when a value cannot be
    /// represented on the transport, refusing to send a partial binding.
    pub(crate) fn write(&self, headers: &mut HeaderMap) -> Result<(), PeerSecurityError> {
        headers.insert(
            StageHeaderNames::SOURCE_NODE,
            header_value(&self.source_node_id.as_uuid().to_string())?,
        );
        headers.insert(
            StageHeaderNames::SOURCE_FENCE,
            header_value(&self.source_fence.to_string())?,
        );
        headers.insert(
            StageHeaderNames::TENANT,
            header_value(&self.tenant_id.as_uuid().to_string())?,
        );
        headers.insert(
            PUBLIC_QUERY_ID_HEADER,
            header_value(&self.graph.public_query_id.to_string())?,
        );
        headers.insert(
            DATAFUSION_QUERY_ID_HEADER,
            header_value(&self.graph.datafusion_query_id.to_string())?,
        );
        headers.insert(
            StageHeaderNames::SNAPSHOT_DIGEST,
            header_value(&self.snapshot_digest)?,
        );
        headers.insert(
            StageHeaderNames::STAGE_ID,
            header_value(&self.stage_id.to_string())?,
        );
        if let Some(task_id) = self.task_id {
            headers.insert(
                StageHeaderNames::TASK_ID,
                header_value(&task_id.to_string())?,
            );
        } else {
            headers.remove(StageHeaderNames::TASK_ID);
        }
        headers.insert(
            StageHeaderNames::ATTEMPT,
            header_value(&self.attempt.to_string())?,
        );
        headers.insert(
            StageHeaderNames::RESERVATION_ID,
            header_value(&self.reservation_id)?,
        );
        headers.insert(
            StageHeaderNames::PERMISSION_DIGEST,
            header_value(&self.permission_digest)?,
        );
        Ok(())
    }

    /// Reads the identity a governed request declared.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Claims`] when any required value is absent
    /// or unparseable. Nothing is derived from a partially readable identity.
    pub(crate) fn read(headers: &HeaderMap) -> Result<Self, PeerSecurityError> {
        let source_node_id = NodeId::new(parse_uuid(required_header(
            headers,
            StageHeaderNames::SOURCE_NODE,
        )?)?);
        let tenant_id = DataTenantId::new(parse_uuid(required_header(
            headers,
            StageHeaderNames::TENANT,
        )?)?)
        .map_err(|_| PeerSecurityError::Claims)?;
        let graph =
            AnalyticalGraphKey::from_headers(headers).map_err(|_| PeerSecurityError::Claims)?;
        let task_id = match headers.get(StageHeaderNames::TASK_ID) {
            Some(value) => Some(parse_u32(
                value.to_str().map_err(|_| PeerSecurityError::Claims)?,
            )?),
            None => None,
        };
        Ok(Self {
            source_node_id,
            source_fence: parse_u64(required_header(headers, StageHeaderNames::SOURCE_FENCE)?)?,
            tenant_id,
            graph,
            snapshot_digest: required_header(headers, StageHeaderNames::SNAPSHOT_DIGEST)?
                .to_owned(),
            stage_id: parse_u32(required_header(headers, StageHeaderNames::STAGE_ID)?)?,
            task_id,
            attempt: parse_u32(required_header(headers, StageHeaderNames::ATTEMPT)?)?,
            reservation_id: required_header(headers, StageHeaderNames::RESERVATION_ID)?.to_owned(),
            permission_digest: required_header(headers, StageHeaderNames::PERMISSION_DIGEST)?
                .to_owned(),
        })
    }

    /// Derives the receiver's own expectation for one governed operation.
    ///
    /// `destination_node_id` and `destination_fence` come from the receiver, not
    /// from the wire, which is the whole reason this is a *binding* rather than
    /// a restatement of what the caller sent.
    #[must_use]
    pub(crate) fn to_binding(
        &self,
        operation: StageOperationV1,
        destination_node_id: NodeId,
        destination_fence: u64,
    ) -> StageBinding {
        StageBinding {
            operation,
            source_node_id: self.source_node_id,
            source_fence: self.source_fence,
            destination_node_id,
            destination_fence,
            tenant_id: self.tenant_id,
            public_query_id: self.graph.public_query_id.as_uuid(),
            datafusion_query_id: self.graph.datafusion_query_id.as_uuid(),
            snapshot_digest: self.snapshot_digest.clone(),
            stage_id: self.stage_id,
            task_id: self.task_id,
            attempt: self.attempt,
            reservation_id: self.reservation_id.clone(),
            permission_digest: self.permission_digest.clone(),
        }
    }
}

/// Writes one minted ticket onto a governed request's metadata.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Encoding`] when a component cannot be
/// represented on the transport.
pub(crate) fn write_ticket(
    headers: &mut HeaderMap,
    ticket: &SignedPeerTicket,
) -> Result<(), PeerSecurityError> {
    headers.insert(StageHeaderNames::KEY_ID, header_value(&ticket.key_id)?);
    headers.insert(
        StageHeaderNames::CLAIMS,
        header_value(&encode_ticket_bytes(&ticket.claims_bytes))?,
    );
    headers.insert(
        StageHeaderNames::SIGNATURE,
        header_value(&encode_ticket_bytes(&ticket.signature))?,
    );
    Ok(())
}

/// Reads the ticket a governed request presented.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] when a component is absent or not
/// valid base64. No signature check happens here; that is the authority's job.
pub(crate) fn read_ticket(headers: &HeaderMap) -> Result<SignedPeerTicket, PeerSecurityError> {
    Ok(SignedPeerTicket {
        key_id: required_header(headers, StageHeaderNames::KEY_ID)?.to_owned(),
        claims_bytes: decode_ticket_bytes(required_header(headers, StageHeaderNames::CLAIMS)?)?,
        signature: decode_ticket_bytes(required_header(headers, StageHeaderNames::SIGNATURE)?)?,
    })
}

/// Parses a UUID carried on the transport.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] for any unparseable value.
fn parse_uuid(value: &str) -> Result<Uuid, PeerSecurityError> {
    Uuid::parse_str(value).map_err(|_| PeerSecurityError::Claims)
}

/// Parses a 32-bit ordinal carried on the transport.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] for any unparseable value.
fn parse_u32(value: &str) -> Result<u32, PeerSecurityError> {
    value.parse().map_err(|_| PeerSecurityError::Claims)
}

/// Parses a 64-bit fence carried on the transport.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Claims`] for any unparseable value.
fn parse_u64(value: &str) -> Result<u64, PeerSecurityError> {
    value.parse().map_err(|_| PeerSecurityError::Claims)
}

/// Everything a coordinator needs to mint tickets for one worker channel.
///
/// One minter serves exactly one target follower, because the destination node
/// identity and role fence it signs are that follower's. Reusing a minter across
/// targets would mint tickets that the receiving follower correctly refuses.
/// The frozen destination set one Analytical attempt is allowed to address.
///
/// Built once by the leader from its pinned attempt cut and thereafter carried
/// inside every signed stage ticket, so a follower acting as a coordinator
/// adopts the leader's cut verbatim instead of re-reading live membership. A
/// destination is identified by endpoint *and* fence: a node that restarts
/// under a new fence is a different incarnation and is simply absent from this
/// cut, which fails the resolution locally rather than redirecting the request
/// to the replacement.
pub(crate) struct AnalyticalParticipantCut {
    /// Signed wire form, stamped verbatim into every ticket minted from it.
    wire: Vec<StageParticipantV1>,
    /// Endpoint index every outbound channel resolves its audience through.
    by_url: HashMap<Url, AnalyticalDestination>,
}

/// One frozen participant's identity and the reservation it executes under.
///
/// The reservation belongs to the destination, not to the coordinator: each
/// node grants its own, and the leader takes one per participant before it
/// freezes the cut. Carrying it here is what lets a follower that becomes a
/// coordinator address its peers under the reservations their own leader
/// granted rather than under an identity it made up.
#[derive(Debug, Clone)]
pub(crate) struct AnalyticalDestination {
    /// Frozen node identity of this participant.
    pub(crate) node_id: NodeId,
    /// Role-incarnation fence this participant was frozen at.
    pub(crate) fence: u64,
    /// Reservation the leader took on this participant for the whole graph.
    pub(crate) reservation_id: String,
}

impl fmt::Debug for AnalyticalParticipantCut {
    /// Reports the cut's size without rendering endpoints.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalParticipantCut")
            .field("participants", &self.wire.len())
            .finish_non_exhaustive()
    }
}

impl AnalyticalParticipantCut {
    /// Freezes one leader-side attempt cut into its immutable destination set.
    ///
    /// Rejects a plaintext endpoint outright: every east-west channel is
    /// mutually authenticated, so an endpoint that could only be dialed in the
    /// clear is not a reachable destination and must never enter the cut a
    /// ticket authorizes.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the cut exceeds
    /// [`MAX_STAGE_PARTICIPANTS`] or names a non-TLS endpoint.
    pub(crate) fn freeze(
        destinations: HashMap<Url, AnalyticalDestination>,
    ) -> Result<Self, BifrostError> {
        let mut wire = destinations
            .iter()
            .map(|(url, destination)| {
                if url.scheme() != "https" {
                    return Err(BifrostError::Internal {
                        detail: "Oracle analytical participant endpoint is not mutually \
                                 authenticated"
                            .to_owned(),
                    });
                }
                Ok(StageParticipantV1 {
                    node_id: destination.node_id.as_uuid().as_bytes().to_vec(),
                    fence: destination.fence,
                    address: url.to_string(),
                    reservation_id: destination.reservation_id.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if wire.len() > MAX_STAGE_PARTICIPANTS {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical attempt cut exceeds the addressable participant bound"
                    .to_owned(),
            });
        }
        canonically_order(&mut wire);
        Ok(Self {
            wire,
            by_url: destinations,
        })
    }

    /// Adopts the cut a verified stage ticket carried, without widening it.
    ///
    /// This is the follower's only source of destinations. Nothing here
    /// consults membership, so a graph authorized on this node addresses
    /// exactly the participants its coordinator froze.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the carried cut exceeds
    /// [`MAX_STAGE_PARTICIPANTS`], names a malformed or plaintext endpoint, or
    /// carries a malformed node identity.
    pub(crate) fn adopt(wire: &[StageParticipantV1]) -> Result<Self, BifrostError> {
        if wire.len() > MAX_STAGE_PARTICIPANTS {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical stage cut exceeds the addressable participant bound"
                    .to_owned(),
            });
        }
        let mut by_url = HashMap::with_capacity(wire.len());
        for participant in wire {
            let url = Url::parse(&participant.address).map_err(|error| BifrostError::Internal {
                detail: format!("Oracle analytical stage cut endpoint is not a valid URL: {error}"),
            })?;
            if url.scheme() != "https" {
                return Err(BifrostError::Internal {
                    detail: "Oracle analytical stage cut endpoint is not mutually authenticated"
                        .to_owned(),
                });
            }
            let node_id = Uuid::from_slice(&participant.node_id)
                .map(NodeId::new)
                .map_err(|error| BifrostError::Internal {
                    detail: format!("Oracle analytical stage cut names an invalid node: {error}"),
                })?;
            by_url.insert(
                url,
                AnalyticalDestination {
                    node_id,
                    fence: participant.fence,
                    reservation_id: participant.reservation_id.clone(),
                },
            );
        }
        Ok(Self {
            wire: wire.to_vec(),
            by_url,
        })
    }

    /// Returns the fenced identity frozen for `url`, or `None` when it is
    /// outside this attempt's cut.
    #[must_use]
    pub(crate) fn destination(&self, url: &Url) -> Option<AnalyticalDestination> {
        self.by_url.get(url).cloned()
    }

    /// Returns every endpoint this attempt may address.
    #[must_use]
    pub(crate) fn urls(&self) -> Vec<Url> {
        self.by_url.keys().cloned().collect()
    }

    /// Returns the signed wire form stamped into every ticket.
    #[must_use]
    pub(crate) fn wire(&self) -> &[StageParticipantV1] {
        &self.wire
    }

    /// Reports whether this exact node incarnation is a member of the cut.
    ///
    /// Identity is the node *and* its fence: a participant that restarted under
    /// a new fence is a different incarnation and is simply not in this cut, so
    /// a stage message it presents is refused rather than accepted as its
    /// predecessor's.
    #[must_use]
    pub(crate) fn contains(&self, node_id: NodeId, fence: u64) -> bool {
        self.by_url
            .values()
            .any(|destination| destination.node_id == node_id && destination.fence == fence)
    }

    /// Returns a membership-only digest of this cut.
    ///
    /// Computed over the same canonical ordering the leader signs, so two
    /// tickets carrying the same membership fingerprint identically no matter
    /// which order they listed it in, and any added, removed, or re-fenced
    /// participant changes the digest. This is what lets a graph retain the
    /// exact cut its first verified ticket carried and refuse a later message
    /// that widens it, without storing a second copy of the cut per message.
    #[must_use]
    pub(crate) fn fingerprint(&self) -> String {
        let mut ordered = self.wire.clone();
        canonically_order(&mut ordered);
        let mut digest = Sha256::new();
        digest.update(b"wyrd-oracle-analytical-participant-cut-v1");
        digest.update((ordered.len() as u64).to_be_bytes());
        for participant in &ordered {
            digest.update((participant.node_id.len() as u64).to_be_bytes());
            digest.update(&participant.node_id);
            digest.update(participant.fence.to_be_bytes());
            digest.update((participant.address.len() as u64).to_be_bytes());
            digest.update(participant.address.as_bytes());
            digest.update((participant.reservation_id.len() as u64).to_be_bytes());
            digest.update(participant.reservation_id.as_bytes());
        }
        hex::encode(digest.finalize())
    }
}

/// Orders one cut so its encoding depends only on membership.
///
/// The signed claims — and therefore the signature — must not vary with the
/// iteration order of the map a leader froze its cut from, and the same order is
/// what makes [`AnalyticalParticipantCut::fingerprint`] comparable across two
/// tickets. One ordering serves both; there is no second one.
fn canonically_order(wire: &mut [StageParticipantV1]) {
    wire.sort_by(|left, right| {
        (&left.address, &left.node_id, left.fence).cmp(&(
            &right.address,
            &right.node_id,
            right.fence,
        ))
    });
}

pub(crate) struct AnalyticalStageMinter {
    /// Server-owned authority holding the signing key.
    authority: Arc<dyn OracleStageAuthority>,
    /// Target follower's node identity, signed as the ticket audience.
    destination_node_id: NodeId,
    /// Target follower's role fence, signed to defeat post-restart replay.
    destination_fence: u64,
    /// Absolute wall-clock deadline of the graph, signed on every operation.
    absolute_deadline_ms: i64,
    /// Ticket lifetime, kept far shorter than the graph's own deadline.
    ticket_ttl: chrono::Duration,
    /// Frozen cut stamped into every ticket so the receiver inherits it.
    cut: Arc<AnalyticalParticipantCut>,
}

impl fmt::Debug for AnalyticalStageMinter {
    /// Reports the audience without rendering the authority or key material.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalStageMinter")
            .field("destination_node_id", &self.destination_node_id)
            .field("destination_fence", &self.destination_fence)
            .finish_non_exhaustive()
    }
}

impl AnalyticalStageMinter {
    /// Creates the minter for one target follower.
    #[must_use]
    pub(crate) fn new(
        authority: Arc<dyn OracleStageAuthority>,
        destination_node_id: NodeId,
        destination_fence: u64,
        absolute_deadline_ms: i64,
        ticket_ttl: chrono::Duration,
        cut: Arc<AnalyticalParticipantCut>,
    ) -> Self {
        Self {
            authority,
            destination_node_id,
            destination_fence,
            absolute_deadline_ms,
            ticket_ttl,
            cut,
        }
    }

    /// Signs one single-use ticket over the exact framed message being sent.
    ///
    /// The identity is read back from the metadata the caller already wrote, so
    /// the signed claims and the wire echo cannot disagree by construction.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Claims`] when the identity metadata is
    /// absent or unparseable, [`PeerSecurityError::Body`] when the framed
    /// message is empty or oversized, and the authority's own encoding failure
    /// when the claims cannot be signed.
    pub(crate) fn mint(
        &self,
        operation: StageOperationV1,
        headers: &HeaderMap,
        framed_message: &[u8],
        now: DateTime<Utc>,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        let identity = StageWireIdentity::read(headers)?;
        let binding =
            identity.to_binding(operation, self.destination_node_id, self.destination_fence);
        let body_digest = stage_body_digest(framed_message)?;
        let claims = StageTicketClaims::for_binding(
            &binding,
            body_digest,
            fresh_nonce(),
            self.absolute_deadline_ms,
            (now + self.ticket_ttl).timestamp_millis(),
            self.cut.wire().to_vec(),
        );
        self.authority.mint_stage(operation, &claims)
    }
}

/// Produces one unguessable single-use nonce for a stage ticket.
///
/// A v4 UUID's bytes are the repository's existing single-use nonce shape; the
/// replay cache only requires that the value never repeat.
fn fresh_nonce() -> Vec<u8> {
    Uuid::new_v4().as_bytes().to_vec()
}

/// Tower layer that signs governed stage operations leaving this coordinator.
#[derive(Clone)]
pub(crate) struct AnalyticalStageMintLayer {
    /// Shared minter for the one follower this channel targets.
    minter: Arc<AnalyticalStageMinter>,
}

impl AnalyticalStageMintLayer {
    /// Creates the layer for one target follower's channel.
    #[must_use]
    pub(crate) fn new(minter: Arc<AnalyticalStageMinter>) -> Self {
        Self { minter }
    }
}

impl<S> tower::Layer<S> for AnalyticalStageMintLayer {
    /// The minting service wrapping one worker channel.
    type Service = AnalyticalStageMint<S>;

    /// Wraps `inner` so every governed request leaves signed.
    fn layer(&self, inner: S) -> Self::Service {
        AnalyticalStageMint {
            inner,
            minter: Arc::clone(&self.minter),
        }
    }
}

/// Signs the first message of every governed request before it is sent.
#[derive(Clone)]
pub(crate) struct AnalyticalStageMint<S> {
    /// The channel this layer wraps.
    inner: S,
    /// Shared minter for the one follower this channel targets.
    minter: Arc<AnalyticalStageMinter>,
}

impl<S, B> tower::Service<Request<B>> for AnalyticalStageMint<S>
where
    S: tower::Service<Request<TonicBody>, Response = Response<TonicBody>> + Clone + Send + 'static,
    S::Future: Send,
    B: Body<Data = Bytes> + Unpin + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// The wrapped channel's response, unchanged on the success path.
    type Response = Response<TonicBody>;
    /// The wrapped channel's error, unchanged.
    type Error = S::Error;
    /// Boxed because binding the first message requires reading the body.
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Delegates readiness to the wrapped channel.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Signs the first message of a governed request, then forwards it.
    ///
    /// An ungoverned path is forwarded with an empty replay, which is a
    /// structural no-op: the body is not read and its frames pass through. The
    /// replayed body is re-boxed into the transport's own body type, so the
    /// wrapped channel is the stock upstream client with nothing else changed.
    fn call(&mut self, request: Request<B>) -> Self::Future {
        // Tower's contract binds one `poll_ready` reservation to the *next*
        // `call` on that exact service value. A `tonic::transport::Channel`
        // enforces it: its buffered sender panics with "`send_item` called
        // without first calling `poll_reserve`" when a request arrives on a
        // clone that never reserved a slot. Because this layer's work is
        // asynchronous, the future — not `self` — must own the readied service,
        // so the readied value is moved out and an unreadied clone is left in
        // its place for the next `poll_ready`.
        let unreadied = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, unreadied);
        let minter = Arc::clone(&self.minter);
        Box::pin(async move {
            let Some(operation) = governed_operation(request.uri().path()) else {
                return inner.call(forward(request, VecDeque::new())).await;
            };
            let path = request.uri().path().to_owned();
            let (parts, mut body) = request.into_parts();
            let (framed, replay) = match read_first_message(&mut body, MAX_STAGE_BODY_BYTES).await {
                Ok(bound) => bound,
                Err(error) => {
                    tracing::warn!(path = %path, error = %error, reason = "framing", "Oracle analytical stage egress refused a request");
                    return Ok(Response::refused());
                }
            };
            let mut request = Request::from_parts(parts, body);
            let ticket = match minter.mint(operation, request.headers(), &framed, Utc::now()) {
                Ok(ticket) => ticket,
                Err(error) => {
                    tracing::warn!(path = %path, error = %error, reason = "mint", "Oracle analytical stage egress refused a request");
                    return Ok(Response::refused());
                }
            };
            if let Err(error) = write_ticket(request.headers_mut(), &ticket) {
                tracing::warn!(path = %path, error = %error, reason = "header", "Oracle analytical stage egress refused a request");
                return Ok(Response::refused());
            }
            inner.call(forward(request, replay)).await
        })
    }
}

/// Rebuilds one outbound request over the transport's own body type.
///
/// The replayed head and the untouched remainder are re-boxed together, so the
/// wrapped upstream client observes exactly the request it built plus the stage
/// metadata this layer added.
fn forward<B>(request: Request<B>, replay: VecDeque<Bytes>) -> Request<TonicBody>
where
    B: Body<Data = Bytes> + Unpin + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let (parts, body) = request.into_parts();
    Request::from_parts(parts, TonicBody::new(ReplayBody::new(replay, body)))
}

/// A transport response that can carry a closed stage refusal.
///
/// Both layers refuse by *responding*, never by erroring: the wrapped channel's
/// and server's error types are opaque and cannot be constructed here, and a
/// gRPC status is what a caller can actually interpret.
pub(crate) trait RefusalResponse {
    /// Builds the closed `PermissionDenied` refusal.
    fn refused() -> Self;
}

impl<B: Default> RefusalResponse for Response<B> {
    /// Builds the closed refusal over this response's own body type.
    fn refused() -> Self {
        refusal()
    }
}

/// Installs [`AnalyticalStageAuth`] over an upstream worker service.
///
/// This is the follower half of the stage-operation authority. It is layered
/// directly onto upstream's own unmodified generated service, so it observes
/// the request while it is still raw HTTP: before tonic's decoder, before the
/// worker's task cache, before provider construction, and before any I/O.
#[derive(Clone)]
pub struct AnalyticalStageAuthLayer {
    /// Follower-owned admission and authority entry point for stage operations.
    ingress: Arc<AnalyticalStageIngress>,
}

impl AnalyticalStageAuthLayer {
    /// Builds the layer over the follower ingress that owns stage admission.
    #[must_use]
    pub fn new(ingress: Arc<AnalyticalStageIngress>) -> Self {
        Self { ingress }
    }
}

impl AnalyticalStageAuthLayer {
    /// Wraps one upstream worker service without importing [`tower::Layer`].
    ///
    /// The trait method is the same thing; this inherent alias exists so a
    /// mounting crate does not have to bring the trait into scope for a single
    /// call site.
    #[must_use]
    pub fn layer_service<S>(&self, inner: S) -> AnalyticalStageAuth<S> {
        <Self as tower::Layer<S>>::layer(self, inner)
    }
}

impl<S> tower::Layer<S> for AnalyticalStageAuthLayer {
    type Service = AnalyticalStageAuth<S>;

    /// Wraps one upstream worker service in the stage-authority check.
    fn layer(&self, inner: S) -> Self::Service {
        AnalyticalStageAuth {
            inner,
            ingress: Arc::clone(&self.ingress),
        }
    }
}

/// Authorizes each governed stage operation against its exact raw first message.
///
/// The service reads exactly one complete gRPC message from the request body,
/// bounded as it reads, and hands those bytes to
/// [`AnalyticalStageIngress::authorize_stage_message`] as the digest input. Only
/// when that returns does the request continue to the upstream service, with the
/// buffered bytes replayed verbatim by [`ReplayBody`] so upstream decodes the
/// same message the ticket committed to. For `CoordinatorChannel` the rest of
/// the stream is never buffered: it is passed through live, preserving frame
/// independence, trailers, body errors, cancellation, and backpressure.
///
/// Every failure path — an unsupported method, malformed or incomplete framing,
/// an oversized message, a body error, an absent or invalid ticket, a binding or
/// digest mismatch, a replayed nonce, or a refused admission — returns the closed
/// `PermissionDenied` refusal without calling the inner service at all.
#[derive(Clone)]
pub struct AnalyticalStageAuth<S> {
    /// Upstream's own unmodified worker service.
    inner: S,
    /// Follower-owned admission and authority entry point for stage operations.
    ingress: Arc<AnalyticalStageIngress>,
}

impl<S> wyrd_tonic::tonic::server::NamedService for AnalyticalStageAuth<S>
where
    S: wyrd_tonic::tonic::server::NamedService,
{
    /// The layer is transparent to routing: it serves upstream's own service name.
    const NAME: &'static str = S::NAME;
}

impl<S, B> tower::Service<Request<B>> for AnalyticalStageAuth<S>
where
    S: tower::Service<Request<ReplayBody<B>>> + Clone + Send + 'static,
    S::Future: Send,
    S::Response: RefusalResponse,
    B: Body<Data = Bytes> + Unpin + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Defers readiness to the upstream service; the check itself is per-request.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Authorizes the first raw message, then forwards it replayed and intact.
    fn call(&mut self, request: Request<B>) -> Self::Future {
        // Same Tower readiness contract as `AnalyticalStageMint::call`: the
        // future must own the service value that was polled ready, so the
        // readied value is moved out and an unreadied clone takes its place.
        let unreadied = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, unreadied);
        let ingress = Arc::clone(&self.ingress);
        Box::pin(async move {
            let path = request.uri().path().to_owned();
            // The peer plane's authentication layer runs before any body is
            // polled and attaches exactly one context. Its absence means this
            // adapter was reached outside that boundary, so the request is
            // refused rather than authorized on stage identity alone: workload
            // identity and stage authority are separate checks and the second
            // never substitutes for the first.
            if request
                .extensions()
                .get::<crate::oracle::peer::AuthenticatedPeerContext>()
                .is_none()
            {
                tracing::warn!(path = %path, reason = "unauthenticated_peer", "Oracle analytical stage ingress refused a request");
                return Ok(S::Response::refused());
            }
            let Some(operation) = governed_operation(&path) else {
                tracing::warn!(path = %path, reason = "ungoverned_path", "Oracle analytical stage ingress refused a request");
                return Ok(S::Response::refused());
            };
            let (parts, mut body) = request.into_parts();
            let framing = read_first_message(&mut body, MAX_STAGE_BODY_BYTES).await;
            let (framed, replay) = match framing {
                Ok(bound) => bound,
                Err(error) => {
                    tracing::warn!(path = %path, error = %error, reason = "framing", "Oracle analytical stage ingress refused a request");
                    return Ok(S::Response::refused());
                }
            };
            let authorized = ingress
                .authorize_stage_message(operation, &parts.headers, &framed, Utc::now())
                .await;
            let key = match authorized {
                Ok(key) => key,
                Err(error) => {
                    tracing::warn!(path = %path, error = %error, reason = "authority", "Oracle analytical stage ingress refused a request");
                    return Ok(S::Response::refused());
                }
            };
            // Only the coordinator channel carries ownership. Its request stream
            // lives for the whole call, so its drop is the coordinator's
            // departure; a unary `ExecuteTask` body is consumed and dropped long
            // before the stage it started has finished producing rows.
            let lease = match operation {
                StageOperationV1::SetPlan => match ingress.retain_connection(key.graph()) {
                    Ok(lease) => Some(lease),
                    Err(error) => {
                        tracing::warn!(path = %path, error = %error, reason = "ownership", "Oracle analytical stage ingress refused a request");
                        return Ok(S::Response::refused());
                    }
                },
                StageOperationV1::ExecuteTask => None,
            };
            inner
                .call(replay_request(
                    Request::from_parts(parts, body),
                    replay,
                    lease,
                ))
                .await
        })
    }
}

/// The query-invariant half of one coordinator's stage identity.
///
/// A leader resolves these once when it selects Analytical execution for a
/// query: they name the graph, the tenant, the coordinator's own fenced
/// identity, the pinned cut, the reservation the work charges, and the attempt.
/// Only the stage and task ordinals vary between calls, and those are taken
/// from upstream's own [`TaskKey`] rather than from anything a caller supplies.
#[derive(Debug, Clone)]
pub(crate) struct AnalyticalCoordinatorIdentity {
    /// This coordinator's own node identity.
    pub(crate) source_node_id: NodeId,
    /// This coordinator's own current Oracle role fence.
    pub(crate) source_fence: u64,
    /// Authenticated data tenant of the query.
    pub(crate) tenant_id: DataTenantId,
    /// The two-identity graph every stage operation belongs to.
    pub(crate) graph: AnalyticalGraphKey,
    /// Pinned snapshot digest of the graph's immutable cut.
    pub(crate) snapshot_digest: String,
    /// Attempt ordinal, identical across every stage of one attempt.
    pub(crate) attempt: u32,
    /// Reservation the graph's work charges against.
    pub(crate) reservation_id: String,
    /// Digest of the leader-authorized permissions for this query.
    pub(crate) permission_digest: String,
}

impl AnalyticalCoordinatorIdentity {
    /// Rebinds this coordinator identity to one destination's own reservation.
    ///
    /// Everything else is query-invariant. Only the reservation differs per
    /// follower, because each follower granted its own and will only honour
    /// that one.
    #[must_use]
    fn for_destination(&self, reservation_id: String) -> Self {
        Self {
            reservation_id,
            tenant_id: self.tenant_id,
            graph: self.graph,
            snapshot_digest: self.snapshot_digest.clone(),
            permission_digest: self.permission_digest.clone(),
            source_node_id: self.source_node_id,
            source_fence: self.source_fence,
            attempt: self.attempt,
        }
    }

    /// Projects the full wire identity for one addressed stage task.
    #[must_use]
    fn for_task(&self, task_key: TaskKey) -> StageWireIdentity {
        StageWireIdentity {
            source_node_id: self.source_node_id,
            source_fence: self.source_fence,
            tenant_id: self.tenant_id,
            graph: self.graph,
            snapshot_digest: self.snapshot_digest.clone(),
            stage_id: u32::try_from(task_key.stage_id).unwrap_or(u32::MAX),
            task_id: Some(u32::try_from(task_key.task_number).unwrap_or(u32::MAX)),
            attempt: self.attempt,
            reservation_id: self.reservation_id.clone(),
            permission_digest: self.permission_digest.clone(),
        }
    }
}

/// Adds the Wyrd stage identity to every call on one upstream worker client.
///
/// The identity cannot be recovered from the raw bytes by the signing layer
/// without decoding upstream's protobuf, which would be a second serialization
/// bridge. It is instead written here, where upstream has already told us the
/// [`TaskKey`] it is addressing, and the mint layer beneath then signs the
/// headers together with the exact message they accompany.
pub(crate) struct AnalyticalWorkerChannel {
    /// The stock upstream client for the target worker.
    inner: Box<dyn WorkerChannel>,
    /// Query-invariant identity every call on this channel carries.
    identity: Arc<AnalyticalCoordinatorIdentity>,
}

impl AnalyticalWorkerChannel {
    /// Wraps one upstream client in this query's coordinator identity.
    #[must_use]
    pub(crate) fn new(
        inner: Box<dyn WorkerChannel>,
        identity: Arc<AnalyticalCoordinatorIdentity>,
    ) -> Self {
        Self { inner, identity }
    }

    /// Writes this query's stage identity for `task_key` onto `headers`.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Execution`] when a component of the identity
    /// cannot be represented on the transport, refusing to send a partial
    /// binding that the follower would reject anyway.
    fn stamp(&self, headers: &mut HeaderMap, task_key: TaskKey) -> Result<(), DataFusionError> {
        self.identity
            .for_task(task_key)
            .write(headers)
            .map_err(|_| {
                DataFusionError::Execution(
                    "Oracle analytical stage identity could not be encoded".to_owned(),
                )
            })
    }
}

#[async_trait]
impl WorkerChannel for AnalyticalWorkerChannel {
    /// Stamps and delegates the stage's plan installation.
    async fn coordinator_channel(
        &mut self,
        mut headers: HeaderMap,
        set_plan_request: SetPlanRequest,
        c2w_stream: BoxStream<'static, CoordinatorToWorkerMsg>,
        metrics: ExecutionPlanMetricsSet,
        task_ctx: &Arc<TaskContext>,
    ) -> Result<BoxStream<'static, Result<WorkerToCoordinatorMsg, DataFusionError>>, DataFusionError>
    {
        self.stamp(&mut headers, set_plan_request.task_key)?;
        self.inner
            .coordinator_channel(headers, set_plan_request, c2w_stream, metrics, task_ctx)
            .await
    }

    /// Stamps and delegates one task execution.
    async fn execute_task(
        &mut self,
        mut headers: HeaderMap,
        request: ExecuteTaskRequest,
        metrics: ExecutionPlanMetricsSet,
        task_ctx: &Arc<TaskContext>,
    ) -> Result<Vec<BoxStream<'static, Result<RecordBatch, DataFusionError>>>, DataFusionError>
    {
        self.stamp(&mut headers, request.task_key)?;
        let partitions = self
            .inner
            .execute_task(headers, request, metrics, task_ctx)
            .await?;
        Ok(partitions.into_iter().map(measured_exchange).collect())
    }

    /// Refuses worker version discovery, which Wyrd never performs.
    ///
    /// The call carries no graph identity, so it can be bound to no stage
    /// ticket and authorized by nothing: the ingress path already refuses it as
    /// an ungoverned method. Refusing it here too closes the other direction —
    /// nothing inside Wyrd may originate it — so the method has no internal
    /// caller rather than merely no successful one. Wyrd pins one worker build
    /// across a topology, so there is nothing a follower could report that its
    /// coordinator does not already know.
    ///
    /// # Errors
    ///
    /// Always returns [`DataFusionError::Execution`].
    async fn get_worker_info(
        &mut self,
        _request: GetWorkerInfoRequest,
    ) -> Result<GetWorkerInfoResponse, DataFusionError> {
        Err(DataFusionError::Execution(
            "Oracle analytical workers do not answer worker discovery".to_owned(),
        ))
    }
}

/// Counts one follower partition's batches and bytes as they cross the exchange.
///
/// This is the only place a coordinator sees follower result data arrive, so it
/// is the only honest place to measure the exchange. The count is taken as each
/// batch is yielded rather than by collecting the stream, so measurement does
/// not change the stream's laziness, backpressure, or cancellation behavior.
///
/// Bytes are the batch's in-memory Arrow footprint, which is what the
/// coordinator's exchange budget is actually charged for; it is deliberately not
/// the encoded wire size, which no owner here bounds.
fn measured_exchange(
    partition: BoxStream<'static, Result<RecordBatch, DataFusionError>>,
) -> BoxStream<'static, Result<RecordBatch, DataFusionError>> {
    Box::pin(partition.inspect(|batch| {
        if let Ok(batch) = batch {
            record_exchange_transfer(1, batch.get_array_memory_size() as u64);
        }
    }))
}

/// The signing authority and lifetimes one coordinator mints every stage
/// ticket under.
///
/// Grouped because the channel resolver only ever passes these three together
/// into the per-destination minter it builds, and because the deadline and the
/// ticket lifetime are attempt-invariant while the destination is not.
pub(crate) struct AnalyticalStageSigning {
    /// Server-owned authority holding this node's stage signing key.
    pub(crate) authority: Arc<dyn OracleStageAuthority>,
    /// Absolute wall-clock deadline of the graph, identical across attempts.
    pub(crate) absolute_deadline_ms: i64,
    /// Ticket lifetime, kept far shorter than the graph's own deadline.
    pub(crate) ticket_ttl: chrono::Duration,
}

/// Tower layer that presents this node's peer workload credential outbound.
///
/// The private listener authenticates the workload credential before it polls
/// a request body, so an east-west Analytical channel that carried only a peer
/// certificate and a stage ticket would be refused before its ticket was ever
/// read. Transport identity, workload identity, and operation authority are
/// three separate proofs, and this layer supplies the second one.
#[derive(Clone)]
pub(crate) struct AnalyticalPeerCredentialLayer {
    /// Source of this node's short-lived peer workload bearer.
    credentials: Arc<dyn OraclePeerCredentials>,
}

impl AnalyticalPeerCredentialLayer {
    /// Creates the layer over this node's own peer credentials.
    #[must_use]
    pub(crate) fn new(credentials: Arc<dyn OraclePeerCredentials>) -> Self {
        Self { credentials }
    }
}

impl<S> tower::Layer<S> for AnalyticalPeerCredentialLayer {
    /// The credential-presenting service wrapping one worker channel.
    type Service = AnalyticalPeerCredential<S>;

    /// Wraps `inner` so every request leaves carrying the peer credential.
    fn layer(&self, inner: S) -> Self::Service {
        AnalyticalPeerCredential {
            inner,
            credentials: Arc::clone(&self.credentials),
        }
    }
}

/// Stamps this node's peer workload credential onto every outbound request.
#[derive(Clone)]
pub(crate) struct AnalyticalPeerCredential<S> {
    /// The channel this layer wraps.
    inner: S,
    /// Source of this node's short-lived peer workload bearer.
    credentials: Arc<dyn OraclePeerCredentials>,
}

impl<S, B> tower::Service<Request<B>> for AnalyticalPeerCredential<S>
where
    S: tower::Service<Request<B>, Response = Response<TonicBody>> + Clone + Send + 'static,
    S::Future: Send,
    B: Send + 'static,
{
    /// The wrapped channel's response, unchanged on the success path.
    type Response = Response<TonicBody>;
    /// The wrapped channel's error, unchanged.
    type Error = S::Error;
    /// Boxed because acquiring the bearer is asynchronous.
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Delegates readiness to the wrapped channel.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Acquires the current bearer, stamps it, and forwards the request.
    ///
    /// A credential this node cannot acquire or render as metadata refuses the
    /// request by responding, exactly as the stage layers do: the wrapped
    /// channel's error type is opaque here, and a closed refusal is what the
    /// caller can interpret. Refusing before sending is also correct on its own
    /// terms, since the destination would refuse the unauthenticated request.
    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        // Same Tower readiness contract as `AnalyticalStageMint::call`: the
        // future owns the service value that was polled ready, and an unreadied
        // clone takes its place.
        let unreadied = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, unreadied);
        let credentials = Arc::clone(&self.credentials);
        Box::pin(async move {
            let bearer = match credentials.bearer(false).await {
                Ok(bearer) => bearer,
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        reason = "credential",
                        "Oracle analytical peer egress refused a request"
                    );
                    return Ok(Response::refused());
                }
            };
            let Ok(value) = HeaderValue::from_str(&format!("Bearer {bearer}")) else {
                tracing::warn!(
                    reason = "credential_metadata",
                    "Oracle analytical peer egress refused a request"
                );
                return Ok(Response::refused());
            };
            request.headers_mut().insert(PEER_CREDENTIAL_HEADER, value);
            inner.call(request).await
        })
    }
}

/// Metadata key the Wyrd workload bearer travels in on every peer request.
const PEER_CREDENTIAL_HEADER: HeaderName = HeaderName::from_static("x-wyrd-access-token");

/// Establishes and reuses mutually authenticated channels to Analytical peers.
///
/// Every east-west channel Wyrd opens is dialed through the Bifrost peer
/// identity, so a follower sees a client certificate chaining to the peer CA
/// before it sees a request. Upstream's own connection cache is deliberately
/// not used: it dials each worker URL with an anonymous channel, which the
/// private peer listener refuses at the TLS handshake.
///
/// Connections are cached for the lifetime of the resolver, which is one
/// query's coordinator side. A dial that fails is not cached, so a transient
/// peer outage does not poison the rest of the query.
struct AnalyticalPeerChannels {
    /// Immutable peer transport identity every dial presents.
    tls: BifrostPeerTls,
    /// Workload credential every request on a dialed channel presents.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Connected channels, keyed by the worker URL they were dialed for.
    channels: tokio::sync::Mutex<HashMap<Url, BoxCloneSyncChannel>>,
}

impl AnalyticalPeerChannels {
    /// Builds an empty channel cache bound to one peer identity.
    fn new(tls: BifrostPeerTls, credentials: Arc<dyn OraclePeerCredentials>) -> Self {
        Self {
            tls,
            credentials,
            channels: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Returns the mutually authenticated channel for `url`, dialing it once.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Execution`] when the peer endpoint cannot be
    /// built from the configured identity or the connection cannot be
    /// established. Cancelling the returned future abandons an in-progress dial
    /// without caching it.
    async fn channel(&self, url: &Url) -> Result<BoxCloneSyncChannel, DataFusionError> {
        let mut channels = self.channels.lock().await;
        if let Some(channel) = channels.get(url) {
            return Ok(channel.clone());
        }
        let endpoint = self.tls.endpoint(url.to_string()).map_err(|error| {
            DataFusionError::Execution(format!(
                "Oracle analytical peer endpoint for {url} is invalid: {error}"
            ))
        })?;
        let channel = endpoint.connect().await.map_err(|error| {
            DataFusionError::Execution(format!(
                "Oracle analytical peer connection to {url} failed: {error}"
            ))
        })?;
        let channel = BoxCloneSyncChannel::new(
            AnalyticalPeerCredentialLayer::new(Arc::clone(&self.credentials)).layer(channel),
        );
        channels.insert(url.clone(), channel.clone());
        Ok(channel)
    }
}

/// Resolves worker clients that sign every governed stage operation they send.
///
/// This owner is the coordinator's complete outbound authority for one query:
/// it dials each follower through the Bifrost peer identity, signs beneath the
/// client, and stamps the coordinator identity above it, so a follower cannot
/// be reached anonymously or with an unsigned stage operation.
pub(crate) struct AnalyticalChannelResolver {
    /// Mutually authenticated connection cache for this query's followers.
    channels: AnalyticalPeerChannels,
    /// Query-invariant identity every call through this resolver carries.
    ///
    /// Everything but the reservation: that is the destination's own, and is
    /// substituted per follower when the channel to it is resolved.
    identity: Arc<AnalyticalCoordinatorIdentity>,
    /// Per-follower minter and identity, keyed by the follower each is bound to.
    minters: Arc<std::sync::Mutex<HashMap<Url, AnalyticalDestinationChannel>>>,
    /// Frozen destination set this resolver may address, and nothing beyond it.
    cut: Arc<AnalyticalParticipantCut>,
    /// Authority and lifetimes every minted ticket is signed under.
    signing: AnalyticalStageSigning,
}

impl fmt::Debug for AnalyticalChannelResolver {
    /// Reports the graph without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalChannelResolver")
            .field("graph", &self.identity.graph)
            .finish_non_exhaustive()
    }
}

impl AnalyticalChannelResolver {
    /// Builds the resolver for one query's coordinator identity and frozen cut.
    ///
    /// `cut` is the only source of destinations. A URL outside it resolves to
    /// no minter, which fails the resolution closed — locally, before any
    /// connection is attempted — rather than sending an unsigned operation or
    /// dialing a node this attempt never froze.
    #[must_use]
    pub(crate) fn new(
        identity: Arc<AnalyticalCoordinatorIdentity>,
        tls: BifrostPeerTls,
        credentials: Arc<dyn OraclePeerCredentials>,
        cut: Arc<AnalyticalParticipantCut>,
        signing: AnalyticalStageSigning,
    ) -> Self {
        Self {
            channels: AnalyticalPeerChannels::new(tls, credentials),
            identity,
            minters: Arc::new(std::sync::Mutex::new(HashMap::new())),
            cut,
            signing,
        }
    }

    /// Returns the minter and identity bound to `url`, resolving them once.
    ///
    /// The identity is per destination because the reservation is: this
    /// coordinator charges each follower against that follower's own frozen
    /// reservation, so one query's channels carry different reservation
    /// identities and each stamps only the one its audience granted.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Execution`] when the cache lock is poisoned
    /// or `url` is not a destination this attempt's frozen cut contains.
    fn resolve(&self, url: &Url) -> Result<AnalyticalDestinationChannel, DataFusionError> {
        let mut minters = self.minters.lock().map_err(|_| {
            DataFusionError::Execution("Oracle analytical minter cache is poisoned".to_owned())
        })?;
        if let Some(resolved) = minters.get(url) {
            return Ok(resolved.clone());
        }
        let destination = self.cut.destination(url).ok_or_else(|| {
            DataFusionError::Execution(format!(
                "Oracle analytical execution has no authorized destination identity for {url}"
            ))
        })?;
        let resolved = AnalyticalDestinationChannel {
            minter: Arc::new(AnalyticalStageMinter::new(
                Arc::clone(&self.signing.authority),
                destination.node_id,
                destination.fence,
                self.signing.absolute_deadline_ms,
                self.signing.ticket_ttl,
                Arc::clone(&self.cut),
            )),
            identity: Arc::new(
                self.identity
                    .for_destination(destination.reservation_id.clone()),
            ),
        };
        minters.insert(url.clone(), resolved.clone());
        Ok(resolved)
    }
}

/// The signing owners one coordinator uses for exactly one destination.
#[derive(Clone)]
pub(crate) struct AnalyticalDestinationChannel {
    /// Ticket minter bound to that destination's node identity and fence.
    minter: Arc<AnalyticalStageMinter>,
    /// Coordinator identity carrying that destination's own reservation.
    identity: Arc<AnalyticalCoordinatorIdentity>,
}

#[async_trait]
impl ChannelResolver for AnalyticalChannelResolver {
    /// Builds a signing, identity-stamping client for one follower URL.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Execution`] when no authorized destination
    /// identity exists for `url`, or when the mutually authenticated channel to
    /// `url` cannot be established.
    async fn get_worker_client_for_url(
        &self,
        url: &Url,
    ) -> Result<Box<dyn WorkerChannel>, DataFusionError> {
        let resolved = self.resolve(url)?;
        let channel = self.channels.channel(url).await?;
        let signed =
            BoxCloneSyncChannel::new(AnalyticalStageMintLayer::new(resolved.minter).layer(channel));
        Ok(Box::new(AnalyticalWorkerChannel::new(
            create_worker_client(signed),
            resolved.identity,
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use async_trait::async_trait;
    use prost::Message as _;
    use tower::{Layer as _, Service as _};

    use super::super::analytical::{
        AnalyticalStageIngressConfig, DataFusionQueryId, PublicQueryId,
    };
    use super::super::peer::AuthorizedStage;
    use super::super::spill::OracleSpillRuntime;
    use super::*;

    /// Builds one https destination entry for a cut fixture.
    fn destination(port: u16, node: u128, fence: u64) -> (Url, AnalyticalDestination) {
        (
            Url::parse(&format!("https://127.0.0.1:{port}/")).expect("fixture endpoint parses"),
            AnalyticalDestination {
                node_id: NodeId::new(Uuid::from_u128(node)),
                fence,
                reservation_id: format!("reservation-{node}"),
            },
        )
    }

    /// Reports whether two frozen destinations name the same identity.
    fn same_destination(
        left: Option<AnalyticalDestination>,
        right: &AnalyticalDestination,
    ) -> bool {
        left.is_some_and(|left| {
            left.node_id == right.node_id
                && left.fence == right.fence
                && left.reservation_id == right.reservation_id
        })
    }

    /// A frozen cut is the complete and only addressable destination set.
    ///
    /// Locks the three properties the immutability claim rests on: a cut round
    /// trips through its signed wire form unchanged, an endpoint outside it
    /// resolves to nothing at all rather than to a live lookup, and a plaintext
    /// endpoint never enters a cut in either direction. The last is what keeps
    /// a coordinator from being talked down to an unauthenticated channel by
    /// the contents of a ticket it received.
    #[test]
    fn a_frozen_cut_is_the_only_addressable_destination_set() {
        let (url, identity) = destination(50052, 1, 4);
        let cut =
            AnalyticalParticipantCut::freeze(HashMap::from([(url.clone(), identity.clone())]))
                .expect("an https cut freezes");

        assert!(same_destination(cut.destination(&url), &identity));
        assert_eq!(cut.urls(), vec![url.clone()]);
        let outside = Url::parse("https://127.0.0.1:50053/").expect("fixture endpoint parses");
        assert!(
            cut.destination(&outside).is_none(),
            "an endpoint outside the cut must resolve to no destination"
        );

        let adopted = AnalyticalParticipantCut::adopt(cut.wire()).expect("a signed cut is adopted");
        assert!(same_destination(adopted.destination(&url), &identity));
        assert!(
            adopted.destination(&outside).is_none(),
            "adoption must not widen the cut it received"
        );

        let plaintext =
            Url::parse("http://127.0.0.1:50052/").expect("fixture plaintext endpoint parses");
        assert!(
            AnalyticalParticipantCut::freeze(HashMap::from([(plaintext, identity.clone())]))
                .is_err(),
            "a plaintext endpoint must never enter a frozen cut"
        );
        assert!(
            AnalyticalParticipantCut::adopt(&[StageParticipantV1 {
                node_id: Uuid::from_u128(1).as_bytes().to_vec(),
                fence: 4,
                address: "http://127.0.0.1:50052/".to_owned(),
                reservation_id: identity.reservation_id,
            }])
            .is_err(),
            "a plaintext endpoint must never be adopted from a ticket"
        );
    }

    /// A cut's signed form depends on its membership, never on iteration order.
    ///
    /// The wire form is signed, so two coordinators that froze the same
    /// participants must produce byte-identical claims; a `HashMap` iteration
    /// order leaking into the signature would make an otherwise identical
    /// ticket verify or not by chance.
    #[test]
    fn a_frozen_cut_encodes_independently_of_iteration_order() {
        let entries = [
            destination(50052, 1, 4),
            destination(50053, 2, 5),
            destination(50054, 3, 6),
        ];
        let forward =
            AnalyticalParticipantCut::freeze(entries.iter().cloned().collect()).expect("freezes");
        let reversed = AnalyticalParticipantCut::freeze(entries.iter().rev().cloned().collect())
            .expect("freezes");
        assert_eq!(forward.wire(), reversed.wire());
    }

    /// A cut larger than the addressable bound is refused in both directions.
    ///
    /// The bound exists so a forged claim cannot grow a receiver's destination
    /// table without limit, which only holds if the freezing side is bounded
    /// too — otherwise a coordinator could mint what no receiver may adopt.
    #[test]
    fn a_cut_beyond_the_participant_bound_is_refused() {
        let oversized = (0..=MAX_STAGE_PARTICIPANTS)
            .map(|index| {
                destination(
                    50052 + u16::try_from(index).expect("fixture index fits a port offset"),
                    u128::try_from(index).expect("fixture index fits a node identity") + 1,
                    4,
                )
            })
            .collect::<HashMap<_, _>>();
        assert!(
            AnalyticalParticipantCut::freeze(oversized.clone()).is_err(),
            "an oversized cut must not freeze"
        );
        let wire = oversized
            .into_iter()
            .map(|(url, destination)| StageParticipantV1 {
                node_id: destination.node_id.as_uuid().as_bytes().to_vec(),
                fence: destination.fence,
                reservation_id: destination.reservation_id,
                address: url.to_string(),
            })
            .collect::<Vec<_>>();
        assert!(
            AnalyticalParticipantCut::adopt(&wire).is_err(),
            "an oversized cut must not be adopted"
        );
    }

    /// gRPC path of upstream's worker metadata call.
    ///
    /// Named here rather than in production code because nothing in production
    /// dispatches on it: it carries no graph identity and no query work, so it
    /// takes no stage ticket and falls through the governed-path table's
    /// fail-closed arm like any other unrecognized method.
    const WORKER_INFO_PATH: &str = "/worker.WorkerService/GetWorkerInfo";

    /// Only the two stage-carrying gRPC paths are governed; every other path,
    /// upstream's worker-metadata call included, is refused.
    ///
    /// Pins the fail-closed direction of the path table: a method this layer
    /// does not understand must resolve to no operation, because an
    /// unrecognized path cannot be bound to a stage ticket and therefore
    /// cannot be authorized. Worker metadata is named explicitly because it is
    /// the one real upstream method that legitimately carries no graph
    /// identity; it still reaches a follower only through peer mTLS and
    /// workload authentication, like every other east-west call.
    #[test]
    fn governed_operation_names_only_the_two_stage_paths() {
        assert_eq!(
            governed_operation(COORDINATOR_CHANNEL_PATH),
            Some(StageOperationV1::SetPlan)
        );
        assert_eq!(
            governed_operation(EXECUTE_TASK_PATH),
            Some(StageOperationV1::ExecuteTask)
        );
        assert_eq!(governed_operation(WORKER_INFO_PATH), None);
        assert_eq!(governed_operation("/unknown.Service/Method"), None);
    }
    /// Body chunks a fixture request delivers before it ends.
    #[derive(Debug)]
    struct ChunkBody {
        /// Remaining data chunks, delivered one per poll.
        chunks: VecDeque<Bytes>,
        /// When set, the body yields this error instead of ending.
        fails: bool,
        /// When set, the body ends with a trailers frame instead of data.
        trailers: bool,
    }

    impl ChunkBody {
        /// Builds a body that delivers `chunks` and then ends cleanly.
        fn new(chunks: Vec<Bytes>) -> Self {
            Self {
                chunks: chunks.into(),
                fails: false,
                trailers: false,
            }
        }

        /// Builds a body that delivers `chunks` and then errors.
        fn failing(chunks: Vec<Bytes>) -> Self {
            Self {
                chunks: chunks.into(),
                fails: true,
                trailers: false,
            }
        }

        /// Builds a body that delivers `chunks` and then sends trailers.
        fn with_trailers(chunks: Vec<Bytes>) -> Self {
            Self {
                chunks: chunks.into(),
                fails: false,
                trailers: true,
            }
        }
    }

    impl Body for ChunkBody {
        type Data = Bytes;
        type Error = std::io::Error;

        /// Yields the next retained chunk, then the configured terminal frame.
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            if let Some(chunk) = self.chunks.pop_front() {
                return Poll::Ready(Some(Ok(Frame::data(chunk))));
            }
            if self.fails {
                self.fails = false;
                return Poll::Ready(Some(Err(std::io::Error::other("fixture body failed"))));
            }
            if self.trailers {
                self.trailers = false;
                return Poll::Ready(Some(Ok(Frame::trailers(HeaderMap::new()))));
            }
            Poll::Ready(None)
        }

        /// Reports end-of-stream once no chunk or terminal frame remains.
        fn is_end_stream(&self) -> bool {
            self.chunks.is_empty() && !self.fails && !self.trailers
        }
    }

    /// A body that never produces a frame, standing in for a stalled peer.
    #[derive(Debug)]
    struct PendingBody;

    impl Body for PendingBody {
        type Data = Bytes;
        type Error = std::io::Error;

        /// Never resolves, so a caller that drops the future cancels mid-buffer.
        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Pending
        }

        /// A stalled body has not ended.
        fn is_end_stream(&self) -> bool {
            false
        }
    }

    /// Records whether upstream was reached and with which bytes.
    #[derive(Debug, Default)]
    struct UpstreamProbe {
        /// Set once the inner service is polled at all.
        reached: AtomicBool,
        /// Number of governed messages the inner service actually decoded.
        messages: AtomicUsize,
    }

    /// Fixture stand-in for upstream's own generated worker service.
    ///
    /// It performs the first thing upstream would: reading the request body and
    /// decoding a framed message. Every ordering assertion in this module is a
    /// statement about whether this ran.
    #[derive(Clone)]
    struct RecordingUpstream {
        /// Shared observation of what upstream saw.
        probe: Arc<UpstreamProbe>,
        /// Bytes upstream reassembled from the forwarded body.
        seen: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl<B> tower::Service<Request<B>> for RecordingUpstream
    where
        B: Body<Data = Bytes> + Unpin + Send + 'static,
    {
        type Response = Response<()>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        /// The fixture upstream is always ready.
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        /// Drains the forwarded body and records the bytes upstream would decode.
        fn call(&mut self, request: Request<B>) -> Self::Future {
            self.probe.reached.store(true, Ordering::SeqCst);
            let probe = Arc::clone(&self.probe);
            let seen = Arc::clone(&self.seen);
            let mut body = request.into_body();
            Box::pin(async move {
                let mut collected = Vec::new();
                while let Some(frame) =
                    std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
                {
                    let Ok(frame) = frame else { break };
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                probe.messages.fetch_add(1, Ordering::SeqCst);
                *seen.lock().expect("fixture upstream lock is uncontended") = collected;
                Ok(Response::new(()))
            })
        }
    }

    /// Composes one injected, idle Oracle role for fixture graph reservations.
    ///
    /// Injected rather than observed so the reservation this fixture takes is
    /// the same on every machine.
    ///
    /// # Panics
    ///
    /// Panics when the injected observation cannot compose an active Oracle role.
    fn fixture_oracle_role() -> crate::resources::OracleResources {
        let snapshot = crate::resources::SystemResourceSnapshot {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            effective_cpu: 8,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: crate::resources::ResourceSource::Injected,
            cpu_source: crate::resources::ResourceSource::Injected,
        };
        let policy = crate::resources::BifrostResourcePolicy {
            roles: [crate::resources::BifrostRole::Oracle]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        };
        crate::resources::BifrostRuntimeResources::from_snapshot(snapshot, policy)
            .expect("an injected Oracle observation composes the production root")
            .compose_roles()
            .expect("role composition is issued from an unpoisoned root")
            .oracle()
            .expect("the Oracle role is active in this policy")
    }

    /// A stage authority that enforces exactly the real binding and digest rules.
    ///
    /// It is not a permissive stub: it recomputes the body digest and runs the
    /// production [`StageTicketClaims::verify_binding`], so a substituted body or
    /// a tampered identity is refused here for the same reason it would be in
    /// the server authority. Only signature custody is fixture-owned.
    struct FixtureAuthority {
        /// Number of authorization attempts observed.
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl OracleStageAuthority for FixtureAuthority {
        /// Encodes the claims verbatim under a fixture key and signature.
        fn mint_stage(
            &self,
            _operation: StageOperationV1,
            claims: &StageTicketClaims,
        ) -> Result<SignedPeerTicket, PeerSecurityError> {
            Ok(SignedPeerTicket {
                key_id: "fixture".to_owned(),
                claims_bytes: claims.encode_to_vec(),
                signature: vec![0; 64],
            })
        }

        /// Verifies the presented claims bind the exact received bytes.
        async fn authorize_stage(
            &self,
            ticket: &SignedPeerTicket,
            binding: &StageBinding,
            body: &[u8],
            _now: DateTime<Utc>,
        ) -> Result<AuthorizedStage, PeerSecurityError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let claims = StageTicketClaims::decode(ticket.claims_bytes.as_slice())
                .map_err(|_| PeerSecurityError::Claims)?;
            let digest = stage_body_digest(body)?;
            claims.verify_binding(binding, &digest)?;
            Ok(AuthorizedStage {
                claims,
                tenant_id: binding.tenant_id,
            })
        }
    }

    /// Everything one layer test needs to send an authorized stage operation.
    struct Fixture {
        /// The follower ingress under test.
        ingress: Arc<AnalyticalStageIngress>,
        /// Shared count of authorization attempts.
        authority_calls: Arc<AtomicUsize>,
        /// This follower's own node identity.
        node_id: NodeId,
        /// This follower's own role fence.
        fence: u64,
        /// The identity every fixture operation carries.
        identity: StageWireIdentity,
        /// Spill owner kept alive for the ingress's runtime construction.
        _spill: Arc<OracleSpillRuntime>,
        /// Scratch root kept alive for the spill owner.
        _root: tempfile::TempDir,
    }

    impl Fixture {
        /// Builds a follower ingress over a live Oracle role and fixture authority.
        ///
        /// # Panics
        ///
        /// Panics when the injected observation cannot compose an Oracle role or
        /// the pod spill owner cannot be created.
        fn new() -> Self {
            let root = tempfile::tempdir().expect("fixture scratch root must exist");
            let spill = Arc::new(
                OracleSpillRuntime::new(root.path(), 2 * 1024 * 1024 * 1024)
                    .expect("bounded spill owner must be created"),
            );
            let authority_calls = Arc::new(AtomicUsize::new(0));
            let node_id = NodeId::new(Uuid::from_u128(2));
            let reservations = Arc::new(super::super::dispatcher::ReservationRegistry::new(
                Arc::new(crate::oracle::OracleSlotManager::new(4, 4)),
                16,
            ));
            let graph = AnalyticalGraphKey::new(
                PublicQueryId::from_uuid(Uuid::from_u128(11)),
                DataFusionQueryId::from_uuid(Uuid::from_u128(12)),
            );
            // The adapter forwards a message only once the follower has taken
            // exact ownership of the graph, and ownership comes from an admitted
            // reservation. Reserving one here keeps this a transport test rather
            // than a test of an unreachable, ownerless graph.
            let reservation = reservations
                .reserve(
                    &wyrd_spec::vala::api::ReserveNodeSlotsRequest {
                        query_id: wyrd_spec::vala::api::QueryId::new(
                            graph.public_query_id.as_uuid(),
                        ),
                        leader_node_id: NodeId::new(Uuid::from_u128(1)),
                        leader_fencing_token: 3,
                        query_class: wyrd_spec::vala::api::QueryClass::Analytical,
                        slot_units: crate::oracle::analytical::ANALYTICAL_GRAPH_SLOT_UNITS,
                        expires_at: Utc::now() + chrono::Duration::seconds(60),
                        graph: Some(wyrd_spec::vala::api::AnalyticalGraphRef {
                            public_query_id: graph.public_query_id.as_uuid(),
                            datafusion_query_id: graph.datafusion_query_id.as_uuid(),
                        }),
                    },
                    Utc::now(),
                    Some(super::super::dispatcher::ReservedCapacity::Graph(Box::new(
                        fixture_oracle_role()
                            .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                                wyrd_spec::vala::api::QueryClass::Analytical,
                                0.0,
                            ))
                            .expect("an idle Oracle admits one analytical query"),
                    ))),
                )
                .expect("an idle follower accepts one graph reservation");
            let ingress = AnalyticalStageIngress::new(AnalyticalStageIngressConfig {
                node_id,
                oracle_fence: 7,
                authority: Arc::new(FixtureAuthority {
                    calls: Arc::clone(&authority_calls),
                }),
                supervisor: Arc::new(
                    super::super::analytical_supervisor::AnalyticalSupervisor::new(),
                ),
                reservations: Arc::clone(&reservations),
                spill: Arc::clone(&spill),
                exchange_buffer_bytes: 64 * 1024,
                leaf: crate::oracle::codec::AnalyticalLeafBinding::new(
                    wyrd_spec::vala::api::ClusterRole::Oracle,
                    Arc::new(crate::oracle::follower::UnresolvableSource),
                    Arc::new(crate::oracle::AcceptingOracleAudit),
                ),
                egress: Arc::new(crate::oracle::analytical::AnalyticalStageEgress::new(
                    Arc::new(FixtureAuthority {
                        calls: Arc::new(AtomicUsize::new(0)),
                    }),
                    node_id,
                    7,
                    chrono::Duration::seconds(30),
                    BifrostPeerTls::unreachable_for_test(),
                    Arc::new(super::super::dispatcher::StaticOraclePeerCredentials::new(
                        secrecy::SecretString::from("fixture-bearer"),
                    )),
                )),
            });
            let identity = StageWireIdentity {
                source_node_id: NodeId::new(Uuid::from_u128(1)),
                source_fence: 3,
                tenant_id: DataTenantId::new_v7(),
                graph,
                snapshot_digest: "fixture-snapshot".to_owned(),
                stage_id: 0,
                task_id: None,
                attempt: 0,
                reservation_id: reservation.reservation_id.as_uuid().to_string(),
                permission_digest: "fixture-permissions".to_owned(),
            };
            Self {
                ingress,
                authority_calls,
                node_id,
                fence: 7,
                identity,
                _spill: spill,
                _root: root,
            }
        }

        /// Builds a governed `SetPlan` request whose ticket binds `bound`.
        ///
        /// Passing a `bound` that differs from the body's own bytes is exactly
        /// the substitution attack the digest exists to refuse.
        ///
        /// # Panics
        ///
        /// Panics when the fixture identity or ticket cannot be encoded.
        fn request<B>(&self, bound: &[u8], body: B) -> Request<B> {
            let mut request = Request::builder()
                .uri(COORDINATOR_CHANNEL_PATH)
                .body(body)
                .expect("fixture request must build");
            self.identity
                .write(request.headers_mut())
                .expect("fixture identity must encode");
            let binding =
                self.identity
                    .to_binding(StageOperationV1::SetPlan, self.node_id, self.fence);
            let claims = StageTicketClaims::for_binding(
                &binding,
                stage_body_digest(bound).expect("fixture body must digest"),
                vec![1, 2, 3, 4],
                0,
                0,
                Vec::new(),
            );
            let ticket = FixtureAuthority {
                calls: Arc::new(AtomicUsize::new(0)),
            }
            .mint_stage(StageOperationV1::SetPlan, &claims)
            .expect("fixture ticket must mint");
            write_ticket(request.headers_mut(), &ticket).expect("fixture ticket must encode");
            request.extensions_mut().insert(Self::authenticated_peer());
            request
        }

        /// Builds the peer context the private listener attaches upstream of
        /// this adapter.
        ///
        /// Every fixture request carries one because in production every
        /// request that reaches this adapter has already been authenticated;
        /// the one test that omits it is the one asserting that fact.
        fn authenticated_peer() -> crate::oracle::peer::AuthenticatedPeerContext {
            crate::oracle::peer::AuthenticatedPeerContext::new(
                DataTenantId::SYSTEM_OWNER,
                wyrd_spec::auth::PrincipalId::new(uuid::Uuid::nil()),
                <wyrd_spec::reference::CardRef as std::str::FromStr>::from_str(
                    "system/Service/bifrost-peer@1.0.0",
                )
                .expect("static card reference is valid"),
                wyrd_runtime::PermissionSet::default(),
                "fixture-credential".to_owned(),
                None,
                wyrd_spec::request_id::RequestId::now_v7(),
                Utc::now(),
            )
        }

        /// Builds the layered service under test over a recording upstream.
        fn service(&self) -> (AnalyticalStageAuth<RecordingUpstream>, Arc<UpstreamProbe>) {
            let probe = Arc::new(UpstreamProbe::default());
            let upstream = RecordingUpstream {
                probe: Arc::clone(&probe),
                seen: Arc::new(std::sync::Mutex::new(Vec::new())),
            };
            let service = AnalyticalStageAuthLayer::new(Arc::clone(&self.ingress)).layer(upstream);
            (service, probe)
        }
    }

    /// Frames `payload` as one uncompressed gRPC message.
    fn framed(payload: &[u8]) -> Bytes {
        let mut message = Vec::with_capacity(payload.len() + GRPC_PREFIX_BYTES);
        message.push(0);
        let length = u32::try_from(payload.len()).expect("test payloads are far below 4 GiB");
        message.extend_from_slice(&length.to_be_bytes());
        message.extend_from_slice(payload);
        Bytes::from(message)
    }

    /// A message split across HTTP/2 data frames reassembles byte for byte.
    ///
    /// The prefix itself is split, which is the case a reader that assumes the
    /// five-byte header arrives whole gets wrong.
    #[tokio::test]
    async fn stage_reader_assembles_a_message_split_across_data_frames() {
        let message = framed(b"distributed-subplan");
        let follow = Bytes::from_static(b"next-message-head");
        let mut body = ChunkBody::new(vec![
            message.slice(0..2),
            message.slice(2..7),
            Bytes::from([message.slice(7..).to_vec(), follow.to_vec()].concat()),
        ]);
        let (assembled, replay) = read_first_message(&mut body, MAX_STAGE_BODY_BYTES)
            .await
            .expect("a fragmented message must reassemble");
        assert_eq!(assembled, message);
        assert_eq!(replay.len(), 2, "the message and its leftover both replay");
        let replayed: Vec<u8> = replay.iter().flat_map(|chunk| chunk.to_vec()).collect();
        assert_eq!(
            replayed,
            [message.to_vec(), follow.to_vec()].concat(),
            "replay must reproduce every consumed byte in wire order"
        );
    }

    /// An oversized declared length is refused the moment the prefix completes.
    #[tokio::test]
    async fn stage_reader_refuses_an_oversized_message_before_buffering_it() {
        let mut reader = StageMessageReader::new(1_024);
        let mut prefix = vec![0_u8];
        prefix.extend_from_slice(&2_000_u32.to_be_bytes());
        assert!(matches!(
            reader.push(&prefix),
            Err(StageFramingError::Oversized)
        ));
        let mut incremental = StageMessageReader::new(1_024);
        let mut undeclared = vec![0_u8];
        undeclared.extend_from_slice(&4_000_000_u32.to_be_bytes());
        assert!(
            matches!(
                incremental.push(&undeclared),
                Err(StageFramingError::Oversized)
            ),
            "the bound is enforced from the declared length, never after collection"
        );
    }

    /// A body that ends mid-message is incomplete, not a short message.
    #[tokio::test]
    async fn stage_reader_refuses_a_body_that_ends_mid_message() {
        let message = framed(b"truncated-subplan");
        let mut body = ChunkBody::new(vec![message.slice(0..8)]);
        assert!(matches!(
            read_first_message(&mut body, MAX_STAGE_BODY_BYTES).await,
            Err(StageFramingError::Incomplete)
        ));
    }

    /// Trailers or a body error before a complete message are both refusals.
    #[tokio::test]
    async fn stage_reader_refuses_trailers_and_body_errors_before_a_message() {
        let message = framed(b"partial");
        let mut trailing = ChunkBody::with_trailers(vec![message.slice(0..4)]);
        assert!(matches!(
            read_first_message(&mut trailing, MAX_STAGE_BODY_BYTES).await,
            Err(StageFramingError::Incomplete)
        ));
        let mut failing = ChunkBody::failing(vec![message.slice(0..4)]);
        assert!(matches!(
            read_first_message(&mut failing, MAX_STAGE_BODY_BYTES).await,
            Err(StageFramingError::BodyFailed)
        ));
    }

    /// A coordinator channel keeps streaming after its buffered `SetPlan`.
    #[tokio::test]
    async fn replay_body_emits_buffered_bytes_before_the_live_remainder() {
        let message = framed(b"set-plan");
        let live = Bytes::from_static(b"work-unit");
        let mut body = ChunkBody::new(vec![message.clone(), live.clone()]);
        let (assembled, replay) = read_first_message(&mut body, MAX_STAGE_BODY_BYTES)
            .await
            .expect("the first message must be read");
        assert_eq!(assembled, message);
        let mut replayed = ReplayBody::new(replay, body);
        let mut delivered = Vec::new();
        while let Some(frame) =
            std::future::poll_fn(|cx| Pin::new(&mut replayed).poll_frame(cx)).await
        {
            let frame = frame.expect("the fixture remainder must not error");
            if let Ok(data) = frame.into_data() {
                delivered.extend_from_slice(&data);
            }
        }
        assert_eq!(
            delivered,
            [message.to_vec(), live.to_vec()].concat(),
            "the live remainder must follow the replayed head untouched"
        );
    }

    /// An authorized operation reaches upstream carrying the exact bound bytes.
    #[tokio::test]
    async fn stage_auth_forwards_the_exact_authorized_message_to_upstream() {
        let fixture = Fixture::new();
        let message = framed(b"authorized-subplan");
        let (mut service, probe) = fixture.service();
        let request = fixture.request(
            &message,
            ChunkBody::new(vec![message.slice(0..3), message.slice(3..)]),
        );
        let response = service
            .call(request)
            .await
            .expect("the fixture upstream is infallible");
        assert_eq!(response.status(), http::StatusCode::OK);
        assert!(probe.reached.load(Ordering::SeqCst));
        assert_eq!(fixture.authority_calls.load(Ordering::SeqCst), 1);
    }

    /// A stage ticket alone cannot substitute for peer workload authentication.
    ///
    /// The two checks answer different questions — which platform service is
    /// calling, and which stage it was authorized for — so a request that
    /// somehow reaches this adapter outside the peer boundary is refused even
    /// when its stage ticket is perfectly valid.
    #[tokio::test]
    async fn stage_auth_refuses_a_request_with_no_authenticated_peer() {
        let fixture = Fixture::new();
        let message = framed(b"authorized-subplan");
        let (mut service, probe) = fixture.service();
        let mut request = fixture.request(&message, ChunkBody::new(vec![message.clone()]));
        request
            .extensions_mut()
            .remove::<crate::oracle::peer::AuthenticatedPeerContext>();

        let response = service
            .call(request)
            .await
            .expect("the fixture upstream is infallible");

        assert_eq!(
            response
                .headers()
                .get("grpc-status")
                .map(|value| value.to_str().unwrap_or_default().to_owned()),
            Some("7".to_owned()),
            "an unauthenticated peer must be refused as permission denied"
        );
        assert!(
            !probe.reached.load(Ordering::SeqCst),
            "refusal must precede upstream entirely"
        );
        assert_eq!(
            fixture.authority_calls.load(Ordering::SeqCst),
            0,
            "stage authority must not be consulted for an unauthenticated peer"
        );
    }

    /// A valid ticket over different bytes cannot authorize the bytes sent.
    #[tokio::test]
    async fn stage_auth_refuses_a_substituted_body_under_a_valid_ticket() {
        let fixture = Fixture::new();
        let bound = framed(b"the-signed-subplan");
        let substituted = framed(b"an-attacker-subplan");
        let (mut service, probe) = fixture.service();
        let request = fixture.request(&bound, ChunkBody::new(vec![substituted]));
        let response = service
            .call(request)
            .await
            .expect("the fixture upstream is infallible");
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("grpc-status")
                .map(|value| value.to_str().unwrap_or_default().to_owned()),
            Some("7".to_owned()),
            "a substituted body must be refused as permission denied"
        );
        assert!(
            !probe.reached.load(Ordering::SeqCst),
            "refusal must precede upstream's decoder, task cache, provider, and IO"
        );
    }

    /// An ungoverned method never reaches upstream at all.
    #[tokio::test]
    async fn stage_auth_refuses_an_unsupported_method_without_calling_upstream() {
        let fixture = Fixture::new();
        let (mut service, probe) = fixture.service();
        let mut request = fixture.request(&framed(b"plan"), ChunkBody::new(vec![framed(b"plan")]));
        *request.uri_mut() = WORKER_INFO_PATH
            .parse()
            .expect("the worker info path must parse");
        let _response = service
            .call(request)
            .await
            .expect("the fixture upstream is infallible");
        assert!(!probe.reached.load(Ordering::SeqCst));
        assert_eq!(
            fixture.authority_calls.load(Ordering::SeqCst),
            0,
            "an ungoverned method fails closed without consuming a ticket"
        );
    }

    /// Malformed framing is refused before authorization or upstream.
    #[tokio::test]
    async fn stage_auth_refuses_malformed_framing_before_authorization() {
        let fixture = Fixture::new();
        let (mut service, probe) = fixture.service();
        let mut oversized = vec![0_u8];
        let over_limit = u32::try_from(MAX_STAGE_BODY_BYTES)
            .expect("the stage body bound is far below 4 GiB")
            + 1;
        oversized.extend_from_slice(&over_limit.to_be_bytes());
        let request = fixture.request(
            &framed(b"plan"),
            ChunkBody::new(vec![Bytes::from(oversized)]),
        );
        let _response = service
            .call(request)
            .await
            .expect("the fixture upstream is infallible");
        assert!(!probe.reached.load(Ordering::SeqCst));
        assert_eq!(fixture.authority_calls.load(Ordering::SeqCst), 0);
    }

    /// Cancellation while buffering reaches neither the authority nor upstream.
    #[tokio::test]
    async fn stage_auth_cancelled_while_buffering_reaches_nothing() {
        let fixture = Fixture::new();
        let (mut service, probe) = fixture.service();
        let request = fixture.request(&framed(b"plan"), PendingBody);
        let call = service.call(request);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), call)
                .await
                .is_err(),
            "a stalled peer must leave the call pending rather than admit it"
        );
        assert!(!probe.reached.load(Ordering::SeqCst));
        assert_eq!(fixture.authority_calls.load(Ordering::SeqCst), 0);
    }
}
