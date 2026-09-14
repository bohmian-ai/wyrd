//! Issuer configuration resolver trait.

use std::future::Future;

use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;

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

    /// Return the tenant's trusted issuer whose URL equals `issuer`, if any.
    ///
    /// Authentication paths resolve exactly one issuer from a token's `iss`, so
    /// this is their lookup. The default filters [`Self::trusted_issuers`];
    /// resolvers backed by a keyed store override it to fetch only that row.
    /// `Ok(None)` means the issuer is not trusted for the tenant.
    ///
    /// # Errors
    ///
    /// Returns the resolver's [`OidcError`] when the issuer store is
    /// unavailable or a stored issuer cannot be decoded.
    fn trusted_issuer(
        &self,
        tenant: &DataTenantId,
        issuer: &IssuerUrl,
    ) -> impl Future<Output = Result<Option<TrustedIssuer>, OidcError>> + Send {
        async move {
            Ok(self
                .trusted_issuers(tenant)
                .await?
                .into_iter()
                .find(|candidate| candidate.issuer == *issuer))
        }
    }
}
