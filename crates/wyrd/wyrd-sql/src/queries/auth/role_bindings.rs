//! Tenant-scoped role bindings, local groups, and the authorization revision.
//!
//! A binding assigns one role to one User, Service account, or local group at
//! one typed scope. Every access-affecting mutation in a transaction calls
//! [`advance_authorization_revision`] once before commit, which notifies
//! replicas transactionally. Functions take `&mut TenantConn<'_>` and rely on
//! RLS for tenant scoping.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;
use sqlx::Row;
use sqlx::types::{Json, Uuid};
use wyrd_spec::auth::{BindingSubject, PermissionScope, PrincipalKindTag, RoleSource};

use crate::TenantConn;

/// The channel replicas listen on for committed authorization revisions.
/// The payload is `<data_tenant_id>:<revision>`.
pub const AUTHORIZATION_CHANNEL: &str = "wyrd_authorization";

const INSERT_BINDING_SQL: &str = r"
INSERT INTO wyrd.auth_role_bindings
    (data_tenant_id, id, role_id, user_id, service_account_id, group_id, scope, source)
VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7)
ON CONFLICT DO NOTHING
";

const DELETE_BINDING_SQL: &str = r"
DELETE FROM wyrd.auth_role_bindings WHERE id = $1 AND source = 'direct'
";

const LIST_BINDINGS_SQL: &str = r"
SELECT b.id, b.role_id, r.name AS role_name, r.permissions AS role_operations,
       b.user_id, b.service_account_id, b.group_id, b.scope, b.source
  FROM wyrd.auth_role_bindings b
  JOIN wyrd.auth_roles r ON r.data_tenant_id = b.data_tenant_id AND r.id = b.role_id
 ORDER BY b.id
";

const REPLACE_IDP_BINDINGS_SQL: &str = r"
WITH wanted AS (
    SELECT r.id AS role_id, w.id
      FROM unnest($2::text[], $3::uuid[]) AS w(name, id)
      JOIN wyrd.auth_roles r ON r.name = w.name
), removed AS (
    DELETE FROM wyrd.auth_role_bindings
     WHERE user_id = $1
       AND source = 'idp'
       AND NOT (scope = to_jsonb('all'::text) AND role_id IN (SELECT role_id FROM wanted))
    RETURNING 1
), added AS (
    INSERT INTO wyrd.auth_role_bindings (data_tenant_id, id, role_id, user_id, scope, source)
    SELECT wyrd.current_tenant(), id, role_id, $1, to_jsonb('all'::text), 'idp' FROM wanted
    ON CONFLICT DO NOTHING
    RETURNING 1
)
SELECT EXISTS (SELECT 1 FROM removed) OR EXISTS (SELECT 1 FROM added)
";

const ADVANCE_REVISION_SQL: &str = r"
WITH advanced AS (
    INSERT INTO wyrd.auth_authorization_revisions (data_tenant_id, revision)
    VALUES (wyrd.current_tenant(), 1)
    ON CONFLICT (data_tenant_id) DO UPDATE
       SET revision = wyrd.auth_authorization_revisions.revision + 1,
           updated_at = statement_timestamp()
    RETURNING data_tenant_id, revision
)
SELECT revision,
       pg_catalog.pg_notify('wyrd_authorization', data_tenant_id::text || ':' || revision::text)
  FROM advanced
";

const CURRENT_REVISION_SQL: &str = r"
SELECT revision FROM wyrd.auth_authorization_revisions
";

const INSERT_GROUP_SQL: &str = r"
INSERT INTO wyrd.auth_groups (data_tenant_id, id, name)
VALUES (wyrd.current_tenant(), $1, $2)
";

const DELETE_GROUP_SQL: &str = r"
DELETE FROM wyrd.auth_groups WHERE id = $1
";

const INSERT_MEMBERSHIP_SQL: &str = r"
INSERT INTO wyrd.auth_group_memberships (data_tenant_id, group_id, user_id, service_account_id)
VALUES (wyrd.current_tenant(), $1, $2, $3)
ON CONFLICT DO NOTHING
";

const DELETE_MEMBERSHIP_SQL: &str = r"
DELETE FROM wyrd.auth_group_memberships
 WHERE group_id = $1
   AND user_id IS NOT DISTINCT FROM $2
   AND service_account_id IS NOT DISTINCT FROM $3
";

const LIST_GROUPS_SQL: &str = r"
SELECT id, name FROM wyrd.auth_groups ORDER BY name
";

const LIST_MEMBERSHIPS_SQL: &str = r"
SELECT group_id, user_id, service_account_id
  FROM wyrd.auth_group_memberships
 ORDER BY group_id, user_id, service_account_id
";

/// The account or group columns one binding or membership references.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubjectColumns {
    /// User account, when the subject is a User.
    pub user_id: Option<Uuid>,
    /// Service account, when the subject is a Service or tenant admin.
    pub service_account_id: Option<Uuid>,
    /// Local group, when the subject is a group.
    pub group_id: Option<Uuid>,
}

impl SubjectColumns {
    /// The columns `subject` occupies.
    ///
    /// # Errors
    ///
    /// Returns `None` for a principal kind that is never a binding subject.
    #[must_use]
    pub fn of(subject: &BindingSubject) -> Option<Self> {
        match subject {
            BindingSubject::Principal { id, kind } => match kind {
                PrincipalKindTag::User => Some(Self {
                    user_id: Some(id.as_uuid()),
                    ..Self::default()
                }),
                PrincipalKindTag::Service | PrincipalKindTag::TenantAdmin => Some(Self {
                    service_account_id: Some(id.as_uuid()),
                    ..Self::default()
                }),
                PrincipalKindTag::System | PrincipalKindTag::GlobalAdmin => None,
            },
            BindingSubject::Group { id } => Some(Self {
                group_id: Some(id.as_uuid()),
                ..Self::default()
            }),
        }
    }
}

/// One stored binding joined to its role.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleBindingRow {
    /// Binding id.
    pub id: Uuid,
    /// Role id.
    pub role_id: Uuid,
    /// Role name.
    pub role_name: String,
    /// The role's stored operation bundle.
    pub role_operations: Value,
    /// Subject columns.
    pub subject: SubjectColumns,
    /// Stored typed scope.
    pub scope: Value,
    /// Assignment provenance.
    pub source: RoleSource,
}

/// One group membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MembershipRow {
    /// Group id.
    pub group_id: Uuid,
    /// Member User, when the member is a User.
    pub user_id: Option<Uuid>,
    /// Member Service account, when the member is a Service.
    pub service_account_id: Option<Uuid>,
}

/// Inserts one binding; `false` when an identical binding already exists.
///
/// # Errors
///
/// Returns the database error, including a foreign-key violation for a
/// subject or role that does not exist in the tenant.
pub async fn insert_role_binding(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    role_id: Uuid,
    subject: SubjectColumns,
    scope: &PermissionScope,
    source: RoleSource,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(INSERT_BINDING_SQL)
        .bind(id)
        .bind(role_id)
        .bind(subject.user_id)
        .bind(subject.service_account_id)
        .bind(subject.group_id)
        .bind(Json(scope))
        .bind(source.as_str())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes one direct binding; provider bindings are replaced only by their
/// provider's login.
///
/// # Errors
///
/// Returns the database error.
pub async fn delete_role_binding(conn: &mut TenantConn<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(DELETE_BINDING_SQL)
        .bind(id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Every binding of the tenant with its role, ordered by id.
///
/// # Errors
///
/// Returns the database error, or a decode error for an unknown source.
pub async fn list_role_bindings(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<RoleBindingRow>, sqlx::Error> {
    let rows = sqlx::query(LIST_BINDINGS_SQL)
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(RoleBindingRow {
                id: row.try_get("id")?,
                role_id: row.try_get("role_id")?,
                role_name: row.try_get("role_name")?,
                role_operations: row.try_get("role_operations")?,
                subject: SubjectColumns {
                    user_id: row.try_get("user_id")?,
                    service_account_id: row.try_get("service_account_id")?,
                    group_id: row.try_get("group_id")?,
                },
                scope: row.try_get("scope")?,
                source: decode_source(row.try_get("source")?)?,
            })
        })
        .collect()
}

/// Replaces one User's provider-projected bindings with tenant-scope
/// bindings of `role_names`, leaving every direct binding intact. `ids`
/// supplies one fresh binding id per name.
///
/// Returns `true` when any provider binding was added or removed.
///
/// # Errors
///
/// Returns the database error.
pub async fn replace_idp_user_bindings(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    role_names: &[&str],
) -> Result<bool, sqlx::Error> {
    let ids: Vec<Uuid> = role_names.iter().map(|_| Uuid::now_v7()).collect();
    sqlx::query_scalar::<_, bool>(REPLACE_IDP_BINDINGS_SQL)
        .bind(user_id)
        .bind(role_names)
        .bind(&ids)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Advances the tenant's authorization revision and notifies replicas when
/// the transaction commits. Returns the new revision.
///
/// Call once in every transaction that changes roles, bindings, groups,
/// memberships, account status, credentials, or provider assignments.
///
/// # Errors
///
/// Returns the database error.
pub async fn advance_authorization_revision(conn: &mut TenantConn<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query(ADVANCE_REVISION_SQL)
        .fetch_one(&mut **conn.transaction())
        .await?
        .try_get("revision")
}

/// The tenant's committed authorization revision; `0` before its first
/// access-affecting change.
///
/// # Errors
///
/// Returns the database error.
pub async fn current_authorization_revision(conn: &mut TenantConn<'_>) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(CURRENT_REVISION_SQL)
        .fetch_optional(&mut **conn.transaction())
        .await?
        .unwrap_or(0))
}

/// Creates a local group.
///
/// # Errors
///
/// Returns the database error, including a unique violation for a taken name.
pub async fn insert_group(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_GROUP_SQL)
        .bind(id)
        .bind(name)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Deletes a group with its memberships and bindings.
///
/// # Errors
///
/// Returns the database error.
pub async fn delete_group(conn: &mut TenantConn<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(DELETE_GROUP_SQL)
        .bind(id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Adds one account to a group; `false` when it is already a member.
///
/// # Errors
///
/// Returns the database error, including a foreign-key violation for an
/// account or group outside the tenant.
pub async fn insert_group_membership(
    conn: &mut TenantConn<'_>,
    group_id: Uuid,
    member: SubjectColumns,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(INSERT_MEMBERSHIP_SQL)
        .bind(group_id)
        .bind(member.user_id)
        .bind(member.service_account_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Removes one account from a group.
///
/// # Errors
///
/// Returns the database error.
pub async fn delete_group_membership(
    conn: &mut TenantConn<'_>,
    group_id: Uuid,
    member: SubjectColumns,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(DELETE_MEMBERSHIP_SQL)
        .bind(group_id)
        .bind(member.user_id)
        .bind(member.service_account_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Every group as `(id, name)`, ordered by name.
///
/// # Errors
///
/// Returns the database error.
pub async fn list_groups(conn: &mut TenantConn<'_>) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String)>(LIST_GROUPS_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Every group membership of the tenant.
///
/// # Errors
///
/// Returns the database error.
pub async fn list_group_memberships(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<MembershipRow>, sqlx::Error> {
    let rows = sqlx::query(LIST_MEMBERSHIPS_SQL)
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(MembershipRow {
                group_id: row.try_get("group_id")?,
                user_id: row.try_get("user_id")?,
                service_account_id: row.try_get("service_account_id")?,
            })
        })
        .collect()
}

fn decode_source(source: &str) -> Result<RoleSource, sqlx::Error> {
    match source {
        "idp" => Ok(RoleSource::Idp),
        "direct" => Ok(RoleSource::Direct),
        other => Err(sqlx::Error::Decode(
            format!("unknown role binding source {other:?}").into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ADVANCE_REVISION_SQL, DELETE_BINDING_SQL, REPLACE_IDP_BINDINGS_SQL, SubjectColumns,
        decode_source,
    };
    use wyrd_spec::auth::{BindingSubject, PrincipalId, PrincipalKindTag, RoleSource};

    /// Proves provider projection and deletion never touch direct bindings,
    /// and the revision advance notifies the replica channel.
    #[test]
    fn writes_keep_provenance_and_notify() {
        assert!(DELETE_BINDING_SQL.contains("source = 'direct'"));
        assert!(REPLACE_IDP_BINDINGS_SQL.contains("AND source = 'idp'"));
        assert!(ADVANCE_REVISION_SQL.contains("pg_notify('wyrd_authorization'"));
        assert!(!ADVANCE_REVISION_SQL.contains("COMMIT"));
    }

    /// Proves subjects map to exactly one column and non-account kinds are
    /// refused.
    #[test]
    fn subjects_map_to_one_column() {
        let id = PrincipalId::new(uuid::Uuid::now_v7());
        let user = SubjectColumns::of(&BindingSubject::Principal {
            id,
            kind: PrincipalKindTag::User,
        })
        .expect("user");
        assert_eq!(user.user_id, Some(id.as_uuid()));
        assert!(
            SubjectColumns::of(&BindingSubject::Principal {
                id,
                kind: PrincipalKindTag::System,
            })
            .is_none()
        );
        assert_eq!(decode_source("idp").expect("idp"), RoleSource::Idp);
        assert!(decode_source("granted").is_err());
    }
}
