//! Server-tier SQL scaffold for Wyrd control-plane storage.
//!
//! `wyrd-sql` owns the `platform` and `wyrd` PostgreSQL schemas. `platform`
//! holds state above the tenant boundary, including the future tenant catalog.
//! `wyrd` holds tenant-scoped control-plane state. Vala owns its own `vala`
//! schema in `vala-sql`; no `skald` schema exists in this phase.

#![deny(missing_docs)]

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::PgConnection;
use sqlx::{AssertSqlSafe, Connection as _};

pub mod dsn;
pub mod error;
pub mod operator_pool;
pub mod pool;
pub mod postgres;
pub mod queries;
pub mod query;
pub mod row_types;
pub mod schema_check;
pub mod tenant_conn;

pub use error::SqlError;
pub use operator_pool::OperatorPool;
pub use pool::PoolConfig;
pub use postgres::WyrdPostgres;
pub use row_types::cards::{CardRow, CardStatus, ParsedCardRow};
pub use schema_check::SchemaAccess;
pub use tenant_conn::TenantConn;

/// Platform-global schema owned by `wyrd-sql`.
pub const PLATFORM_SCHEMA: &str = "platform";
/// Tenant-scoped Wyrd control-plane schema owned by `wyrd-sql`.
pub const CONTROL_SCHEMA: &str = "wyrd";
/// Schemas whose migration lifecycle is owned by `wyrd-sql`.
pub const OWNED_SCHEMAS: &[&str] = &[PLATFORM_SCHEMA, CONTROL_SCHEMA];
/// Search path used only by the one-off migration connection.
pub const MIGRATION_SEARCH_PATH: &str = "wyrd, platform, public";
/// Schema-qualified ledger in which SQLx records applied Wyrd migrations.
pub const MIGRATION_LEDGER: &str = "wyrd._sqlx_migrations";
/// Wyrd migrations embedded in this binary.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Longest a one-off migration waits for another migration's lease.
///
/// Two release slots can start `wyrd-server migrate` at once; the loser waits
/// this long for the winner, then fails instead of queueing indefinitely. A
/// retry after the winner finishes or fails acquires the lease normally.
pub const MIGRATION_LEASE_WAIT: Duration = Duration::from_secs(60);

/// Database-wide advisory key serializing every Wyrd and Vala migration.
const MIGRATION_LEASE_KEY: i64 = 0x0057_5952_4453_514c;

/// One owner session holding the database-wide migration lease.
///
/// The one-off migration process acquires this once and runs the ordered Wyrd
/// migration, the Vala migration, and post-migration validation while holding
/// it, so no two migrators can interleave stages. The lease is a session-level
/// advisory lock on one dedicated physical connection: [`Self::release`]
/// unlocks it and closes the connection, and dropping the lease without
/// releasing closes the connection too, which ends the session and so releases
/// the lock — a crashed or cancelled migrator never strands it.
pub struct MigrationLease {
    /// Dedicated owner session detached from its pool, holding the lock.
    session: PgConnection,
}

impl MigrationLease {
    /// Apply one crate's embedded migrations under this lease.
    ///
    /// Creates the crate's owned `schemas` if absent, sets the session
    /// `search_path` the crate's migrations expect, and runs `migrator` on the
    /// lease's own session. Each schema owner calls this through its own
    /// `migrate` function; Wyrd must run before Vala, whose migrations depend
    /// on Wyrd's schemas.
    ///
    /// # Errors
    /// Returns [`SqlError::InsufficientPrivilege`] when the login lacks the
    /// bootstrap DDL privileges, [`SqlError::Connect`] when another bootstrap
    /// statement fails, and [`SqlError::Migrate`] when a migration fails.
    pub async fn apply(
        &mut self,
        schemas: &[&str],
        search_path: &str,
        migrator: &Migrator,
    ) -> Result<(), SqlError> {
        for schema in schemas {
            sqlx::query(AssertSqlSafe(format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}"
            )))
            .execute(&mut self.session)
            .await
            .map_err(classify_bootstrap_error)?;
        }
        sqlx::query(AssertSqlSafe(format!("SET search_path TO {search_path}")))
            .execute(&mut self.session)
            .await
            .map_err(classify_bootstrap_error)?;
        migrator
            .run(&mut self.session)
            .await
            .map_err(SqlError::from)
    }

    /// Unlock the lease and close its dedicated session.
    ///
    /// A failed unlock still closes the session, which releases the lock; the
    /// failure is returned so the caller does not report a clean release.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the unlock statement fails.
    pub async fn release(mut self) -> Result<(), SqlError> {
        let unlocked = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(MIGRATION_LEASE_KEY)
            .execute(&mut self.session)
            .await
            .map(|_| ())
            .map_err(SqlError::Connect);
        if let Err(error) = self.session.close().await {
            tracing::warn!(error = %error, "failed to close the migration lease session cleanly");
        }
        unlocked
    }
}

/// Apply embedded Wyrd SQL migrations under the migration lease.
///
/// Creates the `platform` and `wyrd` schemas and runs every Wyrd migration
/// with `search_path` set to `wyrd, platform, public`. Calling with a lease
/// acquired from a serving `wyrd_app` DSN fails on the bootstrap DDL and
/// returns [`SqlError::InsufficientPrivilege`].
///
/// # Errors
/// Returns [`SqlError::InsufficientPrivilege`] when the role lacks DDL
/// privileges, [`SqlError::Connect`] when a bootstrap statement fails, and
/// [`SqlError::Migrate`] when migration execution fails.
pub async fn migrate(lease: &mut MigrationLease) -> Result<(), SqlError> {
    lease
        .apply(OWNED_SCHEMAS, MIGRATION_SEARCH_PATH, &MIGRATOR)
        .await
}

/// Prove the Wyrd schema contract holds for serving.
///
/// Checks both serving roles' attributes, every embedded Wyrd migration and
/// checksum, both roles' privileges on the `platform` and `wyrd` schemas, and
/// tenant isolation in `wyrd`. The one-off migration runs this after migrating
/// and serving boot runs it before reporting ready, so both enforce the same
/// contract. Read-only.
///
/// # Errors
/// Returns [`SqlError::SchemaNotReady`] for the first failed check,
/// [`SqlError::MigrateChecksum`] for checksum drift, and [`SqlError::Connect`]
/// on query failure.
pub async fn verify_schema(operator: &OperatorPool) -> Result<(), SqlError> {
    operator.verify_serving_roles().await?;
    operator
        .verify_migrations(MIGRATION_LEDGER, &MIGRATOR)
        .await?;
    for schema in OWNED_SCHEMAS {
        operator
            .verify_schema_privileges(schema, SchemaAccess::Usage, SchemaAccess::Usage)
            .await?;
    }
    operator.verify_tenant_isolation(CONTROL_SCHEMA).await
}

fn classify_bootstrap_error(error: sqlx::Error) -> SqlError {
    if let sqlx::Error::Database(ref dbe) = error
        && dbe.code().as_deref() == Some("42501")
    {
        return SqlError::InsufficientPrivilege {
            detail: dbe.message().to_owned(),
        };
    }
    SqlError::Connect(error)
}

impl OperatorPool {
    /// Acquire the migration lease on a dedicated owner connection.
    ///
    /// Only the one-off `wyrd-server migrate` calls this, on an operator pool
    /// it builds from the database-owner DSN; serving pools never hold that
    /// credential. The connection is detached from the pool so its session
    /// state, and the lock, never return to it. The wait is bounded by `wait`
    /// through the session's `lock_timeout`, which is reset once the lock is
    /// held so migrations run with the server default.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when another migrator still holds the
    /// lease after `wait`, and [`SqlError::Connect`] when the connection or a
    /// session statement fails.
    pub async fn migration_lease(&self, wait: Duration) -> Result<MigrationLease, SqlError> {
        let mut session = self
            .pool()
            .acquire()
            .await
            .map_err(SqlError::Connect)?
            .detach();
        sqlx::query(AssertSqlSafe(format!(
            "SET lock_timeout = '{}ms'",
            wait.as_millis().max(1)
        )))
        .execute(&mut session)
        .await
        .map_err(SqlError::Connect)?;
        let locked = sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(MIGRATION_LEASE_KEY)
            .execute(&mut session)
            .await;
        match locked {
            Ok(_) => {}
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
                return Err(SqlError::Conflict {
                    detail: format!(
                        "another migration held the database migration lease for {}s; \
                         retry after it finishes",
                        wait.as_secs()
                    ),
                });
            }
            Err(error) => return Err(SqlError::Connect(error)),
        }
        sqlx::query("RESET lock_timeout")
            .execute(&mut session)
            .await
            .map_err(SqlError::Connect)?;
        Ok(MigrationLease { session })
    }
}

#[cfg(test)]
mod tests {
    use crate::{MIGRATION_SEARCH_PATH, OWNED_SCHEMAS};

    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn schema_ownership_is_explicit() {
        assert_eq!(OWNED_SCHEMAS, &["platform", "wyrd"]);
        assert_eq!(MIGRATION_SEARCH_PATH, "wyrd, platform, public");
        assert!(!OWNED_SCHEMAS.contains(&"vala"));
        assert!(!OWNED_SCHEMAS.contains(&"skald"));
    }

    #[test]
    fn migrations_embed_count_matches_files() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        assert_eq!(migrator.migrations.len(), files.len());
        assert!(
            !migrator.migrations.is_empty(),
            "wyrd-sql must embed at least one migration"
        );
    }

    #[test]
    fn create_table_statements_stay_in_owned_schemas() {
        let unexpected = migration_files()
            .into_iter()
            .flat_map(|file_name| {
                let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("migrations")
                    .join(file_name);
                fs::read_to_string(path)
                    .expect("migration sql is readable")
                    .lines()
                    .map(str::trim_start)
                    .filter(|line| line.starts_with("CREATE TABLE "))
                    .filter(|line| {
                        !line.starts_with("CREATE TABLE wyrd.")
                            && !line.starts_with("CREATE TABLE platform.")
                    })
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert!(
            unexpected.is_empty(),
            "CREATE TABLE statements must target wyrd.* or platform.*: {unexpected:?}"
        );
    }

    #[test]
    fn migration_filenames_match_timestamp_versions() {
        let migrator = sqlx::migrate!("./migrations");
        let files = migration_files();

        let mut prev_version = 0i64;
        for (migration, file_name) in migrator.migrations.iter().zip(&files) {
            let version = migration.version;
            let prefix = format!("{version}_");

            assert!(
                version > prev_version,
                "migration version {version} must be greater than previous {prev_version}"
            );
            prev_version = version;

            assert!(
                file_name.starts_with(&prefix),
                "migration file {file_name} must start with its version {prefix}"
            );
            assert!(
                file_name.ends_with(".sql"),
                "migration file {file_name} must be forward-only .sql"
            );
            assert!(
                !file_name.ends_with(".up.sql") && !file_name.ends_with(".down.sql"),
                "migration file {file_name} must not use reversible suffixes"
            );
            assert!(
                file_name
                    .trim_end_matches(".sql")
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
                "migration file {file_name} must use snake_case"
            );
        }
    }

    #[test]
    fn queries_module_shape_matches_foundation_plan() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let required_paths = [
            "src/queries/mod.rs",
            "src/queries/auth/mod.rs",
            "src/queries/auth/users.rs",
            "src/queries/auth/roles.rs",
            "src/queries/auth/role_assignments.rs",
            "src/queries/auth/api_keys.rs",
            "src/queries/auth/refresh_tokens.rs",
            "src/queries/auth/login_state.rs",
            "src/queries/auth/user_identities.rs",
            "src/queries/auth/sql",
            "src/queries/platform/mod.rs",
            "src/queries/platform/tenant_resolver.rs",
            "src/queries/platform/tenants.rs",
            "src/queries/platform/principals.rs",
            "src/queries/platform/provisioning.rs",
            "src/queries/platform/credentials.rs",
            "src/queries/platform/principal_grants.rs",
            "src/queries/storage/mod.rs",
            "src/queries/storage/multipart_uploads.rs",
            "src/queries/storage/artifact_metadata.rs",
            "src/queries/storage/idempotency.rs",
            "src/queries/storage/admin/mod.rs",
            "src/queries/storage/admin/multipart_uploads.rs",
            "src/queries/storage/admin/idempotency.rs",
            "src/queries/platform/sql",
            "src/row_types/mod.rs",
            "src/row_types/auth/mod.rs",
            "src/row_types/platform/mod.rs",
        ];

        for relative_path in required_paths {
            assert!(
                crate_dir.join(relative_path).exists(),
                "missing planned wyrd-sql path: {relative_path}"
            );
        }

        let auth_doc = fs::read_to_string(crate_dir.join("src/queries/auth/mod.rs"))
            .expect("auth query module doc is readable");
        assert!(auth_doc.contains("&mut TenantConn<'_>"));
        assert!(auth_doc.contains("data_tenant_id = $"));

        let resolver =
            fs::read_to_string(crate_dir.join("src/queries/platform/tenant_resolver.rs"))
                .expect("tenant resolver module is readable");
        assert!(resolver.contains("resolve_by_slug_for_app"));
        assert!(resolver.contains("&TenantSlug"));
        assert!(resolver.contains("platform.resolve_tenant_by_slug($1)"));
        assert!(resolver.contains("raw-query grep"));
    }

    #[test]
    fn tenant_scoped_query_stubs_do_not_take_raw_pool_executors() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries/auth"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("auth query file is readable");
                let checked = without_line_comments(&body);
                (checked.contains("&PgPool")
                    || checked.contains("PgPool,")
                    || checked.contains("Transaction<'_")
                    || checked.contains("Transaction < '_")
                    || checked.contains(".begin("))
                .then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "tenant-scoped auth query modules must use TenantConn, not raw executors: {forbidden:?}"
        );
    }

    #[test]
    fn tenant_scoped_query_modules_do_not_issue_transaction_control_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let forbidden = rust_files_under(&crate_dir.join("src/queries/auth"))
            .into_iter()
            .chain(sql_files_under(&crate_dir.join("src/queries/auth/sql")))
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("query source is readable");
                let uncommented = without_line_comments(&body);
                let checked = production_source(&uncommented).to_ascii_uppercase();
                let has_transaction_control = ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT"]
                    .into_iter()
                    .any(|keyword| checked.contains(keyword));
                has_transaction_control.then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "tenant-scoped query modules must rely on TenantConn, not raw transaction SQL: {forbidden:?}"
        );
    }

    #[test]
    fn wyrd_sql_does_not_coordinate_transactions_with_vala_sql() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest =
            fs::read_to_string(crate_dir.join("Cargo.toml")).expect("manifest is readable");
        assert!(
            !manifest.contains("vala-sql") && !manifest.contains("vala_sql"),
            "wyrd-sql must not depend on vala-sql for cross-crate transactions"
        );

        let forbidden = rust_files_under(&crate_dir.join("src"))
            .into_iter()
            .filter_map(|path| {
                let body = fs::read_to_string(&path).expect("source file is readable");
                let uncommented = without_line_comments(&body);
                let checked = production_source(&uncommented);
                (checked.contains("vala_sql::queries") || checked.contains("vala_sql :: queries"))
                    .then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            forbidden.is_empty(),
            "wyrd-sql must not call vala-sql query modules: {forbidden:?}"
        );
    }

    /// Joins a prose source into one whitespace-normalized line.
    ///
    /// The documentation assertions below look for whole sentences, but every
    /// source they read is hard-wrapped Markdown or rustdoc. Matching the raw
    /// text makes the assertion depend on where the wrap happens to fall, so a
    /// pure reflow that changes no words breaks the test. Each line therefore
    /// sheds its rustdoc marker — a wrap inside a `//!` block would otherwise
    /// leave the marker sitting mid-sentence — and every run of whitespace
    /// collapses to a single space, so the comparison sees the sentence rather
    /// than its line breaks.
    fn unwrapped(source: &str) -> String {
        source
            .lines()
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("//!")
                    .trim_start_matches("///")
            })
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Verifies the transaction invariants stay stated where callers read them.
    ///
    /// The rule these guard is architectural, not compilable: one `TenantConn`
    /// per tenant-scoped logical operation, and no cross-crate transaction
    /// built by importing another crate's query modules. The sibling tests
    /// enforce the code side; this one keeps the architecture doc, the
    /// `TenantConn` rustdoc, and the query-module doc from quietly dropping the
    /// statement a reader relies on.
    ///
    /// # Panics
    /// Panics when any of the three sources is unreadable, or when one of them
    /// no longer states its invariant.
    #[test]
    fn transaction_discipline_is_documented() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_dir = crate_dir
            .ancestors()
            .nth(3)
            .expect("crate lives three levels below repo root");
        let sql_foundation = unwrapped(
            &fs::read_to_string(repo_dir.join("architecture/v1/00-foundations/sql-foundation.md"))
                .expect("SQL foundation architecture doc is readable"),
        );
        let tenant_conn = unwrapped(
            &fs::read_to_string(crate_dir.join("src/tenant_conn.rs"))
                .expect("TenantConn is readable"),
        );
        let queries_doc = unwrapped(
            &fs::read_to_string(crate_dir.join("src/queries/mod.rs"))
                .expect("query module doc is readable"),
        );

        for (source, label, sentence) in [
            (
                &sql_foundation,
                "sql-foundation.md",
                "tenant-scoped logical operation acquires one `TenantConn`",
            ),
            (
                &sql_foundation,
                "sql-foundation.md",
                "Cross-crate work does not extend a transaction by importing another crate's",
            ),
            (
                &tenant_conn,
                "tenant_conn.rs",
                "transaction boundary for one tenant-scoped logical operation. Handlers and workers",
            ),
            (&queries_doc, "queries/mod.rs", "future outbox path"),
        ] {
            assert!(
                source.contains(sentence),
                "{label} must still state: {sentence}"
            );
        }
    }

    #[test]
    fn storage_migrations_preserve_foundation_locks() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let migrations = migration_files()
            .into_iter()
            .filter(|file_name| file_name.contains("storage"))
            .map(|file_name| {
                fs::read_to_string(crate_dir.join("migrations").join(file_name))
                    .expect("storage migration is readable")
            })
            .collect::<Vec<_>>()
            .join("\n");
        for table in [
            "wyrd.storage_multipart_uploads",
            "wyrd.storage_artifact_metadata",
            "wyrd.storage_idempotency_keys",
        ] {
            assert!(
                migrations.contains(table),
                "storage migration must create or configure {table}"
            );
        }
        let removed_access_table = ["storage_access", "_ledger"].concat();
        assert!(
            !migrations.contains(&removed_access_table),
            "storage migration set must not contain the removed access ledger"
        );

        let forbidden_parts_table = ["storage_multipart_upload", "_parts"].concat();
        assert!(
            !migrations.contains(&forbidden_parts_table),
            "storage foundation must not add a per-part uploads table"
        );
        assert!(
            migrations.contains("ENABLE ROW LEVEL SECURITY")
                && migrations.contains("FORCE ROW LEVEL SECURITY"),
            "storage tables must enable and force RLS"
        );
        assert!(
            migrations.contains("backend_upload_id")
                && migrations.contains("expected_size_bytes")
                && migrations.contains("block_count_planned")
                && migrations.contains("wire_protocol"),
            "multipart rows must persist restart-safe non-bearer completion state"
        );
    }

    #[test]
    fn storage_migrations_revoke_inherited_broad_privileges() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let storage = fs::read_to_string(
            crate_dir
                .join("migrations")
                .join("20260601000002_storage.sql"),
        )
        .expect("storage migration is readable");
        let idempotency = fs::read_to_string(
            crate_dir
                .join("migrations")
                .join("20260601000003_storage_idempotency.sql"),
        )
        .expect("storage idempotency migration is readable");
        let migrations = format!("{storage}\n{idempotency}");

        for (table, app_grant, admin_grant) in [
            (
                "wyrd.storage_multipart_uploads",
                "GRANT SELECT, INSERT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_app;",
                "GRANT SELECT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_platform_admin;",
            ),
            (
                "wyrd.storage_artifact_metadata",
                "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_artifact_metadata TO wyrd_app;",
                "GRANT SELECT ON wyrd.storage_artifact_metadata TO wyrd_platform_admin;",
            ),
            (
                "wyrd.storage_idempotency_keys",
                "GRANT SELECT, INSERT, UPDATE ON wyrd.storage_idempotency_keys TO wyrd_app;",
                "GRANT SELECT, DELETE ON wyrd.storage_idempotency_keys TO wyrd_platform_admin;",
            ),
        ] {
            assert!(
                migrations.contains(&format!("REVOKE ALL ON TABLE {table} FROM wyrd_app;")),
                "{table} must revoke inherited app table privileges"
            );
            assert!(
                migrations.contains(&format!(
                    "REVOKE ALL ON TABLE {table} FROM wyrd_platform_admin;"
                )),
                "{table} must revoke inherited platform-admin table privileges"
            );
            assert!(
                migrations.contains(app_grant),
                "{table} must grant the exact intended app privileges"
            );
            assert!(
                migrations.contains(admin_grant),
                "{table} must grant the exact intended platform-admin privileges"
            );
        }

        for forbidden in [
            "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_multipart_uploads",
            "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_artifact_metadata TO wyrd_platform_admin",
            "GRANT SELECT, INSERT, UPDATE, DELETE ON wyrd.storage_idempotency_keys TO wyrd_app",
        ] {
            assert!(
                !migrations.contains(forbidden),
                "storage migrations must not leave broad grant shape: {forbidden}"
            );
        }
    }

    #[test]
    fn storage_query_boundaries_are_explicit() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let tenant_modules = [
            "multipart_uploads.rs",
            "artifact_metadata.rs",
            "idempotency.rs",
        ];

        for file_name in tenant_modules {
            let path = crate_dir.join("src/queries/storage").join(file_name);
            let body = fs::read_to_string(&path).expect("storage tenant query is readable");
            let checked = without_line_comments(&body);

            assert!(
                checked.contains("&mut TenantConn<'_>"),
                "{file_name} must expose tenant-scoped functions through TenantConn"
            );
            assert!(
                !checked.contains("&PgPool")
                    && !checked.contains("PgPool,")
                    && !checked.contains(".begin("),
                "{file_name} must not take raw pools or open transactions"
            );
        }

        let admin_dir = crate_dir.join("src/queries/storage/admin");
        let admin_body = rust_files_under(&admin_dir)
            .into_iter()
            .map(|path| fs::read_to_string(path).expect("storage admin query is readable"))
            .collect::<Vec<_>>()
            .join("\n");
        let admin_checked = without_line_comments(&admin_body);

        assert!(
            admin_checked.contains("&PgPool"),
            "storage admin queries must make their cross-tenant pool boundary explicit"
        );
        assert!(
            !admin_checked.contains("TenantConn"),
            "storage admin query implementations must not pretend to be tenant-scoped"
        );
    }

    fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
        files_under_with_extension(dir, "rs")
    }

    fn sql_files_under(dir: &Path) -> Vec<PathBuf> {
        files_under_with_extension(dir, "sql")
    }

    fn files_under_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_files_with_extension(dir, extension, &mut files);
        files.sort();
        files
    }

    fn collect_files_with_extension(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
        if !dir.exists() {
            return;
        }

        for entry in fs::read_dir(dir).expect("source directory is readable") {
            let path = entry.expect("source directory entry is readable").path();
            if path.is_dir() {
                collect_files_with_extension(&path, extension, files);
            } else if path
                .extension()
                .is_some_and(|actual_extension| actual_extension == extension)
            {
                files.push(path);
            }
        }
    }

    fn without_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn production_source(source: &str) -> &str {
        source.split("\n#[cfg(test)]").next().unwrap_or(source)
    }

    fn migration_files() -> Vec<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files = fs::read_dir(&dir)
            .expect("migrations directory is readable")
            .map(|entry| {
                entry
                    .expect("migration directory entry is readable")
                    .file_name()
                    .into_string()
                    .expect("migration filename is utf-8")
            })
            .filter(|file_name| file_name.ends_with(".sql"))
            .collect::<Vec<_>>();
        files.sort();
        files
    }
}
