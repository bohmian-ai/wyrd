//! Embedded Postgres role bootstrap SQL.

use secrecy::ExposeSecret;

use super::EmbeddedRoleCredentials;
pub(crate) use crate::dsn::{WYRD_APP_ROLE, WYRD_MIGRATOR_ROLE, WYRD_PLATFORM_ADMIN_ROLE};

pub(crate) const WYRD_DATABASE: &str = "wyrd";

/// Creates the managed Wyrd roles and their baseline catalog memberships.
pub(crate) const ROLE_BOOTSTRAP_SQL_TEMPLATE: &str = r#"
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_migrator') THEN
        CREATE ROLE wyrd_migrator LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD '<migrator_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_app') THEN
        CREATE ROLE wyrd_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD '<app_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
        CREATE ROLE wyrd_platform_admin LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD '<admin_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog') THEN
        CREATE ROLE wyrd_catalog NOLOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog_app') THEN
        CREATE ROLE wyrd_catalog_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD '<catalog_app_pw>';
    END IF;
END $$;
GRANT wyrd_catalog TO wyrd_catalog_app;
GRANT wyrd_catalog TO wyrd_migrator;
"#;

pub(crate) const WYRD_CATALOG_APP_ROLE: &str = "wyrd_catalog_app";

/// Renders embedded bootstrap SQL with managed role attributes, grants, and
/// memberships converged to the external bootstrap contract.
pub(crate) fn role_bootstrap_sql(credentials: &EmbeddedRoleCredentials) -> String {
    let sql = ROLE_BOOTSTRAP_SQL_TEMPLATE
        .replace(
            "'<migrator_pw>'",
            &sql_literal(credentials.migrator.expose_secret()),
        )
        .replace("'<app_pw>'", &sql_literal(credentials.app.expose_secret()))
        .replace(
            "'<admin_pw>'",
            &sql_literal(credentials.platform_admin.expose_secret()),
        )
        .replace(
            "'<catalog_app_pw>'",
            &sql_literal(credentials.catalog_app.expose_secret()),
        );

    format!(
        r#"
{sql}

ALTER ROLE {migrator_role} WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD {migrator_pw};
ALTER ROLE {app_role} WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD {app_pw};
ALTER ROLE {admin_role} WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD {admin_pw};
ALTER ROLE {catalog_role} WITH NOLOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER;
ALTER ROLE {catalog_app_role} WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD {catalog_app_pw};

REVOKE ALL PRIVILEGES ON DATABASE {database} FROM {migrator_role}, {app_role}, {admin_role}, {catalog_role}, {catalog_app_role};
REVOKE {migrator_role}, {app_role}, {admin_role}, {catalog_app_role} FROM {migrator_role}, {app_role}, {admin_role}, {catalog_role}, {catalog_app_role};

DO $$
DECLARE membership record;
BEGIN
    FOR membership IN
        SELECT parent.rolname AS parent_name, member.rolname AS member_name
        FROM pg_auth_members memberships
        JOIN pg_roles parent ON parent.oid = memberships.roleid
        JOIN pg_roles member ON member.oid = memberships.member
        WHERE member.rolname IN ('wyrd_migrator', 'wyrd_app', 'wyrd_platform_admin', 'wyrd_catalog_app')
          AND NOT (
              parent.rolname = 'wyrd_catalog'
              AND member.rolname IN ('wyrd_migrator', 'wyrd_catalog_app')
          )
    LOOP
        EXECUTE format('REVOKE %I FROM %I', membership.parent_name, membership.member_name);
    END LOOP;
END $$;

GRANT CONNECT ON DATABASE {database} TO {migrator_role}, {app_role}, {admin_role}, {catalog_app_role};
GRANT CREATE ON DATABASE {database} TO {migrator_role};
"#,
        sql = sql,
        database = WYRD_DATABASE,
        migrator_role = WYRD_MIGRATOR_ROLE,
        app_role = WYRD_APP_ROLE,
        admin_role = WYRD_PLATFORM_ADMIN_ROLE,
        catalog_role = "wyrd_catalog",
        catalog_app_role = WYRD_CATALOG_APP_ROLE,
        migrator_pw = sql_literal(credentials.migrator.expose_secret()),
        app_pw = sql_literal(credentials.app.expose_secret()),
        admin_pw = sql_literal(credentials.platform_admin.expose_secret()),
        catalog_app_pw = sql_literal(credentials.catalog_app.expose_secret()),
    )
}

/// Quotes one PostgreSQL string literal used by generated role DDL.
///
/// # Panics
///
/// Panics in debug builds when the value contains a NUL byte, which PostgreSQL
/// string literals cannot represent.
fn sql_literal(value: &str) -> String {
    debug_assert!(
        !value.contains('\0'),
        "sql_literal received a NUL byte that PostgreSQL cannot represent"
    );
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::{ROLE_BOOTSTRAP_SQL_TEMPLATE, role_bootstrap_sql};
    use crate::postgres_boot::EmbeddedRoleCredentials;

    /// External bootstrap source used to assert contract parity.
    const EXTERNAL_ROLE_BOOTSTRAP: &str = include_str!("../../bootstrap/roles.sql");

    /// Verifies that the embedded template creates every managed role shape.
    #[test]
    fn bootstrap_template_is_the_only_create_role_shape() {
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE
                .contains("CREATE ROLE wyrd_migrator LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER")
        );
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE
                .contains("CREATE ROLE wyrd_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER")
        );
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE
                .contains("CREATE ROLE wyrd_platform_admin LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER")
        );
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE
                .contains("CREATE ROLE wyrd_catalog NOLOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER")
        );
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains(
            "CREATE ROLE wyrd_catalog_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD"
        ));
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT wyrd_catalog TO wyrd_catalog_app"));
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT wyrd_catalog TO wyrd_migrator"));
    }

    /// Verifies that embedded bootstrap rendering quotes role passwords.
    #[test]
    fn bootstrap_sql_quotes_password_literals() {
        let credentials = EmbeddedRoleCredentials {
            superuser: SecretString::from("supersecret123".to_owned()),
            migrator: SecretString::from("migpw456".to_owned()),
            app: SecretString::from("apppw789".to_owned()),
            platform_admin: SecretString::from("adminpwABC".to_owned()),
            catalog_app: SecretString::from("catalogpwDEF".to_owned()),
        };

        let sql = role_bootstrap_sql(&credentials);

        assert!(sql.contains("PASSWORD 'migpw456'"));
        assert!(sql.contains(
            "ALTER ROLE wyrd_migrator WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD"
        ));
        assert!(sql.contains(
            "ALTER ROLE wyrd_app WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD"
        ));
        assert!(sql.contains(
            "ALTER ROLE wyrd_platform_admin WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD"
        ));
        assert!(sql.contains(
            "ALTER ROLE wyrd_catalog_app WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD"
        ));
        assert!(sql.contains(
            "GRANT CONNECT ON DATABASE wyrd TO wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog_app"
        ));
        assert!(sql.contains("GRANT CREATE ON DATABASE wyrd TO wyrd_migrator"));
    }

    /// Verifies that embedded and external bootstrap retain the same managed
    /// role attributes, grants, and membership repair operations.
    #[test]
    fn embedded_and_external_bootstrap_share_managed_role_shape() {
        let credentials = EmbeddedRoleCredentials {
            superuser: SecretString::from("supersecret123"),
            migrator: SecretString::from("migpw456"),
            app: SecretString::from("apppw789"),
            platform_admin: SecretString::from("adminpwABC"),
            catalog_app: SecretString::from("catalogpwDEF"),
        };
        let embedded = role_bootstrap_sql(&credentials);
        for shape in [
            "NOCREATEDB",
            "NOSUPERUSER",
            "BYPASSRLS",
            "NOBYPASSRLS",
            "REVOKE ALL PRIVILEGES ON DATABASE wyrd",
            "REVOKE wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog_app",
            "GRANT wyrd_catalog TO wyrd_catalog_app",
            "GRANT wyrd_catalog TO wyrd_migrator",
        ] {
            assert!(
                embedded.contains(shape),
                "embedded bootstrap missing {shape}"
            );
            assert!(
                EXTERNAL_ROLE_BOOTSTRAP.contains(shape),
                "external bootstrap missing {shape}"
            );
        }
    }

    /// Live PostgreSQL proofs for embedded managed-role repair.
    mod pg_tests {
        use sqlx::{Connection, PgConnection};

        use super::*;

        /// Exercises embedded role repair against an effective unexpected
        /// BYPASSRLS membership and proves the exact declared parent set remains.
        ///
        /// # Panics
        /// Panics when the repository-managed PostgreSQL fixture is unavailable or
        /// embedded repair leaves unexpected role authority.
        #[tokio::test]
        async fn embedded_bootstrap_removes_unexpected_bypassrls_parent_live() {
            let admin_url = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")
                .expect("repository SQL lane provides the admin DSN");
            let current_app_url = std::env::var("WYRD_DATABASE_URL")
                .expect("repository SQL lane provides the application DSN");
            let mut admin = PgConnection::connect(&admin_url)
                .await
                .expect("connect fixture admin");
            sqlx::raw_sql(
            "DROP ROLE IF EXISTS wyrd_embedded_operator_fixture; CREATE ROLE wyrd_embedded_operator_fixture NOLOGIN BYPASSRLS; GRANT wyrd_embedded_operator_fixture TO wyrd_app",
        )
        .execute(&mut admin)
        .await
        .expect("install unexpected membership");
            let effective_before: bool = sqlx::query_scalar(
                "SELECT pg_has_role('wyrd_app','wyrd_embedded_operator_fixture','member')",
            )
            .fetch_one(&mut admin)
            .await
            .expect("membership before repair");
            assert!(effective_before);

            let admin_dsn = url::Url::parse(&admin_url).expect("parse fixture admin DSN");
            let app_dsn = url::Url::parse(&current_app_url).expect("parse fixture app DSN");
            let credentials = EmbeddedRoleCredentials {
                superuser: SecretString::from(
                    admin_dsn
                        .password()
                        .expect("fixture admin DSN has a password"),
                ),
                migrator: SecretString::from(
                    std::env::var("WYRD_DATABASE_MIGRATOR_PASSWORD")
                        .expect("repository SQL lane provides the migrator password"),
                ),
                app: SecretString::from(
                    app_dsn.password().expect("fixture app DSN has a password"),
                ),
                platform_admin: SecretString::from(
                    std::env::var("WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD")
                        .expect("repository SQL lane provides the platform admin password"),
                ),
                catalog_app: SecretString::from(
                    std::env::var("WYRD_DATABASE_CATALOG_APP_PASSWORD")
                        .expect("repository SQL lane provides the catalog app password"),
                ),
            };
            sqlx::raw_sql(sqlx::AssertSqlSafe(role_bootstrap_sql(&credentials)))
                .execute(&mut admin)
                .await
                .expect("execute embedded role repair");

            let effective_after: bool = sqlx::query_scalar(
                "SELECT pg_has_role('wyrd_app','wyrd_embedded_operator_fixture','member')",
            )
            .fetch_one(&mut admin)
            .await
            .expect("membership after repair");
            assert!(!effective_after);
            let memberships: Vec<(String, String)> = sqlx::query_as(
            "SELECT parent.rolname,member.rolname FROM pg_auth_members memberships JOIN pg_roles parent ON parent.oid=memberships.roleid JOIN pg_roles member ON member.oid=memberships.member WHERE member.rolname IN ('wyrd_migrator','wyrd_app','wyrd_platform_admin','wyrd_catalog_app') ORDER BY parent.rolname,member.rolname",
        )
        .fetch_all(&mut admin)
        .await
        .expect("exact managed memberships");
            assert_eq!(
                memberships,
                vec![
                    ("wyrd_catalog".to_owned(), "wyrd_catalog_app".to_owned()),
                    ("wyrd_catalog".to_owned(), "wyrd_migrator".to_owned()),
                ]
            );
            let mut app = PgConnection::connect(&current_app_url)
                .await
                .expect("connect repaired app");
            assert!(
                sqlx::query("SET ROLE wyrd_embedded_operator_fixture")
                    .execute(&mut app)
                    .await
                    .is_err()
            );
            app.close().await.expect("close app connection");
            sqlx::query("DROP ROLE wyrd_embedded_operator_fixture")
                .execute(&mut admin)
                .await
                .expect("drop fixture role");
        }
    }
}
