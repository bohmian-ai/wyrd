//! Server boot sequence for SQL-backed Wyrd runtime state.

pub mod auth;
pub mod data_root;
pub mod init;
pub mod issuer;
pub mod node_identity;

use crate::config::ForgeRuntimeConfig;
use std::sync::Arc;
use std::time::Duration;
use vala_bifrost_redux::resources::ResourcePlan;

use async_trait::async_trait;
use futures_util::{StreamExt, TryStreamExt};
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::forge::{
    Forge as ForgeCoordinator, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeError,
    ForgeLeaderPeer, ForgeObjectPages, ForgeObjectStore, ForgeTelemetry, ForgeWorker,
    ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::oracle::dispatcher::{
    BifrostPeerTls, OraclePeerTransportDirectory, OraclePeerWorker, ReservationRegistry,
    TonicOraclePeerTransport,
};
use vala_bifrost_redux::oracle::{
    Oracle as OracleEngine, OracleBuildConfig, OracleConfig, OracleMemoryResources,
};
use vala_bifrost_redux::resources::{
    BifrostRole, BifrostRoleResources, BifrostRuntimeResources, OracleClassSplit,
};
use vala_bifrost_redux::scribe::admission::{AdmissionConfig, EventTimeWindow};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribeIngressCpuPool,
    ScribePersistenceConfig, ScribePersistenceCpuPool, ScribeWalIoPool,
};
use wyrd_auth::connections::HumanConnections;
use wyrd_auth::platform_login::PlatformLogin;
use wyrd_auth::platform_sessions::PlatformSessions;
use wyrd_auth::sealing::SealedSecretRewrap;
use wyrd_auth_oidc::WorkloadBinding;
use wyrd_crypt::{SealingKeyring, SecretKey};
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{
    NodeId as ClusterNodeId, OracleCapabilitiesV1, QueryClass, ScribeCapabilitiesV1,
};
use wyrd_sql::OperatorPool;
use wyrd_sql::dsn::{DsnError, ResolvedDsns};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

use crate::auth::oauth::OAuthClients;
use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::boot::data_root::{BifrostDataRoot, BifrostDataRootError};
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::operators::keys::{KeyError, KeyFailure, OperatorKeys};
use crate::config::{BifrostRuntimeRole, WorkloadBindingEntry, WyrdServerConfig};
use crate::oracle::{OraclePeerAuthority, PostgresPeerSecurityAudit};
use crate::postgres::ServerPostgres;
use crate::state::{
    AppState, Forge, ForgeCompactionRuntime, Oracle, ProductionValidationError, Scribe,
    ScribeCoordinationRuntime,
};
use vala_sql::audit_outbox::{AuditOutbox, AuditSink};

/// Cadence of the leader's Forge maintenance timer when `[forge]` sets no
/// `maintenance_interval_secs`; [`resolve_forge_config`] applies it.
const DEFAULT_MAINTENANCE_INTERVAL: Duration = Duration::from_hours(1);
/// Bound on the advisory staging-file wake-up channel that feeds Forge
/// promotion; wake-ups are advisory, so a full channel loses no durable work.
const DEFAULT_HINT_CAPACITY: usize = 1_024;
/// Longest boot waits for the Oracle to finish startup before failing.
const ORACLE_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Number of listing entries the production Forge object store groups into one
/// orphan-GC page. The producer owns page granularity: this bounds how much of
/// an OpenDAL recursive walk materializes before orphan GC can check its page
/// cap and run budget at a page boundary. Sized so a steady-state prefix scans
/// in a handful of pages while a pathological prefix still yields control
/// promptly.
const FORGE_OBJECT_LIST_PAGE_ENTRIES: usize = 1_000;

/// Production Forge object-store capability backed by the server's OpenDAL operator.
#[derive(Debug, Clone)]
struct OpenDalForgeObjectStore {
    /// Shared OpenDAL operator used for Forge reads and lifecycle operations.
    operator: Arc<opendal::Operator>,
}

impl OpenDalForgeObjectStore {
    /// Create a Forge capability over the process-owned storage operator.
    #[must_use]
    fn new(operator: Arc<opendal::Operator>) -> Self {
        Self { operator }
    }

    /// Page a prefix in ascending key order on a backend without a native cursor.
    ///
    /// Each page is one full recursive walk that keeps only the
    /// [`FORGE_OBJECT_LIST_PAGE_ENTRIES`] smallest file keys strictly after the
    /// cursor, so memory stays bounded by the page size whatever the prefix
    /// holds and the walk order the backend happens to use. The last key of a
    /// page becomes the next cursor; a short page ends the stream. Pages are
    /// produced lazily, so orphan GC stopping at its page cap stops the walks.
    // ponytail: one full walk per page (O(pages × objects)); fine for local and
    // Azure staging prefixes, native cursor backends never take this path.
    fn emulated_pages(&self, prefix: &str, start_after: Option<&str>) -> ForgeObjectPages {
        let state = Some((
            self.clone(),
            prefix.to_owned(),
            start_after.map(str::to_owned),
        ));
        Box::pin(futures_util::stream::try_unfold(
            state,
            |state| async move {
                let Some((store, prefix, cursor)) = state else {
                    return Ok(None);
                };
                let page = store.smallest_after(&prefix, cursor.as_deref()).await?;
                let Some(last) = page.last().map(|entry| entry.path().to_owned()) else {
                    return Ok(None);
                };
                let next = (page.len() == FORGE_OBJECT_LIST_PAGE_ENTRIES).then_some((
                    store,
                    prefix,
                    Some(last),
                ));
                Ok(Some((page, next)))
            },
        ))
    }

    /// Walk `prefix` once and return its smallest file keys after `cursor`, sorted.
    ///
    /// Directory keys are skipped for the same reason as on the native path:
    /// they are not addressable objects and must not consume the page bound.
    ///
    /// # Errors
    ///
    /// Returns the underlying OpenDAL error when the walk cannot be opened or
    /// fails part-way.
    async fn smallest_after(
        &self,
        prefix: &str,
        cursor: Option<&str>,
    ) -> opendal::Result<Vec<opendal::Entry>> {
        let mut lister = self.operator.lister_with(prefix).recursive(true).await?;
        let mut smallest = std::collections::BTreeMap::new();
        while let Some(entry) = lister.try_next().await? {
            if !entry.metadata().is_file() || cursor.is_some_and(|cursor| entry.path() <= cursor) {
                continue;
            }
            smallest.insert(entry.path().to_owned(), entry);
            if smallest.len() > FORGE_OBJECT_LIST_PAGE_ENTRIES {
                smallest.pop_last();
            }
        }
        Ok(smallest.into_values().collect())
    }
}

#[async_trait]
impl ForgeObjectStore for OpenDalForgeObjectStore {
    /// Read one staged or Iceberg-owned object through OpenDAL.
    ///
    /// # Errors
    /// Returns the underlying OpenDAL error when the object cannot be read.
    async fn read(&self, path: &str) -> opendal::Result<opendal::Buffer> {
        self.operator.read(path).await
    }

    /// Read only the requested Parquet byte range through OpenDAL's ranged reader.
    ///
    /// # Errors
    ///
    /// Returns the underlying OpenDAL error when the reader cannot be opened or
    /// the requested range cannot be fetched.
    async fn read_range(
        &self,
        path: &str,
        range: std::ops::Range<u64>,
    ) -> opendal::Result<opendal::Buffer> {
        self.operator.reader(path).await?.read(range).await
    }

    /// Recursively list objects below a Forge-owned prefix.
    ///
    /// # Errors
    /// Returns the underlying OpenDAL error when listing cannot complete.
    async fn list(&self, prefix: &str) -> opendal::Result<Vec<opendal::Entry>> {
        self.operator.list_with(prefix).recursive(true).await
    }

    /// Recursively list objects below a Forge-owned prefix as bounded pages.
    ///
    /// Overrides the trait default with a true incremental OpenDAL lister so a
    /// large orphan prefix never materializes at once. The returned stream owns
    /// its lister and groups entries into fixed-size pages; orphan GC pulls one
    /// page at a time and stops at a page boundary once its per-run page cap or
    /// time budget is reached. Each page collects the lister's per-entry results
    /// so a mid-walk backend error surfaces as a failed page rather than being
    /// silently truncated.
    ///
    /// A backend with native `list_with_start_after` (S3, GCS) resumes on the
    /// backend's own cursor. A backend without it (local fs, Azure) is served
    /// by [`Self::emulated_pages`], which keeps the same ascending, exclusive
    /// cursor contract and page bound at the cost of one walk per page.
    ///
    /// # Errors
    ///
    /// Returns the underlying OpenDAL error when the native lister cannot be
    /// opened. Errors encountered after the walk begins, including every
    /// emulated walk, surface as a failed page in the returned stream.
    async fn list_pages(
        &self,
        prefix: &str,
        start_after: Option<&str>,
    ) -> opendal::Result<ForgeObjectPages> {
        if !self.operator.info().full_capability().list_with_start_after {
            return Ok(self.emulated_pages(prefix, start_after));
        }
        let mut listing = self.operator.lister_with(prefix).recursive(true);
        if let Some(cursor) = start_after {
            listing = listing.start_after(cursor);
        }
        let lister = listing.await?;
        // A resumable scan advances its cursor over objects only: a directory
        // key ends in a separator and is not an addressable object, so it must
        // not consume the page bound either — a chunk of only directories would
        // yield an empty page and leave the frontier standing still, starving
        // the objects behind it.
        let objects = lister.try_filter(|entry| std::future::ready(entry.metadata().is_file()));
        let pages = objects
            .chunks(FORGE_OBJECT_LIST_PAGE_ENTRIES)
            .map(|chunk| chunk.into_iter().collect::<opendal::Result<Vec<_>>>());
        Ok(Box::pin(pages))
    }

    /// Read object metadata before Forge makes a destructive decision.
    ///
    /// # Errors
    /// Returns the underlying OpenDAL error when metadata cannot be read.
    async fn stat(&self, path: &str) -> opendal::Result<opendal::Metadata> {
        self.operator.stat(path).await
    }

    /// Delete one object after Forge's live-set and fence checks complete.
    ///
    /// # Errors
    /// Returns the underlying OpenDAL error when deletion cannot complete.
    async fn delete(&self, path: &str) -> opendal::Result<()> {
        self.operator.delete(path).await
    }
}

/// Caller-supplied overrides applied to core `AppState` before
/// `production_validate`. Enterprise uses this to inject a real ABAC policy
/// hook + audit writer through the same seam production hardening enforces.
///
/// Defaults are all `None` — core (OSS) boot supplies its own defaults and
/// applies no overrides.
#[derive(Default)]
pub struct StateOverrides {
    /// Replace authorization handles (policy hook + RBAC + audit writer).
    pub authz: Option<ServerAuthz>,
    /// Test-only failure injected after Scribe activation to exercise rollback.
    #[cfg(feature = "test-support")]
    pub fail_after_scribe_activation: bool,
}

/// External process dependencies prepared before the one Bifrost composition call.
struct BifrostExternalDependencies {
    /// Shared production PostgreSQL handles.
    postgres: Arc<ServerPostgres>,
    /// Shared object-storage handle.
    storage: Arc<StorageHandle>,
    /// This node's one Bifrost storage owner.
    ///
    /// Retained so every co-located role shares one pointer-identical owner,
    /// one request ceiling, and one shutdown, rather than each role composing
    /// its own view of the same backend.
    bifrost_storage: Arc<vala_bifrost_redux::storage::BifrostStorage>,
    /// Shared Bifrost catalog.
    catalog: Arc<BifrostCatalog>,
    /// One root-derived role resource graph.
    resources: BifrostRoleResources,
    /// One shared current-ready cluster registry.
    cluster: Arc<ClusterRegistry>,
    /// Stable physical node identity.
    node_id: ClusterNodeId,
    /// Private endpoint advertised by selected roles.
    advertise_addr: String,
    /// Exclusively owned local root for every Bifrost-managed path.
    data_root: BifrostDataRoot,
}

/// Owns resources created only when the Scribe role is selected.
struct ScribeBootParts {
    /// Recovered Scribe allocation that Gate wraps for ingest.
    scribe: Arc<ScribeImpl>,
    /// Independently fenced Scribe role registered during this boot.
    scribe_role: RegisteredRole,
}

/// Errors raised while assembling server state.
#[derive(Debug, thiserror::Error)]
pub enum ServerBootError {
    /// The serving database URLs are missing or invalid.
    #[error(transparent)]
    Postgres(#[from] DsnError),
    /// Operator connection keys could not be built, or a multi-tenant
    /// production deployment could not read and decode an active tenant key.
    /// Boot fails closed; the error names only the source kind, key version,
    /// and failure class.
    #[error("Operator connection keys are unavailable: {0}")]
    OperatorKeys(#[from] KeyError),
    /// Pool construction, schema readiness, or a SQL operation failed.
    #[error(transparent)]
    Sql(#[from] wyrd_sql::error::SqlError),
    /// Storage boot failed.
    #[error(transparent)]
    Storage(#[from] wyrd_storage::StorageError),
    /// The one local Bifrost data root could not be prepared or is owned elsewhere.
    #[error(transparent)]
    BifrostDataRoot(#[from] BifrostDataRootError),
    /// Runtime pool construction failed.
    #[error("database pool construction failed")]
    PoolConnect(#[source] sqlx::Error),
    /// Wyrd's own signing key could not be loaded or its public key derived.
    /// Boot fails closed: without a usable signing key the server cannot mint or
    /// verify Wyrd JWTs.
    #[error("WYRD_SIGNING_KEY is invalid: {0}")]
    SigningKey(String),
    /// The gateway's policy-enforcing provider transport could not be built,
    /// so no provider could be reached; boot fails instead of serving a
    /// gateway that fails every call.
    #[error("gateway provider transport could not be built: {0}")]
    GatewayTransport(String),
    /// OIDC discovery for a configured trusted issuer was unavailable after the
    /// bounded retry schedule. Boot fails closed rather than starting with a
    /// silently empty registry; the message surfaces the stable
    /// `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` signal and names the offending issuer.
    #[error(
        "WYRD_AUTH_503_DISCOVERY_UNAVAILABLE: OIDC discovery failed for trusted issuer {issuer}: {message}"
    )]
    IssuerDiscoveryUnavailable {
        /// The issuer URL whose discovery could not be completed.
        issuer: String,
        /// Underlying discovery failure detail.
        message: String,
    },
    /// The configured implicit `[auth] tenant_slug` did not resolve to an active
    /// tenant at boot. Never bind issuers/bindings to a sentinel tenant.
    #[error("configured [auth] tenant_slug {slug:?} did not resolve to an active tenant")]
    TenantSlugUnresolved {
        /// The slug that failed to resolve.
        slug: String,
    },
    /// A `[[workload_bindings]]` entry's card target could not be built into a
    /// [`CardRef`]. This is config malformation (bad kind/name/space/version or
    /// issuer URL), caught at boot so the server fails closed rather than
    /// starting with a mis-built binding. It is not a card-existence check.
    #[error("workload_bindings[{index}] is invalid: {message}")]
    InvalidWorkloadBinding {
        /// Index of the offending `[[workload_bindings]]` entry.
        index: usize,
        /// What made the entry invalid.
        message: String,
    },
    /// A `[[trusted_issuers]]` entry declares human login trust. Trusted issuers
    /// are workload-only; human login is configured through the tenant OIDC
    /// connection API, so boot refuses rather than seeding trust login ignores.
    #[error(
        "trusted issuer {issuer} is principal_kind = human; trusted issuers are workload-only. \
         Configure human login through the tenant OIDC connection API \
         (PUT /v1/identity/oidc/candidate, then test and activate) and set \
         principal_kind = \"workload\" or remove this entry"
    )]
    HumanIssuerSeed {
        /// The configured issuer URL.
        issuer: String,
    },
    /// A configured `[[trusted_issuers]]` entry carries a client secret but no
    /// sealing key was provisioned to encrypt it. Boot fails closed rather than
    /// persisting a secret in plaintext.
    #[error("trusted issuer {issuer} could not be sealed: {message}")]
    IssuerSeal {
        /// The issuer URL whose secret could not be sealed.
        issuer: String,
        /// What made sealing fail (missing key, encryption, or serialization).
        message: String,
    },
    /// `WYRD_SEALING_KEY_FILE`/`WYRD_SEALING_KEY_BASE64` was set but did not
    /// decode to a 32-byte AES-256-GCM key, or no key is set while provider
    /// ciphertext is stored. Boot fails closed rather than proceeding with an
    /// unusable sealing key or secrets it could never open.
    #[error("WYRD_SEALING_KEY is invalid: {0}")]
    SealingKey(String),
    /// gRPC router assembly failed (e.g. missing token verifier).
    #[error(transparent)]
    Grpc(#[from] wyrd_tonic::server::GrpcError),
    /// Production-profile state validation failed.
    #[error(transparent)]
    ProductionValidation(#[from] ProductionValidationError),
    /// The supervised Forge worker could not be assembled from the shared
    /// production catalog, storage operator, and operator pool.
    #[error("Forge scheduler context is unavailable: {detail}")]
    ForgeSchedulerRequired {
        /// Missing or invalid Forge dependency detail.
        detail: String,
    },
    /// Production Card recovery requires a cross-tenant Wyrd operator pool.
    #[error(
        "WYRD_REGISTRY_500_OPERATOR_POOL_REQUIRED: production deployment requires a Wyrd operator pool for Card recovery"
    )]
    CardRecoveryPoolRequired,
    /// Forge configuration validation failed during boot.
    #[error(transparent)]
    Forge(#[from] ForgeError),
    /// Scribe WAL/runtime construction failed during boot.
    #[error("Scribe runtime construction failed: {0}")]
    Scribe(String),
    /// Oracle peer authority, audit, role, or worker construction failed.
    #[error("Oracle peer runtime construction failed: {0}")]
    OraclePeer(String),
    /// Peer mode was configured over process-local `file://` object storage.
    ///
    /// Peers read one another's committed objects, so a node-local root would
    /// let two ready members disagree about durable data.
    #[error("peer mode requires shared object storage; file:// storage is local to one process")]
    PeerLocalStorage,
    /// Startup could not ensure every canonical built-in table for every
    /// active tenant, so the server must not report ready.
    #[error(transparent)]
    BuiltinTables(#[from] Box<crate::components::platform::builtins::BuiltinTablesError>),
}

/// Resolve the operator `forge` config section into a `ForgeConfig` plus the
/// scheduler maintenance interval.
///
/// This is the single seam where the promoted operational knobs move from
/// server config into the compiled Forge limit set. Every field starts from
/// [`ForgeConfig::default`] (or [`DEFAULT_MAINTENANCE_INTERVAL`]) and is
/// overridden only when the operator supplied a value, so an empty `[forge]`
/// section yields a `ForgeConfig` byte-identical to the compiled default. The
/// resolved `ForgeConfig` is validated fail-closed downstream by
/// [`Forge::new`], which runs [`ForgeConfig::validate`] and the maintenance
/// interval check; this function performs no validation itself and never
/// panics.
fn resolve_forge_config(forge_runtime: &ForgeRuntimeConfig) -> (ForgeConfig, Duration) {
    let base = ForgeConfig::default();
    let config = ForgeConfig {
        orphan_gc_ttl: forge_runtime
            .orphan_gc_ttl_secs
            .map_or(base.orphan_gc_ttl, Duration::from_secs),
        orphan_gc_max_list_pages: forge_runtime
            .orphan_gc_max_list_pages
            .unwrap_or(base.orphan_gc_max_list_pages),
        orphan_gc_run_budget: forge_runtime
            .orphan_gc_run_budget_secs
            .map_or(base.orphan_gc_run_budget, Duration::from_secs),
        default_target_file_size_bytes: forge_runtime
            .target_file_size_bytes
            .unwrap_or(base.default_target_file_size_bytes),
        ..base
    };
    let maintenance_interval = forge_runtime
        .maintenance_interval_secs
        .map_or(DEFAULT_MAINTENANCE_INTERVAL, Duration::from_secs);
    (config, maintenance_interval)
}

/// Rejects a Scribe configuration whose largest expanded request exceeds the Bifrost cap.
///
/// The comparison charges nothing and creates no role share: it only proves
/// that one maximum legal expanded request can be held by the single shared
/// cap. Runtime admission charges actual bytes through the same root.
///
/// # Errors
///
/// Returns [`ServerBootError::Scribe`] when the configured expanded-data
/// ceiling exceeds the detected Bifrost cap.
fn validate_scribe_expanded_request(
    config: crate::config::ScribeRuntimeConfig,
    cap_bytes: usize,
) -> Result<(), ServerBootError> {
    let limits = config.ingest_limits();
    let required = limits.expanded_bytes();
    if required > cap_bytes {
        let shortfall = required.saturating_sub(cap_bytes);
        return Err(ServerBootError::Scribe(format!(
            "scribe configured expanded request requires {required} bytes but the detected \
             Bifrost cap provides {cap_bytes} bytes ({shortfall} bytes short). The requirement \
             scales from scribe.ingest_request_bytes = {request_bytes}. Lower \
             scribe.ingest_request_bytes to fit this node, or raise the node's memory limit \
             (WYRD_BIFROST_MEMORY_LIMIT_BYTES / the container memory limit)",
            request_bytes = limits.max_frame_bytes
        )));
    }
    Ok(())
}

/// Constructs the external dependency graph injected into Bifrost composition.
///
/// # Errors
/// Returns a boot error when Postgres readiness, storage, catalog, resource
/// detection, or volume-root preparation fails.
async fn build_bifrost_external_dependencies(
    dsns: &ResolvedDsns,
    config: &crate::config::BifrostRuntimeConfig,
    target: crate::config::BifrostTarget,
) -> Result<BifrostExternalDependencies, ServerBootError> {
    let data_root = BifrostDataRoot::prepare(config.data_dir())?;
    let postgres = Arc::new(ServerPostgres::connect_from_dsns(dsns).await?);
    let storage_settings = load_storage_settings()?;
    // Refused before any role row is reserved, so a misconfigured peer never
    // appears in membership, even as not-ready.
    if config.peer.is_enabled()
        && matches!(
            storage_settings.backend,
            wyrd_storage::settings::BackendConfig::Local { .. }
        )
    {
        return Err(ServerBootError::PeerLocalStorage);
    }
    let storage = StorageHandle::from_settings(storage_settings).await?;
    // A Scribe-bearing target owns durable state keyed by its own node, so it
    // reclaims the identity stored beside that state; a target with no Scribe
    // role owns no volume and is free to be a new node each incarnation.
    let node_id =
        if crate::config::BifrostRoles::for_target(target).contains(&BifrostRuntimeRole::Scribe) {
            node_identity::ScribeNodeIdentityStore::new(data_root.wal().to_path_buf())
                .load_or_create()
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?
        } else {
            NodeId::generate()
        };
    // Explicit peer mode publishes its runtime-supplied address. A
    // process-local node publishes an undialable local marker and filters its
    // registry to itself, so it never selects a member it has no channel to.
    let (advertise_addr, cluster) = match config.peer.advertised_uri() {
        Some(address) => (
            address,
            ClusterRegistry::new(
                postgres.vala().clone(),
                ClusterNodeId::new(node_id.as_uuid()),
            ),
        ),
        None => (
            format!("local://{}", node_id.as_uuid()),
            ClusterRegistry::new(
                postgres.vala().clone(),
                ClusterNodeId::new(node_id.as_uuid()),
            )
            .process_local(),
        ),
    };
    let cluster = Arc::new(cluster);
    let roles = crate::config::BifrostRoles::for_target(target);
    let resource_roles = roles
        .iter()
        .map(|role| match role {
            BifrostRuntimeRole::Scribe => BifrostRole::Scribe,
            BifrostRuntimeRole::ForgeCoordinator | BifrostRuntimeRole::ForgeWorker => {
                BifrostRole::Forge
            }
            BifrostRuntimeRole::Oracle => BifrostRole::Oracle,
        })
        .collect();
    let runtime_resources = BifrostRuntimeResources::detect_with_transport_message_limit(
        config.resources.policy(
            resource_roles,
            Some(data_root.oracle_spill().to_path_buf()),
            Some(data_root.volume_roots()),
        ),
        config.scribe.ingest_request_bytes,
    )
    .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    let resources = runtime_resources
        .compose_roles()
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    // Boot order is resources -> storage owner -> catalog. The owner's cache
    // budget is a share of this node's managed memory and is allocated only for
    // an Oracle-serving target, so it cannot be resolved before the resource
    // plan exists, and the catalog cannot be built before the owner it must run
    // its Iceberg I/O through.
    let bifrost_storage = Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
        Arc::clone(&storage),
        vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
            config.storage.to_storage_config(),
            u64::try_from(resources.plan().managed_memory_bytes).unwrap_or(u64::MAX),
            resources.oracle().is_some(),
        )
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        resources.oracle().map(|oracle| oracle.metadata()),
    ));
    let catalog = Arc::new(
        BifrostCatalog::new(
            dsns.catalog().expose_secret(),
            Arc::clone(&bifrost_storage),
            postgres.vala().clone(),
        )
        .await
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
    );
    Ok(BifrostExternalDependencies {
        postgres,
        storage,
        bifrost_storage,
        catalog,
        resources,
        cluster,
        node_id: ClusterNodeId::new(node_id.as_uuid()),
        advertise_addr,
        data_root,
    })
}

/// Composes the complete immutable Bifrost subsystem graph from injected dependencies.
///
/// # Errors
///
/// Returns [`ServerBootError`] when built-in table reconciliation, Scribe,
/// Forge, Oracle, role fencing, or request-boundary construction fails before
/// publication.
///
/// # Panics
///
/// Panics if a Forge compaction runtime owner built around a live runtime
/// yields no handle; construction guarantees one, so this is an invariant.
pub async fn compose_bifrost(
    inputs: crate::state::BifrostBuildInputs,
) -> Result<crate::state::ComposedBifrost, ServerBootError> {
    let crate::state::BifrostBuildInputs {
        target,
        deployment_profile,
        postgres,
        storage,
        bifrost_storage,
        catalog: bifrost,
        resources: bifrost_resources,
        cluster: cluster_registry,
        token_verifier,
        peer_tls,
        config: bifrost_config,
        forge_config: forge_runtime,
        node_id,
        advertise_addr,
        data_root,
        shutdown,
        #[cfg(feature = "test-support")]
        test_controls,
    } = inputs;
    let roles = crate::config::BifrostRoles::for_target(target);
    if roles.contains(&BifrostRuntimeRole::Scribe) {
        bifrost_config
            .scribe
            .validate()
            .map_err(ServerBootError::Scribe)?;
    }
    let postgres = Arc::new(postgres);
    let operator_pool =
        postgres
            .operator_pool()
            .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
                detail: "platform-admin operator pool is unavailable".to_owned(),
            })?;
    // Every active tenant holds the whole built-in inventory before any role
    // activates, so a table added to the inventory is backfilled on restart
    // and a failure aborts boot rather than serving a partial tenant.
    crate::components::platform::builtins::BuiltinTables::new(Arc::clone(&bifrost))
        .reconcile(&operator_pool)
        .await
        .map_err(Box::new)?;
    let scribe_config = bifrost_config.scribe;
    let resource_plan = bifrost_resources.plan();
    let pod_memory_limit = resource_plan.managed_memory_bytes;
    if roles.contains(&BifrostRuntimeRole::Scribe) {
        let scribe_resources = bifrost_resources.scribe().ok_or_else(|| {
            ServerBootError::Scribe(
                "Scribe role selected without a composed Scribe capability".to_owned(),
            )
        })?;
        validate_scribe_expanded_request(
            scribe_config,
            scribe_resources.maximum_ingress_envelope_bytes(),
        )?;
    }
    let (staging_file_publisher, staging_file_inbox) = staging_file_channel(DEFAULT_HINT_CAPACITY)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    // Declared outside the Scribe block because the composed graph must never
    // reach the `Runtime` value: `ScribeBootParts` and the role structs carry a
    // `Handle` only, and this slot hands the executor straight to the single
    // `ScribeCoordinationRuntime` owner returned from this function. It is the
    // owner type from the start rather than a bare `Option<Runtime>` so that an
    // early error return from any later stage releases the executor through the
    // same non-blocking path, instead of running blocking `Runtime` drop glue on
    // this async frame.
    let mut coordination_runtime = ScribeCoordinationRuntime::new(None);
    // Same ownership rule as the coordination runtime above: declared here so an
    // early error from any later stage releases the executor without blocking.
    let mut compaction_runtime = ForgeCompactionRuntime::new(None);
    let scribe = if roles.contains(&BifrostRuntimeRole::Scribe) {
        let scribe_role = cluster_registry
            .reserve_scribe(
                &advertise_addr,
                ScribeCapabilitiesV1 {
                    tail_protocol_version:
                        vala_bifrost_redux::scribe::tail_rpc::TAIL_PROTOCOL_VERSION,
                },
            )
            .await
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
        let stream = StreamIdentity::new(
            NodeId::new(node_id.as_uuid()),
            WriterEpoch::new(i64::try_from(scribe_role.fencing_token).map_err(|_| {
                ServerBootError::Scribe("Scribe role fence exceeds the WAL epoch range".to_owned())
            })?),
        );
        let scribe_output_volume = bifrost_resources
            .scribe()
            .and_then(|resources| resources.output_scratch())
            .ok_or_else(|| {
                ServerBootError::Scribe(
                    "live Scribe role requires a registered output scratch root".to_owned(),
                )
            })?;
        let configured_geometry = scribe_config
            .scribe_geometry(
                resolve_forge_config(&forge_runtime)
                    .0
                    .default_target_file_size_bytes,
            )
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
        #[cfg(feature = "test-support")]
        let geometry = test_controls
            .as_ref()
            .and_then(|controls| controls.scribe_geometry)
            .unwrap_or(configured_geometry);
        #[cfg(not(feature = "test-support"))]
        let geometry = configured_geometry;
        let wal_segment_bytes = geometry.wal_segment_bytes();
        let wal = Arc::new(
            WalWriter::new(
                data_root.wal(),
                *stream.node_id.as_bytes(),
                stream.writer_epoch.as_i64(),
                WalConfig::new(wal_segment_bytes)
                    .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
            )
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        // The dedicated coordination runtime hosts every long-lived Scribe
        // coordination task: the configured shard-owner lanes plus the
        // reconciliation and persistence loops. It is deliberately separate from
        // the request runtime so a saturated ingest path cannot starve shard
        // progress, and its thread count derives from
        // `default_scribe_coordination_threads` (available parallelism,
        // floored at two). Only the `Handle` is handed to
        // consumers; the `Runtime` value itself is moved into the single
        // non-`Clone` `ScribeCoordinationRuntime` owner returned below, which must
        // outlive Scribe role drain and releases the executor without blocking.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(scribe_config.coordination_threads)
            .thread_name_fn(|| {
                static THREAD_INDEX: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                format!(
                    "wyrd-scribe-coordination-{}",
                    THREAD_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                )
            })
            .enable_all()
            .build()
            .map_err(|error| {
                ServerBootError::Scribe(format!("coordination runtime failed: {error}"))
            })?;
        let coordination_handle = runtime.handle().clone();
        coordination_runtime = ScribeCoordinationRuntime::new(Some(runtime));
        #[cfg(feature = "test-support")]
        let wal_sync_delay = test_controls
            .as_ref()
            .map_or(Duration::ZERO, |controls| controls.scribe_wal_sync_delay);
        let execution_pools = ScribeExecutionPools::new(
            ScribeIngressCpuPool::try_new_with_capacity(scribe_config.ingress_cpu_threads, 256)
                .map_err(|error| {
                    ServerBootError::Scribe(format!("ingress CPU pool failed: {error}"))
                })?,
            ScribePersistenceCpuPool::try_new_with_capacity(
                scribe_config.persistence_cpu_threads,
                64,
            )
            .map_err(|error| {
                ServerBootError::Scribe(format!("persistence CPU pool failed: {error}"))
            })?,
            ScribeWalIoPool::try_new_with_capacity_and_delay(scribe_config.wal_io_threads, 256, {
                #[cfg(feature = "test-support")]
                {
                    wal_sync_delay
                }
                #[cfg(not(feature = "test-support"))]
                {
                    Duration::ZERO
                }
            })
            .map_err(|error| ServerBootError::Scribe(format!("WAL IO pool failed: {error}")))?,
        );
        // Built once, not once per feature branch. The previous shape
        // duplicated every field under `test-support` and `not(test-support)`,
        // and a field added to only one of them compiles cleanly on the
        // all-features route while breaking the default-feature production
        // server. One initializer makes that divergence unrepresentable.
        let admission_defaults = AdmissionConfig {
            memory_limit_bytes: pod_memory_limit,
            event_time_window: EventTimeWindow {
                past: scribe_config.event_time_past_window_secs.map_or_else(
                    || std::time::Duration::from_hours(720),
                    std::time::Duration::from_secs,
                ),
                future: scribe_config.event_time_future_window_secs.map_or_else(
                    || std::time::Duration::from_hours(24),
                    std::time::Duration::from_secs,
                ),
            },
        };
        #[cfg(feature = "test-support")]
        let admission = test_controls
            .as_ref()
            .and_then(|controls| controls.scribe_admission)
            .unwrap_or(admission_defaults);
        #[cfg(not(feature = "test-support"))]
        let admission = admission_defaults;
        let persistence = ScribePersistenceConfig::new(
            Arc::new(postgres.vala().clone()),
            64,
            scribe_config.wal_io_threads,
        )
        .with_operator_pool(postgres.operator_pool().ok_or_else(|| {
            ServerBootError::Scribe(
                "Scribe publication requires the platform operator pool".to_owned(),
            )
        })?)
        .with_output_scratch(scribe_output_volume);
        #[cfg(feature = "test-support")]
        let persistence = persistence.with_test_faults(
            test_controls
                .as_ref()
                .map_or_else(Default::default, |controls| {
                    controls.scribe_persistence_faults.clone()
                }),
        );
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            catalog: Some(Arc::clone(&bifrost)),
            operator: Arc::new(storage.operator().clone()),
            wal,
            stream,
            admission,
            coordination_runtime: coordination_handle,
            execution_pools,
            persistence: Some(persistence),
            resources: bifrost_resources.scribe().ok_or_else(|| {
                ServerBootError::Scribe(
                    "Scribe role selected without a composed Scribe capability".to_owned(),
                )
            })?,
            ingest_limits: scribe_config.ingest_limits(),
            geometry,
            staging_file_publisher: Some(staging_file_publisher),
        }));
        // A failed replay is role-local: faulting the WAL leaves Scribe
        // unready, its fence reserved but never activated, and its WAL and
        // staged files untouched for operator repair, while the other roles
        // keep serving.
        let replayed = match scribe.replay_wal_async().await {
            Ok(_) => true,
            Err(error) => {
                tracing::error!(
                    %error,
                    "Scribe WAL recovery failed; Scribe stays unready for operator repair"
                );
                scribe.wal_fault().cancel();
                false
            }
        };
        if let Err(error) = scribe.tail_service() {
            if let Err(cleanup_error) = cluster_registry.shutdown_role(scribe_role.clone()).await {
                tracing::warn!(%cleanup_error, "failed to release reserved Scribe fence after tail failure");
            }
            return Err(ServerBootError::Scribe(format!(
                "tail service failed before role activation: {error}"
            )));
        }
        // A peer-mode Scribe stays reserved but unready until its private
        // listener serves; the serving owner activates it then.
        if replayed && peer_tls.is_none() {
            if let Err(error) = cluster_registry.activate(&scribe_role).await {
                if let Err(cleanup_error) =
                    cluster_registry.shutdown_role(scribe_role.clone()).await
                {
                    tracing::warn!(%cleanup_error, "failed to release reserved Scribe fence after activation failure");
                }
                return Err(ServerBootError::Scribe(error.to_string()));
            }
            if let Err(error) = cluster_registry.refresh_snapshot().await {
                if let Err(cleanup_error) =
                    cluster_registry.shutdown_role(scribe_role.clone()).await
                {
                    tracing::warn!(%cleanup_error, "failed to release active Scribe fence after snapshot failure");
                }
                return Err(ServerBootError::Scribe(error.to_string()));
            }
        }
        Some(ScribeBootParts {
            scribe,
            scribe_role,
        })
    } else {
        None
    };

    let forge = if roles.contains(&BifrostRuntimeRole::ForgeCoordinator)
        || roles.contains(&BifrostRuntimeRole::ForgeWorker)
    {
        let (forge_config, maintenance_interval) = resolve_forge_config(&forge_runtime);
        #[cfg(feature = "test-support")]
        let forge_config = test_controls
            .as_ref()
            .and_then(|controls| controls.forge_config.clone())
            .unwrap_or(forge_config);
        let staging = Arc::new(storage.operator().clone());
        #[cfg(feature = "test-support")]
        let object_store: Arc<dyn ForgeObjectStore> = test_controls
            .as_ref()
            .and_then(|controls| controls.forge_object_store.clone())
            .unwrap_or_else(|| Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging))));
        #[cfg(not(feature = "test-support"))]
        let object_store: Arc<dyn ForgeObjectStore> =
            Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging)));
        let coordinator = ForgeCoordinator::new(ForgeBuildConfig {
            resources: bifrost_resources.forge().ok_or_else(|| {
                ServerBootError::ForgeSchedulerRequired {
                    detail: "Forge role selected without a composed Forge capability".to_owned(),
                }
            })?,
            spill_root: data_root.forge_spill().to_path_buf(),
            vala: postgres.vala().clone(),
            operator_pool: operator_pool.clone(),
            catalog: {
                #[cfg(feature = "test-support")]
                {
                    test_controls
                        .as_ref()
                        .and_then(|controls| controls.forge_catalog.clone())
                        .unwrap_or_else(|| bifrost.iceberg_catalog())
                }
                #[cfg(not(feature = "test-support"))]
                {
                    bifrost.iceberg_catalog()
                }
            },
            staging,
            object_store,
            hints: staging_file_inbox,
            config: forge_config,
            maintenance_interval,
            scheduler_owner: node_id.as_uuid(),
            clock: {
                #[cfg(feature = "test-support")]
                {
                    test_controls
                        .as_ref()
                        .map_or_else(ForgeClock::system, |controls| controls.forge_clock.clone())
                }
                #[cfg(not(feature = "test-support"))]
                {
                    ForgeClock::system()
                }
            },
            #[cfg(feature = "test-support")]
            completion_observer: test_controls
                .as_ref()
                .and_then(|controls| controls.forge_completion_observer.clone()),
            #[cfg(feature = "test-support")]
            scheduler_trigger: test_controls
                .as_ref()
                .map(|controls| controls.forge_scheduler_trigger.clone()),
            telemetry: Arc::new(ForgeTelemetry::new()),
        })?;
        // A peer-mode coordinator publishes its private listener with every
        // leader term, so commit notices from other replicas reach the leader.
        // A dedicated Forge worker holds dial-only credentials: it never
        // contends for a term, so its local advertise marker is never
        // published, and the same route carries its pulls and reports.
        let coordinator = Arc::new(match &peer_tls {
            Some(tls) => coordinator
                .with_leader_peer(ForgeLeaderPeer::new(advertise_addr.clone(), tls.clone())),
            None => coordinator,
        });
        // Only the Forge worker role runs admitted compaction plans, so only it
        // builds the dedicated executor. Its thread count is the resolved
        // effective CPU the same plan derived the memory budget from, rather
        // than a second CPU knob that could disagree with it.
        let worker = if roles.contains(&BifrostRuntimeRole::ForgeWorker) {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(resource_plan.effective_cpu)
                .thread_name_fn(|| {
                    static THREAD_INDEX: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    format!(
                        "wyrd-forge-compaction-{}",
                        THREAD_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    )
                })
                .enable_all()
                .build()
                .map_err(|error| {
                    ServerBootError::Forge(ForgeError::InvalidConfig {
                        detail: format!("Forge compaction runtime failed: {error}"),
                    })
                })?;
            compaction_runtime = ForgeCompactionRuntime::new(Some(runtime));
            let handle = compaction_runtime
                .handle()
                .expect("the owner was just constructed around a live runtime");
            Some(Arc::new(
                ForgeWorker::new(
                    Arc::clone(&coordinator),
                    forge_compaction_worker_config(&resource_plan, &forge_runtime),
                    node_id.as_uuid(),
                )
                .map_err(ServerBootError::Forge)?
                .on_compaction_runtime(handle),
            ))
        } else {
            None
        };
        Some(Arc::new(Forge::new(
            roles
                .contains(&BifrostRuntimeRole::ForgeCoordinator)
                .then_some(coordinator),
            worker,
            shutdown.clone(),
            node_id,
        )))
    } else {
        None
    };

    // The one process audit outbox: Gate, Oracle, peer security, and every
    // request-path decision stage on it; `BoundServer::run` drains it last.
    let audit_outbox = AuditSink::outbox(postgres.vala().clone());
    // The one process Scribe outbox; its route is bound once the roles exist.
    let (scribe_outbox, scribe_route) = crate::scribe_outbox::ScribeSink::outbox();
    let scribe = if let Some(parts) = scribe {
        let fragment_security_audit = Arc::new(
            crate::oracle::PostgresPeerSecurityAudit::try_new(&postgres, Arc::clone(&audit_outbox))
                .await
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        let fragment_authority = Arc::new(crate::oracle::OraclePeerAuthority::new(
            fragment_security_audit.clone(),
        ));
        Some(Arc::new(Scribe::new(crate::state::ScribeBuildInputs {
            ingest: parts.scribe,
            catalog: Arc::clone(&bifrost),
            resources: bifrost_resources.scribe().ok_or_else(|| {
                ServerBootError::Scribe(
                    "selected Scribe role has no root-derived resource capability".to_owned(),
                )
            })?,
            cluster: Arc::clone(&cluster_registry),
            registered_role: parts.scribe_role,
            fragment_verifier: fragment_authority,
            fragment_security_audit,
            role_shutdown: shutdown.clone(),
            activated: peer_tls.is_none(),
        })))
    } else {
        None
    };
    let oracle = OracleRoleBuilder {
        postgres: Arc::clone(&postgres),
        catalog: Arc::clone(&bifrost),
        resources: bifrost_resources.clone(),
        config: &bifrost_config,
        target,
        deployment_profile,
        cluster: Arc::clone(&cluster_registry),
        node_id,
        advertise_addr: &advertise_addr,
        peer_tls: peer_tls.clone(),
        local_scribe: scribe.clone(),
        audit: Arc::clone(&audit_outbox),
        shutdown: shutdown.clone(),
    }
    .build()
    .await?;
    let forwarding_audit = Arc::new(
        PostgresPeerSecurityAudit::try_new(&postgres, Arc::clone(&audit_outbox))
            .await
            .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
    );
    let forwarding_authority = Arc::new(OraclePeerAuthority::new(forwarding_audit));
    let query_controls = oracle.as_ref().map_or_else(
        || {
            crate::oracle::RunningQueryControls::new(
                None,
                Arc::new(crate::oracle::OracleLifecycleTransport::new(
                    Arc::clone(&cluster_registry),
                    node_id,
                    peer_tls.clone(),
                )),
                Arc::clone(&cluster_registry),
            )
        },
        |runtime| runtime.query_controls().clone(),
    );
    let query_forwarder = Arc::new(crate::oracle::ReadyOracleForwarder::new(
        crate::oracle::ReadyOracleForwarderInputs {
            cluster: Arc::clone(&cluster_registry),
            local_oracle: oracle.as_ref().map(|runtime| Arc::clone(runtime.engine())),
            local_node_id: node_id,
            local_fence: oracle
                .as_ref()
                .map(|runtime| runtime.registered_role().fencing_token),
            tls: peer_tls,
            authority: forwarding_authority,
            config: OracleConfig {
                default_deadline: bifrost_config.oracle.default_query_deadline(),
                ..OracleConfig::default()
            },
        },
    ));
    let interceptor =
        vala_bifrost_redux::gate::auth::ingest_auth_interceptor(Arc::clone(&token_verifier));
    let ingest_limits = scribe_config.ingest_limits();
    let observation_runs =
        crate::verification::observations::ObservationRunSink::outbox(postgres.wyrd().clone());
    let gate = match scribe.as_ref() {
        Some(runtime) => vala_bifrost_redux::gate::Gate::with_scribe(
            Arc::clone(runtime.ingest()),
            interceptor,
            ingest_limits,
        ),
        None => vala_bifrost_redux::gate::Gate::without_scribe(interceptor, ingest_limits),
    }
    .with_query_dispatch(
        Arc::clone(&query_forwarder) as Arc<dyn vala_bifrost_redux::contracts::OracleQueryDispatch>
    )
    .with_audit(Arc::clone(&audit_outbox))
    .with_observation_ack(Arc::new(
        crate::verification::observations::ObservationEnqueue::new(Arc::clone(&observation_runs)),
    ))
    .with_card_registry(vala_bifrost_redux::gate::attribution::CardRegistry::new(
        postgres.wyrd().clone(),
    ));
    Ok(crate::state::ComposedBifrost {
        bifrost: crate::state::Bifrost::assembled(crate::state::BifrostComposition {
            gate,
            scribe,
            forge,
            oracle,
            bifrost_storage,
            transport: bifrost_resources.transport_admission(),
            token_verifier,
            query_forwarder: Some(query_forwarder),
            query_controls: Some(query_controls),
            observation_runs,
            audit_outbox,
            scribe_outbox,
            scribe_route,
            #[cfg(feature = "test-support")]
            resources: Some(bifrost_resources.clone()),
        }),
        coordination_runtime,
        compaction_runtime,
        data_root,
    })
}

/// Derives one Forge worker's local compaction-admission bounds.
///
/// Every bound comes from values the immutable [`ResourcePlan`] already
/// resolved, so a node cannot admit compaction work its own resource plan did
/// not reserve. Memory is not bounded here: each rewrite charges the shared
/// governor through its own pool. Running parallelism is twelve units per
/// effective CPU, `RisingWave`'s Iceberg-mode compactor multiplier over its
/// worker threads (`ceil(effective_cpu × 12)`), and waiting parallelism is
/// four times that, so a burst of planned work queues rather than being refused
/// while earlier plans still run. Tenant fairness is unrelated to either and
/// stays with the SQL fair claim.
fn forge_compaction_worker_config(
    plan: &ResourcePlan,
    forge_runtime: &ForgeRuntimeConfig,
) -> ForgeWorkerConfig {
    let max_task_parallelism = u32::try_from(plan.effective_cpu.saturating_mul(12))
        .unwrap_or(u32::MAX)
        .max(1);
    ForgeWorkerConfig {
        per_tenant_active_cap: forge_runtime.resolved_per_tenant_active_cap(),
        max_task_parallelism,
        pending_task_parallelism: max_task_parallelism.saturating_mul(4),
        ..ForgeWorkerConfig::default()
    }
}

/// Build the bounded Forge worker future shared by embedded and worker roles.
///
/// The bounded worker's local compaction parallelism comes from its own
/// admission queue, sized once at composition from the immutable resource plan.
/// This entry point only supervises the resulting single event loop.
///
/// The process supervisor calls this again to replace a failed instance. Every
/// call clones the one boot-composed worker, so the replacement carries the
/// same boot-resolved owner identity a process restart would, and its startup
/// recovery reclaims lapsed claims and reconciles Prepared attempts exactly as
/// after a restart. The failed instance cannot act again: its loop has
/// returned only after joining its own plan runners and claim heartbeats, a
/// same-owner table-lease acquisition bumps the fencing token so any lease it
/// still held can no longer renew or commit, and a reclaimed claim settles
/// only under its new attempt.
///
/// # Errors
/// Returns [`ServerBootError::ForgeSchedulerRequired`] when Forge is absent or
/// [`ServerBootError::Forge`] when the requested worker bound or per-tenant cap
/// is invalid.
pub fn spawn_forge_worker(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<impl Future<Output = Result<(), ForgeError>> + Send + 'static + use<>, ServerBootError>
{
    let forge = state
        .bifrost
        .forge()
        .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
            detail: "Bifrost target has no retained Forge worker".to_owned(),
        })?;
    let worker =
        forge
            .worker()
            .cloned()
            .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
                detail: "Bifrost target has no retained Forge worker".to_owned(),
            })?;
    // The readiness handle is the state's, so `/readyz` observes the loop's
    // current bit rather than a value copied at boot.
    let readiness = forge.worker_readiness();
    Ok(async move { worker.as_ref().clone().run(shutdown, readiness).await })
}

/// One booted server state paired with the coordination-runtime owner it needs.
///
/// [`build_state`] produces two values with different ownership rules: the
/// cloneable [`AppState`] handed to every route, and the single non-`Clone`
/// [`ScribeCoordinationRuntime`]. Returning them as one named value keeps the
/// caller from publishing the state while silently dropping the executor that
/// runs the Scribe shard lanes behind it.
pub struct BootedServer {
    /// Process-wide handle registry published to routes and lifecycle owners.
    pub state: AppState,
    /// Sole owner of the dedicated Scribe coordination runtime.
    ///
    /// Must be dropped only after the Bifrost graph has drained — in the server
    /// process that is after `BoundServer::run` returns.
    pub coordination_runtime: ScribeCoordinationRuntime,
    /// Sole owner of the dedicated Forge compaction runtime.
    ///
    /// Must be dropped only after Forge worker supervision drains, so an
    /// admitted plan runner is never abandoned between writing its outputs and
    /// Preparing its operation.
    pub compaction_runtime: ForgeCompactionRuntime,
    /// Exclusive owner of the local Bifrost data root.
    ///
    /// Held for the life of the process so no second replica can open this
    /// node's WAL identity while it runs.
    pub data_root: BifrostDataRoot,
}

/// Emit pre-telemetry warnings for relaxed config that is still safe to run.
///
/// Production-profile rejection lives in `WyrdServerConfig::validate()` and runs
/// before this function. By the time we reach `production_guards`, any violating
/// production config has already returned `ConfigError::Invalid`. This function
/// only surfaces development-profile warnings for the same signals so an operator
/// running a relaxed dev profile sees them on stderr.
pub fn production_guards(config: &WyrdServerConfig) {
    if config.deployment_profile.is_production() {
        return;
    }
    if config.grpc.reflection_enabled {
        tracing::warn!("grpc.reflection_enabled=true in development profile");
    }
}

/// Assemble production `AppState` from config, applying `overrides` before
/// production validation.
///
/// Creates the shared shutdown `CancellationToken` internally and stores it in
/// the returned state. The gRPC health reporter is left as the `AppState::new`
/// default; `WyrdServer::new` replaces it with the reporter paired to the
/// health service it mounts.
///
/// Returns the composed [`AppState`] together with the single
/// [`ScribeCoordinationRuntime`] owner produced by [`compose_bifrost`]. The
/// caller must retain that owner for at least as long as it runs the server:
/// dropping it early releases the executor hosting the live Scribe shard lanes.
///
/// # Errors
/// Returns [`ServerBootError`] on database/storage/bifrost boot, auth handle
/// construction, federation seeding, or production validation failure.
pub async fn build_state(
    config: &WyrdServerConfig,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
    overrides: StateOverrides,
) -> Result<BootedServer, ServerBootError> {
    let shutdown = CancellationToken::new();
    let dsns = ResolvedDsns::from_env()?;
    let external = build_bifrost_external_dependencies(&dsns, &config.bifrost, config.role).await?;
    let sealing_key = build_sealing_keyring(config)?;
    let signing_key = resolve_signing_key(config)?;
    let auth = install_auth(
        external.postgres.as_ref(),
        config,
        &signing_key,
        sealing_key.clone(),
    )?;
    let verifier = auth
        .token_verifier
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires a token verifier".to_owned()))?;
    let peer_tls = build_bifrost_peer_tls(&config.bifrost.peer)?;
    let crate::state::ComposedBifrost {
        bifrost,
        coordination_runtime,
        compaction_runtime,
        data_root,
    } = compose_bifrost(crate::state::BifrostBuildInputs {
        target: config.role,
        deployment_profile: config.deployment_profile,
        postgres: external.postgres.as_ref().clone(),
        storage: Arc::clone(&external.storage),
        bifrost_storage: Arc::clone(&external.bifrost_storage),
        catalog: Arc::clone(&external.catalog),
        resources: external.resources,
        cluster: Arc::clone(&external.cluster),
        token_verifier: verifier,
        peer_tls,
        config: config.bifrost.clone(),
        forge_config: config.forge,
        node_id: external.node_id,
        advertise_addr: external.advertise_addr,
        data_root: external.data_root,
        shutdown: shutdown.clone(),
        #[cfg(feature = "test-support")]
        test_controls: None,
    })
    .await?;
    #[cfg(feature = "test-support")]
    if overrides.fail_after_scribe_activation {
        bifrost.abort().await;
        return Err(ServerBootError::Scribe(
            "test-injected failure after Scribe activation".to_owned(),
        ));
    }
    let state = AppState::new(
        external.postgres,
        external.storage,
        bifrost,
        shutdown.clone(),
    )
    .with_auth(auth);
    let state = attach_human_logins(state, config, sealing_key.clone());
    let state = attach_config_fields(state, config, telemetry)?;
    if let Err(error) = seed_federation(&state, config, sealing_key.as_deref()).await {
        rollback_state_roles(&state).await;
        return Err(error);
    }
    if let Err(error) =
        rewrap_sealed_secrets(state.postgres.operator_pool(), sealing_key.clone()).await
    {
        rollback_state_roles(&state).await;
        return Err(error);
    }

    let state = apply_overrides(state, overrides);

    if let Err(error) = state.production_validate() {
        rollback_state_roles(&state).await;
        return Err(ServerBootError::ProductionValidation(error));
    }
    if let Err(error) = verify_operator_keys(&state, config).await {
        rollback_state_roles(&state).await;
        return Err(error);
    }
    Ok(BootedServer {
        state,
        coordination_runtime,
        compaction_runtime,
        data_root,
    })
}

/// Tear down role-owned state after server-only boot stages fail.
///
/// Oracle and Scribe fences are activated before federation and production
/// validation. This helper reuses their normal owner/registry shutdown phases
/// with an already-expired deadline, ensuring cancellation and retained-task
/// abort happen without granting a second cleanup budget.
async fn rollback_state_roles(state: &AppState) {
    let deadline = std::time::Instant::now();
    state.bifrost.begin_shutdown();
    if state.bifrost.shutdown(deadline).await.is_err() {
        state.bifrost.abort().await;
    }
}

/// Resolves the north-south Wyrd workload signing authority used by auth.
///
/// Development may generate an ephemeral key so the default mixed-role server
/// still issues real tokens. Production requires configured key material and
/// never falls back to an ephemeral authority. This key never authorizes a
/// Bifrost peer operation; peers are admitted by the cluster mTLS identity.
///
/// # Errors
///
/// Returns [`ServerBootError::SigningKey`] when production has no configured
/// key or development cannot generate an ephemeral Ed25519 key.
fn resolve_signing_key(config: &WyrdServerConfig) -> Result<SecretString, ServerBootError> {
    if let Some(signing_key) = &config.auth.signing_key {
        return Ok(signing_key.clone());
    }
    if config.deployment_profile.is_production() {
        return Err(ServerBootError::SigningKey(
            "no signing key configured (set WYRD_SIGNING_KEY_FILE or WYRD_SIGNING_KEY_PEM)"
                .to_owned(),
        ));
    }
    let ephemeral = wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;
    tracing::warn!(
        "APP_ENV=development and no signing key configured; generated an EPHEMERAL \
         workload signing key. Tokens will not survive a restart and this key \
         must never be used in production."
    );
    Ok(ephemeral)
}

/// Refuse startup of a multi-tenant production API server unless every
/// active provisioned tenant's active Operator key reads and decodes.
///
/// Development and explicitly single-tenant (`auth.tenant_slug`) deployments
/// keep their deferred failure: a missing key refuses only credential writes
/// and deliveries.
///
/// # Errors
/// Returns [`ServerBootError::OperatorKeys`] when the tenant directory is
/// unavailable or any tenant's active key is unavailable, missing, malformed,
/// or not 32 bytes.
pub async fn verify_operator_keys(
    state: &AppState,
    config: &WyrdServerConfig,
) -> Result<(), ServerBootError> {
    if !(config.role.serves_api()
        && config.deployment_profile.is_production()
        && config.auth.tenant_slug.is_none())
    {
        return Ok(());
    }
    let keys = &state.operator_keys;
    let directory = state
        .postgres
        .operator_pool()
        .ok_or_else(|| keys.unavailable(keys.active_version(), KeyFailure::Database))?;
    let tenants = keys.verify_active(&directory).await?;
    tracing::info!(
        tenants,
        key_version = keys.active_version(),
        "active Operator tenant keys verified"
    );
    Ok(())
}

/// Apply caller overrides to a built state. Factored out for unit testing
/// without a live DB boot.
fn apply_overrides(state: AppState, overrides: StateOverrides) -> AppState {
    let mut state = state;
    if let Some(authz) = overrides.authz {
        state = state.with_authz(authz);
    }
    state
}

/// Attach config-derived fields to core state: shutdown token, telemetry,
/// limits, gateway configuration, and the gateway engine whose HTTP provider
/// dispatch screens endpoints under the deployment profile. Pure/sync (no I/O).
fn attach_config_fields(
    state: AppState,
    config: &WyrdServerConfig,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
) -> Result<AppState, ServerBootError> {
    let gateway_secret_keys = Arc::new(
        config
            .gateway
            .managed_secret_keys()
            .map_err(|error| ServerBootError::GatewayTransport(error.to_string()))?,
    );
    Ok(state
        .with_deployment_profile(config.deployment_profile)
        .with_operator_keys(OperatorKeys::for_role(
            config.role,
            &config.verification.operator_keys,
        )?)
        .with_telemetry(telemetry)
        .with_limits(config.limits.into_state())
        .with_workflow_config(config.workflow.clone())
        .with_gateway(config.gateway.clone())
        .with_gateway_secret_keys(Arc::clone(&gateway_secret_keys))
        .with_gateway_engine(wyrd_gateway::GatewayEngine::new(
            config
                .gateway
                .credential_resolver(gateway_secret_keys)
                .map_err(|error| ServerBootError::GatewayTransport(error.to_string()))?,
            wyrd_gateway::DeploymentHealth::default(),
            Arc::new(
                wyrd_gateway::HttpProviderDispatch::new(
                    skald_providers::EndpointPolicy::new(config.deployment_profile.is_production()),
                    wyrd_gateway::BuiltinEndpoints::default(),
                )
                .map_err(|error| ServerBootError::GatewayTransport(error.to_string()))?,
            ),
        )))
}

/// Reseal every stored provider and workload-issuer client secret under the current
/// sealing write key, or prove a keyless deployment stores none.
///
/// This is the rewrap step of sealing-key rotation: with the new key configured
/// as the write key and the old one retained, every boot converges stored
/// ciphertext onto the write key, and the logged `remaining` count — from a
/// pass that started after every writer moved to the new key — tells the
/// operator when the old key may be retired. With a key, a failure is logged
/// and left for the next boot: retained keys still open everything, so serving
/// is unaffected. Without a key, the same pass counts every stored ciphertext
/// as unopenable, and any count fails boot before readiness, because the
/// server could never open those secrets. Production boot and the test
/// harness both call this, so they behave identically. Skipped when no
/// cross-tenant operator pool is configured, since nothing can be read.
///
/// # Errors
/// Returns [`ServerBootError::SealingKey`] when no sealing key is configured
/// and any provider or workload-issuer ciphertext is stored, or the store
/// cannot be read to prove there is none.
pub async fn rewrap_sealed_secrets(
    operator: Option<OperatorPool>,
    keyring: Option<Arc<SealingKeyring>>,
) -> Result<(), ServerBootError> {
    let Some(operator) = operator else {
        return Ok(());
    };
    let keyless = keyring.is_none();
    match SealedSecretRewrap::new(operator, keyring).run().await {
        Ok(report) if keyless && report.remaining > 0 => Err(ServerBootError::SealingKey(format!(
            "{} stored sealed secret(s) (provider or workload-issuer client \
                 secrets) exist but no sealing key is configured; configure the key \
                 they were sealed under",
            report.remaining
        ))),
        Ok(_) => Ok(()),
        Err(error) if keyless => Err(ServerBootError::SealingKey(format!(
            "stored sealed secrets could not be checked with no sealing key \
             configured: {error}"
        ))),
        Err(error) => {
            tracing::error!(error = %error, "sealed secret rewrap failed; retrying next boot");
            Ok(())
        }
    }
}

/// Install Wyrd's own auth handles: build resolvers, construct issuing key +
/// verifier (fails closed in production without a key), and attach via
/// `with_auth`.
///
/// # Errors
/// Returns [`ServerBootError::SigningKey`] when production profile lacks a
/// signing key, or when key material is invalid.
fn install_auth(
    postgres: &ServerPostgres,
    config: &WyrdServerConfig,
    signing_key: &SecretString,
    sealing_key: Option<Arc<SealingKeyring>>,
) -> Result<ServerAuth, ServerBootError> {
    // Postgres is the single source of issuer/binding resolution. Both resolvers
    // are always attached; an empty config simply means the tenant federates no
    // issuers and binds no workloads, which they resolve as empty results. The
    // issuer resolver also feeds the issuance-side external (foreign-OIDC)
    // verifier so federated tokens can be exchanged for Wyrd tokens.
    let issuer_resolver = Arc::new(PgIssuerResolver::new(
        postgres.wyrd().clone(),
        sealing_key.clone(),
    ));
    let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(postgres.wyrd().clone()));

    let handles = crate::boot::auth::build_auth_handles(
        signing_key,
        Arc::clone(&issuer_resolver),
        config.deployment_profile.screened_http(),
    )?;

    let token_exchange_settings = wyrd_auth::issuance::TokenExchangeSettings::default();

    Ok(ServerAuth {
        issuing_key: Some(handles.issuing_key),
        token_verifier: Some(handles.token_verifier),
        external_verifier: Some(handles.external_verifier),
        trusted_issuer_resolver: Some(Arc::clone(&issuer_resolver)),
        workload_binding_resolver: Some(binding_resolver),
        human_connections: None,
        platform_login: None,
        oauth_clients: OAuthClients::new(config.auth.ui_client_secret_hashes.clone()),
        sealing_key: sealing_key.clone(),
        token_exchange_settings,
    })
}

/// Attach the human and platform login owners to `state`'s auth handles.
///
/// Both owners stage their decisions on the process audit outbox, which
/// exists only once [`AppState::new`] has adopted the one `compose_bifrost`
/// built, so they are attached here rather than in [`install_auth`]. One
/// owner of each serves every login, so its provider cache outlives a
/// request. Platform login needs the cross-tenant boundary and an issuing
/// key; without either it stays unset.
fn attach_human_logins(
    mut state: AppState,
    config: &WyrdServerConfig,
    sealing_key: Option<Arc<SealingKeyring>>,
) -> AppState {
    state.auth.human_connections = Some(HumanConnections::new(
        state.postgres.wyrd().clone(),
        sealing_key.clone(),
        config.deployment_profile.screened_http(),
        config.auth.public_origin.as_ref(),
        Arc::clone(&state.audit_outbox),
    ));
    state.auth.platform_login = state
        .postgres
        .operator_pool()
        .zip(state.auth.issuing_key.clone())
        .map(|(pool, issuing_key)| {
            PlatformLogin::new(
                pool.clone(),
                sealing_key,
                Arc::new(PlatformSessions::new(
                    pool,
                    issuing_key,
                    Arc::clone(&state.audit_outbox),
                )),
                config.deployment_profile.screened_http(),
            )
        });
    state
}

/// Builds and registers the local Oracle role after security dependencies verify.
///
/// The exact system sentinel and signing authority are checked before the
/// Oracle role advertises readiness. Development and production use the exact
/// same signing authority already selected for the server auth surface.
///
/// # Errors
///
/// Returns [`ServerBootError::OraclePeer`] when the sentinel, signing key,
/// memory owner, Redux storage adapter, role registration, or snapshot refresh
/// is unavailable. No peer service is attached on partial construction.
/// Owns Oracle construction, activation, publication, and rollback for one boot.
///
/// All dependencies are retained by this owner until activation succeeds, so
/// every partial failure follows the same fenced-role cleanup path.
struct OracleRoleBuilder<'a> {
    /// Shared production PostgreSQL handles.
    postgres: Arc<ServerPostgres>,
    /// Shared Bifrost catalog used by persisted providers.
    catalog: Arc<BifrostCatalog>,
    /// Root-derived selected-role resource graph.
    resources: BifrostRoleResources,
    /// Validated Bifrost configuration borrowed for this activation.
    config: &'a crate::config::BifrostRuntimeConfig,
    /// Closed process target controlling Oracle selection.
    target: crate::config::BifrostTarget,
    /// Deployment posture used for remote TLS validation.
    deployment_profile: crate::config::DeploymentProfile,
    /// Cluster registry owning the role fence and readiness publication.
    cluster: Arc<ClusterRegistry>,
    /// Stable node identity advertised to peer services.
    node_id: ClusterNodeId,
    /// Bound endpoint published in cluster membership.
    advertise_addr: &'a str,
    /// Cluster mTLS identity; present only in peer mode, where it is the sole
    /// trust boundary for every private call this Oracle sends or receives.
    peer_tls: Option<BifrostPeerTls>,
    /// Co-located Scribe a process-local Oracle lists and reads in-process.
    local_scribe: Option<Arc<crate::state::Scribe>>,
    /// Process audit outbox for the leader's read decisions and tenant refusals.
    audit: Arc<AuditOutbox>,
    /// One process-wide shutdown token injected into every Oracle owner.
    shutdown: CancellationToken,
}

impl OracleRoleBuilder<'_> {
    /// Constructs the combined fenced API-serving follower without publishing a partial peer.
    ///
    /// # Errors
    ///
    /// Returns a server boot error when the configured closed role set cannot
    /// be constructed, fenced, reconciled, activated, and published.
    async fn build(self) -> Result<Option<Arc<Oracle>>, ServerBootError> {
        if !crate::config::BifrostRoles::for_target(self.target)
            .contains(&BifrostRuntimeRole::Oracle)
        {
            return Ok(None);
        }
        // A peer-mode Oracle stays reserved but unready until its private
        // listener serves; the serving owner activates it then.
        let activate = self.peer_tls.is_none();
        let built = self.construct().await?;
        built
            .start_reconcile_activate_publish(activate)
            .await
            .map(Some)
    }

    /// Constructs one Oracle role and retains its reserved fence.
    ///
    /// # Errors
    /// Returns [`ServerBootError::OraclePeer`] when security, dependency, or
    /// durable-role construction fails.
    async fn construct(self) -> Result<BuiltOracleRole, ServerBootError> {
        let Self {
            postgres,
            catalog,
            resources: roles,
            config,
            target: _,
            deployment_profile,
            cluster,
            node_id,
            advertise_addr,
            peer_tls,
            local_scribe,
            audit,
            shutdown,
        } = self;
        // The same immutable identity serves Scribe-tail discovery and the
        // Analytical east-west plane; naming it twice would let the two drift.
        let tail_tls = peer_tls.clone();
        let security_audit = Arc::new(
            PostgresPeerSecurityAudit::try_new(&postgres, Arc::clone(&audit))
                .await
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let authority = Arc::new(OraclePeerAuthority::new(security_audit.clone()));
        let resource_plan = roles.plan();
        let resources = roles.oracle().ok_or_else(|| {
            ServerBootError::OraclePeer(
                "Oracle role selected without a composed Oracle capability".to_owned(),
            )
        })?;
        let configured_cpu = resource_plan.effective_cpu as f64;
        let memory_bytes_per_slot = u64::try_from(
            vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
        )
        .map_err(|_| ServerBootError::OraclePeer("worker quantum exceeds u64".to_owned()))?;
        // Derive Oracle capability sizing from the portable resource plan.
        let memory_budget_bytes = u64::try_from(resource_plan.managed_memory_bytes)
            .map_err(|_| ServerBootError::OraclePeer("memory budget exceeds u64".to_owned()))?;
        let raw_slots = u32::try_from(
            vala_bifrost_redux::resources::oracle_worker_slots(resource_plan)
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        )
        .map_err(|_| ServerBootError::OraclePeer("Oracle slot count exceeds u32".to_owned()))?;
        let calibrated =
            crate::config::load_oracle_admission_translation(&config.oracle, raw_slots)
                .map_err(ServerBootError::OraclePeer)?;
        // Without an approved calibration profile the split is derived from the
        // same raw units the capability advertises, so a pod's local capacity is
        // never a number an operator has to reconcile by hand.
        let default_split = OracleClassSplit::derive(raw_slots);
        let oracle_config = OracleConfig {
            max_workers_per_query: config.oracle.max_workers_per_query,
            attempt_memory_bytes: config.oracle.max_frame_bytes.min(8 * 1024 * 1024),
            interactive_slots: calibrated
                .as_ref()
                .map_or(default_split.interactive_floor_units, |value| {
                    value.interactive_slots
                }),
            analytical_slots: calibrated
                .as_ref()
                .map_or(default_split.analytical_max_units, |value| {
                    value.analytical_slots
                }),
            tenant_interactive_slots: calibrated
                .as_ref()
                .map_or(default_split.total_units(), |value| {
                    value.tenant_interactive_slots
                }),
            tenant_analytical_slots: calibrated
                .as_ref()
                .map_or(default_split.analytical_max_units, |value| {
                    value.tenant_analytical_slots
                }),
            queue_capacity: calibrated.as_ref().map_or(
                u32::try_from(config.oracle.admission_waiters).map_err(|_| {
                    ServerBootError::OraclePeer("Oracle queue capacity exceeds u32".to_owned())
                })?,
                |value| value.queue_capacity,
            ),
            max_queue_wait: calibrated.as_ref().map_or(
                Duration::from_millis(config.oracle.max_queue_wait_ms),
                |value| value.max_queue_wait,
            ),
            default_deadline: config.oracle.default_query_deadline(),
            ..OracleConfig::default()
        };
        let capabilities = OracleCapabilitiesV1 {
            storage_protocol_version: 1,
            cpu_cores: configured_cpu,
            memory_budget_bytes,
            cpu_cores_per_slot: 1.0,
            memory_bytes_per_slot,
            raw_slots,
            usable_slots: raw_slots,
            supported_classes: oracle_supported_classes(oracle_config.analytical_slots),
            max_workers_per_query: u32::try_from(config.oracle.max_workers_per_query)
                .map_err(|_| ServerBootError::OraclePeer("worker fanout exceeds u32".to_owned()))?,
        };
        // Publish the resolved split on the shared process root before any
        // query or follower can charge it, so leader admission and follower
        // acquisition enforce one local capacity rather than two derivations.
        let split = resources.install_class_split(OracleClassSplit {
            interactive_floor_units: oracle_config.interactive_slots,
            analytical_max_units: oracle_config.analytical_slots,
        });
        let running_slots = usize::try_from(split.total_units()).map_err(|_| {
            ServerBootError::OraclePeer("Oracle slot count exceeds usize".to_owned())
        })?;
        // Query concurrency is the single number that decides whether this node
        // serves or queues, and it is derived rather than configured. An
        // operator diagnosing query latency needs it at startup, not inferred
        // from a saturation warning under load.
        tracing::info!(
            running_slots,
            interactive_floor_units = split.interactive_floor_units,
            analytical_max_units = split.analytical_max_units,
            configured = resource_plan.oracle_query_slot_limit.is_some(),
            admission_waiters = config.oracle.admission_waiters,
            effective_cpu = resource_plan.effective_cpu,
            managed_memory_bytes = resource_plan.managed_memory_bytes,
            "Oracle admission capacity resolved"
        );
        let reservations = Arc::new(ReservationRegistry::new(running_slots, 1_024));
        let snapshot = cluster.snapshot();
        validate_remote_oracle_addresses(
            deployment_profile,
            snapshot
                .live_oracles()
                .into_iter()
                .filter(|lease| lease.key.node_id != node_id)
                .map(|lease| lease.address.as_str()),
        )?;
        // Remote Oracle peers exist only in peer mode, which always carries
        // the cluster mTLS identity; the local `all` target dispatches in-process.
        let remote_transport = tail_tls.clone().map(|tls| {
            Arc::new(TonicOraclePeerTransport::with_tls(
                Arc::clone(&cluster),
                tls,
            ))
        });
        let lifecycle_transport = Arc::new(crate::oracle::OracleLifecycleTransport::new(
            Arc::clone(&cluster),
            node_id,
            tail_tls.clone(),
        ));
        let reconciliation_limit_bytes = resource_plan
            .managed_memory_bytes
            .checked_div(4)
            .filter(|limit| *limit > 0)
            .ok_or_else(|| {
                ServerBootError::OraclePeer("Oracle reconciliation budget is zero".to_owned())
            })?;
        let tail_discovery = Arc::new(crate::oracle::RegistryTailStreamDiscovery::new(
            tail_tls,
            local_scribe.as_ref().map(|scribe| scribe.tail_service()),
        ));
        let stage_authority: Arc<dyn vala_bifrost_redux::oracle::peer::OracleStageAuthority> =
            authority.clone();
        let role = cluster
            .reserve_oracle(advertise_addr, capabilities)
            .await
            .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?;
        let oracle_resources = roles.oracle().ok_or_else(|| {
            ServerBootError::OraclePeer(
                "Oracle peer role requires root resource capability".to_owned(),
            )
        })?;
        let worker = Arc::new(OraclePeerWorker::new(
            Arc::clone(&reservations),
            oracle_resources.clone(),
        ));
        let peer = Arc::new(crate::oracle::OraclePeerRuntime::new(
            Arc::clone(&worker),
            Arc::clone(&security_audit),
            lifecycle_transport,
            Arc::clone(&authority),
        ));
        let peer_transports = Arc::new(OraclePeerTransportDirectory::new(
            node_id,
            remote_transport,
            local_scribe.map(|scribe| {
                Arc::new(crate::oracle::ScribeFragmentExecutor::new(scribe))
                    as Arc<dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerTransport>
            }),
        ));
        let oracle = match OracleEngine::new(OracleBuildConfig {
            shutdown: shutdown.clone(),
            catalog: Arc::clone(&catalog),
            vala: postgres.vala().clone(),
            cluster: Arc::clone(&cluster),
            local_role: role.clone(),
            memory: OracleMemoryResources {
                resources,
                reconciliation_limit_bytes,
            },
            audit,
            reservations,
            stage_authority: Some(stage_authority),
            peer_tls,
            tail_discovery: Some(tail_discovery),
            peer_transports: Some(peer_transports),
            config: oracle_config,
        }) {
            Ok(oracle) => Arc::new(oracle),
            Err(error) => {
                release_failed_oracle_role(&cluster, &role, "construction").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
        };
        Ok(BuiltOracleRole {
            catalog,
            oracle,
            role,
            peer,
            cluster,
            resources: oracle_resources,
            shutdown,
        })
    }
}

/// Rejects plaintext live remote Oracle membership before runtime publication.
///
/// # Errors
/// Returns [`ServerBootError::OraclePeer`] when production membership contains
/// any remote address without the `https://` scheme.
fn validate_remote_oracle_addresses<'a>(
    profile: crate::config::DeploymentProfile,
    addresses: impl Iterator<Item = &'a str>,
) -> Result<(), ServerBootError> {
    if profile.is_production()
        && let Some(address) = addresses
            .into_iter()
            .find(|address| !address.starts_with("https://"))
    {
        return Err(ServerBootError::OraclePeer(format!(
            "live remote Oracle membership address must use https://: {address}"
        )));
    }
    Ok(())
}

/// Fully constructed Oracle role waiting for lifecycle publication.
struct BuiltOracleRole {
    /// Shared catalog retained by the selected Oracle owner.
    catalog: Arc<BifrostCatalog>,
    /// Oracle engine awaiting startup reconciliation.
    oracle: Arc<OracleEngine>,
    /// Reserved cluster role fence.
    role: RegisteredRole,
    /// Oracle-owned private peer service attached after role activation.
    peer: Arc<crate::oracle::OraclePeerRuntime>,
    /// Cluster owner used for activation and snapshot publication.
    cluster: Arc<ClusterRegistry>,
    /// Root-derived Oracle resource capability.
    resources: vala_bifrost_redux::resources::OracleResources,
    /// One process-wide shutdown token retained through lifecycle publication.
    shutdown: CancellationToken,
}

impl BuiltOracleRole {
    /// Starts reconciliation, optionally activates the role, and publishes query access.
    ///
    /// With `activate` false the fence stays reserved but unready, so no peer
    /// routes to this Oracle before its private listener serves; the serving
    /// owner then calls [`Oracle::activate`]. One-process deployments with no
    /// private listener activate here.
    ///
    /// # Errors
    /// Returns [`ServerBootError::OraclePeer`] when startup reconciliation,
    /// activation, snapshot refresh, or readiness publication fails.
    async fn start_reconcile_activate_publish(
        self,
        activate: bool,
    ) -> Result<Arc<Oracle>, ServerBootError> {
        let Self {
            catalog,
            oracle,
            role,
            peer,
            cluster,
            resources,
            shutdown,
        } = self;
        match tokio::time::timeout(ORACLE_STARTUP_TIMEOUT, oracle.await_startup()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "startup reconciliation").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
            Err(_) => {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "startup reconciliation").await;
                return Err(ServerBootError::OraclePeer(
                    "Oracle startup admission reconciliation timed out".to_owned(),
                ));
            }
        }
        if activate {
            if let Err(error) = cluster.activate(&role).await {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "activation").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
            if let Err(error) = cluster.refresh_snapshot().await {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "snapshot refresh").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
            oracle.refresh_membership(&cluster.snapshot());
            if !oracle.is_ready() {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "readiness publication").await;
                return Err(ServerBootError::OraclePeer(
                    "Oracle role did not become ready after activation".to_owned(),
                ));
            }
        }
        let lifecycle_transport = peer.lifecycle_transport();
        let query_runtime = match Oracle::new(crate::state::OracleBuildInputs {
            engine: Arc::clone(&oracle),
            catalog,
            registered_role: role.clone(),
            cluster: Arc::clone(&cluster),
            lifecycle_transport,
            resources,
            peer,
            role_shutdown: shutdown,
            activated: activate,
        }) {
            Ok(runtime) => Arc::new(runtime),
            Err(error) => {
                oracle.shutdown(std::time::Instant::now()).await;
                release_failed_oracle_role(&cluster, &role, "lifecycle construction").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
        };
        Ok(query_runtime)
    }
}

/// Releases a reserved or active Oracle fence after partial boot failure.
async fn release_failed_oracle_role(
    cluster: &ClusterRegistry,
    role: &RegisteredRole,
    phase: &'static str,
) {
    if let Err(error) = cluster.shutdown_role(role.clone()).await {
        tracing::warn!(%error, phase, "failed to release Oracle fence after boot failure");
    }
}

/// Seed `[[trusted_issuers]]` and `[[workload_bindings]]` into Postgres under
/// the implicit tenant. Issuers are seeded first because workload bindings
/// reference them by FK.
async fn seed_federation(
    state: &AppState,
    config: &WyrdServerConfig,
    sealing_key: Option<&SealingKeyring>,
) -> Result<(), ServerBootError> {
    // Seed `[[trusted_issuers]]` and `[[workload_bindings]]` into Postgres under
    // the implicit tenant. Both resolve the slug through the same
    // `WyrdPostgres::resolve_tenant_slug` the request handlers use, so the bound
    // tenant matches request-time lookups by construction. Each binding's card
    // target is built into a server-owned `CardRef` (F04, never from token
    // claims); building the ref is not a card-existence check.
    if !config.trusted_issuers.is_empty() || !config.workload_bindings.is_empty() {
        let slug = config.auth.tenant_slug.as_ref().ok_or_else(|| {
            ServerBootError::TenantSlugUnresolved {
                slug: "(unset)".to_owned(),
            }
        })?;
        let tenant_id =
            crate::boot::issuer::resolve_implicit_tenant(state.postgres.wyrd(), slug).await?;

        crate::boot::issuer::seed_trusted_issuers(
            state.postgres.wyrd(),
            tenant_id,
            &config.trusted_issuers,
            sealing_key,
        )
        .await?;

        let bindings = build_workload_bindings(&config.workload_bindings, tenant_id)?;
        crate::boot::issuer::seed_workload_bindings(state.postgres.wyrd(), tenant_id, &bindings)
            .await?;
    }
    Ok(())
}

/// Build the deployment [`SealingKeyring`] from the configured write key and
/// retained keys.
///
/// Returns `Ok(None)` when no write key is configured; secret-free deployments
/// need none. Retained keys without a write key are refused, because nothing
/// could seal or rewrap under them.
///
/// # Errors
/// Returns [`ServerBootError::SealingKey`] when any configured key is not valid
/// base64 or does not decode to exactly 32 bytes, or when retained keys are set
/// without a write key.
fn build_sealing_keyring(
    config: &WyrdServerConfig,
) -> Result<Option<Arc<SealingKeyring>>, ServerBootError> {
    let Some(encoded) = config.auth.sealing_key.as_ref() else {
        if config.auth.sealing_retained_keys.is_empty() {
            return Ok(None);
        }
        return Err(ServerBootError::SealingKey(
            "retained sealing keys require a configured write sealing key".to_owned(),
        ));
    };
    let mut keyring = SealingKeyring::new(decode_sealing_key(encoded, "sealing key")?);
    for retained in &config.auth.sealing_retained_keys {
        keyring = keyring.with_retained(decode_sealing_key(retained, "retained sealing key")?);
    }
    tracing::info!(
        write_key_id = %keyring.write_key_id(),
        retained_key_ids = ?keyring.retained_key_ids(),
        "sealing keyring configured"
    );
    Ok(Some(Arc::new(keyring)))
}

/// Decode one base64 sealing key into a 32-byte AES-256-GCM key.
///
/// # Errors
/// Returns [`ServerBootError::SealingKey`] naming `label` (never the key) when
/// the value is not base64 or not 32 bytes.
fn decode_sealing_key(encoded: &SecretString, label: &str) -> Result<SecretKey, ServerBootError> {
    crate::components::operators::keys::decode_key(encoded.expose_secret()).map_err(|failure| {
        ServerBootError::SealingKey(format!(
            "{label} must be base64 of exactly 32 bytes: {}",
            failure.as_str()
        ))
    })
}

/// Map `[[workload_bindings]]` config entries to domain [`WorkloadBinding`]s, all
/// bound to the resolved implicit `tenant_id` (F4).
///
/// # Errors
/// Returns [`ServerBootError::InvalidWorkloadBinding`] when an entry's issuer URL
/// or card target cannot be parsed into the domain types.
pub fn build_workload_bindings(
    entries: &[WorkloadBindingEntry],
    tenant_id: DataTenantId,
) -> Result<Vec<WorkloadBinding>, ServerBootError> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| build_workload_binding(index, entry, tenant_id))
        .collect()
}

/// Build a single [`WorkloadBinding`] from a config entry under `tenant_id`.
fn build_workload_binding(
    index: usize,
    entry: &WorkloadBindingEntry,
    tenant_id: DataTenantId,
) -> Result<WorkloadBinding, ServerBootError> {
    let issuer = IssuerUrl::new(entry.issuer.clone()).map_err(|error| {
        ServerBootError::InvalidWorkloadBinding {
            index,
            message: format!("issuer URL is not a valid https issuer: {error}"),
        }
    })?;
    Ok(WorkloadBinding {
        tenant_id,
        issuer,
        subject: entry.subject.clone(),
        audience: entry.audience.clone(),
        card_ref: build_binding_card_ref(index, entry)?,
    })
}

/// Build the server-owned [`CardRef`] for a binding from its config card target.
///
/// `uid` is always `None` and no DB/existence lookup is performed (N2, F04):
/// building the ref is not validating that the card exists.
fn build_binding_card_ref(
    index: usize,
    entry: &WorkloadBindingEntry,
) -> Result<CardRef, ServerBootError> {
    let invalid = |message: String| ServerBootError::InvalidWorkloadBinding { index, message };

    let kind = CardKind::native()
        .into_iter()
        .find(|kind| kind.wire_name().eq_ignore_ascii_case(&entry.kind))
        .ok_or_else(|| invalid(format!("unknown card kind {:?}", entry.kind)))?;
    // Only Service and Agent cards back a workload principal; any other kind
    // parses but never resolves at jwt-bearer exchange. Fail fast at boot.
    if !matches!(kind, CardKind::Service | CardKind::Agent) {
        return Err(invalid(format!(
            "workload binding card kind must be \"service\" or \"agent\", got {:?}",
            entry.kind
        )));
    }
    let name = CardName::new(entry.name.clone())
        .map_err(|error| invalid(format!("invalid card name {:?}: {error}", entry.name)))?;
    let version = VersionBlock::parse(entry.version.clone())
        .map_err(|error| invalid(format!("invalid card version {:?}: {error}", entry.version)))?;
    let space = SpaceName::new(entry.space.clone())
        .map_err(|error| invalid(format!("invalid card space {:?}: {error}", entry.space)))?;

    Ok(CardRef {
        kind,
        name,
        version,
        space: Some(space),
        uid: None,
    })
}

/// Spawn the storage sweeper if enabled and the operator pool is available.
///
/// Returns `None` when the sweeper is disabled or the operator pool is absent.
///
/// # Errors
/// Returns [`wyrd_storage::StorageError`] when the sweeper config cannot be read
/// from the process environment.
pub fn spawn_storage_sweeper(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<Option<tokio::task::JoinHandle<()>>, wyrd_storage::StorageError> {
    let cfg = wyrd_storage::sweeper::SweeperConfig::from_env()?;
    if !cfg.enabled {
        tracing::info!("storage sweeper disabled via WYRD_STORAGE_SWEEPER_ENABLED=false");
        return Ok(None);
    }

    let Some(operator_pool) = state.postgres.operator_pool() else {
        tracing::warn!("storage sweeper skipped because operator pool is unavailable");
        return Ok(None);
    };

    let sweeper = wyrd_storage::sweeper::Sweeper::new(
        Arc::clone(&state.storage),
        operator_pool,
        state.postgres.wyrd().clone(),
        cfg,
        shutdown,
    );
    Ok(Some(tokio::spawn(async move { sweeper.run().await })))
}

/// Build the single supervised Redux Forge maintenance scheduler future.
///
/// The returned future is the actual scheduler loop. The server inserts it
/// directly into its supervised `JoinSet`, so an unexpected completion or
/// panic cannot be hidden behind a detached shutdown waiter.
///
/// # Errors
/// Returns [`ServerBootError::ForgeSchedulerRequired`] when the real server
/// state was not assembled with a Forge owner.
pub fn spawn_maintenance_scheduler(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<
    Option<impl Future<Output = Result<(), ForgeError>> + Send + 'static + use<>>,
    ServerBootError,
> {
    let Some(forge) = state.forge_handle().cloned() else {
        return Ok(None);
    };
    let readiness = state
        .bifrost
        .forge()
        .map(Forge::coordinator_readiness)
        .unwrap_or_default();
    Ok(Some(async move { forge.run(shutdown, readiness).await }))
}

/// Loads the outbound cluster mTLS identity when peer mode is enabled.
///
/// The default in-process deployment returns `None` and reads nothing. Peer
/// mode fails closed here: an unreadable bundle is a boot failure rather than
/// a transport that refuses every call later.
///
/// # Errors
///
/// Returns [`ServerBootError::OraclePeer`] when the peer TLS bundle cannot be read.
fn build_bifrost_peer_tls(
    peer: &crate::config::BifrostPeerConfig,
) -> Result<Option<BifrostPeerTls>, ServerBootError> {
    Ok(peer
        .read_bundle()
        .map_err(ServerBootError::OraclePeer)?
        .map(|bundle| {
            BifrostPeerTls::new(
                bundle.ca_certificate,
                crate::config::PEER_SERVER_NAME.to_owned(),
                bundle.certificate_chain,
                bundle.private_key,
            )
        }))
}

/// Ensure production Card recovery can run through the Wyrd operator pool.
pub fn check_card_recovery_pool(state: &AppState) -> Result<(), ServerBootError> {
    check_card_recovery_pool_inner(
        state.postgres.operator_pool().is_some(),
        state.deployment_profile,
    )
}

fn check_card_recovery_pool_inner(
    has_operator_pool: bool,
    profile: crate::config::DeploymentProfile,
) -> Result<(), ServerBootError> {
    if !has_operator_pool {
        if profile.is_production() {
            return Err(ServerBootError::CardRecoveryPoolRequired);
        }
        tracing::warn!(
            "Wyrd operator pool is not configured — Card recovery is disabled (dev/test only)"
        );
    }
    Ok(())
}

/// Returns the query classes this pod's own local split can actually admit.
///
/// Interactive is always served. Analytical is advertised only when the local
/// split reserves at least one Analytical slot, because a leader that
/// deterministically selects a replica advertising a class it always refuses
/// would fail a query a capable replica could have served.
fn oracle_supported_classes(analytical_slots: u32) -> Vec<QueryClass> {
    if analytical_slots > 0 {
        vec![QueryClass::Interactive, QueryClass::Analytical]
    } else {
        vec![QueryClass::Interactive]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A pod advertises Analytical only when its own split can admit one.
    ///
    /// # Panics
    ///
    /// Panics when a disabled split still advertises the class.
    #[test]
    fn oracle_advertises_only_locally_admissible_classes() {
        assert_eq!(
            oracle_supported_classes(0),
            vec![QueryClass::Interactive],
            "a split that disables Analytical must not advertise it"
        );
        assert_eq!(
            oracle_supported_classes(1),
            vec![QueryClass::Interactive, QueryClass::Analytical]
        );
    }

    /// The compaction runtime is role-scoped and sized from effective CPU.
    ///
    /// Only a process that actually runs admitted plans builds a dedicated
    /// executor, and its admission bounds follow the resolved effective CPU
    /// rather than a second knob that could disagree. Memory is not bounded
    /// here: every rewrite charges the one shared governor.
    ///
    /// # Panics
    ///
    /// Panics when a non-worker role builds an executor or the derived
    /// admission bounds do not follow effective CPU.
    #[test]
    fn forge_runtime_is_role_scoped_and_cpu_sized() {
        let mut plan = ResourcePlan {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            effective_cpu: 6,
            oracle_query_slot_limit: None,
            server_memory_min_bytes: 1024 * 1024 * 1024,
            managed_memory_bytes: 3 * 1024 * 1024 * 1024,
            scribe_enabled: false,
            oracle_enabled: false,
            forge_enabled: true,
            scratch_limit_bytes: 1024 * 1024 * 1024,
        };
        let forge_runtime = ForgeRuntimeConfig::default();
        let worker = super::forge_compaction_worker_config(&plan, &forge_runtime);
        assert_eq!(
            worker.max_task_parallelism, 72,
            "twelve per effective CPU, the Iceberg compactor multiplier"
        );
        assert_eq!(
            worker.pending_task_parallelism, 288,
            "four times running parallelism may wait"
        );
        assert_eq!(worker.per_tenant_active_cap, 1);
        worker
            .validate()
            .expect("composition-derived bounds are usable");
        plan.effective_cpu = 0;
        super::forge_compaction_worker_config(&plan, &forge_runtime)
            .validate()
            .expect("running parallelism never falls below one");

        // Only the Forge worker role reaches the executor construction at all.
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("boot module has production source before tests");
        assert_eq!(
            production
                .matches("compaction_runtime = ForgeCompactionRuntime::new(Some(runtime))")
                .count(),
            1,
            "exactly one composition site installs the dedicated executor"
        );
        let installed = production
            .split("compaction_runtime = ForgeCompactionRuntime::new(Some(runtime))")
            .next()
            .expect("source before the install site");
        assert!(
            installed
                .rsplit("if roles.contains(")
                .next()
                .is_some_and(|guard| guard.starts_with("&BifrostRuntimeRole::ForgeWorker)")),
            "the executor must be built only under the Forge worker role guard"
        );
    }

    /// Orphan listing pages a backend without a native cursor in ascending order.
    ///
    /// The filesystem service advertises no `list_with_start_after` and walks
    /// in directory order, so it takes the emulated path. Keys are written in
    /// reverse so walk order and key order disagree; the pages must still be
    /// bounded, globally ascending, and complete, and a resume from a mid-page
    /// cursor must yield exactly the keys after it.
    ///
    /// # Panics
    ///
    /// Panics when the operator cannot be built or written, when it starts
    /// advertising cursor support, or when any page is oversized, out of order,
    /// or skips or repeats a key.
    #[tokio::test]
    async fn open_dal_forge_listing_pages_backend_without_start_after() {
        let root = tempfile::tempdir().expect("listing root");
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(&root.path().to_string_lossy()),
        )
        .expect("filesystem operator builds")
        .finish();
        assert!(
            !operator.info().full_capability().list_with_start_after,
            "the filesystem service is the backend the emulated path exists for"
        );
        let prefix = "tenants/t/table/data/forge/v1/";
        let total = FORGE_OBJECT_LIST_PAGE_ENTRIES * 2 + 3;
        let mut keys = (0..total)
            .map(|index| format!("{prefix}{index:06}.parquet"))
            .collect::<Vec<_>>();
        for key in keys.iter().rev() {
            operator
                .write(key, "orphan")
                .await
                .expect("object is written");
        }
        keys.sort();
        let store = OpenDalForgeObjectStore::new(Arc::new(operator));

        for (cursor, expected) in [(None, &keys[..]), (Some(keys[700].as_str()), &keys[701..])] {
            let pages = store
                .list_pages(prefix, cursor)
                .await
                .expect("an emulated listing opens")
                .try_collect::<Vec<_>>()
                .await
                .expect("every emulated page lists");
            assert!(
                pages
                    .iter()
                    .all(|page| page.len() <= FORGE_OBJECT_LIST_PAGE_ENTRIES),
                "every page stays within the page bound"
            );
            let listed = pages
                .iter()
                .flatten()
                .map(|entry| entry.path().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                listed, expected,
                "pages are ascending, complete, and exclusive"
            );
        }
    }

    /// Directory entries never consume the page bound.
    ///
    /// The production page bound counts addressable objects, because the task
    /// cursor advances over objects only. A chunk of leading directory markers
    /// would otherwise yield an empty page and leave the frontier stationary,
    /// starving the later object behind it. The filesystem service is the
    /// installed backend that yields directory entries.
    ///
    /// # Panics
    ///
    /// Panics when the operator cannot be built, when the single page is not
    /// exactly the one object, or when the stream yields another page.
    #[tokio::test]
    async fn open_dal_forge_listing_filters_directories_before_page_boundary() {
        let root = tempfile::tempdir().expect("listing root");
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(&root.path().to_string_lossy()),
        )
        .expect("filesystem operator builds")
        .finish();
        let prefix = "tenants/t/table/data/forge/v1/";
        for index in 0..FORGE_OBJECT_LIST_PAGE_ENTRIES {
            operator
                .create_dir(&format!("{prefix}{index:06}/"))
                .await
                .expect("leading directory marker is created");
        }
        let object = format!("{prefix}zzzzzz.parquet");
        operator
            .write(&object, "orphan")
            .await
            .expect("trailing object is written");

        let store = OpenDalForgeObjectStore::new(Arc::new(operator));
        let mut pages = store
            .list_pages(prefix, None)
            .await
            .expect("the backend is listable");
        let page = pages
            .next()
            .await
            .expect("one object-only page is yielded")
            .expect("the page lists successfully");
        let paths = page
            .iter()
            .map(|entry| entry.path().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec![object], "the page holds only the object");
        assert!(
            pages.next().await.is_none(),
            "the object-only page is the whole listing"
        );
    }

    /// Boot rejects an expanded request above the cap while accepting its exact boundary.
    ///
    /// # Panics
    ///
    /// Panics if a cap one byte too small is accepted or the exact cap is refused.
    #[test]
    fn scribe_boot_rejects_expanded_request_above_cap() {
        let config = crate::config::ScribeRuntimeConfig::default();
        let required = config.ingest_limits().expanded_bytes();
        let error = validate_scribe_expanded_request(config, required - 1)
            .expect_err("an expanded request above the cap must fail boot");
        assert!(error.to_string().contains("configured expanded request"));
        validate_scribe_expanded_request(config, required)
            .expect("an expanded request equal to the cap must remain bootable");
    }

    /// An empty `forge` config resolves to the compiled `ForgeConfig` default
    /// and the default maintenance interval, pinning byte-identical no-config
    /// behavior (AC1).
    ///
    /// # Panics
    ///
    /// Panics when the resolved config or interval differs from the compiled
    /// defaults, including the 1 GiB deployment file target.
    #[test]
    fn resolve_forge_config_defaults_match_compiled_defaults() {
        let (config, maintenance_interval) = resolve_forge_config(&ForgeRuntimeConfig::default());
        assert_eq!(config, ForgeConfig::default());
        assert_eq!(
            config.default_target_file_size_bytes,
            1024 * 1024 * 1024,
            "an unset deployment file target is 1 GiB, not Iceberg's 512 MiB"
        );
        assert_eq!(maintenance_interval, DEFAULT_MAINTENANCE_INTERVAL);
    }

    /// Supplied `forge` values override the compiled defaults on exactly the
    /// promoted fields, and the resolved config still validates fail-closed.
    ///
    /// # Panics
    ///
    /// Panics when a supplied value is not carried into the resolved config or
    /// when the overridden config fails validation.
    #[test]
    fn resolve_forge_config_applies_supplied_overrides() {
        let runtime = ForgeRuntimeConfig {
            orphan_gc_ttl_secs: Some(3_600),
            orphan_gc_max_list_pages: Some(64),
            orphan_gc_run_budget_secs: Some(30),
            maintenance_interval_secs: Some(45),
            target_file_size_bytes: Some(2_147_483_648),
            ..ForgeRuntimeConfig::default()
        };
        let (config, maintenance_interval) = resolve_forge_config(&runtime);
        assert_eq!(config.default_target_file_size_bytes, 2_147_483_648);
        assert_eq!(config.orphan_gc_ttl, Duration::from_hours(1));
        assert_eq!(config.orphan_gc_max_list_pages, 64);
        assert_eq!(config.orphan_gc_run_budget, Duration::from_secs(30));
        assert_eq!(maintenance_interval, Duration::from_secs(45));
        config
            .validate()
            .expect("resolved override config must validate");
        // Fields outside the promoted set retain their compiled defaults.
        assert_eq!(config.lease_ttl, ForgeConfig::default().lease_ttl);
    }

    /// A zeroed promoted duration resolves through and is rejected by the
    /// downstream `ForgeConfig::validate` fail-closed check.
    ///
    /// # Panics
    /// Panics if `validate` accepts the zero orphan-GC TTL.
    #[test]
    fn resolve_forge_config_zero_value_is_rejected_by_validate() {
        let runtime = ForgeRuntimeConfig {
            orphan_gc_ttl_secs: Some(0),
            ..ForgeRuntimeConfig::default()
        };
        let (config, _) = resolve_forge_config(&runtime);
        assert!(config.validate().is_err());
    }

    /// Production boot rejects a plaintext address discovered from live membership.
    #[test]
    fn production_boot_rejects_http_live_remote_oracle_member() {
        let addresses = HashMap::from([(
            NodeId::new(uuid::Uuid::now_v7()),
            "http://oracle-remote.internal:50052".to_owned(),
        )]);
        let error = validate_remote_oracle_addresses(
            crate::config::DeploymentProfile::Production,
            addresses.values().map(String::as_str),
        )
        .expect_err("plaintext live membership must fail before publication");
        assert!(error.to_string().contains("must use https://"));
    }

    fn implicit_tenant() -> DataTenantId {
        "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn sample_binding_entry() -> WorkloadBindingEntry {
        WorkloadBindingEntry {
            issuer: "https://idp.example.com".to_owned(),
            subject: "system:serviceaccount:default/my-sa".to_owned(),
            audience: Some("my-audience".to_owned()),
            // Lowercase mirrors the canonical config example; the boot parse is
            // case-insensitive against the kind wire name.
            kind: "service".to_owned(),
            name: "my-model".to_owned(),
            space: "prod".to_owned(),
            version: "1.0.0".to_owned(),
        }
    }

    #[test]
    fn workload_binding_binds_resolved_implicit_tenant() {
        let tenant = implicit_tenant();
        let bindings =
            build_workload_bindings(&[sample_binding_entry()], tenant).expect("bindings build");

        assert_eq!(bindings.len(), 1);
        // F4: every binding carries the resolved implicit tenant, never a sentinel.
        assert_eq!(bindings[0].tenant_id, tenant);
        assert_ne!(bindings[0].tenant_id, DataTenantId::SYSTEM_OWNER);
    }

    #[test]
    fn workload_binding_rejects_non_service_or_agent_kind() {
        let tenant = implicit_tenant();
        let mut entry = sample_binding_entry();
        entry.kind = "model".to_owned();

        let error = build_workload_bindings(&[entry], tenant)
            .expect_err("a model-kind binding must be rejected at boot");

        assert!(
            matches!(error, ServerBootError::InvalidWorkloadBinding { .. }),
            "expected InvalidWorkloadBinding, got {error:?}"
        );
    }
}

#[cfg(test)]
/// Postgres-backed boot fixtures and regressions shared by crate tests.
pub(crate) mod pg_tests {
    use super::*;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use tempfile::tempdir;

    /// Production and test serving paths publish the same one-composite ownership shape.
    ///
    /// # Panics
    /// Panics when the shared boot helper duplicates Gate, catalog, peer, or tail owners.
    #[test]
    fn production_and_test_server_share_bifrost_composition() {
        use crate::config::{BifrostRoles, BifrostRuntimeRole as Role, BifrostTarget};

        let cases = [
            (
                BifrostTarget::Server,
                vec![Role::Scribe, Role::ForgeCoordinator, Role::Oracle],
            ),
            (BifrostTarget::Oracle, vec![Role::Oracle]),
            (BifrostTarget::Scribe, vec![Role::Scribe]),
            (BifrostTarget::ForgeWorker, vec![Role::ForgeWorker]),
            (
                BifrostTarget::All,
                vec![
                    Role::Scribe,
                    Role::ForgeCoordinator,
                    Role::ForgeWorker,
                    Role::Oracle,
                ],
            ),
        ];

        for (target, expected) in cases {
            let selected = BifrostRoles::for_target(target);
            assert_eq!(selected.iter().copied().collect::<Vec<_>>(), expected);
            assert_eq!(selected.serves_gate(), selected.serves_api());
        }
    }

    /// Builds the shared non-Bifrost application shell for focused boot tests.
    async fn make_test_state() -> AppState {
        crate::test_support::test_app_state(
            crate::test_support::test_server_postgres(),
            crate::test_support::test_storage(),
            crate::test_support::test_catalog().await,
        )
    }

    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    /// An authz override replaces the permission evaluator wholesale.
    ///
    /// # Panics
    /// Panics when the patched state does not carry the override's evaluator.
    #[tokio::test(flavor = "current_thread")]
    async fn state_overrides_authz_applied() {
        let state = make_test_state().await;
        let replacement = ServerAuthz::default();
        let expected = Arc::clone(&replacement.permission_check);
        let overrides = StateOverrides {
            authz: Some(replacement),
            ..StateOverrides::default()
        };

        let patched = apply_overrides(state, overrides);

        assert!(Arc::ptr_eq(&patched.authz.permission_check, &expected));
    }

    /// Default overrides leave the assembled permission evaluator in place.
    ///
    /// # Panics
    /// Panics when an empty override changes the evaluator.
    #[tokio::test(flavor = "current_thread")]
    async fn state_overrides_default_is_noop() {
        let state = make_test_state().await;
        let original = Arc::clone(&state.authz.permission_check);

        let patched = apply_overrides(state, StateOverrides::default());

        assert!(Arc::ptr_eq(&patched.authz.permission_check, &original));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn app_state_retains_only_runtime_pools() {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), Some(admin_pool));
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let state = crate::test_support::test_app_state(
            postgres,
            storage,
            crate::test_support::test_catalog().await,
        );

        assert!(state.postgres.operator_pool().is_some());
        assert_eq!(
            state.storage.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
    }
}

#[cfg(test)]
mod sealing_boot_pg_tests {
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};

    use super::*;

    /// A keyless deployment boots while no provider secret is stored, and
    /// refuses to boot with `SealingKey` once one is, since it could never
    /// open that ciphertext.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded or either boot outcome differs.
    #[tokio::test]
    async fn keyless_boot_refuses_only_when_ciphertext_is_stored() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator = fixture.operator_pool().clone();
        rewrap_sealed_secrets(Some(operator.clone()), None)
            .await
            .expect("a keyless deployment with no stored secret boots");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        let superuser = fixture.superuser_pool().expect("superuser pool opens");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections \
             SET client_auth = 'SecretPost', client_secret_enc = '\\x0102'::bytea \
             WHERE connection_id = $1",
        )
        .bind(binding.connection_id)
        .execute(&superuser)
        .await
        .expect("a sealed secret is stored");

        let refused = rewrap_sealed_secrets(Some(operator), None)
            .await
            .expect_err("a keyless deployment with a stored secret refuses to boot");
        assert!(
            matches!(refused, ServerBootError::SealingKey(_)),
            "{refused:?}"
        );
    }
}
