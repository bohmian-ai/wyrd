//! Production-shaped catalog registration regressions for managed-column drift.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::datatypes::Schema;
use iceberg::TableCreation;
use iceberg::spec::FormatVersion;
use sqlx::postgres::PgPoolOptions;
use vala_bifrost::BifrostError;
use vala_bifrost::BifrostNamespace;
use vala_bifrost::WyrdCatalog;
use vala_bifrost::tables::traces::SpansTable;
use vala_bifrost::tables::{self, DomainTable};
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

/// Builds a catalog against the canonical Postgres URL supplied by the wrapper.
///
/// # Panics
///
/// Panics when the wrapper URL, local warehouse, storage handle, pool, or catalog
/// cannot be constructed.
async fn test_catalog() -> (Arc<WyrdCatalog>, PathBuf) {
    let database_url = env::var("WYRD_DATABASE_URL").expect("canonical Postgres URL is set");
    let catalog_url =
        env::var("WYRD_TEST_DATABASE_ADMIN_URL").expect("canonical catalog URL is set");
    let warehouse = env::temp_dir().join(format!("wyrd-t7-schema-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&warehouse).expect("test warehouse creates");
    let storage = StorageHandle::from_settings(StorageSettings {
        backend: BackendConfig::Local {
            root: warehouse.clone(),
        },
        require_encryption: false,
        presign_ttl: std::time::Duration::from_mins(10),
        part_size_bytes: 16 * 1024 * 1024,
        multipart_threshold_bytes: 100 * 1024 * 1024,
        public_base_url: Some("https://wyrd.test".to_owned()),
    })
    .await
    .expect("test storage creates");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("test pool connects");
    let catalog = WyrdCatalog::new(&catalog_url, storage.backend_config(), Arc::new(pool))
        .await
        .expect("test catalog creates");
    (Arc::new(catalog), warehouse)
}

/// Returns the normalized declared `SpansTable` schema used by Iceberg reads.
///
/// # Panics
///
/// Panics when the declared schema cannot round-trip through Iceberg conversion.
fn normalized_spans_schema() -> Schema {
    let iceberg_schema =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(SpansTable::schema().as_ref())
            .expect("declared schema converts to Iceberg");
    iceberg::arrow::schema_to_arrow_schema(&iceberg_schema)
        .expect("declared schema converts back to Arrow")
}

/// Installs one malformed physical table and returns its identifier for cleanup.
///
/// # Panics
///
/// Panics when namespace creation, schema conversion, or malformed table
/// installation fails; those failures invalidate the boundary fixture itself.
async fn install_physical(
    catalog: &Arc<WyrdCatalog>,
    warehouse: &Path,
    schema: Schema,
) -> iceberg::TableIdent {
    let namespace = BifrostNamespace::from_domain_namespace(SpansTable::NAMESPACE)
        .expect("declared namespace exists");
    catalog
        .ensure_namespace(namespace)
        .await
        .expect("test namespace creates");
    let namespace_ident = namespace.to_namespace_ident();
    let table_ident =
        iceberg::TableIdent::new(namespace_ident.clone(), SpansTable::NAME.to_owned());
    let iceberg_schema = iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&schema)
        .expect("malformed schema converts to Iceberg");
    let creation = TableCreation::builder()
        .name(SpansTable::NAME.to_owned())
        .location(format!(
            "file://{}/{}/{}",
            warehouse.display(),
            SpansTable::NAMESPACE,
            SpansTable::NAME
        ))
        .schema(iceberg_schema)
        .format_version(FormatVersion::V2)
        .build();
    catalog
        .iceberg_catalog()
        .create_table(&namespace_ident, creation)
        .await
        .expect("malformed table installs");
    table_ident
}

/// Removes a scenario's local warehouse if an assertion aborts before cleanup.
struct WarehouseCleanup {
    /// Temporary local warehouse owned by the scenario.
    path: PathBuf,
}

/// Best-effort local cleanup for a failed boundary assertion.
impl Drop for WarehouseCleanup {
    /// Deletes the warehouse, tolerating normal explicit-cleanup removal.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("test warehouse cleanup failed: {error}");
        }
    }
}

/// Removes the temporary Iceberg table and local warehouse after one assertion.
///
/// # Panics
///
/// Panics when Iceberg or filesystem cleanup fails.
async fn cleanup(catalog: &Arc<WyrdCatalog>, warehouse: &Path, table: &iceberg::TableIdent) {
    let drop_result = catalog.iceberg_catalog().drop_table(table).await;
    let remove_result = std::fs::remove_dir_all(warehouse);
    drop_result.expect("malformed table cleanup");
    if let Err(error) = remove_result {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "test warehouse cleanup failed: {error}"
        );
    }
}

/// Asserts the exact physical-drift error for the declared `SpansTable`.
///
/// # Panics
///
/// Panics when registration returns any result other than the expected drift.
fn assert_physical_drift(result: Result<(), BifrostError>) {
    match result {
        Err(BifrostError::PhysicalDrift {
            namespace, name, ..
        }) => {
            assert_eq!(namespace, SpansTable::NAMESPACE);
            assert_eq!(name, SpansTable::NAME);
        }
        other => panic!("expected SpansTable physical drift, got {other:?}"),
    }
}

/// Exercises one malformed physical schema through the real registration seam.
///
/// # Panics
///
/// Panics when the physical schema cannot be inspected, registration does not
/// fail closed, or repair mutates metadata/control state.
async fn assert_repair_rejects_schema(
    catalog: &Arc<WyrdCatalog>,
    warehouse: &Path,
    schema: Schema,
) {
    let _warehouse_cleanup = WarehouseCleanup {
        path: warehouse.to_owned(),
    };
    let table = install_physical(catalog, warehouse, schema).await;
    let before = catalog
        .iceberg_physical_schema(SpansTable::NAMESPACE, SpansTable::NAME)
        .await
        .expect("physical schema loads before repair");

    assert_physical_drift(tables::register::<SpansTable>(catalog).await);
    assert!(
        catalog
            .domain_table_fingerprint(SpansTable::NAMESPACE, SpansTable::NAME)
            .await
            .expect("control row lookup succeeds")
            .is_none()
    );
    let after = catalog
        .iceberg_physical_schema(SpansTable::NAMESPACE, SpansTable::NAME)
        .await
        .expect("physical schema loads after repair");
    assert_eq!(before, after, "repair must not rewrite Iceberg metadata");
    assert!(
        catalog
            .iceberg_table_exists(SpansTable::NAMESPACE, SpansTable::NAME)
            .await
            .expect("table existence lookup succeeds")
    );
    cleanup(catalog, warehouse, &table).await;
}

/// Repair rejects both managed-column drift cases without mutating the physical table.
///
/// # Panics
///
/// Panics when the registration boundary accepts drift or mutates catalog state.
#[tokio::test]
async fn repair_rejects_managed_column_drift_without_mutating_physical_schema() {
    let (catalog, warehouse) = test_catalog().await;
    let schema = normalized_spans_schema();
    let nullable_principal: Vec<_> = schema
        .fields()
        .iter()
        .map(|field| {
            if field.name() == wyrd_spec::vala::PRINCIPAL_ID {
                Arc::new(field.as_ref().clone().with_nullable(true))
            } else {
                Arc::clone(field)
            }
        })
        .collect();
    assert_repair_rejects_schema(&catalog, &warehouse, Schema::new(nullable_principal)).await;

    let missing_request: Vec<_> = schema
        .fields()
        .iter()
        .filter(|field| field.name() != wyrd_spec::vala::WYRD_REQUEST_ID)
        .cloned()
        .collect();
    assert_repair_rejects_schema(&catalog, &warehouse, Schema::new(missing_request)).await;
}
