pub mod workload;

use std::sync::Arc;

use arrow::datatypes::{DataType, Field};
use tokio::runtime::Runtime;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;
use wyrd_spec::ids::DataTenantId;
use wyrd_storage::settings::BackendConfig;

/// Stable warehouse root for benches — persists across runs so the Iceberg catalog
/// in Postgres and the on-disk metadata always agree.
const BENCH_WAREHOUSE_DIR: &str = "/tmp/vala-bifrost-bench";

/// Shared benchmark fixture: Postgres + local object store + `WyrdCatalog`.
pub struct BenchFixture {
    pub catalog: WyrdCatalog,
    pub tenant: DataTenantId,
}

impl BenchFixture {
    /// Build the fixture synchronously by blocking on `rt`.
    /// Panics if `BIFROST_TEST_DB_URL` is not set.
    pub fn setup(rt: &Runtime) -> Self {
        let db_url = std::env::var("BIFROST_TEST_DB_URL")
            .expect("BIFROST_TEST_DB_URL must be set to run vala-bifrost benchmarks");

        rt.block_on(async {
            let bench_dir = std::path::PathBuf::from(BENCH_WAREHOUSE_DIR);
            std::fs::create_dir_all(&bench_dir).expect("create bench warehouse dir");

            let pool = Arc::new(
                sqlx::PgPool::connect(&db_url)
                    .await
                    .expect("connect to bench db"),
            );
            let catalog_uri = {
                let sep = if db_url.contains('?') { "&" } else { "?" };
                format!(
                    "{db_url}{sep}options=-c%20role%3Dwyrd_catalog%20-c%20search_path%3Diceberg_catalog",
                )
            };
            let backend = BackendConfig::Local { root: bench_dir };
            let catalog = WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
                .await
                .expect("WyrdCatalog::new");

            let tenant = DataTenantId::new_v7();
            sqlx::query(
                "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status) \
                 VALUES ($1, $2, $2, 'active') ON CONFLICT (data_tenant_id) DO NOTHING",
            )
            .bind(tenant.as_uuid())
            .bind(format!("test-{}", tenant.as_uuid().simple()))
            .execute(&*pool)
            .await
            .expect("seed bench tenant");

            Self { catalog, tenant }
        })
    }

    /// Create a bench table, dropping and recreating if it already exists.
    /// This ensures the Iceberg location always points to the current warehouse.
    pub async fn create_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        extra_fields: Vec<Field>,
        scope: TableScope,
    ) {
        match self
            .catalog
            .create_table(vala_bifrost::catalog::CreateTableRequest {
                ns,

                name,

                user_fields: extra_fields.clone(),

                scope,

                tenant: self.tenant,

                partition_columns: &[],

                audit: None,
            })
            .await
        {
            Ok(_) => {}
            Err(e) if e.to_string().contains("already exists") => {
                self.catalog
                    .drop_table(ns, name, self.tenant)
                    .await
                    .expect("drop stale bench table");
                self.catalog
                    .create_table(vala_bifrost::catalog::CreateTableRequest {
                        ns,
                        name,
                        user_fields: extra_fields,
                        scope,
                        tenant: self.tenant,
                        partition_columns: &[],
                        audit: None,
                    })
                    .await
                    .expect("recreate bench table after drop");
            }
            Err(e) => panic!("create bench table: {e}"),
        }
    }

    /// Create a wide table with `col_count` Int64 columns for projection benches.
    #[allow(dead_code)]
    pub async fn create_wide_table(&self, ns: BifrostNamespace, name: &str, col_count: usize) {
        let fields: Vec<Field> = (0..col_count)
            .map(|i| Field::new(format!("col_{i}"), DataType::Int64, false))
            .collect();
        self.create_table(ns, name, fields, TableScope::TenantOwned)
            .await;
    }
}
