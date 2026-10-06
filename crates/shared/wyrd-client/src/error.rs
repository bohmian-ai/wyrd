//! Concrete client error returned by config `validate()` methods, the
//! credential chain, and the auth token-exchange path, plus `pub`
//! server-response mappers.

use thiserror::Error;
use wyrd_spec::error::{WyrdError, WyrdProblem, WyrdStorageError};
use wyrd_spec::vala::error::BifrostError;
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

impl From<&WyrdClientError> for WyrdError {
    /// Project one client-local failure onto its `WYRD_CLIENT_*` catalog variant.
    ///
    /// This is the single client-error projection shared by every facade, so a
    /// missing credential, invalid configuration, or unavailable transport keeps
    /// the same code and status whether it surfaced through `Cards` or
    /// `Bifrost`. The variant `Display` text becomes the catalog message, and
    /// only the safe structured metadata each variant already carries becomes
    /// `details`: `field` and `reason` for configuration, the `transport`
    /// discriminator for transport unavailability, and nothing for missing
    /// credentials. Raw transport failure text stays in the message only.
    fn from(error: &WyrdClientError) -> Self {
        let message = error.to_string();
        match error {
            WyrdClientError::Config { field, reason } => Self::ClientConfigInvalid {
                message,
                details: serde_json::json!({ "field": field, "reason": reason }),
            },
            WyrdClientError::NoCredentials => Self::ClientNoCredentials {
                message,
                details: serde_json::json!({}),
            },
            WyrdClientError::TransportDown { transport, .. } => Self::ClientTransportDown {
                message,
                details: serde_json::json!({ "transport": transport }),
            },
        }
    }
}

impl From<WyrdClientError> for WyrdError {
    /// Project an owned client-local failure through the borrowed projection.
    fn from(error: WyrdClientError) -> Self {
        Self::from(&error)
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

/// Rebuild the [`WyrdError`] a typed [`WyrdProblem`] carries.
///
/// A failed query terminal carries the same problem the server returns before
/// a stream starts, so both reach callers through this one reconstruction:
/// a `WYRD_VALA_*` problem rebuilds its exact typed error from `details`.
pub fn from_problem(problem: &WyrdProblem) -> WyrdError {
    code_to_wyrd_error(
        &problem.code,
        problem.detail.clone(),
        problem.details.clone(),
    )
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
/// A Bifrost code is rebuilt exactly from the serialized [`BifrostError`] its
/// `details` carry — every server producer (the HTTP problem document, the
/// gRPC problem header, and the query terminal) serializes it — so every
/// catalog Bifrost variant keeps its code and fields without a per-code arm.
///
/// Reconstruction goes through [`WyrdError::from_code`], which covers the whole
/// catalog (every `{ message, details }` variant), so a `404`/`403`/`429`
/// response keeps its real code **and** [`WyrdError::status`] instead of
/// collapsing onto `502`. Codes with no matching variant fall back to
/// [`WyrdError::UpstreamFailure`] with the original code preserved in
/// `details.original_code` rather than silently dropped.
fn code_to_wyrd_error(code: &str, message: String, details: serde_json::Value) -> WyrdError {
    if code.starts_with("WYRD_STORAGE_")
        && let Ok(error) = serde_json::from_value::<WyrdStorageError>(details.clone())
    {
        return error.into();
    }
    if code.starts_with("WYRD_VALA_")
        && let Ok(error) = serde_json::from_value::<BifrostError>(details.clone())
        && error.code() == code
    {
        return WyrdError::Vala { error };
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

#[cfg(test)]
mod tests {
    use super::{BifrostError, WyrdClientError, from_problem_json};
    use wyrd_spec::error::{WyrdError, WyrdStorageError};

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

    /// A Bifrost problem document round-trips to the exact variant and fields.
    ///
    /// The body is the server's own rendering, so a Variant query failure keeps
    /// its catalog code and location instead of collapsing to an upstream
    /// failure on the client.
    ///
    /// # Panics
    ///
    /// Panics when the reconstructed code or problem document drifts from the
    /// sent error.
    #[test]
    fn bifrost_problem_details_reconstruct_exact_variant() {
        let sent = WyrdError::Vala {
            error: BifrostError::VariantInvalidJson {
                field: "parse_json".to_owned(),
                row: 0,
                path: String::new(),
            },
        };
        let received = from_problem_json(&sent.as_problem_json());
        assert_eq!(received.code(), "WYRD_VALA_400_VARIANT_INVALID_JSON");
        assert_eq!(received.as_problem_json(), sent.as_problem_json());
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

    /// Every Bifrost problem the server renders reconstructs its exact variant.
    ///
    /// Each body is the server's own `as_problem_json` rendering, whose
    /// `details` carry the serialized `BifrostError`, so the client keeps the
    /// typed status, code, message, and fields instead of collapsing onto a
    /// retryable-looking 502 `UpstreamFailure`.
    ///
    /// # Panics
    ///
    /// Panics when any reconstructed problem drifts from the one sent.
    #[test]
    fn bifrost_problems_reconstruct_their_exact_variant() {
        let table = || "vala.datasets.events".to_owned();
        for (sent, status) in [
            (BifrostError::TableNotFound { table: table() }, 404),
            (BifrostError::FingerprintMismatch { table: table() }, 409),
            (
                BifrostError::CompactionTargetMismatch { table: table() },
                409,
            ),
            (BifrostError::CompactionTypeMismatch { table: table() }, 409),
            (
                BifrostError::SchemaParse {
                    detail: "too many leaves".to_owned(),
                },
                400,
            ),
            (
                BifrostError::InvalidCompactionTarget {
                    table: table(),
                    bytes: 1,
                },
                400,
            ),
            (
                BifrostError::QueryInvalidSql {
                    detail: "parser rejected SELECT FROM".to_owned(),
                },
                400,
            ),
            (BifrostError::QueryAdmissionRejected, 429),
            (BifrostError::QueryQueueFull, 429),
            (BifrostError::QueryMemoryRequestTooLarge, 422),
            (BifrostError::OracleRoleUnavailable, 503),
            (BifrostError::RunningQueryNotFound, 404),
            (BifrostError::RunningQueryConflict, 409),
            (BifrostError::RunningQueryControlUnavailable, 503),
            (BifrostError::QueryAuditUnavailable, 503),
            (
                BifrostError::AuditUnavailable {
                    detail: "outbox down".to_owned(),
                },
                500,
            ),
            (
                BifrostError::EventTimeOutOfRange {
                    value: "100".to_owned(),
                    past_bound: "200".to_owned(),
                    future_bound: "300".to_owned(),
                },
                400,
            ),
        ] {
            let sent = WyrdError::from(sent);
            let received = from_problem_json(&sent.as_problem_json());
            assert_eq!(received.status(), status, "{}", sent.code());
            assert_eq!(received.as_problem_json(), sent.as_problem_json());
        }
    }

    #[test]
    fn previously_collapsed_code_now_keeps_its_status() {
        // A code that maps to a typed variant must reconstruct that variant and
        // report its real status, not collapse onto UpstreamFailure's 502. Here
        // a 403 code must surface as status 403.
        let body = serde_json::json!({
            "code": "WYRD_PERMISSION_403_DENIED_RBAC",
            "detail": "permission denied",
            "details": {},
        });
        let err = from_problem_json(&body);
        assert_eq!(err.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            err.status(),
            403,
            "status must survive the round-trip, not collapse to 502"
        );
    }

    #[test]
    fn storage_problem_json_reconstructs_delegated_error() {
        let source: WyrdError = WyrdStorageError::InvalidUploadId {
            reason: "upload id must start with wyu_".to_owned(),
        }
        .into();

        let mapped = from_problem_json(&source.as_problem_json());

        assert_eq!(mapped.code(), "WYRD_STORAGE_400_INVALID_UPLOAD_ID");
        assert_eq!(mapped.status(), 400);
        assert_eq!(mapped.as_problem_json(), source.as_problem_json());
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

        use wyrd_spec::error::WyrdError;
        use wyrd_spec::vala::error::BifrostError;
        use wyrd_tonic::error::wyrd_error_to_status;

        use crate::error::{from_grpc_status, from_problem_json};

        /// Capacity and poison problems reconstruct identically across both transports.
        ///
        /// Each problem is rendered by the server's own HTTP and gRPC producers,
        /// so retryability is read from the same serialized `BifrostError`.
        ///
        /// # Panics
        ///
        /// Panics when either transport loses the code or status, or when the
        /// two reconstructions disagree.
        #[test]
        fn capacity_http_and_grpc_reconstruct_retryability_identically() {
            for (sent, status) in [
                (BifrostError::QueryAdmissionRejected, 429),
                (BifrostError::QueryQueueFull, 429),
                (BifrostError::QueryMemoryRequestTooLarge, 422),
                (BifrostError::QueryExecutionFailed, 500),
            ] {
                let sent = WyrdError::from(sent);
                let from_http = from_problem_json(&sent.as_problem_json());
                let from_grpc = from_grpc_status(&wyrd_error_to_status(sent.clone(), None));
                assert_eq!(from_grpc.code(), sent.code());
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
            let sent = WyrdError::from(BifrostError::PayloadTooLarge {
                bytes: 41_943_040,
                limit: 8_388_608,
            });
            let detail = sent.to_string();
            let grpc = wyrd_error_to_status(sent.clone(), None);
            let http = sent.as_problem_json();

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
