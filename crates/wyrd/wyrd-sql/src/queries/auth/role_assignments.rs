//! Tenant-scoped role-assignment queries.
//!
//! Functions take `&mut TenantConn<'_>` and rely on database RLS for tenant
//! scoping.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::Error as SqlxError;
use sqlx::types::Uuid;
use wyrd_spec::auth::RoleSource;

use crate::TenantConn;

const GRANT_ROLE_TO_USER_SQL: &str = r"
        INSERT INTO wyrd.auth_user_roles (data_tenant_id, user_id, role_id, source)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (data_tenant_id, user_id, role_id, source) DO NOTHING
        ";

const REVOKE_ROLE_FROM_USER_SQL: &str = "DELETE FROM wyrd.auth_user_roles
           WHERE data_tenant_id = wyrd.current_tenant()
             AND user_id = $1
             AND role_id = $2
             AND source = $3";

/// A Role held from both sources is one effective Role, so names are distinct.
const LIST_USER_ROLES_SQL: &str = r"
        SELECT DISTINCT r.name
          FROM wyrd.auth_user_roles ur
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = ur.data_tenant_id
           AND r.id = ur.role_id
         WHERE ur.data_tenant_id = wyrd.current_tenant()
           AND ur.user_id = $1
         ORDER BY r.name
        ";

const LIST_USER_ROLE_ASSIGNMENTS_SQL: &str = r"
        SELECT r.name, ur.source
          FROM wyrd.auth_user_roles ur
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = ur.data_tenant_id
           AND r.id = ur.role_id
         WHERE ur.data_tenant_id = wyrd.current_tenant()
           AND ur.user_id = $1
         ORDER BY r.name, ur.source
        ";

/// Replaces a user's `idp` role bindings with exactly the named set in one
/// statement, leaving `direct` bindings untouched.
const REPLACE_IDP_USER_ROLES_SQL: &str = r"
        WITH wanted AS (
            SELECT id
              FROM wyrd.auth_roles
             WHERE name = ANY($2)
        ), removed AS (
            DELETE FROM wyrd.auth_user_roles
             WHERE user_id = $1
               AND source = 'idp'
               AND role_id NOT IN (SELECT id FROM wanted)
            RETURNING 1
        ), added AS (
            INSERT INTO wyrd.auth_user_roles (data_tenant_id, user_id, role_id, source)
            SELECT wyrd.current_tenant(), $1, id, 'idp' FROM wanted
            ON CONFLICT (data_tenant_id, user_id, role_id, source) DO NOTHING
            RETURNING 1
        )
        SELECT EXISTS (SELECT 1 FROM removed) OR EXISTS (SELECT 1 FROM added)
        ";

const GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL: &str = r"
        INSERT INTO wyrd.auth_service_account_roles (
            data_tenant_id, service_account_id, role_id
        ) VALUES ($1, $2, $3)
        ON CONFLICT (data_tenant_id, service_account_id, role_id) DO NOTHING
        ";

const REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL: &str = "DELETE FROM wyrd.auth_service_account_roles
           WHERE data_tenant_id = wyrd.current_tenant()
             AND service_account_id = $1
             AND role_id = $2";

const LIST_SERVICE_ACCOUNT_ROLES_SQL: &str = r"
        SELECT r.name
          FROM wyrd.auth_service_account_roles sar
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = sar.data_tenant_id
           AND r.id = sar.role_id
         WHERE sar.data_tenant_id = wyrd.current_tenant()
           AND sar.service_account_id = $1
         ORDER BY r.name
        ";

/// Grant a role to a user from `source`.
///
/// The same Role may be held from both sources; each is its own row. Returns
/// `Ok(true)` when a row was inserted and `Ok(false)` when the user already
/// held the Role from `source`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn grant_role_to_user(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_id: Uuid,
    source: RoleSource,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(GRANT_ROLE_TO_USER_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(user_id)
        .bind(role_id)
        .bind(source.as_str())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Revoke a user's assignment of a role from `source`, leaving an assignment
/// of the same Role from the other source in place.
///
/// Returns `Ok(true)` when a row was deleted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn revoke_role_from_user(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_id: Uuid,
    source: RoleSource,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(REVOKE_ROLE_FROM_USER_SQL)
        .bind(user_id)
        .bind(role_id)
        .bind(source.as_str())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Make a user's `idp` role assignments exactly the named set.
///
/// The federated sign-in path owns this: a human's provider-asserted authority
/// is restated on every login, so the login result is the whole truth for the
/// user's `idp` assignments. Persisting it is what lets a later refresh
/// rotation re-read the roles the session actually holds instead of minting an
/// authority-free successor. `direct` assignments belong to tenant
/// administrators and are never touched here.
///
/// Names with no `wyrd.auth_roles` row are dropped rather than stored. Such a
/// name resolves to no permission anywhere in Wyrd, so keeping it would record
/// authority that does not exist. `idp` Roles the user holds and the new set
/// omits are removed in the same statement, which is how a provider-side
/// revocation reaches Wyrd.
///
/// Returns `true` when the statement granted or removed at least one role, so
/// the caller can audit a real change and stay silent for an unchanged set.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the statement.
pub async fn replace_idp_user_roles(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_names: &[&str],
) -> Result<bool, SqlxError> {
    sqlx::query_scalar::<_, bool>(REPLACE_IDP_USER_ROLES_SQL)
        .bind(user_id)
        .bind(role_names)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// List the distinct role names a user holds from any source, ordered by
/// name. This is the user's effective Role set for token issuance.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn list_user_roles(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(LIST_USER_ROLES_SQL)
        .bind(user_id)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// List a user's role assignments with their sources, ordered by role name
/// and then source.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query, or a decode error
/// when a stored source is outside the `idp`/`direct` check constraint.
pub async fn list_user_role_assignments(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
) -> Result<Vec<(String, RoleSource)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(LIST_USER_ROLE_ASSIGNMENTS_SQL)
        .bind(user_id)
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter()
        .map(|(role, source)| Ok((role, decode_source(&source)?)))
        .collect()
}

/// Decode a stored `auth_user_roles.source` value.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] for a value outside the check constraint.
fn decode_source(source: &str) -> Result<RoleSource, sqlx::Error> {
    match source {
        "idp" => Ok(RoleSource::Idp),
        "direct" => Ok(RoleSource::Direct),
        other => Err(sqlx::Error::Decode(
            format!("unknown role assignment source {other:?}").into(),
        )),
    }
}

/// Grant a role to a service account.
///
/// Returns `Ok(true)` when a row was inserted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn grant_role_to_service_account(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(service_account_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Revoke a role from a service account.
///
/// Returns `Ok(true)` when a row was deleted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn revoke_role_from_service_account(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
    role_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL)
        .bind(service_account_id)
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List role names granted to a service account, ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn list_service_account_roles(
    conn: &mut TenantConn<'_>,
    service_account_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(LIST_SERVICE_ACCOUNT_ROLES_SQL)
        .bind(service_account_id)
        .fetch_all(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::{
        GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL, GRANT_ROLE_TO_USER_SQL, LIST_SERVICE_ACCOUNT_ROLES_SQL,
        LIST_USER_ROLES_SQL, REPLACE_IDP_USER_ROLES_SQL, REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL,
        REVOKE_ROLE_FROM_USER_SQL, decode_source,
    };
    use wyrd_spec::auth::RoleSource;

    /// Grants are idempotent per assignment, and a user's key includes the
    /// source so one Role can be held from both.
    #[test]
    fn grants_are_idempotent() {
        assert!(
            GRANT_ROLE_TO_USER_SQL
                .contains("ON CONFLICT (data_tenant_id, user_id, role_id, source) DO NOTHING")
        );
        assert!(
            GRANT_ROLE_TO_SERVICE_ACCOUNT_SQL
                .contains("ON CONFLICT (data_tenant_id, service_account_id, role_id) DO NOTHING")
        );
    }

    /// Login replacement and user revocation are scoped to one source.
    #[test]
    fn user_writes_are_source_scoped() {
        assert!(REVOKE_ROLE_FROM_USER_SQL.contains("AND source = $3"));
        assert!(REPLACE_IDP_USER_ROLES_SQL.contains("AND source = 'idp'"));
        assert!(REPLACE_IDP_USER_ROLES_SQL.contains("SELECT wyrd.current_tenant(), $1, id, 'idp'"));
    }

    /// Stored sources decode to the wire enum and anything else is refused.
    #[test]
    fn sources_decode() {
        assert_eq!(decode_source("idp").expect("idp decodes"), RoleSource::Idp);
        assert_eq!(
            decode_source("direct").expect("direct decodes"),
            RoleSource::Direct
        );
        assert!(decode_source("granted").is_err());
    }

    #[test]
    fn revokes_target_join_tables_without_transaction_control() {
        assert!(REVOKE_ROLE_FROM_USER_SQL.contains("DELETE FROM wyrd.auth_user_roles"));
        assert!(
            REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL
                .contains("DELETE FROM wyrd.auth_service_account_roles")
        );
        assert!(!REVOKE_ROLE_FROM_USER_SQL.contains("COMMIT"));
        assert!(!REVOKE_ROLE_FROM_SERVICE_ACCOUNT_SQL.contains("COMMIT"));
    }

    #[test]
    fn list_queries_join_roles_for_names() {
        for sql in [LIST_USER_ROLES_SQL, LIST_SERVICE_ACCOUNT_ROLES_SQL] {
            assert!(sql.contains("r.name"));
            assert!(sql.contains("JOIN wyrd.auth_roles r"));
            assert!(sql.contains("ORDER BY r.name"));
        }
    }
}
