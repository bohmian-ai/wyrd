//! Shared unit-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live Redux catalog, and the catalog connects
//! to Postgres at construction. The in-crate unit tests cannot go through the
//! `wyrd-testing` harness (that would be a dependency cycle), so this module
//! stands up one Postgres-backed catalog and shares it across every
//! unit test that only needs a well-formed `AppState`.

use std::sync::{Arc, OnceLock};

use crate::postgres::ServerPostgres;
use crate::state::{AppState, Bifrost};
use secrecy::ExposeSecret;
use tempfile::TempDir;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_sql::ValaPostgres;
use wyrd_auth_verify::{TokenVerifier, WyrdAuthVerifySettings};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_sql::WyrdPostgres;
use wyrd_storage::StorageHandle;
use wyrd_storage::settings::{BackendConfig, StorageSettings};

/// Owns the repository-managed Postgres fixture and warehouse tempdir for the lifetime of
/// the test process so the shared catalog's connections stay live.
struct SharedCatalog {
    fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<BifrostCatalog>,
    storage: Arc<StorageHandle>,
}

impl SharedCatalog {
    /// Borrow the fixture's Wyrd Postgres handle for typed SQL capabilities.
    fn wyrd_postgres(&self) -> &WyrdPostgres {
        self.fixture.wyrd_postgres()
    }

    /// Borrow the fixture's Vala Postgres handle used by the Redux catalog.
    fn vala_postgres(&self) -> &ValaPostgres {
        self.fixture.vala_postgres()
    }
}

static SHARED: OnceLock<SharedCatalog> = OnceLock::new();

/// Stand up the Postgres-backed catalog. Runs exactly once, driven by
/// [`shared`] on the process-wide persistent runtime.
async fn build_shared() -> SharedCatalog {
    let fixture = PgFixture::start().await.expect("embedded fixture starts");
    let warehouse = tempfile::tempdir().expect("warehouse tempdir");
    let storage = StorageHandle::from_settings(StorageSettings {
        backend: BackendConfig::Local {
            root: warehouse.path().to_path_buf(),
        },
        require_encryption: false,
        presign_ttl: std::time::Duration::from_mins(10),
        part_size_bytes: 16 * 1024 * 1024,
        multipart_threshold_bytes: 100 * 1024 * 1024,
    })
    .await
    .expect("local storage handle");

    let bifrost_storage = Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
        Arc::clone(&storage),
        vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
            vala_bifrost_redux::storage::BifrostStorageConfig::default(),
            u64::from(u32::MAX),
            false,
        )
        .expect("the default storage policy is valid"),
        None,
    ));
    let catalog = BifrostCatalog::new(
        fixture.catalog_dsn().expose_secret(),
        bifrost_storage,
        fixture.vala_postgres().clone(),
    )
    .await
    .expect("Redux catalog builds against repository-managed Postgres");

    SharedCatalog {
        fixture,
        _warehouse: warehouse,
        catalog: Arc::new(catalog),
        storage,
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
pub async fn test_catalog() -> Arc<BifrostCatalog> {
    Arc::clone(&shared().catalog)
}

/// One static, local-only decoding key for a unit-test [`TokenVerifier`].
///
/// [`TokenVerifier::new`] asserts at least one decoding key, so an `AppState`
/// shell cannot be composed from an empty map. The unit tests reach the service
/// functions with an already-resolved `Caller` and never verify a token, so any
/// well-formed public key satisfies the invariant; this is the same static key
/// the `state` module's shell tests use.
fn shell_decoding_keys()
-> std::collections::HashMap<wyrd_auth_verify::Kid, Arc<jsonwebtoken::DecodingKey>> {
    let mut keys = std::collections::HashMap::new();
    keys.insert(
        wyrd_auth_verify::Kid::new("test").expect("static kid is valid"),
        Arc::new(
            wyrd_auth_verify::public_key_from_pem(
                b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n",
            )
            .expect("static test key is valid"),
        ),
    );
    keys
}

/// Constructs a non-Bifrost unit-test application shell around shared dependencies.
pub fn test_app_state(
    postgres: Arc<ServerPostgres>,
    storage: Arc<StorageHandle>,
    catalog: Arc<BifrostCatalog>,
) -> AppState {
    let verifier = Arc::new(TokenVerifier::new(
        shell_decoding_keys(),
        "wyrd",
        WyrdAuthVerifySettings::default(),
    ));
    let shutdown = tokio_util::sync::CancellationToken::new();
    AppState::new(
        postgres,
        storage,
        Bifrost::test_shell_with_catalog(verifier, catalog),
        shutdown,
    )
}

/// Return a server Postgres owner over the shared fixture's exact runtime handles.
pub(crate) fn test_server_postgres() -> Arc<crate::postgres::ServerPostgres> {
    Arc::new(crate::postgres::ServerPostgres::from_parts(
        shared().wyrd_postgres().clone(),
        shared().vala_postgres().clone(),
    ))
}

/// Return the process-lifetime local storage handle used by the test catalog.
pub(crate) fn test_storage() -> Arc<StorageHandle> {
    Arc::clone(&shared().storage)
}

/// Return the fixture's `wyrd_app` pool for tests that must persist rows under
/// RLS. Its connections take reactor affinity from the process-wide persistent
/// runtime that initialized the fixture, so DB-acquiring tests must run on that
/// runtime via `wyrd_runtime::runtime().block_on(..)`.
pub(crate) fn test_pool() -> sqlx::PgPool {
    shared().fixture.app_pool().clone()
}

/// Return the fixture's seeded data tenant.
///
/// `vala.bifrost_tables.data_tenant_id` carries a FK to `platform.tenants`, and
/// the embedded fixture seeds exactly this one tenant. Unit tests that register
/// a `TenantOwned` table must bind this tenant: a freshly minted `DataTenantId`
/// is absent from `platform.tenants`, so the register INSERT FK-violates.
pub(crate) fn test_tenant() -> wyrd_spec::DataTenantId {
    shared().fixture.data_tenant_id()
}

/// Waits up to thirty seconds until the process audit outbox has counted a
/// failed write in `outbox_write_failures_total{outbox="audit"}`.
///
/// Audit-failure tests call this before restoring staging, so the retried
/// decision they then observe is proven to have failed at least once first.
/// The process recorder is the shared test recorder.
///
/// # Panics
///
/// Panics when no failed audit write is counted in time.
pub(crate) async fn await_audit_write_failure() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !crate::app::metrics::test_prometheus_handle()
        .render()
        .contains("outbox_write_failures_total{outbox=\"audit\"}")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "no failed audit write was counted"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
