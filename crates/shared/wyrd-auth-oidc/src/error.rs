//! Error types for OIDC verification.

/// Errors produced by OIDC discovery, JWKS resolution, and claim mapping.
///
/// All variants own their data (no boxed sources) so the type is [`Clone`] and
/// compatible with [`moka`]'s `try_get_with` coalescing, which returns
/// `Arc<E>` on failure and requires a way to recover `E` from the `Arc`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum OidcError {
    /// OIDC discovery endpoint request failed.
    ///
    /// Maps to `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` at the server boundary.
    #[error("OIDC discovery failed for issuer {issuer}: {message}")]
    Discovery { issuer: String, message: String },

    /// The issuer in the discovery metadata does not match the requested issuer.
    ///
    /// Anti-spoofing guard per the OIDC spec (§4.3).
    #[error("issuer mismatch: expected {expected:?}, found {found:?}")]
    IssuerMismatch { expected: String, found: String },

    /// The discovery document does not advertise any asymmetric signing algorithm.
    ///
    /// RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384, ES512, and EdDSA
    /// are all acceptable. Symmetric-only issuers are rejected.
    #[error(
        "no asymmetric signing algorithm in id_token_signing_alg_values_supported for issuer {issuer}"
    )]
    NoAsymmetricAlg { issuer: String },

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
