//! Shared unit-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live `Arc<WyrdCatalog>`, and the catalog connects
//! to Postgres at construction. The in-crate unit tests cannot go through the
//! `wyrd-testing` harness (that would be a dependency cycle), so this module
//! stands up one embedded-Postgres-backed catalog and shares it across every
//! unit test that only needs a well-formed `AppState`.

use std::sync::{Arc, OnceLock};

use secrecy::ExposeSecret;
use tempfile::TempDir;
use vala_bifrost::catalog::WyrdCatalog;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

/// Owns the embedded Postgres fixture and warehouse tempdir for the lifetime of
/// the test process so the shared catalog's connections stay live.
struct SharedCatalog {
    fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<WyrdCatalog>,
}

static SHARED: OnceLock<SharedCatalog> = OnceLock::new();

/// Stand up the embedded-Postgres-backed catalog. Runs exactly once, driven by
/// [`shared`] on the process-wide persistent runtime.
async fn build_shared() -> SharedCatalog {
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
        fixture,
        _warehouse: warehouse,
        catalog: Arc::new(catalog),
    }
}

/// Borrow the shared catalog, initializing it once on the process-wide
/// persistent Tokio runtime.
///
/// The fixture's `PgPool` connections take reactor affinity from the runtime
/// that establishes them. Each `#[tokio::test]` spins a throwaway runtime, so
/// initializing (or acquiring) on a caller's test runtime would poison the pool
/// the moment that test ends — the symptom is `pool timed out while waiting for
/// an open connection`. Driving init on `wyrd_runtime::runtime()` (a `'static`
/// runtime that outlives every test) from a dedicated thread — so it is safe to
/// call even from inside a `#[tokio::test]` runtime — keeps the pool live for
/// the whole binary. DB-acquiring tests must likewise run on that runtime via
/// `wyrd_runtime::runtime().block_on(..)`.
fn shared() -> &'static SharedCatalog {
    SHARED.get_or_init(|| {
        std::thread::spawn(|| wyrd_runtime::runtime().block_on(build_shared()))
            .join()
            .expect("shared catalog init thread")
    })
}

/// Return a live catalog handle for constructing `AppState` in unit tests.
pub(crate) async fn test_catalog() -> Arc<WyrdCatalog> {
    shared().catalog.clone()
}

/// Return the fixture's `wyrd_app` pool for tests that must persist rows under
/// RLS. Its connections take reactor affinity from the process-wide persistent
/// runtime that initialized the fixture, so DB-acquiring tests must run on that
/// runtime via `wyrd_runtime::runtime().block_on(..)`.
pub(crate) async fn test_pool() -> sqlx::PgPool {
    shared().fixture.app_pool().clone()
}

/// Return the fixture's seeded data tenant.
///
/// `vala.bifrost_tables.data_tenant_id` carries a FK to `platform.tenants`, and
/// the embedded fixture seeds exactly this one tenant. Unit tests that register
/// a `TenantOwned` table must bind this tenant: a freshly minted `DataTenantId`
/// is absent from `platform.tenants`, so the register INSERT FK-violates.
pub(crate) async fn test_tenant() -> wyrd_spec::DataTenantId {
    shared().fixture.data_tenant_id()
}
