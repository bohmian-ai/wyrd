use thiserror::Error;

#[derive(Debug, Error)]
pub enum BifrostError {
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

    #[error("SystemShared table missing data_tenant_id column: {0}")]
    MissingTenantColumn(String),

    #[error("TenantOwned table must not include data_tenant_id: {0}")]
    UnexpectedTenantColumn(String),

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

    #[error("internal bifrost failure: {0}")]
    Internal(String),
}

impl BifrostError {
    pub fn into_public(self) -> wyrd_spec::vala::BifrostError {
        use wyrd_spec::vala::BifrostError as Pub;
        match self {
            Self::ReservedColumn(column) => Pub::ReservedColumn { column },
            Self::MissingTenantColumn(table) => Pub::MissingTenantColumn { table },
            Self::UnexpectedTenantColumn(table) => Pub::UnexpectedTenantColumn { table },
            Self::TenantBindingMissing => Pub::TenantBindingMissing,
            Self::TableNotFound(table) => Pub::TableNotFound { table },
            Self::FingerprintMismatch(table) => Pub::FingerprintMismatch { table },
            Self::CommitConflict(table) => Pub::CommitConflict { table },
            Self::DuplicateFailedBatch(batch_id) => Pub::DuplicateFailedBatch { batch_id },
            Self::MetadataMismatch(detail) => Pub::MetadataMismatch { detail },
            Self::WriterUnavailable(table) => Pub::WriterUnavailable { table },
            Self::Iceberg(e) => Pub::CatalogUnreachable {
                detail: e.to_string(),
            },
            Self::Sql(e) => Pub::CatalogUnreachable {
                detail: e.to_string(),
            },
            Self::Arrow(e) => Pub::Internal {
                detail: e.to_string(),
            },
            Self::DataFusion(e) => Pub::Internal {
                detail: e.to_string(),
            },
            Self::Internal(detail) => Pub::Internal { detail },
        }
    }
}
