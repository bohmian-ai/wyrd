//! Concrete client error returned by `WyrdClient::new()` and config
//! `validate()` methods, plus `pub` server-response mappers.

use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Concrete client error returned by `WyrdClient::new()`, config
/// `validate()` methods, and the credential chain.
///
/// Server-side failures are mapped via [`from_problem_json`] (HTTP) and
/// [`from_grpc_status`] (gRPC) into the unified [`WyrdError`] catalog. These
/// client-local variants are never echoed back by the server.
#[derive(Debug, Error)]
pub enum WyrdClientError {
    /// Structural validation failed on a config struct. Maps to
    /// `WYRD_CLIENT_400_CONFIG_INVALID` at the public boundary
    /// (`WyrdClient::new()` and the Python wrapper). Not `WYRD_SPEC_400_VALIDATION`;
    /// that code is reserved for `wyrd-spec` contract violations only.
    #[error("config validation failed: {field}: {reason}")]
    Config {
        /// Dotted-path name of the offending config field, e.g.
        /// `"grpc_config.endpoint"` or `"queue_config.sample_ratio"`. Echoed
        /// verbatim into the catalog payload's `field`.
        field: String,
        /// Short human reason, e.g. `"must not be empty"` or
        /// `"must be in [0.0, 1.0]"`. Echoed verbatim into the catalog
        /// payload's `reason`.
        reason: String,
    },

    /// A flush failed because the transport itself is unavailable
    /// (DNS, TCP, or gRPC/HTTP connect failure). The buffered records remain
    /// in the queue; `Flushable::len()` reflects what was not flushed. Maps
    /// to `WYRD_CLIENT_503_TRANSPORT_DOWN` at the public boundary.
    #[error("transport unavailable ({transport}): {message}")]
    TransportDown {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`). Echoed into the
        /// catalog payload's `transport`.
        transport: String,
        /// Human-readable failure description. Echoed into the catalog
        /// payload's `message`.
        message: String,
    },

    /// The credential chain produced no usable credential. The caller must
    /// configure at least one source (explicit token, workload provider, or
    /// API key). Maps to `WYRD_CLIENT_401_NO_CREDENTIALS` at the public boundary.
    #[error("no credentials available; configure WYRD_ACCESS_TOKEN or WYRD_API_KEY")]
    NoCredentials,
}

/// Map an HTTP `application/problem+json` response body into a [`WyrdError`].
///
/// Reads the stable `code` field and reconstructs the matching
/// [`WyrdError`] variant. Unknown codes are preserved in
/// [`WyrdError::UpstreamFailure`]`::details.original_code` so the original
/// code is never dropped. `pub` so the S3.C4 sink can reuse it.
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
#[cfg(feature = "transport-grpc")]
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
/// which transport the response arrived on. Unknown codes are preserved in
/// `UpstreamFailure.details.original_code` rather than silently dropped.
fn code_to_wyrd_error(code: &str, message: String, details: serde_json::Value) -> WyrdError {
    match code {
        "WYRD_SPEC_400_VALIDATION" => WyrdError::Validation { message, details },
        "WYRD_SPEC_404_NOT_FOUND" => WyrdError::NotFound { message, details },
        "WYRD_SPEC_409_CONFLICT" => WyrdError::Conflict { message, details },
        "WYRD_SPEC_500_INTERNAL" => WyrdError::Internal { message, details },
        "WYRD_SPEC_502_UPSTREAM_FAILURE" => WyrdError::UpstreamFailure { message, details },
        "WYRD_SPEC_504_TIMEOUT" => WyrdError::Timeout { message, details },
        _ => WyrdError::UpstreamFailure {
            message: format!("server error: {message}"),
            details: serde_json::json!({
                "original_code": code,
                "original_details": details,
            }),
        },
    }
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

    #[cfg(feature = "transport-grpc")]
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
