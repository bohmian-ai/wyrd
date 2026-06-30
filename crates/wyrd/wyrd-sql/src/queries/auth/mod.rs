//! Tenant-scoped auth queries for `wyrd.auth_*` tables.
//!
//! Functions in this module tree take `&mut TenantConn<'_>`. Database RLS is
//! the load-bearing tenant boundary; selected queries also include explicit
//! `data_tenant_id = $...` predicates when needed for joins or indexes.
//! Keep short single-statement queries inline; put joins, CTEs, batch writes,
//! and reused statements under `queries/auth/sql/` and load them with SQLx's
//! query-file macros.

pub mod api_keys;
pub mod login_state;
pub mod refresh_tokens;
pub mod revocation;
pub mod role_assignments;
pub mod roles;
pub mod service_accounts;
pub mod user_identities;
pub mod users;

pub use api_keys::revoke_api_key;
pub use login_state::{LoginStateRow, insert_login_state, take_login_state};
pub use refresh_tokens::{
    consume_active_refresh, insert_refresh_token_rotated, refresh_by_hash, revoke_refresh,
    revoke_refresh_family,
};
pub use revocation::{
    revoke_service_account_principal, revoke_user_principal, service_account_revocation_epoch,
    user_revocation_epoch,
};
pub use role_assignments::{
    grant_role_to_service_account, grant_role_to_user, list_service_account_roles, list_user_roles,
    revoke_role_from_service_account, revoke_role_from_user,
};
pub use roles::{
    RoleRow, delete_role, insert_role, list_roles, role_by_id, role_by_name, roles_by_name,
};
#[allow(deprecated)]
pub use service_accounts::service_account_roles;
pub use service_accounts::{
    ApiKeyLookupRow, ApiKeyStatus, ServiceAccountPrincipalRow, api_key_by_prefix,
    api_key_status_by_prefix, delete_service_account, insert_api_key,
    insert_audit_credential_issuance, insert_audit_token_exchange, insert_refresh_token,
    insert_service_account, service_account_by_card_ref, service_account_by_id,
    touch_api_key_last_used,
};
pub use user_identities::{upsert_user_identity, user_id_by_identity};
pub use users::{UserRow, delete_user, insert_user, user_by_email, user_by_id};
