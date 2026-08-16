//! Shared integration-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live Redux catalog, and the catalog connects
//! to Postgres at construction. These integration tests build `AppState`
//! directly (rather than through the `wyrd-testing` harness), so this module
//! stands up one embedded-Postgres-backed catalog and shares it across every
//! test in the binary that only needs a well-formed `AppState`.

use std::sync::Arc;

use secrecy::ExposeSecret;
use sqlx::PgPool;
use tempfile::TempDir;
use tokio::sync::OnceCell;
use vala_bifrost_redux::catalog::BifrostCatalog;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_server::postgres::ServerPostgres;
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

struct SharedCatalog {
    _fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<BifrostCatalog>,
}

static SHARED: OnceCell<SharedCatalog> = OnceCell::const_new();

async fn shared() -> &'static SharedCatalog {
    SHARED
        .get_or_init(|| async {
            let fixture = PgFixture::start().await.expect("embedded fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse tempdir");
            let storage = StorageHandle::from_settings(StorageSettings {
                backend: BackendConfig::Local {
                    root: warehouse.path().to_path_buf(),
                },
                require_encryption: false,
                presign_ttl: std::time::Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
                public_base_url: Some("https://wyrd.test".to_owned()),
            })
            .await
            .expect("local storage handle");

            let catalog_dsn = fixture.catalog_dsn();
            let catalog = BifrostCatalog::new(
                catalog_dsn.expose_secret(),
                storage.backend_config(),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("Redux catalog builds against embedded postgres");

            SharedCatalog {
                _fixture: fixture,
                _warehouse: warehouse,
                catalog: Arc::new(catalog),
            }
        })
        .await
}

/// Return a live catalog handle for constructing `AppState` in integration tests.
pub async fn test_catalog() -> Arc<BifrostCatalog> {
    Arc::clone(&shared().await.catalog)
}

/// Return the shared production-shaped Postgres handles, including the
/// platform-admin pool required by durable security-audit assertions.
pub async fn test_server_postgres() -> Arc<ServerPostgres> {
    let shared = shared().await;
    Arc::new(ServerPostgres::from_parts(
        shared._fixture.wyrd_postgres().clone(),
        shared._fixture.vala_postgres().clone(),
    ))
}

/// Return a migrator-backed read pool for durable cross-tenant assertions.
pub async fn test_superuser_pool() -> PgPool {
    shared()
        .await
        ._fixture
        .superuser_pool()
        .await
        .expect("shared fixture has a migrator assertion pool")
}

/// Seed an additional tenant for a gRPC security-boundary journey.
pub async fn seed_test_tenant(tenant: wyrd_spec::DataTenantId, slug: &str) {
    shared()
        .await
        ._fixture
        .seed_additional_tenant_with_uuid(tenant, slug)
        .await
        .expect("shared fixture seeds the requested tenant");
}
