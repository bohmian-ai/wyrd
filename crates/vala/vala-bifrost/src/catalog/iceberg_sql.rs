use std::collections::HashMap;
use std::sync::Arc;

use iceberg::CatalogBuilder;
use iceberg::io::StorageFactory;
use iceberg_catalog_sql::{SqlBindStyle, SqlCatalog, SqlCatalogBuilder};

use crate::error::BifrostError;

/// Build an iceberg-rust [`SqlCatalog`] named `wyrd` over the Postgres control
/// database at `catalog_uri`, backed by `warehouse` storage.
///
/// Installs the SQL drivers, forces `$N` bind style (Postgres), and merges the
/// storage factory's `storage_props` into the catalog config.
///
/// # Errors
/// Returns [`BifrostError::Iceberg`] when the catalog fails to load.
pub async fn build_catalog<S: std::hash::BuildHasher>(
    catalog_uri: &str,
    warehouse: &str,
    storage_factory: Arc<dyn StorageFactory>,
    storage_props: HashMap<String, String, S>,
) -> Result<SqlCatalog, BifrostError> {
    sqlx_catalog::any::install_default_drivers();

    let mut props = storage_props;
    props.insert(
        iceberg_catalog_sql::SQL_CATALOG_PROP_BIND_STYLE.to_string(),
        SqlBindStyle::DollarNumeric.to_string(),
    );

    SqlCatalogBuilder::default()
        .with_storage_factory(storage_factory)
        .load(
            "wyrd",
            [
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_URI.to_string(),
                    catalog_uri.to_string(),
                ),
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_WAREHOUSE.to_string(),
                    warehouse.to_string(),
                ),
            ]
            .into_iter()
            .chain(props)
            .collect(),
        )
        .await
        .map_err(BifrostError::Iceberg)
}
