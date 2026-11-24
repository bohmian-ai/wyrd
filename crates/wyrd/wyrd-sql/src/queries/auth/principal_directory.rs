//! Tenant-scoped directory of principals a tenant administrator can assign
//! Roles to.
//!
//! Assignable principals are users and Service accounts that are not
//! deleted (see `assignable_principals_union!`).
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_spec::auth::WorkloadFamily;

use crate::TenantConn;
use crate::queries::auth::service_accounts::workload_family;

/// Each assignable principal with the Roles it holds; each statement below
/// appends its own predicate.
///
/// `roles` is the distinct `{role, scope}` pairs the account holds directly
/// or through a Group, ordered by role then scope.
macro_rules! assignable_principals_sql {
    () => {
        concat!(
            r"
        SELECT id, kind, status, email, name, display_name, identity_issuer,
               restriction_kind, restriction_name,
               COALESCE((
                   SELECT jsonb_agg(a.assignment ORDER BY a.assignment->>'role', a.assignment->'scope')
                     FROM (SELECT DISTINCT jsonb_build_object('role', r.name, 'scope', b.scope) AS assignment
                             FROM wyrd.auth_role_bindings b
                             JOIN wyrd.auth_roles r
                               ON r.data_tenant_id = b.data_tenant_id AND r.id = b.role_id
                            WHERE b.data_tenant_id = wyrd.current_tenant()
                              AND (b.user_id = principals.id
                                   OR b.service_account_id = principals.id
                                   OR b.group_id IN (
                                       SELECT m.group_id FROM wyrd.auth_group_memberships m
                                        WHERE m.data_tenant_id = b.data_tenant_id
                                          AND (m.user_id = principals.id
                                               OR m.service_account_id = principals.id)))) a
               ), '[]'::jsonb) AS roles
          FROM (",
            assignable_principals_union!(),
            r") principals
        "
        )
    };
}

/// The directory filters shared by a page and its status counts: kind, exact
/// email, exact name and Role held at any scope.
macro_rules! principal_filter_sql {
    () => {
        concat!(
            r"
         WHERE ($1::text IS NULL OR kind = $1)
           AND ($2::text IS NULL OR email = $2)
           AND ($3::text IS NULL OR name = $3)
           AND ($4::text IS NULL OR id IN (",
            role_holders!("r.name = $4"),
            r"))"
        )
    };
}

/// One assignable principal by id.
const ASSIGNABLE_PRINCIPAL_BY_ID_SQL: &str =
    concat!(assignable_principals_sql!(), " WHERE id = $1");

/// A filtered keyset page of assignable principals, ordered by id.
const LIST_ASSIGNABLE_PRINCIPALS_SQL: &str = concat!(
    assignable_principals_sql!(),
    principal_filter_sql!(),
    r"
           AND ($5::uuid IS NULL OR id > $5)
         ORDER BY id
         LIMIT $6"
);

/// Status counts over every principal the filters match, ignoring paging.
const COUNT_ASSIGNABLE_PRINCIPALS_SQL: &str = concat!(
    r"
        SELECT count(*) FILTER (WHERE status = 'active') AS active,
               count(*) FILTER (WHERE status = 'suspended') AS suspended
          FROM (",
    assignable_principals_union!(),
    r") principals",
    principal_filter_sql!()
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
    /// A Service account's name, or a local User's username.
    pub name: Option<String>,
    /// A User's provider-sourced display name.
    pub display_name: Option<String>,
    /// Issuer of a User's linked external identity, when it has one.
    pub identity_issuer: Option<String>,
    /// Restricting family kind, for a family-restricted Service account.
    pub restriction_kind: Option<String>,
    /// Restricting family name, for a family-restricted Service account.
    pub restriction_name: Option<String>,
    /// JSON array of the distinct `{role, scope}` pairs the account holds.
    pub roles: serde_json::Value,
}

/// How many principals matching a directory filter are in each status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct PrincipalStatusCountsRow {
    /// Active principals.
    pub active: i64,
    /// Suspended principals.
    pub suspended: i64,
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
    /// Exact name: a Service account's name or a local User's username.
    pub name: Option<&'a str>,
    /// Role name the principal holds at any scope, directly or through a
    /// Group.
    pub role: Option<&'a str>,
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
        .bind(filter.role)
        .bind(filter.after)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Count the principals in each status that `filter` matches, across every
/// page; `filter.after` is ignored.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn count_assignable_principals(
    conn: &mut TenantConn<'_>,
    filter: PrincipalFilter<'_>,
) -> Result<PrincipalStatusCountsRow, sqlx::Error> {
    sqlx::query_as::<_, PrincipalStatusCountsRow>(COUNT_ASSIGNABLE_PRINCIPALS_SQL)
        .bind(filter.kind)
        .bind(filter.email)
        .bind(filter.name)
        .bind(filter.role)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// SQL-shape checks of the assignable-principal directory statements.
#[cfg(test)]
mod tests {
    use super::{
        ASSIGNABLE_PRINCIPAL_BY_ID_SQL, COUNT_ASSIGNABLE_PRINCIPALS_SQL,
        LIST_ASSIGNABLE_PRINCIPALS_SQL,
    };

    /// Neither lookup, listing nor counting reaches deleted principals, the tenant
    /// administrator, or the `system` writer.
    ///
    /// # Panics
    /// Panics when a statement does not carry exactly two
    /// `status <> 'deleted'` guards or does not restrict rows to
    /// `principal_kind = 'service'`.
    #[test]
    fn directory_excludes_unassignable_principals() {
        for sql in [
            ASSIGNABLE_PRINCIPAL_BY_ID_SQL,
            LIST_ASSIGNABLE_PRINCIPALS_SQL,
            COUNT_ASSIGNABLE_PRINCIPALS_SQL,
        ] {
            assert_eq!(sql.matches("status <> 'deleted'").count(), 2);
            assert!(sql.contains("principal_kind = 'service'"));
        }
    }
}
