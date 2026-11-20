//! Server-tier role administration helpers.

use serde_json::json;
use uuid::Uuid;
use wyrd_spec::auth::{Operation, RoleSource, ScopedRoleAssignment, compile_binding};
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    SubjectColumns, advance_authorization_revision, insert_role_binding, role_by_name,
};

use crate::error::store_error;

/// Validates `assignments` as [`bind_assignments`] will bind them and
/// returns each assignment's role id, in order.
///
/// Each role must exist and grant at least one operation applicable to its
/// explicit, valid scope, so an invitation or account creation can never
/// become broader than it reads.
///
/// # Errors
///
/// [`WyrdError::Validation`] for an empty list or the first missing role,
/// invalid scope, or inapplicable assignment, and
/// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
pub async fn validate_assignments(
    conn: &mut TenantConn<'_>,
    assignments: &[ScopedRoleAssignment],
) -> Result<Vec<Uuid>, WyrdError> {
    if assignments.is_empty() {
        return Err(WyrdError::Validation {
            message: "assignments must name at least one role and scope".to_owned(),
            details: json!({ "field": "assignments" }),
        });
    }
    let mut role_ids = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        let invalid = |message: &str| WyrdError::Validation {
            message: message.to_owned(),
            details: json!({ "role": assignment.role, "scope": assignment.scope }),
        };
        assignment
            .scope
            .validate()
            .map_err(|_| invalid("the assignment scope is not a valid object scope"))?;
        let role = role_by_name(conn, &assignment.role)
            .await
            .map_err(store_error)?
            .ok_or_else(|| invalid("a Role does not exist in this tenant"))?;
        let operations: Vec<Operation> = serde_json::from_value(role.permissions)
            .map_err(|_| invalid("the Role's stored operations are corrupt"))?;
        compile_binding(&operations, &assignment.scope)
            .map_err(|_| invalid("the Role grants no operation applicable to this scope"))?;
        role_ids.push(role.id);
    }
    Ok(role_ids)
}

/// Binds each of `assignments` to `subject` as direct bindings and advances
/// the tenant's authorization revision once, on the caller's transaction.
/// An identical existing binding is kept.
///
/// # Errors
///
/// Every [`validate_assignments`] error. The caller rolls back on any
/// error, so no partial grant remains.
pub async fn bind_assignments(
    conn: &mut TenantConn<'_>,
    subject: SubjectColumns,
    assignments: &[ScopedRoleAssignment],
) -> Result<(), WyrdError> {
    let role_ids = validate_assignments(conn, assignments).await?;
    for (assignment, role_id) in assignments.iter().zip(role_ids) {
        insert_role_binding(
            conn,
            Uuid::now_v7(),
            role_id,
            subject,
            &assignment.scope,
            RoleSource::Direct,
        )
        .await
        .map_err(store_error)?;
    }
    advance_authorization_revision(conn)
        .await
        .map_err(store_error)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum RoleAdminError {
    /// Builtin roles cannot be deleted.
    #[error("cannot delete builtin role")]
    CannotDeleteBuiltin,
    /// Role row was not found in the current tenant.
    #[error("role not found")]
    NotFound,
    /// Database query failed.
    #[error("database query failed: {0}")]
    Database(#[from] sqlx::Error),
}

/// Delete a non-builtin role by ID.
#[tracing::instrument(skip(conn), fields(role_id = %role_id))]
pub async fn delete_role(
    conn: &mut TenantConn<'_>,
    role_id: uuid::Uuid,
) -> Result<(), RoleAdminError> {
    let row: Option<(bool,)> = sqlx::query_as("SELECT builtin FROM wyrd.auth_roles WHERE id = $1")
        .bind(role_id)
        .fetch_optional(&mut **conn.transaction())
        .await?;
    let (builtin,) = row.ok_or(RoleAdminError::NotFound)?;
    if builtin {
        return Err(RoleAdminError::CannotDeleteBuiltin);
    }

    sqlx::query("DELETE FROM wyrd.auth_roles WHERE id = $1")
        .bind(role_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

#[cfg(test)]
mod pg_tests {
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::builtin_roles::builtin_role_uuid;

    use crate::seed::seed_builtin_roles_for_tenant;

    use super::{RoleAdminError, delete_role};

    #[tokio::test]
    async fn delete_blocks_builtin() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");

        for name in ["admin", "viewer"] {
            let error = delete_role(&mut conn, builtin_role_uuid(tenant, name))
                .await
                .expect_err("builtin delete should fail");

            assert!(matches!(error, RoleAdminError::CannotDeleteBuiltin));
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM wyrd.auth_roles WHERE name = $1)")
                    .bind(name)
                    .fetch_one(&mut **conn.transaction())
                    .await
                    .expect("exists query succeeds");
            assert!(exists);
        }
    }

    #[tokio::test]
    async fn delete_succeeds_for_custom() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let role_id = uuid::Uuid::new_v4();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query(
            "INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
             VALUES ($1, $2, 'custom', '[]'::jsonb, FALSE)",
        )
        .bind(role_id)
        .bind(tenant.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("custom role inserts");

        delete_role(&mut conn, role_id)
            .await
            .expect("custom role deletes");

        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM wyrd.auth_roles WHERE id = $1)")
                .bind(role_id)
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("exists query succeeds");
        assert!(!exists);
    }

    #[tokio::test]
    async fn update_succeeds_for_builtin() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");

        sqlx::query(
            "UPDATE wyrd.auth_roles
                SET permissions = $1
              WHERE name = 'editor'",
        )
        .bind(serde_json::json!([{ "resource": "cards", "action": "read", "scope": "all" }]))
        .execute(&mut **conn.transaction())
        .await
        .expect("builtin permissions update succeeds");
    }

    #[tokio::test]
    async fn rename_admin_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");

        let error = sqlx::query(
            "UPDATE wyrd.auth_roles
                SET name = 'renamed'
              WHERE name = 'admin'",
        )
        .execute(&mut **conn.transaction())
        .await
        .expect_err("builtin role rename should fail");

        let sqlx::Error::Database(db_error) = error else {
            panic!("expected database error");
        };
        assert_eq!(db_error.code().as_deref(), Some("23514"));
        assert_eq!(
            db_error.constraint(),
            Some("auth_builtin_role_immutable_name")
        );
    }

    #[tokio::test]
    async fn operator_role_renamable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let role_id = uuid::Uuid::new_v4();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query(
            "INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
             VALUES ($1, $2, 'custom', '[]'::jsonb, FALSE)",
        )
        .bind(role_id)
        .bind(tenant.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("custom role inserts");

        sqlx::query("UPDATE wyrd.auth_roles SET name = 'renamed' WHERE id = $1")
            .bind(role_id)
            .execute(&mut **conn.transaction())
            .await
            .expect("custom role rename succeeds");

        let name: String = sqlx::query_scalar("SELECT name FROM wyrd.auth_roles WHERE id = $1")
            .bind(role_id)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("renamed role fetch succeeds");
        assert_eq!(name, "renamed");
    }
}
