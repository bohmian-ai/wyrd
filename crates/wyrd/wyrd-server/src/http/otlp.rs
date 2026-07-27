//! OTLP/HTTP transport for the Wyrd trace collector.
//!
//! Wyrd is a drop-in OTLP backend over HTTP as well as gRPC: `POST /v1/traces`
//! accepts both `application/x-protobuf` (the OTLP default) and
//! `application/json` (OTLP protobuf-JSON), decodes the payload into the same
//! `ExportTraceServiceRequest` the gRPC collector uses, and drives Task B's
//! shared decode→write core in `vala_bifrost_redux::gate`. There is one
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
//! Backpressure: [`vala_bifrost_redux::gate::IngestError::IngestBusy`] maps to HTTP `429`
//! (retryable), never to `partial_success` — no rows from a busy request are
//! written. The gRPC edge maps the same condition to gRPC
//! `RESOURCE_EXHAUSTED` (also retryable, per the OTLP spec's backpressure
//! guidance). Every other ingest failure maps to its stable `WYRD_VALA_*`
//! [`WyrdError`] and problem+json status. Authentication is enforced by the
//! `require_authenticated` layer on the `/v1` group; the [`Caller`] extractor
//! then yields the token-derived tenant/principal (never wire-derived).
//!
//! `/v1/metrics` and `/v1/logs` share the same three-step shape as `/v1/traces`:
//! Content-Type decode, the shared decode→write core
//! (`vala_bifrost_redux::gate::Gate::ingest_resource_metrics` /
//! `Gate::ingest_resource_logs`),
//! then a response encoded in the request's encoding with `partial_success` for
//! per-item rejections.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use serde::de::DeserializeOwned;
use vala_bifrost_redux::gate::{AuthContext, IngestError};
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

/// The OTLP signal type handled by a specific export endpoint.
///
/// Used to derive the physical Bifrost table name for per-signal error messages
/// so that `IngestBusy` / `IngressClosed` responses name the correct table
/// (`vala.traces.spans`, `vala.metrics.points`, `vala.logs.records`) rather
/// than always reporting the traces table.
#[derive(Clone, Copy, Debug)]
enum OtlpSignal {
    Traces,
    Metrics,
    Logs,
}

impl OtlpSignal {
    /// The fully-qualified Bifrost table name for this signal.
    fn table(self) -> &'static str {
        match self {
            Self::Traces => "vala.traces.spans",
            Self::Metrics => "vala.metrics.points",
            Self::Logs => "vala.logs.records",
        }
    }
}

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
/// Registers all three OTLP signal endpoints. Each is wired to its shared
/// decode→write core (`traces`/`metrics`/`logs`) with identical Content-Type
/// handling and `partial_success` semantics.
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
    let request: ExportTraceServiceRequest = decode_request(encoding, &body)
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
    let auth = caller_auth_context(&caller);

    let gate = state.gate.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::ServiceUnavailable {
            message: "Bifrost Gate is not available".to_owned(),
            details: serde_json::Value::Null,
        })
    })?;
    let outcome = gate
        .ingest_resource_spans(&auth, request)
        .await
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;

    tracing::Span::current().record("accepted_spans", outcome.accepted_spans);
    tracing::Span::current().record("rejected_spans", outcome.rejected_spans);

    let response = ExportTraceServiceResponse {
        partial_success: outcome.partial_success(),
    };
    Ok(encode_response(encoding, &response))
}

/// `POST /v1/metrics` — decode an OTLP metrics export and write it through the
/// shared decode→write core.
///
/// Accepts protobuf and protobuf-JSON, replies in the request's encoding, and
/// surfaces per-point rejections as OTLP `partial_success`.
#[tracing::instrument(
    name = "otlp.http.metrics.export",
    skip_all,
    fields(tenant, encoding, accepted_points, rejected_points)
)]
async fn export_metrics(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, WyrdErrorResponse> {
    tracing::Span::current().record("tenant", tracing::field::display(caller.data_tenant_id));
    let encoding = OtlpEncoding::from_headers(&headers);
    tracing::Span::current().record("encoding", tracing::field::debug(encoding));
    let request: ExportMetricsServiceRequest = decode_request(encoding, &body)
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
    let auth = caller_auth_context(&caller);

    let gate = state.gate.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::ServiceUnavailable {
            message: "Bifrost Gate is not available".to_owned(),
            details: serde_json::Value::Null,
        })
    })?;
    let outcome = gate
        .ingest_resource_metrics(&auth, request)
        .await
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;

    tracing::Span::current().record("accepted_points", outcome.accepted_points);
    tracing::Span::current().record("rejected_points", outcome.rejected_points);

    let response = ExportMetricsServiceResponse {
        partial_success: outcome.partial_success(),
    };
    Ok(encode_response(encoding, &response))
}

/// `POST /v1/logs` — decode an OTLP logs export and write it through the shared
/// decode→write core.
///
/// Accepts protobuf and protobuf-JSON, replies in the request's encoding, and
/// surfaces per-record rejections as OTLP `partial_success`.
#[tracing::instrument(
    name = "otlp.http.logs.export",
    skip_all,
    fields(tenant, encoding, accepted_records, rejected_records)
)]
async fn export_logs(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, WyrdErrorResponse> {
    tracing::Span::current().record("tenant", tracing::field::display(caller.data_tenant_id));
    let encoding = OtlpEncoding::from_headers(&headers);
    tracing::Span::current().record("encoding", tracing::field::debug(encoding));
    let request: ExportLogsServiceRequest = decode_request(encoding, &body)
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
    let auth = caller_auth_context(&caller);

    let gate = state.gate.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::ServiceUnavailable {
            message: "Bifrost Gate is not available".to_owned(),
            details: serde_json::Value::Null,
        })
    })?;
    let outcome = gate
        .ingest_resource_logs(&auth, request)
        .await
        .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;

    tracing::Span::current().record("accepted_records", outcome.accepted_records);
    tracing::Span::current().record("rejected_records", outcome.rejected_records);

    let response = ExportLogsServiceResponse {
        partial_success: outcome.partial_success(),
    };
    Ok(encode_response(encoding, &response))
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

/// Map an [`IngestError`] onto its HTTP response for the given OTLP signal.
///
/// Backpressure ([`IngestError::IngestBusy`]) becomes a retryable `429`, matching
/// the stable ingest catalog (the gRPC edge maps the same condition to retryable
/// `RESOURCE_EXHAUSTED`). The `signal` parameter provides the per-endpoint
/// physical table name so error messages name the correct target table rather
/// than always reporting the traces table. Every other ingest error renders its
/// stable `WYRD_VALA_*` [`WyrdError`] problem+json.
fn ingest_error_to_response(error: IngestError, signal: OtlpSignal) -> WyrdErrorResponse {
    if matches!(error, IngestError::IngestBusy) {
        return WyrdErrorResponse::from(WyrdError::from(BifrostError::IngestBusy {
            table: signal.table().to_owned(),
        }));
    }
    WyrdErrorResponse::from(ingest_error_to_wyrd(error, signal))
}

/// Map a non-backpressure [`IngestError`] onto its stable [`WyrdError`].
///
/// Preserves the `WYRD_VALA_*` / `WYRD_PERMISSION_*` code and HTTP status the
/// gRPC path already publishes for each condition. The `signal` parameter
/// provides the per-endpoint physical table name for writer-availability and
/// decode errors so each OTLP endpoint reports its own table in error messages.
fn ingest_error_to_wyrd(error: IngestError, signal: OtlpSignal) -> WyrdError {
    let fallback_table = signal.table().to_owned();
    match error {
        IngestError::Unauthenticated(message) => {
            WyrdError::from(BifrostError::IngestAuthentication { message })
        }
        IngestError::PrincipalUnresolved => WyrdError::from(BifrostError::PrincipalUnresolved),
        IngestError::RbacDenied { detail } => WyrdError::PermissionDeniedRbac {
            message: detail,
            details: serde_json::Value::Null,
        },
        IngestError::ReservedBuiltinWriteDenied { table } => {
            WyrdError::from(BifrostError::ReservedBuiltinWriteDenied { table })
        }
        IngestError::CardScopeDenied { card_ref } => {
            WyrdError::from(BifrostError::CardScopeDenied { card_ref })
        }
        IngestError::CardUnresolved { card_ref } => {
            WyrdError::from(BifrostError::CardUnresolved { card_ref })
        }
        IngestError::TableNotFound { table } => {
            WyrdError::from(BifrostError::TableNotFound { table })
        }
        IngestError::SchemaMismatch { table } => {
            WyrdError::from(BifrostError::FingerprintMismatch { table })
        }
        IngestError::PayloadTooLarge { bytes, .. } => {
            WyrdError::from(BifrostError::PayloadTooLarge {
                bytes: usize::try_from(bytes).unwrap_or(usize::MAX),
            })
        }
        IngestError::TooManyRows { rows, limit } => {
            WyrdError::from(BifrostError::IngestOversized { rows, limit })
        }
        IngestError::IngressClosed => WyrdError::from(BifrostError::WriterUnavailable {
            table: fallback_table,
        }),
        IngestError::WalDiskFull => WyrdError::from(BifrostError::WalDiskFull),
        // A malformed OTLP request body (bad protobuf or invalid JSON) is a
        // permanent client-side error. Map to the dedicated OTLP-request-validation
        // code (HTTP 400) — NOT to QueryInvalidSql, which is reserved for SQL query
        // validation failures and would give the client a nonsense remediation.
        IngestError::RequestValidation(detail) | IngestError::Decode(detail) => {
            WyrdError::from(BifrostError::IngestProtocol { message: detail })
        }
        // Backpressure is handled by ingest_error_to_response before this point.
        IngestError::IngestBusy => WyrdError::from(BifrostError::IngestBusy {
            table: fallback_table,
        }),
        IngestError::Internal(detail) => WyrdError::from(BifrostError::Internal { detail }),
    }
}

#[cfg(test)]
mod tests {
    use super::{OtlpEncoding, OtlpSignal, ingest_error_to_response, ingest_error_to_wyrd};
    use axum::http::{HeaderMap, StatusCode, header};
    use vala_bifrost_redux::gate::IngestError;
    use wyrd_spec::vala::error::BifrostError;

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

    // Backpressure is a coordinator-local condition. The stable catalog identity
    // is preserved on HTTP as 429, matching the gRPC resource-exhausted class.
    #[test]
    fn writer_busy_maps_to_429() {
        let response = ingest_error_to_response(IngestError::IngestBusy, OtlpSignal::Traces);
        let status = axum::response::IntoResponse::into_response(response).status();
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "OTLP/HTTP backpressure must preserve the catalog's 429"
        );
    }

    #[test]
    fn writer_busy_table_name_is_per_signal() {
        // IngressClosed must report the correct physical table for each OTLP signal.
        for (signal, expected_table) in [
            (OtlpSignal::Traces, "vala.traces.spans"),
            (OtlpSignal::Metrics, "vala.metrics.points"),
            (OtlpSignal::Logs, "vala.logs.records"),
        ] {
            let wyrd_err = ingest_error_to_wyrd(IngestError::IngressClosed, signal);
            match wyrd_err {
                wyrd_spec::error::WyrdError::Vala {
                    error: BifrostError::WriterUnavailable { table },
                } => {
                    assert_eq!(
                        table, expected_table,
                        "signal {signal:?} must name its own table"
                    );
                }
                other => panic!("expected WriterUnavailable, got {other:?}"),
            }
        }
    }

    #[test]
    fn every_ingest_error_preserves_catalog_code_and_http_status() {
        let cases = vec![
            (
                IngestError::Unauthenticated("x".to_owned()),
                "WYRD_VALA_401_INGEST_AUTH",
                401,
            ),
            (
                IngestError::RequestValidation("x".to_owned()),
                "WYRD_VALA_400_INGEST_PROTO",
                400,
            ),
            (
                IngestError::ReservedBuiltinWriteDenied {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE",
                403,
            ),
            (
                IngestError::CardScopeDenied {
                    card_ref: "c".to_owned(),
                },
                "WYRD_VALA_403_BIFROST_CARD_SCOPE",
                403,
            ),
            (
                IngestError::CardUnresolved {
                    card_ref: "c".to_owned(),
                },
                "WYRD_VALA_403_CARD_UNRESOLVED",
                403,
            ),
            (
                IngestError::PrincipalUnresolved,
                "WYRD_VALA_401_PRINCIPAL_UNRESOLVED",
                401,
            ),
            (
                IngestError::RbacDenied {
                    detail: "x".to_owned(),
                },
                "WYRD_PERMISSION_403_DENIED_RBAC",
                403,
            ),
            (
                IngestError::TableNotFound {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
                404,
            ),
            (
                IngestError::SchemaMismatch {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
                409,
            ),
            (
                IngestError::PayloadTooLarge { bytes: 2, limit: 1 },
                "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
                413,
            ),
            (
                IngestError::TooManyRows { rows: 2, limit: 1 },
                "WYRD_VALA_413_INGEST_OVERSIZED",
                413,
            ),
            (
                IngestError::IngressClosed,
                "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE",
                503,
            ),
            (IngestError::IngestBusy, "WYRD_VALA_429_INGEST_BUSY", 429),
            (IngestError::WalDiskFull, "WYRD_VALA_507_WAL_DISK_FULL", 507),
            (
                IngestError::Decode("x".to_owned()),
                "WYRD_VALA_400_INGEST_PROTO",
                400,
            ),
            (
                IngestError::Internal("x".to_owned()),
                "WYRD_VALA_500_BIFROST_INTERNAL",
                500,
            ),
        ];
        for (error, code, status) in cases {
            let catalog = ingest_error_to_wyrd(error, OtlpSignal::Traces);
            assert_eq!(catalog.code(), code);
            assert_eq!(catalog.status(), status);
        }
    }

    #[test]
    fn table_not_found_maps_to_404() {
        let response = ingest_error_to_response(
            IngestError::TableNotFound {
                table: "vala.traces.spans".to_owned(),
            },
            OtlpSignal::Traces,
        );
        let status = axum::response::IntoResponse::into_response(response).status();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn decode_error_maps_to_400_not_sql_error() {
        // A malformed OTLP body must map to WYRD_VALA_400_OTLP_REQUEST_MALFORMED
        // (HTTP 400), NOT to QueryInvalidSql. OTLP clients must not receive a
        // "invalid SQL query" remediation when they sent a bad protobuf payload.
        for (variant, label) in [
            (
                IngestError::Decode("OTLP protobuf decode failed: eof".to_owned()),
                "Decode",
            ),
            (
                IngestError::RequestValidation("mismatched batch_id".to_owned()),
                "RequestValidation",
            ),
        ] {
            let wyrd_err = ingest_error_to_wyrd(variant, OtlpSignal::Traces);
            match &wyrd_err {
                wyrd_spec::error::WyrdError::Vala {
                    error: BifrostError::IngestProtocol { .. },
                } => {
                    let _ = label;
                }
                other => panic!("{label}: expected IngestProtocol, got {other:?}"),
            }
            // Confirm HTTP 400 via the WyrdError status accessor.
            assert_eq!(
                wyrd_err.status(),
                400,
                "{label}: malformed OTLP body must be HTTP 400"
            );
            // Confirm the code is NOT the SQL code.
            assert_ne!(
                wyrd_err.code(),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
                "{label}: must not return the SQL error code for an OTLP decode failure"
            );
            assert_eq!(wyrd_err.code(), "WYRD_VALA_400_INGEST_PROTO");
        }
    }

    #[test]
    fn metrics_decode_error_names_metrics_table() {
        let wyrd_err = ingest_error_to_wyrd(
            IngestError::Decode("bad bytes".to_owned()),
            OtlpSignal::Metrics,
        );
        match wyrd_err {
            wyrd_spec::error::WyrdError::Vala {
                error: BifrostError::IngestProtocol { .. },
            } => {}
            other => panic!("expected IngestProtocol, got {other:?}"),
        }
    }

    #[test]
    fn logs_decode_error_names_logs_table() {
        let wyrd_err = ingest_error_to_wyrd(
            IngestError::Decode("bad bytes".to_owned()),
            OtlpSignal::Logs,
        );
        match wyrd_err {
            wyrd_spec::error::WyrdError::Vala {
                error: BifrostError::IngestProtocol { .. },
            } => {}
            other => panic!("expected IngestProtocol, got {other:?}"),
        }
    }
}
