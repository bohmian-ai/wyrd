//! SQL error catalog shared by Wyrd server-tier storage crates.

use wyrd_spec::error::derive::WyrdError;

/// SQL storage errors with stable Wyrd problem metadata.
#[derive(Debug, thiserror::Error, WyrdError)]
pub enum SqlError {
    /// Database connection failed.
    #[wyrd_error(
        code = "WYRD_SQL_500_CONNECT",
        status = 500,
        title = "Database connection failed",
        remediation = "Check WYRD_DATABASE_URL and verify the database is reachable."
    )]
    #[error("database connection failed: {0}")]
    Connect(#[source] sqlx::Error),

    /// Database migration failed.
    #[wyrd_error(
        code = "WYRD_SQL_500_MIGRATE",
        status = 500,
        title = "Database migration failed",
        remediation = "Inspect the failing migration SQL and check server logs."
    )]
    #[error("migration failed: {0}")]
    Migrate(#[source] sqlx::migrate::MigrateError),

    /// Previously-applied migration checksum no longer matches the embedded migration.
    #[wyrd_error(
        code = "WYRD_SQL_500_MIGRATE_CHECKSUM",
        status = 500,
        title = "Migration checksum drift",
        remediation = "A previously-applied migration was modified. Restore the original or write a new migration."
    )]
    #[error("migration checksum drift on version {version}: {detail}")]
    MigrateChecksum {
        /// Migration version whose checksum drifted.
        version: i64,
        /// Operator-facing checksum drift detail.
        detail: String,
    },

    /// Database query failed.
    #[wyrd_error(
        code = "WYRD_SQL_500_QUERY",
        status = 500,
        title = "Database query failed",
        remediation = "Check server logs for the SQL error."
    )]
    #[error("query failed: {0}")]
    Query(#[source] sqlx::Error),

    /// Query expected a row but none was returned.
    #[wyrd_error(
        code = "WYRD_SQL_404_NO_ROWS",
        status = 404,
        title = "Row not found",
        remediation = "Confirm the identifier and re-query."
    )]
    #[error("no rows returned")]
    NoRows,

    /// Unique constraint was violated.
    #[wyrd_error(
        code = "WYRD_SQL_409_UNIQUE_VIOLATION",
        status = 409,
        title = "Unique constraint violated",
        remediation = "Change the conflicting identifier and retry."
    )]
    #[error("unique constraint violated: {constraint}")]
    UniqueViolation {
        /// Constraint or unique-index name reported by Postgres.
        constraint: String,
    },

    /// Foreign-key constraint was violated.
    #[wyrd_error(
        code = "WYRD_SQL_409_FK_VIOLATION",
        status = 409,
        title = "Foreign key constraint violated",
        remediation = "Confirm the referenced row exists before retrying."
    )]
    #[error("foreign key constraint violated: {constraint}")]
    FkViolation {
        /// Constraint name reported by Postgres.
        constraint: String,
    },

    /// Check constraint was violated.
    #[wyrd_error(
        code = "WYRD_SQL_409_CHECK_VIOLATION",
        status = 409,
        title = "Check constraint violated",
        remediation = "Inspect the column values and adjust."
    )]
    #[error("check constraint violated: {constraint}")]
    CheckViolation {
        /// Constraint name reported by Postgres.
        constraint: String,
    },

    /// PL/pgSQL `RAISE EXCEPTION USING ERRCODE='P0001', CONSTRAINT=...`.
    ///
    /// Used by cards spec-hash immutability trigger and any trigger that needs
    /// a dispatchable invariant separate from real CHECK violations.
    #[wyrd_error(
        code = "WYRD_SQL_409_TRIGGER_EXCEPTION",
        status = 409,
        title = "Trigger exception raised",
        remediation = "The operation violated a trigger-enforced invariant; inspect the constraint name for details."
    )]
    #[error("trigger exception: {constraint}")]
    TriggerException {
        /// Constraint name carried by the PL/pgSQL RAISE EXCEPTION USING CONSTRAINT=... clause.
        constraint: String,
    },

    /// Operation conflicted with the current row state.
    #[wyrd_error(
        code = "WYRD_SQL_409_CONFLICT",
        status = 409,
        title = "SQL operation conflict",
        remediation = "Reload the affected row and retry only if the row is still in the expected state."
    )]
    #[error("sql operation conflict: {detail}")]
    Conflict {
        /// Conflict detail.
        detail: String,
    },

    /// A newer Forge planning-demand generation superseded the scheduler read.
    #[wyrd_error(
        code = "WYRD_SQL_409_FORGE_DEMAND_GENERATION_CHANGED",
        status = 409,
        title = "Forge planning demand changed",
        remediation = "Retry planning from the newest durable demand generation."
    )]
    #[error("Forge planning demand generation changed before acknowledgement")]
    ForgeDemandGenerationChanged,

    /// Stored SQL data violated an invariant guaranteed by Wyrd migrations.
    #[wyrd_error(
        code = "WYRD_SQL_500_INVARIANT_VIOLATION",
        status = 500,
        title = "SQL invariant violation",
        remediation = "Inspect the stored row and repair data that no longer matches Wyrd's schema invariants."
    )]
    #[error("sql invariant violation: {detail}")]
    InvariantViolation {
        /// Invariant violation detail.
        detail: String,
    },

    /// Tenant-scoped transaction failed.
    #[wyrd_error(
        code = "WYRD_SQL_500_TX_FAILED",
        status = 500,
        title = "Transaction failed",
        remediation = "Check server logs; the transaction has been rolled back."
    )]
    #[error("transaction failed: {0}")]
    TxFailed(#[source] sqlx::Error),

    /// Row-level security policy rejected the row for the current tenant binding.
    #[wyrd_error(
        code = "WYRD_SQL_403_RLS_DENIED",
        status = 403,
        title = "Row-level security policy violation",
        remediation = "The current tenant binding does not permit this row. Verify the request's tenant context matches the row's data_tenant_id."
    )]
    #[error("row-level security policy violation: {detail}")]
    RlsDenied {
        /// Database detail message identifying the RLS denial.
        detail: String,
    },

    /// Bootstrap DDL failed because the database role lacks required privileges.
    #[wyrd_error(
        code = "WYRD_SQL_500_INSUFFICIENT_PRIVILEGE",
        status = 500,
        title = "Insufficient database privileges",
        remediation = "Ensure WYRD_DATABASE_URL authenticates as wyrd_app for runtime queries, \
                       and WYRD_DATABASE_MIGRATOR_PASSWORD matches the wyrd_migrator role used \
                       by migrate()."
    )]
    #[error("insufficient database privileges: {detail}")]
    InsufficientPrivilege {
        /// Privilege denial message from Postgres.
        detail: String,
    },

    /// Stored tenant identifier violated Wyrd's tenant-id contract.
    #[wyrd_error(
        code = "WYRD_SQL_500_INVALID_TENANT_ID",
        status = 500,
        title = "Stored tenant identifier is invalid",
        remediation = "Inspect platform.tenants and repair rows whose data_tenant_id does not satisfy Wyrd's UUIDv7 contract."
    )]
    #[error("stored tenant identifier violated Wyrd's UUIDv7 contract: {0}")]
    InvalidDataTenantId(#[source] wyrd_spec::ids::IdError),
}

impl SqlError {
    /// Wrap an [`IdError`] as an [`SqlError::InvariantViolation`].
    pub fn from_id_error(e: wyrd_spec::ids::IdError) -> Self {
        Self::InvariantViolation {
            detail: format!("id validation failed: {e}"),
        }
    }

    /// Wrap a [`wyrd_semver::VersionError`] as an [`SqlError::InvariantViolation`].
    pub fn from_version(e: wyrd_semver::VersionError) -> Self {
        Self::InvariantViolation {
            detail: format!("version parse failed: {e}"),
        }
    }

    /// Wrap a [`wyrd_spec::envelope::SpecDecodeError`] as an [`SqlError::InvariantViolation`].
    pub fn from_spec_decode(e: wyrd_spec::envelope::SpecDecodeError) -> Self {
        Self::InvariantViolation {
            detail: format!("spec decode failed: {e}"),
        }
    }
}

impl From<sqlx::Error> for SqlError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NoRows,
            sqlx::Error::Database(db_error) => {
                let code = db_error.code().map(std::borrow::Cow::into_owned);
                match code.as_deref() {
                    Some("23505") => Self::UniqueViolation {
                        constraint: constraint_name(db_error.as_ref()),
                    },
                    Some("23503") => Self::FkViolation {
                        constraint: constraint_name(db_error.as_ref()),
                    },
                    Some("23514") => Self::CheckViolation {
                        constraint: constraint_name(db_error.as_ref()),
                    },
                    Some("P0001") => Self::TriggerException {
                        constraint: constraint_name(db_error.as_ref()),
                    },
                    Some("42501") if is_rls_denied(db_error.message()) => Self::RlsDenied {
                        detail: db_error.message().to_owned(),
                    },
                    _ => Self::Query(sqlx::Error::Database(db_error)),
                }
            }
            other => Self::Query(other),
        }
    }
}

impl From<sqlx::migrate::MigrateError> for SqlError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        match error {
            sqlx::migrate::MigrateError::VersionMismatch(version) => Self::MigrateChecksum {
                version,
                detail: format!(
                    "applied migration {version} checksum differs from the embedded file"
                ),
            },
            other => Self::Migrate(other),
        }
    }
}

fn constraint_name(error: &(dyn sqlx::error::DatabaseError + 'static)) -> String {
    error.constraint().unwrap_or("").to_owned()
}

fn is_rls_denied(message: &str) -> bool {
    // Matches English-locale Postgres error text. Non-English clusters with a
    // different lc_messages will have 42501 errors fall through to SqlError::Query.
    // If non-English deployments are required, also check db_error.detail() or
    // set lc_messages = 'en_US.UTF-8' on the Postgres cluster.
    message.to_ascii_lowercase().contains("row-level security")
}

#[cfg(test)]
mod tests {
    use super::SqlError;

    use std::borrow::Cow;
    use std::error::Error as StdError;
    use std::fmt;

    use sqlx::error::{DatabaseError, ErrorKind};

    #[test]
    fn metadata_codes_and_statuses_match_sql_catalog() {
        let cases = [
            (
                SqlError::Connect(sqlx::Error::PoolClosed),
                "WYRD_SQL_500_CONNECT",
                500,
                "Database connection failed",
            ),
            (
                SqlError::Migrate(sqlx::migrate::MigrateError::VersionMissing(7)),
                "WYRD_SQL_500_MIGRATE",
                500,
                "Database migration failed",
            ),
            (
                SqlError::MigrateChecksum {
                    version: 7,
                    detail: "changed".to_owned(),
                },
                "WYRD_SQL_500_MIGRATE_CHECKSUM",
                500,
                "Migration checksum drift",
            ),
            (
                SqlError::Query(sqlx::Error::PoolClosed),
                "WYRD_SQL_500_QUERY",
                500,
                "Database query failed",
            ),
            (
                SqlError::NoRows,
                "WYRD_SQL_404_NO_ROWS",
                404,
                "Row not found",
            ),
            (
                SqlError::UniqueViolation {
                    constraint: "uq_name".to_owned(),
                },
                "WYRD_SQL_409_UNIQUE_VIOLATION",
                409,
                "Unique constraint violated",
            ),
            (
                SqlError::FkViolation {
                    constraint: "fk_parent".to_owned(),
                },
                "WYRD_SQL_409_FK_VIOLATION",
                409,
                "Foreign key constraint violated",
            ),
            (
                SqlError::CheckViolation {
                    constraint: "ck_value".to_owned(),
                },
                "WYRD_SQL_409_CHECK_VIOLATION",
                409,
                "Check constraint violated",
            ),
            (
                SqlError::TriggerException {
                    constraint: "cards_spec_hash_immutable".to_owned(),
                },
                "WYRD_SQL_409_TRIGGER_EXCEPTION",
                409,
                "Trigger exception raised",
            ),
            (
                SqlError::Conflict {
                    detail: "state changed".to_owned(),
                },
                "WYRD_SQL_409_CONFLICT",
                409,
                "SQL operation conflict",
            ),
            (
                SqlError::ForgeDemandGenerationChanged,
                "WYRD_SQL_409_FORGE_DEMAND_GENERATION_CHANGED",
                409,
                "Forge planning demand changed",
            ),
            (
                SqlError::InvariantViolation {
                    detail: "bad enum".to_owned(),
                },
                "WYRD_SQL_500_INVARIANT_VIOLATION",
                500,
                "SQL invariant violation",
            ),
            (
                SqlError::RlsDenied {
                    detail: "new row violates row-level security policy".to_owned(),
                },
                "WYRD_SQL_403_RLS_DENIED",
                403,
                "Row-level security policy violation",
            ),
            (
                SqlError::TxFailed(sqlx::Error::PoolClosed),
                "WYRD_SQL_500_TX_FAILED",
                500,
                "Transaction failed",
            ),
            (
                SqlError::InsufficientPrivilege {
                    detail: "permission denied".to_owned(),
                },
                "WYRD_SQL_500_INSUFFICIENT_PRIVILEGE",
                500,
                "Insufficient database privileges",
            ),
            (
                SqlError::InvalidDataTenantId(wyrd_spec::ids::IdError::InvalidUuid7),
                "WYRD_SQL_500_INVALID_TENANT_ID",
                500,
                "Stored tenant identifier is invalid",
            ),
        ];

        for (error, code, status, title) in cases {
            assert_eq!(error.code(), code);
            assert_eq!(error.status(), status);
            assert_eq!(error.title(), title);
            assert!(!error.remediation().is_empty());
        }
    }

    #[test]
    fn row_not_found_maps_to_no_rows() {
        let error = SqlError::from(sqlx::Error::RowNotFound);
        assert!(matches!(error, SqlError::NoRows));
        assert_eq!(error.code(), "WYRD_SQL_404_NO_ROWS");
    }

    #[test]
    fn database_sqlstate_maps_to_constraint_variants() {
        let unique = SqlError::from(database_error("23505", "duplicate key", Some("uq_name")));
        let fk = SqlError::from(database_error(
            "23503",
            "foreign key failed",
            Some("fk_parent"),
        ));
        let check = SqlError::from(database_error("23514", "check failed", Some("ck_value")));
        let trigger = SqlError::from(database_error(
            "P0001",
            "trigger enforced invariant",
            Some("cards_spec_hash_immutable"),
        ));

        assert!(matches!(
            unique,
            SqlError::UniqueViolation { ref constraint } if constraint == "uq_name"
        ));
        assert!(matches!(
            fk,
            SqlError::FkViolation { ref constraint } if constraint == "fk_parent"
        ));
        assert!(matches!(
            check,
            SqlError::CheckViolation { ref constraint } if constraint == "ck_value"
        ));
        assert!(matches!(
            trigger,
            SqlError::TriggerException { ref constraint } if constraint == "cards_spec_hash_immutable"
        ));
        assert_eq!(trigger.code(), "WYRD_SQL_409_TRIGGER_EXCEPTION");
        assert_eq!(trigger.status(), 409);
    }

    #[test]
    fn rls_policy_message_maps_to_rls_denied() {
        let error = SqlError::from(database_error(
            "42501",
            "new row violates row-level security policy for table \"runs\"",
            None,
        ));

        assert!(matches!(
            error,
            SqlError::RlsDenied { ref detail } if detail.contains("row-level security")
        ));
        assert_eq!(error.code(), "WYRD_SQL_403_RLS_DENIED");
    }

    #[test]
    fn non_rls_42501_remains_generic_query() {
        let error = SqlError::from(database_error(
            "42501",
            "permission denied for table platform.tenants",
            None,
        ));

        assert!(matches!(error, SqlError::Query(_)));
        assert_eq!(error.code(), "WYRD_SQL_500_QUERY");
    }

    #[test]
    fn unmapped_database_error_remains_generic_query() {
        let error = SqlError::from(database_error("XX000", "internal database error", None));

        assert!(matches!(error, SqlError::Query(_)));
        assert_eq!(error.code(), "WYRD_SQL_500_QUERY");
    }

    #[test]
    fn migration_version_mismatch_maps_to_checksum_drift() {
        let error = SqlError::from(sqlx::migrate::MigrateError::VersionMismatch(42));

        assert!(matches!(
            error,
            SqlError::MigrateChecksum { version: 42, ref detail }
                if detail.contains("checksum differs")
        ));
        assert_eq!(error.code(), "WYRD_SQL_500_MIGRATE_CHECKSUM");
    }

    #[test]
    fn other_migration_errors_remain_generic_migrate() {
        let error = SqlError::from(sqlx::migrate::MigrateError::VersionMissing(42));

        assert!(matches!(error, SqlError::Migrate(_)));
        assert_eq!(error.code(), "WYRD_SQL_500_MIGRATE");
    }

    fn database_error(
        code: &'static str,
        message: &'static str,
        constraint: Option<&'static str>,
    ) -> sqlx::Error {
        sqlx::Error::Database(Box::new(MockDatabaseError {
            code,
            message,
            constraint,
        }))
    }

    #[derive(Debug)]
    struct MockDatabaseError {
        code: &'static str,
        message: &'static str,
        constraint: Option<&'static str>,
    }

    impl fmt::Display for MockDatabaseError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl StdError for MockDatabaseError {}

    impl DatabaseError for MockDatabaseError {
        fn message(&self) -> &str {
            self.message
        }

        fn code(&self) -> Option<Cow<'_, str>> {
            Some(Cow::Borrowed(self.code))
        }

        fn as_error(&self) -> &(dyn StdError + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn StdError + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn StdError + Send + Sync + 'static> {
            self
        }

        fn constraint(&self) -> Option<&str> {
            self.constraint
        }

        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }
}
