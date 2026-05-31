use crate::media::MediaKind;
use crate::request::ProviderName;

/// Crate-local result type for skald-spec behavior.
pub type SkaldResult<T> = Result<T, SkaldError>;

/// Stable skald-spec error catalog.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SkaldError {
    /// A declared prompt variable was not supplied to `Prompt::render`.
    #[error("render variable missing: {0}")]
    MissingVariable(String),
    /// A native provider request could not be serialized before rendering.
    #[error("native request serialize failed: {0}")]
    Serialize(String),
    /// Rendered JSON did not deserialize back into a native provider request.
    #[error("native request deserialize failed: {0}")]
    Deserialize(String),
    /// No direct native message conversion exists for the requested providers.
    #[error("unsupported message conversion from {src:?} to {dst:?}")]
    UnsupportedConversion {
        /// Source provider.
        src: ProviderName,
        /// Destination provider.
        dst: ProviderName,
    },
    /// A provider does not support the requested operation.
    #[error("provider {provider:?} does not support operation {op}")]
    ProviderUnsupported {
        /// Provider that rejected or cannot perform the operation.
        provider: ProviderName,
        /// Operation name, such as `embeddings` or `responses`.
        op: String,
    },
    /// A typed `model_settings` object belonged to a different provider.
    #[error("model_settings provider mismatch: expected {expected:?}, got {got:?}")]
    SettingsProviderMismatch {
        /// Provider expected by the prompt constructor.
        expected: ProviderName,
        /// Provider carried by the supplied settings object.
        got: ProviderName,
    },
    /// A `model_settings` object or dict failed to decode into native settings.
    #[error("model_settings for {provider:?} failed to decode: {message}")]
    SettingsDecode {
        /// Provider whose settings were being decoded.
        provider: ProviderName,
        /// Decode failure message.
        message: String,
    },
    /// A media placeholder was not present as an isolated text part.
    #[error("media placeholder not found: {name}")]
    MediaPlaceholderNotFound {
        /// Placeholder name.
        name: String,
    },
    /// A media placeholder was present but not isolated in its own text part.
    #[error("media placeholder '{name}' is not isolated; it must occupy its own text part")]
    MediaPlaceholderNotIsolated {
        /// Placeholder name.
        name: String,
    },
    /// A media placeholder appeared in system content.
    #[error(
        "media placeholder '{name}' found in a system message; media is not allowed in system content"
    )]
    MediaInSystemMessage {
        /// Placeholder name.
        name: String,
    },
    /// The selected provider does not support this media kind/source.
    #[error("media kind {kind:?} unsupported for provider {provider:?}")]
    UnsupportedMediaForProvider {
        /// Provider that rejected the media.
        provider: ProviderName,
        /// Media kind that was rejected.
        kind: MediaKind,
    },
    /// A media MIME type or URI was invalid for the selected provider.
    #[error("invalid media type: {0}")]
    InvalidMediaType(String),
    /// A declared media variable remained unbound at render time.
    #[error("media variable '{name}' was declared but never bound before render")]
    MissingMediaVariable {
        /// Missing media variable name.
        name: String,
    },
    /// A media path helper was given a path that is not a regular file.
    #[error("media path {path} is not a regular file")]
    MediaNotRegularFile {
        /// Local path.
        path: String,
    },
    /// A media path helper rejected an oversized file.
    #[error("media file {path} is {size} bytes; exceeds limit of {limit}")]
    MediaTooLarge {
        /// Local path.
        path: String,
        /// Observed byte size.
        size: u64,
        /// Maximum allowed byte size.
        limit: u64,
    },
    /// A media path helper could not infer a MIME type from the extension.
    #[error("media file {path} has unrecognized extension for kind {kind:?}")]
    MediaInvalidExtension {
        /// Local path.
        path: String,
        /// Expected media kind.
        kind: MediaKind,
    },
    /// Media path helper IO failed.
    #[error("media io error: {0}")]
    MediaIo(String),
    /// A declarative `PromptDraft` failed to compile into a native `Prompt`.
    #[error("prompt draft compile failed for provider {provider}: {message}")]
    PromptDraftInvalid {
        /// Provider name from the draft.
        provider: String,
        /// Compile failure message.
        message: String,
    },
}

impl SkaldError {
    /// Construct a missing-variable render error.
    pub fn missing_variable(name: impl Into<String>) -> Self {
        Self::MissingVariable(name.into())
    }

    /// Construct a serialization error.
    pub fn serialize(error: impl std::fmt::Display) -> Self {
        Self::Serialize(error.to_string())
    }

    /// Construct a deserialization error.
    pub fn deserialize(error: impl std::fmt::Display) -> Self {
        Self::Deserialize(error.to_string())
    }

    /// Construct an unsupported message-conversion error.
    pub fn unsupported_conversion(src: ProviderName, dst: ProviderName) -> Self {
        Self::UnsupportedConversion { src, dst }
    }

    /// Construct an unsupported provider-operation error.
    pub fn provider_unsupported(provider: ProviderName, op: impl Into<String>) -> Self {
        Self::ProviderUnsupported {
            provider,
            op: op.into(),
        }
    }

    /// Stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingVariable(_) => "SKALD_SPEC_422_MISSING_VARIABLE",
            Self::Serialize(_) => "SKALD_SPEC_500_SERIALIZE",
            Self::Deserialize(_) => "SKALD_SPEC_422_DESERIALIZE",
            Self::UnsupportedConversion { .. } => "SKALD_SPEC_501_UNSUPPORTED_CONVERSION",
            Self::ProviderUnsupported { .. } => "SKALD_SPEC_501_PROVIDER_UNSUPPORTED",
            Self::SettingsProviderMismatch { .. } => "SKALD_SPEC_400_SETTINGS_PROVIDER_MISMATCH",
            Self::SettingsDecode { .. } => "SKALD_SPEC_400_SETTINGS_DECODE",
            Self::MediaPlaceholderNotFound { .. } => "SKALD_SPEC_422_MEDIA_PLACEHOLDER_NOT_FOUND",
            Self::MediaPlaceholderNotIsolated { .. } => {
                "SKALD_SPEC_422_MEDIA_PLACEHOLDER_NOT_ISOLATED"
            }
            Self::MediaInSystemMessage { .. } => "SKALD_SPEC_400_MEDIA_IN_SYSTEM_MESSAGE",
            Self::UnsupportedMediaForProvider { .. } => {
                "SKALD_SPEC_400_UNSUPPORTED_MEDIA_FOR_PROVIDER"
            }
            Self::InvalidMediaType(_) => "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
            Self::MissingMediaVariable { .. } => "SKALD_SPEC_422_MISSING_MEDIA_VARIABLE",
            Self::MediaNotRegularFile { .. } => "SKALD_SPEC_400_MEDIA_NOT_REGULAR_FILE",
            Self::MediaTooLarge { .. } => "SKALD_SPEC_400_MEDIA_TOO_LARGE",
            Self::MediaInvalidExtension { .. } => "SKALD_SPEC_400_MEDIA_INVALID_EXTENSION",
            Self::MediaIo(_) => "SKALD_SPEC_500_MEDIA_IO",
            Self::PromptDraftInvalid { .. } => "SKALD_SPEC_400_PROMPT_DRAFT_INVALID",
        }
    }
}
