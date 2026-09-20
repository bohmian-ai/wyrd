//! Platform-scoped queries for `platform.*` tables.
//!
//! `tenant_resolver` is the runtime auth-boundary exception and runs on the
//! `wyrd_app` pool through `platform.resolve_tenant_by_slug`. Other modules in
//! this tree are for audited platform-admin reads and writes and take the
//! platform-admin pool.
//!
//! `principals`, `credentials`, and `principal_grants` own the deployment's
//! administrative identity: a durable principal, the credentials that
//! authenticate it, and the grants that authorize it, kept as three separate
//! concerns so rotating a credential never disturbs identity or authority.
//!
//! `identity` adds the optional federated way in: the one deployment-owned OIDC
//! connection, its login state, and the durable `(issuer, subject)` pin that
//! resolves a human to an existing platform principal. It never creates one.
//!
//! `provisioning` owns the durable stages that admit a tenant, so a failure at
//! any one of them leaves the directory entry failed rather than half-admitted.

pub mod credentials;
pub mod identity;
pub mod principal_grants;
pub mod principals;
pub mod provisioning;
pub mod tenant_resolver;
pub mod tenants;
