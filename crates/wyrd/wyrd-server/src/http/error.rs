//! HTTP response mapping for Wyrd errors.

use axum::body::Body;
use axum::extract::rejection::PathRejection;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use std::fmt::Display;
use wyrd_auth_verify::{AuthError, MAX_DELEGATION_DEPTH};
use wyrd_runtime::PermissionDenyReason;
use wyrd_spec::error::WyrdError;

use crate::auth::roles::RoleAdminError;
use crate::auth::seed::SeedError;

/// Server-owned response wrapper for public Wyrd errors.
///
/// Axum's `IntoResponse` trait and `WyrdError` are both owned outside this
/// crate, so Rust's orphan rules require the local wrapper. All server Wyrd
/// error responses still flow through this one mapper.
#[derive(Debug, Clone)]
pub struct WyrdErrorResponse(pub WyrdError);

impl From<WyrdError> for WyrdErrorResponse {
    fn from(error: WyrdError) -> Self {
        Self(error)
    }
}

impl From<WyrdErrorResponse> for WyrdError {
    fn from(response: WyrdErrorResponse) -> Self {
        response.0
    }
}

impl From<AuthError> for WyrdErrorResponse {
    fn from(error: AuthError) -> Self {
        Self(auth_error_to_wyrd(error))
    }
}

impl From<PermissionDenyReason> for WyrdErrorResponse {
    fn from(reason: PermissionDenyReason) -> Self {
        Self(permission_deny_reason_to_wyrd(reason))
    }
}

impl From<RoleAdminError> for WyrdErrorResponse {
    fn from(error: RoleAdminError) -> Self {
        Self(role_admin_error_to_wyrd(error))
    }
}

impl From<SeedError> for WyrdErrorResponse {
    fn from(error: SeedError) -> Self {
        Self(seed_error_to_wyrd(error))
    }
}

impl IntoResponse for WyrdErrorResponse {
    fn into_response(self) -> Response {
        wyrd_error_response(self.0)
    }
}

/// Record an internal failure and return the stable public error for it.
///
/// A SQL, session, provider, key-store, cryptographic, or serialization
/// message names the libraries and infrastructure behind the boundary and is
/// useless to the caller, so the cause is traced server-side and the served
/// body carries only the operation that failed. Keeping the cause out of
/// `details` is also what stops wire behavior from tracking the error text of
/// an internal dependency.
pub fn internal_failure(message: &'static str, cause: &dyn Display) -> WyrdError {
    tracing::error!(failure = message, cause = %cause, "request failed internally");
    WyrdError::Internal {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

/// Refuse a path identifier the route's typed extractor could not decode.
///
/// Handlers that take `Result<Path<T>, PathRejection>` pass the rejection here
/// so a malformed identifier — which the published typed parameter already
/// excludes — answers with the canonical `WYRD_SPEC_400_VALIDATION` problem
/// instead of Axum's plain-text body. The rejection text names only the
/// segment that failed to parse, never request state.
#[must_use]
pub fn path_rejection(rejection: &PathRejection) -> WyrdErrorResponse {
    WyrdErrorResponse(WyrdError::Validation {
        message: format!("path identifier is invalid: {}", rejection.body_text()),
        details: serde_json::json!({ "location": "path" }),
    })
}

/// Render a Wyrd error as an RFC 9457 problem+json response.
#[must_use]
pub fn wyrd_error_response(error: WyrdError) -> Response {
    wyrd_error_response_from_parts(error, None)
}

/// Render a Wyrd error as problem+json, optionally inserting the request id
/// into the `instance` field and the `wyrd-request-id` response header.
#[must_use]
pub fn wyrd_error_response_from_parts(
    error: WyrdError,
    request_id: Option<&wyrd_spec::request_id::RequestId>,
) -> Response {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let retry_after = matches!(
        &error,
        WyrdError::AuthVerifyUnavailable { .. }
            | WyrdError::Vala {
                error: wyrd_spec::vala::error::BifrostError::QueryAdmissionRejected,
            }
            | WyrdError::Vala {
                error: wyrd_spec::vala::error::BifrostError::IngestBusy { .. },
            }
    );
    let mut body = error.as_problem_json();
    if let (serde_json::Value::Object(map), Some(id)) = (&mut body, request_id) {
        map.insert(
            "instance".to_owned(),
            serde_json::Value::String(format!("urn:wyrd:request:{}", id.as_str())),
        );
    }
    match serde_json::to_vec(&body) {
        Ok(bytes) => {
            let mut response = response_with_body(status, bytes);
            if retry_after {
                response.headers_mut().insert(
                    axum::http::header::RETRY_AFTER,
                    axum::http::HeaderValue::from_static("1"),
                );
            }
            if let Some(id) = request_id
                && let Ok(val) = axum::http::HeaderValue::from_str(id.as_str())
            {
                response.headers_mut().insert(
                    axum::http::header::HeaderName::from_static(
                        crate::http::middleware::request_id::REQUEST_ID_HEADER,
                    ),
                    val,
                );
            }
            response
        }
        Err(_) => response_with_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            br#"{"type":"https://wyrd.dev/problems/WYRD_SPEC_500_INTERNAL","title":"Internal error","status":500,"detail":"failed to serialize Wyrd error response","code":"WYRD_SPEC_500_INTERNAL","details":{},"remediation":"Retry later or inspect server logs using the request ID."}"#.to_vec(),
        ),
    }
}

/// Adapter for `HandleErrorLayer`: converts tower `BoxError` into a Wyrd
/// problem+json response.
///
/// Maps:
/// - `tower::timeout::error::Elapsed` → 504 `WYRD_SERVER_504_REQUEST_TIMEOUT`
/// - `tower::load_shed::error::Overloaded` → 503 `WYRD_SERVER_503_SERVICE_UNAVAILABLE`
/// - Unknown → 500 `WYRD_SPEC_500_INTERNAL`
pub async fn map_tower_error_to_wyrd(error: tower::BoxError) -> Response {
    if error.is::<tower::timeout::error::Elapsed>() {
        return wyrd_error_response(WyrdError::RequestTimeout {
            message: "request exceeded the configured handler timeout".to_owned(),
            details: serde_json::json!({}),
        });
    }
    if error.is::<tower::load_shed::error::Overloaded>() {
        return wyrd_error_response(WyrdError::ServiceUnavailable {
            message: "server is at capacity; retry with backoff".to_owned(),
            details: serde_json::json!({}),
        });
    }
    tracing::error!(
        tower_error_class = "unknown",
        "unexpected tower BoxError in HandleErrorLayer"
    );
    wyrd_error_response(WyrdError::Internal {
        message: "internal server error".to_owned(),
        details: serde_json::json!({}),
    })
}

/// Panic handler for `CatchPanicLayer::custom`.
///
/// Returns a fixed 500 problem+json body. The panic payload is never logged or
/// echoed to the caller.
pub fn wyrd_panic_response(_panic_info: Box<dyn std::any::Any + Send>) -> Response {
    tracing::error!(panic.class = "handler", "handler panicked; returning 500");
    wyrd_error_response(WyrdError::Internal {
        message: "internal server error".to_owned(),
        details: serde_json::json!({}),
    })
}

fn response_with_body(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/problem+json"),
    );
    response
}

/// Convert auth verifier failures into stable public Wyrd errors.
#[must_use]
pub fn auth_error_to_wyrd(error: AuthError) -> WyrdError {
    match error {
        AuthError::Jwt(error) => jwt_error_to_wyrd(error),
        AuthError::InvalidToken => invalid_token("token rejected", serde_json::json!({})),
        AuthError::TokenExpired => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        },
        AuthError::InvalidCardRef => WyrdError::InvalidCardRef {
            message: "non-user token card_ref claim is absent or malformed".to_owned(),
            details: serde_json::json!({}),
        },
        AuthError::CardScopeMissingRoot => WyrdError::InvalidCardRef {
            message: "card-bound token scope is missing its root card_ref".to_owned(),
            details: serde_json::json!({ "field": "card_ref_scope" }),
        },
        AuthError::DelegationDepthExceeded => WyrdError::DelegationDepthExceededVerify {
            message: format!(
                "delegation chain exceeds MAX_DELEGATION_DEPTH={MAX_DELEGATION_DEPTH}"
            ),
            details: serde_json::json!({ "max": MAX_DELEGATION_DEPTH }),
        },
        AuthError::BadTokenFormat => {
            bad_token_format("X-Wyrd-Access-Token is not a compact Wyrd JWT")
        }
        AuthError::VerifyUnavailable => WyrdError::AuthVerifyUnavailable {
            message: "auth verify backend unavailable".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        },
    }
}

fn jwt_error_to_wyrd(error: jsonwebtoken::errors::Error) -> WyrdError {
    use jsonwebtoken::errors::ErrorKind;

    // The specific JWT failure kind is a low-noise oracle for token structure
    // probing. Log it internally; do not surface it on the public wire.
    tracing::debug!(jwt_error = ?error.kind(), "JWT validation failed");

    match error.kind() {
        ErrorKind::ExpiredSignature => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        },
        ErrorKind::Base64(_) | ErrorKind::Json(_) | ErrorKind::Utf8(_) => {
            bad_token_format("token payload is malformed")
        }
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidIssuer
        | ErrorKind::InvalidSubject
        | ErrorKind::InvalidAudience
        | ErrorKind::InvalidAlgorithm
        | ErrorKind::InvalidAlgorithmName => invalid_token(
            "bearer token signature, issuer, subject, audience, or algorithm invalid",
            serde_json::json!({}),
        ),
        _ => invalid_token("bearer token rejected", serde_json::json!({})),
    }
}

/// Convert an RBAC denial into a stable public Wyrd error.
#[must_use]
pub fn permission_deny_reason_to_wyrd(reason: PermissionDenyReason) -> WyrdError {
    match reason {
        PermissionDenyReason::Rbac {
            required,
            principal,
        } => {
            let resource = format!("{:?}", required.resource);
            let action = format!("{:?}", required.action);
            WyrdError::PermissionDeniedRbac {
                message: format!("principal {principal} lacks {resource}/{action}"),
                details: serde_json::json!({
                    "required": required,
                    "principal": principal.to_string(),
                }),
            }
        }
    }
}

/// Convert role-admin errors at the handler boundary.
#[must_use]
pub fn role_admin_error_to_wyrd(error: RoleAdminError) -> WyrdError {
    match error {
        RoleAdminError::CannotDeleteBuiltin => WyrdError::PermissionDeniedRbac {
            message: "builtin roles cannot be deleted".to_owned(),
            details: serde_json::json!({ "resource": "roles", "action": "delete" }),
        },
        RoleAdminError::NotFound => WyrdError::NotFound {
            message: "role not found".to_owned(),
            details: serde_json::json!({ "resource": "role" }),
        },
        RoleAdminError::Database(error) => sqlx_error_to_wyrd(error),
    }
}

/// Convert seed errors at the handler boundary.
#[must_use]
pub fn seed_error_to_wyrd(error: SeedError) -> WyrdError {
    match error {
        SeedError::Database(error) => sqlx_error_to_wyrd(error),
        SeedError::Serialize(error) => WyrdError::Internal {
            message: "builtin role permissions failed to serialize".to_owned(),
            details: serde_json::json!({ "source": error.to_string() }),
        },
    }
}

/// Convert SQLx database errors that have public auth/RBAC contract meaning.
#[must_use]
pub fn sqlx_error_to_wyrd(error: sqlx::Error) -> WyrdError {
    if let sqlx::Error::Database(db_error) = &error
        && db_error.code().as_deref() == Some("23514")
        && db_error.constraint() == Some("auth_builtin_role_immutable_name")
    {
        return WyrdError::BuiltinRoleImmutableName {
            message: "builtin role names are immutable".to_owned(),
            details: serde_json::json!({
                "sqlstate": "23514",
                "constraint": "auth_builtin_role_immutable_name",
            }),
        };
    }

    tracing::warn!(error = %error, "database operation failed");
    WyrdError::Internal {
        message: "database operation failed".to_owned(),
        details: serde_json::json!({}),
    }
}

fn bad_token_format(message: &str) -> WyrdError {
    WyrdError::BadTokenFormat {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

fn invalid_token(message: &str, details: serde_json::Value) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details,
    }
}

#[cfg(test)]
mod error_mapper_tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use std::borrow::Cow;
    use wyrd_auth_verify::AuthError;
    use wyrd_runtime::{Permission, PermissionDenyReason, PrincipalId};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::error::storage::WyrdStorageError;

    #[tokio::test]
    async fn all_wyrd_error_variants_render_problem_json() {
        for error in sample_errors() {
            let expected = error.as_problem_json();
            let response = WyrdErrorResponse::from(error).into_response();

            assert_eq!(response.status().as_u16(), expected["status"]);
            assert_eq!(
                response
                    .headers()
                    .get(axum::http::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok()),
                Some("application/problem+json")
            );

            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body collects");
            let actual: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
            assert_eq!(actual, expected);
        }
    }

    #[tokio::test]
    async fn storage_error_renders_storage_problem_json_code() {
        let response =
            WyrdErrorResponse::from(WyrdError::from(WyrdStorageError::BackendUnavailable {
                status: 503,
            }))
            .into_response();

        assert_eq!(response.status().as_u16(), 503);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
        assert_eq!(problem["code"], "WYRD_STORAGE_503_BACKEND_UNAVAILABLE");
        assert_eq!(problem["status"], 503);
        assert_eq!(problem["title"], "Backend transiently unavailable");
        assert_eq!(problem["details"]["variant"], "backend_unavailable");
        assert_eq!(problem["details"]["data"]["status"], 503);
    }

    #[tokio::test]
    async fn ingest_busy_http_retry_metadata() {
        let retryable = WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
            message: "resolver unavailable".to_owned(),
            details: serde_json::json!({}),
        })
        .into_response();
        assert_eq!(
            retryable
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
        let ingest_busy = WyrdErrorResponse::from(WyrdError::from(
            wyrd_spec::vala::error::BifrostError::IngestBusy {
                table: "vala.traces.spans".to_owned(),
            },
        ))
        .into_response();
        assert_eq!(
            ingest_busy
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
        let capacity = WyrdErrorResponse::from(WyrdError::from(
            wyrd_spec::vala::error::BifrostError::QueryAdmissionRejected,
        ))
        .into_response();
        assert_eq!(
            capacity
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );

        for error in [
            WyrdError::AuditUnavailable {
                message: "audit unavailable".to_owned(),
                details: serde_json::json!({}),
            },
            wyrd_spec::vala::error::BifrostError::QueryMemoryRequestTooLarge.into(),
            wyrd_spec::vala::error::BifrostError::QueryExecutionFailed.into(),
            wyrd_spec::vala::error::BifrostError::PayloadTooLarge { bytes: 1, limit: 1 }.into(),
            wyrd_spec::vala::error::BifrostError::WalDiskFull.into(),
        ] {
            let response = WyrdErrorResponse::from(error).into_response();
            assert!(
                response
                    .headers()
                    .get(axum::http::header::RETRY_AFTER)
                    .is_none()
            );
        }
    }

    /// Proves the enforced payload ceiling survives the HTTP problem+json
    /// rendering. An agent reading only the response body must be able to see
    /// both what it sent and what the server would have accepted; a remediation
    /// asserting a fixed ceiling would contradict the configured limit.
    ///
    /// # Panics
    ///
    /// Panics when the code, status, measured bytes, enforced limit, detail, or
    /// remediation is lost or contradicted at the HTTP boundary.
    #[tokio::test]
    async fn payload_limit_survives_the_http_boundary() {
        let response = WyrdErrorResponse::from(WyrdError::from(
            wyrd_spec::vala::error::BifrostError::PayloadTooLarge {
                bytes: 41_943_040,
                limit: 8_388_608,
            },
        ))
        .into_response();
        assert_eq!(response.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("problem body reads");
        let problem: serde_json::Value =
            serde_json::from_slice(&body).expect("problem body is JSON");

        assert_eq!(problem["code"], "WYRD_VALA_413_PAYLOAD_TOO_LARGE");
        assert_eq!(problem["status"], 413);
        assert_eq!(
            problem["details"]["data"]["bytes"], 41_943_040,
            "measured bytes must survive HTTP: {problem}"
        );
        assert_eq!(
            problem["details"]["data"]["limit"], 8_388_608,
            "the enforced limit must survive HTTP: {problem}"
        );
        let detail = problem["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("41943040") && detail.contains("8388608"),
            "detail must name both bounds: {problem}"
        );
        let remediation = problem["remediation"].as_str().unwrap_or_default();
        assert!(
            !remediation.contains("32 MiB") && remediation.contains("limit"),
            "remediation must point at the supplied limit: {problem}"
        );
    }

    #[test]
    fn auth_error_mapping_uses_locked_codes() {
        let cases = [
            (
                auth_error_to_wyrd(AuthError::BadTokenFormat),
                "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
            ),
            (
                auth_error_to_wyrd(AuthError::InvalidToken),
                "WYRD_AUTH_401_INVALID_TOKEN",
            ),
            (
                auth_error_to_wyrd(AuthError::InvalidCardRef),
                "WYRD_AUTH_401_INVALID_CARD_REF",
            ),
            (
                auth_error_to_wyrd(AuthError::DelegationDepthExceeded),
                "WYRD_AUTH_401_DELEGATION_DEPTH_EXCEEDED",
            ),
            (
                auth_error_to_wyrd(AuthError::VerifyUnavailable),
                "WYRD_AUTH_503_VERIFY_UNAVAILABLE",
            ),
        ];

        for (error, code) in cases {
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn jwt_error_kind_mapping_classifies_bad_format() {
        let error = jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::Json(
            std::sync::Arc::new(
                serde_json::from_str::<serde_json::Value>("{").expect_err("invalid json"),
            ),
        ));

        let wyrd = auth_error_to_wyrd(AuthError::Jwt(error));

        assert_eq!(wyrd.code(), "WYRD_AUTH_400_BAD_TOKEN_FORMAT");
    }

    #[test]
    fn permission_deny_reason_maps_to_rbac_problem_details() {
        let principal = PrincipalId::new(uuid::Uuid::now_v7());
        let error = permission_deny_reason_to_wyrd(PermissionDenyReason::Rbac {
            required: Box::new(Permission::card_write()),
            principal,
        });
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(problem["details"]["required"]["resource"], "cards");
        assert_eq!(problem["details"]["required"]["action"], "write");
        assert_eq!(problem["details"]["principal"], principal.to_string());
    }

    #[test]
    fn sqlx_non_database_errors_map_to_internal() {
        let error = sqlx_error_to_wyrd(sqlx::Error::RowNotFound);

        assert_eq!(error.code(), "WYRD_SPEC_500_INTERNAL");
    }

    #[test]
    fn sqlx_builtin_immutable_name_constraint_maps_to_409() {
        let error = sqlx_error_to_wyrd(sqlx::Error::Database(Box::new(FakeDatabaseError {
            code: Some("23514"),
            constraint: Some("auth_builtin_role_immutable_name"),
        })));

        assert_eq!(error.code(), "WYRD_RBAC_409_BUILTIN_ROLE_IMMUTABLE_NAME");
        assert_eq!(error.status(), 409);
        assert_eq!(
            error.as_problem_json()["details"]["constraint"],
            "auth_builtin_role_immutable_name"
        );
    }

    fn sample_errors() -> Vec<WyrdError> {
        macro_rules! sample {
            ($($variant:ident),+ $(,)?) => {
                vec![
                    $(
                        WyrdError::$variant {
                            message: stringify!($variant).to_owned(),
                            details: serde_json::json!({ "variant": stringify!($variant) }),
                        },
                    )+
                ]
            };
        }

        let mut errors = sample!(
            Validation,
            NotFound,
            Conflict,
            Internal,
            UpstreamFailure,
            Timeout,
            Unauthenticated,
            TokenExpired,
            InvalidToken,
            BadTokenFormat,
            UnsupportedGrantType,
            DelegationDepthExceededIssue,
            PrincipalKindCardKindMismatch,
            InvalidCardRefVersion,
            CredentialRevoked,
            ApiKeyInvalid,
            InvalidCardRef,
            DelegationDepthExceededVerify,
            PrincipalNotFound,
            AuthVerifyUnavailable,
            AuditUnavailable,
            AuthzRequiresDelegatedToken,
            MissingRequiredField,
            PolicyDenied,
            PermissionUnauthenticated,
            PermissionDeniedRbac,
            RoleCorrupt,
            BuiltinRoleImmutableName,
            DataValidation,
            DataUnknownDataType,
            DataInvalidSplitRule,
            DataTargetColumnUnknown,
            DataInvalidInterfaceOption,
            DataInterfaceMetadataRequired,
            ModelUnknownModelType,
            SourceValidation,
            ModelValidation,
            ModelMissingSignature,
            ModelDtypeNormalizeFailed,
            ModelShapeInvalid,
            ModelHfRevisionInvalid,
            ModelHfTaskMissing,
            ModelCustomLoaderInvalid,
            ModelSerializerUnavailable,
            PromptInvalidVariableName,
            PromptDuplicateVariable,
            PromptUndeclaredPlaceholder,
            PromptUnreferencedVariable,
            PromptUndeclaredMediaPlaceholder,
            PromptUnreferencedMediaVariable,
            PromptMediaPlaceholderNotIsolated,
            PromptMediaInSystemMessage,
            PromptUnsupportedMediaForProvider,
            PromptInvalidMediaType,
            PromptMissingMediaVariable,
            PromptMediaNotRegularFile,
            PromptMediaTooLarge,
            PromptMediaInvalidExtension,
            PromptMediaIo,
            PromptEmptyModel,
            PromptInvalidResponseSchema,
            PromptInvalidOutputSchema,
            PromptPydanticRequired,
            PromptLoaderBadExtension,
            PromptLoaderIo,
            PromptSerializeRequest,
            PromptProviderAuth,
            PromptProviderRateLimit,
            PromptProviderUpstream,
            PromptProviderTimeout,
            PromptProviderMismatch,
            PromptDraftInvalid,
            PromptSettingsProviderMismatch,
            PromptSettingsDecode,
            PromptMissingVariable,
            PromptUnsupportedHandoff,
            PromptResponseDecode,
            AgentValidation,
            AgentMissingName,
            AgentMissingVersion,
            AgentPromptCardNotFound,
            AgentRuntimeLocalToolNotFound,
            AgentRuntimeLocalToolsNotRegistrable,
            AgentCallbackReturnType,
            AgentCallbackAborted,
            AgentLoopMessageType,
            WorkflowValidation,
            WorkflowMissingName,
            WorkflowMissingVersion,
            WorkflowDuplicateStepId,
            WorkflowMissingDependency,
            WorkflowCycle,
        );
        errors.push(
            WyrdStorageError::ObjectNotFound {
                storage_path: "tenant/cards/card/model.bin".to_owned(),
            }
            .into(),
        );
        errors
    }

    #[derive(Debug)]
    struct FakeDatabaseError {
        code: Option<&'static str>,
        constraint: Option<&'static str>,
    }

    impl std::fmt::Display for FakeDatabaseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake database error")
        }
    }

    impl std::error::Error for FakeDatabaseError {}

    impl sqlx::error::DatabaseError for FakeDatabaseError {
        fn message(&self) -> &str {
            "fake database error"
        }

        fn code(&self) -> Option<Cow<'_, str>> {
            self.code.map(Cow::Borrowed)
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn constraint(&self) -> Option<&str> {
            self.constraint
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::CheckViolation
        }
    }
}
