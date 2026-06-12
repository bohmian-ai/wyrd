//! Tenant-scoped auth queries for `wyrd.auth_*` tables.
//!
//! Functions in this module tree take `&mut TenantConn<'_>` and must include
//! an explicit `data_tenant_id = $...` predicate in addition to database RLS.
//! Keep short single-statement queries inline; put joins, CTEs, batch writes,
//! and reused statements under `queries/auth/sql/` and load them with SQLx's
//! query-file macros.

pub mod api_keys;
pub mod governance_tokens;
pub mod refresh_tokens;
pub mod roles;
pub mod users;
