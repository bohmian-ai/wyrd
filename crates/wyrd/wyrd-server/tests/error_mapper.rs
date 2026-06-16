use axum::body::to_bytes;
use axum::response::IntoResponse;
use std::borrow::Cow;
use wyrd_auth_verify::AuthError;
use wyrd_runtime::{Permission, PermissionDenyReason, PrincipalId};
use wyrd_server::error::{
    WyrdErrorResponse, auth_error_to_wyrd, permission_deny_reason_to_wyrd, sqlx_error_to_wyrd,
};
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
    let response = WyrdErrorResponse::from(WyrdError::from(WyrdStorageError::BackendUnavailable {
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
async fn retry_after_only_on_verify_unavailable() {
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

    for error in [
        WyrdError::AuthPreviewDisabled {
            message: "preview disabled".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::AuditUnavailable {
            message: "audit unavailable".to_owned(),
            details: serde_json::json!({}),
        },
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
        (
            auth_error_to_wyrd(AuthError::PermissionsCorrupt),
            "WYRD_PERMISSION_500_ROLE_CORRUPT",
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
        required: Permission::card_write(),
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
        AuthPreviewDisabled,
        AuditUnavailable,
        AuthzRequiresDelegatedToken,
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
