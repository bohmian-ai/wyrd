//! Read-only serving readiness checks over a migrated database.
//!
//! Serving Wyrd never runs DDL. Before a serving process reports ready, and
//! again after the one-off owner migration, each schema owner (`wyrd-sql`,
//! `vala-sql`) proves through these [`OperatorPool`] checks that its embedded
//! migrations are applied unchanged, that its tenant tables are isolated by the
//! policies its migrations create, and that the two serving roles carry exactly
//! the authority the tenant model assumes. Every check reads the system
//! catalogs by role name, so it proves the same contract whichever login runs
//! it; which login each *serving pool* actually uses is checked by the serving
//! owners themselves.

use sqlx::migrate::Migrator;

use crate::dsn::{WYRD_APP_ROLE, WYRD_PLATFORM_ADMIN_ROLE};
use crate::{OperatorPool, SqlError};

/// Name of the policy every tenant-keyed table uses to confine the app role.
const TENANT_ISOLATION_POLICY: &str = "tenant_isolation";
/// The deparsed `USING` and `WITH CHECK` expression that policy must carry.
///
/// Readiness compares `pg_get_expr` output to this exactly, so a policy keyed
/// on any other column (for example the row's own `id`) is refused.
const TENANT_ISOLATION_EXPR: &str = "(data_tenant_id = wyrd.current_tenant())";

/// Schema-level authority one serving role must hold on an owned schema.
///
/// The closed set of postures Wyrd and Vala migrations establish. Any other
/// privilege a readiness check observes is drift that widens a serving role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaAccess {
    /// No usage at all; the role must not even resolve the schema's objects.
    None,
    /// `USAGE` without `CREATE`: the role uses objects the owner created.
    Usage,
    /// `USAGE` and `CREATE`: the role owns DDL inside this one schema.
    UsageCreate,
}

impl SchemaAccess {
    /// Whether this posture includes schema `USAGE`.
    fn usage(self) -> bool {
        self != Self::None
    }

    /// Whether this posture includes schema `CREATE`.
    fn create(self) -> bool {
        self == Self::UsageCreate
    }
}

impl OperatorPool {
    /// Prove every migration embedded in `migrator` is applied with its
    /// embedded checksum in the ledger table `ledger` (schema-qualified).
    ///
    /// Applied versions newer than the binary are tolerated so a migrated
    /// schema can serve an older, compatible binary during a rollout.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] when the ledger is unreadable or a
    /// migration is missing or failed, and [`SqlError::MigrateChecksum`] when
    /// an applied checksum differs.
    pub async fn verify_migrations(
        &self,
        ledger: &str,
        migrator: &Migrator,
    ) -> Result<(), SqlError> {
        let applied: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT version, checksum, success FROM {ledger}"
        )))
        .fetch_all(self.pool())
        .await
        .map_err(|error| SqlError::SchemaNotReady {
            detail: format!("migration ledger {ledger} is unreadable: {error}"),
        })?;
        for migration in migrator.iter() {
            let Some((_, checksum, success)) = applied
                .iter()
                .find(|(version, _, _)| *version == migration.version)
            else {
                return Err(SqlError::SchemaNotReady {
                    detail: format!(
                        "migration {} ({}) is not applied",
                        migration.version, migration.description
                    ),
                });
            };
            if !success {
                return Err(SqlError::SchemaNotReady {
                    detail: format!("migration {} did not complete", migration.version),
                });
            }
            if checksum.as_slice() != &*migration.checksum {
                return Err(SqlError::MigrateChecksum {
                    version: migration.version,
                    detail: "applied checksum differs from this binary".to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Prove the operator pool itself logs in as exactly `wyrd_platform_admin`.
    ///
    /// Both the session login and the effective role must match, so neither a
    /// differently named BYPASSRLS login nor a `SET ROLE` in the DSN passes.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the observed role when it
    /// differs, and [`SqlError::Connect`] on query failure.
    pub async fn verify_platform_login(&self) -> Result<(), SqlError> {
        let (session, current): (String, String) =
            sqlx::query_as("SELECT session_user::text, current_user::text")
                .fetch_one(self.pool())
                .await
                .map_err(SqlError::Connect)?;
        verify_login_name(WYRD_PLATFORM_ADMIN_ROLE, &session, &current)
    }

    /// Prove both serving roles carry exactly their approved role attributes.
    ///
    /// Each role must exist and log in; neither may be a superuser, create
    /// roles or databases, replicate, create objects in the database, or be a
    /// member of any other role (membership would inherit grants outside the
    /// migrations' narrow ones). `wyrd_app` must not bypass row-level security;
    /// `wyrd_platform_admin` must.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the first non-conforming
    /// role, and [`SqlError::Connect`] on query failure.
    pub async fn verify_serving_roles(&self) -> Result<(), SqlError> {
        for (role, bypass_rls) in [(WYRD_APP_ROLE, false), (WYRD_PLATFORM_ADMIN_ROLE, true)] {
            let conforms: Option<bool> = sqlx::query_scalar(
                "SELECT r.rolcanlogin AND NOT r.rolsuper AND NOT r.rolcreaterole \
                   AND NOT r.rolcreatedb AND NOT r.rolreplication AND r.rolbypassrls = $2 \
                   AND NOT has_database_privilege(r.oid, current_database(), 'CREATE') \
                   AND NOT EXISTS (SELECT 1 FROM pg_auth_members m WHERE m.member = r.oid) \
                 FROM pg_roles r WHERE r.rolname = $1",
            )
            .bind(role)
            .bind(bypass_rls)
            .fetch_optional(self.pool())
            .await
            .map_err(SqlError::Connect)?;
            if conforms != Some(true) {
                return Err(SqlError::SchemaNotReady {
                    detail: format!(
                        "serving role {role} must exist as a login with BYPASSRLS={bypass_rls}, \
                         no superuser, role, database, or replication authority, and no role \
                         memberships"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Prove the serving roles hold exactly `app` and `platform` on `schema`.
    ///
    /// Beyond the schema posture, neither serving role may own a relation in
    /// the schema (ownership confers `ALTER TABLE`, including disabling
    /// row-level security), nor hold `TRUNCATE`,
    /// `REFERENCES`, or `TRIGGER` on tables it does not own — `TRUNCATE` in
    /// particular ignores row-level security. A schema whose posture grants
    /// `CREATE` exists precisely so that role owns its objects, so ownership
    /// (and the table rights it implies) is only refused where it is not granted.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the role and the widened
    /// privilege, and [`SqlError::Connect`] on query failure.
    pub async fn verify_schema_privileges(
        &self,
        schema: &str,
        app: SchemaAccess,
        platform: SchemaAccess,
    ) -> Result<(), SqlError> {
        for (role, access) in [(WYRD_APP_ROLE, app), (WYRD_PLATFORM_ADMIN_ROLE, platform)] {
            let (usage, create, owns, table_rights): (bool, bool, bool, bool) = sqlx::query_as(
                "SELECT has_schema_privilege($1, n.oid, 'USAGE'), \
                        has_schema_privilege($1, n.oid, 'CREATE'), \
                        EXISTS (SELECT 1 FROM pg_class c WHERE c.relnamespace = n.oid \
                                  AND c.relowner = r.oid), \
                        EXISTS (SELECT 1 FROM pg_class c WHERE c.relnamespace = n.oid \
                                  AND c.relkind IN ('r', 'p') AND c.relowner <> r.oid \
                                  AND has_table_privilege(r.oid, c.oid, \
                                        'TRUNCATE, REFERENCES, TRIGGER')) \
                 FROM pg_namespace n, pg_roles r WHERE n.nspname = $2 AND r.rolname = $1",
            )
            .bind(role)
            .bind(schema)
            .fetch_optional(self.pool())
            .await
            .map_err(SqlError::Connect)?
            .ok_or_else(|| SqlError::SchemaNotReady {
                detail: format!("schema {schema} or serving role {role} does not exist"),
            })?;
            let widened = if usage != access.usage() || create != access.create() {
                Some(format!("schema privileges other than {access:?}"))
            } else if owns && !access.create() {
                Some("ownership of schema relations".to_owned())
            } else if table_rights {
                Some("TRUNCATE, REFERENCES, or TRIGGER on a table".to_owned())
            } else {
                None
            };
            if let Some(widened) = widened {
                return Err(SqlError::SchemaNotReady {
                    detail: format!("serving role {role} holds {widened} in schema {schema}"),
                });
            }
        }
        Ok(())
    }

    /// Prove every tenant-keyed table in `schema` is isolated by its policies.
    ///
    /// Each table carrying `data_tenant_id` must enable and force row-level
    /// security and carry the permissive, all-command `tenant_isolation` policy
    /// for `PUBLIC` whose `USING` and `WITH CHECK` are both exactly
    /// [`TENANT_ISOLATION_EXPR`], so the policy keys on `data_tenant_id` and
    /// not on another column of the same type. Any other permissive policy must apply
    /// only to `wyrd_platform_admin`, because permissive policies are OR-ed and
    /// one more for the app role would widen what a tenant session sees.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the first unprotected table,
    /// and [`SqlError::Connect`] on query failure.
    pub async fn verify_tenant_isolation(&self, schema: &str) -> Result<(), SqlError> {
        let unprotected: Option<String> = sqlx::query_scalar(
            "SELECT c.relname::text FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'data_tenant_id' \
             WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition \
               AND NOT (c.relrowsecurity AND c.relforcerowsecurity \
                 AND EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid \
                   AND p.polname = $2 AND p.polpermissive AND p.polcmd = '*' \
                   AND p.polroles = '{0}'::oid[] \
                   AND pg_get_expr(p.polqual, p.polrelid) = $4 \
                   AND pg_get_expr(p.polwithcheck, p.polrelid) = $4) \
                 AND NOT EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid \
                   AND p.polpermissive AND p.polname <> $2 \
                   AND p.polroles <> ARRAY[(SELECT oid FROM pg_roles WHERE rolname = $3)]))\
             ORDER BY c.relname LIMIT 1",
        )
        .bind(schema)
        .bind(TENANT_ISOLATION_POLICY)
        .bind(WYRD_PLATFORM_ADMIN_ROLE)
        .bind(TENANT_ISOLATION_EXPR)
        .fetch_optional(self.pool())
        .await
        .map_err(SqlError::Connect)?;
        match unprotected {
            Some(table) => Err(SqlError::SchemaNotReady {
                detail: format!(
                    "{schema}.{table} does not force row-level security under its \
                     {TENANT_ISOLATION_POLICY} policy alone"
                ),
            }),
            None => Ok(()),
        }
    }
}

/// Require a serving pool's session and effective role to be `expected`.
///
/// Shared by both serving owners so the app and platform pools refuse a
/// substituted login the same way.
///
/// # Errors
/// Returns [`SqlError::SchemaNotReady`] naming both observed roles when either
/// differs from `expected`.
pub fn verify_login_name(expected: &str, session: &str, current: &str) -> Result<(), SqlError> {
    if session == expected && current == expected {
        return Ok(());
    }
    Err(SqlError::SchemaNotReady {
        detail: format!(
            "serving login must be {expected}, found session role {session} acting as {current}"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::verify_login_name;

    /// Only an exact session and effective role match is a valid serving login.
    #[test]
    fn login_name_must_match_session_and_effective_role() {
        assert!(verify_login_name("wyrd_app", "wyrd_app", "wyrd_app").is_ok());
        assert!(verify_login_name("wyrd_app", "wyrd_owner", "wyrd_app").is_err());
        assert!(verify_login_name("wyrd_app", "wyrd_app", "wyrd_owner").is_err());
    }
}
