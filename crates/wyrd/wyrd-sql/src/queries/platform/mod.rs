//! Platform-scoped queries for `platform.*` tables.
//!
//! `tenant_resolver` is the runtime auth-boundary exception and runs on the
//! `wyrd_app` pool through `platform.resolve_tenant_by_slug`. Other modules in
//! this tree are for audited platform-admin reads and writes and take the
//! platform-admin pool.

pub mod api_keys;
pub mod roles;
pub mod tenant_resolver;
pub mod tenants;
pub mod users;
