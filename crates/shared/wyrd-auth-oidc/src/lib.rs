//! Generic OIDC verification toolkit.
//!
//! Provides the human-login relying party ([`relying_party`]), JWKS caching, trusted-issuer registry, claim
//! mapping, and configuration resolver traits for any conformant OIDC provider.
//!
//! # Design constraints
//!
//! - No dependency on `wyrd-auth-verify` (F09 — commit 03 makes that crate
//!   depend on this one; a reverse dependency would create a cycle).
//! - No SQL: issuer configuration arrives through [`config::IssuerConfigResolver`].
//! - No PyO3: this crate is strictly PyO3-free.
//! - [`claims::MappedClaims`] carries the external subject only — no `card_ref`
//!   from token claims (F04). The server resolves the card via
//!   [`registry::WorkloadBindingResolver`].

pub mod claims;
pub mod config;
pub mod error;
pub mod jwks;
pub mod registry;
pub mod relying_party;
pub mod screening;

pub use claims::{MappedClaims, map_claims};
pub use config::IssuerConfigResolver;
pub use error::OidcError;
pub use jwks::{JwksCache, OidcKid};
pub use registry::{
    ClaimMapping, ClaimPath, ClientAuth, IssuerVerification, TrustedIssuer, TrustedIssuerRegistry,
    WorkloadBinding, WorkloadBindingResolver,
};
pub use relying_party::{
    Authorization, CodeRedemption, IssuerParameterMetadata, ProviderHttpError, ProviderMetadata,
    RelyingParty, RelyingPartyError, VerifiedIdToken,
};
pub use screening::{
    AddressPolicy, MAX_RESPONSE_BYTES, ScreenError, ScreenedHttp, read_bounded_body,
};
