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
pub mod service_accounts;
pub mod users;

pub use roles::{RoleRow, roles_by_name};
pub use service_accounts::{
    ApiKeyLookupRow, ServiceAccountPrincipalRow, api_key_by_prefix, insert_api_key,
    insert_audit_credential_issuance, insert_audit_token_exchange, insert_refresh_token,
    service_account_by_card_ref, service_account_by_id, service_account_roles,
    touch_api_key_last_used,
};
