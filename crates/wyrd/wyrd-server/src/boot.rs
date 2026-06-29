//! Server boot sequence for SQL-backed Wyrd runtime state.

use std::sync::Arc;

use secrecy::ExposeSecret;
use tokio_util::sync::CancellationToken;
use wyrd_auth_oidc::WorkloadBinding;
use wyrd_semver::VersionBlock;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_sql::{
    pool::{build_app_pool, build_migrator_pool, build_platform_admin_pool},
    postgres_boot::{BootError, PostgresBoot},
};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

use crate::config::WorkloadBindingEntry;
use crate::state::AppState;

/// Errors raised while assembling server state.
#[derive(Debug, thiserror::Error)]
pub enum ServerBootError {
    /// Postgres boot or pool construction failed.
    #[error(transparent)]
    Postgres(#[from] BootError),
    /// Runtime pool construction failed.
    #[error("database pool construction failed")]
    PoolConnect(#[source] sqlx::Error),
    /// SQL migrations failed.
    #[error(transparent)]
    Sql(#[from] wyrd_sql::error::SqlError),
    /// Storage boot failed.
    #[error(transparent)]
    Storage(#[from] wyrd_storage::StorageError),
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
    let migrator_pool = build_migrator_pool(dsns.migrator.expose_secret())
        .await
        .map_err(ServerBootError::PoolConnect)?;

    let migration_result: Result<(), wyrd_sql::error::SqlError> = async {
        wyrd_sql::migrate(&migrator_pool).await?;
        vala_sql::migrate(&migrator_pool).await?;
        Ok(())
    }
    .await;
    migrator_pool.close().await;
    migration_result?;

    let pool = build_app_pool(dsns.app.expose_secret())
        .await
        .map_err(ServerBootError::PoolConnect)?;
    let platform_admin_pool = match dsns.platform_admin {
        Some(dsn) => Some(
            build_platform_admin_pool(dsn.expose_secret())
                .await
                .map_err(ServerBootError::PoolConnect)?,
        ),
        None => None,
    };
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

    Ok(AppState::new(pool, platform_admin_pool, storage))
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
    if config.request_id.trust_upstream && config.request_id.trusted_upstreams.is_empty() {
        tracing::warn!("request_id.trust_upstream=true with no trusted_upstreams configured");
    }
    if config.grpc.reflection_enabled {
        tracing::warn!("grpc.reflection_enabled=true in development profile");
    }
    if config.auth.allow_preview {
        tracing::warn!("auth.allow_preview=true in development profile");
    }
}

/// Assemble runtime state from a resolved `WyrdServerConfig`.
///
/// This is the primary boot entry point from `main.rs` once the config is
/// loaded. It runs the postgres boot, migrations, and pool phases, then chains
/// the `with_*` builder calls to attach config-derived fields.
///
/// # Errors
/// Returns [`ServerBootError`] when database boot, migration, pool construction,
/// or CIDR parsing fails.
pub async fn build_app_state_from_config(
    config: &crate::config::WyrdServerConfig,
    shutdown: CancellationToken,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
    reporter: wyrd_tonic::tonic_health::server::HealthReporter,
) -> Result<AppState, ServerBootError> {
    let boot = PostgresBoot::from_env().await?;
    let state = build_app_state_from_boot(&boot).await?;

    let trusted_upstreams_parsed: Vec<ipnetwork::IpNetwork> = config
        .request_id
        .trusted_upstreams
        .iter()
        .filter_map(|cidr| cidr.parse::<ipnetwork::IpNetwork>().ok())
        .collect();

    let mut state = state
        .with_deployment_profile(config.deployment_profile)
        .with_shutdown_token(shutdown)
        .with_telemetry(telemetry)
        .with_limits(config.limits.into_state())
        .with_grpc_health(reporter)
        .with_trusted_upstreams_parsed(Arc::from(trusted_upstreams_parsed))
        .with_trusted_request_id_propagation(config.request_id.trust_upstream)
        .with_preview_auth(config.auth.allow_preview);

    // Resolve the implicit tenant once for both the trusted-issuer and
    // workload-binding registries. Both resolve through the same slug path the
    // request handlers use, so the bound tenant matches request-time lookups by
    // construction.
    let implicit_tenant =
        if config.trusted_issuers.is_empty() && config.workload_bindings.is_empty() {
            None
        } else {
            let slug = config.auth.tenant_slug.as_ref().ok_or_else(|| {
                ServerBootError::TenantSlugUnresolved {
                    slug: "(unset)".to_owned(),
                }
            })?;
            Some(crate::issuer_boot::resolve_implicit_tenant(&state.pool, slug).await?)
        };

    // Resolve the configured trusted issuers into the static registry. Discovery
    // failures fail boot closed. The resolved registry is also handed to the
    // token verifier below so its external (foreign-OIDC) path can exchange
    // federated tokens.
    let trusted_issuer_registry = if config.trusted_issuers.is_empty() {
        None
    } else {
        let tenant_id =
            implicit_tenant.expect("implicit tenant resolved when trusted issuers are configured");
        let issuers = crate::issuer_boot::ConfigFileIssuerResolver::new(
            config.trusted_issuers.clone(),
            tenant_id,
        )
        .resolve()
        .await?;
        let registry = Arc::new(wyrd_auth_oidc::TrustedIssuerRegistry::from_issuers(issuers));
        state = state.with_trusted_issuer_registry(Arc::clone(&registry));
        Some(registry)
    };

    // Install Wyrd's own auth handles (issuing key + token verifier). A server
    // without a signing key cannot mint or verify Wyrd JWTs, so auth is the same
    // working experience across environments: a production profile (staging and
    // production) fails boot closed when no key is provisioned; development mints
    // an ephemeral key so auth works on a fresh local run. The verifier's
    // external path is wired to the trusted-issuer registry resolved above.
    match config.auth.signing_key.as_ref() {
        Some(signing_key) => {
            let (issuing_key, verifier) = crate::auth_boot::build_auth_handles(
                signing_key,
                &state.pool,
                trusted_issuer_registry.as_ref(),
            )?;
            state = state.with_auth_handles(issuing_key, verifier);
        }
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
            let (issuing_key, verifier) = crate::auth_boot::build_auth_handles(
                &ephemeral,
                &state.pool,
                trusted_issuer_registry.as_ref(),
            )?;
            state = state.with_auth_handles(issuing_key, verifier);
        }
    }

    // Populate the in-memory workload binding registry from `[[workload_bindings]]`.
    // Each entry's card target is built into a server-owned `CardRef` (F04, never
    // from token claims) under the SAME implicit tenant the request-time
    // `WorkloadBindingResolver::binding` lookup is keyed by, so matches hold by
    // construction. Building the ref is not a card-existence check.
    if !config.workload_bindings.is_empty() {
        let tenant_id = implicit_tenant
            .expect("implicit tenant resolved when workload bindings are configured");
        let bindings = build_workload_bindings(&config.workload_bindings, tenant_id)?;
        let registry = crate::auth::jwt_bearer::WorkloadBindingRegistry::from_bindings(bindings);
        state = state.with_workload_binding_registry(Arc::new(registry));
    }

    Ok(state)
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

    let Some(admin_pool) = state.platform_admin_pool.clone() else {
        tracing::warn!("storage sweeper skipped because platform admin pool is unavailable");
        return Ok(None);
    };

    let sweeper =
        wyrd_storage::sweeper::Sweeper::new(Arc::clone(&state.storage), admin_pool, cfg, shutdown);
    Ok(Some(tokio::spawn(async move { sweeper.run().await })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use tempfile::tempdir;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    #[tokio::test(flavor = "current_thread")]
    async fn app_state_retains_only_runtime_pools() {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let platform_admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let state = AppState::new(app_pool, Some(platform_admin_pool), storage);

        assert!(state.platform_admin_pool.is_some());
        assert_eq!(
            state.storage.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
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

    #[tokio::test(flavor = "current_thread")]
    async fn workload_binding_registry_resolves_bound_and_rejects_unbound() {
        use wyrd_auth_oidc::WorkloadBindingResolver;

        let tenant = implicit_tenant();
        let bindings =
            build_workload_bindings(&[sample_binding_entry()], tenant).expect("bindings build");
        let registry = crate::auth::jwt_bearer::WorkloadBindingRegistry::from_bindings(bindings);
        let issuer = IssuerUrl::new("https://idp.example.com".to_owned()).expect("issuer url");

        let card_ref = registry
            .binding(
                &tenant,
                &issuer,
                "system:serviceaccount:default/my-sa",
                Some("my-audience"),
            )
            .await
            .expect("lookup ok")
            .expect("bound subject resolves to a card ref");
        assert_eq!(card_ref.kind, CardKind::Service);
        assert_eq!(card_ref.name.to_string(), "my-model");
        assert_eq!(card_ref.version.to_string(), "1.0.0");
        assert_eq!(card_ref.space.to_string(), "prod");
        assert!(card_ref.uid.is_none());

        let unbound = registry
            .binding(
                &tenant,
                &issuer,
                "system:serviceaccount:default/other-sa",
                Some("my-audience"),
            )
            .await
            .expect("lookup ok");
        assert!(unbound.is_none(), "unbound subject must resolve to None");
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
