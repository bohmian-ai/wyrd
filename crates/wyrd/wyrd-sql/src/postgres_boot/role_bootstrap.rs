//! Embedded Postgres role bootstrap SQL.

use secrecy::ExposeSecret;

use super::EmbeddedRoleCredentials;

pub(crate) const WYRD_DATABASE: &str = "wyrd";
pub(crate) const WYRD_MIGRATOR_ROLE: &str = "wyrd_migrator";
pub(crate) const WYRD_APP_ROLE: &str = "wyrd_app";
pub(crate) const WYRD_PLATFORM_ADMIN_ROLE: &str = "wyrd_platform_admin";

pub(crate) const ROLE_BOOTSTRAP_SQL_TEMPLATE: &str = r#"
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_migrator') THEN
        CREATE ROLE wyrd_migrator LOGIN BYPASSRLS PASSWORD '<migrator_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_app') THEN
        CREATE ROLE wyrd_app LOGIN PASSWORD '<app_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
        CREATE ROLE wyrd_platform_admin LOGIN BYPASSRLS PASSWORD '<admin_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog') THEN
        CREATE ROLE wyrd_catalog NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog_app') THEN
        CREATE ROLE wyrd_catalog_app LOGIN PASSWORD '<catalog_app_pw>';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery_owner') THEN
        CREATE ROLE vala_recovery_owner NOLOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery') THEN
        CREATE ROLE vala_recovery LOGIN PASSWORD '<recovery_pw>';
    END IF;
END $$;
GRANT wyrd_catalog TO wyrd_catalog_app;
GRANT wyrd_catalog TO wyrd_migrator;
GRANT vala_recovery_owner TO wyrd_migrator;
"#;

pub(crate) const WYRD_CATALOG_APP_ROLE: &str = "wyrd_catalog_app";
pub(crate) const VALA_RECOVERY_ROLE: &str = "vala_recovery";

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
        )
        .replace(
            "'<recovery_pw>'",
            &sql_literal(credentials.recovery.expose_secret()),
        );

    format!(
        r#"
{sql}

ALTER ROLE {migrator_role} WITH LOGIN BYPASSRLS PASSWORD {migrator_pw};
ALTER ROLE {app_role} WITH LOGIN NOBYPASSRLS PASSWORD {app_pw};
ALTER ROLE {admin_role} WITH LOGIN BYPASSRLS PASSWORD {admin_pw};
ALTER ROLE {catalog_app_role} WITH LOGIN PASSWORD {catalog_app_pw};
ALTER ROLE {recovery_role} WITH LOGIN PASSWORD {recovery_pw};

GRANT CONNECT ON DATABASE {database} TO {migrator_role}, {app_role}, {admin_role}, {catalog_app_role}, {recovery_role};
GRANT CREATE ON DATABASE {database} TO {migrator_role};
"#,
        sql = sql,
        database = WYRD_DATABASE,
        migrator_role = WYRD_MIGRATOR_ROLE,
        app_role = WYRD_APP_ROLE,
        admin_role = WYRD_PLATFORM_ADMIN_ROLE,
        catalog_app_role = WYRD_CATALOG_APP_ROLE,
        recovery_role = VALA_RECOVERY_ROLE,
        migrator_pw = sql_literal(credentials.migrator.expose_secret()),
        app_pw = sql_literal(credentials.app.expose_secret()),
        admin_pw = sql_literal(credentials.platform_admin.expose_secret()),
        catalog_app_pw = sql_literal(credentials.catalog_app.expose_secret()),
        recovery_pw = sql_literal(credentials.recovery.expose_secret()),
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

    #[test]
    fn bootstrap_template_is_the_only_create_role_shape() {
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE wyrd_migrator LOGIN BYPASSRLS"));
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE wyrd_app LOGIN PASSWORD"));
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE wyrd_platform_admin LOGIN BYPASSRLS")
        );
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE wyrd_catalog NOLOGIN"));
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE wyrd_catalog_app LOGIN PASSWORD")
        );
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE
                .contains("CREATE ROLE vala_recovery_owner NOLOGIN BYPASSRLS")
        );
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("CREATE ROLE vala_recovery LOGIN PASSWORD")
        );
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT wyrd_catalog TO wyrd_catalog_app"));
        assert!(ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT wyrd_catalog TO wyrd_migrator"));
        assert!(
            ROLE_BOOTSTRAP_SQL_TEMPLATE.contains("GRANT vala_recovery_owner TO wyrd_migrator")
        );
    }

    #[test]
    fn bootstrap_sql_quotes_password_literals() {
        let credentials = EmbeddedRoleCredentials {
            superuser: SecretString::from("supersecret123".to_owned()),
            migrator: SecretString::from("migpw456".to_owned()),
            app: SecretString::from("apppw789".to_owned()),
            platform_admin: SecretString::from("adminpwABC".to_owned()),
            catalog_app: SecretString::from("catalogpwDEF".to_owned()),
            recovery: SecretString::from("recoverypwGHI".to_owned()),
        };

        let sql = role_bootstrap_sql(&credentials);

        assert!(sql.contains("PASSWORD 'migpw456'"));
        assert!(sql.contains("ALTER ROLE wyrd_migrator WITH LOGIN BYPASSRLS PASSWORD"));
        assert!(sql.contains("ALTER ROLE wyrd_app WITH LOGIN NOBYPASSRLS PASSWORD"));
        assert!(sql.contains("ALTER ROLE wyrd_platform_admin WITH LOGIN BYPASSRLS PASSWORD"));
        assert!(sql.contains("ALTER ROLE wyrd_catalog_app WITH LOGIN PASSWORD"));
        assert!(sql.contains("ALTER ROLE vala_recovery WITH LOGIN PASSWORD"));
        assert!(sql.contains(
            "GRANT CONNECT ON DATABASE wyrd TO wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog_app, vala_recovery"
        ));
        assert!(sql.contains("GRANT CREATE ON DATABASE wyrd TO wyrd_migrator"));
    }
}
