//! Server-owned OTLP gRPC adapters with pre-construction decode ownership.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tower::Service;
use vala_bifrost_redux::contracts::DecodedOtlp;
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::prost::Message;
use wyrd_tonic::prost::bytes::Buf;
use wyrd_tonic::tonic::body::Body as TonicBody;
use wyrd_tonic::tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use wyrd_tonic::tonic::codegen::http::{Request as HttpRequest, Response as HttpResponse};
use wyrd_tonic::tonic::codegen::{Body, StdError};
use wyrd_tonic::tonic::server::{Grpc, NamedService, UnaryService};
use wyrd_tonic::tonic::{Request, Response, Status};

use crate::otlp_decode::{decode_trace_protobuf, preflight_trace_protobuf};
use crate::state::ServerGate;

/// Canonical OTLP trace export route retained from the generated service.
const TRACE_EXPORT_PATH: &str = "/opentelemetry.proto.collector.trace.v1.TraceService/Export";
/// Canonical OTLP trace service name used by tonic routing.
const TRACE_SERVICE_NAME: &str = "opentelemetry.proto.collector.trace.v1.TraceService";

/// Server-owned trace service that installs the bounded decoder before prost.
#[derive(Clone)]
pub(super) struct TraceOtlpGrpcService {
    /// Gate retains authentication and routing authority after adapter decode.
    gate: Arc<ServerGate>,
}

impl TraceOtlpGrpcService {
    /// Couples the trace adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        Self { gate }
    }
}

impl NamedService for TraceOtlpGrpcService {
    const NAME: &'static str = TRACE_SERVICE_NAME;
}

impl<B> Service<HttpRequest<B>> for TraceOtlpGrpcService
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = HttpResponse<TonicBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// The adapter has no independent readiness state beyond its retained Gate.
    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Routes the canonical trace path through the preflighting codec.
    fn call(&mut self, request: HttpRequest<B>) -> Self::Future {
        if request.uri().path() != TRACE_EXPORT_PATH {
            return Box::pin(async move {
                Ok(Status::unimplemented("unknown OTLP trace RPC").into_http())
            });
        }
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            let method = TraceExportUnary {
                gate: Arc::clone(&gate),
            };
            let codec = TraceOtlpCodec { gate };
            let response = Grpc::new(codec)
                .max_decoding_message_size(
                    vala_bifrost_redux::gate::limits::BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
                )
                .unary(method, request)
                .await;
            Ok(response)
        })
    }
}

/// Unary trace handler that preserves Gate authentication and response semantics.
struct TraceExportUnary {
    /// Gate selected during server boot.
    gate: Arc<ServerGate>,
}

impl UnaryService<DecodedOtlp<ExportTraceServiceRequest>> for TraceExportUnary {
    type Response = ExportTraceServiceResponse;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::Response>, Status>> + Send + 'static>>;

    /// Authenticates metadata, routes the owned request, and returns OTLP partial success.
    fn call(&mut self, request: Request<DecodedOtlp<ExportTraceServiceRequest>>) -> Self::Future {
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            let auth = gate
                .authenticate_otlp_metadata(request.metadata())
                .await
                .map_err(Status::from)?;
            let outcome = gate
                .ingest_decoded_resource_spans(&auth, request.into_inner())
                .await
                .map_err(Status::from)?;
            Ok(Response::new(ExportTraceServiceResponse {
                partial_success: outcome.partial_success(),
            }))
        })
    }
}

/// Tonic codec whose decoder owns trace preflight and root-backed admission.
struct TraceOtlpCodec {
    /// Gate used only to acquire the Scribe transport-decode child.
    gate: Arc<ServerGate>,
}

impl Codec for TraceOtlpCodec {
    type Encode = ExportTraceServiceResponse;
    type Decode = DecodedOtlp<ExportTraceServiceRequest>;
    type Encoder = OtlpResponseEncoder<ExportTraceServiceResponse>;
    type Decoder = TraceRequestDecoder;

    /// Creates the bounded response encoder.
    fn encoder(&mut self) -> Self::Encoder {
        OtlpResponseEncoder::default()
    }

    /// Creates a decoder retaining the exact Gate/Scribe owner source.
    fn decoder(&mut self) -> Self::Decoder {
        TraceRequestDecoder {
            gate: Arc::clone(&self.gate),
        }
    }
}

/// Prost response encoder used by the custom request codec.
struct OtlpResponseEncoder<T> {
    /// Retains the encoded response type without runtime state.
    marker: std::marker::PhantomData<T>,
}

impl<T> Default for OtlpResponseEncoder<T> {
    /// Creates a stateless response encoder.
    fn default() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}

impl<T> Encoder for OtlpResponseEncoder<T>
where
    T: Message,
{
    type Item = T;
    type Error = Status;

    /// Encodes one small OTLP response into tonic's bounded output buffer.
    ///
    /// # Errors
    ///
    /// Returns internal status if prost cannot write the response.
    fn encode(&mut self, item: Self::Item, destination: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(destination)
            .map_err(|error| Status::internal(format!("OTLP response encode failed: {error}")))
    }
}

/// Trace protobuf decoder that acquires capacity after wire preflight.
struct TraceRequestDecoder {
    /// Gate used to acquire exactly the preflighted decode capacity.
    gate: Arc<ServerGate>,
}

impl Decoder for TraceRequestDecoder {
    type Item = DecodedOtlp<ExportTraceServiceRequest>;
    type Error = Status;

    /// Preflights a complete tonic message, reserves, then constructs prost types.
    ///
    /// The tonic decode buffer is contiguous for one unary frame. Refusing a
    /// fragmented view avoids a hidden consolidation allocation.
    ///
    /// # Errors
    ///
    /// Returns invalid argument for malformed or fragmented input and the
    /// stable Gate capacity status when the decode owner cannot be acquired.
    fn decode(&mut self, source: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Status> {
        let wire_bytes = source.remaining();
        let bytes = source.chunk();
        if bytes.len() != wire_bytes {
            return Err(Status::invalid_argument(
                "OTLP unary frame is not contiguous",
            ));
        }
        let plan =
            preflight_trace_protobuf(bytes, self.gate.otlp_wire_limits()).map_err(Status::from)?;
        let owner = self
            .gate
            .reserve_otlp_decode(plan.decode_bytes)
            .map_err(Status::from)?;
        let request = decode_trace_protobuf(bytes).map_err(Status::from)?;
        source.advance(plan.wire_bytes);
        Ok(Some(DecodedOtlp::new(request, plan.wire_bytes, owner)))
    }
}
