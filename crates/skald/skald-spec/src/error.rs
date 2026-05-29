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
        }
    }
}
