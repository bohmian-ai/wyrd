use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::derive::WyrdError;

/// Public Bifrost error variants exchanged across HTTP, MCP, and the Python SDK.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Error,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    WyrdError,
    strum::EnumCount,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "variant", content = "data", rename_all = "snake_case")]
pub enum BifrostError {
    /// This server has no ready local Oracle role.
    #[error("Oracle role unavailable")]
    #[wyrd_error(
        code = "WYRD_VALA_503_ORACLE_ROLE_UNAVAILABLE",
        status = 503,
        title = "Oracle role unavailable",
        remediation = "Retry through an Oracle-ready query endpoint."
    )]
    OracleRoleUnavailable,

    /// This server has no WAL-ready local Scribe role.
    #[error("Scribe role unavailable")]
    #[wyrd_error(
        code = "WYRD_VALA_503_SCRIBE_ROLE_UNAVAILABLE",
        status = 503,
        title = "Scribe role unavailable",
        remediation = "Retry ingest through a WAL-ready Scribe endpoint."
    )]
    ScribeRoleUnavailable,

    /// Query admission capacity was unavailable.
    #[error("query admission rejected")]
    #[wyrd_error(
        code = "WYRD_VALA_429_QUERY_ADMISSION_REJECTED",
        status = 429,
        title = "Query admission rejected",
        remediation = "Retry after the supplied bounded delay or reduce demand."
    )]
    QueryAdmissionRejected,

    /// The requested sealed or live visibility cut could not be acquired.
    #[error("query visibility unavailable")]
    #[wyrd_error(
        code = "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE",
        status = 503,
        title = "Query visibility unavailable",
        remediation = "Retry when the sealed/live source is available or select an allowed weaker freshness."
    )]
    QueryVisibilityUnavailable,

    /// A row violated the authenticated tenant boundary.
    #[error("query tenant invariant violated")]
    #[wyrd_error(
        code = "WYRD_VALA_500_QUERY_TENANT_INVARIANT",
        status = 500,
        title = "Query tenant invariant violated",
        remediation = "Stop and investigate tenant isolation; never retry as caller input."
    )]
    QueryTenantInvariant,

    /// Cross-tier rows with one identity contained inconsistent values.
    #[error("query reconciliation invariant violated")]
    #[wyrd_error(
        code = "WYRD_VALA_500_QUERY_RECONCILIATION_INVARIANT",
        status = 500,
        title = "Query reconciliation invariant violated",
        remediation = "Stop and investigate cross-tier row corruption."
    )]
    QueryReconciliationInvariant,

    /// A peer credential, fence, or replay check failed.
    #[error("query peer security validation failed")]
    #[wyrd_error(
        code = "WYRD_VALA_403_QUERY_PEER_SECURITY",
        status = 403,
        title = "Query peer security validation failed",
        remediation = "Reject the peer attempt and investigate credential/fence/replay state."
    )]
    QueryPeerSecurity,

    /// A query stream violated the closed frame protocol.
    #[error("query stream protocol violation")]
    #[wyrd_error(
        code = "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
        status = 502,
        title = "Query stream protocol violation",
        remediation = "Discard partial output and retry through a healthy server."
    )]
    QueryStreamProtocol,

    /// A query stream ended without a terminal frame.
    #[error("query stream incomplete")]
    #[wyrd_error(
        code = "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
        status = 502,
        title = "Query stream incomplete",
        remediation = "Discard partial output; retry because no terminal was observed."
    )]
    QueryStreamIncomplete,

    /// The transactional query read-decision audit could not be committed.
    #[error("query audit unavailable")]
    #[wyrd_error(
        code = "WYRD_VALA_503_QUERY_AUDIT_UNAVAILABLE",
        status = 503,
        title = "Query audit unavailable",
        remediation = "Restore the audit/SQL dependency before retrying."
    )]
    QueryAuditUnavailable,

    /// Query execution failed after the public stream began.
    #[error("query execution failed")]
    #[wyrd_error(
        code = "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
        status = 500,
        title = "Query execution failed",
        remediation = "Inspect the scrubbed terminal error and server diagnostics before retrying."
    )]
    QueryExecutionFailed,

    /// The ingest authentication credentials were missing or rejected.
    #[error("ingest authentication failed: {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_401_INGEST_AUTH",
        status = 401,
        title = "Ingest authentication failed",
        remediation = "Provide a valid Wyrd access token with permission to write the target ingest surface."
    )]
    IngestAuthentication {
        /// Human-readable authentication failure detail.
        message: String,
    },

    /// The ingest request violated the native protocol contract.
    #[error("ingest request validation failed: {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_INGEST_PROTO",
        status = 400,
        title = "Invalid ingest request",
        remediation = "Fix the ingest request fields and retry with a valid Wyrd ingest payload."
    )]
    IngestProtocol {
        /// Human-readable protocol validation detail.
        message: String,
    },

    /// A user-supplied schema field uses a reserved system column name.
    #[error("reserved system column: {column}")]
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

    /// A caller attempted to write a server-managed built-in table.
    #[error("write to reserved built-in table denied: {table}")]
    #[wyrd_error(
        code = "WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE",
        status = 403,
        title = "Reserved built-in write denied",
        remediation = "Write through the supported observation or audit API instead of directly targeting a reserved built-in table."
    )]
    ReservedBuiltinWriteDenied {
        /// Fully-qualified table name that was refused.
        table: String,
    },

    /// A physical table schema is missing the required tenant isolation column.
    #[error("table missing data_tenant_id managed column: {table}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_BIFROST_TENANT_ISOLATION_COLUMN_MISSING",
        status = 400,
        title = "Tenant isolation column missing",
        remediation = "Add the server-managed data_tenant_id Utf8 column to the physical table schema."
    )]
    TenantIsolationColumnMissing {
        /// Fully-qualified table name that is missing the tenant column.
        table: String,
    },

    /// No tenant binding was present when attempting an OLAP write or query.
    #[error("no tenant binding for OLAP operation")]
    #[wyrd_error(
        code = "WYRD_VALA_403_BIFROST_TENANT_BINDING_MISSING",
        status = 403,
        title = "No tenant binding for OLAP operation",
        remediation = "Acquire a TenantConn for a valid tenant before writing or querying Bifrost tables."
    )]
    TenantBindingMissing,

    /// The client-supplied `card_ref` is not within the authenticated
    /// principal's card scope, so the tagged write is refused.
    #[error("card_ref outside principal card scope: {card_ref}")]
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

    /// A card reference could not be resolved to a tenant-local card UID.
    #[error("card_ref cannot be resolved: {card_ref}")]
    #[wyrd_error(
        code = "WYRD_VALA_403_CARD_UNRESOLVED",
        status = 403,
        title = "Card reference unresolved",
        remediation = "Use a card_ref that resolves to a registered card in the current tenant."
    )]
    CardUnresolved {
        /// Canonical card reference that could not be resolved.
        card_ref: String,
    },

    /// The authenticated principal could not be stamped onto the ingest row.
    #[error("principal_id cannot be stamped for authenticated write")]
    #[wyrd_error(
        code = "WYRD_VALA_401_PRINCIPAL_UNRESOLVED",
        status = 401,
        title = "Ingest principal unresolved",
        remediation = "Authenticate with a token containing a valid principal identity and retry."
    )]
    PrincipalUnresolved,

    /// The requested Bifrost table does not exist in the catalog.
    #[error("bifrost table not found: {table}")]
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
    #[error("schema fingerprint mismatch for table: {table}")]
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
    #[error("concurrent commit conflict for table: {table}")]
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
    #[error("duplicate failed batch: {batch_id}")]
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

    /// The tail-fetch request targets a `(node_id, writer_epoch)` that does not
    /// match this Scribe's current writer stream (pod was replaced or restarted).
    #[error("live-tail stream mismatch: requested={requested}, actual={actual}")]
    #[wyrd_error(
        code = "WYRD_VALA_409_STREAM_MISMATCH",
        status = 409,
        title = "Live-tail stream identity mismatch",
        remediation = "Refresh the Scribe stream identity from the cluster catalog and retry the tail-fetch against the current writer stream."
    )]
    StreamMismatch {
        /// The stream identity the caller requested (e.g. "node:epoch").
        requested: String,
        /// The stream identity this Scribe actually owns.
        actual: String,
    },

    /// Registered catalog metadata is inconsistent with the actual catalog state.
    #[error("catalog metadata inconsistency: {detail}")]
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
    #[error("OLAP catalog unreachable: {detail}")]
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
    #[error("OLAP object storage unreachable: {detail}")]
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
    #[error("writer unavailable for table: {table}")]
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
    #[error("invalid or unsupported query SQL: {detail}")]
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
    #[error("query execution timed out")]
    #[wyrd_error(
        code = "WYRD_VALA_504_QUERY_TIMEOUT",
        status = 504,
        title = "Query execution timed out",
        remediation = "Narrow the query by adding filters or reducing scanned partitions."
    )]
    QueryTimeout,

    /// The query result exceeded the configured size limit.
    #[error("query result too large")]
    #[wyrd_error(
        code = "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        status = 413,
        title = "Query result too large",
        remediation = "Add a LIMIT or narrower filters for large result sets."
    )]
    QueryResultTooLarge,

    /// The decompressed canonical ingest payload exceeded the Scribe limit.
    #[error("ingest payload too large: {bytes} bytes")]
    #[wyrd_error(
        code = "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
        status = 413,
        title = "Ingest payload too large",
        remediation = "Reduce the request to at most 32 MiB of canonical transport bytes and retry."
    )]
    PayloadTooLarge {
        /// Server-measured canonical transport bytes.
        bytes: usize,
    },

    /// The ingest request exceeded the aggregate row bound.
    #[error("ingest request has too many rows ({rows} > {limit})")]
    #[wyrd_error(
        code = "WYRD_VALA_413_INGEST_OVERSIZED",
        status = 413,
        title = "Ingest request oversized",
        remediation = "Reduce the number of rows in the request and retry."
    )]
    IngestOversized {
        /// Number of rows observed in the request.
        rows: u64,
        /// Maximum rows allowed by the ingest contract.
        limit: u64,
    },

    /// An unexpected internal Bifrost failure occurred.
    #[error("internal bifrost failure: {detail}")]
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

    /// The transactional audit outbox could not durably record the operation.
    #[error("audit outbox unavailable: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_500_AUDIT_UNAVAILABLE",
        status = 500,
        title = "Audit outbox unavailable",
        remediation = "The operation was refused because its audit row could not be durably recorded. Retry; if it persists, check the audit outbox and catalog database health."
    )]
    AuditUnavailable {
        /// Human-readable detail about the audit-append failure.
        detail: String,
    },

    /// A pre-declared domain table's schema fingerprint does not match the
    /// registered fingerprint — schema evolution happened without a coordinated
    /// migration. Operator action required.
    #[error("domain table schema fingerprint drift: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_500_SCHEMA_DRIFT",
        status = 500,
        title = "Domain table schema fingerprint drift",
        remediation = "A domain table schema changed without a coordinated migration. Restore the previous schema or run the migration runbook."
    )]
    SchemaDrift {
        /// Human-readable detail naming the table and the drift.
        detail: String,
    },

    /// A pre-declared domain table's physical Iceberg schema does not match the
    /// declared schema despite the fingerprint saying clean.
    #[error("domain table physical schema drift: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_500_PHYSICAL_DRIFT",
        status = 500,
        title = "Domain table physical schema drift",
        remediation = "The Iceberg table physical schema drifted from the declared schema. Run the repair runbook."
    )]
    PhysicalDrift {
        /// Human-readable detail naming the table and the drift.
        detail: String,
    },

    /// A pre-declared domain table's control row is present but the Iceberg
    /// table is missing — the table was dropped or is otherwise unreachable.
    #[error("domain table Iceberg table missing: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_500_ICEBERG_MISSING",
        status = 500,
        title = "Domain table Iceberg table missing",
        remediation = "The Iceberg table for a pre-declared domain table is missing. Run the restore runbook."
    )]
    IcebergMissing {
        /// Human-readable detail naming the affected table.
        detail: String,
    },

    /// The write-path redaction pass failed on a sensitive-payload table.
    /// The commit was refused; no raw secrets were written.
    #[error("write-path redaction failed: {0}")]
    #[wyrd_error(
        code = "WYRD_VALA_500_REDACTION_FAILED",
        status = 500,
        title = "Write-path redaction failed",
        remediation = "The redaction classifier failed on this batch. Retry; if it persists, check the classifier installation."
    )]
    RedactionFailed(String),

    /// The requested trace id was not found within the queried window.
    #[error("trace not found: {trace_id}")]
    #[wyrd_error(
        code = "WYRD_VALA_404_TRACE_NOT_FOUND",
        status = 404,
        title = "Trace not found",
        remediation = "Confirm the trace_id and that it falls within the queried time window."
    )]
    TraceNotFound {
        /// Trace id that was not found.
        trace_id: String,
    },

    /// A query was submitted without a time window.
    #[error("query time window required")]
    #[wyrd_error(
        code = "WYRD_VALA_400_WINDOW_REQUIRED",
        status = 400,
        title = "Query time window required",
        remediation = "Supply `since` and/or `until` on the query request."
    )]
    WindowRequired,

    /// A query filter value was invalid.
    #[error("query filter invalid: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_FILTER_INVALID",
        status = 400,
        title = "Query filter invalid",
        remediation = "Correct the filter value; see the error detail for the offending field."
    )]
    FilterInvalid {
        /// Description of the offending filter.
        detail: String,
    },

    /// The supplied page token failed verification.
    #[error("page token invalid")]
    #[wyrd_error(
        code = "WYRD_VALA_400_PAGE_TOKEN_INVALID",
        status = 400,
        title = "Page token invalid",
        remediation = "Restart the query from page 1 without a page_token."
    )]
    PageTokenInvalid,

    /// The Iceberg snapshot pinned by the page token has expired.
    #[error("page snapshot expired")]
    #[wyrd_error(
        code = "WYRD_VALA_410_PAGE_SNAPSHOT_EXPIRED",
        status = 410,
        title = "Page snapshot expired",
        remediation = "restart the query from page 1"
    )]
    PageSnapshotExpired,

    /// The principal lacks query permission for the table.
    #[error("query forbidden")]
    #[wyrd_error(
        code = "WYRD_VALA_403_QUERY_FORBIDDEN",
        status = 403,
        title = "Query forbidden",
        remediation = "The principal lacks bifrost_query:read for this table."
    )]
    QueryForbidden,

    /// The principal lacks the payload resource permission.
    #[error("payload access forbidden")]
    #[wyrd_error(
        code = "WYRD_VALA_403_PAYLOAD_FORBIDDEN",
        status = 403,
        title = "Payload access forbidden",
        remediation = "The principal lacks the payload resource permission; sensitive columns were omitted."
    )]
    PayloadForbidden,

    /// The ingest writer's local buffer is full; the caller must retry.
    ///
    /// This is **local buffer backpressure only** — the per-physical-table
    /// coordinator's bounded channel is saturated. It does NOT indicate a
    /// partial success: no row from this request was written. Callers should
    /// back off and retry the full request.
    ///
    /// The OTLP ingest path maps overload to HTTP `429` and gRPC
    /// `RESOURCE_EXHAUSTED`, never to `partial_success`; no rows from the
    /// request are written.
    #[error("ingest writer busy: {table}")]
    #[wyrd_error(
        code = "WYRD_VALA_429_INGEST_BUSY",
        status = 429,
        title = "Ingest writer busy",
        remediation = "The per-table ingest buffer is full. Back off and retry the full request — no rows were written."
    )]
    IngestBusy {
        /// Fully-qualified table name whose writer buffer is saturated.
        table: String,
    },

    /// The WAL disk is full and cannot accept additional writes.
    ///
    /// This is a capacity error at the Scribe WAL layer. The operator must expand
    /// WAL storage capacity or trigger a seal to free space. No rows from this
    /// request were written.
    #[error("WAL disk full")]
    #[wyrd_error(
        code = "WYRD_VALA_507_WAL_DISK_FULL",
        status = 507,
        title = "WAL disk full",
        remediation = "The Scribe WAL disk is at capacity. Expand storage or wait for seal to free space, then retry."
    )]
    WalDiskFull,

    /// The OTLP request body was malformed and could not be decoded.
    ///
    /// Covers both protobuf decode failures and JSON parse errors on the
    /// OTLP/HTTP path. The request must be fixed before retrying — no rows
    /// were written and no retry will succeed with the same payload.
    #[error("OTLP request body malformed for table {table}: {detail}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
        status = 400,
        title = "OTLP request body malformed",
        remediation = "The OTLP request body could not be decoded. Verify the Content-Type matches the encoding (application/x-protobuf or application/json) and that the payload is a valid OTLP ExportRequest."
    )]
    OtlpRequestMalformed {
        /// Fully-qualified table name the request was targeting.
        table: String,
        /// Human-readable decode error detail.
        detail: String,
    },
}
