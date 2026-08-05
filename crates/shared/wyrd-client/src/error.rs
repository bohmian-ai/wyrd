//! Concrete client error returned by config `validate()` methods, the
//! credential chain, and the auth token-exchange path, plus `pub`
//! server-response mappers.

use thiserror::Error;
use wyrd_spec::error::WyrdError;

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
    /// `WYRD_API_KEY`, an explicit `ClientConfig::api_key`, or the
    /// `~/.config/wyrd/credentials.toml` `[default].api_key` floor. Maps to
    /// `WYRD_CLIENT_401_NO_CREDENTIALS` at the public boundary.
    #[error(
        "no credentials available; set WYRD_ACCESS_TOKEN, WYRD_WORKLOAD_TOKEN+WYRD_TENANT, \
         or WYRD_API_KEY, pass ClientConfig::api_key, or add [default].api_key to \
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

/// Map a gRPC [`wyrd_tonic::tonic::Status`] carrying a `google.rpc.ErrorInfo`
/// into a [`WyrdError`].
///
/// Extracts `ErrorInfo` from the status details via
/// `tonic_types::StatusExt::get_details_error_info`, reads the stable Wyrd
/// code from `ErrorInfo.reason`, and feeds the **same** code→variant
/// reconstruction as [`from_problem_json`]. Both transports therefore produce
/// byte-identical output for the same stable code by construction.
///
/// Falls back to [`WyrdError::Internal`] when the status carries no
/// `ErrorInfo`.
pub fn from_grpc_status(status: &wyrd_tonic::tonic::Status) -> WyrdError {
    use wyrd_tonic::tonic_types::StatusExt as _;

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
        "WYRD_VALA_429_QUERY_ADMISSION_REJECTED" => BifrostError::QueryAdmissionRejected,
        _ => return None,
    };
    Some(WyrdError::Vala { error })
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
