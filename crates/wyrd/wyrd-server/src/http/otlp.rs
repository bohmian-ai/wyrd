//! OTLP/HTTP transport for the Wyrd trace collector.
//!
//! Wyrd is a drop-in OTLP backend over HTTP as well as gRPC: `POST /v1/traces`
//! accepts both `application/x-protobuf` (the OTLP default) and
//! `application/json` (OTLP protobuf-JSON), decodes the payload into the same
//! `ExportTraceServiceRequest` the gRPC collector uses, and drives Task B's
//! shared decode→write core [`vala_ingest::ingest_resource_spans`]. There is one
//! RBAC + map + `commit_one` core; this module only owns the HTTP framing.
//!
//! Content-Type handling (shared by all three signals):
//! - `application/x-protobuf` → `prost::Message::decode`.
//! - `application/json` → `serde_json` (the OTLP protobuf-JSON mapping the
//!   `with-serde` feature emits on the proto message types).
//! - missing / unrecognized Content-Type → treated as protobuf, per the OTLP/HTTP
//!   spec, which defines `application/x-protobuf` as the default request encoding.
//!
//! The response is encoded in the request's encoding (protobuf bytes for a
//! protobuf request, JSON for a JSON request) and carries the OTLP
//! `Export*ServiceResponse`, including `partial_success` when spans were
//! rejected.
//!
//! Backpressure: [`vala_ingest::IngestError::WriterBusy`] maps to HTTP `503`
//! (retryable), never to `partial_success` — no rows from a busy request are
//! written. Every other ingest failure maps to its stable `WYRD_VALA_*`
//! [`WyrdError`] and problem+json status. Authentication is enforced by the
//! `require_authenticated` layer on the `/v1` group; the [`Caller`] extractor
//! then yields the token-derived tenant/principal (never wire-derived).
//!
//! `/v1/metrics` and `/v1/logs` share the Content-Type decode branch but their
//! ingest service bodies are not yet implemented — the decoded request is handed
//! to [`metrics_ingest_unimplemented`] / [`logs_ingest_unimplemented`], which
//! return a `501` [`BifrostError::IngestUnimplemented`]. Those seams are what a
//! later metrics/logs ingest change replaces with real write cores.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use serde::de::DeserializeOwned;
use vala_ingest::IngestError;
use vala_ingest::auth::AuthContext;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::error::BifrostError;
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::prost::Message;

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// OTLP protobuf request/response content type.
const CONTENT_TYPE_PROTOBUF: &str = "application/x-protobuf";
/// OTLP protobuf-JSON request/response content type.
const CONTENT_TYPE_JSON: &str = "application/json";

/// Wire encoding an OTLP/HTTP request declared, and that its response echoes.
///
/// The OTLP/HTTP spec defines `application/x-protobuf` as the default, so a
/// missing or unrecognized `Content-Type` decodes as protobuf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OtlpEncoding {
    /// `application/x-protobuf` — length-delimited prost message.
    Protobuf,
    /// `application/json` — OTLP protobuf-JSON.
    Json,
}

impl OtlpEncoding {
    /// Resolve the encoding from the request `Content-Type`, defaulting to
    /// protobuf per the OTLP/HTTP spec.
    fn from_headers(headers: &HeaderMap) -> Self {
        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        // Match the media type, ignoring any `; charset=...` parameters.
        let media_type = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if media_type == CONTENT_TYPE_JSON {
            Self::Json
        } else {
            Self::Protobuf
        }
    }

    /// The response `Content-Type` header value for this encoding.
    fn response_content_type(self) -> &'static str {
        match self {
            Self::Protobuf => CONTENT_TYPE_PROTOBUF,
            Self::Json => CONTENT_TYPE_JSON,
        }
    }
}

/// Standalone OTLP/HTTP router for the `/v1` group.
///
/// Registers all three OTLP signal endpoints. `/v1/traces` is fully wired to the
/// shared trace decode→write core; `/v1/metrics` and `/v1/logs` share the
/// Content-Type decode branch but return `501` until their ingest services land.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/traces", post(export_traces))
        .route("/metrics", post(export_metrics))
        .route("/logs", post(export_logs))
}

/// Decode an OTLP request body under the resolved `encoding`.
///
/// Protobuf bodies are length-delimited prost messages; JSON bodies use the OTLP
/// protobuf-JSON mapping. Decode failures map to the ingest protocol code so both
/// transports report a malformed export identically.
fn decode_request<T>(encoding: OtlpEncoding, body: &Bytes) -> Result<T, IngestError>
where
    T: Message + DeserializeOwned + Default,
{
    match encoding {
        OtlpEncoding::Protobuf => T::decode(body.as_ref())
            .map_err(|error| IngestError::Decode(format!("OTLP protobuf decode failed: {error}"))),
        OtlpEncoding::Json => serde_json::from_slice(body.as_ref())
            .map_err(|error| IngestError::Decode(format!("OTLP JSON decode failed: {error}"))),
    }
}

/// Encode an OTLP response message in the request's `encoding` and attach the
/// matching `Content-Type`.
fn encode_response<T>(encoding: OtlpEncoding, message: &T) -> Response
where
    T: Message + Serialize,
{
    match encoding {
        OtlpEncoding::Protobuf => {
            let body = message.encode_to_vec();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)],
                body,
            )
                .into_response()
        }
        OtlpEncoding::Json => {
            let mut response = Json(message).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static(encoding.response_content_type()),
            );
            response
        }
    }
}

/// `POST /v1/traces` — decode an OTLP trace export and write it through the
/// shared decode→write core.
///
/// Accepts protobuf and protobuf-JSON, replies in the request's encoding, and
/// surfaces per-span rejections as OTLP `partial_success`.
#[tracing::instrument(
    name = "otlp.http.trace.export",
    skip_all,
    fields(tenant, encoding, accepted_spans, rejected_spans)
)]
async fn export_traces(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, WyrdErrorResponse> {
    tracing::Span::current().record("tenant", tracing::field::display(caller.data_tenant_id));
    let encoding = OtlpEncoding::from_headers(&headers);
    tracing::Span::current().record("encoding", tracing::field::debug(encoding));
    let request: ExportTraceServiceRequest =
        decode_request(encoding, &body).map_err(ingest_error_to_response)?;
    let auth = caller_auth_context(&caller);

    let outcome = vala_ingest::ingest_resource_spans(&state.bifrost, &auth, request)
        .await
        .map_err(ingest_error_to_response)?;

    tracing::Span::current().record("accepted_spans", outcome.accepted_spans);
    tracing::Span::current().record("rejected_spans", outcome.rejected_spans);

    let response = ExportTraceServiceResponse {
        partial_success: outcome.partial_success(),
    };
    Ok(encode_response(encoding, &response))
}

/// `POST /v1/metrics` — the Content-Type decode branch is live; the metrics
/// ingest service is not yet implemented, so a decoded request maps to `501`.
#[tracing::instrument(name = "otlp.http.metrics.export", skip_all, fields(tenant))]
async fn export_metrics(
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, WyrdErrorResponse> {
    tracing::Span::current().record("tenant", tracing::field::display(caller.data_tenant_id));
    let encoding = OtlpEncoding::from_headers(&headers);
    let request: ExportMetricsServiceRequest =
        decode_request(encoding, &body).map_err(ingest_error_to_response)?;
    let response: ExportMetricsServiceResponse = metrics_ingest_unimplemented(request)?;
    Ok(encode_response(encoding, &response))
}

/// `POST /v1/logs` — the Content-Type decode branch is live; the logs ingest
/// service is not yet implemented, so a decoded request maps to `501`.
#[tracing::instrument(name = "otlp.http.logs.export", skip_all, fields(tenant))]
async fn export_logs(
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, WyrdErrorResponse> {
    tracing::Span::current().record("tenant", tracing::field::display(caller.data_tenant_id));
    let encoding = OtlpEncoding::from_headers(&headers);
    let request: ExportLogsServiceRequest =
        decode_request(encoding, &body).map_err(ingest_error_to_response)?;
    let response: ExportLogsServiceResponse = logs_ingest_unimplemented(request)?;
    Ok(encode_response(encoding, &response))
}

/// Metrics ingest write core seam. Returns a `501` until the metrics ingest
/// service is implemented; a later change replaces this body with the real
/// decode→write core (mirroring [`vala_ingest::ingest_resource_spans`]).
fn metrics_ingest_unimplemented(
    _request: ExportMetricsServiceRequest,
) -> Result<ExportMetricsServiceResponse, WyrdErrorResponse> {
    Err(ingest_unimplemented("metrics"))
}

/// Logs ingest write core seam. Returns a `501` until the logs ingest service is
/// implemented; a later change replaces this body with the real decode→write
/// core (mirroring [`vala_ingest::ingest_resource_spans`]).
fn logs_ingest_unimplemented(
    _request: ExportLogsServiceRequest,
) -> Result<ExportLogsServiceResponse, WyrdErrorResponse> {
    Err(ingest_unimplemented("logs"))
}

/// Build the `501` response for an OTLP signal whose ingest service is not yet
/// implemented.
fn ingest_unimplemented(signal: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::from(BifrostError::IngestUnimplemented {
        signal: signal.to_owned(),
    }))
}

/// Build the token-derived [`AuthContext`] for the shared ingest core.
///
/// Tenant, principal, and request id all come from the verified [`Caller`] — the
/// tenant is token-derived, never taken from the request body, so the HTTP and
/// gRPC transports preserve tenant isolation identically.
fn caller_auth_context(caller: &Caller) -> AuthContext {
    AuthContext {
        principal: caller.principal.clone(),
        tenant: caller.data_tenant_id,
        request_id: caller.request_id.clone(),
    }
}

/// Map an [`IngestError`] onto its HTTP response.
///
/// Backpressure ([`IngestError::WriterBusy`]) becomes a retryable `503`, matching
/// the OTLP/HTTP throttling contract (the gRPC edge maps the same condition to
/// retryable `UNAVAILABLE`). Every other ingest error renders its stable
/// `WYRD_VALA_*` [`WyrdError`] problem+json.
fn ingest_error_to_response(error: IngestError) -> WyrdErrorResponse {
    if matches!(error, IngestError::WriterBusy) {
        return WyrdErrorResponse::from(WyrdError::from(BifrostError::WriterUnavailable {
            table: "vala.traces.spans".to_owned(),
        }));
    }
    WyrdErrorResponse::from(ingest_error_to_wyrd(error))
}

/// Map a non-backpressure [`IngestError`] onto its stable [`WyrdError`].
///
/// Preserves the `WYRD_VALA_*` / `WYRD_PERMISSION_*` code and HTTP status the
/// gRPC path already publishes for each condition.
fn ingest_error_to_wyrd(error: IngestError) -> WyrdError {
    match error {
        IngestError::Unauthenticated(message) => WyrdError::Unauthenticated {
            message,
            details: serde_json::Value::Null,
        },
        IngestError::PrincipalUnresolved => WyrdError::Unauthenticated {
            message: "principal_id cannot be stamped for authenticated write".to_owned(),
            details: serde_json::Value::Null,
        },
        IngestError::RbacDenied { detail } => WyrdError::PermissionDeniedRbac {
            message: detail,
            details: serde_json::Value::Null,
        },
        IngestError::SystemTableWriteDenied { table } => {
            WyrdError::from(BifrostError::CardScopeDenied { card_ref: table })
        }
        IngestError::CardScopeDenied { card_ref }
        | IngestError::CardUnresolved { card_ref } => {
            WyrdError::from(BifrostError::CardScopeDenied { card_ref })
        }
        IngestError::TableNotFound { table } => {
            WyrdError::from(BifrostError::TableNotFound { table })
        }
        IngestError::SchemaMismatch { table } => {
            WyrdError::from(BifrostError::FingerprintMismatch { table })
        }
        IngestError::WriterClosed => WyrdError::from(BifrostError::WriterUnavailable {
            table: "vala.traces.spans".to_owned(),
        }),
        IngestError::StreamProtocolViolation(detail) | IngestError::Decode(detail) => {
            WyrdError::from(BifrostError::QueryInvalidSql { detail })
        }
        // Backpressure is handled by ingest_error_to_response before this point.
        IngestError::WriterBusy => WyrdError::from(BifrostError::IngestBusy {
            table: "vala.traces.spans".to_owned(),
        }),
        other => WyrdError::from(BifrostError::Internal {
            detail: other.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{OtlpEncoding, ingest_error_to_response};
    use axum::http::{HeaderMap, StatusCode, header};
    use vala_ingest::IngestError;

    fn headers_with(content_type: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_str(content_type).expect("valid header"),
        );
        headers
    }

    #[test]
    fn missing_content_type_defaults_to_protobuf() {
        // OTLP/HTTP defines protobuf as the default request encoding.
        assert_eq!(
            OtlpEncoding::from_headers(&HeaderMap::new()),
            OtlpEncoding::Protobuf
        );
    }

    #[test]
    fn json_content_type_selects_json() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("application/json")),
            OtlpEncoding::Json
        );
        // Charset parameters and casing do not change the media type.
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("Application/JSON; charset=utf-8")),
            OtlpEncoding::Json
        );
    }

    #[test]
    fn protobuf_content_type_selects_protobuf() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("application/x-protobuf")),
            OtlpEncoding::Protobuf
        );
    }

    #[test]
    fn unknown_content_type_defaults_to_protobuf() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("text/plain")),
            OtlpEncoding::Protobuf
        );
    }

    // Backpressure is a coordinator-local, cross-boundary-state-free condition
    // (channel-full). Forcing genuine channel saturation end-to-end is flaky and
    // materially harder than asserting the load-bearing mapping directly, so the
    // OTLP/HTTP `WriterBusy → 503` contract is pinned here (mirroring the gRPC
    // collector's unit test for `WriterBusy → UNAVAILABLE`).
    #[test]
    fn writer_busy_maps_to_503() {
        let response = ingest_error_to_response(IngestError::WriterBusy);
        let status = axum::response::IntoResponse::into_response(response).status();
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "OTLP/HTTP backpressure must be a retryable 503"
        );
    }

    #[test]
    fn table_not_found_maps_to_404() {
        let response = ingest_error_to_response(IngestError::TableNotFound {
            table: "vala.traces.spans".to_owned(),
        });
        let status = axum::response::IntoResponse::into_response(response).status();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
