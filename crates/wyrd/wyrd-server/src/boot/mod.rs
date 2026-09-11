//! Server boot sequence for SQL-backed Wyrd runtime state.

pub mod auth;
pub mod bootstrap;
pub mod issuer;
pub mod node_identity;

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use futures_util::{StreamExt, TryStreamExt};
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::forge::{
    Forge as ForgeCoordinator, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeObjectPages,
    ForgeObjectStore, ForgeTelemetry, ForgeWorker, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::oracle::dispatcher::{
    BifrostPeerTls, LocalOraclePeerTransport, OraclePeerCredentials, OraclePeerTransportDirectory,
    OraclePeerWorker, OraclePeerWorkerConfig, ReservationRegistry, TonicOraclePeerTransport,
};
use vala_bifrost_redux::oracle::peer::ReservationTicketMinter;
use vala_bifrost_redux::oracle::{
    Oracle as OracleEngine, OracleBuildConfig, OracleConfig, OracleMemoryResources,
    OracleSlotManager, OracleSpillRuntime, TailTransportDirectory,
};
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRoleResources, BifrostRuntimeResources,
    OracleClassSplit,
};
use vala_bifrost_redux::scribe::admission::{AdmissionConfig, EventTimeWindow};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribeIngressCpuPool,
    ScribePersistenceConfig, ScribePersistenceCpuPool, ScribeWalIoPool,
};
use wyrd_auth_oidc::WorkloadBinding;
use wyrd_crypt::SecretKey;
use wyrd_runtime::{Permission, PrincipalKind};
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{
    NodeId as ClusterNodeId, OracleCapabilitiesV1, QueryClass, ScribeCapabilitiesV1,
};
use wyrd_sql::postgres_boot::{BootError, PostgresBoot};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::components::auth::audit_writer::RealAuthzAuditWriter;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::EvalAuditWriter;
use crate::config::{BifrostRuntimeRole, WorkloadBindingEntry};
use crate::oracle::{
    OracleAuditPublisher, OraclePeerAuthority, PostgresPeerSecurityAudit,
    ServerBifrostPeerCredentials,
};
use crate::postgres::ServerPostgres;
use crate::state::{
    AppState, Forge, ForgeCompactionRuntime, Oracle, ProductionValidationError, Scribe,
    ScribeCoordinationRuntime,
};

const DEFAULT_MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const DEFAULT_HINT_CAPACITY: usize = 1_024;
const ORACLE_STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
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
    /// Listing is gated on the backend advertising `list_with_start_after`.
    /// The bounded scan resumes by cursor, and emulating that cursor by
    /// filtering would relist every earlier page on every attempt — exactly the
    /// unbounded listing the page cap exists to prevent — so an incapable
    /// backend is refused before any listing rather than served an
    /// anti-starvation guarantee this adapter cannot keep.
    ///
    /// # Errors
    ///
    /// Returns [`opendal::ErrorKind::Unsupported`] when the backend cannot
    /// resume from a cursor, and the underlying OpenDAL error when the lister
    /// cannot be opened. Errors encountered after the walk begins surface as a
    /// failed page in the returned stream.
    async fn list_pages(
        &self,
        prefix: &str,
        start_after: Option<&str>,
    ) -> opendal::Result<ForgeObjectPages> {
        if !self.operator.info().full_capability().list_with_start_after {
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unsupported,
                "Forge orphan listing requires backend list_with_start_after support",
            ));
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
    /// Replace the eval audit writer.
    pub eval_audit: Option<Arc<dyn EvalAuditWriter>>,
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
    /// Durable Scribe WAL root.
    wal_dir: std::path::PathBuf,
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
    /// Postgres boot or pool construction failed.
    #[error(transparent)]
    Postgres(#[from] BootError),
    /// Server database readiness failed.
    #[error(transparent)]
    Database(#[from] crate::postgres::ServerPostgresError),
    /// SQL migrations failed.
    #[error(transparent)]
    Sql(#[from] wyrd_sql::error::SqlError),
    /// Storage boot failed.
    #[error(transparent)]
    Storage(#[from] wyrd_storage::StorageError),
    /// Runtime pool construction failed.
    #[error("database pool construction failed")]
    PoolConnect(#[source] sqlx::Error),
    /// Wyrd's own signing key could not be loaded or its public key derived.
    /// Boot fails closed: without a usable signing key the server cannot mint or
    /// verify Wyrd JWTs.
    #[error("WYRD_SIGNING_KEY is invalid: {0}")]
    SigningKey(String),
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
    /// decode to a 32-byte AES-256-GCM key. Boot fails closed rather than
    /// proceeding with an unusable sealing key.
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
    Forge(#[from] vala_bifrost_redux::forge::ForgeError),
    /// Scribe WAL/runtime construction failed during boot.
    #[error("Scribe runtime construction failed: {0}")]
    Scribe(String),
    /// Oracle peer authority, audit, role, or worker construction failed.
    #[error("Oracle peer runtime construction failed: {0}")]
    OraclePeer(String),
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
fn resolve_forge_config(
    forge_runtime: &crate::config::ForgeRuntimeConfig,
) -> (ForgeConfig, std::time::Duration) {
    let base = ForgeConfig::default();
    let config = ForgeConfig {
        snapshot_retention: forge_runtime
            .snapshot_retention_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(base.snapshot_retention),
        retain_last: forge_runtime.retain_last.unwrap_or(base.retain_last),
        orphan_gc_ttl: forge_runtime
            .orphan_gc_ttl_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(base.orphan_gc_ttl),
        maintenance_trigger_snapshot_count: forge_runtime
            .maintenance_trigger_snapshot_count
            .unwrap_or(base.maintenance_trigger_snapshot_count),
        maintenance_trigger_interval: forge_runtime
            .maintenance_trigger_interval_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(base.maintenance_trigger_interval),
        orphan_gc_max_list_pages: forge_runtime
            .orphan_gc_max_list_pages
            .unwrap_or(base.orphan_gc_max_list_pages),
        orphan_gc_run_budget: forge_runtime
            .orphan_gc_run_budget_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(base.orphan_gc_run_budget),
        ..base
    };
    let maintenance_interval = forge_runtime
        .maintenance_interval_secs
        .map(std::time::Duration::from_secs)
        .unwrap_or(DEFAULT_MAINTENANCE_INTERVAL);
    (config, maintenance_interval)
}

/// Resolves and creates the one disposable Oracle scratch directory.
///
/// An injected base is used by test support. Production otherwise uses the
/// Scribe WAL base environment setting or the portable local default, then
/// appends exactly one `oracle-spill` component. WAL contents remain outside
/// the scratch allocator even when both directories share a filesystem.
///
/// # Errors
///
/// Returns a typed boot error when the canonical scratch directory cannot be
/// created; callers must stop before Bifrost role activation or detection.
fn prepare_oracle_spill_root(
    base: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf, ServerBootError> {
    let base = base.unwrap_or_else(|| {
        std::env::var_os("WYRD_SCRIBE_WAL_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(".wyrd/scribe-wal"))
    });
    let root = base.join("oracle-spill");
    std::fs::create_dir_all(&root)
        .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?;
    Ok(root)
}

/// Rejects a Scribe configuration whose largest admitted unit cannot replay on this root.
///
/// The comparison uses only the immutable configured shape and detected maximum
/// Scribe envelope. Temporary occupancy remains governed by the existing
/// capacity-epoch wait during replay.
///
/// # Errors
///
/// Returns [`ServerBootError::Scribe`] when envelope arithmetic overflows or
/// the intrinsic requirement exceeds the detected root capability.
fn validate_scribe_replay_envelope(
    config: crate::config::ScribeRuntimeConfig,
    maximum_envelope_bytes: usize,
) -> Result<(), ServerBootError> {
    let limits = config.ingest_limits();
    let required = vala_bifrost_redux::scribe::configured_maximum_envelope_bytes(limits)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    if required > maximum_envelope_bytes {
        let shortfall = required.saturating_sub(maximum_envelope_bytes);
        return Err(ServerBootError::Scribe(format!(
            "scribe configured replay envelope requires {required} bytes but the detected root \
             provides {maximum_envelope_bytes} bytes ({shortfall} bytes short). The requirement \
             scales from scribe.ingest_request_bytes = {request_bytes}, which Scribe must be able \
             to hold, persist, and replay after a crash. Lower scribe.ingest_request_bytes to fit \
             this node, or raise the node's memory limit \
             (WYRD_BIFROST_MEMORY_LIMIT_BYTES / the container memory limit)",
            request_bytes = limits.max_frame_bytes
        )));
    }
    Ok(())
}

/// Constructs the external dependency graph injected into Bifrost composition.
///
/// # Errors
/// Returns a boot error when Postgres, storage, catalog, resource detection, or
/// volume-root preparation fails.
async fn build_bifrost_external_dependencies(
    boot: &PostgresBoot,
    config: &crate::config::BifrostRuntimeConfig,
    target: crate::config::BifrostTarget,
) -> Result<BifrostExternalDependencies, ServerBootError> {
    let dsns = boot.dsns()?;
    let postgres = Arc::new(ServerPostgres::connect_from_boot(boot).await?);
    let storage_settings = load_storage_settings()?;
    let storage = StorageHandle::from_settings(storage_settings).await?;
    let wal_dir = std::env::var_os("WYRD_SCRIBE_WAL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(".wyrd/scribe-wal"));
    // A Scribe-bearing target owns durable state keyed by its own node, so it
    // reclaims the identity stored beside that state; a target with no Scribe
    // role owns no volume and is free to be a new node each incarnation.
    let node_id =
        if crate::config::BifrostRoles::for_target(target).contains(&BifrostRuntimeRole::Scribe) {
            node_identity::ScribeNodeIdentityStore::new(wal_dir.clone())
                .load_or_create()
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?
        } else {
            NodeId::generate()
        };
    let advertise_addr = config
        .peer
        .advertise_addr
        .clone()
        .unwrap_or_else(|| format!("https://{}", config.peer.bind));
    let cluster = Arc::new(ClusterRegistry::new(
        postgres.vala().clone(),
        ClusterNodeId::new(node_id.as_uuid()),
    ));
    let oracle_scratch = prepare_oracle_spill_root(Some(wal_dir.clone()))?;
    let scribe_stage = wal_dir.join("scribe-stage");
    let scribe_output_scratch = wal_dir.join("scribe-output-scratch");
    for root in [&wal_dir, &scribe_stage, &scribe_output_scratch] {
        std::fs::create_dir_all(root).map_err(|error| {
            ServerBootError::Scribe(format!("Bifrost volume root creation failed: {error}"))
        })?;
    }
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
        BifrostResourcePolicy {
            roles: resource_roles,
            memory_limit_bytes: config.resources.memory_limit_bytes,
            unmanaged_reserve_bytes: config.resources.unmanaged_reserve_bytes,
            scratch_limit_bytes: config.resources.scratch_limit_bytes,
            effective_cpu: config.resources.effective_cpu,
            oracle_query_slot_limit: config.resources.oracle_query_slot_limit,
            forge_compaction_memory_limit_bytes: config
                .resources
                .forge_compaction_memory_limit_bytes,
            scratch_root: oracle_scratch.clone(),
            volume_roots: Some(vala_bifrost_redux::resources::BifrostVolumeRoots {
                wal: wal_dir.clone(),
                scribe_stage,
                scribe_output_scratch,
                oracle_scratch,
            }),
        },
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
            dsns.catalog_app.expose_secret(),
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
        wal_dir,
    })
}

/// Composes the complete immutable Bifrost subsystem graph from injected dependencies.
///
/// # Errors
///
/// Returns [`ServerBootError`] when Scribe, Forge, Oracle, role fencing, or
/// request-boundary construction fails before publication.
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
        peer_credentials,
        peer_tls,
        peer_keyring,
        config: bifrost_config,
        forge_config: forge_runtime,
        node_id,
        advertise_addr,
        wal_dir,
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
    let scribe_config = bifrost_config.scribe;
    let scribe_stage = wal_dir.join("scribe-stage");
    let scribe_output_scratch = wal_dir.join("scribe-output-scratch");
    for root in [&wal_dir, &scribe_stage, &scribe_output_scratch] {
        std::fs::create_dir_all(root).map_err(|error| {
            ServerBootError::Scribe(format!("Bifrost volume root creation failed: {error}"))
        })?;
    }
    let resource_plan = bifrost_resources.plan();
    let pod_memory_limit = resource_plan.managed_memory_bytes;
    if roles.contains(&BifrostRuntimeRole::Scribe) {
        let scribe_resources = bifrost_resources.scribe().ok_or_else(|| {
            ServerBootError::Scribe(
                "Scribe role selected without a composed Scribe capability".to_owned(),
            )
        })?;
        validate_scribe_replay_envelope(
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
        std::fs::create_dir_all(&wal_dir).map_err(|error| {
            ServerBootError::Scribe(format!("WAL directory creation failed: {error}"))
        })?;
        let (wal_volume, scribe_output_volume) = bifrost_resources
            .scribe()
            .and_then(|resources| resources.volume_capabilities())
            .ok_or_else(|| {
                ServerBootError::Scribe(
                    "live Scribe role requires registered WAL volume capabilities".to_owned(),
                )
            })?;
        let configured_geometry = scribe_config
            .scribe_geometry()
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
            WalWriter::new_with_volume(
                &wal_dir,
                *stream.node_id.as_bytes(),
                stream.writer_epoch.as_i64(),
                WalConfig::new(wal_segment_bytes)
                    .map_err(|error| ServerBootError::Scribe(error.to_string()))?
                    .with_disk_limit(scribe_config.wal_disk_limit_bytes)
                    .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
                wal_volume,
            )
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        // The dedicated coordination runtime hosts every long-lived Scribe
        // coordination task: the `SCRIBE_SHARD_COUNT` shard-owner lanes plus the
        // reconciliation and persistence loops. It is deliberately separate from
        // the request runtime so a saturated ingest path cannot starve shard
        // progress, and its thread count derives from
        // `default_scribe_coordination_threads` (available parallelism, capped at
        // the lane count, floored at two). Only the `Handle` is handed to
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
            .map_or(std::time::Duration::ZERO, |controls| {
                controls.scribe_wal_sync_delay
            });
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
                    std::time::Duration::ZERO
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
            // Scribe's real ceiling is its guaranteed floor plus the shared
            // elastic allowance it may borrow, which is what the role governor
            // enforces. Validating against the floor alone would understate the
            // memory Scribe actually owns and refuse pods that can serve.
            scribe_memory_limit_bytes: (resource_plan.scribe_floor_bytes > 0).then(|| {
                resource_plan
                    .scribe_floor_bytes
                    .saturating_add(resource_plan.elastic_memory_bytes)
            }),
            policy: vala_bifrost_redux::scribe::geometry::ScribeArtifactPolicy::new(geometry),
            event_time_window: EventTimeWindow {
                past: scribe_config
                    .event_time_past_window_secs
                    .map(std::time::Duration::from_secs)
                    .unwrap_or_else(|| std::time::Duration::from_secs(30 * 24 * 60 * 60)),
                future: scribe_config
                    .event_time_future_window_secs
                    .map(std::time::Duration::from_secs)
                    .unwrap_or_else(|| std::time::Duration::from_secs(24 * 60 * 60)),
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
        let scribe = Arc::new(
            ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
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
            })
            .map_err(|error| {
                ServerBootError::Scribe(format!(
                    "Scribe cannot complete one table's lifecycle on this node's measured resources: {error}"
                ))
            })?,
        );
        if let Err(error) = scribe.replay_wal_async().await {
            if let Err(cleanup_error) = cluster_registry.shutdown_role(scribe_role.clone()).await {
                tracing::warn!(%cleanup_error, "failed to release reserved Scribe fence after recovery failure");
            }
            return Err(ServerBootError::Scribe(format!(
                "WAL recovery failed before role activation: {error}"
            )));
        }
        if let Err(error) = scribe.tail_reader() {
            if let Err(cleanup_error) = cluster_registry.shutdown_role(scribe_role.clone()).await {
                tracing::warn!(%cleanup_error, "failed to release reserved Scribe fence after tail failure");
            }
            return Err(ServerBootError::Scribe(format!(
                "tail reader failed before role activation: {error}"
            )));
        }
        if let Err(error) = cluster_registry.activate(&scribe_role).await {
            if let Err(cleanup_error) = cluster_registry.shutdown_role(scribe_role.clone()).await {
                tracing::warn!(%cleanup_error, "failed to release reserved Scribe fence after activation failure");
            }
            return Err(ServerBootError::Scribe(error.to_string()));
        }
        if let Err(error) = cluster_registry.refresh_snapshot().await {
            if let Err(cleanup_error) = cluster_registry.shutdown_role(scribe_role.clone()).await {
                tracing::warn!(%cleanup_error, "failed to release active Scribe fence after snapshot failure");
            }
            return Err(ServerBootError::Scribe(error.to_string()));
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
        let (mut forge_config, maintenance_interval) = resolve_forge_config(&forge_runtime);
        #[cfg(feature = "test-support")]
        if let Some(config) = test_controls
            .as_ref()
            .and_then(|controls| controls.forge_config.clone())
        {
            forge_config = config;
        }
        let staging = Arc::new(storage.operator().clone());
        // Read from the concrete operator before it is erased behind
        // `ForgeObjectStore`: only the backend itself can answer whether a
        // bounded orphan listing can resume from a cursor.
        let staging_lists_by_cursor = staging.info().full_capability().list_with_start_after;
        #[cfg(feature = "test-support")]
        let object_store: Arc<dyn ForgeObjectStore> = test_controls
            .as_ref()
            .and_then(|controls| controls.forge_object_store.clone())
            .unwrap_or_else(|| Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging))));
        #[cfg(not(feature = "test-support"))]
        let object_store: Arc<dyn ForgeObjectStore> =
            Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging)));
        let coordinator = Arc::new(ForgeCoordinator::new(ForgeBuildConfig {
            resource_plan,
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
            staging_lists_by_cursor,
            object_store,
            hints: staging_file_inbox,
            config: forge_config,
            maintenance_interval,
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
        })?);
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
                    ServerBootError::Forge(vala_bifrost_redux::forge::ForgeError::InvalidConfig {
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

    let query_audit = if roles.contains(&BifrostRuntimeRole::Oracle)
        || roles.contains(&BifrostRuntimeRole::Scribe)
    {
        Some(
            OracleAuditPublisher::new(postgres.vala().clone(), (&bifrost_config.oracle).into())
                .map_err(|error| {
                    ServerBootError::OraclePeer(format!(
                        "Oracle audit WAL recovery failed: {error:?}"
                    ))
                })?,
        )
    } else {
        None
    };
    let scribe = if let Some(parts) = scribe {
        let tail_audit = Arc::new(
            crate::oracle::PostgresTailSecurityAudit::try_new(&postgres)
                .await
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        let fragment_security_audit = Arc::new(
            crate::oracle::PostgresPeerSecurityAudit::try_new(&postgres)
                .await
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        let fragment_authority = Arc::new(
            crate::oracle::OraclePeerAuthority::from_keyring(
                Arc::clone(&peer_keyring),
                fragment_security_audit.clone(),
            )
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        Some(Arc::new(
            Scribe::new(crate::state::ScribeBuildInputs {
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
                fragment_query_audit: query_audit.clone().ok_or_else(|| {
                    ServerBootError::Scribe(
                        "selected Scribe role has no tenant-tripwire audit owner".to_owned(),
                    )
                })?,
                owns_fragment_query_audit: !roles.contains(&BifrostRuntimeRole::Oracle),
                role_shutdown: shutdown.clone(),
            })
            .with_tail_authority(Arc::new(
                crate::oracle::ScribeTailAuthority::from_keyring(
                    Arc::clone(&peer_keyring),
                    tail_audit,
                ),
            )),
        ))
    } else {
        None
    };
    let oracle = OracleRoleBuilder {
        postgres: Arc::clone(&postgres),
        catalog: Arc::clone(&bifrost),
        resources: bifrost_resources.clone(),
        token_verifier: Arc::clone(&token_verifier),
        config: &bifrost_config,
        target,
        deployment_profile,
        peer_keyring: Arc::clone(&peer_keyring),
        cluster: Arc::clone(&cluster_registry),
        node_id,
        advertise_addr: &advertise_addr,
        spill_root: Some(wal_dir),
        peer_credentials: Arc::clone(&peer_credentials),
        peer_tls: peer_tls.clone(),
        audit: query_audit.clone(),
        shutdown: shutdown.clone(),
    }
    .build()
    .await?;
    let forwarding_audit = Arc::new(
        PostgresPeerSecurityAudit::try_new(&postgres)
            .await
            .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
    );
    let forwarding_authority = Arc::new(
        OraclePeerAuthority::from_keyring(Arc::clone(&peer_keyring), forwarding_audit)
            .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
    );
    let query_controls = oracle.as_ref().map_or_else(
        || {
            let transport = if let Some(tls) = peer_tls.clone() {
                crate::oracle::OracleLifecycleTransport::with_tls(
                    Arc::clone(&cluster_registry),
                    Arc::clone(&peer_credentials),
                    node_id,
                    tls,
                )
            } else {
                crate::oracle::OracleLifecycleTransport::new(
                    Arc::clone(&cluster_registry),
                    Arc::clone(&peer_credentials),
                    node_id,
                )
            };
            crate::oracle::RunningQueryControls::new(
                None,
                Arc::new(transport),
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
            credentials: Arc::clone(&peer_credentials),
            tls: peer_tls,
            authority: forwarding_authority,
            config: OracleConfig::default(),
        },
    ));
    let interceptor =
        vala_bifrost_redux::gate::auth::ingest_auth_interceptor(Arc::clone(&token_verifier));
    let ingest_limits = scribe_config.ingest_limits();
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
    );
    // Resolved from the process's own credential, so the identity a replica
    // presents on the peer plane and the identity it admits are the same
    // principal. A peer-serving target that cannot prove it fails to boot.
    let peer_identity = if target.serves_peer() {
        Some(
            crate::grpc::PeerWorkloadIdentity::resolve(
                peer_credentials.as_ref(),
                token_verifier.as_ref(),
            )
            .await
            .map_err(ServerBootError::OraclePeer)?,
        )
    } else {
        None
    };
    Ok(crate::state::ComposedBifrost {
        bifrost: crate::state::Bifrost::assembled(crate::state::BifrostComposition {
            gate,
            scribe,
            forge,
            oracle,
            bifrost_storage,
            transport: bifrost_resources.transport_admission(),
            token_verifier,
            peer_identity,
            query_forwarder: Some(query_forwarder),
            query_controls: Some(query_controls),
            #[cfg(feature = "test-support")]
            resources: Some(bifrost_resources.clone()),
        }),
        coordination_runtime,
        compaction_runtime,
    })
}

/// Derives one Forge worker's local compaction-admission bounds.
///
/// Every bound comes from values the immutable [`ResourcePlan`] already
/// resolved, so a node cannot admit compaction work its own resource plan did
/// not reserve. The memory budget is the plan's selected Forge budget verbatim.
/// Running parallelism is three units per effective CPU, matching upstream's
/// task multiplier over its detected worker threads, and waiting parallelism is
/// four times that, so a burst of planned work queues rather than being refused
/// while earlier plans still run. Tenant fairness is unrelated to either and
/// stays with the SQL fair claim.
///
/// [`ResourcePlan`]: vala_bifrost_redux::resources::ResourcePlan
fn forge_compaction_worker_config(
    plan: &vala_bifrost_redux::resources::ResourcePlan,
    forge_runtime: &crate::config::ForgeRuntimeConfig,
) -> ForgeWorkerConfig {
    let max_task_parallelism = u32::try_from(plan.effective_cpu.saturating_mul(3))
        .unwrap_or(u32::MAX)
        .max(1);
    ForgeWorkerConfig {
        per_tenant_active_cap: forge_runtime.resolved_per_tenant_active_cap(),
        compaction_memory_budget_bytes: plan.forge_compaction_memory_limit_bytes,
        max_task_parallelism,
        pending_task_parallelism: max_task_parallelism.saturating_mul(4),
    }
}

/// Build the bounded Forge worker future shared by embedded and worker roles.
///
/// The bounded worker's local compaction parallelism comes from its own
/// admission queue, sized once at composition from the immutable resource plan.
/// This entry point only supervises the resulting single event loop.
///
/// # Errors
/// Returns [`ServerBootError::ForgeSchedulerRequired`] when Forge is absent or
/// [`ServerBootError::Forge`] when the requested worker bound or per-tenant cap
/// is invalid.
pub fn spawn_forge_worker(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<
    impl std::future::Future<Output = Result<(), vala_bifrost_redux::forge::ForgeError>>
    + Send
    + 'static,
    ServerBootError,
> {
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
}

/// Emit pre-telemetry warnings for relaxed config that is still safe to run.
///
/// Production-profile rejection lives in `WyrdServerConfig::validate()` and runs
/// before this function. By the time we reach `production_guards`, any violating
/// production config has already returned `ConfigError::Invalid`. This function
/// only surfaces development-profile warnings for the same signals so an operator
/// running a relaxed dev profile sees them on stderr.
pub fn production_guards(config: &crate::config::WyrdServerConfig) {
    if config.deployment_profile.is_production() {
        return;
    }
    if config.grpc.reflection_enabled {
        tracing::warn!("grpc.reflection_enabled=true in development profile");
    }
    if config.auth.allow_preview {
        tracing::warn!("auth.allow_preview=true in development profile");
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
    config: &crate::config::WyrdServerConfig,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
    overrides: StateOverrides,
) -> Result<BootedServer, ServerBootError> {
    let shutdown = CancellationToken::new();
    let boot = PostgresBoot::from_env().await?;
    let external = build_bifrost_external_dependencies(&boot, &config.bifrost, config.role).await?;
    let sealing_key = build_sealing_key(config)?;
    let signing_key = resolve_signing_key(config)?;
    let auth = install_auth(
        external.postgres.as_ref(),
        config,
        &signing_key,
        sealing_key.clone(),
    )
    .await?;
    let verifier = auth
        .token_verifier
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires a token verifier".to_owned()))?;
    let peer_credentials: Arc<dyn OraclePeerCredentials> = Arc::new(
        ServerBifrostPeerCredentials::from_configured_key(config.bifrost.peer.api_key.clone())
            .map_err(ServerBootError::OraclePeer)?,
    );
    let peer_tls = build_bifrost_peer_tls(&config.bifrost.peer, config.role)?;
    let peer_keyring = resolve_peer_ticket_keyring(config)?;
    let crate::state::ComposedBifrost {
        bifrost,
        coordination_runtime,
        compaction_runtime,
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
        peer_credentials,
        peer_tls,
        peer_keyring,
        config: config.bifrost.clone(),
        forge_config: config.forge,
        node_id: external.node_id,
        advertise_addr: external.advertise_addr,
        wal_dir: external.wal_dir,
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
    let state = attach_config_fields(state, config, telemetry)?;
    if let Err(error) = seed_federation(&state, config, sealing_key.as_deref()).await {
        rollback_state_roles(&state).await;
        return Err(error);
    }

    // Install the real authz audit writer as the OSS default. Callers can
    // still replace the entire ServerAuthz (policy hook + writer) via overrides.
    let state = state.with_authz(ServerAuthz {
        audit_writer: Arc::new(RealAuthzAuditWriter),
        ..ServerAuthz::default()
    });
    let state = apply_overrides(state, overrides);

    if let Err(error) = state.production_validate() {
        rollback_state_roles(&state).await;
        return Err(ServerBootError::ProductionValidation(error));
    }
    Ok(BootedServer {
        state,
        coordination_runtime,
        compaction_runtime,
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
/// Bifrost peer operation; see [`resolve_peer_ticket_keyring`].
///
/// # Errors
///
/// Returns [`ServerBootError::SigningKey`] when production has no configured
/// key or development cannot generate an ephemeral Ed25519 key.
fn resolve_signing_key(
    config: &crate::config::WyrdServerConfig,
) -> Result<SecretString, ServerBootError> {
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

/// Resolves the independent Bifrost peer ticket keyring for this process.
///
/// Peer authority is deliberately separate from the workload signing key: a
/// user or API token must never validate as a peer-purpose ticket, and the
/// peer keyring rotates on its own schedule with retired verification keys
/// still accepted until their published instant. Configured material is
/// required in production. Development without configured material generates
/// an ephemeral single-key keyring so a default mixed-role server still
/// constructs a real local Oracle; that key is distinct from the workload key
/// and does not survive a restart.
///
/// # Errors
///
/// Returns [`ServerBootError::SigningKey`] when production has no complete
/// peer ticket configuration, when the configured keyring cannot be loaded,
/// or when development cannot generate an ephemeral Ed25519 key.
fn resolve_peer_ticket_keyring(
    config: &crate::config::WyrdServerConfig,
) -> Result<Arc<crate::oracle::PeerTicketKeyring>, ServerBootError> {
    let ticket = &config.bifrost.peer.ticket;
    if ticket.is_complete() {
        return crate::oracle::PeerTicketKeyring::load(ticket)
            .map(Arc::new)
            .map_err(|error| ServerBootError::SigningKey(error.to_string()));
    }
    if config.deployment_profile.is_production() {
        return Err(ServerBootError::SigningKey(
            "no Bifrost peer ticket keyring configured (set \
             WYRD_BIFROST_PEER_TICKET_ACTIVE_KEY_ID, \
             WYRD_BIFROST_PEER_TICKET_SIGNING_KEY_PATH, and \
             WYRD_BIFROST_PEER_TICKET_VERIFYING_KEYRING_PATH)"
                .to_owned(),
        ));
    }
    let ephemeral = wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;
    tracing::warn!(
        "APP_ENV=development and no Bifrost peer ticket keyring configured; generated an \
         EPHEMERAL peer keyring. Peer tickets will not survive a restart and this keyring \
         must never be used in production."
    );
    crate::oracle::PeerTicketKeyring::from_signing_key_pem(&ephemeral)
        .map(Arc::new)
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))
}

/// Apply caller overrides to a built state. Factored out for unit testing
/// without a live DB boot.
fn apply_overrides(state: AppState, overrides: StateOverrides) -> AppState {
    let mut state = state;
    if let Some(authz) = overrides.authz {
        state = state.with_authz(authz);
    }
    if let Some(eval_audit) = overrides.eval_audit {
        state = state.with_eval_audit(eval_audit);
    }
    state
}

/// Attach config-derived fields to core state: shutdown token, telemetry, and
/// limits. Pure/sync (no I/O).
fn attach_config_fields(
    state: AppState,
    config: &crate::config::WyrdServerConfig,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
) -> Result<AppState, ServerBootError> {
    Ok(state
        .with_deployment_profile(config.deployment_profile)
        .with_telemetry(telemetry)
        .with_limits(config.limits.into_state()))
}

/// Install Wyrd's own auth handles: build resolvers, construct issuing key +
/// verifier (fails closed in production without a key), and attach via
/// `with_auth`.
///
/// # Errors
/// Returns [`ServerBootError::SigningKey`] when production profile lacks a
/// signing key, or when key material is invalid.
async fn install_auth(
    postgres: &ServerPostgres,
    config: &crate::config::WyrdServerConfig,
    signing_key: &SecretString,
    sealing_key: Option<Arc<SecretKey>>,
) -> Result<ServerAuth, ServerBootError> {
    // Postgres is the single source of issuer/binding resolution. Both resolvers
    // are always attached; an empty config simply means the tenant federates no
    // issuers and binds no workloads, which they resolve as empty results. The
    // issuer resolver also feeds the token verifier's external (foreign-OIDC)
    // path so federated tokens can be exchanged per-request.
    let issuer_resolver = Arc::new(PgIssuerResolver::new(
        Arc::new(postgres.app_pool().clone()),
        sealing_key.clone(),
    ));
    let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(Arc::new(
        postgres.app_pool().clone(),
    )));

    let (issuing_key, verifier) = crate::boot::auth::build_auth_handles(
        signing_key,
        postgres.app_pool(),
        Arc::clone(&issuer_resolver),
    )?;

    Ok(ServerAuth {
        allow_preview: config.auth.allow_preview,
        issuing_key: Some(issuing_key),
        token_verifier: Some(verifier),
        trusted_issuer_resolver: Some(Arc::clone(&issuer_resolver)),
        workload_binding_resolver: Some(binding_resolver),
        sealing_key: sealing_key.clone(),
        token_exchange_settings: crate::auth::exchange_api_key::TokenExchangeSettings::default(),
    })
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
    /// Exact boot-constructed token verifier used for peer preflight.
    token_verifier: Arc<crate::state::WyrdTokenVerifier>,
    /// Validated Bifrost configuration borrowed for this activation.
    config: &'a crate::config::BifrostRuntimeConfig,
    /// Closed process target controlling Oracle selection.
    target: crate::config::BifrostTarget,
    /// Deployment posture used for remote TLS validation.
    deployment_profile: crate::config::DeploymentProfile,
    /// Independent Bifrost peer ticket keyring used to mint and verify tickets.
    peer_keyring: Arc<crate::oracle::PeerTicketKeyring>,
    /// Cluster registry owning the role fence and readiness publication.
    cluster: Arc<ClusterRegistry>,
    /// Stable node identity advertised to peer services.
    node_id: ClusterNodeId,
    /// Bound endpoint published in cluster membership.
    advertise_addr: &'a str,
    /// Optional harness-owned Bifrost root replacing environment discovery.
    spill_root: Option<std::path::PathBuf>,
    /// Shared outbound peer bearer owner.
    peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Immutable peer TLS trust policy, present only when CA material is configured.
    peer_tls: Option<BifrostPeerTls>,
    /// Shared query audit used by the leader and role-local tenant tripwires.
    audit: Option<Arc<OracleAuditPublisher>>,
    /// One process-wide shutdown token injected into every Oracle owner.
    shutdown: CancellationToken,
}

impl<'a> OracleRoleBuilder<'a> {
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
        let built = self.construct().await?;
        built.start_reconcile_activate_publish().await.map(Some)
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
            token_verifier,
            config,
            target: _,
            deployment_profile,
            peer_keyring,
            cluster,
            node_id,
            advertise_addr,
            spill_root,
            peer_credentials,
            peer_tls,
            audit,
            shutdown,
        } = self;
        // The same immutable identity serves Scribe-tail discovery and the
        // Analytical east-west plane; naming it twice would let the two drift.
        let tail_tls = peer_tls.clone();
        let audit = audit.ok_or_else(|| {
            ServerBootError::OraclePeer("selected Oracle role has no query audit owner".to_owned())
        })?;
        let security_audit = Arc::new(
            PostgresPeerSecurityAudit::try_new(&postgres)
                .await
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let authority = Arc::new(
            OraclePeerAuthority::from_keyring(Arc::clone(&peer_keyring), security_audit.clone())
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
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
        let memory_budget = resource_plan
            .oracle_floor_bytes
            .checked_add(resource_plan.elastic_memory_bytes)
            .ok_or_else(|| {
                ServerBootError::OraclePeer("Oracle memory grant overflow".to_owned())
            })?;
        let memory_budget_bytes = u64::try_from(memory_budget)
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
            planning_permits: config.oracle.planning_permits,
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
                std::time::Duration::from_millis(config.oracle.max_queue_wait_ms),
                |value| value.max_queue_wait,
            ),
            ..OracleConfig::default()
        };
        let operator_pool = postgres.operator_pool().ok_or_else(|| {
            ServerBootError::OraclePeer(
                "Oracle reader-epoch authority requires the operator pool".to_owned(),
            )
        })?;
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
        let slots = Arc::new(OracleSlotManager::new(
            config.oracle.admission_waiters,
            running_slots,
        ));
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
            oracle_floor_bytes = resource_plan.oracle_floor_bytes,
            elastic_memory_bytes = resource_plan.elastic_memory_bytes,
            "Oracle admission capacity resolved"
        );
        let reservations = Arc::new(ReservationRegistry::new(Arc::clone(&slots), 1_024));
        let snapshot = cluster.snapshot();
        validate_remote_oracle_addresses(
            deployment_profile,
            snapshot
                .live_oracles()
                .into_iter()
                .filter(|lease| lease.key.node_id != node_id)
                .map(|lease| lease.address.as_str()),
        )?;
        let initial_bearer = peer_credentials.bearer(false).await.map_err(|_| {
            ServerBootError::OraclePeer("Oracle peer credential exchange failed".to_owned())
        })?;
        let mut metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        let bearer = format!("Bearer {initial_bearer}").parse().map_err(|_| {
            ServerBootError::OraclePeer("Oracle peer access token is invalid".to_owned())
        })?;
        metadata.insert("x-wyrd-access-token", bearer);
        let authenticated =
            vala_bifrost_redux::gate::auth::authenticate(token_verifier.as_ref(), &metadata)
                .await
                .map_err(|_| {
                    ServerBootError::OraclePeer("Oracle peer access token was rejected".to_owned())
                })?;
        if !matches!(authenticated.principal.kind, PrincipalKind::Service { .. })
            || authenticated.principal.tenant_id != DataTenantId::SYSTEM_OWNER
            || !authenticated
                .principal
                .effective_permissions
                .contains(&Permission::bifrost_peer_invoke())
        {
            return Err(ServerBootError::OraclePeer(
                "Oracle peer credential lacks platform service authority".to_owned(),
            ));
        }
        let remote_transport = Arc::new(
            if let Some(tls) = tail_tls.clone() {
                TonicOraclePeerTransport::with_credentials_and_tls(
                    Arc::clone(&cluster),
                    Arc::clone(&peer_credentials),
                    tls,
                )
            } else {
                TonicOraclePeerTransport::with_credentials(
                    Arc::clone(&cluster),
                    Arc::clone(&peer_credentials),
                )
            }
            // The same authority that verifies inbound reservation tickets
            // signs the outbound ones, so a node cannot mint authority it would
            // not itself accept.
            .with_reservation_minter(Arc::clone(&authority) as Arc<dyn ReservationTicketMinter>),
        );
        let lifecycle_transport = Arc::new(if let Some(tls) = tail_tls.clone() {
            crate::oracle::OracleLifecycleTransport::with_tls(
                Arc::clone(&cluster),
                Arc::clone(&peer_credentials),
                node_id,
                tls,
            )
        } else {
            crate::oracle::OracleLifecycleTransport::new(
                Arc::clone(&cluster),
                Arc::clone(&peer_credentials),
                node_id,
            )
        });
        let reconciliation_limit_bytes = memory_budget
            .checked_div(4)
            .filter(|limit| *limit > 0)
            .ok_or_else(|| {
                ServerBootError::OraclePeer("Oracle reconciliation budget is zero".to_owned())
            })?;
        let tail_audit = Arc::new(
            crate::oracle::PostgresTailSecurityAudit::try_new(&postgres)
                .await
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let tail_authority = Arc::new(crate::oracle::ScribeTailAuthority::from_keyring(
            Arc::clone(&peer_keyring),
            tail_audit,
        ));
        let tail_discovery = Arc::new(crate::oracle::RegistryTailStreamDiscovery::new(
            Arc::clone(&cluster),
            Arc::clone(&peer_credentials),
            tail_tls,
            Arc::clone(&tail_authority)
                as Arc<dyn vala_bifrost_redux::scribe::tail_rpc::TailTicketMinter>,
            node_id,
            None,
        ));
        let verifier: Arc<dyn vala_bifrost_redux::oracle::peer::PeerTicketVerifier> =
            authority.clone();
        let stage_authority: Arc<dyn vala_bifrost_redux::oracle::peer::OracleStageAuthority> =
            authority.clone();
        let peer_ticket_minter: Arc<dyn vala_bifrost_redux::oracle::peer::PeerTicketMinter> =
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
        let follower_resolver = Arc::new(
            vala_bifrost_redux::oracle::follower::OracleCatalogResolver::new(Arc::clone(&catalog)),
        );
        let worker = Arc::new(OraclePeerWorker::new_physical_with_resources(
            OraclePeerWorkerConfig {
                worker_node_id: node_id,
                oracle_fence: role.fencing_token,
                verifier,
                security_audit: security_audit.clone(),
                reservations: Arc::clone(&reservations),
                oracle_resources: oracle_resources.clone(),
                resolver: follower_resolver,
                audit: audit.clone(),
            },
        ));
        let peer = Arc::new(crate::oracle::OraclePeerRuntime::new(
            Arc::clone(&worker),
            Arc::clone(&security_audit),
            lifecycle_transport,
            Arc::clone(&authority),
        ));
        let local_transport = Arc::new(LocalOraclePeerTransport::new(Arc::clone(&worker)));
        let peer_transports = Arc::new(OraclePeerTransportDirectory::new(
            node_id,
            local_transport,
            remote_transport,
        ));
        let oracle_spill_root = prepare_oracle_spill_root(spill_root)?;
        let spill_runtime =
            match OracleSpillRuntime::new(&oracle_spill_root, resource_plan.scratch_limit_bytes) {
                Ok(runtime) => runtime,
                Err(error) => {
                    release_failed_oracle_role(&cluster, &role, "spill runtime construction").await;
                    return Err(ServerBootError::OraclePeer(error.to_string()));
                }
            };
        let oracle = match OracleEngine::new(OracleBuildConfig {
            shutdown: shutdown.clone(),
            catalog: Arc::clone(&catalog),
            vala: postgres.vala().clone(),
            operator_pool,
            cluster: Arc::clone(&cluster),
            local_role: role.clone(),
            local_slots: slots,
            memory: OracleMemoryResources {
                resources,
                reconciliation_limit_bytes,
            },
            spill_runtime: Arc::new(spill_runtime),
            tails: Arc::new(TailTransportDirectory::default()),
            audit: audit.clone(),
            peer_ticket_minter,
            reservations,
            stage_authority: Some(stage_authority),
            peer_tls,
            peer_credentials: Some(Arc::clone(&peer_credentials)),
            tail_ticket_minter: Some(tail_authority),
            tail_discovery: Some(tail_discovery),
            peer_transports: Some(peer_transports),
            config: oracle_config,
        })
        .await
        {
            Ok(oracle) => Arc::new(oracle),
            Err(error) => {
                release_failed_oracle_role(&cluster, &role, "construction").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
        };
        // The engine owns the one process reader authority and is built after
        // this worker, so the follower's single-assignment cell is filled here
        // — before startup reconciliation, activation, snapshot publication, or
        // readiness. Until it succeeds a snapshot-bearing assignment fails
        // closed, and a repeated or late installation fails boot outright.
        if let Err(error) = worker.install_reader_authority(Arc::clone(oracle.reader_authority())) {
            oracle.shutdown(std::time::Instant::now()).await;
            release_failed_oracle_role(&cluster, &role, "reader authority installation").await;
            return Err(ServerBootError::OraclePeer(error.to_string()));
        }
        Ok(BuiltOracleRole {
            catalog,
            oracle,
            role,
            peer,
            cluster,
            audit,
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
    /// Local audit publisher recovered before role activation.
    audit: Arc<OracleAuditPublisher>,
    /// Root-derived Oracle resource capability.
    resources: vala_bifrost_redux::resources::OracleResources,
    /// One process-wide shutdown token retained through lifecycle publication.
    shutdown: CancellationToken,
}

impl BuiltOracleRole {
    /// Starts reconciliation, activates the role, and publishes query access.
    ///
    /// # Errors
    /// Returns [`ServerBootError::OraclePeer`] when startup reconciliation,
    /// activation, snapshot refresh, or readiness publication fails.
    async fn start_reconcile_activate_publish(self) -> Result<Arc<Oracle>, ServerBootError> {
        let Self {
            catalog,
            oracle,
            role,
            peer,
            cluster,
            audit,
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
        let lifecycle_transport = peer.lifecycle_transport();
        let query_runtime = match Oracle::new(crate::state::OracleBuildInputs {
            engine: Arc::clone(&oracle),
            catalog,
            registered_role: role.clone(),
            cluster: Arc::clone(&cluster),
            audit,
            lifecycle_transport,
            resources,
            peer,
            role_shutdown: shutdown,
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
    config: &crate::config::WyrdServerConfig,
    sealing_key: Option<&SecretKey>,
) -> Result<(), ServerBootError> {
    // Seed `[[trusted_issuers]]` and `[[workload_bindings]]` into Postgres under
    // the implicit tenant. Both resolve the slug through the same
    // `resolve_by_slug_for_app` path the request handlers use, so the bound
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
            crate::boot::issuer::resolve_implicit_tenant(state.postgres.app_pool(), slug).await?;

        crate::boot::issuer::seed_trusted_issuers(
            state.postgres.app_pool(),
            tenant_id,
            &config.trusted_issuers,
            sealing_key,
        )
        .await?;

        let bindings = build_workload_bindings(&config.workload_bindings, tenant_id)?;
        crate::boot::issuer::seed_workload_bindings(
            state.postgres.app_pool(),
            tenant_id,
            &bindings,
        )
        .await?;
    }
    Ok(())
}

/// Decode the optional base64 sealing key from config into an AES-256-GCM key.
///
/// Returns `Ok(None)` when no sealing key is configured. Boot fails closed with
/// [`ServerBootError::SealingKey`] when the key is set but is not valid base64
/// or does not decode to exactly 32 bytes.
///
/// **Single-key model:** this function produces one static process-wide key. There
/// is no key-id column, no keyring, and no live rotation path. See `config.rs`
/// `AuthConfig::sealing_key` for the manual rotation runbook. Key-id versioning,
/// keyring support, KMS-backed KEK, and AAD binding are tracked in issue #72.
fn build_sealing_key(
    config: &crate::config::WyrdServerConfig,
) -> Result<Option<Arc<SecretKey>>, ServerBootError> {
    let Some(encoded) = config.auth.sealing_key.as_ref() else {
        return Ok(None);
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.expose_secret())
        .map_err(|error| {
            ServerBootError::SealingKey(format!("sealing key is not valid base64: {error}"))
        })?;
    let key: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        ServerBootError::SealingKey(format!(
            "sealing key must decode to 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    Ok(Some(Arc::new(SecretKey::from_bytes(key))))
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
    Option<
        impl std::future::Future<Output = Result<(), vala_bifrost_redux::forge::ForgeError>>
        + Send
        + 'static,
    >,
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

/// Loads the one role-neutral Bifrost peer identity for this process.
///
/// A peer-bearing target fails closed here: an absent or unreadable CA, leaf
/// chain, or private key is a boot failure rather than a listener that starts
/// and refuses every connection later. Non-peer targets (Forge workers) load
/// nothing and return `None`.
///
/// # Errors
///
/// Returns [`ServerBootError::OraclePeer`] when a peer-bearing target has an
/// incomplete peer configuration or any PEM file cannot be read.
fn build_bifrost_peer_tls(
    peer: &crate::config::BifrostPeerConfig,
    target: crate::config::BifrostTarget,
) -> Result<Option<BifrostPeerTls>, ServerBootError> {
    if !peer.is_complete() {
        if target.serves_peer() {
            return Err(ServerBootError::OraclePeer(
                "Scribe- and Oracle-bearing targets require the complete bifrost.peer identity"
                    .to_owned(),
            ));
        }
        return Ok(None);
    }
    let read = |path: &std::path::Path, label: &str| -> Result<Vec<u8>, ServerBootError> {
        std::fs::read(path).map_err(|error| {
            ServerBootError::OraclePeer(format!(
                "failed to read Bifrost peer {label} {}: {error}",
                path.display()
            ))
        })
    };
    let ca = read(
        peer.ca_certificate_path
            .as_ref()
            .expect("peer completeness guarantees a CA path"),
        "CA certificate",
    )?;
    let chain = read(
        peer.certificate_chain_path
            .as_ref()
            .expect("peer completeness guarantees a certificate chain path"),
        "certificate chain",
    )?;
    let key = read(
        peer.private_key_path
            .as_ref()
            .expect("peer completeness guarantees a private key path"),
        "private key",
    )?;
    let key = String::from_utf8(key).map_err(|_| {
        ServerBootError::OraclePeer("Bifrost peer private key is not valid PEM text".to_owned())
    })?;
    Ok(Some(BifrostPeerTls::new(
        ca,
        peer.server_name
            .as_ref()
            .expect("peer completeness guarantees a server name")
            .clone(),
        chain,
        secrecy::SecretString::from(key),
    )))
}

/// Ensure production Card recovery can run through the Wyrd operator pool.
pub fn check_card_recovery_pool(state: &AppState) -> Result<(), ServerBootError> {
    check_card_recovery_pool_inner(
        state.postgres.operator_pool().is_some(),
        &state.deployment_profile,
    )
}

fn check_card_recovery_pool_inner(
    has_operator_pool: bool,
    profile: &DeploymentProfile,
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
/// split can cover one Analytical query's slot cost, because a leader that
/// deterministically selects a replica advertising a class it always refuses
/// would fail a query a capable replica could have served.
fn oracle_supported_classes(analytical_slots: u32) -> Vec<QueryClass> {
    if analytical_slots >= vala_bifrost_redux::resources::ANALYTICAL_QUERY_SLOT_UNITS {
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
            oracle_supported_classes(vala_bifrost_redux::resources::ANALYTICAL_QUERY_SLOT_UNITS),
            vec![QueryClass::Interactive, QueryClass::Analytical]
        );
    }

    /// The compaction runtime is role-scoped and its budget refuses invalid boot.
    ///
    /// Two facts sit on the same owner because composition decides both at the
    /// same moment. Only a process that actually runs admitted plans builds a
    /// dedicated executor, and it sizes that executor from the resolved
    /// effective CPU the memory budget came from rather than from a second CPU
    /// knob that could disagree. A budget the protected floors cannot cover is
    /// refused outright, because a clamped budget would silently admit plans
    /// against memory another role is guaranteed.
    ///
    /// # Panics
    ///
    /// Panics when a non-worker role builds an executor, when the derived
    /// admission bounds do not follow effective CPU, or when an invalid budget
    /// is accepted.
    #[test]
    fn forge_runtime_is_role_scoped_and_budget_refuses_invalid_boot() {
        use vala_bifrost_redux::resources::{
            BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, ResourceSource,
            SystemResourceSnapshot,
        };

        /// Plans one node exactly as `compose_bifrost` does for these roles.
        fn plan_for(
            roles: &[BifrostRole],
            override_bytes: Option<usize>,
        ) -> Result<vala_bifrost_redux::resources::ResourcePlan, String> {
            let scratch = 1024 * 1024 * 1024_u64;
            BifrostRuntimeResources::from_snapshot(
                SystemResourceSnapshot {
                    memory_limit_bytes: 4 * 1024 * 1024 * 1024,
                    effective_cpu: 6,
                    scratch_capacity_bytes: scratch * 2,
                    scratch_available_bytes: scratch * 2,
                    memory_source: ResourceSource::Injected,
                    cpu_source: ResourceSource::Injected,
                },
                BifrostResourcePolicy {
                    roles: roles.iter().copied().collect(),
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: Some(scratch),
                    effective_cpu: None,
                    oracle_query_slot_limit: None,
                    forge_compaction_memory_limit_bytes: override_bytes,
                    scratch_root: std::path::PathBuf::new(),
                    volume_roots: None,
                },
            )
            .map(|resources| resources.plan())
            .map_err(|error| error.to_string())
        }

        // Admission bounds follow the plan's effective CPU, not a new knob.
        let plan = plan_for(&[BifrostRole::Forge], None).expect("dedicated Forge plans");
        let forge_runtime = crate::config::ForgeRuntimeConfig::default();
        let worker = super::forge_compaction_worker_config(&plan, &forge_runtime);
        assert_eq!(
            worker.compaction_memory_budget_bytes, plan.forge_compaction_memory_limit_bytes,
            "the worker charges plans against exactly the reserved budget"
        );
        assert_eq!(worker.max_task_parallelism, 18, "three per effective CPU");
        assert_eq!(
            worker.pending_task_parallelism, 72,
            "four times running parallelism may wait"
        );
        assert_eq!(worker.per_tenant_active_cap, 1);
        worker
            .validate()
            .expect("composition-derived bounds are usable");

        // The same derivation holds for co-located `All`, which additionally
        // preserves both protected floors.
        let all = plan_for(
            &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
            None,
        )
        .expect("co-located All plans");
        assert!(all.scribe_floor_bytes > 0 && all.oracle_floor_bytes > 0);
        super::forge_compaction_worker_config(&all, &forge_runtime)
            .validate()
            .expect("co-located bounds are usable");

        // Forge absent: nothing is reserved, so nothing may be admitted, which
        // is what makes the executor role-scoped rather than always-composed.
        let absent =
            plan_for(&[BifrostRole::Scribe, BifrostRole::Oracle], None).expect("Forge-absent plan");
        assert_eq!(absent.forge_compaction_memory_limit_bytes, 0);
        assert!(
            super::forge_compaction_worker_config(&absent, &forge_runtime)
                .validate()
                .is_err(),
            "a node that reserved nothing must not compose an admitting worker"
        );

        // Invalid budgets refuse at planning, before any executor is built.
        assert!(
            plan_for(&[BifrostRole::Forge], Some(0)).is_err(),
            "a zero budget refuses boot"
        );
        let safe = all.managed_memory_bytes - all.scribe_floor_bytes - all.oracle_floor_bytes;
        assert!(
            plan_for(
                &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
                Some(safe + 1),
            )
            .is_err(),
            "a budget past the protected floors refuses boot, never clamps"
        );

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

    /// Orphan listing refuses a backend that cannot resume from a cursor.
    ///
    /// The bounded orphan scan's only anti-starvation mechanism is an exclusive
    /// `start_after` cursor: without native support a leading page of protected
    /// objects would be relisted forever and the later pages behind it would
    /// never be reached. Emulating the cursor by filtering would reintroduce the
    /// unbounded listing the cap exists to prevent, so the production adapter
    /// fails closed instead. The filesystem service is the already-installed
    /// backend that advertises the capability as absent, which makes it the
    /// exact case this refusal exists for.
    ///
    /// # Panics
    ///
    /// Panics when the filesystem operator cannot be built, when it starts
    /// advertising cursor support, or when listing is permitted anyway.
    #[tokio::test]
    async fn open_dal_forge_listing_refuses_backend_without_start_after() {
        let root = tempfile::tempdir().expect("listing root");
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(&root.path().to_string_lossy()),
        )
        .expect("filesystem operator builds")
        .finish();
        assert!(
            !operator.info().full_capability().list_with_start_after,
            "the filesystem service is the backend this refusal exists for"
        );
        let store = OpenDalForgeObjectStore::new(Arc::new(operator));
        for cursor in [None, Some("tenants/t/table/data/forge/v1/a.parquet")] {
            let error = store
                .list_pages("tenants/t/table/data/forge/v1/", cursor)
                .await
                .err()
                .expect("a backend without cursor support cannot be listed");
            assert_eq!(error.kind(), opendal::ErrorKind::Unsupported);
        }
    }

    /// A capable lister's directory entries never consume the page bound.
    ///
    /// The production page bound counts addressable objects, because the task
    /// cursor advances over objects only. A chunk of leading directory markers
    /// would otherwise yield an empty page and leave the frontier stationary,
    /// starving the later object behind it. The filesystem service plus the
    /// installed capability override is the smallest backend that both yields
    /// directory entries and advertises cursor support.
    ///
    /// # Panics
    ///
    /// Panics when the operator cannot be built, when the override does not
    /// advertise cursor support, when the single page is not exactly the one
    /// object, or when the stream yields another page.
    #[tokio::test]
    async fn open_dal_forge_listing_filters_directories_before_page_boundary() {
        let root = tempfile::tempdir().expect("listing root");
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(&root.path().to_string_lossy()),
        )
        .expect("filesystem operator builds")
        .layer(opendal::layers::CapabilityOverrideLayer::new(
            |mut capability| {
                capability.list_with_start_after = true;
                capability
            },
        ))
        .finish();
        assert!(
            operator.info().full_capability().list_with_start_after,
            "the overridden operator must advertise cursor support"
        );
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
            .expect("a cursor-capable backend is listable");
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

    /// Boot rejects an intrinsic replay envelope while accepting its exact boundary.
    ///
    /// # Panics
    ///
    /// Panics if configured maximum-envelope derivation overflows, a root one
    /// byte too small is accepted, or the exact detected capability is refused.
    #[test]
    fn scribe_boot_rejects_intrinsically_unreplayable_config() {
        let config = crate::config::ScribeRuntimeConfig::default();
        let required =
            vala_bifrost_redux::scribe::configured_maximum_envelope_bytes(config.ingest_limits())
                .expect("configured replay envelope");
        let error = validate_scribe_replay_envelope(config, required - 1)
            .expect_err("intrinsically unreplayable root must fail boot");
        assert!(error.to_string().contains("configured replay envelope"));
        validate_scribe_replay_envelope(config, required)
            .expect("exact replay envelope must remain bootable");
    }

    /// Server composition appends one disposable component before detection.
    #[test]
    fn config_composes_one_oracle_spill_root_before_detection() {
        let base = tempfile::tempdir().expect("scratch base");
        let root = prepare_oracle_spill_root(Some(base.path().to_path_buf()))
            .expect("scratch root creation");
        assert_eq!(root, base.path().join("oracle-spill"));
        assert!(root.is_dir());
        assert!(!root.join("oracle-spill").exists());
    }

    /// An uncreatable scratch child fails before resource-owner construction.
    #[test]
    fn config_rejects_uncreatable_oracle_spill_root_before_activation() {
        let base = tempfile::tempdir().expect("scratch fixture");
        let file = base.path().join("not-a-directory");
        std::fs::write(&file, b"occupied").expect("blocking file");
        let error = prepare_oracle_spill_root(Some(file))
            .expect_err("file-backed base cannot create a scratch child");
        assert!(matches!(error, ServerBootError::OraclePeer(_)));
    }

    /// An empty `forge` config resolves to the compiled `ForgeConfig` default
    /// and the default maintenance interval, pinning byte-identical no-config
    /// behavior (AC1).
    #[test]
    fn resolve_forge_config_defaults_match_compiled_defaults() {
        let (config, maintenance_interval) =
            resolve_forge_config(&crate::config::ForgeRuntimeConfig::default());
        assert_eq!(config, ForgeConfig::default());
        assert_eq!(maintenance_interval, DEFAULT_MAINTENANCE_INTERVAL);
    }

    /// Supplied `forge` values override the compiled defaults on exactly the
    /// promoted fields, and the resolved config still validates fail-closed.
    #[test]
    fn resolve_forge_config_applies_supplied_overrides() {
        let runtime = crate::config::ForgeRuntimeConfig {
            snapshot_retention_secs: Some(7_200),
            retain_last: Some(3),
            orphan_gc_ttl_secs: Some(3_600),
            maintenance_trigger_snapshot_count: Some(8),
            maintenance_trigger_interval_secs: Some(900),
            orphan_gc_max_list_pages: Some(64),
            orphan_gc_run_budget_secs: Some(30),
            maintenance_interval_secs: Some(45),
            ..crate::config::ForgeRuntimeConfig::default()
        };
        let (config, maintenance_interval) = resolve_forge_config(&runtime);
        assert_eq!(
            config.snapshot_retention,
            std::time::Duration::from_secs(7_200)
        );
        assert_eq!(config.retain_last, 3);
        assert_eq!(config.orphan_gc_ttl, std::time::Duration::from_secs(3_600));
        assert_eq!(config.maintenance_trigger_snapshot_count, 8);
        assert_eq!(
            config.maintenance_trigger_interval,
            std::time::Duration::from_secs(900)
        );
        assert_eq!(config.orphan_gc_max_list_pages, 64);
        assert_eq!(
            config.orphan_gc_run_budget,
            std::time::Duration::from_secs(30)
        );
        assert_eq!(maintenance_interval, std::time::Duration::from_secs(45));
        config
            .validate()
            .expect("resolved override config must validate");
        // Fields outside the promoted set retain their compiled defaults.
        assert_eq!(config.lease_ttl, ForgeConfig::default().lease_ttl);
    }

    /// A zeroed promoted duration resolves through and is rejected by the
    /// downstream `ForgeConfig::validate` fail-closed check.
    #[test]
    fn resolve_forge_config_zero_value_is_rejected_by_validate() {
        let runtime = crate::config::ForgeRuntimeConfig {
            snapshot_retention_secs: Some(0),
            ..crate::config::ForgeRuntimeConfig::default()
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

    #[test]
    fn real_authz_audit_writer_is_not_stub() {
        use crate::components::auth::audit_writer::{
            AuthzAuditWriter, NoopAuthzAuditWriter, RealAuthzAuditWriter,
        };

        assert!(
            NoopAuthzAuditWriter.is_stub_default(),
            "noop writer must be a stub"
        );
        assert!(
            !RealAuthzAuditWriter.is_stub_default(),
            "RealAuthzAuditWriter must not be a stub"
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
            crate::test_support::test_server_postgres().await,
            crate::test_support::test_storage().await,
            crate::test_support::test_catalog().await,
        )
    }

    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    #[tokio::test(flavor = "current_thread")]
    async fn state_overrides_authz_applied() {
        use wyrd_auth_check::DenyAllPolicyHook;

        let state = make_test_state().await;
        assert!(state.authz.policy_hook.is_stub_default(), "default is stub");

        let non_stub_authz = ServerAuthz {
            policy_hook: Arc::new(DenyAllPolicyHook {
                reason: "test-override".to_owned(),
            }),
            ..ServerAuthz::default()
        };
        let overrides = StateOverrides {
            authz: Some(non_stub_authz),
            eval_audit: None,
            ..StateOverrides::default()
        };
        let patched = apply_overrides(state, overrides);

        assert!(
            !patched.authz.policy_hook.is_stub_default(),
            "override replaced stub policy hook"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn state_overrides_eval_audit_applied() {
        use crate::components::eval::{EvalAuditEvent, EvalAuditWriter};

        struct MarkerWriter;
        impl EvalAuditWriter for MarkerWriter {
            fn record(&self, _event: &EvalAuditEvent) {}
        }

        let state = make_test_state().await;
        // Confirm default is the tracing-backed stub (Arc::ptr_eq won't work
        // across two Arc<dyn Trait> directly, so we replace and check the new
        // instance is reachable via the state field).
        let marker: Arc<dyn EvalAuditWriter> = Arc::new(MarkerWriter);
        let marker_ptr = Arc::as_ptr(&marker) as *const ();

        let overrides = StateOverrides {
            authz: None,
            eval_audit: Some(marker),
            ..StateOverrides::default()
        };
        let patched = apply_overrides(state, overrides);

        assert_eq!(
            Arc::as_ptr(&patched.eval_audit) as *const (),
            marker_ptr,
            "override replaced the default eval audit writer"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn state_overrides_default_is_noop() {
        let state = make_test_state().await;
        assert!(state.authz.policy_hook.is_stub_default());
        assert!(state.authz.audit_writer.is_stub_default());

        let patched = apply_overrides(state, StateOverrides::default());

        assert!(
            patched.authz.policy_hook.is_stub_default(),
            "authz unchanged"
        );
        assert!(
            patched.authz.audit_writer.is_stub_default(),
            "audit unchanged"
        );
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
