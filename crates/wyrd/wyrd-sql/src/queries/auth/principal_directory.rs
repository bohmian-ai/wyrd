//! Tenant-scoped directory of principals a tenant administrator can assign
//! Roles to.
//!
//! Assignable principals are users and Service accounts that are not
//! deleted. The tenant administrator and the internal `system` writer are
//! deliberately absent: their authority is fixed and never re-assigned.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_spec::auth::WorkloadFamily;

use crate::TenantConn;
use crate::queries::auth::service_accounts::workload_family;

/// Users and Service accounts in one shape; each statement below appends its
/// own predicate.
macro_rules! assignable_principals_sql {
    () => {
        r"
        SELECT id, kind, status, email, name, restriction_kind, restriction_name
          FROM (
                SELECT id, 'user'::text AS kind, status, email,
                       NULL::text AS name, NULL::text AS restriction_kind,
                       NULL::text AS restriction_name
                  FROM wyrd.auth_users
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND status <> 'deleted'
                UNION ALL
                SELECT id, principal_kind AS kind, status, NULL::text AS email,
                       name, restriction_kind, restriction_name
                  FROM wyrd.auth_service_accounts
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND principal_kind = 'service'
                   AND status <> 'deleted'
               ) principals
        "
    };
}

/// One assignable principal by id.
const ASSIGNABLE_PRINCIPAL_BY_ID_SQL: &str =
    concat!(assignable_principals_sql!(), " WHERE id = $1");

/// A filtered keyset page of assignable principals, ordered by id.
const LIST_ASSIGNABLE_PRINCIPALS_SQL: &str = concat!(
    assignable_principals_sql!(),
    r"
         WHERE ($1::text IS NULL OR kind = $1)
           AND ($2::text IS NULL OR email = $2)
           AND ($3::text IS NULL OR name = $3)
           AND ($4::uuid IS NULL OR id > $4)
         ORDER BY id
         LIMIT $5"
);

/// One assignable principal.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssignablePrincipalRow {
    /// Principal id.
    pub id: Uuid,
    /// Principal kind label: `user` or `service`.
    pub kind: String,
    /// Status label: `active` or `suspended`.
    pub status: String,
    /// Email, for users that have one.
    pub email: Option<String>,
    /// Principal name, for Service accounts.
    pub name: Option<String>,
    /// Restricting family kind, for a family-restricted Service account.
    pub restriction_kind: Option<String>,
    /// Restricting family name, for a family-restricted Service account.
    pub restriction_name: Option<String>,
}

impl AssignablePrincipalRow {
    /// The account's restricting family, when it has one.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] when the stored pair is malformed.
    pub fn workload_family(&self) -> Result<Option<WorkloadFamily>, sqlx::Error> {
        workload_family(
            self.restriction_kind.as_deref(),
            self.restriction_name.as_deref(),
        )
    }
}

/// Exact-match filters and keyset position for
/// [`list_assignable_principals`]. `None` leaves a dimension unfiltered.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrincipalFilter<'a> {
    /// Kind label: `user` or `service`.
    pub kind: Option<&'a str>,
    /// Exact email; only users carry one.
    pub email: Option<&'a str>,
    /// Exact name; only Service accounts carry one.
    pub name: Option<&'a str>,
    /// Return only principals whose id sorts after this one.
    pub after: Option<Uuid>,
}

/// Resolve one assignable principal by id.
///
/// Returns `None` for an id that is unknown, deleted, in another tenant, or
/// names the tenant administrator or `system` writer, so a caller can refuse
/// all of them identically without enumerating which.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn assignable_principal(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<AssignablePrincipalRow>, sqlx::Error> {
    sqlx::query_as::<_, AssignablePrincipalRow>(ASSIGNABLE_PRINCIPAL_BY_ID_SQL)
        .bind(id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// List up to `limit` assignable principals matching `filter`, ordered by id.
///
/// Paging is keyset on the id: pass the last id of a page as
/// `filter.after` to read the next page. Ids are UUIDv7, so the order is
/// creation order.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn list_assignable_principals(
    conn: &mut TenantConn<'_>,
    filter: PrincipalFilter<'_>,
    limit: i64,
) -> Result<Vec<AssignablePrincipalRow>, sqlx::Error> {
    sqlx::query_as::<_, AssignablePrincipalRow>(LIST_ASSIGNABLE_PRINCIPALS_SQL)
        .bind(filter.kind)
        .bind(filter.email)
        .bind(filter.name)
        .bind(filter.after)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// SQL-shape checks of the assignable-principal directory statements.
#[cfg(test)]
mod tests {
    use super::{ASSIGNABLE_PRINCIPAL_BY_ID_SQL, LIST_ASSIGNABLE_PRINCIPALS_SQL};

    /// Neither lookup nor listing reaches deleted principals, the tenant
    /// administrator, or the `system` writer.
    #[test]
    fn directory_excludes_unassignable_principals() {
        for sql in [
            ASSIGNABLE_PRINCIPAL_BY_ID_SQL,
            LIST_ASSIGNABLE_PRINCIPALS_SQL,
        ] {
            assert_eq!(sql.matches("status <> 'deleted'").count(), 2);
            assert!(sql.contains("principal_kind = 'service'"));
        }
    }
}
