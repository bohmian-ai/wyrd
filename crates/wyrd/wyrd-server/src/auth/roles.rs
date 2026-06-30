//! Server-tier role administration helpers.

use wyrd_sql::TenantConn;

/// Role administration failure.
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
mod tests {
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::builtin_roles::builtin_role_uuid;

    use crate::auth::seed::seed_builtin_roles_for_tenant;

    use super::{RoleAdminError, delete_role};

    #[tokio::test]
    async fn delete_blocks_builtin() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");

        for name in ["admin", "runtime_admin"] {
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
              WHERE name = 'writer'",
        )
        .bind(serde_json::json!([{ "resource": "cards", "action": "read" }]))
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
