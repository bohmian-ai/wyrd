//! Redux-owned Iceberg SQL catalog construction.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::CatalogBuilder;
use iceberg::io::StorageFactory;
use iceberg_catalog_sql::{SqlBindStyle, SqlCatalog, SqlCatalogBuilder};

use crate::catalog::BifrostCatalogError;

/// Build the Iceberg SQL catalog used by Redux Gate, Forge, and Oracle.
///
/// # Errors
/// Returns [`BifrostCatalogError::Iceberg`] when the catalog cannot be loaded.
pub async fn build_catalog<S: std::hash::BuildHasher>(
    catalog_uri: &str,
    warehouse: &str,
    storage_factory: Arc<dyn StorageFactory>,
    storage_properties: HashMap<String, String, S>,
) -> Result<SqlCatalog, BifrostCatalogError> {
    sqlx_catalog::any::install_default_drivers();

    let mut properties = storage_properties;
    properties.insert(
        iceberg_catalog_sql::SQL_CATALOG_PROP_BIND_STYLE.to_owned(),
        SqlBindStyle::DollarNumeric.to_string(),
    );

    SqlCatalogBuilder::default()
        .with_storage_factory(storage_factory)
        .load(
            "wyrd-redux",
            [
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_URI.to_owned(),
                    catalog_uri.to_owned(),
                ),
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_WAREHOUSE.to_owned(),
                    warehouse.to_owned(),
                ),
            ]
            .into_iter()
            .chain(properties)
            .collect(),
        )
        .await
        .map_err(BifrostCatalogError::Iceberg)
}
