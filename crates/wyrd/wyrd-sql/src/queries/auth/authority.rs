//! One-statement load of a tenant's current authorization state.
//!
//! The server's authority cache compiles this row into per-account grants. It
//! is read on a cache miss, a known invalidation, or an observed revision
//! change, never on a warm request, so the whole load is one statement.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde::Deserialize;
use serde_json::Value;
use sqlx::types::{Json, Uuid};

use crate::TenantConn;

/// Reads, in one statement, everything one tenant's authority compiles
/// from: the committed revision, every binding with its role's operations,
/// group memberships, live Users and Services, and every stored API key,
/// revoked ones included, with its ceiling.
const AUTHORITY_STATE_SQL: &str = r"
SELECT
  COALESCE((SELECT revision FROM wyrd.auth_authorization_revisions), 0) AS revision,
  COALESCE((
    SELECT jsonb_agg(jsonb_build_object(
             'role', r.name, 'operations', r.permissions, 'scope', b.scope,
             'user_id', b.user_id, 'service_account_id', b.service_account_id,
             'group_id', b.group_id))
      FROM wyrd.auth_role_bindings b
      JOIN wyrd.auth_roles r ON r.data_tenant_id = b.data_tenant_id AND r.id = b.role_id
  ), '[]'::jsonb) AS bindings,
  COALESCE((
    SELECT jsonb_agg(jsonb_build_object(
             'group_id', group_id, 'user_id', user_id,
             'service_account_id', service_account_id))
      FROM wyrd.auth_group_memberships
  ), '[]'::jsonb) AS memberships,
  COALESCE((
    SELECT jsonb_agg(id) FROM wyrd.auth_users WHERE status = 'active'
  ), '[]'::jsonb) AS users,
  COALESCE((
    SELECT jsonb_agg(jsonb_build_object(
             'id', id, 'principal_kind', principal_kind,
             'restriction_kind', restriction_kind, 'restriction_name', restriction_name))
      FROM wyrd.auth_service_accounts
     WHERE status = 'active' AND principal_kind <> 'system'
  ), '[]'::jsonb) AS service_accounts,
  COALESCE((
    SELECT jsonb_agg(jsonb_build_object(
             'id', id, 'permission_ceiling', permission_ceiling))
      FROM wyrd.auth_api_keys
  ), '[]'::jsonb) AS api_keys
";

/// One stored binding with its role's operation bundle.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthorityBinding {
    /// Role name, for diagnostics.
    pub role: String,
    /// The role's stored operations.
    pub operations: Value,
    /// The binding's stored typed scope.
    pub scope: Value,
    /// User subject.
    pub user_id: Option<Uuid>,
    /// Service-account subject.
    pub service_account_id: Option<Uuid>,
    /// Group subject.
    pub group_id: Option<Uuid>,
}

/// One group membership.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AuthorityMembership {
    /// Group.
    pub group_id: Uuid,
    /// Member User.
    pub user_id: Option<Uuid>,
    /// Member Service account.
    pub service_account_id: Option<Uuid>,
}

/// One active Service or tenant-admin account.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthorityServiceAccount {
    /// Account id.
    pub id: Uuid,
    /// `service` or `tenant_admin`.
    pub principal_kind: String,
    /// Restricting family kind.
    pub restriction_kind: Option<String>,
    /// Restricting family name.
    pub restriction_name: Option<String>,
}

/// One stored API key, revoked or not, and the ceiling it bounds.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthorityApiKey {
    /// Credential id.
    pub id: Uuid,
    /// Stored permission-expression ceiling, when the key carries one.
    pub permission_ceiling: Option<Vec<String>>,
}

/// A tenant's complete current authorization state at one revision.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AuthorityState {
    /// Committed authorization revision; `0` before the first change.
    pub revision: i64,
    /// Every binding.
    pub bindings: Json<Vec<AuthorityBinding>>,
    /// Every group membership.
    pub memberships: Json<Vec<AuthorityMembership>>,
    /// Active User ids.
    pub users: Json<Vec<Uuid>>,
    /// Active Service and tenant-admin accounts.
    pub service_accounts: Json<Vec<AuthorityServiceAccount>>,
    /// Every API key with its lifecycle state.
    pub api_keys: Json<Vec<AuthorityApiKey>>,
}

/// Reads the tenant's current authorization state in one statement.
///
/// # Errors
///
/// Returns the database error, or a decode error for malformed stored JSON.
pub async fn authority_state(conn: &mut TenantConn<'_>) -> Result<AuthorityState, sqlx::Error> {
    sqlx::query_as::<_, AuthorityState>(AUTHORITY_STATE_SQL)
        .fetch_one(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::AUTHORITY_STATE_SQL;

    /// Proves the load reads committed revision, bindings, memberships and
    /// only live accounts, never the SYSTEM writer.
    #[test]
    fn loads_live_state_in_one_statement() {
        assert_eq!(AUTHORITY_STATE_SQL.matches(';').count(), 0);
        assert!(AUTHORITY_STATE_SQL.contains("principal_kind <> 'system'"));
        assert!(AUTHORITY_STATE_SQL.contains("status = 'active'"));
        assert!(AUTHORITY_STATE_SQL.contains("wyrd.auth_authorization_revisions"));
    }
}
