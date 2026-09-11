//! Concrete client error returned by config `validate()` methods, the
//! credential chain, and the auth token-exchange path, plus `pub`
//! server-response mappers.

use thiserror::Error;
use wyrd_spec::error::WyrdError;
use wyrd_tonic::error::WYRD_ERROR_HEADER;

/// Concrete client-local error.
///
/// Produced by the config `validate()` methods, [`CredentialChain::resolve`],
/// and the [`AuthMiddleware`] token-exchange path. Server-side failures are
/// mapped separately via [`from_problem_json`] (HTTP) and [`from_grpc_status`]
/// (gRPC) into the unified [`WyrdError`] catalog; these client-local variants
/// are never echoed back by the server.
///
/// [`CredentialChain::resolve`]: crate::transport::credential::CredentialChain::resolve
/// [`AuthMiddleware`]: crate::auth::AuthMiddleware
#[derive(Debug, Error)]
pub enum WyrdClientError {
    /// Structural validation failed on a config struct. Maps to
    /// `WYRD_CLIENT_400_CONFIG_INVALID` at the public boundary. Not
    /// `WYRD_SPEC_400_VALIDATION`; that code is reserved for `wyrd-spec`
    /// contract violations only.
    #[error("config validation failed: {field}: {reason}")]
    Config {
        /// Dotted-path name of the offending config field, e.g.
        /// `"grpc_config.endpoint"` or `"http_config.timeout_ms"`. Echoed
        /// verbatim into the catalog payload's `field`.
        field: String,
        /// Short human reason, e.g. `"must not be empty"` or
        /// `"must be at least 1"`. Echoed verbatim into the catalog
        /// payload's `reason`.
        reason: String,
    },

    /// A request could not reach the server because the transport itself is
    /// unavailable — a DNS, TCP, TLS, or HTTP/gRPC connect failure. Produced by
    /// [`GrpcConnection::connect`], the token-exchange path in [`AuthMiddleware`],
    /// and [`MockTransport`] (`fail_on_drain`). Maps to
    /// `WYRD_CLIENT_503_TRANSPORT_DOWN` at the public boundary.
    ///
    /// [`GrpcConnection::connect`]: crate::transport::grpc::GrpcConnection::connect
    /// [`AuthMiddleware`]: crate::auth::AuthMiddleware
    /// [`MockTransport`]: crate::transport::mock::MockTransport
    #[error("transport unavailable ({transport}): {message}")]
    TransportDown {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`). Echoed into the
        /// catalog payload's `transport`.
        transport: String,
        /// Human-readable failure description. Echoed into the catalog
        /// payload's `message`.
        message: String,
    },

    /// The credential chain produced no usable credential. Configure at least
    /// one source: `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`+`WYRD_TENANT`,
    /// `WYRD_API_KEY`, an explicit `ClientConfig::credential`, or the
    /// `~/.config/wyrd/credentials.toml` `[default].api_key` floor. Maps to
    /// `WYRD_CLIENT_401_NO_CREDENTIALS` at the public boundary.
    ///
    /// `ClientConfig::credential` is the only credential field a caller sets by
    /// hand — one string classified by its own prefix — so this guidance must
    /// never name a per-grant field the config no longer has.
    #[error(
        "no credentials available; set WYRD_ACCESS_TOKEN, WYRD_WORKLOAD_TOKEN+WYRD_TENANT, \
         or WYRD_API_KEY, pass ClientConfig::credential, or add [default].api_key to \
         ~/.config/wyrd/credentials.toml"
    )]
    NoCredentials,
}

impl WyrdClientError {
    /// Stable `WYRD_CLIENT_*` code for this client-local error.
    ///
    /// These codes are the client-boundary projection of the variants; the
    /// server never emits them. Server-reported failures carry their own
    /// catalog code via [`WyrdError::code`].
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config { .. } => "WYRD_CLIENT_400_CONFIG_INVALID",
            Self::NoCredentials => "WYRD_CLIENT_401_NO_CREDENTIALS",
            Self::TransportDown { .. } => "WYRD_CLIENT_503_TRANSPORT_DOWN",
        }
    }
}

/// Map an HTTP `application/problem+json` response body into a [`WyrdError`].
///
/// Reads the stable `code` field and reconstructs the matching
/// [`WyrdError`] variant. Unknown codes are preserved in
/// [`WyrdError::UpstreamFailure`]`::details.original_code` so the original
/// code is never dropped. `pub` so other transport response sinks can reuse it.
pub fn from_problem_json(body: &serde_json::Value) -> WyrdError {
    let code = body["code"].as_str().unwrap_or("");
    let message = body["detail"].as_str().unwrap_or("").to_owned();
    let details = body
        .get("details")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    code_to_wyrd_error(code, message, details)
}

/// Maps a gRPC [`wyrd_tonic::tonic::Status`] into a [`WyrdError`].
///
/// Prefers Wyrd's canonical problem document and accepts `google.rpc.ErrorInfo`
/// from older or external gRPC services.
///
/// Falls back to [`WyrdError::Internal`] when the status carries no
/// `ErrorInfo`.
pub fn from_grpc_status(status: &wyrd_tonic::tonic::Status) -> WyrdError {
    use wyrd_tonic::tonic_types::StatusExt as _;

    if let Some(problem) = status
        .metadata()
        .get_bin(WYRD_ERROR_HEADER)
        .and_then(|value| value.to_bytes().ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        return from_problem_json(&problem);
    }
    if let Some(info) = status.get_details_error_info() {
        let details = if info.metadata.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::Value::Object(
                info.metadata
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect(),
            )
        };
        return code_to_wyrd_error(&info.reason, status.message().to_owned(), details);
    }
    WyrdError::Internal {
        message: status.message().to_owned(),
        details: serde_json::json!({}),
    }
}

/// Shared code→[`WyrdError`] reconstruction used by both transport mappers.
///
/// Both [`from_problem_json`] and [`from_grpc_status`] route through here so
/// identical codes always produce identical [`WyrdError`] values regardless of
/// which transport the response arrived on.
///
/// Reconstruction goes through [`WyrdError::from_code`], which covers the whole
/// catalog (every `{ message, details }` variant), so a `404`/`403`/`429`
/// response keeps its real code **and** [`WyrdError::status`] instead of
/// collapsing onto `502`. Codes with no matching variant fall back to
/// [`WyrdError::UpstreamFailure`] with the original code preserved in
/// `details.original_code` rather than silently dropped.
fn code_to_wyrd_error(code: &str, message: String, details: serde_json::Value) -> WyrdError {
    if let Some(bifrost) = bifrost_error_from_code(code, &message, &details) {
        return bifrost;
    }
    if let Some(reconstructed) = WyrdError::from_code(code, message.clone(), details.clone()) {
        return reconstructed;
    }
    WyrdError::UpstreamFailure {
        message: format!("server error: {message}"),
        details: serde_json::json!({
            "original_code": code,
            "original_details": details,
        }),
    }
}

/// Reconstructs Bifrost's typed error variants from their stable wire identity.
fn bifrost_error_from_code(
    code: &str,
    message: &str,
    details: &serde_json::Value,
) -> Option<WyrdError> {
    use wyrd_spec::vala::error::BifrostError;

    let table_from_message = |prefix: &str| {
        message
            .strip_prefix(prefix)
            .or_else(|| details.get("table").and_then(serde_json::Value::as_str))
            .unwrap_or("<unknown>")
            .to_owned()
    };
    let error = match code {
        "WYRD_VALA_429_INGEST_BUSY" => BifrostError::IngestBusy {
            table: table_from_message("ingest coordinator busy for table ")
                .trim_end_matches(" — local buffer full")
                .to_owned(),
        },
        "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND" => BifrostError::TableNotFound {
            table: table_from_message("bifrost table not found: "),
        },
        "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH" => BifrostError::FingerprintMismatch {
            table: table_from_message("schema fingerprint mismatch: "),
        },
        "WYRD_VALA_400_QUERY_INVALID_SQL" => BifrostError::QueryInvalidSql {
            detail: message
                .strip_prefix("invalid or unsupported query SQL: ")
                .unwrap_or(message)
                .to_owned(),
        },
        "WYRD_VALA_503_ORACLE_ROLE_UNAVAILABLE" => BifrostError::OracleRoleUnavailable,
        // Ingest backpressure. A caller that cannot see this code cannot tell
        // "the pod is out of WAL space, retry when it drains" from a generic
        // upstream failure, which is the difference between a client that waits
        // and a client that gives up on rows the server would have taken.
        "WYRD_VALA_507_WAL_DISK_FULL" => BifrostError::WalDiskFull,
        "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND" => BifrostError::RunningQueryNotFound,
        "WYRD_VALA_409_RUNNING_QUERY_CONFLICT" => BifrostError::RunningQueryConflict,
        "WYRD_VALA_503_RUNNING_QUERY_CONTROL_UNAVAILABLE" => {
            BifrostError::RunningQueryControlUnavailable
        }
        "WYRD_VALA_500_AUDIT_UNAVAILABLE" => BifrostError::AuditUnavailable {
            detail: message
                .strip_prefix("audit outbox unavailable: ")
                .unwrap_or(message)
                .to_owned(),
        },
        "WYRD_VALA_429_QUERY_ADMISSION_REJECTED" => BifrostError::QueryAdmissionRejected,
        "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE" => BifrostError::QueryMemoryRequestTooLarge,
        "WYRD_VALA_500_QUERY_EXECUTION_FAILED" => BifrostError::QueryExecutionFailed,
        // Every remaining closed query terminal. Without these a caller cannot
        // tell a retryable timeout from a permanent authorization refusal: both
        // arrive as an untyped upstream failure carrying only prose, and any
        // retry policy built on that is matching on Display text.
        "WYRD_VALA_504_QUERY_TIMEOUT" => BifrostError::QueryTimeout,
        "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE" => BifrostError::QueryVisibilityUnavailable,
        "WYRD_VALA_500_QUERY_TENANT_INVARIANT" => BifrostError::QueryTenantInvariant,
        "WYRD_VALA_500_QUERY_RECONCILIATION_INVARIANT" => {
            BifrostError::QueryReconciliationInvariant
        }
        "WYRD_VALA_403_QUERY_PEER_SECURITY" => BifrostError::QueryPeerSecurity,
        "WYRD_VALA_403_QUERY_FORBIDDEN" => BifrostError::QueryForbidden,
        "WYRD_VALA_502_QUERY_STREAM_PROTOCOL" => BifrostError::QueryStreamProtocol,
        "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE" => BifrostError::QueryStreamIncomplete,
        "WYRD_VALA_503_QUERY_AUDIT_UNAVAILABLE" => BifrostError::QueryAuditUnavailable,
        "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE" => BifrostError::QueryResultTooLarge,
        // The enforced ceiling is configured, not universal. A caller that only
        // learns "too large" cannot resize its batch to fit; it has to guess at
        // a limit the server never promised, so both bounds are reconstructed.
        "WYRD_VALA_413_PAYLOAD_TOO_LARGE" => {
            let (bytes, limit) = payload_bounds_from(message, details);
            BifrostError::PayloadTooLarge { bytes, limit }
        }
        "WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE" => {
            let (value, past_bound, future_bound) = event_time_window_bounds_from_message(message);
            BifrostError::EventTimeOutOfRange {
                value,
                past_bound,
                future_bound,
            }
        }
        _ => return None,
    };
    Some(WyrdError::Vala { error })
}

/// Reconstruct the three `EventTimeOutOfRange` fields from a wire error message.
///
/// The server renders this Bifrost error through the variant's `#[error(...)]`
/// form, `"event time out of acceptance window: {value} not in [{past_bound},
/// {future_bound}]"`, and that `Display` text is the only field source carried
/// on **both** transports: the HTTP `problem+json` `detail` and the gRPC
/// `Status` message. The gRPC `ErrorInfo` metadata for Bifrost errors is empty,
/// so the structured `details` object cannot be relied on across transports —
/// which is why the sibling single-field arms parse the message too. The three
/// values are epoch-microsecond strings with no embedded separators, so the
/// bracketed list splits unambiguously. On any shape mismatch every field falls
/// back to `"<unknown>"`, mirroring the `<unknown>` fallback the single-field
/// arms use, so reconstruction never fails the typed-variant mapping.
fn event_time_window_bounds_from_message(message: &str) -> (String, String, String) {
    let parsed = message
        .strip_prefix("event time out of acceptance window: ")
        .and_then(|rest| rest.split_once(" not in ["))
        .and_then(|(value, bounds)| {
            let (past_bound, future_bound) = bounds.strip_suffix(']')?.split_once(", ")?;
            Some((
                value.to_owned(),
                past_bound.to_owned(),
                future_bound.to_owned(),
            ))
        });
    parsed.unwrap_or_else(|| {
        let unknown = "<unknown>".to_owned();
        (unknown.clone(), unknown.clone(), unknown)
    })
}

/// Recover the measured byte count and enforced ceiling from a payload-limit
/// failure.
///
/// The gRPC projection carries only the stable code and the rendered detail
/// message, so the bounds are parsed from that message first and the structured
/// HTTP `details.data` payload is used as the fallback. Both transports
/// therefore reconstruct the identical typed error. Unparseable input yields
/// zeroes rather than a panic, because a malformed upstream response must not
/// take down the caller.
fn payload_bounds_from(message: &str, details: &serde_json::Value) -> (usize, usize) {
    let from_message = message
        .strip_prefix("ingest payload too large: ")
        .and_then(|rest| rest.split_once(" bytes exceeds the "))
        .and_then(|(bytes, rest)| {
            let limit = rest.strip_suffix(" byte limit")?;
            Some((bytes.parse().ok()?, limit.parse().ok()?))
        });
    from_message.unwrap_or_else(|| {
        let field = |name: &str| {
            details
                .get("data")
                .and_then(|data| data.get(name))
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0)
        };
        (field("bytes"), field("limit"))
    })
}

#[cfg(test)]
mod tests {
    use super::{WyrdClientError, from_problem_json};

    #[test]
    fn known_code_reconstructs_correct_variant() {
        let body = serde_json::json!({
            "code": "WYRD_SPEC_404_NOT_FOUND",
            "detail": "card not found",
            "details": {},
        });
        let err = from_problem_json(&body);
        assert_eq!(err.code(), "WYRD_SPEC_404_NOT_FOUND");
    }

    #[test]
    fn unknown_code_preserved_in_catch_all() {
        let body = serde_json::json!({
            "code": "WYRD_CUSTOM_999_EXPERIMENTAL",
            "detail": "experimental error",
            "details": {},
        });
        let err = from_problem_json(&body);
        assert_eq!(
            err.code(),
            "WYRD_SPEC_502_UPSTREAM_FAILURE",
            "unknown code must fall through to UpstreamFailure catch-all"
        );
        let json = err.as_problem_json();
        assert_eq!(
            json["details"]["original_code"].as_str(),
            Some("WYRD_CUSTOM_999_EXPERIMENTAL"),
            "original code must be preserved in catch-all variant details"
        );
    }

    /// Bifrost-specific codes retain their typed status instead of becoming upstream failures.
    #[test]
    fn bifrost_grpc_codes_keep_their_wire_status() {
        let not_found = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
            "detail": "bifrost table not found: vala.bifrost.missing",
            "details": {},
        }));
        assert_eq!(not_found.status(), 404);
        assert_eq!(not_found.code(), "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");

        let conflict = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
            "detail": "schema fingerprint mismatch: vala.bifrost.events",
            "details": {},
        }));
        assert_eq!(conflict.status(), 409);
        assert_eq!(
            conflict.code(),
            "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
        );

        let invalid_sql = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_400_QUERY_INVALID_SQL",
            "detail": "invalid or unsupported query SQL: parser rejected SELECT FROM",
            "details": {},
        }));
        assert_eq!(invalid_sql.status(), 400);
        assert_eq!(invalid_sql.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");

        let admission = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_429_QUERY_ADMISSION_REJECTED",
            "detail": "query admission rejected",
            "details": {},
        }));
        assert_eq!(admission.status(), 429);
        assert_eq!(admission.code(), "WYRD_VALA_429_QUERY_ADMISSION_REJECTED");

        let oversized = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE",
            "detail": "query memory request too large",
            "details": {},
        }));
        assert_eq!(oversized.status(), 422);
        assert_eq!(
            oversized.code(),
            "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE"
        );

        for (code, status) in [
            ("WYRD_VALA_503_ORACLE_ROLE_UNAVAILABLE", 503),
            ("WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND", 404),
            ("WYRD_VALA_409_RUNNING_QUERY_CONFLICT", 409),
            ("WYRD_VALA_503_RUNNING_QUERY_CONTROL_UNAVAILABLE", 503),
            ("WYRD_VALA_500_AUDIT_UNAVAILABLE", 500),
        ] {
            let error = from_problem_json(&serde_json::json!({
                "code": code,
                "detail": "running query lifecycle result",
                "details": {},
            }));
            assert_eq!(error.code(), code);
            assert_eq!(error.status(), status);
        }
    }

    /// Proves the stable `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` code
    /// reconstructs the typed `BifrostError::EventTimeOutOfRange` variant with
    /// its real permanent 400 status and preserved code — never the
    /// retryable-looking 502 `UpstreamFailure` an unmapped Bifrost code would
    /// collapse onto — and that the offending value and both window bounds are
    /// recovered from the wire message, the only field source carried on both
    /// the HTTP and gRPC transports.
    #[test]
    fn bifrost_event_time_out_of_range_reconstructs_typed_400() {
        let err = from_problem_json(&serde_json::json!({
            "code": "WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE",
            "detail": "event time out of acceptance window: 100 not in [200, 300]",
            "details": {},
        }));
        assert_eq!(err.code(), "WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE");
        assert_eq!(
            err.status(),
            400,
            "out-of-range is a permanent 400, not a retryable 502 UpstreamFailure"
        );
        let json = err.as_problem_json();
        assert_eq!(
            json["details"]["variant"].as_str(),
            Some("event_time_out_of_range"),
            "typed variant must be reconstructed, not the UpstreamFailure catch-all"
        );
        assert_eq!(json["details"]["data"]["value"].as_str(), Some("100"));
        assert_eq!(json["details"]["data"]["past_bound"].as_str(), Some("200"));
        assert_eq!(
            json["details"]["data"]["future_bound"].as_str(),
            Some("300")
        );
    }

    #[test]
    fn previously_collapsed_code_now_keeps_its_status() {
        // A code that maps to a typed variant must reconstruct that variant and
        // report its real status, not collapse onto UpstreamFailure's 502. Here
        // a 403 code must surface as status 403.
        let body = serde_json::json!({
            "code": "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN",
            "detail": "delegated-token guard failed",
            "details": {},
        });
        let err = from_problem_json(&body);
        assert_eq!(err.code(), "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
        assert_eq!(
            err.status(),
            403,
            "status must survive the round-trip, not collapse to 502"
        );
    }

    #[test]
    fn client_error_code_accessor_reports_stable_codes() {
        assert_eq!(
            WyrdClientError::Config {
                field: "f".to_owned(),
                reason: "r".to_owned(),
            }
            .code(),
            "WYRD_CLIENT_400_CONFIG_INVALID"
        );
        assert_eq!(
            WyrdClientError::NoCredentials.code(),
            "WYRD_CLIENT_401_NO_CREDENTIALS"
        );
        assert_eq!(
            WyrdClientError::TransportDown {
                transport: "http".to_owned(),
                message: "refused".to_owned(),
            }
            .code(),
            "WYRD_CLIENT_503_TRANSPORT_DOWN"
        );
    }

    /// The actionable half of the no-credentials error names the field that
    /// still exists.
    ///
    /// `ClientConfig` carries one `credential`; the former per-grant `api_key`
    /// field is gone, so telling a caller to pass it would send them to an API
    /// that no longer compiles.
    #[test]
    fn no_credentials_guidance_names_the_credential_field() {
        let message = WyrdClientError::NoCredentials.to_string();
        assert!(
            message.contains("ClientConfig::credential"),
            "guidance must name the field a caller can actually set: {message}"
        );
        assert!(
            !message.contains("ClientConfig::api_key"),
            "guidance must not name the removed per-grant field: {message}"
        );
    }

    #[test]
    fn client_error_has_only_expected_variants() {
        // Smoke test: verify Config, TransportDown, NoCredentials compile.
        // PayloadTooLarge and FlushTimeout are intentionally absent — those
        // variants moved to wyrd-queue.
        let _c = WyrdClientError::Config {
            field: "f".to_owned(),
            reason: "r".to_owned(),
        };
        let _d = WyrdClientError::TransportDown {
            transport: "grpc".to_owned(),
            message: "refused".to_owned(),
        };
        let _n = WyrdClientError::NoCredentials;
    }

    mod grpc_convergence {
        use std::collections::HashMap;

        use wyrd_tonic::tonic::Code;
        use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};

        use crate::error::{from_grpc_status, from_problem_json};

        /// Capacity and poison codes reconstruct identically across both transports.
        #[test]
        fn capacity_http_and_grpc_reconstruct_retryability_identically() {
            for (code, status, grpc_code) in [
                (
                    "WYRD_VALA_429_QUERY_ADMISSION_REJECTED",
                    429,
                    Code::ResourceExhausted,
                ),
                (
                    "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE",
                    422,
                    Code::InvalidArgument,
                ),
                ("WYRD_VALA_500_QUERY_EXECUTION_FAILED", 500, Code::Internal),
            ] {
                let details = ErrorDetails::with_error_info(code, "wyrd.dev", HashMap::new());
                let grpc = wyrd_tonic::tonic::Status::with_error_details(
                    grpc_code,
                    "capacity result",
                    details,
                );
                let http = serde_json::json!({
                    "code": code,
                    "status": status,
                    "detail": "capacity result",
                    "details": {},
                });
                let from_grpc = from_grpc_status(&grpc);
                let from_http = from_problem_json(&http);
                assert_eq!(from_grpc.code(), code);
                assert_eq!(from_grpc.status(), status);
                assert_eq!(from_grpc.as_problem_json(), from_http.as_problem_json());
            }
        }

        #[test]
        fn grpc_and_http_same_code_produce_equal_wyrd_error() {
            let details = ErrorDetails::with_error_info(
                "WYRD_SPEC_404_NOT_FOUND",
                "wyrd.dev",
                HashMap::new(),
            );
            let status = wyrd_tonic::tonic::Status::with_error_details(
                Code::NotFound,
                "card not found",
                details,
            );

            let body = serde_json::json!({
                "code": "WYRD_SPEC_404_NOT_FOUND",
                "detail": "card not found",
                "details": {},
            });

            let from_grpc = from_grpc_status(&status);
            let from_http = from_problem_json(&body);

            assert_eq!(
                from_grpc.as_problem_json(),
                from_http.as_problem_json(),
                "gRPC and HTTP mappers must produce identical WyrdError for the same stable code"
            );
        }

        /// The enforced payload ceiling must reach the caller identically over
        /// both transports. A client that cannot read `limit` from the
        /// reconstructed error cannot resize its batch to fit; it can only
        /// guess at a ceiling the server never promised.
        ///
        /// # Panics
        ///
        /// Panics when either transport loses the code, status, measured bytes,
        /// enforced limit, detail, or remediation, or when the two disagree.
        #[test]
        fn payload_limit_reconstructs_identically_across_transports() {
            let detail = "ingest payload too large: 41943040 bytes exceeds the 8388608 byte limit";
            let grpc = wyrd_tonic::tonic::Status::with_error_details(
                Code::ResourceExhausted,
                detail,
                ErrorDetails::with_error_info(
                    "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
                    "wyrd.dev",
                    HashMap::new(),
                ),
            );
            let http = serde_json::json!({
                "code": "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
                "status": 413,
                "detail": detail,
                "details": { "variant": "payload_too_large", "data": { "bytes": 41_943_040, "limit": 8_388_608 } },
            });

            let from_grpc = from_grpc_status(&grpc);
            let from_http = from_problem_json(&http);
            assert_eq!(
                from_grpc.as_problem_json(),
                from_http.as_problem_json(),
                "both transports must reconstruct the same payload-limit error"
            );

            for reconstructed in [&from_grpc, &from_http] {
                assert_eq!(reconstructed.code(), "WYRD_VALA_413_PAYLOAD_TOO_LARGE");
                assert_eq!(reconstructed.status(), 413);
                assert_eq!(reconstructed.to_string(), detail);
                let problem = reconstructed.as_problem_json();
                assert_eq!(
                    problem["details"]["data"]["bytes"], 41_943_040,
                    "measured bytes must survive the transport: {problem}"
                );
                assert_eq!(
                    problem["details"]["data"]["limit"], 8_388_608,
                    "the enforced limit must survive the transport: {problem}"
                );
                let remediation = problem["remediation"].as_str().unwrap_or_default();
                assert!(
                    !remediation.contains("32 MiB"),
                    "remediation must not contradict the supplied limit: {problem}"
                );
                assert!(
                    remediation.contains("limit"),
                    "remediation must point at the supplied limit: {problem}"
                );
            }
        }

        #[test]
        fn grpc_unknown_code_preserved_in_catch_all() {
            let details = ErrorDetails::with_error_info(
                "WYRD_CUSTOM_999_UNKNOWN",
                "wyrd.dev",
                HashMap::new(),
            );
            let status = wyrd_tonic::tonic::Status::with_error_details(
                Code::Unknown,
                "unknown server error",
                details,
            );
            let err = from_grpc_status(&status);
            assert_eq!(
                err.code(),
                "WYRD_SPEC_502_UPSTREAM_FAILURE",
                "unknown gRPC code must fall through to UpstreamFailure catch-all"
            );
            let json = err.as_problem_json();
            assert_eq!(
                json["details"]["original_code"].as_str(),
                Some("WYRD_CUSTOM_999_UNKNOWN"),
                "original code must be preserved in catch-all variant details"
            );
        }
    }
}
