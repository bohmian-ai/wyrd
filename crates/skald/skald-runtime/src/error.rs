//! Runtime error catalog for native Skald dispatch.

use skald_providers::ProviderError;
use skald_spec::ProviderName;
use thiserror::Error;

/// Result alias for runtime operations.
pub type SkaldRuntimeResult<T> = Result<T, SkaldRuntimeError>;

/// Runtime-owned dispatch and response-validation failures.
#[derive(Debug, Error)]
pub enum SkaldRuntimeError {
    /// No provider client was registered for the request target.
    #[error("provider not registered: {provider:?}")]
    ProviderNotRegistered {
        /// Missing provider name.
        provider: ProviderName,
    },
    /// A registered provider client failed the call.
    #[error("provider error for {provider:?}: {source}")]
    Provider {
        /// Provider that handled the request.
        provider: ProviderName,
        /// Provider-layer error with its own stable code.
        #[source]
        source: ProviderError,
    },
    /// Provider output did not satisfy the prompt response schema.
    #[error("response did not match schema {schema_name}: {message}")]
    ResponseDecode {
        /// Response schema name from the prompt response type.
        schema_name: String,
        /// Validation or decode failure message.
        message: String,
    },
}

impl SkaldRuntimeError {
    /// Creates a missing-provider error for registry dispatch.
    pub fn provider_not_registered(provider: ProviderName) -> Self {
        Self::ProviderNotRegistered { provider }
    }

    /// Wraps a provider client failure with the dispatch target.
    pub fn from_provider(provider: ProviderName, source: ProviderError) -> Self {
        Self::Provider { provider, source }
    }

    /// Creates a response schema decode error.
    pub fn response_decode(schema_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ResponseDecode {
            schema_name: schema_name.into(),
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable Skald runtime code.
    ///
    /// Provider wrappers delegate to the inner error code so downstream
    /// boundaries can preserve the original transport reason.
    pub fn code(&self) -> &str {
        match self {
            Self::ProviderNotRegistered { .. } => "SKALD_RUNTIME_404_PROVIDER",
            Self::Provider { source, .. } => source.code(),
            Self::ResponseDecode { .. } => "SKALD_RUNTIME_422_RESPONSE_DECODE",
        }
    }
}
