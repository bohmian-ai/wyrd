use serde::{Deserialize, Serialize};

use crate::error::derive::WyrdError;

/// Public Bifrost error variants exchanged across HTTP, MCP, and the Python SDK.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, WyrdError)]
#[serde(tag = "variant", content = "data", rename_all = "snake_case")]
pub enum BifrostError {
    /// A user-supplied schema field uses a reserved system column name.
    #[wyrd_error(
        code = "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
        status = 400,
        title = "Reserved system column name",
        remediation = "Rename the column — wyrd_event_time, wyrd_ingested_at, wyrd_batch_id, and data_tenant_id are reserved."
    )]
    ReservedColumn {
        /// The reserved column name that was supplied.
        column: String,
    },

    /// A SystemShared table schema is missing the required `data_tenant_id` column.
    #[wyrd_error(
        code = "WYRD_VALA_400_BIFROST_MISSING_TENANT_COLUMN",
        status = 400,
        title = "SystemShared table missing data_tenant_id column",
        remediation = "Add a data_tenant_id Utf8 column to the schema for SystemShared tables."
    )]
    MissingTenantColumn {
        /// Fully-qualified table name that is missing the tenant column.
        table: String,
    },

    /// A TenantOwned table schema includes the `data_tenant_id` column, which is not allowed.
    #[wyrd_error(
        code = "WYRD_VALA_400_BIFROST_UNEXPECTED_TENANT_COLUMN",
        status = 400,
        title = "TenantOwned table must not include data_tenant_id",
        remediation = "Remove data_tenant_id from the schema — TenantOwned tables are isolated by catalog namespace."
    )]
    UnexpectedTenantColumn {
        /// Fully-qualified table name that incorrectly includes the tenant column.
        table: String,
    },

    /// No tenant binding was present when attempting an OLAP write or query.
    #[wyrd_error(
        code = "WYRD_VALA_403_BIFROST_TENANT_BINDING_MISSING",
        status = 403,
        title = "No tenant binding for OLAP operation",
        remediation = "Acquire a TenantConn for a valid tenant before writing or querying Bifrost tables."
    )]
    TenantBindingMissing,

    /// The client-supplied `card_ref` is not within the authenticated
    /// principal's card scope, so the tagged write is refused.
    #[wyrd_error(
        code = "WYRD_VALA_403_BIFROST_CARD_SCOPE",
        status = 403,
        title = "card_ref outside principal card scope",
        remediation = "Supply a card_ref the authenticated principal is authorized to tag, or add the target card to the Service card's components."
    )]
    CardScopeDenied {
        /// Canonical string form of the card reference that was refused.
        card_ref: String,
    },

    /// The requested Bifrost table does not exist in the catalog.
    #[wyrd_error(
        code = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
        status = 404,
        title = "Bifrost table not found",
        remediation = "Create the table via the catalog API before writing or querying."
    )]
    TableNotFound {
        /// Fully-qualified table name that was not found.
        table: String,
    },

    /// The Arrow schema fingerprint does not match the registered table schema.
    #[wyrd_error(
        code = "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
        status = 409,
        title = "Schema fingerprint mismatch",
        remediation = "The Arrow schema does not match the registered table schema. Evolve the schema explicitly."
    )]
    FingerprintMismatch {
        /// Fully-qualified table name whose schema does not match.
        table: String,
    },

    /// A concurrent writer committed to the same table at the same time.
    #[wyrd_error(
        code = "WYRD_VALA_409_BIFROST_COMMIT_CONFLICT",
        status = 409,
        title = "Concurrent OLAP commit conflict",
        remediation = "Retry the write — another writer committed to the same table concurrently."
    )]
    CommitConflict {
        /// Fully-qualified table name where the conflict occurred.
        table: String,
    },

    /// A batch with this ID was previously attempted but failed permanently.
    #[wyrd_error(
        code = "WYRD_VALA_409_BIFROST_DUPLICATE_FAILED_BATCH",
        status = 409,
        title = "Duplicate batch previously failed",
        remediation = "This batch_id was committed but the write failed. Use a new batch_id to retry."
    )]
    DuplicateFailedBatch {
        /// The idempotency batch ID that previously failed.
        batch_id: String,
    },

    /// Registered catalog metadata is inconsistent with the actual catalog state.
    #[wyrd_error(
        code = "WYRD_VALA_500_BIFROST_METADATA_MISMATCH",
        status = 500,
        title = "Bifrost table metadata mismatch",
        remediation = "Investigate table metadata integrity; possible mid-flight schema-evolution race."
    )]
    MetadataMismatch {
        /// Human-readable description of the inconsistency.
        detail: String,
    },

    /// The OLAP catalog database is unreachable.
    #[wyrd_error(
        code = "WYRD_VALA_503_BIFROST_CATALOG_UNREACHABLE",
        status = 503,
        title = "OLAP catalog unreachable",
        remediation = "Check the catalog database connection and retry."
    )]
    CatalogUnreachable {
        /// Underlying connectivity error detail.
        detail: String,
    },

    /// The OLAP object storage backend is unreachable.
    #[wyrd_error(
        code = "WYRD_VALA_503_BIFROST_STORAGE_UNREACHABLE",
        status = 503,
        title = "OLAP object storage unreachable",
        remediation = "Check the object storage configuration and credentials, then retry."
    )]
    StorageUnreachable {
        /// Underlying connectivity error detail.
        detail: String,
    },

    /// The Bifrost writer actor for this table is not running.
    #[wyrd_error(
        code = "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE",
        status = 503,
        title = "Bifrost writer unavailable",
        remediation = "The writer actor for this table is not running. Restart the write operation."
    )]
    WriterUnavailable {
        /// Fully-qualified table name whose writer is unavailable.
        table: String,
    },

    /// The submitted query SQL was not valid or not a supported `SELECT`.
    #[wyrd_error(
        code = "WYRD_VALA_400_QUERY_INVALID_SQL",
        status = 400,
        title = "Invalid or unsupported query SQL",
        remediation = "Submit a single SELECT statement; DDL/DML and unsupported constructs are rejected."
    )]
    QueryInvalidSql {
        /// Human-readable parse/validation detail.
        detail: String,
    },

    /// The query exceeded the configured execution time budget.
    #[wyrd_error(
        code = "WYRD_VALA_504_QUERY_TIMEOUT",
        status = 504,
        title = "Query execution timed out",
        remediation = "Narrow the query (add filters, reduce scanned partitions) or use the async query API."
    )]
    QueryTimeout,

    /// The query result exceeded the configured size limit.
    #[wyrd_error(
        code = "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        status = 413,
        title = "Query result too large",
        remediation = "Add a LIMIT or narrower filters, or use the async query API for large result sets."
    )]
    QueryResultTooLarge,

    /// An unexpected internal Bifrost failure occurred.
    #[wyrd_error(
        code = "WYRD_VALA_500_BIFROST_INTERNAL",
        status = 500,
        title = "Internal Bifrost failure",
        remediation = "Check server logs for details and contact support if the issue persists."
    )]
    Internal {
        /// Human-readable detail about the internal failure.
        detail: String,
    },
}
