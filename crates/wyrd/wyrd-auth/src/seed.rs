//! Server-tier builtin role seeding.

use serde_json::Value;
use wyrd_runtime::builtin_roles::{BUILTIN_ROLES, builtin_role_uuid};
use wyrd_spec::DataTenantId;
use wyrd_sql::TenantConn;

/// Error returned while seeding builtin roles.
#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    /// Database query failed.
    #[error("seed query failed: {0}")]
    Database(#[from] sqlx::Error),
    /// Builtin permissions failed to serialize to JSONB.
    #[error("serializing builtin role permissions failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Seed the builtin roles for `data_tenant_id`.
///
/// Existing rows are left untouched, making this helper safe to rerun during
/// tenant repair workflows.
#[tracing::instrument(skip(conn), fields(data_tenant_id = %data_tenant_id))]
pub async fn seed_builtin_roles_for_tenant(
    conn: &mut TenantConn<'_>,
    data_tenant_id: DataTenantId,
) -> Result<(), SeedError> {
    for role in BUILTIN_ROLES {
        let permissions_json: Value = serde_json::to_value(role.permissions)?;
        let id = builtin_role_uuid(data_tenant_id, role.name);
        sqlx::query(
            r"
            INSERT INTO wyrd.auth_roles
                (id, data_tenant_id, name, permissions, builtin)
            VALUES ($1, $2, $3, $4, TRUE)
            ON CONFLICT (data_tenant_id, name) DO NOTHING
            ",
        )
        .bind(id)
        .bind(data_tenant_id.as_uuid())
        .bind(role.name)
        .bind(permissions_json)
        .execute(&mut **conn.transaction())
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod pg_tests {
    use std::collections::BTreeSet;

    use sqlx::Row;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::Permission;
    use wyrd_runtime::builtin_roles::{BUILTIN_ROLES, DEFAULT_CARD_ROLE, builtin_role_uuid};
    use wyrd_spec::DataTenantId;
    use wyrd_sql::queries::auth::list_service_account_roles;

    use super::seed_builtin_roles_for_tenant;

    #[tokio::test]
    async fn seed_round_trips_constant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");

        for role in BUILTIN_ROLES {
            let row =
                sqlx::query("SELECT id, permissions, builtin FROM wyrd.auth_roles WHERE name = $1")
                    .bind(role.name)
                    .fetch_one(&mut **conn.transaction())
                    .await
                    .expect("role row exists");
            let id: uuid::Uuid = row.try_get("id").expect("id decodes");
            let permissions: serde_json::Value =
                row.try_get("permissions").expect("permissions decode");
            let builtin: bool = row.try_get("builtin").expect("builtin decodes");
            let decoded: Vec<Permission> =
                serde_json::from_value(permissions).expect("permissions deserialize");

            assert_eq!(id, builtin_role_uuid(tenant, role.name));
            assert!(builtin);
            assert_eq!(decoded, role.permissions);
        }
    }

    /// The existing-tenant backfill migration writes the `workload` and
    /// `wyrd_default` rows with exactly the permissions the built-in seed
    /// serializes, and grants `wyrd_default` to a Card-bound principal that
    /// existed before it.
    ///
    /// The migration runs inside one tenant connection, so row-level security
    /// confines its set-based inserts to that tenant.
    ///
    /// # Panics
    ///
    /// Panics when the fixture, seed, migration, or decoded permissions differ,
    /// or the existing principal does not hold `wyrd_default` afterward.
    #[tokio::test]
    async fn backfill_matches_seed_and_grants_existing_card_principals() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        let backfilled = ["workload", DEFAULT_CARD_ROLE];
        sqlx::query("DELETE FROM wyrd.auth_roles WHERE name = ANY($1)")
            .bind(&backfilled[..])
            .execute(&mut **conn.transaction())
            .await
            .expect("backfilled rows delete");
        let principal = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref,
                  space, name, version, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3,
                     '{\"kind\":\"Service\",\"name\":\"svc\",\"version\":\"1.0.0\",\"space\":\"prod\"}',
                     'prod', 'svc', '1.0.0', 'active', $1)",
        )
        .bind(principal)
        .bind(tenant.as_uuid())
        .bind(uuid::Uuid::new_v4())
        .execute(&mut **conn.transaction())
        .await
        .expect("existing card principal inserts");

        sqlx::raw_sql(include_str!(
            "../../wyrd-sql/migrations/20261002000100_workload_role.sql"
        ))
        .execute(&mut **conn.transaction())
        .await
        .expect("backfill runs");

        for name in backfilled {
            let permissions: serde_json::Value =
                sqlx::query_scalar("SELECT permissions FROM wyrd.auth_roles WHERE name = $1")
                    .bind(name)
                    .fetch_one(&mut **conn.transaction())
                    .await
                    .expect("backfilled row exists");
            let decoded: Vec<Permission> =
                serde_json::from_value(permissions).expect("permissions deserialize");
            let seeded = BUILTIN_ROLES
                .iter()
                .find(|role| role.name == name)
                .expect("backfilled role is built in");
            assert_eq!(decoded, seeded.permissions, "{name}");
        }
        assert_eq!(
            list_service_account_roles(&mut conn, principal)
                .await
                .expect("roles list"),
            [DEFAULT_CARD_ROLE]
        );
    }

    #[tokio::test]
    async fn seed_is_idempotent() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("first seed succeeds");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("second seed succeeds");

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_roles")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("count query succeeds");

        assert_eq!(
            usize::try_from(count).expect("role count is nonnegative"),
            BUILTIN_ROLES.len()
        );
    }

    #[tokio::test]
    async fn seed_per_tenant_isolation() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant_b, "test-tenant-2")
            .await
            .expect("tenant B inserts");

        let mut conn_a = fixture
            .tenant_conn_for(tenant_a)
            .await
            .expect("tenant A conn opens");
        seed_builtin_roles_for_tenant(&mut conn_a, tenant_a)
            .await
            .expect("tenant A seeds");
        let admin_a: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM wyrd.auth_roles WHERE name = 'admin'")
                .fetch_one(&mut **conn_a.transaction())
                .await
                .expect("tenant A admin exists");

        let mut conn_b = fixture
            .tenant_conn_for(tenant_b)
            .await
            .expect("tenant B conn opens");
        seed_builtin_roles_for_tenant(&mut conn_b, tenant_b)
            .await
            .expect("tenant B seeds");
        let rows_b: Vec<(uuid::Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM wyrd.auth_roles ORDER BY name")
                .fetch_all(&mut **conn_b.transaction())
                .await
                .expect("tenant B roles query succeeds");

        assert_eq!(rows_b.len(), BUILTIN_ROLES.len());
        assert_eq!(
            rows_b
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<BTreeSet<_>>(),
            BUILTIN_ROLES.iter().map(|role| role.name).collect()
        );
        let admin_b = rows_b
            .iter()
            .find(|(_, name)| name == "admin")
            .map(|(id, _)| *id)
            .expect("tenant B admin exists");
        assert_ne!(admin_a, admin_b);
    }
}
