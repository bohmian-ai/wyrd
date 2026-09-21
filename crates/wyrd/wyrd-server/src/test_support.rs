//! Shared unit-test scaffolding for `wyrd-server`.
//!
//! `AppState::new` requires a live Redux catalog, and the catalog connects
//! to Postgres at construction. The in-crate unit tests cannot go through the
//! `wyrd-testing` harness (that would be a dependency cycle), so this module
//! stands up one embedded-Postgres-backed catalog and shares it across every
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

/// Owns the embedded Postgres fixture and warehouse tempdir for the lifetime of
/// the test process so the shared catalog's connections stay live.
struct SharedCatalog {
    _fixture: PgFixture,
    _warehouse: TempDir,
    catalog: Arc<BifrostCatalog>,
    storage: Arc<StorageHandle>,
}

impl SharedCatalog {
    /// Borrow the fixture's Wyrd Postgres handle for typed SQL capabilities.
    fn wyrd_postgres(&self) -> &WyrdPostgres {
        self._fixture.wyrd_postgres()
    }

    /// Borrow the fixture's Vala Postgres handle used by the Redux catalog.
    fn vala_postgres(&self) -> &ValaPostgres {
        self._fixture.vala_postgres()
    }
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
    .expect("Redux catalog builds against embedded postgres");

    SharedCatalog {
        _fixture: fixture,
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

/// One process-lifetime peer identity for unit tests that compose a `WyrdServer`.
///
/// A default target is Scribe- and Oracle-bearing, so `WyrdServer::new` refuses
/// to compose without a complete `bifrost.peer` identity. These unit tests
/// compose no peer listener — the shell `Bifrost` serves no API, so
/// `build_peer_grpc` returns nothing — but the identity must still be present
/// and readable. It is minted once and kept alive with its material for the
/// whole test binary.
///
/// `wyrd_testing::bifrost::peer_ca` mints the same shape for the harness, but
/// `wyrd-testing` is a dev-dependency of this crate: linking it from the lib
/// test target pulls in a second `wyrd-server`, so its types are not the ones
/// this crate's `WyrdServerConfig` accepts.
static PEER_IDENTITY: OnceLock<(TempDir, crate::config::BifrostPeerConfig)> = OnceLock::new();

/// Returns the shared unit-test peer identity, minting it on first use.
///
/// # Panics
///
/// Panics when certificate material cannot be minted or written under the
/// process-lifetime temporary directory.
pub(crate) fn test_peer_config() -> crate::config::BifrostPeerConfig {
    PEER_IDENTITY
        .get_or_init(|| {
            let root = tempfile::tempdir().expect("peer material tempdir");
            let key = rcgen::KeyPair::generate().expect("peer key pair generates");
            let certificate = rcgen::CertificateParams::new(vec!["localhost".to_owned()])
                .expect("peer certificate parameters are valid")
                .self_signed(&key)
                .expect("peer certificate self-signs");
            let certificate_path = root.path().join("peer-cert.pem");
            let private_key_path = root.path().join("peer-key.pem");
            let ca_path = root.path().join("peer-ca.pem");
            // Self-signed: the same certificate is the presented leaf and the
            // trust root, which is all a construction-time read requires.
            std::fs::write(&certificate_path, certificate.pem()).expect("peer certificate writes");
            std::fs::write(&ca_path, certificate.pem()).expect("peer CA writes");
            std::fs::write(&private_key_path, key.serialize_pem()).expect("peer key writes");
            let config = crate::config::BifrostPeerConfig {
                advertise_addr: Some("https://127.0.0.1:8443".to_owned()),
                ca_certificate_path: Some(ca_path),
                certificate_chain_path: Some(certificate_path),
                private_key_path: Some(private_key_path),
                server_name: Some("localhost".to_owned()),
                api_key: Some("unit-test-peer-api-key".to_owned()),
                ticket: crate::config::PeerTicketKeyringConfig {
                    active_key_id: Some("unit-test-peer-ticket".to_owned()),
                    signing_key_path: Some(root.path().join("peer-ticket-key.pem")),
                    verifying_keyring_path: Some(root.path().join("peer-ticket-keyring.json")),
                },
                ..crate::config::BifrostPeerConfig::default()
            };
            (root, config)
        })
        .1
        .clone()
}

/// Return a server Postgres owner over the shared fixture's exact runtime handles.
pub(crate) async fn test_server_postgres() -> Arc<crate::postgres::ServerPostgres> {
    Arc::new(crate::postgres::ServerPostgres::from_parts(
        shared().wyrd_postgres().clone(),
        shared().vala_postgres().clone(),
    ))
}

/// Return the process-lifetime local storage handle used by the test catalog.
pub(crate) async fn test_storage() -> Arc<StorageHandle> {
    Arc::clone(&shared().storage)
}

/// Return the fixture's `wyrd_app` pool for tests that must persist rows under
/// RLS. Its connections take reactor affinity from the process-wide persistent
/// runtime that initialized the fixture, so DB-acquiring tests must run on that
/// runtime via `wyrd_runtime::runtime().block_on(..)`.
#[allow(dead_code)]
pub(crate) async fn test_pool() -> sqlx::PgPool {
    shared()._fixture.app_pool().clone()
}

/// Return the fixture's seeded data tenant.
///
/// `vala.bifrost_tables.data_tenant_id` carries a FK to `platform.tenants`, and
/// the embedded fixture seeds exactly this one tenant. Unit tests that register
/// a `TenantOwned` table must bind this tenant: a freshly minted `DataTenantId`
/// is absent from `platform.tenants`, so the register INSERT FK-violates.
#[allow(dead_code)]
pub(crate) async fn test_tenant() -> wyrd_spec::DataTenantId {
    shared()._fixture.data_tenant_id()
}
