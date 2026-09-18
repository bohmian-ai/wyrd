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

pub mod credentials;
pub mod principal_grants;
pub mod principals;
pub mod tenant_resolver;
pub mod tenants;
