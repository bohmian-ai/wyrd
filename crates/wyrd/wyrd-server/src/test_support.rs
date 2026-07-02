//! Shared unit-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live `Arc<WyrdCatalog>`, and the catalog connects
//! to Postgres at construction. The in-crate unit tests cannot go through the
//! `wyrd-testing` harness (that would be a dependency cycle), so this module
//! stands up one embedded-Postgres-backed catalog and shares it across every
//! unit test that only needs a well-formed `AppState`.

use std::sync::Arc;

use secrecy::ExposeSecret;
use tempfile::TempDir;
use tokio::sync::OnceCell;
use vala_bifrost::catalog::WyrdCatalog;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

/// Owns the embedded Postgres fixture and warehouse tempdir for the lifetime of
/// the test process so the shared catalog's connections stay live.
struct SharedCatalog {
    _fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<WyrdCatalog>,
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

            let (factory, props) = storage
                .iceberg_storage_factory()
                .expect("iceberg storage factory");
            let catalog_dsn = fixture.catalog_dsn().expect("catalog dsn resolves");
            let catalog = WyrdCatalog::new(
                catalog_dsn.expose_secret(),
                storage.warehouse_uri(),
                Arc::new(fixture.app_pool().clone()),
                None,
                factory,
                props,
            )
            .await
            .expect("catalog builds against embedded postgres");

            SharedCatalog {
                _fixture: fixture,
                _warehouse: warehouse,
                catalog: Arc::new(catalog),
            }
        })
        .await
}

/// Return a live catalog handle for constructing `AppState` in unit tests.
pub(crate) async fn test_catalog() -> Arc<WyrdCatalog> {
    shared().await.catalog.clone()
}
