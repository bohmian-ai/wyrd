use axum::body::to_bytes;
use axum::response::IntoResponse;
use wyrd_server::error::WyrdErrorResponse;
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
        PermissionDenied,
        Internal,
        UpstreamFailure,
        Timeout,
        Unauthenticated,
        TokenExpired,
        InvalidToken,
        InsufficientScope,
        CredentialRevoked,
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
