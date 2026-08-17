//! Server-owned OTLP gRPC adapters with pre-construction decode ownership.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use tower::Service;
use vala_bifrost_redux::contracts::DecodedOtlp;
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::prost::Message;
use wyrd_tonic::prost::bytes::Buf;
use wyrd_tonic::tonic::body::Body as TonicBody;
use wyrd_tonic::tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use wyrd_tonic::tonic::codegen::http::{Request as HttpRequest, Response as HttpResponse};
use wyrd_tonic::tonic::codegen::{Body, StdError};
use wyrd_tonic::tonic::metadata::MetadataMap;
use wyrd_tonic::tonic::server::{Grpc, NamedService, UnaryService};
use wyrd_tonic::tonic::{Request, Response, Status};

use crate::otlp_decode::{decode_trace_protobuf, preflight_trace_protobuf};
use crate::otlp_logs_decode::{decode_logs_protobuf, preflight_logs_protobuf};
use crate::otlp_metrics_decode::{decode_metrics_protobuf, preflight_metrics_protobuf};
use crate::state::ServerGate;

/// Canonical OTLP trace export route retained from the generated service.
const TRACE_EXPORT_PATH: &str = "/opentelemetry.proto.collector.trace.v1.TraceService/Export";
/// Canonical OTLP trace service name used by tonic routing.
const TRACE_SERVICE_NAME: &str = "opentelemetry.proto.collector.trace.v1.TraceService";
/// Canonical OTLP metrics export route retained from the generated service.
const METRICS_EXPORT_PATH: &str = "/opentelemetry.proto.collector.metrics.v1.MetricsService/Export";
/// Canonical OTLP metrics service name used by tonic routing.
const METRICS_SERVICE_NAME: &str = "opentelemetry.proto.collector.metrics.v1.MetricsService";
/// Canonical OTLP logs export route retained from the generated service.
const LOGS_EXPORT_PATH: &str = "/opentelemetry.proto.collector.logs.v1.LogsService/Export";
/// Canonical OTLP logs service name used by tonic routing.
const LOGS_SERVICE_NAME: &str = "opentelemetry.proto.collector.logs.v1.LogsService";

/// Bounded debug-build counters used only by transport integration tests.
#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtlpCodecActivity {
    /// Codec preflight entries.
    pub preflight: usize,
    /// Typed decode construction entries.
    pub decode: usize,
    /// Scribe decode-reservation attempts.
    pub scribe_reservation: usize,
}

#[cfg(debug_assertions)]
static OTLP_PREFLIGHT: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static OTLP_DECODE: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static OTLP_RESERVE: AtomicUsize = AtomicUsize::new(0);

/// Clears bounded debug-build codec counters before an isolated test.
#[cfg(debug_assertions)]
pub fn reset_otlp_codec_activity() {
    OTLP_PREFLIGHT.store(0, Ordering::Release);
    OTLP_DECODE.store(0, Ordering::Release);
    OTLP_RESERVE.store(0, Ordering::Release);
}

/// Snapshots bounded debug-build codec counters after an isolated test.
#[cfg(debug_assertions)]
#[must_use]
pub fn snapshot_otlp_codec_activity() -> OtlpCodecActivity {
    OtlpCodecActivity {
        preflight: OTLP_PREFLIGHT.load(Ordering::Acquire),
        decode: OTLP_DECODE.load(Ordering::Acquire),
        scribe_reservation: OTLP_RESERVE.load(Ordering::Acquire),
    }
}

/// Records a codec stage in debug builds without changing release behavior.
fn record_codec_activity(stage: &str) {
    #[cfg(debug_assertions)]
    match stage {
        "preflight" => OTLP_PREFLIGHT.fetch_add(1, Ordering::Relaxed),
        "decode" => OTLP_DECODE.fetch_add(1, Ordering::Relaxed),
        "reserve" => OTLP_RESERVE.fetch_add(1, Ordering::Relaxed),
        _ => 0,
    };
    #[cfg(not(debug_assertions))]
    let _ = stage;
}

/// Server-owned trace service that installs bounded decode before typed construction.
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
    ///
    /// # Errors
    ///
    /// This method is infallible because [`Self::Error`] is [`Infallible`].
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
            let metadata = MetadataMap::from_headers(request.headers().clone());
            let auth = match gate.authenticate_otlp_metadata(&metadata).await {
                Ok(auth) => auth,
                Err(error) => return Ok(Status::from(error).into_http()),
            };
            let method = TraceExportUnary {
                gate: Arc::clone(&gate),
                auth: Some(auth),
            };
            let maximum_message_size = gate.otlp_decoding_message_size();
            let codec = TraceOtlpCodec { gate };
            let response = Grpc::new(codec)
                .max_decoding_message_size(maximum_message_size)
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
    /// Call-scoped authentication consumed exactly once before routing.
    auth: Option<vala_bifrost_redux::gate::AuthContext>,
}

impl UnaryService<DecodedOtlp<ExportTraceServiceRequest>> for TraceExportUnary {
    type Response = ExportTraceServiceResponse;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::Response>, Status>> + Send + 'static>>;

    /// Consumes call-scoped authentication, routes the request, and returns partial success.
    fn call(&mut self, request: Request<DecodedOtlp<ExportTraceServiceRequest>>) -> Self::Future {
        let gate = Arc::clone(&self.gate);
        let auth = self.auth.take();
        Box::pin(async move {
            let auth = auth.ok_or_else(|| Status::internal("OTLP authentication was consumed"))?;
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

/// Tonic codec whose decoder owns trace preflight and root-backed decode reservation.
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

    /// Creates a decoder that obtains its decode child through routing-only Gate.
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
    /// Scribe reservation refusal mapped through Gate when ownership cannot be
    /// acquired.
    fn decode(&mut self, source: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Status> {
        let wire_bytes = source.remaining();
        let bytes = source.chunk();
        if bytes.len() != wire_bytes {
            return Err(Status::invalid_argument(
                "OTLP unary frame is not contiguous",
            ));
        }
        record_codec_activity("preflight");
        let plan =
            preflight_trace_protobuf(bytes, self.gate.otlp_wire_limits()).map_err(Status::from)?;
        record_codec_activity("reserve");
        let owner = self
            .gate
            .reserve_otlp_decode(plan.decode_bytes)
            .map_err(Status::from)?;
        record_codec_activity("decode");
        let request = decode_trace_protobuf(bytes).map_err(Status::from)?;
        source.advance(plan.wire_bytes);
        Ok(Some(DecodedOtlp::new(request, plan.wire_bytes, owner)))
    }
}

/// Server-owned metrics service that installs bounded decoding before Prost allocation.
#[derive(Clone)]
pub(super) struct MetricsOtlpGrpcService {
    /// Gate retains authentication and routing authority after adapter decode.
    gate: Arc<ServerGate>,
}

impl MetricsOtlpGrpcService {
    /// Couples the metrics adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        Self { gate }
    }
}

impl NamedService for MetricsOtlpGrpcService {
    const NAME: &'static str = METRICS_SERVICE_NAME;
}

impl<B> Service<HttpRequest<B>> for MetricsOtlpGrpcService
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = HttpResponse<TonicBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Reports immediate readiness because the retained Gate owns all state.
    ///
    /// # Errors
    ///
    /// This method is infallible because [`Self::Error`] is [`Infallible`].
    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Routes only the canonical metrics export path through the bounded codec.
    fn call(&mut self, request: HttpRequest<B>) -> Self::Future {
        if request.uri().path() != METRICS_EXPORT_PATH {
            return Box::pin(async move {
                Ok(Status::unimplemented("unknown OTLP metrics RPC").into_http())
            });
        }
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            let metadata = MetadataMap::from_headers(request.headers().clone());
            let auth = match gate.authenticate_otlp_metadata(&metadata).await {
                Ok(auth) => auth,
                Err(error) => return Ok(Status::from(error).into_http()),
            };
            let maximum_message_size = gate.otlp_decoding_message_size();
            let response = Grpc::new(MetricsOtlpCodec {
                gate: Arc::clone(&gate),
            })
            .max_decoding_message_size(maximum_message_size)
            .unary(
                MetricsExportUnary {
                    gate,
                    auth: Some(auth),
                },
                request,
            )
            .await;
            Ok(response)
        })
    }
}

/// Unary metrics handler preserving authentication and partial-success behavior.
struct MetricsExportUnary {
    /// Gate selected during server boot.
    gate: Arc<ServerGate>,
    /// Call-scoped authentication consumed exactly once before routing.
    auth: Option<vala_bifrost_redux::gate::AuthContext>,
}

impl UnaryService<DecodedOtlp<ExportMetricsServiceRequest>> for MetricsExportUnary {
    type Response = ExportMetricsServiceResponse;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::Response>, Status>> + Send + 'static>>;

    /// Consumes call-scoped authentication and transfers the typed request to Scribe.
    fn call(&mut self, request: Request<DecodedOtlp<ExportMetricsServiceRequest>>) -> Self::Future {
        let gate = Arc::clone(&self.gate);
        let auth = self.auth.take();
        Box::pin(async move {
            let auth = auth.ok_or_else(|| Status::internal("OTLP authentication was consumed"))?;
            let outcome = gate
                .ingest_decoded_resource_metrics(&auth, request.into_inner())
                .await
                .map_err(Status::from)?;
            Ok(Response::new(ExportMetricsServiceResponse {
                partial_success: outcome.partial_success(),
            }))
        })
    }
}

/// Metrics codec coupling the bounded request decoder to the standard response encoder.
struct MetricsOtlpCodec {
    /// Gate used to acquire the Scribe transport-decode child.
    gate: Arc<ServerGate>,
}

impl Codec for MetricsOtlpCodec {
    type Encode = ExportMetricsServiceResponse;
    type Decode = DecodedOtlp<ExportMetricsServiceRequest>;
    type Encoder = OtlpResponseEncoder<ExportMetricsServiceResponse>;
    type Decoder = MetricsRequestDecoder;

    /// Creates the standard bounded response encoder.
    fn encoder(&mut self) -> Self::Encoder {
        OtlpResponseEncoder::default()
    }

    /// Creates a metrics decoder from the shared boot-time limits owner.
    fn decoder(&mut self) -> Self::Decoder {
        MetricsRequestDecoder {
            gate: Arc::clone(&self.gate),
        }
    }
}

/// Metrics protobuf decoder that preflights before acquiring typed capacity.
struct MetricsRequestDecoder {
    /// Gate used solely for limits and transport-decode ownership.
    gate: Arc<ServerGate>,
}

impl Decoder for MetricsRequestDecoder {
    type Item = DecodedOtlp<ExportMetricsServiceRequest>;
    type Error = Status;

    /// Constructs one fixed-capacity typed metrics request from a contiguous frame.
    ///
    /// # Errors
    ///
    /// Returns invalid argument for fragmented, malformed, or over-limit frames,
    /// or the Scribe ownership refusal mapped through Gate when the decode child
    /// cannot be reserved.
    fn decode(&mut self, source: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Status> {
        let wire_bytes = source.remaining();
        let bytes = source.chunk();
        if bytes.len() != wire_bytes {
            return Err(Status::invalid_argument(
                "OTLP unary frame is not contiguous",
            ));
        }
        record_codec_activity("preflight");
        let plan = preflight_metrics_protobuf(bytes, self.gate.otlp_wire_limits())
            .map_err(Status::from)?;
        record_codec_activity("reserve");
        let owner = self
            .gate
            .reserve_otlp_decode(plan.decode_bytes)
            .map_err(Status::from)?;
        record_codec_activity("decode");
        let request = decode_metrics_protobuf(bytes, plan).map_err(Status::from)?;
        source.advance(plan.wire_bytes);
        Ok(Some(DecodedOtlp::new(request, plan.wire_bytes, owner)))
    }
}

/// Server-owned logs service that installs bounded decoding before Prost allocation.
#[derive(Clone)]
pub(super) struct LogsOtlpGrpcService {
    /// Gate retains authentication and routing authority after adapter decode.
    gate: Arc<ServerGate>,
}

impl LogsOtlpGrpcService {
    /// Couples the logs adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        Self { gate }
    }
}

impl NamedService for LogsOtlpGrpcService {
    const NAME: &'static str = LOGS_SERVICE_NAME;
}

impl<B> Service<HttpRequest<B>> for LogsOtlpGrpcService
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = HttpResponse<TonicBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Reports immediate readiness because the retained Gate owns all state.
    ///
    /// # Errors
    ///
    /// This method is infallible because [`Self::Error`] is [`Infallible`].
    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Routes only the canonical logs export path through the bounded codec.
    fn call(&mut self, request: HttpRequest<B>) -> Self::Future {
        if request.uri().path() != LOGS_EXPORT_PATH {
            return Box::pin(async move {
                Ok(Status::unimplemented("unknown OTLP logs RPC").into_http())
            });
        }
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            let metadata = MetadataMap::from_headers(request.headers().clone());
            let auth = match gate.authenticate_otlp_metadata(&metadata).await {
                Ok(auth) => auth,
                Err(error) => return Ok(Status::from(error).into_http()),
            };
            let maximum_message_size = gate.otlp_decoding_message_size();
            let response = Grpc::new(LogsOtlpCodec {
                gate: Arc::clone(&gate),
            })
            .max_decoding_message_size(maximum_message_size)
            .unary(
                LogsExportUnary {
                    gate,
                    auth: Some(auth),
                },
                request,
            )
            .await;
            Ok(response)
        })
    }
}

/// Unary logs handler preserving authentication and partial-success behavior.
struct LogsExportUnary {
    /// Gate selected during server boot.
    gate: Arc<ServerGate>,
    /// Call-scoped authentication consumed exactly once before routing.
    auth: Option<vala_bifrost_redux::gate::AuthContext>,
}

impl UnaryService<DecodedOtlp<ExportLogsServiceRequest>> for LogsExportUnary {
    type Response = ExportLogsServiceResponse;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::Response>, Status>> + Send + 'static>>;

    /// Consumes call-scoped authentication and transfers the typed request to Scribe.
    fn call(&mut self, request: Request<DecodedOtlp<ExportLogsServiceRequest>>) -> Self::Future {
        let gate = Arc::clone(&self.gate);
        let auth = self.auth.take();
        Box::pin(async move {
            let auth = auth.ok_or_else(|| Status::internal("OTLP authentication was consumed"))?;
            let outcome = gate
                .ingest_decoded_resource_logs(&auth, request.into_inner())
                .await
                .map_err(Status::from)?;
            Ok(Response::new(ExportLogsServiceResponse {
                partial_success: outcome.partial_success(),
            }))
        })
    }
}

/// Logs codec coupling the bounded request decoder to the standard response encoder.
struct LogsOtlpCodec {
    /// Gate used to acquire the Scribe transport-decode child.
    gate: Arc<ServerGate>,
}

impl Codec for LogsOtlpCodec {
    type Encode = ExportLogsServiceResponse;
    type Decode = DecodedOtlp<ExportLogsServiceRequest>;
    type Encoder = OtlpResponseEncoder<ExportLogsServiceResponse>;
    type Decoder = LogsRequestDecoder;

    /// Creates the standard bounded response encoder.
    fn encoder(&mut self) -> Self::Encoder {
        OtlpResponseEncoder::default()
    }

    /// Creates a logs decoder from the shared boot-time limits owner.
    fn decoder(&mut self) -> Self::Decoder {
        LogsRequestDecoder {
            gate: Arc::clone(&self.gate),
        }
    }
}

/// Logs protobuf decoder that preflights before acquiring typed capacity.
struct LogsRequestDecoder {
    /// Gate used solely for limits and transport-decode ownership.
    gate: Arc<ServerGate>,
}

impl Decoder for LogsRequestDecoder {
    type Item = DecodedOtlp<ExportLogsServiceRequest>;
    type Error = Status;

    /// Constructs one fixed-capacity typed logs request from a contiguous frame.
    ///
    /// # Errors
    ///
    /// Returns invalid argument for fragmented, malformed, or over-limit frames,
    /// or the Scribe ownership refusal mapped through Gate when the decode child
    /// cannot be reserved.
    fn decode(&mut self, source: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Status> {
        let wire_bytes = source.remaining();
        let bytes = source.chunk();
        if bytes.len() != wire_bytes {
            return Err(Status::invalid_argument(
                "OTLP unary frame is not contiguous",
            ));
        }
        record_codec_activity("preflight");
        let plan =
            preflight_logs_protobuf(bytes, self.gate.otlp_wire_limits()).map_err(Status::from)?;
        record_codec_activity("reserve");
        let owner = self
            .gate
            .reserve_otlp_decode(plan.decode_bytes)
            .map_err(Status::from)?;
        record_codec_activity("decode");
        let request = decode_logs_protobuf(bytes).map_err(Status::from)?;
        source.advance(plan.wire_bytes);
        Ok(Some(DecodedOtlp::new(request, plan.wire_bytes, owner)))
    }
}
