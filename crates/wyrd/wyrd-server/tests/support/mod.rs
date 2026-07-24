//! Shared integration-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live `Arc<WyrdCatalog>`, and the catalog connects
//! to Postgres at construction. These integration tests build `AppState`
//! directly (rather than through the `wyrd-testing` harness), so this module
//! stands up one embedded-Postgres-backed catalog and shares it across every
//! test in the binary that only needs a well-formed `AppState`.

use std::sync::Arc;

use secrecy::ExposeSecret;
use tempfile::TempDir;
use tokio::sync::OnceCell;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

struct SharedCatalog {
    _fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<WyrdCatalog>,
    redux_catalog: Arc<BifrostCatalog>,
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
            let catalog = WyrdCatalog::new(
                catalog_dsn.expose_secret(),
                storage.backend_config(),
                Arc::new(fixture.app_pool().clone()),
            )
            .await
            .expect("catalog builds against embedded postgres");
            let redux_catalog = BifrostCatalog::new(
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
                redux_catalog: Arc::new(redux_catalog),
            }
        })
        .await
}

/// Return a live catalog handle for constructing `AppState` in integration tests.
pub async fn test_catalog() -> Arc<WyrdCatalog> {
    shared().await.catalog.clone()
}

/// Return the independent Redux catalog used by Gate and Forge.
pub async fn test_redux_catalog() -> Arc<BifrostCatalog> {
    Arc::clone(&shared().await.redux_catalog)
}
