//! Read-only serving readiness checks over a migrated database.
//!
//! Serving Wyrd never runs DDL. Before a serving process reports ready, and
//! again after the one-off migration, each schema owner (`wyrd-sql`,
//! `vala-sql`) proves through these checks that its embedded migrations are
//! applied unchanged and that every table it owns forces the row-level
//! security its migrations create. Serving boot also proves each pool's login
//! can keep that security meaningful: the tenant login is policy-bound, and the
//! platform login is the operator session that owns Wyrd's objects. Wyrd names
//! no role; every check reads properties of the connected login and the
//! catalog.

use sqlx::PgPool;
use sqlx::migrate::Migrator;

use crate::dsn::OPERATOR_SETTING;
use crate::{OperatorPool, SqlError};

/// Name of the policy every tenant-keyed table uses to confine tenant sessions.
const TENANT_ISOLATION_POLICY: &str = "tenant_isolation";
/// The deparsed `USING` and `WITH CHECK` expression that policy must carry.
///
/// Readiness compares `pg_get_expr` output to this exactly, so a policy keyed
/// on any other column (for example the row's own `id`) is refused.
const TENANT_ISOLATION_EXPR: &str = "(data_tenant_id = wyrd.current_tenant())";
/// Name of the policy every table uses to admit operator sessions.
const OPERATOR_ACCESS_POLICY: &str = "operator_access";
/// The deparsed `USING` and `WITH CHECK` expression that policy must carry.
const OPERATOR_ACCESS_EXPR: &str = "wyrd.operator_session()";

/// Whether an owned schema's tenant-keyed tables are tenant-visible.
///
/// The closed set of schema postures Wyrd and Vala migrations establish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowScope {
    /// Tables carrying `data_tenant_id` admit their tenant through
    /// `tenant_isolation` and operators through `operator_access`; other
    /// tables admit operators only.
    Tenant,
    /// Platform-plane schema: every table admits operators only, whatever its
    /// columns, so no tenant session reads the tenant directory.
    Platform,
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

    /// Prove this pool is an operator session on the login that owns Wyrd's
    /// objects.
    ///
    /// `operator_access` policies apply only to the owner role and admit only
    /// sessions with the operator flag, so a platform login that does not own
    /// the tables in `schemas`, or a session without the flag, would see no
    /// rows. A superuser is refused: Postgres exempts it from every policy, so
    /// it proves nothing about the deployment's posture.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the login and the failed
    /// requirement, and [`SqlError::Connect`] on query failure.
    pub async fn verify_operator_session(&self, schemas: &[&str]) -> Result<(), SqlError> {
        let (login, superuser, flagged, foreign): (String, bool, bool, Option<String>) =
            sqlx::query_as(
                "SELECT current_user::text, r.rolsuper, \
                    coalesce(current_setting($2, true), '') = 'on', \
                    (SELECT n.nspname || '.' || c.relname FROM pg_class c \
                     JOIN pg_namespace n ON n.oid = c.relnamespace \
                     WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p') \
                       AND NOT pg_has_role(current_user, c.relowner, 'MEMBER') \
                     ORDER BY 1 LIMIT 1) \
                 FROM pg_roles r WHERE r.rolname = current_user",
            )
            .bind(schemas)
            .bind(OPERATOR_SETTING)
            .fetch_one(self.pool())
            .await
            .map_err(SqlError::Connect)?;
        let failure = if superuser {
            Some("is a superuser, which row-level security cannot scope".to_owned())
        } else if !flagged {
            Some("did not connect as an operator session".to_owned())
        } else {
            foreign.map(|table| {
                format!("does not own {table}; run wyrd-server migrate as this platform login")
            })
        };
        match failure {
            Some(failure) => Err(SqlError::SchemaNotReady {
                detail: format!("platform login {login} {failure}"),
            }),
            None => Ok(()),
        }
    }

    /// Prove every table in `schema` forces the row-level security its
    /// migrations create.
    ///
    /// Each table, the SQLx ledger included, must enable and force row-level
    /// security and carry the permissive, all-command `operator_access` policy
    /// for exactly its owner, whose `USING` and `WITH CHECK` are both
    /// [`OPERATOR_ACCESS_EXPR`]. Under [`RowScope::Tenant`], a table carrying
    /// `data_tenant_id` must also carry the permissive, all-command
    /// `tenant_isolation` policy for `PUBLIC` whose expressions are both
    /// exactly [`TENANT_ISOLATION_EXPR`]. No other permissive policy may exist,
    /// because permissive policies are OR-ed and one more would widen what a
    /// session sees.
    ///
    /// The check runs in its own transaction with `search_path` pinned to
    /// `pg_catalog`: `pg_get_expr` omits the qualifier of any schema on the
    /// search path, and the default `"$user"` entry puts `wyrd` there for a
    /// login named `wyrd`.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the first unprotected table,
    /// and [`SqlError::Connect`] on query failure.
    pub async fn verify_row_security(&self, schema: &str, scope: RowScope) -> Result<(), SqlError> {
        let mut tx = self.pool().begin().await.map_err(SqlError::Connect)?;
        sqlx::query("SET LOCAL search_path = pg_catalog")
            .execute(&mut *tx)
            .await
            .map_err(SqlError::Connect)?;
        let unprotected: Option<String> = sqlx::query_scalar(
            "WITH t AS ( \
                SELECT c.oid, c.relname, c.relowner, c.relrowsecurity, c.relforcerowsecurity, \
                       $2 AND EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid \
                                        AND a.attname = 'data_tenant_id' AND NOT a.attisdropped) \
                         AS tenant_keyed \
                FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition) \
             SELECT t.relname::text FROM t \
             WHERE NOT (t.relrowsecurity AND t.relforcerowsecurity \
                 AND EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = t.oid \
                   AND p.polname = $5 AND p.polpermissive AND p.polcmd = '*' \
                   AND p.polroles = ARRAY[t.relowner] \
                   AND pg_get_expr(p.polqual, p.polrelid) = $6 \
                   AND pg_get_expr(p.polwithcheck, p.polrelid) = $6) \
                 AND (NOT t.tenant_keyed OR EXISTS (SELECT 1 FROM pg_policy p \
                   WHERE p.polrelid = t.oid \
                   AND p.polname = $3 AND p.polpermissive AND p.polcmd = '*' \
                   AND p.polroles = '{0}'::oid[] \
                   AND pg_get_expr(p.polqual, p.polrelid) = $4 \
                   AND pg_get_expr(p.polwithcheck, p.polrelid) = $4)) \
                 AND NOT EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = t.oid \
                   AND p.polpermissive AND p.polname <> $5 \
                   AND (p.polname <> $3 OR NOT t.tenant_keyed))) \
             ORDER BY t.relname LIMIT 1",
        )
        .bind(schema)
        .bind(scope == RowScope::Tenant)
        .bind(TENANT_ISOLATION_POLICY)
        .bind(TENANT_ISOLATION_EXPR)
        .bind(OPERATOR_ACCESS_POLICY)
        .bind(OPERATOR_ACCESS_EXPR)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqlError::Connect)?;
        tx.commit().await.map_err(SqlError::Connect)?;
        match unprotected {
            Some(table) => Err(SqlError::SchemaNotReady {
                detail: format!(
                    "{schema}.{table} does not force row-level security under exactly its \
                     {OPERATOR_ACCESS_POLICY} and {TENANT_ISOLATION_POLICY} policies"
                ),
            }),
            None => Ok(()),
        }
    }
}

/// Prove a tenant pool's login stays bound by row-level security.
///
/// The login must not be a superuser or hold `BYPASSRLS`, which Postgres
/// exempts from every policy, and must not connect as an operator session.
/// When `separate_login` is set (a distinct platform login exists), it also
/// must not own, or be a member of the owner of, any table in `schemas`, so
/// setting the operator flag gains it nothing; must not hold `TRUNCATE`,
/// `REFERENCES`, or `TRIGGER` on them, since `TRUNCATE` in particular ignores
/// row-level security; and must hold no privilege on a table there that
/// row-level security does not protect, such as the cross-tenant Iceberg
/// catalog. A login shared with the platform pool is the owner by design and
/// is scoped by the pool instead.
///
/// # Errors
/// Returns [`SqlError::SchemaNotReady`] naming the login and the failed
/// requirement, and [`SqlError::Connect`] on query failure.
pub async fn verify_tenant_pool(
    pool: &PgPool,
    separate_login: bool,
    schemas: &[&str],
) -> Result<(), SqlError> {
    let (login, exempt, flagged, owned, widened, unprotected): (
        String,
        bool,
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT current_user::text, r.rolsuper OR r.rolbypassrls, \
            coalesce(current_setting($2, true), '') = 'on', \
            (SELECT n.nspname || '.' || c.relname FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p') \
               AND pg_has_role(current_user, c.relowner, 'MEMBER') ORDER BY 1 LIMIT 1), \
            (SELECT n.nspname || '.' || c.relname FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p') \
               AND has_table_privilege(current_user, c.oid, 'TRUNCATE, REFERENCES, TRIGGER') \
             ORDER BY 1 LIMIT 1), \
            (SELECT n.nspname || '.' || c.relname FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p') AND NOT c.relrowsecurity \
               AND has_table_privilege(current_user, c.oid, 'SELECT, INSERT, UPDATE, DELETE') \
             ORDER BY 1 LIMIT 1) \
         FROM pg_roles r WHERE r.rolname = current_user",
    )
    .bind(schemas)
    .bind(OPERATOR_SETTING)
    .fetch_one(pool)
    .await
    .map_err(SqlError::Connect)?;
    let failure = if exempt {
        Some(
            "is a superuser or has BYPASSRLS, which row-level security cannot scope; \
             use an ordinary login"
                .to_owned(),
        )
    } else if flagged {
        Some("connected as an operator session".to_owned())
    } else if !separate_login {
        None
    } else if let Some(table) = owned {
        Some(format!(
            "owns {table}; with a separate WYRD_PLATFORM_DATABASE_URL the tenant login must \
             not own Wyrd's objects"
        ))
    } else if let Some(table) = widened {
        Some(format!("holds TRUNCATE, REFERENCES, or TRIGGER on {table}"))
    } else {
        unprotected.map(|table| {
            format!("holds privileges on {table}, which row-level security does not protect")
        })
    };
    match failure {
        Some(failure) => Err(SqlError::SchemaNotReady {
            detail: format!("tenant login {login} {failure}"),
        }),
        None => Ok(()),
    }
}
