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

/// Closed OTLP service set that may consume the configured Scribe decode limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScribeOtlpService {
    /// Trace export service.
    Traces,
    /// Metrics export service.
    Metrics,
    /// Logs export service.
    Logs,
}

/// One boot-frozen decode allowance shared only by the three Scribe OTLP services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScribeOtlpDecodingLimit {
    /// Configured tonic message allowance projected from Gate.
    bytes: usize,
}

impl ScribeOtlpDecodingLimit {
    /// Captures the immutable Gate allowance during service construction.
    fn from_gate(gate: &ServerGate) -> Self {
        Self {
            bytes: gate.otlp_decoding_message_size(),
        }
    }

    /// Returns the shared allowance for one member of the closed OTLP set.
    const fn for_service(self, _service: ScribeOtlpService) -> usize {
        self.bytes
    }
}

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
    /// Boot-frozen decode allowance owned only by Scribe OTLP services.
    decoding_limit: ScribeOtlpDecodingLimit,
}

impl TraceOtlpGrpcService {
    /// Couples the trace adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        let decoding_limit = ScribeOtlpDecodingLimit::from_gate(&gate);
        Self {
            gate,
            decoding_limit,
        }
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
        let maximum_message_size = self.decoding_limit.for_service(ScribeOtlpService::Traces);
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
    /// Boot-frozen decode allowance owned only by Scribe OTLP services.
    decoding_limit: ScribeOtlpDecodingLimit,
}

impl MetricsOtlpGrpcService {
    /// Couples the metrics adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        let decoding_limit = ScribeOtlpDecodingLimit::from_gate(&gate);
        Self {
            gate,
            decoding_limit,
        }
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
        let maximum_message_size = self.decoding_limit.for_service(ScribeOtlpService::Metrics);
        Box::pin(async move {
            let metadata = MetadataMap::from_headers(request.headers().clone());
            let auth = match gate.authenticate_otlp_metadata(&metadata).await {
                Ok(auth) => auth,
                Err(error) => return Ok(Status::from(error).into_http()),
            };
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
    /// Boot-frozen decode allowance owned only by Scribe OTLP services.
    decoding_limit: ScribeOtlpDecodingLimit,
}

impl LogsOtlpGrpcService {
    /// Couples the logs adapter to the process Gate selected during boot.
    pub(super) fn new(gate: Arc<ServerGate>) -> Self {
        let decoding_limit = ScribeOtlpDecodingLimit::from_gate(&gate);
        Self {
            gate,
            decoding_limit,
        }
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
        let maximum_message_size = self.decoding_limit.for_service(ScribeOtlpService::Logs);
        Box::pin(async move {
            let metadata = MetadataMap::from_headers(request.headers().clone());
            let auth = match gate.authenticate_otlp_metadata(&metadata).await {
                Ok(auth) => auth,
                Err(error) => return Ok(Status::from(error).into_http()),
            };
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::Duration;
    use futures_util::StreamExt;
    use http_body_util::{BodyStream, Full, StreamBody};
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tower::Service;
    use vala_bifrost_redux::gate::auth::ingest_auth_interceptor;
    use vala_bifrost_redux::gate::limits::{
        BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES, BifrostTransportAdmission, IngestLimits,
    };
    use vala_bifrost_redux::scribe::ScribeImpl;
    use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_runtime::{PrincipalId, RoleRef};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_tonic::prost::bytes::Bytes;
    use wyrd_tonic::tonic::body::Body as TonicBody;
    use wyrd_tonic::tonic::codegen::http::Request as HttpRequest;
    use wyrd_tonic::tonic_health::server::health_reporter;

    use super::{
        LOGS_EXPORT_PATH, LogsOtlpGrpcService, METRICS_EXPORT_PATH, MetricsOtlpGrpcService,
        ScribeOtlpDecodingLimit, ScribeOtlpService, TRACE_EXPORT_PATH,
    };
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::state::ServerGate;

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const TEST_PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    /// Builds a gate with a lazy SQL verifier so service calls reach the real
    /// OTLP adapters without requiring a database connection.
    fn test_gate(configured: usize) -> (Arc<ServerGate>, String, tempfile::TempDir) {
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("test").expect("test key id is valid"),
            Arc::new(public_key_from_pem(TEST_PUBLIC_KEY_PEM).expect("test key parses")),
        );
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(pool))),
            WyrdAuthVerifySettings::default(),
        ));
        let auth = ingest_auth_interceptor(verifier);
        let mut limits = IngestLimits::default();
        limits.max_decoding_message_size = configured;
        limits.otlp.request_bytes = configured;
        let wal_root = tempfile::tempdir().expect("test WAL directory creates");
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *uuid::Uuid::now_v7().as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("test WAL initializes"),
        );
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory operator initializes")
                .finish(),
        );
        let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps(
            operator,
            wal,
            &uuid::Uuid::now_v7().to_string(),
            1,
        ));
        let gate = Arc::new(ServerGate::with_scribe(scribe, auth, limits));
        let issuing_key = IssuingKey::from_ed_pem(
            secrecy::SecretString::from(TEST_PRIVATE_KEY_PEM),
            Kid::new("test").expect("test key id is valid"),
            "wyrd",
        )
        .expect("test issuing key parses");
        let tenant = DataTenantId::new_v7();
        let principal = wyrd_auth_verify::TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindTag::User,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        let token = issuing_key
            .issue_user_access_token(principal, Vec::<RoleRef>::new(), Duration::minutes(5))
            .expect("test token signs");
        (gate, token, wal_root)
    }

    /// Builds a header-only gRPC frame whose body has ended.
    fn ended_declared_frame_body(message_bytes: usize) -> TonicBody {
        let length = u32::try_from(message_bytes).expect("test frame length fits gRPC u32");
        let mut frame = vec![0_u8; 5];
        frame[1..5].copy_from_slice(&length.to_be_bytes());
        TonicBody::new(Full::new(Bytes::from(frame)))
    }

    /// Builds a header-only gRPC frame that stays open for immediate rejection.
    fn pending_declared_frame_body(message_bytes: usize) -> TonicBody {
        let length = u32::try_from(message_bytes).expect("test frame length fits gRPC u32");
        let mut frame = vec![0_u8; 5];
        frame[1..5].copy_from_slice(&length.to_be_bytes());
        let header = BodyStream::new(Full::new(Bytes::from(frame)));
        TonicBody::new(StreamBody::new(
            header.chain(futures_util::stream::pending()),
        ))
    }

    /// Calls one actual OTLP adapter route at a declared boundary and returns
    /// the status emitted by tonic's production decoder path.
    async fn call_otlp_route(
        path: &'static str,
        gate: Arc<ServerGate>,
        token: &str,
        message_bytes: usize,
        pending: bool,
    ) -> Option<String> {
        let body = if pending {
            pending_declared_frame_body(message_bytes)
        } else {
            ended_declared_frame_body(message_bytes)
        };
        let mut request = HttpRequest::builder()
            .uri(path)
            .body(body)
            .expect("OTLP request builds");
        request.headers_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {token}")
                .parse()
                .expect("token metadata parses"),
        );
        let response = match path {
            TRACE_EXPORT_PATH => {
                let mut service = super::TraceOtlpGrpcService::new(gate);
                service
                    .call(request)
                    .await
                    .expect("trace adapter is infallible")
            }
            METRICS_EXPORT_PATH => {
                let mut service = MetricsOtlpGrpcService::new(gate);
                service
                    .call(request)
                    .await
                    .expect("metrics adapter is infallible")
            }
            LOGS_EXPORT_PATH => {
                let mut service = LogsOtlpGrpcService::new(gate);
                service
                    .call(request)
                    .await
                    .expect("logs adapter is infallible")
            }
            _ => panic!("unexpected OTLP test route"),
        };
        response
            .headers()
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    /// Proves real OTLP service construction and calls retain the selected
    /// nondefault Scribe bound while tonic health keeps its smaller default.
    #[tokio::test]
    async fn scribe_otlp_services_share_configured_decoding_limit() {
        let configured = 5 * 1024 * 1024;
        let (gate, token, _wal_root) = test_gate(configured);
        let limits = ScribeOtlpDecodingLimit::from_gate(&gate);

        assert_eq!(limits.for_service(ScribeOtlpService::Traces), configured);
        assert_eq!(limits.for_service(ScribeOtlpService::Metrics), configured);
        assert_eq!(limits.for_service(ScribeOtlpService::Logs), configured);

        for path in [TRACE_EXPORT_PATH, METRICS_EXPORT_PATH, LOGS_EXPORT_PATH] {
            let at_bound =
                call_otlp_route(path, Arc::clone(&gate), &token, configured, false).await;
            assert_ne!(at_bound.as_deref(), Some("11"));
            let over_bound = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                call_otlp_route(path, Arc::clone(&gate), &token, configured + 1, true),
            )
            .await
            .expect("oversized OTLP header is rejected before its payload arrives");
            assert_eq!(over_bound.as_deref(), Some("11"));
        }

        let control_limit = 1024 * 1024;
        let (_reporter, health) = health_reporter();
        let mut health = health.max_decoding_message_size(control_limit);
        let mut health_request = HttpRequest::builder()
            .uri("/grpc.health.v1.Health/Check")
            .body(pending_declared_frame_body(control_limit + 1))
            .expect("health request builds");
        health_request.headers_mut().insert(
            "content-type",
            "application/grpc".parse().expect("content type parses"),
        );
        let health_response = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            health.call(health_request),
        )
        .await
        .expect("health decoder rejects before the oversized payload arrives")
        .expect("health adapter is infallible");
        assert_eq!(
            health_response
                .headers()
                .get("grpc-status")
                .and_then(|value| value.to_str().ok()),
            Some("11")
        );
        assert!(control_limit + 1 <= configured);

        let admission = BifrostTransportAdmission::default();
        assert_eq!(
            admission.message_limit_bytes(),
            BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES
        );
    }
}
