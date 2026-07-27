use thiserror::Error;

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("invalid Forge configuration: {detail}")]
    InvalidConfig { detail: String },
    #[error("Forge lease query failed: {0}")]
    Lease(#[source] vala_sql::SqlError),
    #[error("Forge SQL operation failed: {0}")]
    Sql(#[source] vala_sql::SqlError),
    #[error("Forge catalog operation failed: {0}")]
    Catalog(#[source] iceberg::Error),
    #[error("Forge staging object read failed: {0}")]
    ObjectStore(#[source] opendal::Error),
    #[error("Forge snapshot expiry failed: {detail}")]
    SnapshotExpiry { detail: String },
    #[error("Forge live-set construction failed: {detail}")]
    LiveSet { detail: String },
    #[error("Forge object listing failed: {0}")]
    ObjectList(#[source] opendal::Error),
    #[error("Forge object deletion failed: {0}")]
    ObjectDelete(#[source] opendal::Error),
    #[error("Forge parquet operation failed: {detail}")]
    Parquet { detail: String },
    #[error("Forge schema validation failed: {detail}")]
    Schema { detail: String },
    #[error("Forge candidate group is invalid: {detail}")]
    Group { detail: String },
    #[error("Forge lost lease fence `{lease_key}`")]
    FenceLost { lease_key: String },
    #[error("Forge reconciliation failed: {detail}")]
    Reconciliation { detail: String },
    #[error("Forge scheduler was shut down")]
    Shutdown,
    #[error("Forge {operation} timed out")]
    Timeout { operation: &'static str },
    #[error("Forge memory workspace admission failed: {detail}")]
    MemoryBudget { detail: String },
    #[error("Forge invariant failed: {detail}")]
    Invariant { detail: String },
    #[error("Forge scheduler is already running")]
    AlreadyRunning,
    #[error("Forge DataFusion execution failed: {0}")]
    DataFusion(#[source] datafusion::error::DataFusionError),
    #[error("Forge spill limit of {limit_bytes} bytes was exceeded")]
    SpillLimitExceeded { limit_bytes: u64 },
}
