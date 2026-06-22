pub mod workload;

use std::sync::Arc;

use arrow::datatypes::{DataType, Field};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost::types::TableScope;
use wyrd_spec::ids::DataTenantId;
use wyrd_storage::factory::iceberg_factory::iceberg_storage_factory;
use wyrd_storage::settings::BackendConfig;

/// Shared benchmark fixture: Postgres + local object store + WyrdCatalog.
pub struct BenchFixture {
    pub catalog: WyrdCatalog,
    pub tenant: DataTenantId,
    _tmp: TempDir,
}

impl BenchFixture {
    /// Build the fixture synchronously by blocking on `rt`.
    /// Panics if `BIFROST_TEST_DB_URL` is not set.
    pub fn setup(rt: &Runtime) -> Self {
        let db_url = std::env::var("BIFROST_TEST_DB_URL")
            .expect("BIFROST_TEST_DB_URL must be set to run vala-bifrost benchmarks");

        rt.block_on(async {
            let tmp = tempfile::tempdir().expect("tmp dir");
            let warehouse = format!("file://{}", tmp.path().display());

            let pool = Arc::new(
                sqlx::PgPool::connect(&db_url)
                    .await
                    .expect("connect to bench db"),
            );
            vala_sql::testing::migrate_for_test(&pool)
                .await
                .expect("migrate bench db");

            let backend = BackendConfig::Local { root: tmp.path().to_path_buf() };
            let (factory, props) = iceberg_storage_factory(&backend).expect("storage factory");

            let catalog = WyrdCatalog::new(&db_url, &warehouse, pool, factory, props)
                .await
                .expect("WyrdCatalog::new");

            Self { catalog, tenant: DataTenantId::new_v7(), _tmp: tmp }
        })
    }

    /// Create a tenant-owned bench table with the given extra fields.
    /// Returns the table name.
    pub async fn create_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        extra_fields: Vec<Field>,
        scope: TableScope,
    ) {
        self.catalog
            .create_table(ns, name, extra_fields, scope, &[])
            .await
            .expect("create bench table");
    }

    /// Create a wide table with `col_count` Int64 columns for projection benches.
    pub async fn create_wide_table(&self, ns: BifrostNamespace, name: &str, col_count: usize) {
        let fields: Vec<Field> = (0..col_count)
            .map(|i| Field::new(format!("col_{i}"), DataType::Int64, false))
            .collect();
        self.catalog
            .create_table(ns, name, fields, TableScope::TenantOwned, &[])
            .await
            .expect("create wide bench table");
    }
}
