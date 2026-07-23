//! Server adapter from the legacy control-plane catalog to Redux Gate's seam.

use std::sync::Arc;

use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::gate::{Catalog, CatalogError};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_spec::ids::DataTenantId;

#[derive(Clone)]
pub struct ServerCatalog(pub Arc<WyrdCatalog>);

impl ServerCatalog {
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>) -> Self {
        Self(catalog)
    }
}

#[async_trait::async_trait]
impl Catalog for ServerCatalog {
    async fn table_schema_fingerprint(
        &self,
        namespace: BifrostNamespace,
        table: &str,
        tenant: DataTenantId,
    ) -> Result<SchemaFingerprint, CatalogError> {
        let namespace = vala_bifrost::BifrostNamespace::from_wire(namespace.as_str())
            .ok_or_else(|| CatalogError::Internal("unknown Redux namespace".to_owned()))?;
        self.0
            .table_schema_fingerprint(namespace, table, tenant)
            .await
            .map(SchemaFingerprint)
            .map_err(|error| match error {
                vala_bifrost::error::BifrostError::TableNotFound(table) => {
                    CatalogError::TableNotFound(table)
                }
                vala_bifrost::error::BifrostError::FingerprintMismatch(table) => {
                    CatalogError::FingerprintMismatch(table)
                }
                other => CatalogError::Internal(other.to_string()),
            })
    }
}
