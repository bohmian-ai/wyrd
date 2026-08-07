//! Server boot sequence for SQL-backed Wyrd runtime state.

pub mod auth;
pub mod bootstrap;
pub mod issuer;

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeObjectStore, ForgeRewriteRuntime,
    ForgeTelemetry, ForgeWorker, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::oracle::dispatcher::{
    LocalOraclePeerTransport, OraclePeerCredentials, OraclePeerTls, OraclePeerTransportDirectory,
    OraclePeerWorker, PEER_PROTOCOL_VERSION, ReservationRegistry, TonicOraclePeerTransport,
};
use vala_bifrost_redux::oracle::executor::SealedFragmentExecutor;
use vala_bifrost_redux::oracle::{
    Oracle, OracleBuildConfig, OracleConfig, OracleMemoryResources, OracleSlotManager,
    TailTransportDirectory,
};
use vala_bifrost_redux::scribe::admission::{AdmissionConfig, EventTimeWindow};
use vala_bifrost_redux::scribe::memory::{BifrostDataFusionMemoryPool, BifrostMemoryGovernor};
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
    OracleAuditPublisher, OraclePeerAuthority, OraclePeerRuntime, PostgresPeerSecurityAudit,
    ServerOraclePeerCredentials,
};
use crate::postgres::ServerPostgres;
use crate::state::{
    AppState, BifrostIngestRuntime, BifrostQueryRuntime, ProductionValidationError,
};

const DEFAULT_MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const DEFAULT_HINT_CAPACITY: usize = 1_024;
const ORACLE_STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

/// Holds shared and role-specific Bifrost dependencies during server boot.
struct BifrostBootParts {
    /// State without mounted Gate role adapters.
    state: AppState,
    /// Optional recovered Scribe resources selected by runtime role config.
    scribe: Option<ScribeBootParts>,
    /// Cluster registry shared by independently fenced local roles.
    cluster_registry: Arc<ClusterRegistry>,
    /// Physical node identity shared by independently fenced local roles.
    node_id: ClusterNodeId,
}

/// Owns resources created only when the Scribe role is selected.
struct ScribeBootParts {
    /// Recovered Scribe allocation that Gate wraps for ingest.
    scribe: Arc<ScribeImpl>,
    /// Dedicated runtime that owns Scribe coordination tasks.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Independently fenced Scribe role registered during this boot.
    scribe_role: RegisteredRole,
}

impl BifrostBootParts {
    /// Roll back resources that were activated before the enclosing server
    /// state became usable.
    ///
    /// Scribe recovery and role registration happen before auth/Oracle
    /// composition. Any later boot error must close the writer and release its
    /// exact fence; this method is intentionally idempotent and never starts a
    /// second shutdown budget.
    async fn rollback(&mut self) {
        if let Some(scribe) = &mut self.scribe {
            scribe.scribe.shutdown(std::time::Instant::now()).await;
            if let Err(error) = self
                .cluster_registry
                .shutdown_role(scribe.scribe_role.clone())
                .await
            {
                tracing::warn!(%error, "failed to release Scribe role during boot rollback");
            }
            if let Some(runtime) = scribe.coordination_runtime.take()
                && let Ok(runtime) = Arc::try_unwrap(runtime)
            {
                let _ = tokio::task::spawn_blocking(move || {
                    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
                })
                .await;
            }
        }
    }
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
    /// Bifrost catalog construction failed.
    #[error(transparent)]
    Bifrost(#[from] vala_bifrost::error::BifrostError),
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

/// Resolve database configuration and assemble a Scribe/Forge test fixture.
///
/// Production boot uses the role-aware authenticated builder. This convenience
/// surface remains available only to crate tests and the `test-support` feature
/// so its intentionally absent Oracle/auth composition cannot become a server
/// activation path.
///
/// # Errors
/// Returns [`ServerBootError`] when database boot, migration, or runtime pool
/// construction fails.
#[cfg(any(test, feature = "test-support"))]
pub async fn build_app_state() -> Result<AppState, ServerBootError> {
    let boot = PostgresBoot::from_env().await?;
    build_app_state_from_boot_with_config(&boot, crate::config::ScribeRuntimeConfig::default())
        .await
}

/// Assemble a Scribe/Forge test fixture from a resolved Postgres boot mode.
///
/// # Errors
/// Returns [`ServerBootError`] when DSN resolution, migrations, or runtime
/// pool construction fails.
#[cfg(any(test, feature = "test-support"))]
pub async fn build_app_state_from_boot(boot: &PostgresBoot) -> Result<AppState, ServerBootError> {
    build_app_state_from_boot_with_config(boot, crate::config::ScribeRuntimeConfig::default()).await
}

/// Assemble a Scribe/Forge test fixture with an explicit Scribe configuration.
///
/// # Errors
///
/// Returns [`ServerBootError`] when database, storage, catalog, Scribe, or Forge
/// fixture construction fails.
#[cfg(any(test, feature = "test-support"))]
pub async fn build_app_state_from_boot_with_config(
    boot: &PostgresBoot,
    scribe_config: crate::config::ScribeRuntimeConfig,
) -> Result<AppState, ServerBootError> {
    let bifrost_config = crate::config::BifrostRuntimeConfig {
        scribe: scribe_config,
        ..crate::config::BifrostRuntimeConfig::default()
    };
    let roles = [BifrostRuntimeRole::Scribe, BifrostRuntimeRole::Forge]
        .into_iter()
        .collect();
    Ok(build_bifrost_parts_from_boot(boot, &bifrost_config, &roles)
        .await?
        .state)
}

/// Builds state plus unmounted Scribe dependencies for the authenticated boot path.
///
/// # Errors
///
/// Returns [`ServerBootError`] when database, storage, catalog, WAL, execution
/// pool, recovery, or Forge setup fails.
async fn build_bifrost_parts_from_boot(
    boot: &PostgresBoot,
    bifrost_config: &crate::config::BifrostRuntimeConfig,
    roles: &std::collections::BTreeSet<BifrostRuntimeRole>,
) -> Result<BifrostBootParts, ServerBootError> {
    if roles.contains(&BifrostRuntimeRole::Scribe) {
        bifrost_config
            .scribe
            .validate()
            .map_err(ServerBootError::Scribe)?;
    }
    let dsns = boot.dsns()?;
    let postgres = Arc::new(ServerPostgres::connect_from_boot(boot).await?);
    let storage_settings = load_storage_settings()?;
    tracing::info!(
        backend = %storage_settings.backend.kind(),
        require_encryption = storage_settings.require_encryption,
        presign_ttl_secs = storage_settings.presign_ttl.as_secs(),
        part_size_bytes = storage_settings.part_size_bytes,
        "storage settings loaded"
    );
    let storage = StorageHandle::from_settings(storage_settings).await?;
    tracing::info!(backend = %storage.backend(), "storage handle ready");
    let bifrost = WyrdCatalog::new(
        dsns.catalog_app.expose_secret(),
        storage.backend_config(),
        Arc::new(postgres.app_pool().clone()),
    )
    .await?;
    let bifrost = Arc::new(bifrost);

    // Provision the pre-declared OLAP domain tables (traces.spans, genai.*, ...)
    // so the ingest and query paths have their physical Iceberg tables. Idempotent
    // and append-only — a no-op after first boot; fails closed on schema drift.
    vala_bifrost::tables::register_all(&bifrost).await?;
    let bifrost_redux = Arc::new(
        BifrostCatalog::new(
            dsns.catalog_app.expose_secret(),
            storage.backend_config(),
            postgres.vala().clone(),
        )
        .await
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
    );

    let operator_pool =
        postgres
            .operator_pool()
            .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
                detail: "platform-admin operator pool is unavailable".to_owned(),
            })?;
    let node_id = NodeId::generate();
    let advertise_addr =
        std::env::var("WYRD_SERVER_ADVERTISE_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_owned());
    let cluster_registry = Arc::new(ClusterRegistry::new(
        postgres.vala().clone(),
        ClusterNodeId::new(node_id.as_uuid()),
    ));
    let scribe_config = bifrost_config.scribe;
    let pod_memory_limit = BifrostMemoryGovernor::detect(1024 * 1024 * 1024)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?
        .pod_limit_bytes();
    let bifrost_memory = BifrostMemoryGovernor::new_with_child_limits(
        pod_memory_limit,
        scribe_config.memory_limit_bytes,
        bifrost_config.oracle.memory_limit_bytes,
    )
    .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    let bifrost_datafusion_memory_pool = Arc::new(BifrostDataFusionMemoryPool::for_oracle(
        bifrost_memory.clone(),
    ));
    let (staging_file_publisher, staging_file_inbox) = staging_file_channel(DEFAULT_HINT_CAPACITY)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    let wal_dir = std::env::var_os("WYRD_SCRIBE_WAL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(".wyrd/scribe-wal"));
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
            node_id,
            WriterEpoch::new(i64::try_from(scribe_role.fencing_token).map_err(|_| {
                ServerBootError::Scribe("Scribe role fence exceeds the WAL epoch range".to_owned())
            })?),
        );
        std::fs::create_dir_all(&wal_dir).map_err(|error| {
            ServerBootError::Scribe(format!("WAL directory creation failed: {error}"))
        })?;
        let wal = Arc::new(
            WalWriter::new(
                &wal_dir,
                *stream.node_id.as_bytes(),
                stream.writer_epoch.as_i64(),
                WalConfig::default()
                    .with_disk_limit(scribe_config.wal_disk_limit_bytes)
                    .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
            )
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        let coordination_runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
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
                })?,
        );
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
            ScribeWalIoPool::try_new_with_capacity(scribe_config.wal_io_threads, 256)
                .map_err(|error| ServerBootError::Scribe(format!("WAL IO pool failed: {error}")))?,
        );
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            operator: Arc::new(storage.operator().clone()),
            wal,
            stream,
            admission: AdmissionConfig {
                memory_limit_bytes: pod_memory_limit,
                scribe_memory_limit_bytes: scribe_config.memory_limit_bytes,
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
            },
            coordination_runtime: coordination_runtime.handle().clone(),
            execution_pools,
            persistence: Some(
                ScribePersistenceConfig::new(
                    Arc::new(postgres.vala().clone()),
                    64,
                    scribe_config.wal_io_threads,
                )
                .with_operator_pool(postgres.operator_pool().ok_or_else(|| {
                    ServerBootError::Scribe(
                        "Scribe publication requires the platform operator pool".to_owned(),
                    )
                })?),
            ),
            memory_budget: Some(bifrost_memory.scribe_budget()),
            staging_file_publisher: Some(staging_file_publisher),
        }));
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
            coordination_runtime: Some(coordination_runtime),
            scribe_role,
        })
    } else {
        None
    };

    let forge = if roles.contains(&BifrostRuntimeRole::Forge) {
        let forge_config = ForgeConfig::default();
        let forge_datafusion_memory_pool = Arc::new(BifrostDataFusionMemoryPool::for_parent(
            bifrost_memory.clone(),
        ));
        let rewrite_runtime = ForgeRewriteRuntime::new(
            forge_datafusion_memory_pool,
            &wal_dir.join("forge-spill"),
            forge_config.spill_limit_bytes,
        )?;
        let staging = Arc::new(storage.operator().clone());
        let object_store: Arc<dyn ForgeObjectStore> =
            Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging)));
        Some(Arc::new(Forge::new(ForgeBuildConfig {
            vala: postgres.vala().clone(),
            operator_pool: operator_pool.clone(),
            catalog: bifrost_redux.iceberg_catalog(),
            staging,
            object_store,
            rewrite_runtime,
            hints: staging_file_inbox,
            config: forge_config,
            maintenance_interval: DEFAULT_MAINTENANCE_INTERVAL,
            clock: ForgeClock::system(),
            completion_observer: None,
            scheduler_trigger: None,
            telemetry: Arc::new(ForgeTelemetry::new()),
        })?))
    } else {
        None
    };

    let mut state = AppState::new(postgres, storage, bifrost)
        .with_bifrost_node_id(ClusterNodeId::new(node_id.as_uuid()))
        .with_bifrost_redux(bifrost_redux)
        .with_bifrost_memory_pool(bifrost_memory, bifrost_datafusion_memory_pool);
    if let Some(forge) = forge {
        state = state.with_forge(forge);
    }
    Ok(BifrostBootParts {
        state,
        scribe,
        cluster_registry,
        node_id: ClusterNodeId::new(node_id.as_uuid()),
    })
}

/// Build the bounded Forge worker future shared by embedded and worker roles.
///
/// # Errors
/// Returns [`ServerBootError::ForgeSchedulerRequired`] when Forge is absent or
/// [`ServerBootError::Forge`] when the requested worker bound is invalid.
pub fn spawn_forge_worker(
    state: &AppState,
    shutdown: CancellationToken,
    worker_concurrency: usize,
) -> Result<
    impl std::future::Future<Output = Result<(), vala_bifrost_redux::forge::ForgeError>>
    + Send
    + 'static,
    ServerBootError,
> {
    let forge =
        state
            .forge_handle()
            .cloned()
            .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
                detail: "AppState has no shared Forge for worker execution".to_owned(),
            })?;
    let node_id =
        state
            .bifrost_node_id()
            .ok_or_else(|| ServerBootError::ForgeSchedulerRequired {
                detail: "AppState has no configured physical Bifrost node identity".to_owned(),
            })?;
    let worker = ForgeWorker::new(
        forge,
        ForgeWorkerConfig { worker_concurrency },
        node_id.as_uuid(),
    )
    .map_err(ServerBootError::Forge)?;
    Ok(async move { worker.run(shutdown).await })
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
/// # Errors
/// Returns [`ServerBootError`] on database/storage/bifrost boot, auth handle
/// construction, federation seeding, or production validation failure.
pub async fn build_state(
    config: &crate::config::WyrdServerConfig,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
    overrides: StateOverrides,
) -> Result<AppState, ServerBootError> {
    let shutdown = CancellationToken::new();

    let boot = PostgresBoot::from_env().await?;
    let roles = config.bifrost_roles();
    let mut bifrost_parts = build_bifrost_parts_from_boot(&boot, &config.bifrost, &roles).await?;
    #[cfg(feature = "test-support")]
    if overrides.fail_after_scribe_activation {
        bifrost_parts.rollback().await;
        return Err(ServerBootError::Scribe(
            "test-injected failure after Scribe activation".to_owned(),
        ));
    }
    let state = bifrost_parts.state.clone();
    // A dedicated Forge worker is intentionally a narrow process graph. It
    // reuses the durable Forge/catalog dependencies assembled above, but does
    // not construct auth, Gate, Scribe WAL ownership, Oracle membership, or
    // Oracle TLS/calibration state. This branch must remain before any
    // server-only side effect such as signing-key resolution or federation
    // seeding.
    if matches!(config.role, crate::config::ForgeProcessRole::ForgeWorker) {
        let state = attach_config_fields(state, config, shutdown, telemetry)?;
        state
            .production_validate()
            .map_err(ServerBootError::ProductionValidation)?;
        return Ok(state);
    }
    let sealing_key = match build_sealing_key(config) {
        Ok(key) => key,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let signing_key = match resolve_signing_key(config) {
        Ok(key) => key,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };

    let state = match attach_config_fields(state, config, shutdown, telemetry) {
        Ok(state) => state,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let state = match install_auth(state, config, &signing_key, sealing_key.clone()).await {
        Ok(state) => state,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let state = match (OracleRoleBuilder {
        state,
        config,
        signing_key: &signing_key,
        cluster: Arc::clone(&bifrost_parts.cluster_registry),
        node_id: bifrost_parts.node_id,
        advertise_addr: &config.bifrost.oracle.advertise_addr,
        #[cfg(feature = "test-support")]
        peer_credentials: None,
    }
    .build()
    .await)
    {
        Ok(state) => state,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let verifier = match state
        .auth
        .token_verifier
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires a token verifier".to_owned()))
    {
        Ok(verifier) => verifier,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let bifrost_redux = match state
        .bifrost_redux
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires the Redux catalog".to_owned()))
    {
        Ok(catalog) => catalog,
        Err(error) => {
            bifrost_parts.rollback().await;
            return Err(error);
        }
    };
    let limits = vala_bifrost_redux::gate::limits::IngestLimits::default();
    let state = if roles.contains(&BifrostRuntimeRole::Scribe) {
        let scribe_parts = bifrost_parts.scribe.ok_or_else(|| {
            ServerBootError::Scribe("selected Scribe role was not constructed".to_owned())
        })?;
        let tail_audit = Arc::new(
            crate::oracle::PostgresTailSecurityAudit::try_new(&state.postgres)
                .await
                .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
        );
        let ingest = Arc::new(
            BifrostIngestRuntime::new(
                scribe_parts.scribe,
                Arc::clone(&bifrost_redux),
                Arc::clone(&verifier),
                limits.clone(),
                Some(
                    scribe_parts
                        .coordination_runtime
                        .expect("Scribe coordination runtime remains during state composition"),
                ),
            )
            .with_tail_authority(Arc::new(
                crate::oracle::ScribeTailAuthority::from_pem(&signing_key, tail_audit)
                    .map_err(|error| ServerBootError::Scribe(error.to_string()))?,
            ))
            .with_scribe_role(
                Arc::clone(&bifrost_parts.cluster_registry),
                scribe_parts.scribe_role,
            ),
        );
        state.with_bifrost_ingest(ingest)
    } else {
        state
    };
    let mut gate = match &state.bifrost_ingest {
        Some(ingest) => vala_bifrost_redux::gate::Gate::with_scribe_and_projection(
            bifrost_redux,
            ingest.scribe().clone(),
            vala_bifrost_redux::gate::auth::ingest_auth_interceptor(verifier),
            limits,
            Arc::new(vala_bifrost_redux::gate::IngressCpuProjection::new(
                ingest.scribe().ingress_cpu_pool(),
            )),
        ),
        None => vala_bifrost_redux::gate::Gate::without_scribe(
            bifrost_redux,
            vala_bifrost_redux::gate::auth::ingest_auth_interceptor(verifier),
            limits,
        ),
    };
    if let Some(query) = state.bifrost_query() {
        gate = gate.with_oracle(Arc::clone(query.oracle()));
    }
    let state = state.with_bifrost_gate(Arc::new(gate));
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
    Ok(state)
}

/// Tear down role-owned state after server-only boot stages fail.
///
/// Oracle and Scribe fences are activated before federation and production
/// validation. This helper reuses their normal owner/registry shutdown phases
/// with an already-expired deadline, ensuring cancellation and retained-task
/// abort happen without granting a second cleanup budget.
async fn rollback_state_roles(state: &AppState) {
    let deadline = std::time::Instant::now();
    if let Some(query) = state.bifrost_query() {
        query.abort_shutdown();
        query.shutdown_owner(deadline).await;
        query.shutdown_registry(deadline).await;
    }
    if let Some(ingest) = &state.bifrost_ingest {
        ingest.abort_shutdown();
        ingest.shutdown_owner(deadline).await;
        ingest.shutdown_registry(deadline).await;
    }
}

/// Resolves the one Wyrd signing authority shared by auth and Oracle peers.
///
/// Development may generate an ephemeral key so the default mixed-role server
/// still constructs a real local Oracle. Production requires configured key
/// material and never falls back to an ephemeral authority.
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
         signing key shared by auth and the local Oracle peer. Tokens and peer tickets \
         will not survive a restart and this key must never be used in production."
    );
    Ok(ephemeral)
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
    shutdown: CancellationToken,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
) -> Result<AppState, ServerBootError> {
    Ok(state
        .with_deployment_profile(config.deployment_profile)
        .with_bifrost_roles(config.bifrost_roles())
        .with_shutdown_token(shutdown)
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
    state: AppState,
    config: &crate::config::WyrdServerConfig,
    signing_key: &SecretString,
    sealing_key: Option<Arc<SecretKey>>,
) -> Result<AppState, ServerBootError> {
    // Postgres is the single source of issuer/binding resolution. Both resolvers
    // are always attached; an empty config simply means the tenant federates no
    // issuers and binds no workloads, which they resolve as empty results. The
    // issuer resolver also feeds the token verifier's external (foreign-OIDC)
    // path so federated tokens can be exchanged per-request.
    let issuer_resolver = Arc::new(PgIssuerResolver::new(
        Arc::new(state.postgres.app_pool().clone()),
        sealing_key.clone(),
    ));
    let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(Arc::new(
        state.postgres.app_pool().clone(),
    )));

    let (issuing_key, verifier) = crate::boot::auth::build_auth_handles(
        signing_key,
        state.postgres.app_pool(),
        Arc::clone(&issuer_resolver),
    )?;

    Ok(state.with_auth(ServerAuth {
        allow_preview: config.auth.allow_preview,
        issuing_key: Some(issuing_key),
        token_verifier: Some(verifier),
        trusted_issuer_resolver: Some(Arc::clone(&issuer_resolver)),
        workload_binding_resolver: Some(binding_resolver),
        sealing_key: sealing_key.clone(),
        token_exchange_settings: crate::auth::exchange_api_key::TokenExchangeSettings::default(),
    }))
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
    /// Server state containing SQL, auth, storage, and memory owners.
    state: AppState,
    /// Validated server configuration borrowed for this activation.
    config: &'a crate::config::WyrdServerConfig,
    /// Server signing authority used by peer tickets.
    signing_key: &'a SecretString,
    /// Cluster registry owning the role fence and readiness publication.
    cluster: Arc<ClusterRegistry>,
    /// Stable node identity advertised to peer services.
    node_id: ClusterNodeId,
    /// Bound endpoint published in cluster membership.
    advertise_addr: &'a str,
    /// Optional test-harness credential owner replacing environment discovery.
    #[cfg(feature = "test-support")]
    peer_credentials: Option<Arc<dyn OraclePeerCredentials>>,
}

impl<'a> OracleRoleBuilder<'a> {
    /// Constructs one fenced Oracle role without starting or publishing it.
    async fn build(self) -> Result<AppState, ServerBootError> {
        if !self
            .config
            .bifrost_roles()
            .contains(&BifrostRuntimeRole::Oracle)
        {
            return Ok(self.state);
        }
        let built = self.construct().await?;
        built.start_reconcile_activate_publish().await
    }

    /// Constructs one Oracle role and retains its reserved fence.
    ///
    /// # Errors
    /// Returns [`ServerBootError::OraclePeer`] when security, dependency, or
    /// durable-role construction fails.
    async fn construct(self) -> Result<BuiltOracleRole, ServerBootError> {
        let Self {
            state,
            config,
            signing_key,
            cluster,
            node_id,
            advertise_addr,
            #[cfg(feature = "test-support")]
                peer_credentials: injected_peer_credentials,
        } = self;
        let security_audit = Arc::new(
            PostgresPeerSecurityAudit::try_new(&state.postgres)
                .await
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let authority = Arc::new(
            OraclePeerAuthority::from_pem(signing_key, security_audit.clone())
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let memory = state.bifrost_memory.clone().ok_or_else(|| {
            ServerBootError::OraclePeer("shared Bifrost memory governor is absent".to_owned())
        })?;
        let catalog = state
            .bifrost_redux
            .as_ref()
            .ok_or_else(|| ServerBootError::OraclePeer("Redux catalog is absent".to_owned()))?;
        let configured_cpu = config.bifrost.oracle.cpu_cores;
        let cpu_cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        let cpu_cores = u32::try_from(cpu_cores)
            .map_err(|_| ServerBootError::OraclePeer("CPU count exceeds u32".to_owned()))?;
        let memory_bytes_per_slot = 256_u64 * 1024 * 1024;
        // Derive Oracle sizing from the resolved child limit — no unbounded-parent fallback (D79).
        let memory_budget = memory.oracle_limit_bytes();
        let memory_budget_bytes = u64::try_from(memory_budget)
            .map_err(|_| ServerBootError::OraclePeer("memory budget exceeds u64".to_owned()))?;
        let memory_slots = (memory_budget_bytes / memory_bytes_per_slot).max(1);
        let raw_slots = u32::try_from(u64::from(cpu_cores).min(memory_slots).max(1))
            .map_err(|_| ServerBootError::OraclePeer("Oracle slot count exceeds u32".to_owned()))?;
        let calibrated = crate::config::load_oracle_admission_translation(
            &config.bifrost.oracle,
            raw_slots,
            memory_budget_bytes,
        )
        .map_err(ServerBootError::OraclePeer)?;
        let capabilities = OracleCapabilitiesV1 {
            peer_protocol_version: u16::try_from(PEER_PROTOCOL_VERSION)
                .map_err(|_| ServerBootError::OraclePeer("peer protocol exceeds u16".to_owned()))?,
            storage_protocol_version: 1,
            cpu_cores: configured_cpu,
            memory_budget_bytes,
            cpu_cores_per_slot: 1.0,
            memory_bytes_per_slot,
            raw_slots,
            usable_slots: raw_slots,
            supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
            max_workers_per_query: u32::try_from(config.bifrost.oracle.max_workers_per_query)
                .map_err(|_| ServerBootError::OraclePeer("worker fanout exceeds u32".to_owned()))?,
        };
        let running_slots = usize::try_from(raw_slots).map_err(|_| {
            ServerBootError::OraclePeer("Oracle slot count exceeds usize".to_owned())
        })?;
        let slots = Arc::new(OracleSlotManager::new(
            config.bifrost.oracle.admission_waiters,
            running_slots,
        ));
        let reservations = Arc::new(ReservationRegistry::new(Arc::clone(&slots), 1_024));
        let snapshot = cluster.snapshot();
        validate_remote_oracle_addresses(
            config.deployment_profile,
            snapshot
                .live_oracles()
                .into_iter()
                .filter(|lease| lease.key.node_id != node_id)
                .map(|lease| lease.address.as_str()),
        )?;
        #[cfg(feature = "test-support")]
        let peer_credentials: Arc<dyn OraclePeerCredentials> = match injected_peer_credentials {
            Some(credentials) => credentials,
            None => Arc::new(
                ServerOraclePeerCredentials::from_env().map_err(ServerBootError::OraclePeer)?,
            ),
        };
        #[cfg(not(feature = "test-support"))]
        let peer_credentials: Arc<dyn OraclePeerCredentials> =
            Arc::new(ServerOraclePeerCredentials::from_env().map_err(ServerBootError::OraclePeer)?);
        let initial_bearer = peer_credentials.bearer(false).await.map_err(|_| {
            ServerBootError::OraclePeer("Oracle peer credential exchange failed".to_owned())
        })?;
        let verifier =
            state.auth.token_verifier.as_ref().ok_or_else(|| {
                ServerBootError::OraclePeer("auth backend not configured".to_owned())
            })?;
        let mut metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        let bearer = format!("Bearer {initial_bearer}").parse().map_err(|_| {
            ServerBootError::OraclePeer("Oracle peer access token is invalid".to_owned())
        })?;
        metadata.insert("x-wyrd-access-token", bearer);
        let authenticated =
            vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), &metadata)
                .await
                .map_err(|_| {
                    ServerBootError::OraclePeer("Oracle peer access token was rejected".to_owned())
                })?;
        if !matches!(authenticated.principal.kind, PrincipalKind::Service { .. })
            || authenticated.principal.tenant_id != DataTenantId::SYSTEM_OWNER
            || !authenticated
                .principal
                .effective_permissions
                .contains(&Permission::bifrost_oracle_peer_invoke())
        {
            return Err(ServerBootError::OraclePeer(
                "Oracle peer credential lacks platform service authority".to_owned(),
            ));
        }
        let tail_tls = if let (Some(ca_path), Some(server_name)) = (
            &config.bifrost.oracle.peer_ca_certificate_path,
            &config.bifrost.oracle.peer_server_name,
        ) {
            let ca_certificate_pem = std::fs::read(ca_path).map_err(|_| {
                ServerBootError::OraclePeer(format!(
                    "failed to read Oracle peer CA certificate {}",
                    ca_path.display()
                ))
            })?;
            Some(OraclePeerTls::new(ca_certificate_pem, server_name.clone()))
        } else {
            None
        };
        let remote_transport = if let Some(tls) = tail_tls.clone() {
            Arc::new(TonicOraclePeerTransport::with_credentials_and_tls(
                Arc::clone(&cluster),
                Arc::clone(&peer_credentials),
                tls,
            ))
        } else {
            Arc::new(TonicOraclePeerTransport::with_credentials(
                Arc::clone(&cluster),
                Arc::clone(&peer_credentials),
            ))
        };
        let reconciliation_limit_bytes = memory_budget
            .checked_div(4)
            .filter(|limit| *limit > 0)
            .ok_or_else(|| {
                ServerBootError::OraclePeer("Oracle reconciliation budget is zero".to_owned())
            })?;
        let audit = OracleAuditPublisher::new(
            state.postgres.vala().clone(),
            (&config.bifrost.oracle).into(),
        )
        .map_err(|error| {
            ServerBootError::OraclePeer(format!("Oracle audit WAL recovery failed: {error:?}"))
        })?;
        let tail_audit = Arc::new(
            crate::oracle::PostgresTailSecurityAudit::try_new(&state.postgres)
                .await
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
        let tail_authority = Arc::new(
            crate::oracle::ScribeTailAuthority::from_pem(signing_key, tail_audit)
                .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?,
        );
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
        let peer_ticket_minter: Arc<dyn vala_bifrost_redux::oracle::peer::PeerTicketMinter> =
            authority;
        let role = cluster
            .reserve_oracle(advertise_addr, capabilities)
            .await
            .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?;
        let worker = Arc::new(OraclePeerWorker::new(
            node_id,
            role.fencing_token,
            verifier,
            security_audit.clone(),
            reservations,
            SealedFragmentExecutor::with_memory_governor(catalog.file_io(), memory.clone()),
        ));
        let peer = Arc::new(OraclePeerRuntime::new(
            Arc::clone(&worker),
            Arc::clone(&security_audit),
            Arc::clone(&cluster),
        ));
        let local_transport = Arc::new(LocalOraclePeerTransport::new(worker));
        let peer_transports =
            OraclePeerTransportDirectory::new(node_id, local_transport, remote_transport);
        let oracle = match Oracle::new(OracleBuildConfig {
            catalog: Arc::clone(catalog),
            vala: state.postgres.vala().clone(),
            cluster: Arc::clone(&cluster),
            local_role: role.clone(),
            local_slots: slots,
            memory: OracleMemoryResources {
                governor: memory,
                reconciliation_limit_bytes,
            },
            tails: Arc::new(TailTransportDirectory::default()),
            audit: audit.clone(),
            peer_ticket_minter,
            tail_ticket_minter: Some(tail_authority),
            tail_discovery: Some(tail_discovery),
            peer_transports: Some(peer_transports),
            config: OracleConfig {
                planning_permits: config.bifrost.oracle.planning_permits,
                max_workers_per_query: config.bifrost.oracle.max_workers_per_query,
                attempt_memory_bytes: config.bifrost.oracle.max_frame_bytes.min(8 * 1024 * 1024),
                interactive_slots: calibrated
                    .as_ref()
                    .map_or((raw_slots / 2).max(1), |value| value.interactive_slots),
                analytical_slots: calibrated
                    .as_ref()
                    .map_or((raw_slots.saturating_sub(raw_slots / 2)).max(1), |value| {
                        value.analytical_slots
                    }),
                single_tenant_ceiling: calibrated
                    .as_ref()
                    .map_or(raw_slots.max(1), |value| value.single_tenant_ceiling),
                multi_tenant_ceiling: calibrated
                    .as_ref()
                    .map_or((raw_slots / 2).max(1), |value| value.multi_tenant_ceiling),
                queue_capacity: calibrated.as_ref().map_or(
                    u32::try_from(config.bifrost.oracle.admission_waiters).map_err(|_| {
                        ServerBootError::OraclePeer("Oracle queue capacity exceeds u32".to_owned())
                    })?,
                    |value| value.queue_capacity,
                ),
                max_queue_wait: calibrated.as_ref().map_or(
                    std::time::Duration::from_millis(config.bifrost.oracle.max_queue_wait_ms),
                    |value| value.max_queue_wait,
                ),
                interactive_memory_bytes: calibrated
                    .as_ref()
                    .map_or(memory_budget_bytes, |value| value.interactive_memory_bytes),
                analytical_memory_bytes: calibrated
                    .as_ref()
                    .map_or(memory_budget_bytes, |value| value.analytical_memory_bytes),
                spill_bytes: calibrated
                    .as_ref()
                    .map_or(config.bifrost.oracle.spill_limit_bytes, |value| {
                        value.spill_bytes
                    }),
                ..OracleConfig::default()
            },
        }) {
            Ok(oracle) => Arc::new(oracle),
            Err(error) => {
                release_failed_oracle_role(&cluster, &role, "construction").await;
                return Err(ServerBootError::OraclePeer(error.to_string()));
            }
        };
        Ok(BuiltOracleRole {
            state,
            oracle,
            role,
            peer,
            cluster,
            audit,
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
    /// Server state retained until the query runtime is published.
    state: AppState,
    /// Oracle engine awaiting startup reconciliation.
    oracle: Arc<Oracle>,
    /// Reserved cluster role fence.
    role: RegisteredRole,
    /// Peer runtime attached after role activation.
    peer: Arc<OraclePeerRuntime>,
    /// Cluster owner used for activation and snapshot publication.
    cluster: Arc<ClusterRegistry>,
    /// Local audit publisher recovered before role activation.
    audit: Arc<OracleAuditPublisher>,
}

impl BuiltOracleRole {
    /// Starts reconciliation, activates the role, and publishes query access.
    ///
    /// # Errors
    /// Returns [`ServerBootError::OraclePeer`] when startup reconciliation,
    /// activation, snapshot refresh, or readiness publication fails.
    async fn start_reconcile_activate_publish(self) -> Result<AppState, ServerBootError> {
        let Self {
            state,
            oracle,
            role,
            peer,
            cluster,
            audit,
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
        let query_runtime = Arc::new(BifrostQueryRuntime::new(
            oracle, role, peer, cluster, None, audit,
        ));
        Ok(state.with_bifrost_query(query_runtime))
    }
}

/// Attaches one production-shaped local Oracle runtime to test-tier state.
///
/// This helper is compiled only for the `test-support` feature. It reuses the
/// production role registration, peer security, admission, reconciliation,
/// and lifecycle constructor so external language journeys do not build a
/// second Oracle implementation in `wyrd-testing`.
///
/// # Errors
///
/// Returns [`ServerBootError::SigningKey`] if an ephemeral peer key cannot be
/// generated, or the same [`ServerBootError::OraclePeer`] failures as the
/// production Oracle constructor.
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime(state: AppState) -> Result<AppState, ServerBootError> {
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    let signing_key = wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;
    attach_test_oracle_runtime_for_node(state, node_id, signing_key).await
}

/// Attach a production-shaped Oracle role with cluster-controlled identity.
///
/// Restartable cluster journeys call this variant with the same physical node
/// ID and signing authority while constructing a fresh fenced role instance.
/// The production Oracle constructor remains the sole owner of registration,
/// admission, peer security, and readiness behavior.
///
/// # Errors
///
/// Returns the same signing, sentinel, registration, admission, and readiness
/// failures as [`attach_test_oracle_runtime`].
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
) -> Result<AppState, ServerBootError> {
    attach_test_oracle_runtime_for_node_at(
        state,
        node_id,
        signing_key,
        "http://127.0.0.1:0".to_owned(),
    )
    .await
}

/// Attach a test Oracle role advertising its already-reserved private endpoint.
///
/// # Errors
///
/// Returns the same signing, sentinel, registration, admission, and readiness
/// failures as [`attach_test_oracle_runtime_for_node`].
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node_at(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
    advertise_addr: String,
) -> Result<AppState, ServerBootError> {
    let node_id = ClusterNodeId::new(node_id.as_uuid());
    let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
    let mut config = crate::config::WyrdServerConfig::default();
    config.auth.signing_key = Some(signing_key.clone());
    config.bifrost.oracle.audit_wal_root = Some(test_oracle_audit_root(node_id.as_uuid()));
    OracleRoleBuilder {
        state,
        config: &config,
        signing_key: &signing_key,
        cluster,
        node_id,
        advertise_addr: &advertise_addr,
        peer_credentials: None,
    }
    .build()
    .await
}

/// Attach a test Oracle role with a harness-owned refreshing Service credential.
///
/// # Errors
///
/// Returns the same signing, credential, registration, admission, and readiness
/// failures as [`attach_test_oracle_runtime_for_node_at`].
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node_at_with_credentials(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
    advertise_addr: String,
    peer_credentials: Arc<dyn OraclePeerCredentials>,
) -> Result<AppState, ServerBootError> {
    attach_test_oracle_runtime_for_node_at_with_credentials_and_root(
        state,
        node_id,
        signing_key,
        advertise_addr,
        peer_credentials,
        None,
    )
    .await
}

/// Attach a test Oracle role while retaining an explicit harness-owned WAL root.
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node_at_with_credentials_and_root(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
    advertise_addr: String,
    peer_credentials: Arc<dyn OraclePeerCredentials>,
    audit_wal_root: Option<std::path::PathBuf>,
) -> Result<AppState, ServerBootError> {
    let node_id = ClusterNodeId::new(node_id.as_uuid());
    let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
    let mut config = crate::config::WyrdServerConfig::default();
    config.auth.signing_key = Some(signing_key.clone());
    config.bifrost.oracle.audit_wal_root =
        Some(audit_wal_root.unwrap_or_else(|| test_oracle_audit_root(node_id.as_uuid())));
    OracleRoleBuilder {
        state,
        config: &config,
        signing_key: &signing_key,
        cluster,
        node_id,
        advertise_addr: &advertise_addr,
        peer_credentials: Some(peer_credentials),
    }
    .build()
    .await
}

/// Attach a test Oracle role with real server TLS and CA/DNS-authenticated peers.
///
/// # Errors
/// Returns the same boot, credential, membership, and TLS configuration failures
/// as production Oracle attachment.
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node_at_with_credentials_and_tls(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
    advertise_addr: String,
    peer_credentials: Arc<dyn OraclePeerCredentials>,
    ca_path: std::path::PathBuf,
    server_name: String,
) -> Result<AppState, ServerBootError> {
    attach_test_oracle_runtime_for_node_at_with_credentials_and_tls_and_root(
        state,
        node_id,
        signing_key,
        advertise_addr,
        peer_credentials,
        TestOracleTlsAttachment {
            ca_path,
            server_name,
            audit_wal_root: None,
        },
    )
    .await
}

/// TLS and durable-root inputs for a test Oracle attachment.
#[cfg(feature = "test-support")]
pub struct TestOracleTlsAttachment {
    /// Peer CA certificate path.
    pub ca_path: std::path::PathBuf,
    /// Expected peer DNS name.
    pub server_name: String,
    /// Harness-owned audit WAL root.
    pub audit_wal_root: Option<std::path::PathBuf>,
}

/// Attach a TLS test Oracle role with an explicit harness-owned WAL root.
#[cfg(feature = "test-support")]
pub async fn attach_test_oracle_runtime_for_node_at_with_credentials_and_tls_and_root(
    state: AppState,
    node_id: wyrd_spec::vala::api::NodeId,
    signing_key: secrecy::SecretString,
    advertise_addr: String,
    peer_credentials: Arc<dyn OraclePeerCredentials>,
    tls: TestOracleTlsAttachment,
) -> Result<AppState, ServerBootError> {
    let node_id = ClusterNodeId::new(node_id.as_uuid());
    let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
    let mut config = crate::config::WyrdServerConfig::default();
    config.auth.signing_key = Some(signing_key.clone());
    config.bifrost.oracle.audit_wal_root = Some(
        tls.audit_wal_root
            .unwrap_or_else(|| test_oracle_audit_root(node_id.as_uuid())),
    );
    config.bifrost.oracle.peer_ca_certificate_path = Some(tls.ca_path);
    config.bifrost.oracle.peer_server_name = Some(tls.server_name);
    OracleRoleBuilder {
        state,
        config: &config,
        signing_key: &signing_key,
        cluster,
        node_id,
        advertise_addr: &advertise_addr,
        peer_credentials: Some(peer_credentials),
    }
    .build()
    .await
}

#[cfg(feature = "test-support")]
/// Returns a node-stable temporary WAL root so restart tests replay backlog.
fn test_oracle_audit_root(node_id: uuid::Uuid) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("wyrd-oracle-audit-test-{node_id}"))
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
        space,
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
    Ok(Some(async move { forge.run(shutdown).await }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::catalog::default_table_source::provider_as_source;
    use datafusion::datasource::empty::EmptyTable;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use futures_util::StreamExt;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::forge::{Forge, ForgeBuildConfig};
    use vala_bifrost_redux::maintenance::{StagingFileCommitted, staging_file_channel};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::oracle::dispatcher::DispatchError;
    use vala_bifrost_redux::oracle::{AuthorizedQueryContext, QueryOptions};
    use vala_bifrost_redux::scribe::memory::{BifrostDataFusionMemoryPool, BifrostMemoryGovernor};
    use vala_sql::OperatorPool;
    use wyrd_runtime::permission::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalKind, RoleRef};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::CardRefScope;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuthMethod, ClusterCapabilities, QueryStreamFrame, QueryTerminalErrorCode,
        QueryTerminalOutcome, VisibilityMode,
    };
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    /// A post-Scribe activation failure rolls back the exact local role fence
    /// before returning the production boot error.
    #[cfg(feature = "test-support")]
    #[tokio::test(flavor = "current_thread")]
    async fn production_boot_failure_after_scribe_activation_rolls_back_roles() {
        let mut config = crate::config::WyrdServerConfig::default();
        config.role = crate::config::ForgeProcessRole::Server;
        config.bifrost.oracle.allow_unapproved_profile = true;
        let telemetry = Arc::new(wyrd_telemetry::init_test_only_no_global(
            wyrd_telemetry::TelemetryConfig::default(),
        ));
        let result = build_state(
            &config,
            telemetry,
            StateOverrides {
                fail_after_scribe_activation: true,
                ..StateOverrides::default()
            },
        )
        .await;
        let error = match result {
            Ok(_) => panic!("injected post-Scribe failure must abort boot"),
            Err(error) => error,
        };
        assert!(
            matches!(error, ServerBootError::Scribe(ref message) if message.contains("after Scribe activation")),
            "unexpected injected failure: {error:?}"
        );

        let node_id = ClusterNodeId::new(uuid::Uuid::now_v7());
        let cluster =
            ClusterRegistry::new(crate::test_support::test_vala_postgres().await, node_id);
        cluster
            .refresh_snapshot()
            .await
            .expect("refresh cluster after rollback");
        assert!(
            cluster.snapshot().live_scribes().is_empty(),
            "rollback must remove the activated Scribe membership and fence"
        );
        assert!(
            cluster.snapshot().live_oracles().is_empty(),
            "failure before Oracle composition must leave no Oracle membership or fence"
        );
    }

    /// Test-only peer credential that returns a production-signed access token.
    struct TestOraclePeerCredentials {
        /// Access token minted by the same issuing key installed in test auth.
        token: SecretString,
    }

    impl std::fmt::Debug for TestOraclePeerCredentials {
        /// Redacts the bearer token from test diagnostics.
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("TestOraclePeerCredentials")
                .finish_non_exhaustive()
        }
    }

    #[async_trait]
    impl OraclePeerCredentials for TestOraclePeerCredentials {
        /// Returns the pre-minted token for each local peer exchange.
        ///
        /// # Errors
        ///
        /// This fixture never refreshes or rejects its pre-minted token, so it
        /// always returns `Ok` unless the credential contract changes.
        async fn bearer(&self, _force_refresh: bool) -> Result<String, DispatchError> {
            Ok(self.token.expose_secret().to_owned())
        }
    }

    /// Compose one real Forge from the retained test fixture resources.
    pub(crate) async fn composed_test_state() -> (
        AppState,
        vala_bifrost_redux::maintenance::StagingFilePublisher,
    ) {
        let state = make_test_state().await;
        let storage = crate::test_support::test_storage().await;
        let redux = crate::test_support::test_redux_catalog().await;
        let vala = crate::test_support::test_vala_postgres().await;
        let operator_pool: OperatorPool = crate::test_support::test_operator_pool().await;
        let (publisher, inbox) = staging_file_channel(16).expect("hint channel");
        let memory = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("memory governor");
        let query_memory = Arc::new(BifrostDataFusionMemoryPool::new(memory.clone()));
        let config = ForgeConfig {
            max_hints_per_wake: 16,
            ..ForgeConfig::default()
        };
        let spill = Box::leak(Box::new(tempdir().expect("spill directory")));
        let rewrite_runtime =
            ForgeRewriteRuntime::new(query_memory.clone(), spill.path(), config.spill_limit_bytes)
                .expect("rewrite runtime");
        let staging = Arc::new(storage.operator().clone());
        let object_store: Arc<dyn ForgeObjectStore> =
            Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging)));
        let forge = Arc::new(
            Forge::new(ForgeBuildConfig {
                vala,
                operator_pool,
                catalog: redux.iceberg_catalog(),
                staging,
                object_store,
                rewrite_runtime,
                hints: inbox,
                config,
                maintenance_interval: Duration::from_millis(10),
                clock: ForgeClock::system(),
                completion_observer: None,
                scheduler_trigger: None,
                telemetry: Arc::new(ForgeTelemetry::new()),
            })
            .expect("Forge"),
        );
        (
            state
                .with_bifrost_redux(redux)
                .with_bifrost_memory_pool(memory, query_memory)
                .with_forge(forge),
            publisher,
        )
    }

    /// Production-shaped boot retains one Forge allocation for state and supervision.
    #[tokio::test]
    async fn forge_is_composed_once_and_supervised_directly() {
        let (state, _publisher) = composed_test_state().await;
        let retained = state.forge_handle().expect("retained Forge").clone();
        let retained_pool = state
            .bifrost_query_memory
            .as_ref()
            .expect("retained query pool")
            .clone();
        let shutdown = CancellationToken::new();
        let supervised = spawn_maintenance_scheduler(&state, shutdown.clone())
            .expect("Forge supervisor result")
            .expect("Forge supervisor future");
        assert!(Arc::ptr_eq(&retained, state.forge_handle().expect("Forge")));
        assert!(Arc::ptr_eq(
            &retained_pool,
            state.bifrost_query_memory.as_ref().expect("query pool")
        ));
        let task = tokio::spawn(supervised);
        tokio::task::yield_now().await;
        shutdown.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .expect("supervisor bound")
                .expect("supervisor join")
                .is_ok()
        );
    }

    /// Production Forge cancellation remains bounded at idle and active edges.
    #[tokio::test]
    async fn forge_shutdown_is_bounded_at_idle_and_active_boundaries() {
        let (state, publisher) = composed_test_state().await;
        let idle_shutdown = CancellationToken::new();
        idle_shutdown.cancel();
        let idle = spawn_maintenance_scheduler(&state, idle_shutdown)
            .expect("idle Forge result")
            .expect("idle Forge supervisor");
        assert!(
            tokio::time::timeout(Duration::from_secs(2), idle)
                .await
                .expect("idle shutdown bound")
                .is_ok()
        );

        let active_shutdown = CancellationToken::new();
        let active = spawn_maintenance_scheduler(&state, active_shutdown.clone())
            .expect("active Forge result")
            .expect("active Forge supervisor");
        let task = tokio::spawn(active);
        let tenant = crate::test_support::test_tenant().await;
        let binding = TenantTableBinding::resolve((
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "lifecycle_probe"),
        ))
        .expect("binding");
        let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 27).expect("day");
        assert_eq!(
            publisher.try_publish(StagingFileCommitted::new(binding, day)),
            vala_bifrost_redux::maintenance::StagingPublishOutcome::Published
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while publisher.capacity_for_test() < 16 {
            assert!(tokio::time::Instant::now() < deadline, "hint not consumed");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Let the supervised task enter its active tick before cancellation;
        // this exercises the same lifecycle edge as a rewrite holding spill.
        tokio::task::yield_now().await;
        active_shutdown.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .expect("active shutdown bound")
                .expect("supervisor join")
                .is_ok()
        );
    }

    async fn make_test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), Some(admin_pool));
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        AppState::new(postgres, storage, crate::test_support::test_catalog().await)
    }

    /// Installs auth and provisions a real service-role token for Oracle boot.
    ///
    /// The credential bypasses only network transport; service-account lookup,
    /// role resolution, token signing, and access-token verification stay on
    /// the production paths exercised by [`OracleRoleBuilder`].
    ///
    /// # Panics
    ///
    /// Panics when the shared fixture cannot seed the peer role, auth handles,
    /// or production-shaped service token.
    pub(crate) async fn with_test_oracle_peer_credentials(
        state: AppState,
        config: &crate::config::WyrdServerConfig,
        signing_key: &SecretString,
    ) -> (AppState, Arc<dyn OraclePeerCredentials>) {
        let state = install_auth(state, config, signing_key, None)
            .await
            .expect("test auth installs");
        let tenant = DataTenantId::SYSTEM_OWNER;
        let service_id = uuid::Uuid::now_v7();
        let service_name = format!("oracle-peer-{}", service_id.simple());
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new(service_name.clone()).expect("service card name"),
            version: VersionBlock::parse("1.0.0").expect("service card version"),
            space: SpaceName::new("system").expect("service card space"),
            uid: Some(CardUid::new(service_id.to_string()).expect("service card uid")),
        };
        let role_name = "bifrost_oracle_peer";
        let permissions = serde_json::to_value([Permission::bifrost_oracle_peer_invoke()])
            .expect("peer permissions serialize");
        let mut conn = state
            .postgres
            .tenant_conn(tenant)
            .await
            .expect("system tenant connection");
        crate::auth::seed::seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        wyrd_sql::queries::auth::insert_role(
            &mut conn,
            uuid::Uuid::now_v7(),
            role_name,
            &permissions,
            false,
        )
        .await
        .expect("peer role inserts");
        wyrd_sql::queries::auth::insert_service_account(
            &mut conn,
            service_id,
            "service",
            &card_ref,
            &service_name,
            None,
            crate::boot::bootstrap::SYSTEM_OPERATOR_ID,
        )
        .await
        .expect("peer service account inserts");
        let role = wyrd_sql::queries::auth::role_by_name(&mut conn, role_name)
            .await
            .expect("peer role lookup")
            .expect("peer role exists");
        wyrd_sql::queries::auth::grant_role_to_service_account(&mut conn, service_id, role.id)
            .await
            .expect("peer role grants");
        conn.commit().await.expect("peer role transaction commits");

        let token = state
            .auth
            .issuing_key
            .as_ref()
            .expect("test auth issuing key")
            .issue_service_access_token(
                PrincipalId::new(service_id),
                tenant,
                card_ref,
                CardRefScope::default(),
                vec![RoleRef::new(role_name).expect("peer role ref")],
                chrono::Duration::minutes(5),
            )
            .expect("peer access token issues");
        (
            state,
            Arc::new(TestOraclePeerCredentials {
                token: SecretString::new(token.into()),
            }) as Arc<dyn OraclePeerCredentials>,
        )
    }

    /// Verified boot dependencies register and retain one ready local Oracle peer.
    #[test]
    fn oracle_peer_boot_registers_role_and_runtime_after_sentinel_verification() {
        wyrd_runtime::runtime().block_on(async {
            let postgres = crate::test_support::test_server_postgres().await;
            let storage = crate::test_support::test_storage().await;
            let catalog = crate::test_support::test_catalog().await;
            let redux = crate::test_support::test_redux_catalog().await;
            let memory = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("memory governor");
            let query_memory = Arc::new(BifrostDataFusionMemoryPool::new(memory.clone()));
            let state = AppState::new(postgres, storage, catalog)
                .with_bifrost_redux(redux)
                .with_bifrost_memory_pool(memory, query_memory);
            let node_id = ClusterNodeId::new(uuid::Uuid::now_v7());
            let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
            let mut config = crate::config::WyrdServerConfig::default();
            config.auth.signing_key = Some(
                wyrd_auth_issue::IssuingKey::generate_ephemeral_pem().expect("ephemeral test key"),
            );

            let signing_key = config
                .auth
                .signing_key
                .as_ref()
                .expect("test config retains signing key");
            let (state, peer_credentials) =
                with_test_oracle_peer_credentials(state, &config, signing_key).await;
            let state = OracleRoleBuilder {
                state,
                config: &config,
                signing_key,
                cluster: Arc::clone(&cluster),
                node_id,
                advertise_addr: "127.0.0.1:9443",
                peer_credentials: Some(peer_credentials),
            }
            .build()
            .await
            .expect("verified Oracle peer boot");

            assert!(state.oracle_peer.is_some());
            let snapshot = cluster.snapshot();
            let role = snapshot
                .live_oracles()
                .into_iter()
                .find(|role| role.key.node_id == node_id)
                .expect("local Oracle role is ready");
            assert!(role.ready);
            assert_eq!(role.address, "127.0.0.1:9443");
        });
    }

    /// Proves colocated role shutdown is durably ordered and fence-independent.
    ///
    /// The injected readiness heartbeats replace wall-clock sleeps: each
    /// lifecycle phase is persisted and inspected before the next phase runs.
    /// Oracle retains an admitted frame stream through endpoint drain, then
    /// cancels it before exact-fence unregister. Scribe likewise remains
    /// open during drain, closes new Gate work during teardown, and removes only
    /// its own fence.
    #[test]
    fn colocated_oracle_and_scribe_shutdown_preserves_exact_role_fences() {
        wyrd_runtime::runtime().block_on(async {
            let postgres = crate::test_support::test_server_postgres().await;
            let storage = crate::test_support::test_storage().await;
            let catalog = crate::test_support::test_catalog().await;
            let redux = crate::test_support::test_redux_catalog().await;
            let memory = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("memory governor");
            let query_memory = Arc::new(BifrostDataFusionMemoryPool::new(memory.clone()));
            let state = AppState::new(postgres, Arc::clone(&storage), catalog)
                .with_bifrost_redux(Arc::clone(&redux))
                .with_bifrost_memory_pool(memory, query_memory);
            let node_id = ClusterNodeId::new(uuid::Uuid::now_v7());
            let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
            let mut config = crate::config::WyrdServerConfig::default();
            config.auth.signing_key = Some(
                wyrd_auth_issue::IssuingKey::generate_ephemeral_pem().expect("ephemeral test key"),
            );
            let signing_key = config
                .auth
                .signing_key
                .as_ref()
                .expect("test config retains signing key");
            let (state, peer_credentials) =
                with_test_oracle_peer_credentials(state, &config, signing_key).await;
            let state = OracleRoleBuilder {
                state,
                config: &config,
                signing_key,
                cluster: Arc::clone(&cluster),
                node_id,
                advertise_addr: "127.0.0.1:9443",
                peer_credentials: Some(peer_credentials),
            }
            .build()
            .await
            .expect("Oracle role");
            let query = Arc::clone(state.bifrost_query().expect("query runtime"));
            let oracle_lease = cluster
                .snapshot()
                .live_oracles()
                .into_iter()
                .find(|lease| lease.key.node_id == node_id)
                .expect("ready Oracle")
                .clone();
            let oracle_role = RegisteredRole {
                key: oracle_lease.key,
                fencing_token: oracle_lease.fencing_token,
                capabilities: oracle_lease.capabilities,
            };

            let scribe_role = cluster
                .register_scribe(
                    "127.0.0.1:9444",
                    ScribeCapabilitiesV1 {
                        tail_protocol_version: 1,
                    },
                )
                .await
                .expect("Scribe role");
            cluster.refresh_snapshot().await.expect("mixed snapshot");
            assert_eq!(cluster.snapshot().live_oracles().len(), 1);
            assert_eq!(cluster.snapshot().live_scribes().len(), 1);
            assert_ne!(
                oracle_role.fencing_token, 0,
                "Oracle retains a concrete fence"
            );
            assert_ne!(
                scribe_role.fencing_token, 0,
                "Scribe retains a concrete fence"
            );

            let wal_root = tempdir().expect("WAL root");
            let writer_epoch =
                i64::try_from(scribe_role.fencing_token).expect("Scribe fence fits writer epoch");
            let wal = Arc::new(
                WalWriter::new(
                    wal_root.path(),
                    *node_id.as_uuid().as_bytes(),
                    writer_epoch,
                    WalConfig::default(),
                )
                .expect("WAL"),
            );
            let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps(
                Arc::new(storage.operator().clone()),
                wal,
                &node_id.as_uuid().to_string(),
                writer_epoch,
            ));
            let verifier = state.auth.token_verifier.clone().expect("test verifier");
            let ingest = Arc::new(
                BifrostIngestRuntime::new(
                    scribe,
                    Arc::clone(&redux),
                    verifier,
                    vala_bifrost_redux::gate::limits::IngestLimits::default(),
                    None,
                )
                .with_scribe_role(Arc::clone(&cluster), scribe_role.clone()),
            );

            let tenant = crate::test_support::test_tenant().await;
            let principal = Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                Vec::new(),
                PermissionSet::from_iter([Permission::bifrost_query_read()]),
            );
            let context = AuthorizedQueryContext::try_new(
                principal,
                tenant,
                RequestId::now_v7(),
                None,
                AuthMethod::Internal,
                Permission::bifrost_query_read().to_string(),
            )
            .expect("authorized query context");
            let table_ref = TableRef::new(
                BifrostNamespace::Datasets,
                format!("shutdown_probe_{}", uuid::Uuid::now_v7().simple()),
            );
            redux
                .register_dataset(
                    tenant,
                    table_ref.clone(),
                    vec![Field::new("value", DataType::Int64, false)],
                    None,
                )
                .await
                .expect("register typed query fixture");
            let source =
                provider_as_source(Arc::new(EmptyTable::new(Arc::new(Schema::new(vec![
                    Field::new("value", DataType::Int64, false),
                ])))));
            let plan = LogicalPlanBuilder::scan(table_ref.fqn(), source, None)
                .expect("fixture scan")
                .build()
                .expect("read-only empty scan");
            let mut active_query = query
                .oracle()
                .query_plan(
                    context,
                    plan,
                    QueryOptions {
                        visibility: VisibilityMode::PublishedOnly,
                        deadline: std::time::Instant::now() + Duration::from_secs(10),
                    },
                )
                .await
                .expect("admitted Oracle query stream");
            query
                .begin_shutdown()
                .await
                .expect("Oracle durable deactivation");
            cluster
                .heartbeat_readiness_for_test(&oracle_role, false)
                .await
                .expect("injected Oracle drain heartbeat");
            cluster.refresh_snapshot().await.expect("draining snapshot");
            assert!(cluster.snapshot().live_oracles().is_empty());
            assert_eq!(
                cluster.snapshot().live_scribes().len(),
                1,
                "Oracle deactivation cannot mutate colocated Scribe capacity"
            );

            query
                .shutdown(std::time::Instant::now() + Duration::from_secs(2))
                .await;
            assert!(matches!(
                active_query.frames.next().await,
                Some(Ok(QueryStreamFrame::Schema(_)))
            ));
            assert!(matches!(
                active_query.frames.next().await,
                Some(Ok(QueryStreamFrame::Terminal(terminal)))
                    if terminal.outcome == QueryTerminalOutcome::Failed
                        && terminal.error.as_ref().is_some_and(|error| {
                            error.code == QueryTerminalErrorCode::QueryExecutionFailed
                        })
            ));
            let replacement_capabilities = match oracle_role.capabilities.clone() {
                ClusterCapabilities::OracleV1(capabilities) => capabilities,
                ClusterCapabilities::ScribeV1(_) => {
                    panic!("Oracle role carries Oracle capabilities")
                }
            };
            let replacement_role = cluster
                .register_oracle("127.0.0.1:9445", replacement_capabilities)
                .await
                .expect("replacement Oracle fence");
            assert!(matches!(
                cluster
                    .heartbeat_readiness_for_test(&oracle_role, true)
                    .await,
                Err(vala_bifrost_redux::cluster::ClusterError::StaleFence)
            ));
            cluster
                .shutdown_role(replacement_role)
                .await
                .expect("replacement Oracle fence shutdown");
            cluster
                .heartbeat_readiness_for_test(&scribe_role, true)
                .await
                .expect("Scribe heartbeat survives Oracle unregister");

            ingest
                .begin_shutdown()
                .await
                .expect("Scribe durable deactivation");
            cluster
                .heartbeat_readiness_for_test(&scribe_role, false)
                .await
                .expect("injected Scribe drain heartbeat");
            assert!(
                !ingest.gate().is_closed_for_test(),
                "Gate remains available to accepted transports during drain"
            );
            ingest
                .shutdown(std::time::Instant::now() + Duration::from_secs(2))
                .await;
            assert!(ingest.gate().is_closed_for_test());
            assert!(!ingest.is_ready());
            let replacement_scribe = cluster
                .register_scribe(
                    "127.0.0.1:9445",
                    ScribeCapabilitiesV1 {
                        tail_protocol_version: 1,
                    },
                )
                .await
                .expect("replacement Scribe fence");
            assert!(matches!(
                cluster
                    .heartbeat_readiness_for_test(&scribe_role, true)
                    .await,
                Err(vala_bifrost_redux::cluster::ClusterError::StaleFence)
            ));
            cluster
                .shutdown_role(replacement_scribe)
                .await
                .expect("replacement Scribe fence shutdown");
            cluster.refresh_snapshot().await.expect("stopped snapshot");
            assert!(cluster.snapshot().live_oracles().is_empty());
            assert!(cluster.snapshot().live_scribes().is_empty());
        });
    }

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
        let state = AppState::new(postgres, storage, crate::test_support::test_catalog().await);

        assert!(state.postgres.operator_pool().is_some());
        assert_eq!(
            state.storage.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
    }
}
