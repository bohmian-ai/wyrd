//! Error types for OIDC verification.

/// Errors produced by JWKS resolution and claim mapping.
///
/// All variants own their data (no boxed sources) so the type is [`Clone`] and
/// compatible with [`moka`]'s `try_get_with` coalescing, which returns
/// `Arc<E>` on failure and requires a way to recover `E` from the `Arc`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum OidcError {
    /// JWKS endpoint is unreachable, returned a non-2xx status, or the response
    /// could not be parsed.
    ///
    /// Maps to `WYRD_AUTH_503_VERIFY_UNAVAILABLE` at the server boundary.
    #[error("JWKS unavailable for issuer {issuer}: {message}")]
    JwksUnavailable { issuer: String, message: String },

    /// The JWT `kid` is not present in the JWKS, even after one refetch.
    ///
    /// Maps to `WYRD_AUTH_401_INVALID_TOKEN` at the server boundary.
    #[error("unknown key id {kid:?} for issuer {issuer}")]
    UnknownKid { issuer: String, kid: String },

    /// A required claim path resolved to nothing in the token claims object.
    #[error("required claim at path {path:?} is missing or empty")]
    ClaimMissing { path: String },
}
