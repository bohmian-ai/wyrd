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

fn sql_literal(value: &str) -> String {
    debug_assert!(
        value.chars().all(|c| c.is_ascii_alphanumeric()),
        "sql_literal received non-alphanumeric input; generated passwords must be alphanumeric"
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
}
