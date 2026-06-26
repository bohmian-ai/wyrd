//! Issuer configuration resolver trait.

use std::future::Future;

use wyrd_spec::DataTenantId;

use crate::error::OidcError;
use crate::registry::TrustedIssuer;

/// Resolves the trusted issuers configured for a tenant.
///
/// The concrete implementation is provided by the server layer:
/// - Self-hosted: reads from a config file.
/// - Cloud multi-tenant: queries a per-tenant config store.
///
/// This crate stays SQL-free; resolvers that need database access implement
/// this trait in the server tier.
///
/// # F02 — tenant-scoped
///
/// The server resolves the tenant (from the hostname, slug, or explicit header)
/// before calling this resolver and passes the resolved `DataTenantId`. The
/// resolver returns only that tenant's configured issuers; it must never
/// cross-pollinate issuers across tenants.
pub trait IssuerConfigResolver: Send + Sync + std::fmt::Debug {
    /// Return all trusted issuers for the given tenant.
    ///
    /// Returns an empty `Vec` when no issuers are configured for the tenant.
    /// This is not an error; the auth layer will reject the token with
    /// `UntrustedIssuer`.
    fn trusted_issuers(
        &self,
        tenant: &DataTenantId,
    ) -> impl Future<Output = Result<Vec<TrustedIssuer>, OidcError>> + Send;
}
