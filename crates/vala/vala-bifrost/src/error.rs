use thiserror::Error;

#[derive(Debug, Error)]
pub enum BifrostError {
    // --- DomainTable registration errors (task 02) ---
    #[error("schema fingerprint drift on domain table {namespace}.{name}")]
    SchemaDrift {
        namespace: &'static str,
        name: &'static str,
        expected: [u8; 32],
        actual: [u8; 32],
    },

    #[error(
        "physical schema drift on domain table {namespace}.{name}: control says clean but Iceberg schema differs"
    )]
    PhysicalDrift {
        namespace: &'static str,
        name: &'static str,
        expected: [u8; 32],
        actual: [u8; 32],
    },

    #[error("domain table {namespace}.{name}: control row present but Iceberg table is missing")]
    IcebergMissing {
        namespace: &'static str,
        name: &'static str,
    },

    #[error("domain table {namespace}.{name}: Iceberg table already exists")]
    IcebergAlreadyExists {
        namespace: &'static str,
        name: &'static str,
    },

    #[error("redaction pass failed: {0}")]
    RedactionFailed(String),

    // --- original variants ---
    #[error("iceberg error: {0}")]
    Iceberg(#[from] iceberg::Error),

    #[error("sql error: {0}")]
    Sql(#[from] vala_sql::SqlError),

    #[error("arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("datafusion error: {0}")]
    DataFusion(#[from] datafusion::error::DataFusionError),

    #[error("reserved system column: {0}")]
    ReservedColumn(String),

    #[error("no tenant binding for OLAP operation")]
    TenantBindingMissing,

    #[error("bifrost table not found: {0}")]
    TableNotFound(String),

    #[error("schema fingerprint mismatch for table: {0}")]
    FingerprintMismatch(String),

    #[error("concurrent commit conflict for table: {0}")]
    CommitConflict(String),

    #[error("duplicate failed batch: {0}")]
    DuplicateFailedBatch(String),

    #[error("catalog metadata inconsistency: {0}")]
    MetadataMismatch(String),

    #[error("writer unavailable for table: {0}")]
    WriterUnavailable(String),

    #[error("audit outbox unavailable: {0}")]
    AuditUnavailable(String),

    #[error("internal bifrost failure: {0}")]
    Internal(String),

    /// The per-physical-table coordinator's local buffer is full.
    ///
    /// Local backpressure only — no rows from this request were written.
    /// Callers must back off and retry the full request (Q5).
    #[error("ingest writer busy for table: {0}")]
    IngestBusy(String),
}

impl BifrostError {
    pub fn into_public(self) -> wyrd_spec::vala::BifrostError {
        use wyrd_spec::vala::BifrostError as Pub;
        match self {
            Self::ReservedColumn(column) => Pub::ReservedColumn { column },
            Self::TenantBindingMissing => Pub::TenantBindingMissing,
            Self::TableNotFound(table) => Pub::TableNotFound { table },
            Self::FingerprintMismatch(table) => Pub::FingerprintMismatch { table },
            Self::CommitConflict(table) => Pub::CommitConflict { table },
            Self::DuplicateFailedBatch(batch_id) => Pub::DuplicateFailedBatch { batch_id },
            Self::MetadataMismatch(detail) => Pub::MetadataMismatch { detail },
            Self::WriterUnavailable(table) => Pub::WriterUnavailable { table },
            Self::SchemaDrift {
                namespace, name, ..
            } => Pub::MetadataMismatch {
                detail: format!("schema fingerprint drift on domain table {namespace}.{name}"),
            },
            Self::PhysicalDrift {
                namespace, name, ..
            } => Pub::MetadataMismatch {
                detail: format!("physical schema drift on domain table {namespace}.{name}"),
            },
            Self::IcebergMissing { namespace, name } => Pub::MetadataMismatch {
                detail: format!("Iceberg table missing for domain table {namespace}.{name}"),
            },
            Self::IcebergAlreadyExists { namespace, name } => Pub::MetadataMismatch {
                detail: format!("Iceberg table already exists: {namespace}.{name}"),
            },
            // Sanitize underlying-engine detail before it crosses the public API.
            Self::Iceberg(e) => {
                tracing::error!(error = %e, "iceberg catalog error");
                Pub::CatalogUnreachable {
                    detail: "catalog metadata operation failed".to_string(),
                }
            }
            Self::Sql(e) => {
                tracing::error!(error = %e, "sql catalog error");
                Pub::CatalogUnreachable {
                    detail: "catalog database connection failed".to_string(),
                }
            }
            Self::Arrow(e) => {
                tracing::error!(error = %e, "arrow error");
                Pub::Internal {
                    detail: "internal data processing error".to_string(),
                }
            }
            Self::DataFusion(e) => {
                tracing::error!(error = %e, "datafusion error");
                Pub::Internal {
                    detail: "internal query error".to_string(),
                }
            }
            Self::AuditUnavailable(detail) => Pub::AuditUnavailable { detail },
            Self::IngestBusy(table) => Pub::IngestBusy { table },
            Self::RedactionFailed(detail) | Self::Internal(detail) => Pub::Internal { detail },
        }
    }
}
