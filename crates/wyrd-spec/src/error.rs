//! Stable Wyrd error hierarchy and error-code helpers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::derive::WyrdError as WyrdErrorMeta;

/// Proc-macro re-export for Wyrd-coded error enums.
pub mod derive {
    pub use wyrd_error_derive::WyrdError;
}

/// Top-level Wyrd error value passed across crate and wire boundaries.
#[derive(Debug, Error, Serialize, Deserialize, schemars::JsonSchema, WyrdErrorMeta)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WyrdError {
    /// Request payload failed validation.
    #[error("[WYRD_SPEC_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_400_VALIDATION",
        status = 400,
        title = "Validation failed",
        remediation = "Check the submitted Wyrd request fields against the published schema and retry."
    )]
    Validation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested entity was not found.
    #[error("[WYRD_SPEC_404_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_404_NOT_FOUND",
        status = 404,
        title = "Resource not found",
        remediation = "Check that the referenced Wyrd resource exists."
    )]
    NotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Write was rejected due to a duplicate key or concurrent change.
    #[error("[WYRD_SPEC_409_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_409_CONFLICT",
        status = 409,
        title = "Conflict",
        remediation = "Refresh the resource and retry with the current version or idempotency key."
    )]
    Conflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but lacks permission.
    #[error("[WYRD_SPEC_403_PERMISSION_DENIED] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_403_PERMISSION_DENIED",
        status = 403,
        title = "Permission denied",
        remediation = "Check the caller scopes, actor identity, and policy decision."
    )]
    PermissionDenied {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Unexpected internal failure.
    #[error("[WYRD_SPEC_500_INTERNAL] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_500_INTERNAL",
        status = 500,
        title = "Internal error",
        remediation = "Retry later or inspect server logs using the request ID."
    )]
    Internal {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Upstream dependency failed.
    #[error("[WYRD_SPEC_502_UPSTREAM_FAILURE] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_502_UPSTREAM_FAILURE",
        status = 502,
        title = "Upstream dependency failed",
        remediation = "Check the upstream dependency health and retry policy."
    )]
    UpstreamFailure {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Operation exceeded its deadline.
    #[error("[WYRD_SPEC_504_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_504_TIMEOUT",
        status = 504,
        title = "Timeout",
        remediation = "Retry with a longer timeout or reduce the request scope."
    )]
    Timeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Vala eval task graph is not a DAG.
    #[error("[WYRD_VALA_400_TASK_DAG_CYCLE] {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_TASK_DAG_CYCLE",
        status = 400,
        title = "Eval task DAG validation failed",
        remediation = "Remove cycles, self-dependencies, or references to missing eval tasks."
    )]
    ValaTaskDagCycle {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Vala eval contract referenced a CardRef kind outside its allowlist.
    #[error("[WYRD_VALA_400_EVAL_REF_KIND_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH",
        status = 400,
        title = "Eval reference kind mismatch",
        remediation = "Use the required CardRef kind for the eval field being validated."
    )]
    ValaEvalRefKindMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller presented no credential or an unparseable one.
    #[error("[WYRD_AUTH_401_UNAUTHENTICATED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_UNAUTHENTICATED",
        status = 401,
        title = "Not authenticated",
        remediation = "Present a valid Wyrd token in the Authorization header."
    )]
    Unauthenticated {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Token signature and shape were valid but the token is past its expiry.
    #[error("[WYRD_AUTH_401_TOKEN_EXPIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_TOKEN_EXPIRED",
        status = 401,
        title = "Token expired",
        remediation = "Re-authenticate to obtain a fresh token."
    )]
    TokenExpired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Token was malformed, had a bad signature, or failed a claim check.
    #[error("[WYRD_AUTH_401_INVALID_TOKEN] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_INVALID_TOKEN",
        status = 401,
        title = "Invalid token",
        remediation = "Re-authenticate to obtain a valid token signed by the current issuer key."
    )]
    InvalidToken {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but the resolved scopes do not cover the action.
    #[error("[WYRD_AUTH_403_INSUFFICIENT_SCOPE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_403_INSUFFICIENT_SCOPE",
        status = 403,
        title = "Insufficient scope",
        remediation = "Request a role that grants the required scope for this action."
    )]
    InsufficientScope {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Credential (API key, governance token, or refresh token) has been revoked.
    #[error("[WYRD_AUTH_401_CREDENTIAL_REVOKED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_CREDENTIAL_REVOKED",
        status = 401,
        title = "Credential revoked",
        remediation = "The credential was explicitly revoked. Re-authenticate or request a new credential."
    )]
    CredentialRevoked {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// DataCard validation failed.
    #[error("[WYRD_DATA_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_VALIDATION",
        status = 400,
        title = "DataCard validation failed",
        remediation = "Fix the DataCard schema, artifact, stats, or interface metadata."
    )]
    DataValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python data input could not be classified as a supported DataCard source.
    #[error("[WYRD_DATA_400_UNKNOWN_DATA_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_UNKNOWN_DATA_TYPE",
        status = 400,
        title = "Unknown DataCard data type",
        remediation = "Pass a supported data object, path, or explicit DataInterface."
    )]
    DataUnknownDataType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard split declaration is invalid.
    #[error("[WYRD_DATA_400_INVALID_SPLIT_RULE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_SPLIT_RULE",
        status = 400,
        title = "Invalid DataCard split rule",
        remediation = "Use a supported split operator and reference schema columns that exist."
    )]
    DataInvalidSplitRule {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard target column is absent from the declared schema.
    #[error("[WYRD_DATA_400_TARGET_COLUMN_UNKNOWN] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN",
        status = 400,
        title = "Unknown DataCard target column",
        remediation = "Declare target columns that exist in the DataCard schema."
    )]
    DataTargetColumnUnknown {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Python DataCard interface option was invalid.
    #[error("[WYRD_DATA_400_INVALID_INTERFACE_OPTION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_INTERFACE_OPTION",
        status = 400,
        title = "Invalid DataCard interface option",
        remediation = "Use one of the supported interface option values."
    )]
    DataInvalidInterfaceOption {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Interface metadata was required but could not be inferred.
    #[error("[WYRD_DATA_400_INTERFACE_METADATA_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INTERFACE_METADATA_REQUIRED",
        status = 400,
        title = "DataCard interface metadata required",
        remediation = "Pass an explicit DataInterface with enough metadata to describe the data source."
    )]
    DataInterfaceMetadataRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python model input could not be classified as a supported ModelCard interface.
    #[error("[WYRD_MODEL_400_UNKNOWN_MODEL_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE",
        status = 400,
        title = "Unknown ModelCard model type",
        remediation = "Pass a supported model object or an explicit ModelInterface."
    )]
    ModelUnknownModelType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// ModelCard validation failed.
    #[error("[WYRD_MODEL_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_VALIDATION",
        status = 400,
        title = "ModelCard validation failed",
        remediation = "Fix the ModelCard signature, framework version, or interface configuration."
    )]
    ModelValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature was missing required fields.
    #[error("[WYRD_MODEL_400_MISSING_SIGNATURE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_MISSING_SIGNATURE",
        status = 400,
        title = "Missing model signature",
        remediation = "Pass signature fields explicitly or build a ModelSignature before registration."
    )]
    ModelMissingSignature {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature dtype was not canonical.
    #[error("[WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED",
        status = 400,
        title = "Could not normalize signature dtype",
        remediation = "Provide a FieldSpec with a canonical Arrow logical dtype string."
    )]
    ModelDtypeNormalizeFailed {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature shape was invalid.
    #[error("[WYRD_MODEL_400_SHAPE_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_SHAPE_INVALID",
        status = 400,
        title = "Invalid model signature shape",
        remediation = "Use Dim::Fixed(n) with n > 0 or Dim::Dynamic(name) for unknown dimensions."
    )]
    ModelShapeInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Hugging Face revision was invalid.
    #[error("[WYRD_MODEL_400_HF_REVISION_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_HF_REVISION_INVALID",
        status = 400,
        title = "Invalid HuggingFace revision",
        remediation = "Pass a 7-40 character lowercase hex revision from the HuggingFace repo."
    )]
    ModelHfRevisionInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Hugging Face task was missing.
    #[error("[WYRD_MODEL_400_HF_TASK_MISSING] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_HF_TASK_MISSING",
        status = 400,
        title = "Missing HuggingFace task",
        remediation = "Set HuggingfaceMeta.hf_task before saving."
    )]
    ModelHfTaskMissing {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A custom loader declaration was invalid.
    #[error("[WYRD_MODEL_400_CUSTOM_LOADER_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_CUSTOM_LOADER_INVALID",
        status = 400,
        title = "Invalid custom loader",
        remediation = "Set CustomMeta.loader_module and CustomMeta.loader_class to non-empty values."
    )]
    ModelCustomLoaderInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard sample or artifact serializer is unavailable.
    #[error("[WYRD_MODEL_501_SERIALIZER_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_501_SERIALIZER_UNAVAILABLE",
        status = 501,
        title = "Required serializer unavailable",
        remediation = "Install the Wyrd Python extra for the required framework serializer."
    )]
    ModelSerializerUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard variable name was invalid.
    #[error("[WYRD_PROMPT_400_INVALID_VARIABLE_NAME] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_VARIABLE_NAME",
        status = 400,
        title = "Invalid variable name",
        remediation = "Variable names must match ^[a-zA-Z_][a-zA-Z0-9_]*$."
    )]
    PromptInvalidVariableName {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared the same variable more than once.
    #[error("[WYRD_PROMPT_409_DUPLICATE_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_409_DUPLICATE_VARIABLE",
        status = 409,
        title = "Duplicate variable name",
        remediation = "Each entry in prompt.variables must be unique."
    )]
    PromptDuplicateVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request referenced an undeclared placeholder.
    #[error("[WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER",
        status = 422,
        title = "Undeclared placeholder in request",
        remediation = "Add the placeholder name to prompt.variables, or remove it from the request."
    )]
    PromptUndeclaredPlaceholder {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared a variable that the request never references.
    #[error("[WYRD_PROMPT_422_UNREFERENCED_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNREFERENCED_VARIABLE",
        status = 422,
        title = "Declared variable is never referenced",
        remediation = "Reference the variable somewhere in the native request, or drop it from prompt.variables."
    )]
    PromptUnreferencedVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request referenced an undeclared media placeholder.
    #[error("[WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER",
        status = 422,
        title = "Undeclared media placeholder in request",
        remediation = "Add the media placeholder name to prompt.media_variables, or remove it from the request."
    )]
    PromptUndeclaredMediaPlaceholder {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared a media variable that the request never references.
    #[error("[WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE",
        status = 422,
        title = "Declared media variable is never referenced",
        remediation = "Reference the media variable as ${media:name}, or drop it from prompt.media_variables."
    )]
    PromptUnreferencedMediaVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media placeholder was not isolated in its own text part.
    #[error("[WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED",
        status = 422,
        title = "Media placeholder is not isolated",
        remediation = "Ensure each ${media:name} token occupies its own native text part before binding."
    )]
    PromptMediaPlaceholderNotIsolated {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media placeholder appeared in system content.
    #[error("[WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE",
        status = 400,
        title = "Media placeholder in system message",
        remediation = "Move media placeholders to user or assistant content."
    )]
    PromptMediaInSystemMessage {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider does not support this media kind/source.
    #[error("[WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER",
        status = 400,
        title = "Unsupported media for provider",
        remediation = "Use a media source supported by the selected provider."
    )]
    PromptUnsupportedMediaForProvider {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media MIME type or URI was invalid.
    #[error("[WYRD_PROMPT_400_INVALID_MEDIA_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_MEDIA_TYPE",
        status = 400,
        title = "Invalid media type",
        remediation = "Supply the required MIME type and a provider-supported URI."
    )]
    PromptInvalidMediaType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt render call omitted a declared media variable.
    #[error("[WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE",
        status = 422,
        title = "Media variable missing",
        remediation = "Bind every declared media variable before rendering the prompt."
    )]
    PromptMissingMediaVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media path was not a regular file.
    #[error("[WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE",
        status = 400,
        title = "Media path is not a regular file",
        remediation = "Pass a regular local file path."
    )]
    PromptMediaNotRegularFile {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media file exceeded the allowed byte limit.
    #[error("[WYRD_PROMPT_400_MEDIA_TOO_LARGE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_TOO_LARGE",
        status = 400,
        title = "Media file too large",
        remediation = "Use a smaller media file or upload it to the provider and bind a file URI."
    )]
    PromptMediaTooLarge {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media file extension could not be mapped to a supported MIME type.
    #[error("[WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION",
        status = 400,
        title = "Media file extension is unsupported",
        remediation = "Use a supported image or document extension."
    )]
    PromptMediaInvalidExtension {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media path helper IO failed.
    #[error("[WYRD_PROMPT_500_MEDIA_IO] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_MEDIA_IO",
        status = 500,
        title = "Media IO failure",
        remediation = "Inspect the OS error; usually missing path or permission denied."
    )]
    PromptMediaIo {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard model string was empty.
    #[error("[WYRD_PROMPT_400_EMPTY_MODEL] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_EMPTY_MODEL",
        status = 400,
        title = "Prompt model is empty",
        remediation = "Set prompt.model to a non-empty provider model id."
    )]
    PromptEmptyModel {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard JSON Schema response type was not an object schema.
    #[error("[WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA",
        status = 400,
        title = "JsonSchema response_type schema is not an object",
        remediation = "The schema of ResponseType::JsonSchema must be a JSON object."
    )]
    PromptInvalidResponseSchema {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard codec extension was unsupported.
    #[error("[WYRD_PROMPT_400_LOADER_BAD_EXTENSION] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_LOADER_BAD_EXTENSION",
        status = 400,
        title = "Loader extension not supported",
        remediation = "Use .json, .yaml, or .yml."
    )]
    PromptLoaderBadExtension {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard loader IO failed in an IO-owning crate.
    #[error("[WYRD_PROMPT_500_LOADER_IO] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_LOADER_IO",
        status = 500,
        title = "Loader IO failure",
        remediation = "Inspect the OS error; usually missing path or permission denied."
    )]
    PromptLoaderIo {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request serialization failed during validation.
    #[error("[WYRD_PROMPT_500_SERIALIZE_REQUEST] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_SERIALIZE_REQUEST",
        status = 500,
        title = "Serialize native request failed during validation",
        remediation = "The native request could not be serialized to JSON. Capture the spec and file an issue."
    )]
    PromptSerializeRequest {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider authentication failed at a prompt boundary.
    #[error("[WYRD_PROMPT_401_PROVIDER_AUTH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_401_PROVIDER_AUTH",
        status = 401,
        title = "Provider authentication failed",
        remediation = "Check the provider credentials environment variables."
    )]
    PromptProviderAuth {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider rate-limited a prompt request.
    #[error("[WYRD_PROMPT_429_PROVIDER_RATE_LIMIT] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_429_PROVIDER_RATE_LIMIT",
        status = 429,
        title = "Provider rate limit",
        remediation = "Back off, lower concurrency, or upgrade provider tier."
    )]
    PromptProviderRateLimit {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider returned an upstream failure for a prompt request.
    #[error("[WYRD_PROMPT_502_PROVIDER_UPSTREAM] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_502_PROVIDER_UPSTREAM",
        status = 502,
        title = "Provider upstream error",
        remediation = "Retry with backoff; provider returned a 5xx."
    )]
    PromptProviderUpstream {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider prompt request timed out.
    #[error("[WYRD_PROMPT_504_PROVIDER_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_504_PROVIDER_TIMEOUT",
        status = 504,
        title = "Provider request timed out",
        remediation = "Retry; raise the per-request timeout if this is chronic."
    )]
    PromptProviderTimeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt request variant did not match the dialed provider.
    #[error("[WYRD_PROMPT_400_PROVIDER_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_PROVIDER_MISMATCH",
        status = 400,
        title = "Request shape did not match the dialed provider",
        remediation = "Pick the matching provider client or dispatch through the runtime registry."
    )]
    PromptProviderMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Declarative prompt draft failed to compile into a native prompt.
    #[error("[WYRD_PROMPT_400_DRAFT_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_DRAFT_INVALID",
        status = 400,
        title = "Declarative prompt draft compile failed",
        remediation = "Check the provider, model, messages, and model_settings fields in your prompt YAML."
    )]
    PromptDraftInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt model_settings object did not match the selected provider.
    #[error("[WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH",
        status = 400,
        title = "model_settings provider mismatch",
        remediation = "Pass the settings type for this provider, or pass a dict."
    )]
    PromptSettingsProviderMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt model_settings value failed to decode.
    #[error("[WYRD_PROMPT_400_SETTINGS_DECODE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_SETTINGS_DECODE",
        status = 400,
        title = "model_settings decode failed",
        remediation = "Check model_settings field names and types; unknown fields are allowed and pass through."
    )]
    PromptSettingsDecode {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt render call omitted a declared variable.
    #[error("[WYRD_PROMPT_422_MISSING_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MISSING_VARIABLE",
        status = 422,
        title = "Render variable missing",
        remediation = "Supply every declared variable when calling Prompt::render."
    )]
    PromptMissingVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt message handoff conversion is unsupported.
    #[error("[WYRD_PROMPT_501_UNSUPPORTED_HANDOFF] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_501_UNSUPPORTED_HANDOFF",
        status = 501,
        title = "Unsupported message conversion",
        remediation = "Use a same-provider handoff or convert manually."
    )]
    PromptUnsupportedHandoff {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt runtime response did not satisfy the response schema.
    #[error("[WYRD_PROMPT_422_RESPONSE_DECODE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_RESPONSE_DECODE",
        status = 422,
        title = "Response did not match requested schema",
        remediation = "Inspect the provider response or relax the prompt response schema."
    )]
    PromptResponseDecode {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
}

impl WyrdError {
    /// RFC 9457 JSON problem payload.
    #[must_use]
    pub fn as_problem_json(&self) -> serde_json::Value {
        let (message, details) = self.message_details();
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": self.title(),
            "status": self.status(),
            "detail": message,
            "code": self.code(),
            "details": details,
            "remediation": self.remediation(),
        })
    }

    fn message_details(&self) -> (&str, &serde_json::Value) {
        match self {
            Self::Validation { message, details }
            | Self::NotFound { message, details }
            | Self::Conflict { message, details }
            | Self::PermissionDenied { message, details }
            | Self::Internal { message, details }
            | Self::UpstreamFailure { message, details }
            | Self::Timeout { message, details }
            | Self::ValaTaskDagCycle { message, details }
            | Self::ValaEvalRefKindMismatch { message, details }
            | Self::Unauthenticated { message, details }
            | Self::TokenExpired { message, details }
            | Self::InvalidToken { message, details }
            | Self::InsufficientScope { message, details }
            | Self::CredentialRevoked { message, details }
            | Self::DataValidation { message, details }
            | Self::DataUnknownDataType { message, details }
            | Self::DataInvalidSplitRule { message, details }
            | Self::DataTargetColumnUnknown { message, details }
            | Self::DataInvalidInterfaceOption { message, details }
            | Self::DataInterfaceMetadataRequired { message, details }
            | Self::ModelUnknownModelType { message, details }
            | Self::ModelValidation { message, details }
            | Self::ModelMissingSignature { message, details }
            | Self::ModelDtypeNormalizeFailed { message, details }
            | Self::ModelShapeInvalid { message, details }
            | Self::ModelHfRevisionInvalid { message, details }
            | Self::ModelHfTaskMissing { message, details }
            | Self::ModelCustomLoaderInvalid { message, details }
            | Self::ModelSerializerUnavailable { message, details }
            | Self::PromptInvalidVariableName { message, details }
            | Self::PromptDuplicateVariable { message, details }
            | Self::PromptUndeclaredPlaceholder { message, details }
            | Self::PromptUnreferencedVariable { message, details }
            | Self::PromptUndeclaredMediaPlaceholder { message, details }
            | Self::PromptUnreferencedMediaVariable { message, details }
            | Self::PromptMediaPlaceholderNotIsolated { message, details }
            | Self::PromptMediaInSystemMessage { message, details }
            | Self::PromptUnsupportedMediaForProvider { message, details }
            | Self::PromptInvalidMediaType { message, details }
            | Self::PromptMissingMediaVariable { message, details }
            | Self::PromptMediaNotRegularFile { message, details }
            | Self::PromptMediaTooLarge { message, details }
            | Self::PromptMediaInvalidExtension { message, details }
            | Self::PromptMediaIo { message, details }
            | Self::PromptEmptyModel { message, details }
            | Self::PromptInvalidResponseSchema { message, details }
            | Self::PromptLoaderBadExtension { message, details }
            | Self::PromptLoaderIo { message, details }
            | Self::PromptSerializeRequest { message, details }
            | Self::PromptProviderAuth { message, details }
            | Self::PromptProviderRateLimit { message, details }
            | Self::PromptProviderUpstream { message, details }
            | Self::PromptProviderTimeout { message, details }
            | Self::PromptProviderMismatch { message, details }
            | Self::PromptSettingsProviderMismatch { message, details }
            | Self::PromptSettingsDecode { message, details }
            | Self::PromptMissingVariable { message, details }
            | Self::PromptUnsupportedHandoff { message, details }
            | Self::PromptResponseDecode { message, details }
            | Self::PromptDraftInvalid { message, details } => (message, details),
        }
    }

    /// Creates an internal error that preserves the source Skald code.
    #[must_use]
    pub fn internal_from_skald(code: &'static str, message: String) -> Self {
        Self::Internal {
            message,
            details: serde_json::json!({ "skald_code": code }),
        }
    }
}

impl From<skald_spec::SkaldError> for WyrdError {
    fn from(error: skald_spec::SkaldError) -> Self {
        let message = error.to_string();
        match error {
            skald_spec::SkaldError::MissingVariable(name) => Self::PromptMissingVariable {
                message,
                details: serde_json::json!({ "name": name }),
            },
            skald_spec::SkaldError::MediaPlaceholderNotFound { name } => {
                Self::PromptUndeclaredMediaPlaceholder {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaPlaceholderNotIsolated { name } => {
                Self::PromptMediaPlaceholderNotIsolated {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaInSystemMessage { name } => {
                Self::PromptMediaInSystemMessage {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::UnsupportedMediaForProvider { provider, kind } => {
                Self::PromptUnsupportedMediaForProvider {
                    message,
                    details: serde_json::json!({
                        "provider": format!("{provider:?}"),
                        "kind": format!("{kind:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::InvalidMediaType(source) => Self::PromptInvalidMediaType {
                message,
                details: serde_json::json!({ "source": source }),
            },
            skald_spec::SkaldError::MissingMediaVariable { name } => {
                Self::PromptMissingMediaVariable {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaNotRegularFile { path } => {
                Self::PromptMediaNotRegularFile {
                    message,
                    details: serde_json::json!({ "path": path }),
                }
            }
            skald_spec::SkaldError::MediaTooLarge { path, size, limit } => {
                Self::PromptMediaTooLarge {
                    message,
                    details: serde_json::json!({ "path": path, "size": size, "limit": limit }),
                }
            }
            skald_spec::SkaldError::MediaInvalidExtension { path, kind } => {
                Self::PromptMediaInvalidExtension {
                    message,
                    details: serde_json::json!({ "path": path, "kind": format!("{kind:?}") }),
                }
            }
            skald_spec::SkaldError::MediaIo(source) => Self::PromptMediaIo {
                message,
                details: serde_json::json!({ "source": source }),
            },
            skald_spec::SkaldError::UnsupportedConversion { src, dst } => {
                Self::PromptUnsupportedHandoff {
                    message,
                    details: serde_json::json!({
                        "src": format!("{src:?}"),
                        "dst": format!("{dst:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::SettingsProviderMismatch { expected, got } => {
                Self::PromptSettingsProviderMismatch {
                    message,
                    details: serde_json::json!({
                        "expected": format!("{expected:?}"),
                        "got": format!("{got:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::SettingsDecode {
                provider,
                message: source,
            } => Self::PromptSettingsDecode {
                message,
                details: serde_json::json!({
                    "provider": format!("{provider:?}"),
                    "source": source,
                }),
            },
            skald_spec::SkaldError::PromptDraftInvalid {
                provider,
                message: source,
            } => Self::PromptDraftInvalid {
                message,
                details: serde_json::json!({ "provider": provider, "source": source }),
            },
            other => Self::internal_from_skald(other.code(), other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WyrdError;

    #[test]
    fn auth_codes_match_status() {
        for error in auth_errors() {
            let status = error
                .code()
                .split('_')
                .nth(2)
                .expect("error code has status segment")
                .parse::<u16>()
                .expect("status segment is numeric");

            assert_eq!(status, error.status());
        }
    }

    #[test]
    fn auth_variant_problem_json_roundtrips() {
        let error = WyrdError::Unauthenticated {
            message: "missing bearer token".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Not authenticated");
        assert_eq!(
            problem["remediation"],
            "Present a valid Wyrd token in the Authorization header."
        );
        assert_eq!(problem["detail"], "missing bearer token");
        assert_eq!(problem["details"]["header"], "authorization");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_UNAUTHENTICATED")
        );
    }

    #[test]
    fn auth_variants_serde_tag() {
        let error = WyrdError::InsufficientScope {
            message: "card:write is required".to_owned(),
            details: serde_json::json!({ "required": "card:write" }),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "insufficient_scope");
    }

    #[test]
    fn token_expired_problem_json() {
        let error = WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_TOKEN_EXPIRED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Token expired");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_TOKEN_EXPIRED")
        );
    }

    #[test]
    fn token_expired_serde_tag() {
        let error = WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "token_expired");
    }

    #[test]
    fn invalid_token_problem_json() {
        let error = WyrdError::InvalidToken {
            message: "signature mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_INVALID_TOKEN");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Invalid token");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_INVALID_TOKEN")
        );
    }

    #[test]
    fn invalid_token_serde_tag() {
        let error = WyrdError::InvalidToken {
            message: "signature mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "invalid_token");
    }

    #[test]
    fn credential_revoked_problem_json() {
        let error = WyrdError::CredentialRevoked {
            message: "api key wyrd_sk_abc revoked".to_owned(),
            details: serde_json::json!({ "prefix": "wyrd_sk_abc" }),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_CREDENTIAL_REVOKED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Credential revoked");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_CREDENTIAL_REVOKED")
        );
    }

    #[test]
    fn credential_revoked_serde_tag() {
        let error = WyrdError::CredentialRevoked {
            message: "credential revoked".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "credential_revoked");
    }

    fn auth_errors() -> [WyrdError; 5] {
        [
            WyrdError::Unauthenticated {
                message: "missing bearer token".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::TokenExpired {
                message: "token expired".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::InvalidToken {
                message: "token rejected".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::InsufficientScope {
                message: "scope missing".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::CredentialRevoked {
                message: "credential revoked".to_owned(),
                details: serde_json::json!({}),
            },
        ]
    }
}
