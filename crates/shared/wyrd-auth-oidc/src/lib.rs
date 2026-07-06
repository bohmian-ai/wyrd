//! Generic OIDC verification toolkit.
//!
//! Provides provider discovery, JWKS caching, trusted-issuer registry, claim
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
pub mod provider;
pub mod registry;

pub use claims::{MappedClaims, map_claims};
pub use config::IssuerConfigResolver;
pub use error::OidcError;
pub use jwks::{JwksCache, OidcKid};
pub use provider::{OidcProvider, ProviderMetadata};
pub use registry::{
    ClaimMapping, ClaimPath, ClientAuth, TrustedIssuer, TrustedIssuerRegistry, WorkloadBinding,
    WorkloadBindingResolver,
};
