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
    #[error("Forge invariant failed: {detail}")]
    Invariant { detail: String },
}
