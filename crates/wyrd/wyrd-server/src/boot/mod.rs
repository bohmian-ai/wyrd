//! Server boot sequence for SQL-backed Wyrd runtime state.

pub mod auth;
pub mod bootstrap;
pub mod issuer;

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use secrecy::ExposeSecret;
use tokio_util::sync::CancellationToken;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeClock, ForgeConfig, ForgeObjectStore, ForgeRewriteRuntime,
    ForgeTelemetry, ForgeWorker, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::memory::{BifrostDataFusionMemoryPool, BifrostMemoryGovernor};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, acquire_on_boot};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribeIngressCpuPool,
    ScribePersistenceConfig, ScribePersistenceCpuPool, ScribeWalIoPool,
};
use wyrd_auth_oidc::WorkloadBinding;
use wyrd_crypt::SecretKey;
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_sql::postgres_boot::{BootError, PostgresBoot};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::components::auth::audit_writer::RealAuthzAuditWriter;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::EvalAuditWriter;
use crate::config::WorkloadBindingEntry;
use crate::postgres::ServerPostgres;
use crate::state::{AppState, BifrostIngestRuntime, ProductionValidationError};

const DEFAULT_MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const DEFAULT_HINT_CAPACITY: usize = 1_024;

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
}

/// Holds unmounted Bifrost dependencies while boot creates authentication.
///
/// This private boot value never reaches request handling. Once authentication
/// exists, [`build_state`] consumes it to create one complete
/// [`BifrostIngestRuntime`].
struct BifrostIngestParts {
    /// State without an ingest subsystem.
    state: AppState,
    /// Recovered Scribe allocation that Gate must wrap.
    scribe: Option<Arc<ScribeImpl>>,
    /// Dedicated runtime that owns Scribe coordination tasks.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
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
}

/// Resolve database configuration, run migrations, and assemble runtime state.
///
/// The boot-only migrator pool is closed before runtime pools are constructed
/// so a BYPASSRLS connection cannot survive into request handling.
///
/// # Errors
/// Returns [`ServerBootError`] when database boot, migration, or runtime pool
/// construction fails.
pub async fn build_app_state() -> Result<AppState, ServerBootError> {
    let boot = PostgresBoot::from_env().await?;
    build_app_state_from_boot_with_config(&boot, crate::config::ScribeRuntimeConfig::default())
        .await
}

/// Assemble runtime state from a resolved Postgres boot mode.
///
/// # Errors
/// Returns [`ServerBootError`] when DSN resolution, migrations, or runtime
/// pool construction fails.
pub async fn build_app_state_from_boot(boot: &PostgresBoot) -> Result<AppState, ServerBootError> {
    build_app_state_from_boot_with_config(boot, crate::config::ScribeRuntimeConfig::default()).await
}

/// Assemble runtime state from a resolved Postgres boot mode and Scribe config.
pub async fn build_app_state_from_boot_with_config(
    boot: &PostgresBoot,
    scribe_config: crate::config::ScribeRuntimeConfig,
) -> Result<AppState, ServerBootError> {
    Ok(build_bifrost_parts_from_boot(boot, scribe_config, true)
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
    scribe_config: crate::config::ScribeRuntimeConfig,
    include_scribe: bool,
) -> Result<BifrostIngestParts, ServerBootError> {
    scribe_config.validate().map_err(ServerBootError::Scribe)?;
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
    if include_scribe {
        vala_bifrost::tables::register_all(&bifrost).await?;
    }
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
    let wal_dir = std::env::var_os("WYRD_SCRIBE_WAL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(".wyrd/scribe-wal"));
    std::fs::create_dir_all(&wal_dir).map_err(|error| {
        ServerBootError::Scribe(format!("WAL directory creation failed: {error}"))
    })?;
    let pod_memory_limit = BifrostMemoryGovernor::detect(1024 * 1024 * 1024)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?
        .pod_limit_bytes();
    let bifrost_memory = BifrostMemoryGovernor::new_with_scribe_limit(
        pod_memory_limit,
        scribe_config.memory_limit_bytes,
    )
    .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    let bifrost_datafusion_memory_pool =
        Arc::new(BifrostDataFusionMemoryPool::new(bifrost_memory.clone()));
    let (staging_file_publisher, staging_file_inbox) = staging_file_channel(DEFAULT_HINT_CAPACITY)
        .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
    let (scribe, coordination_runtime) = if include_scribe {
        let node_id = NodeId::generate();
        let advertise_addr = std::env::var("WYRD_SERVER_ADVERTISE_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:0".to_owned());
        let stream = acquire_on_boot(&operator_pool, node_id, "scribe", &advertise_addr)
            .await
            .map_err(|error| ServerBootError::Scribe(error.to_string()))?;
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
        tracing::info!(
            coordination_threads = scribe_config.coordination_threads,
            ingress_cpu_threads = scribe_config.ingress_cpu_threads,
            persistence_cpu_threads = scribe_config.persistence_cpu_threads,
            wal_io_threads = scribe_config.wal_io_threads,
            memory_limit_bytes = scribe_config.memory_limit_bytes,
            wal_disk_limit_bytes = scribe_config.wal_disk_limit_bytes,
            "resolved Scribe runtime configuration"
        );
        let runtime = Arc::new(
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
        let admission = AdmissionConfig {
            memory_limit_bytes: pod_memory_limit,
            scribe_memory_limit_bytes: scribe_config.memory_limit_bytes,
        };
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
            admission,
            coordination_runtime: runtime.handle().clone(),
            execution_pools,
            persistence: Some(ScribePersistenceConfig::new(
                Arc::new(postgres.vala().clone()),
                64,
                scribe_config.wal_io_threads,
            )),
            memory_budget: Some(bifrost_memory.scribe_budget()),
            staging_file_publisher: Some(staging_file_publisher),
        }));
        match scribe.replay_wal_async().await {
            Ok(replayed_generations) => {
                tracing::info!(replayed_generations, "Scribe WAL recovery complete");
            }
            Err(error) => {
                tracing::error!(error = %error, "Scribe WAL recovery failed; keeping the server alive but not ready");
            }
        }
        (Some(scribe), Some(runtime))
    } else {
        drop(staging_file_publisher);
        (None, None)
    };

    let forge_config = ForgeConfig::default();
    let rewrite_runtime = ForgeRewriteRuntime::new(
        bifrost_datafusion_memory_pool.clone(),
        &wal_dir.join("forge-spill"),
        forge_config.spill_limit_bytes,
    )?;
    let staging = Arc::new(storage.operator().clone());
    let object_store: Arc<dyn ForgeObjectStore> =
        Arc::new(OpenDalForgeObjectStore::new(Arc::clone(&staging)));
    let forge = Arc::new(Forge::new(ForgeBuildConfig {
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
    })?);

    let state = AppState::new(postgres, storage, bifrost)
        .with_bifrost_redux(bifrost_redux)
        .with_bifrost_memory_pool(bifrost_memory, bifrost_datafusion_memory_pool)
        .with_forge(forge);
    Ok(BifrostIngestParts {
        state,
        scribe,
        coordination_runtime,
    })
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
    let worker_only = config.role == crate::config::ForgeProcessRole::ForgeWorker;
    let bifrost_parts = build_bifrost_parts_from_boot(&boot, config.scribe, !worker_only).await?;
    let state = bifrost_parts.state;
    let state = attach_config_fields(state, config, shutdown, telemetry)?;
    if worker_only {
        return Ok(state);
    }

    let sealing_key = build_sealing_key(config)?;
    let state = install_auth(state, config, sealing_key.clone()).await?;
    let verifier = state
        .auth
        .token_verifier
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires a token verifier".to_owned()))?;
    let bifrost_redux = state
        .bifrost_redux
        .clone()
        .ok_or_else(|| ServerBootError::Scribe("Gate requires the Redux catalog".to_owned()))?;
    let ingest = Arc::new(BifrostIngestRuntime::new(
        bifrost_parts
            .scribe
            .ok_or_else(|| ServerBootError::Scribe("server role requires Scribe".to_owned()))?,
        bifrost_redux,
        verifier,
        vala_bifrost_redux::gate::limits::IngestLimits::default(),
        bifrost_parts.coordination_runtime,
    ));
    let state = state.with_bifrost_ingest(ingest);
    seed_federation(&state, config, sealing_key.as_deref()).await?;

    // Install the real authz audit writer as the OSS default. Callers can
    // still replace the entire ServerAuthz (policy hook + writer) via overrides.
    let state = state.with_authz(ServerAuthz {
        audit_writer: Arc::new(RealAuthzAuditWriter),
        ..ServerAuthz::default()
    });
    let state = apply_overrides(state, overrides);

    state
        .production_validate()
        .map_err(ServerBootError::ProductionValidation)?;
    Ok(state)
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

    // Install Wyrd's own auth handles (issuing key + token verifier). A server
    // without a signing key cannot mint or verify Wyrd JWTs, so auth is the same
    // working experience across environments: a production profile (staging and
    // production) fails boot closed when no key is provisioned; development mints
    // an ephemeral key so auth works on a fresh local run. The verifier's
    // external path is wired to the Postgres issuer resolver built above.
    let (issuing_key, verifier) = match config.auth.signing_key.as_ref() {
        Some(signing_key) => crate::boot::auth::build_auth_handles(
            signing_key,
            state.postgres.app_pool(),
            Arc::clone(&issuer_resolver),
        )?,
        None if config.deployment_profile.is_production() => {
            return Err(ServerBootError::SigningKey(
                "no signing key configured (set WYRD_SIGNING_KEY_FILE or WYRD_SIGNING_KEY_PEM)"
                    .to_owned(),
            ));
        }
        None => {
            let ephemeral = wyrd_auth_issue::IssuingKey::generate_ephemeral_pem()
                .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;
            tracing::warn!(
                "APP_ENV=development and no signing key configured; generated an EPHEMERAL \
                 signing key so auth works locally. Tokens will not survive a restart and this \
                 key must never be used in staging or production. Set WYRD_SIGNING_KEY_FILE to \
                 provision a stable key."
            );
            crate::boot::auth::build_auth_handles(
                &ephemeral,
                state.postgres.app_pool(),
                Arc::clone(&issuer_resolver),
            )?
        }
    };

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
                detail: "AppState has no shared Forge".to_owned(),
            })?;
    Ok(async move { forge.run(shutdown).await })
}

/// Build the single bounded claim-driven Forge worker-pool future.
///
/// Embedded and dedicated roles call this same constructor, which keeps task
/// decoding, execution, reporting, and recovery on one implementation path.
///
/// # Errors
///
/// Returns [`ServerBootError::ForgeSchedulerRequired`] when Forge dependencies
/// are absent, or [`ServerBootError::Forge`] when worker bounds are invalid.
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
    let worker = ForgeWorker::new(forge, ForgeWorkerConfig { worker_concurrency })?;
    Ok(async move { worker.run(shutdown).await })
}

#[cfg(test)]
mod tests {
    use super::*;

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
/// PostgreSQL-backed boot composition regressions and shared Forge fixtures.
pub(crate) mod pg_tests {
    use super::*;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::forge::{Forge, ForgeBuildConfig};
    use vala_bifrost_redux::maintenance::{StagingFileCommitted, staging_file_channel};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::scribe::memory::{BifrostDataFusionMemoryPool, BifrostMemoryGovernor};
    use vala_sql::OperatorPool;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

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
        let supervised =
            spawn_maintenance_scheduler(&state, shutdown.clone()).expect("Forge supervisor future");
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
        let idle =
            spawn_maintenance_scheduler(&state, idle_shutdown).expect("idle Forge supervisor");
        assert!(
            tokio::time::timeout(Duration::from_secs(2), idle)
                .await
                .expect("idle shutdown bound")
                .is_ok()
        );

        let active_shutdown = CancellationToken::new();
        let active = spawn_maintenance_scheduler(&state, active_shutdown.clone())
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
