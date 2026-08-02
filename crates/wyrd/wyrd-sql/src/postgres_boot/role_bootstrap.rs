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
REVOKE {migrator_role}, {app_role}, {admin_role}, {catalog_role}, {catalog_app_role} FROM {migrator_role}, {app_role}, {admin_role}, {catalog_role}, {catalog_app_role};

GRANT CONNECT ON DATABASE {database} TO {migrator_role}, {app_role}, {admin_role}, {catalog_app_role};
GRANT CREATE ON DATABASE {database} TO {migrator_role};

GRANT {catalog_role} TO {catalog_app_role};
GRANT {catalog_role} TO {migrator_role};
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

fn sql_literal(value: &str) -> String {
    debug_assert!(
        value.chars().all(|c| c.is_ascii_alphanumeric()),
        "sql_literal received non-alphanumeric input; generated passwords must be alphanumeric"
    );
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use std::env;

    use secrecy::{ExposeSecret, SecretString};
    use sqlx::AssertSqlSafe;
    use url::Url;

    use super::{ROLE_BOOTSTRAP_SQL_TEMPLATE, role_bootstrap_sql};
    use crate::postgres_boot::EmbeddedRoleCredentials;
    use crate::{PoolConfig, pool::build_pool};

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
        assert!(!ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT wyrd_catalog"));
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
            "REVOKE wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog, wyrd_catalog_app",
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
        for source in [&embedded, EXTERNAL_ROLE_BOOTSTRAP] {
            let revoke = source
                .find("REVOKE wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog, wyrd_catalog_app")
                .expect("five-role revoke exists");
            let catalog_app_grant = source
                .find("GRANT wyrd_catalog TO wyrd_catalog_app")
                .expect("catalog application grant exists");
            let migrator_grant = source
                .find("GRANT wyrd_catalog TO wyrd_migrator")
                .expect("catalog migrator grant exists");
            assert!(revoke < catalog_app_grant);
            assert!(revoke < migrator_grant);
            assert_eq!(
                source
                    .matches("GRANT wyrd_catalog TO wyrd_catalog_app")
                    .count(),
                1
            );
            assert_eq!(
                source
                    .matches("GRANT wyrd_catalog TO wyrd_migrator")
                    .count(),
                1
            );
        }
    }

    /// The rendered embedded owner repairs every managed membership edge while
    /// retaining authority that belongs to an unrelated operator role.
    #[tokio::test]
    async fn embedded_bootstrap_converges_real_catalog() {
        let Some(admin_url) = env::var("WYRD_TEST_DATABASE_ADMIN_URL").ok() else {
            return;
        };
        let credentials = EmbeddedRoleCredentials {
            superuser: SecretString::from(password_from_url(&admin_url)),
            migrator: SecretString::from(password_from_env_url("DATABASE_URL")),
            app: SecretString::from(password_from_env_url("WYRD_DATABASE_URL")),
            platform_admin: SecretString::from(
                env::var("WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD")
                    .expect("platform administrator password is configured"),
            ),
            catalog_app: SecretString::from(
                env::var("WYRD_DATABASE_CATALOG_APP_PASSWORD")
                    .expect("catalog application password is configured"),
            ),
        };
        let admin = build_pool(&admin_url, PoolConfig::migrator_defaults())
            .await
            .expect("neutral administrator connects");
        sqlx::raw_sql(
            "CREATE ROLE wyrd_embedded_operator; \
             GRANT wyrd_embedded_operator TO wyrd_app; \
             GRANT CONNECT ON DATABASE wyrd TO wyrd_embedded_operator; \
             GRANT wyrd_app TO wyrd_catalog",
        )
        .execute(&admin)
        .await
        .expect("embedded drift seeds");
        sqlx::raw_sql(AssertSqlSafe(role_bootstrap_sql(&credentials)))
            .execute(&admin)
            .await
            .expect("embedded bootstrap repairs reverse drift");
        sqlx::raw_sql("GRANT wyrd_catalog TO wyrd_app, wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("omitted-direction drift seeds");
        sqlx::raw_sql(AssertSqlSafe(role_bootstrap_sql(&credentials)))
            .execute(&admin)
            .await
            .expect("embedded bootstrap repairs omitted-direction drift");

        let memberships: Vec<(String, String)> = sqlx::query_as(
            "SELECT granted.rolname, member.rolname FROM pg_auth_members edge \
             JOIN pg_roles granted ON granted.oid=edge.roleid \
             JOIN pg_roles member ON member.oid=edge.member \
             WHERE granted.rolname = ANY($1) AND member.rolname = ANY($1) \
             ORDER BY granted.rolname, member.rolname",
        )
        .bind([
            "wyrd_migrator",
            "wyrd_app",
            "wyrd_platform_admin",
            "wyrd_catalog",
            "wyrd_catalog_app",
        ])
        .fetch_all(&admin)
        .await
        .expect("managed memberships read");
        assert_eq!(
            memberships,
            vec![
                ("wyrd_catalog".to_owned(), "wyrd_catalog_app".to_owned()),
                ("wyrd_catalog".to_owned(), "wyrd_migrator".to_owned()),
            ]
        );
        let attributes: Vec<(String, bool, bool, bool, bool)> = sqlx::query_as(
            "SELECT rolname,rolcanlogin,rolbypassrls,rolsuper,rolcreatedb FROM pg_roles \
             WHERE rolname = ANY($1) ORDER BY rolname",
        )
        .bind([
            "wyrd_migrator",
            "wyrd_app",
            "wyrd_platform_admin",
            "wyrd_catalog",
            "wyrd_catalog_app",
        ])
        .fetch_all(&admin)
        .await
        .expect("managed role attributes read");
        assert_eq!(
            attributes,
            vec![
                ("wyrd_app".to_owned(), true, false, false, false),
                ("wyrd_catalog".to_owned(), false, false, false, false),
                ("wyrd_catalog_app".to_owned(), true, false, false, false),
                ("wyrd_migrator".to_owned(), true, true, false, false),
                ("wyrd_platform_admin".to_owned(), true, true, false, false),
            ]
        );
        let database_acl: Vec<(String, String)> = sqlx::query_as(
            "SELECT role.rolname, acl.privilege_type FROM pg_database database \
             CROSS JOIN LATERAL aclexplode(database.datacl) acl \
             JOIN pg_roles role ON role.oid=acl.grantee \
             WHERE database.datname='wyrd' AND role.rolname = ANY($1) \
             ORDER BY role.rolname, acl.privilege_type",
        )
        .bind([
            "wyrd_migrator",
            "wyrd_app",
            "wyrd_platform_admin",
            "wyrd_catalog",
            "wyrd_catalog_app",
        ])
        .fetch_all(&admin)
        .await
        .expect("managed database ACL reads");
        assert_eq!(
            database_acl,
            vec![
                ("wyrd_app".to_owned(), "CONNECT".to_owned()),
                ("wyrd_catalog_app".to_owned(), "CONNECT".to_owned()),
                ("wyrd_migrator".to_owned(), "CONNECT".to_owned()),
                ("wyrd_migrator".to_owned(), "CREATE".to_owned()),
                ("wyrd_platform_admin".to_owned(), "CONNECT".to_owned()),
            ]
        );
        for (role, password) in [
            ("wyrd_migrator", credentials.migrator.expose_secret()),
            ("wyrd_app", credentials.app.expose_secret()),
            (
                "wyrd_platform_admin",
                credentials.platform_admin.expose_secret(),
            ),
            ("wyrd_catalog_app", credentials.catalog_app.expose_secret()),
        ] {
            let role_pool = build_pool(
                &role_url(&admin_url, role, password),
                PoolConfig::migrator_defaults(),
            )
            .await
            .expect("managed login role connects with converged password");
            let current_role: String = sqlx::query_scalar("SELECT current_user")
                .fetch_one(&role_pool)
                .await
                .expect("managed login identity reads");
            assert_eq!(current_role, role);
            role_pool.close().await;
        }
        assert!(
            build_pool(
                &role_url(&admin_url, "wyrd_catalog", "cannot-login"),
                PoolConfig::migrator_defaults(),
            )
            .await
            .is_err(),
            "the catalog owner remains a no-login role"
        );
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT pg_has_role('wyrd_app','wyrd_embedded_operator','member')",
            )
            .fetch_one(&admin)
            .await
            .expect("unrelated membership reads")
        );
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT has_database_privilege('wyrd_embedded_operator','wyrd','CONNECT')",
            )
            .fetch_one(&admin)
            .await
            .expect("unrelated database ACL reads")
        );

        sqlx::raw_sql(
            "REVOKE wyrd_embedded_operator FROM wyrd_app; \
             REVOKE ALL PRIVILEGES ON DATABASE wyrd FROM wyrd_embedded_operator; \
             DROP ROLE wyrd_embedded_operator",
        )
        .execute(&admin)
        .await
        .expect("embedded drift fixture cleans up");
        admin.close().await;
    }

    /// Extracts the password from a test-only Postgres URL.
    fn password_from_url(value: &str) -> String {
        Url::parse(value)
            .expect("test Postgres URL parses")
            .password()
            .expect("test Postgres URL carries a password")
            .to_owned()
    }

    /// Extracts a password from one required test-only Postgres URL variable.
    fn password_from_env_url(name: &str) -> String {
        password_from_url(&env::var(name).expect("test Postgres URL is configured"))
    }

    /// Rewrites the neutral administrator URL for one managed login role.
    fn role_url(admin_url: &str, role: &str, password: &str) -> String {
        let mut url = Url::parse(admin_url).expect("test Postgres URL parses");
        url.set_username(role)
            .expect("managed role is a valid URL username");
        url.set_password(Some(password))
            .expect("generated password is valid URL userinfo");
        url.to_string()
    }
}
