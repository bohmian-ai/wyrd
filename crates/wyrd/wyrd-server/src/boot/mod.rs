//! Server boot sequence for SQL-backed Wyrd runtime state.

pub mod auth;
pub mod bootstrap;
pub mod issuer;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use secrecy::ExposeSecret;
use tokio_util::sync::CancellationToken;
use vala_bifrost::catalog::WyrdCatalog;
use wyrd_auth_oidc::WorkloadBinding;
use wyrd_crypt::SecretKey;
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_sql::pool::{PoolConfig, build_pool};
use wyrd_sql::postgres_boot::{BootError, PostgresBoot};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

use metrics::{counter, gauge};
use vala_bifrost::reconcile::reconcile_audit;
use vala_bifrost::relay::AuditRelay;

use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::components::auth::audit_writer::RealAuthzAuditWriter;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::EvalAuditWriter;
use crate::config::{DeploymentProfile, WorkloadBindingEntry};
use crate::postgres::ServerPostgres;
use crate::state::{AppState, ProductionValidationError};

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
    /// `WYRD_VALA_500_AUDIT_RELAY_REQUIRED`: the audit relay is disabled or its
    /// dependencies are unavailable in a production deployment. A server that
    /// accepts auth/mutating writes while the audit outbox cannot drain to
    /// `audit_log` must not boot (M-12 fail-closed).
    #[error(
        "WYRD_VALA_500_AUDIT_RELAY_REQUIRED: audit relay is required in production \
         but is disabled or unavailable: {detail}"
    )]
    AuditRelayRequired {
        /// Detail about why the relay could not be started.
        detail: String,
    },
    /// `WYRD_VALA_500_AUDIT_RECONCILER_REQUIRED`: the audit reconciler is disabled or
    /// its dependencies are unavailable in a production deployment. A production server
    /// cannot accept mutating writes without the durability proof running.
    #[error(
        "WYRD_VALA_500_AUDIT_RECONCILER_REQUIRED: audit reconciler is required in \
         production but is disabled or unavailable: {detail}"
    )]
    AuditReconcilerRequired {
        /// Detail about why the reconciler could not be started.
        detail: String,
    },
    /// `WYRD_VALA_500_RECOVERY_POOL_REQUIRED`: production boot requires a
    /// `vala_recovery` pool for the commit-recovery sweep. A server that cannot
    /// recover stale precommits risks permanent data loss and must not boot.
    ///
    /// Set `WYRD_RECOVERY_DSN` (or configure the `recovery` DSN slot) to
    /// provision the `vala_recovery` SECURITY DEFINER pool before starting in
    /// production.
    #[error(
        "WYRD_VALA_500_RECOVERY_POOL_REQUIRED: production deployment requires a \
         recovery pool (vala_recovery DSN) for the commit-recovery sweep, \
         but none is configured"
    )]
    RecoveryPoolRequired,
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
    build_app_state_from_boot(&boot).await
}

/// Assemble runtime state from a resolved Postgres boot mode.
///
/// # Errors
/// Returns [`ServerBootError`] when DSN resolution, migrations, or runtime
/// pool construction fails.
pub async fn build_app_state_from_boot(boot: &PostgresBoot) -> Result<AppState, ServerBootError> {
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
    let recovery_pool = build_pool(dsns.recovery.expose_secret(), PoolConfig::default())
        .await
        .map_err(ServerBootError::PoolConnect)?;
    let bifrost = WyrdCatalog::new(
        dsns.catalog_app.expose_secret(),
        storage.backend_config(),
        Arc::new(postgres.app_pool().clone()),
        Some(Arc::new(recovery_pool)),
    )
    .await?;

    Ok(AppState::new(postgres, storage, Arc::new(bifrost)))
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
    let state = build_app_state_from_boot(&boot).await?;
    let sealing_key = build_sealing_key(config)?;

    let state = attach_config_fields(state, config, shutdown, telemetry)?;
    let state = install_auth(state, config, sealing_key.clone()).await?;
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

    let audit_seal_key = build_audit_seal_key(config)?;

    Ok(state.with_auth(ServerAuth {
        allow_preview: config.auth.allow_preview,
        issuing_key: Some(issuing_key),
        token_verifier: Some(verifier),
        trusted_issuer_resolver: Some(Arc::clone(&issuer_resolver)),
        workload_binding_resolver: Some(binding_resolver),
        sealing_key: sealing_key.clone(),
        audit_seal_key,
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

/// Load and parse the dedicated Ed25519 audit-seal key from config.
///
/// Returns `None` when `WYRD_AUDIT_SEAL_KEY_FILE`/`WYRD_AUDIT_SEAL_KEY_PEM`
/// are absent. Returns `Err` when the PEM is present but invalid.
fn build_audit_seal_key(
    config: &crate::config::WyrdServerConfig,
) -> Result<Option<Arc<wyrd_auth_issue::AuditSealKey>>, ServerBootError> {
    let Some(pem) = config.auth.audit_seal_key.as_ref() else {
        return Ok(None);
    };
    let key = wyrd_auth_issue::AuditSealKey::from_pkcs8_pem(pem.expose_secret())
        .map_err(|e| ServerBootError::SigningKey(format!("WYRD_AUDIT_SEAL_KEY is invalid: {e}")))?;
    Ok(Some(Arc::new(key)))
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

/// Spawn the storage sweeper if enabled and the platform admin pool is available.
///
/// Returns `None` when the sweeper is disabled or the platform admin pool is absent.
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

    let Some(admin_pool) = state.postgres.platform_admin_pool().cloned() else {
        tracing::warn!("storage sweeper skipped because platform admin pool is unavailable");
        return Ok(None);
    };

    let sweeper =
        wyrd_storage::sweeper::Sweeper::new(Arc::clone(&state.storage), admin_pool, cfg, shutdown);
    Ok(Some(tokio::spawn(async move { sweeper.run().await })))
}

/// Configuration for the background audit relay worker.
pub struct RelayConfig {
    /// Whether the relay is enabled (`WYRD_AUDIT_RELAY_ENABLED`, default `true`).
    pub enabled: bool,
    /// How long to sleep between relay ticks in milliseconds
    /// (`WYRD_AUDIT_RELAY_TICK_MS`, default `5000`).
    pub tick_ms: u64,
    /// Maximum rows to claim per tick (`WYRD_AUDIT_RELAY_CLAIM_LIMIT`, default `500`).
    pub claim_limit: i32,
}

impl RelayConfig {
    /// Read relay configuration from the process environment.
    pub fn from_env() -> Self {
        let enabled = std::env::var("WYRD_AUDIT_RELAY_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
        let tick_ms = std::env::var("WYRD_AUDIT_RELAY_TICK_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5_000u64);
        let claim_limit = std::env::var("WYRD_AUDIT_RELAY_CLAIM_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500i32);
        Self {
            enabled,
            tick_ms,
            claim_limit,
        }
    }
}

/// Spawn the background audit relay that drains `vala.audit_outbox` into
/// `vala.system.audit_log`. Mirrors `spawn_storage_sweeper`.
///
/// **Fail-closed in production (M-12).** If the relay is disabled or the
/// platform admin pool is unavailable in a production deployment, boot fails
/// with [`ServerBootError::AuditRelayRequired`]. In development/staging, a
/// disabled relay logs a warning and returns `None`.
///
/// # Errors
/// Returns [`ServerBootError::AuditRelayRequired`] when the relay cannot start
/// in production; returns `Ok(None)` when cleanly skipped in non-production.
pub async fn spawn_audit_relay(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<Option<tokio::task::JoinHandle<()>>, ServerBootError> {
    let cfg = RelayConfig::from_env();
    let is_production = state.deployment_profile == DeploymentProfile::Production;

    if !cfg.enabled {
        if is_production {
            return Err(ServerBootError::AuditRelayRequired {
                detail: "WYRD_AUDIT_RELAY_ENABLED=false is not permitted in production".to_owned(),
            });
        }
        tracing::warn!("audit relay disabled via WYRD_AUDIT_RELAY_ENABLED=false (dev/test only)");
        return Ok(None);
    }

    let Some(admin_pool) = state.postgres.platform_admin_pool() else {
        if is_production {
            return Err(ServerBootError::AuditRelayRequired {
                detail: "platform admin pool is unavailable".to_owned(),
            });
        }
        tracing::warn!("audit relay skipped: platform admin pool unavailable (dev/test only)");
        return Ok(None);
    };

    let relay = AuditRelay::new(Arc::clone(&state.bifrost), Arc::new(admin_pool.clone()));
    relay.ensure_audit_log_table().await?;

    let tick_interval = Duration::from_millis(cfg.tick_ms);
    let claim_limit = cfg.claim_limit;
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(tick_interval) => {}
            }
            if shutdown.is_cancelled() {
                break;
            }
            if let Err(e) = relay.tick(claim_limit).await {
                tracing::error!(error = %e, "audit relay tick failed");
            }
        }
    });

    Ok(Some(handle))
}

/// Configuration for the background audit reconciler worker.
pub struct ReconcileConfig {
    /// Whether the reconciler is enabled (`WYRD_AUDIT_RECONCILER_ENABLED`, default `true`).
    pub enabled: bool,
    /// How often to run reconciliation in milliseconds
    /// (`WYRD_AUDIT_RECONCILER_TICK_MS`, default `300000` = 5 minutes).
    pub tick_ms: u64,
}

impl ReconcileConfig {
    /// Read reconciler configuration from the process environment.
    pub fn from_env() -> Self {
        let enabled = std::env::var("WYRD_AUDIT_RECONCILER_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
        let tick_ms = std::env::var("WYRD_AUDIT_RECONCILER_TICK_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300_000u64);
        Self { enabled, tick_ms }
    }
}

/// Spawn the background audit reconciler that verifies durability of
/// `vala.audit_outbox` against `vala.system.audit_log`. Mirrors
/// `spawn_audit_relay`.
///
/// **Fail-closed in production (M-12).** If the reconciler is disabled or the
/// platform admin pool is unavailable in a production deployment, boot fails
/// with `WYRD_VALA_500_AUDIT_RECONCILER_REQUIRED`. In development/staging, a
/// disabled reconciler logs a warning and returns `None`.
///
/// # Errors
/// Returns [`ServerBootError::AuditReconcilerRequired`] when the reconciler
/// cannot start in production; returns `Ok(None)` when cleanly skipped.
pub async fn spawn_audit_reconciler(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<Option<tokio::task::JoinHandle<()>>, ServerBootError> {
    let cfg = ReconcileConfig::from_env();
    let is_production = state.deployment_profile == DeploymentProfile::Production;

    if !cfg.enabled {
        if is_production {
            return Err(ServerBootError::AuditReconcilerRequired {
                detail: "WYRD_AUDIT_RECONCILER_ENABLED=false is not permitted in production"
                    .to_owned(),
            });
        }
        tracing::warn!(
            "audit reconciler disabled via WYRD_AUDIT_RECONCILER_ENABLED=false (dev/test only)"
        );
        return Ok(None);
    }

    let Some(op) = state.postgres.operator_pool() else {
        if is_production {
            return Err(ServerBootError::AuditReconcilerRequired {
                detail: "platform admin pool is unavailable".to_owned(),
            });
        }
        tracing::warn!("audit reconciler skipped: platform admin pool unavailable (dev/test only)");
        return Ok(None);
    };

    let app_pool = state.postgres.vala_pool().clone();
    let catalog = Arc::clone(&state.bifrost);
    let tick_interval = Duration::from_millis(cfg.tick_ms);

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(tick_interval) => {}
            }
            if shutdown.is_cancelled() {
                break;
            }

            let tenant_ids = match vala_sql::queries::relay::list_audit_tenant_ids(&op).await {
                Ok(ids) => ids,
                Err(e) => {
                    tracing::error!(error = %e, "audit reconciler: tenant enumeration failed");
                    continue;
                }
            };

            let mut all_clean = true;
            for raw_id in tenant_ids {
                let tenant_id = match DataTenantId::try_from(raw_id) {
                    Ok(id) => id,
                    Err(_) => continue,
                };
                let result = match reconcile_audit(&app_pool, &catalog, tenant_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(
                            tenant_id = %tenant_id,
                            error = %e,
                            "audit reconciler: reconciliation failed"
                        );
                        all_clean = false;
                        continue;
                    }
                };

                let tenant_str = tenant_id.to_string();
                for gap in &result.seq_gaps {
                    counter!(
                        "vala_audit_reconcile_violations_total",
                        "tenant" => tenant_str.clone(),
                        "kind" => "seq_gap"
                    )
                    .increment(1);
                    tracing::error!(
                        tenant_id = %tenant_id,
                        gap_from = gap.gap_from,
                        gap_to = gap.gap_to,
                        "AUDIT INTEGRITY: seq gap detected"
                    );
                    all_clean = false;
                }
                for brk in &result.chain_breaks {
                    counter!(
                        "vala_audit_reconcile_violations_total",
                        "tenant" => tenant_str.clone(),
                        "kind" => "chain_break"
                    )
                    .increment(1);
                    tracing::error!(
                        tenant_id = %tenant_id,
                        seq = brk.seq,
                        "AUDIT INTEGRITY: hash-chain break detected"
                    );
                    all_clean = false;
                }
                for seq in &result.parity_misses {
                    counter!(
                        "vala_audit_reconcile_violations_total",
                        "tenant" => tenant_str.clone(),
                        "kind" => "parity_miss"
                    )
                    .increment(1);
                    tracing::error!(
                        tenant_id = %tenant_id,
                        seq,
                        "AUDIT INTEGRITY: shipped outbox row missing from warehouse"
                    );
                    all_clean = false;
                }
                for seq in &result.orphan_warehouse_seqs {
                    counter!(
                        "vala_audit_reconcile_violations_total",
                        "tenant" => tenant_str.clone(),
                        "kind" => "parity_miss"
                    )
                    .increment(1);
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        seq,
                        "AUDIT INTEGRITY: warehouse row has no matching shipped outbox entry"
                    );
                    all_clean = false;
                }

                if result.is_clean() {
                    gauge!(
                        "vala_audit_reconcile_last_success_timestamp",
                        "tenant" => tenant_str
                    )
                    .set(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64(),
                    );
                }
            }

            if all_clean {
                tracing::debug!("audit reconciler: all tenants clean");
            }
        }
    });

    Ok(Some(handle))
}

/// Spawn the maintenance scheduler (slice 01).
///
/// On each 60-second tick it collects the maintenance health report and runs
/// the commit-recovery sweep when a recovery pool is available. The sweep is
/// idempotent and fencing-token guarded, so two pods running it concurrently
/// cannot double-finalize a precommit. Other maintenance concerns (snapshot
/// expiry, compaction, orphan GC, index build, projection health) report
/// `pending` until their owning slices wire them into `scheduler::tick`.
///
/// Always spawns: `scheduler::tick` no-ops the recovery sweep when the recovery
/// pool is absent (dev/test), so the health tick still runs. Production requires
/// the recovery pool via [`check_recovery_pool`], enforced separately at boot.
#[must_use]
pub fn spawn_maintenance_scheduler(
    state: &AppState,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let vala = state.postgres.vala().clone();
    let catalog = Arc::clone(&state.bifrost);
    let tick_interval =
        Duration::from_secs(vala_bifrost::serving::repair::scheduler::TICK_INTERVAL_SECS);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(tick_interval) => {}
            }
            if shutdown.is_cancelled() {
                break;
            }
            vala_bifrost::serving::repair::scheduler::tick(&vala, &catalog).await;
        }
    })
}

/// Ensure the recovery pool is present in production deployments.
///
/// The commit-recovery sweep requires the `vala_recovery` SECURITY DEFINER
/// pool to claim and resolve stale precommits across all tenants. Without it
/// unresolved precommits accumulate indefinitely, risking permanent data loss.
///
/// **Fail-closed in production.** If `ValaPostgres::recovery_pool()` returns
/// `None` and the deployment profile is production, boot returns
/// [`ServerBootError::RecoveryPoolRequired`]. In development / staging, a
/// missing recovery pool logs a warning and returns `Ok(())`.
///
/// # Errors
/// Returns [`ServerBootError::RecoveryPoolRequired`] when the recovery pool is
/// absent in a production deployment.
pub fn check_recovery_pool(state: &AppState) -> Result<(), ServerBootError> {
    check_recovery_pool_inner(
        state.bifrost.recovery_pool().is_some(),
        &state.deployment_profile,
    )
}

/// Inner logic for [`check_recovery_pool`], accepting just the two values it needs.
/// Extracted so the behavior can be unit-tested without constructing `AppState`.
fn check_recovery_pool_inner(
    has_recovery_pool: bool,
    profile: &DeploymentProfile,
) -> Result<(), ServerBootError> {
    if !has_recovery_pool {
        if profile.is_production() {
            return Err(ServerBootError::RecoveryPoolRequired);
        }
        tracing::warn!(
            "recovery pool (vala_recovery DSN) is not configured — \
             commit-recovery sweep is disabled (dev/test only)"
        );
    }
    Ok(())
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
mod pg_tests {
    use super::*;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use tempfile::tempdir;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    async fn make_test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), Some(admin_pool));
        let vala = vala_sql::ValaPostgres::from_pools(app_pool, None);
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
        let vala = vala_sql::ValaPostgres::from_pools(app_pool, None);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let state = AppState::new(postgres, storage, crate::test_support::test_catalog().await);

        assert!(state.postgres.platform_admin_pool().is_some());
        assert_eq!(
            state.storage.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
    }
}

/// Slice 01 behavior-test gate: `boot::recovery_pool_required`.
///
/// `check_recovery_pool_inner` is a pure sync function: no Postgres, no
/// `AppState`. Tests drive it with the two boolean/enum inputs it needs.
///
/// Gate: `mise exec -- cargo test --locked -p wyrd-server --all-features boot::recovery_pool_required -- --nocapture`
#[cfg(test)]
mod recovery_pool_required {
    use super::*;

    /// Production boot without a recovery pool must fail with `RecoveryPoolRequired`.
    #[test]
    fn fails_in_production() {
        let result = check_recovery_pool_inner(false, &DeploymentProfile::Production);
        assert!(
            matches!(result, Err(ServerBootError::RecoveryPoolRequired)),
            "expected RecoveryPoolRequired in production without recovery pool, got {result:?}"
        );
    }

    /// In development profile a missing recovery pool returns `Ok(())`.
    #[test]
    fn missing_ok_in_development() {
        let result = check_recovery_pool_inner(false, &DeploymentProfile::Development);
        assert!(
            result.is_ok(),
            "missing recovery pool must not fail in development, got {result:?}"
        );
    }

    /// With a recovery pool present the check always succeeds regardless of profile.
    #[test]
    fn present_ok_in_production() {
        let result = check_recovery_pool_inner(true, &DeploymentProfile::Production);
        assert!(
            result.is_ok(),
            "a present recovery pool must not fail in production, got {result:?}"
        );
    }
}
