//! Redux catalog error taxonomy.

use thiserror::Error;

/// Failures produced by the Redux-owned Bifrost catalog.
#[derive(Debug, Error)]
pub enum BifrostCatalogError {
    /// Iceberg catalog or metadata operation failed.
    #[error("iceberg catalog error: {0}")]
    Iceberg(#[from] iceberg::Error),
    /// Tenant-scoped Vala catalog query failed.
    #[error("catalog SQL error: {0}")]
    Sql(#[from] vala_sql::SqlError),
    /// A user field attempted to use a server-owned column name.
    #[error("reserved system column: {0}")]
    ReservedColumn(String),
    /// The tenant-scoped logical table registration does not exist.
    #[error("bifrost table not found: {0}")]
    TableNotFound(String),
    /// The supplied schema does not match the tenant-scoped registration.
    #[error("schema fingerprint mismatch for table: {0}")]
    FingerprintMismatch(String),
    /// Durable catalog metadata is internally inconsistent.
    #[error("catalog metadata inconsistency: {0}")]
    MetadataMismatch(String),
    /// The physical tenant/table binding is invalid.
    #[error("invalid tenant table binding: {0}")]
    InvalidBinding(String),
    /// Registration audit could not be appended atomically with the control row.
    #[error("audit outbox unavailable: {0}")]
    AuditUnavailable(String),
}

impl BifrostCatalogError {
    /// Convert the engine-local error into the stable public Bifrost catalog.
    #[must_use]
    pub fn into_public(self) -> wyrd_spec::vala::BifrostError {
        use wyrd_spec::vala::BifrostError as PublicError;

        match self {
            Self::ReservedColumn(column) => PublicError::ReservedColumn { column },
            Self::TableNotFound(table) => PublicError::TableNotFound { table },
            Self::FingerprintMismatch(table) => PublicError::FingerprintMismatch { table },
            Self::MetadataMismatch(detail) | Self::InvalidBinding(detail) => {
                PublicError::MetadataMismatch { detail }
            }
            Self::AuditUnavailable(detail) => PublicError::AuditUnavailable { detail },
            Self::Iceberg(error) => {
                tracing::error!(error = %error, "Redux Iceberg catalog operation failed");
                PublicError::CatalogUnreachable {
                    detail: "catalog metadata operation failed".to_owned(),
                }
            }
            Self::Sql(error) => {
                tracing::error!(error = %error, "Redux catalog SQL operation failed");
                PublicError::CatalogUnreachable {
                    detail: "catalog database operation failed".to_owned(),
                }
            }
        }
    }
}
