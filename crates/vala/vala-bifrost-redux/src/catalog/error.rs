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
    /// A physical-layout declaration was rejected, or a valid declaration
    /// conflicted with the registered canonical layout.
    ///
    /// The variant carries the already-public error so the exact field,
    /// violation, and column survive to the HTTP, MCP, and SDK boundaries
    /// unchanged.
    #[error("physical layout rejected: {0}")]
    Layout(wyrd_spec::vala::BifrostError),
    /// Durable catalog metadata is internally inconsistent.
    #[error("catalog metadata inconsistency: {0}")]
    MetadataMismatch(String),
    /// Iceberg changed around every attempted tenant manifest projection.
    #[error("catalog visibility cut did not stabilize after {attempts} attempts")]
    UnstableCut {
        /// Number of complete snapshot/SQL/snapshot attempts discarded.
        attempts: u8,
    },
    /// A prepared row cannot be placed safely in either snapshot or hot membership.
    #[error("catalog publication visibility is ambiguous")]
    AmbiguousPublication,
    /// The physical tenant/table binding is invalid.
    #[error("invalid tenant table binding: {0}")]
    InvalidBinding(String),
    /// Registration audit could not be appended atomically with the control row.
    #[error("audit outbox unavailable: {0}")]
    AuditUnavailable(String),
    /// `DataFusion` provider construction failed.
    #[error("datafusion provider error: {0}")]
    DataFusion(#[from] datafusion::error::DataFusionError),
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
            Self::Layout(error) => error,
            Self::MetadataMismatch(detail) | Self::InvalidBinding(detail) => {
                PublicError::MetadataMismatch { detail }
            }
            Self::UnstableCut { .. } | Self::AmbiguousPublication => {
                PublicError::QueryVisibilityUnavailable
            }
            Self::AuditUnavailable(detail) => PublicError::AuditUnavailable { detail },
            Self::DataFusion(error) => {
                tracing::error!(error = %error, "Redux DataFusion provider construction failed");
                PublicError::CatalogUnreachable {
                    detail: "query provider construction failed".to_owned(),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Ambiguous publication metadata fails as visibility unavailable.
    #[test]
    fn ambiguous_publication_maps_to_visibility_unavailable() {
        assert!(matches!(
            BifrostCatalogError::AmbiguousPublication.into_public(),
            wyrd_spec::vala::BifrostError::QueryVisibilityUnavailable
        ));
    }
}
