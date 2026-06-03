//! Error types for prompt authoring.

use thiserror::Error;

/// Result alias for prompt builder operations.
pub type PromptBuilderResult<T> = Result<T, PromptBuilderError>;

/// Failures raised while converting authoring inputs into native provider wire structs.
#[derive(Debug, Error)]
pub enum PromptBuilderError {
    /// Prompt model was empty.
    #[error("prompt model must be non-empty")]
    EmptyModel,
    /// Provider name was not accepted by the raw passthrough builder.
    #[error("unsupported provider value: {0}")]
    InvalidProvider(String),
    /// Raw provider body was not valid UTF-8 JSON.
    #[error("raw provider body must be valid JSON: {0}")]
    InvalidRawJson(String),
    /// Structured-output schema was not a JSON object.
    #[error("response_format JsonSchema schema must be a JSON object")]
    InvalidResponseSchema,
    /// A requested role helper is unsupported for the current provider variant.
    #[error("role helper {role} is unsupported for provider request {provider}")]
    UnsupportedRole {
        /// Provider request variant.
        provider: String,
        /// Role helper name.
        role: String,
    },
    /// Native prompt validation failed.
    #[error("prompt validation failed: {0}")]
    Validation(String),
    /// Public Wyrd validation or codec error.
    #[error(transparent)]
    Wyrd(#[from] wyrd_spec::error::WyrdError),
    /// Prompt render failed.
    #[error(transparent)]
    Skald(#[from] skald_spec::SkaldError),
    /// Filesystem IO failed in the loader boundary.
    #[error("prompt loader IO failed at {path}: {message}")]
    Io {
        /// Path being read or written.
        path: String,
        /// Source IO message.
        message: String,
    },
}

impl PromptBuilderError {
    /// Returns the stable builder error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyModel => "SKALD_PROMPT_400_EMPTY_MODEL",
            Self::InvalidProvider(_) => "SKALD_PROMPT_400_INVALID_PROVIDER",
            Self::InvalidRawJson(_) => "SKALD_PROMPT_400_INVALID_RAW_JSON",
            Self::InvalidResponseSchema => "SKALD_PROMPT_400_INVALID_RESPONSE_SCHEMA",
            Self::UnsupportedRole { .. } => "SKALD_PROMPT_400_UNSUPPORTED_ROLE",
            Self::Validation(_) => "SKALD_PROMPT_422_VALIDATION",
            Self::Wyrd(error) => error.code(),
            Self::Skald(error) => error.code(),
            Self::Io { .. } => "SKALD_PROMPT_500_IO",
        }
    }
}

#[cfg(feature = "python")]
impl From<PromptBuilderError> for wyrd_interfaces::error::WyrdPyError {
    fn from(error: PromptBuilderError) -> Self {
        use serde_json::json;
        let message = error.to_string();
        match error {
            PromptBuilderError::EmptyModel => wyrd_spec::error::WyrdError::from(
                wyrd_spec::card::prompt::validate::PromptError::EmptyModel,
            )
            .into(),
            PromptBuilderError::InvalidResponseSchema => wyrd_spec::error::WyrdError::from(
                wyrd_spec::card::prompt::validate::PromptError::InvalidResponseSchema,
            )
            .into(),
            PromptBuilderError::Io { path, message } => wyrd_spec::error::WyrdError::from(
                wyrd_spec::card::prompt::validate::PromptError::LoaderIo { path, message },
            )
            .into(),
            PromptBuilderError::Wyrd(error) => error.into(),
            PromptBuilderError::Skald(error) => wyrd_spec::error::WyrdError::from(error).into(),
            PromptBuilderError::InvalidProvider(provider) => {
                wyrd_spec::error::WyrdError::PromptProviderMismatch {
                    message,
                    details: json!({ "provider": provider }),
                }
                .into()
            }
            PromptBuilderError::InvalidRawJson(source) => {
                wyrd_spec::error::WyrdError::PromptSerializeRequest {
                    message,
                    details: json!({ "source": source }),
                }
                .into()
            }
            PromptBuilderError::Validation(source) => {
                wyrd_spec::error::WyrdError::PromptDraftInvalid {
                    message,
                    details: json!({ "source": source }),
                }
                .into()
            }
            PromptBuilderError::UnsupportedRole { provider, role } => {
                wyrd_spec::error::WyrdError::PromptProviderMismatch {
                    message,
                    details: json!({ "provider": provider, "role": role }),
                }
                .into()
            }
        }
    }
}
