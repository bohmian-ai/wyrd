//! Error types for native Skald cache operations.

/// Result alias for fallible skald-cache operations.
pub type SkaldCacheResult<T> = Result<T, SkaldCacheError>;

/// Cache-layer failures with stable machine-readable codes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkaldCacheError {
    /// An opaque request body cannot be inspected for native cache directives.
    #[error("opaque provider request cannot produce a cache key")]
    OpaqueRequest,
    /// A typed native request failed deterministic JSON serialization.
    #[error("failed to serialize cache key material: {0}")]
    Serialize(String),
}

impl SkaldCacheError {
    /// Creates a serialization error while preserving the stable error variant.
    pub fn serialize(error: impl std::fmt::Display) -> Self {
        Self::Serialize(error.to_string())
    }

    /// Returns the stable Wyrd error code for this cache failure.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::OpaqueRequest => "SKALD_CACHE_400_OPAQUE_REQUEST",
            Self::Serialize(_) => "SKALD_CACHE_500_SERIALIZE",
        }
    }
}
