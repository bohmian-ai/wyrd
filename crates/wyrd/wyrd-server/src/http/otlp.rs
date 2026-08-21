//! OTLP/HTTP transport for the Wyrd trace collector.
//!
//! Wyrd is a drop-in OTLP backend over HTTP as well as gRPC: `POST /v1/traces`
//! accepts both `application/x-protobuf` (the OTLP default) and
//! `application/json` (OTLP protobuf-JSON), preflights the payload and constructs
//! the same fixed-capacity typed request the gRPC adapter uses. The typed
//! request and move-only decode owner then cross routing-only Gate; Scribe owns
//! catalog resolution, projection, binding, material admission, WAL durability,
//! visibility, and acknowledgment.
//!
//! Content-Type handling (shared by all three signals):
//! - `application/x-protobuf` → bounded wire preflight followed by direct,
//!   fixed-capacity generated-message construction.
//! - `application/json` → bounded schema-aware preflight followed by direct,
//!   exact-capacity generated-message construction.
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
//! bounded adapter decode, owner-backed Gate routing
//! (`vala_bifrost_redux::gate::Gate::ingest_decoded_resource_metrics` /
//! `Gate::ingest_decoded_resource_logs`),
//! then a response encoded in the request's encoding with `partial_success` for
//! per-item rejections.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use vala_bifrost_redux::contracts::DecodedOtlp;
use vala_bifrost_redux::gate::{AuthContext, IngestError};
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceResponse;
use wyrd_tonic::prost::Message;

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::otlp_decode::{decode_trace_protobuf, preflight_trace_protobuf};
use crate::otlp_json::{preflight_logs_json, preflight_metrics_json, preflight_trace_json};
use crate::otlp_logs_decode::{decode_logs_protobuf, preflight_logs_protobuf};
use crate::otlp_logs_json::decode_logs_json;
use crate::otlp_metrics_decode::{decode_metrics_protobuf, preflight_metrics_protobuf};
use crate::otlp_metrics_json::decode_metrics_json;
use crate::otlp_trace_json::decode_trace_json;
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
///
/// # Errors
///
/// Returns [`WyrdErrorResponse`] when Gate is unavailable, protobuf/JSON
/// preflight or fixed-capacity decode fails, decode ownership cannot be
/// reserved/adopted, Gate rejects routing or transport limits, or Scribe
/// rejects material admission or durable ingest.
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
    let auth = caller_auth_context(&caller);

    let bifrost = Arc::clone(&state.bifrost);
    let (owner, request) = match encoding {
        OtlpEncoding::Protobuf => {
            let plan = preflight_trace_protobuf(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            let owner = bifrost
                .reserve_otlp_decode(plan.decode_bytes)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            let request = decode_trace_protobuf(&body)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            (owner, request)
        }
        OtlpEncoding::Json => {
            let plan = preflight_trace_json(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            let total = plan
                .decode_bytes
                .checked_add(plan.scratch_bytes)
                .ok_or_else(|| {
                    ingest_error_to_response(
                        IngestError::Decode("OTLP JSON capacity overflow".to_owned()),
                        OtlpSignal::Traces,
                    )
                })?;
            let mut owner = bifrost
                .reserve_otlp_decode(total)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            let scratch = owner
                .split_scratch(plan.scratch_bytes)
                .map_err(IngestError::from_scribe)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            let request = decode_trace_json(&body, plan.decode_bytes)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Traces))?;
            drop(scratch);
            (owner, request)
        }
    };
    let outcome = bifrost
        .ingest_decoded_resource_spans(&auth, DecodedOtlp::new(request, body.len(), owner))
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
///
/// # Errors
///
/// Returns [`WyrdErrorResponse`] when Gate is unavailable, protobuf/JSON
/// preflight or fixed-capacity decode fails, decode ownership cannot be
/// reserved/adopted, Gate rejects routing or transport limits, or Scribe
/// rejects material admission or durable ingest.
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
    let auth = caller_auth_context(&caller);

    let bifrost = Arc::clone(&state.bifrost);
    let (owner, request) = match encoding {
        OtlpEncoding::Protobuf => {
            let plan = preflight_metrics_protobuf(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            let owner = bifrost
                .reserve_otlp_decode(plan.decode_bytes)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            let request = decode_metrics_protobuf(&body, plan)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            (owner, request)
        }
        OtlpEncoding::Json => {
            let plan = preflight_metrics_json(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            let total = plan
                .decode_bytes
                .checked_add(plan.scratch_bytes)
                .ok_or_else(|| {
                    ingest_error_to_response(
                        IngestError::Decode("OTLP JSON capacity overflow".to_owned()),
                        OtlpSignal::Metrics,
                    )
                })?;
            let mut owner = bifrost
                .reserve_otlp_decode(total)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            let scratch = owner
                .split_scratch(plan.scratch_bytes)
                .map_err(IngestError::from_scribe)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            let request: ExportMetricsServiceRequest =
                decode_metrics_json(&body, plan.decode_bytes)
                    .map_err(|e| ingest_error_to_response(e, OtlpSignal::Metrics))?;
            drop(scratch);
            (owner, request)
        }
    };
    let outcome = bifrost
        .ingest_decoded_resource_metrics(&auth, DecodedOtlp::new(request, body.len(), owner))
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
///
/// # Errors
///
/// Returns [`WyrdErrorResponse`] when Gate is unavailable, protobuf/JSON
/// preflight or fixed-capacity decode fails, decode ownership cannot be
/// reserved/adopted, Gate rejects routing or transport limits, or Scribe
/// rejects material admission or durable ingest.
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
    let auth = caller_auth_context(&caller);

    let bifrost = Arc::clone(&state.bifrost);
    let (owner, request) = match encoding {
        OtlpEncoding::Protobuf => {
            let plan = preflight_logs_protobuf(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            let owner = bifrost
                .reserve_otlp_decode(plan.decode_bytes)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            let request = decode_logs_protobuf(&body)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            (owner, request)
        }
        OtlpEncoding::Json => {
            let plan = preflight_logs_json(&body, bifrost.gate().otlp_wire_limits())
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            let total = plan
                .decode_bytes
                .checked_add(plan.scratch_bytes)
                .ok_or_else(|| {
                    ingest_error_to_response(
                        IngestError::Decode("OTLP JSON capacity overflow".to_owned()),
                        OtlpSignal::Logs,
                    )
                })?;
            let mut owner = bifrost
                .reserve_otlp_decode(total)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            let scratch = owner
                .split_scratch(plan.scratch_bytes)
                .map_err(IngestError::from_scribe)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            let request: ExportLogsServiceRequest = decode_logs_json(&body, plan.decode_bytes)
                .map_err(|e| ingest_error_to_response(e, OtlpSignal::Logs))?;
            drop(scratch);
            (owner, request)
        }
    };
    let outcome = bifrost
        .ingest_decoded_resource_logs(&auth, DecodedOtlp::new(request, body.len(), owner))
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

/// Map an [`IngestError`] onto the single Gate-owned HTTP error projection.
///
/// The signal contributes only its physical table hint. Identity, HTTP status,
/// and the matching gRPC/ErrorInfo projection are selected by
/// [`IngestError::to_wyrd_error`], so OTLP endpoints cannot drift into a second
/// error taxonomy.
fn ingest_error_to_response(error: IngestError, signal: OtlpSignal) -> WyrdErrorResponse {
    WyrdErrorResponse::from(error.to_wyrd_error(signal.table()))
}

#[cfg(test)]
mod tests {
    use super::{OtlpEncoding, OtlpSignal, ingest_error_to_response};
    use axum::http::{HeaderMap, StatusCode, header};
    use vala_bifrost_redux::gate::IngestError;
    use wyrd_spec::vala::error::BifrostError;

    /// Build request headers for an encoding-selection test.
    fn headers_with(content_type: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_str(content_type).expect("valid test header"),
        );
        headers
    }

    /// Verify OTLP's protobuf default when Content-Type is absent.
    #[test]
    fn missing_content_type_defaults_to_protobuf() {
        assert_eq!(
            OtlpEncoding::from_headers(&HeaderMap::new()),
            OtlpEncoding::Protobuf
        );
    }

    /// Verify JSON selection accepts media-type parameters and casing.
    #[test]
    fn json_content_type_selects_json() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("application/json")),
            OtlpEncoding::Json
        );
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("Application/JSON; charset=utf-8")),
            OtlpEncoding::Json
        );
    }

    /// Verify the explicit protobuf media type selects protobuf encoding.
    #[test]
    fn protobuf_content_type_selects_protobuf() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("application/x-protobuf")),
            OtlpEncoding::Protobuf
        );
    }

    /// Verify unrecognized media types fail closed to OTLP's protobuf default.
    #[test]
    fn unknown_content_type_defaults_to_protobuf() {
        assert_eq!(
            OtlpEncoding::from_headers(&headers_with("text/plain")),
            OtlpEncoding::Protobuf
        );
    }

    /// Verify Gate's busy identity renders as retryable HTTP 429.
    #[test]
    fn writer_busy_maps_to_429() {
        let response = ingest_error_to_response(
            IngestError::IngestBusy {
                table: "vala.traces.spans".to_owned(),
            },
            OtlpSignal::Traces,
        );
        let status = axum::response::IntoResponse::into_response(response).status();
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    /// Verify endpoint-specific table context survives Gate's canonical projection.
    #[test]
    fn writer_closed_table_name_is_per_signal() {
        for (signal, expected_table) in [
            (OtlpSignal::Traces, "vala.traces.spans"),
            (OtlpSignal::Metrics, "vala.metrics.points"),
            (OtlpSignal::Logs, "vala.logs.records"),
        ] {
            let wyrd_err = IngestError::IngressClosed.to_wyrd_error(signal.table());
            match wyrd_err {
                wyrd_spec::error::WyrdError::Vala {
                    error: BifrostError::WriterUnavailable { table },
                } => assert_eq!(table, expected_table),
                other => panic!("expected WriterUnavailable, got {other:?}"),
            }
        }
    }

    /// Verify every ingest condition keeps one catalog code and HTTP status.
    #[test]
    fn every_ingest_error_preserves_catalog_code_and_http_status() {
        use axum::response::IntoResponse;
        let cases = vec![
            (
                IngestError::Unauthenticated("x".to_owned()),
                "WYRD_VALA_401_INGEST_AUTH",
                401,
            ),
            (
                IngestError::RequestValidation("x".to_owned()),
                "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
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
            (
                IngestError::IngestBusy {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_429_INGEST_BUSY",
                429,
            ),
            (IngestError::WalDiskFull, "WYRD_VALA_507_WAL_DISK_FULL", 507),
            (
                IngestError::Decode("x".to_owned()),
                "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
                400,
            ),
            (
                IngestError::Internal("x".to_owned()),
                "WYRD_VALA_500_BIFROST_INTERNAL",
                500,
            ),
        ];
        for (error, code, status) in cases {
            let public = error.to_wyrd_error(OtlpSignal::Traces.table());
            assert_eq!(public.code(), code);
            assert_eq!(public.status(), status);
            assert_eq!(
                ingest_error_to_response(error, OtlpSignal::Traces)
                    .into_response()
                    .status()
                    .as_u16(),
                status
            );
        }
    }

    /// Verify table-not-found uses the catalog's 404 response.
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

    /// Verify malformed OTLP requests retain the dedicated code and table.
    #[test]
    fn decode_error_maps_to_400_with_signal_table_context() {
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
            let wyrd_err = variant.to_wyrd_error(OtlpSignal::Traces.table());
            match &wyrd_err {
                wyrd_spec::error::WyrdError::Vala {
                    error: BifrostError::OtlpRequestMalformed { table, .. },
                } => assert_eq!(table, "vala.traces.spans", "{label} table context"),
                other => panic!("{label}: expected OtlpRequestMalformed, got {other:?}"),
            }
            assert_eq!(wyrd_err.status(), 400);
            assert_eq!(wyrd_err.code(), "WYRD_VALA_400_OTLP_REQUEST_MALFORMED");
        }
    }

    /// Verify malformed metrics requests use the metrics physical table.
    #[test]
    fn metrics_decode_error_names_metrics_table() {
        let wyrd_err =
            IngestError::Decode("bad bytes".to_owned()).to_wyrd_error(OtlpSignal::Metrics.table());
        match wyrd_err {
            wyrd_spec::error::WyrdError::Vala {
                error: BifrostError::OtlpRequestMalformed { table, .. },
            } => assert_eq!(table, "vala.metrics.points"),
            other => panic!("expected OtlpRequestMalformed, got {other:?}"),
        }
    }

    /// Verify malformed logs requests use the logs physical table.
    #[test]
    fn logs_decode_error_names_logs_table() {
        let wyrd_err =
            IngestError::Decode("bad bytes".to_owned()).to_wyrd_error(OtlpSignal::Logs.table());
        match wyrd_err {
            wyrd_spec::error::WyrdError::Vala {
                error: BifrostError::OtlpRequestMalformed { table, .. },
            } => assert_eq!(table, "vala.logs.records"),
            other => panic!("expected OtlpRequestMalformed, got {other:?}"),
        }
    }
}
