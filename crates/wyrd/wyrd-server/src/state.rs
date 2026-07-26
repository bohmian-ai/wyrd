//! Shared axum application state.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use datafusion::execution::memory_pool::MemoryPool;
use tokio_util::sync::CancellationToken;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::forge::ForgeContext;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
use wyrd_auth_verify::TokenVerifier;
use wyrd_storage::StorageHandle;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::PgIssuerResolver;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::{EvalAuditWriter, EvalRuns, TracingEvalAuditWriter, new_run_map};
use crate::components::health::ReadinessSnapshot;
use crate::config::DeploymentProfile;
use crate::postgres::ServerPostgres;

/// Production [`TokenVerifier`] specialization: SQL-backed permission resolution
/// (`SqlPermissionResolver`) plus Postgres-backed issuer resolution
/// (`PgIssuerResolver`). Aliased so the nested handle type stays readable across
/// `AppState`, the boot path, and the test harness.
pub type WyrdTokenVerifier = TokenVerifier<SqlPermissionResolver, PgIssuerResolver>;
/// Production Gate specialization used by AppState.
pub type ServerGate =
    vala_bifrost_redux::gate::Gate<BifrostCatalog, SqlPermissionResolver, PgIssuerResolver>;

/// Runtime-ready limits derived from config.
#[derive(Debug, Clone, Copy)]
pub struct LimitsConfig {
    /// Maximum allowed request body size in bytes.
    pub body_bytes: usize,
    /// Per-request processing timeout.
    pub timeout: std::time::Duration,
    /// Maximum in-flight concurrent requests.
    pub concurrency: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            body_bytes: 1_048_576,
            timeout: std::time::Duration::from_secs(30),
            concurrency: 1024,
        }
    }
}

/// Process-wide handle registry. One instance is shared by all HTTP handlers.
///
/// Foundation commits append their owned handles here, for example storage,
/// auth verification, and policy evaluation. This skeleton ships only the
/// runtime database pools that already survive boot.
#[derive(Clone)]
pub struct AppState {
    /// Composed production Postgres handle. Single DB access path for all routes.
    pub postgres: Arc<ServerPostgres>,
    /// Process-wide artifact storage handle.
    pub storage: Arc<StorageHandle>,
    /// Process-wide Bifrost OLAP catalog.
    pub bifrost: Arc<WyrdCatalog>,
    /// Tenant-qualified Redux catalog used by Gate, Scribe, Forge, and Oracle
    /// query paths.
    pub bifrost_redux: Option<Arc<BifrostCatalog>>,
    /// Shared parent memory governor used by Scribe, Forge, and Oracle reads.
    pub bifrost_memory: Option<BifrostMemoryGovernor>,
    /// Shared DataFusion pool bounded by the Bifrost parent ceiling.
    pub bifrost_query_memory: Option<Arc<dyn MemoryPool>>,
    /// Private lifecycle handle for the booted Scribe runtime. Request handlers
    /// use `gate`; this field exists only for startup recovery and shutdown.
    pub(crate) scribe: Option<Arc<ScribeImpl>>,
    /// Fully constructed native Bifrost Gate. Production boot installs this
    /// before any listener can bind; test-only states may leave it absent.
    pub gate: Option<Arc<ServerGate>>,
    /// Dedicated Tokio runtime that owns Scribe coordination consumers.
    pub(crate) scribe_coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Shared Redux Forge context built from the process-wide catalog and storage.
    pub forge_context: Option<Arc<ForgeContext>>,
    /// Interval used by the supervised Forge worker.
    pub forge_interval: Duration,
    /// Authentication handles: token issuance + verification + issuer/binding resolution.
    pub auth: ServerAuth,
    /// Authorization handles: policy decision + RBAC evaluation + decision audit.
    pub authz: ServerAuthz,
    /// Deployment posture (Development / Production) locked at boot.
    pub deployment_profile: DeploymentProfile,
    /// Shared cancellation token for cooperative shutdown.
    pub shutdown_token: CancellationToken,
    /// Telemetry guard (holds the tracer provider).
    pub telemetry: Arc<TelemetryGuard>,
    /// Request-shaping limits for the router middleware stack.
    pub limits: LimitsConfig,
    /// gRPC health reporter shared between HTTP readiness and gRPC health service.
    pub grpc_health: HealthReporter,
    /// Cached readiness snapshot from the background readiness_loop task.
    pub readiness: Arc<ArcSwap<ReadinessSnapshot>>,
    /// Tenant-keyed in-memory eval run/lease/session map. Ephemeral, single-replica.
    pub eval_runs: EvalRuns,
    /// Audit sink for eval run open/complete events.
    pub eval_audit: Arc<dyn EvalAuditWriter>,
}

impl AppState {
    /// Build runtime state from production-ready Postgres handles.
    #[must_use]
    pub fn new(
        postgres: Arc<ServerPostgres>,
        storage: Arc<StorageHandle>,
        bifrost: Arc<WyrdCatalog>,
    ) -> Self {
        let (reporter, _service) = wyrd_tonic::tonic_health::server::health_reporter();
        Self {
            postgres,
            storage,
            bifrost,
            bifrost_redux: None,
            bifrost_memory: None,
            bifrost_query_memory: None,
            scribe: None,
            gate: None,
            scribe_coordination_runtime: None,
            forge_context: None,
            forge_interval: Duration::from_secs(60),
            auth: ServerAuth::default(),
            authz: ServerAuthz::default(),
            deployment_profile: DeploymentProfile::Development,
            shutdown_token: CancellationToken::new(),
            telemetry: Arc::new(wyrd_telemetry::init_test_only_no_global(
                wyrd_telemetry::TelemetryConfig::default(),
            )),
            limits: LimitsConfig::default(),
            grpc_health: reporter,
            readiness: Arc::new(ArcSwap::from_pointee(ReadinessSnapshot::initial())),
            eval_runs: new_run_map(),
            eval_audit: Arc::new(TracingEvalAuditWriter),
        }
    }

    /// Replace the eval audit writer, primarily for tests.
    #[must_use]
    pub fn with_eval_audit(mut self, eval_audit: Arc<dyn EvalAuditWriter>) -> Self {
        self.eval_audit = eval_audit;
        self
    }

    /// Attach authentication handles (issuance, verification, resolvers).
    #[must_use]
    pub fn with_auth(mut self, auth: ServerAuth) -> Self {
        self.auth = auth;
        self
    }

    /// Attach authorization handles (policy, RBAC, audit).
    #[must_use]
    pub fn with_authz(mut self, authz: ServerAuthz) -> Self {
        self.authz = authz;
        self
    }

    /// Set the deployment posture.
    #[must_use]
    pub fn with_deployment_profile(mut self, profile: DeploymentProfile) -> Self {
        self.deployment_profile = profile;
        self
    }

    /// Set the shared shutdown cancellation token.
    #[must_use]
    pub fn with_shutdown_token(mut self, shutdown_token: CancellationToken) -> Self {
        self.shutdown_token = shutdown_token;
        self
    }

    /// Attach the live telemetry guard.
    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<TelemetryGuard>) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Attach request-shaping limits from WyrdServerConfig.
    #[must_use]
    pub fn with_limits(mut self, limits: LimitsConfig) -> Self {
        self.limits = limits;
        self
    }

    /// Attach the gRPC health reporter.
    #[must_use]
    pub fn with_grpc_health(mut self, reporter: HealthReporter) -> Self {
        self.grpc_health = reporter;
        self
    }

    /// Attach the cached readiness publisher.
    #[must_use]
    pub fn with_readiness(mut self, readiness: Arc<ArcSwap<ReadinessSnapshot>>) -> Self {
        self.readiness = readiness;
        self
    }

    /// Replace the storage handle.
    #[must_use]
    pub fn with_storage(mut self, storage: Arc<StorageHandle>) -> Self {
        self.storage = storage;
        self
    }

    /// Attach the Redux-owned tenant-qualified Bifrost catalog.
    #[must_use]
    pub fn with_bifrost_redux(mut self, catalog: Arc<BifrostCatalog>) -> Self {
        self.bifrost_redux = Some(catalog);
        self
    }

    /// Attach the process-global Bifrost memory governor.
    #[must_use]
    pub fn with_bifrost_memory(mut self, memory: BifrostMemoryGovernor) -> Self {
        self.bifrost_query_memory = Some(Arc::new(
            vala_bifrost_redux::scribe::memory::BifrostDataFusionMemoryPool::new(memory.clone()),
        ));
        self.bifrost_memory = Some(memory);
        self
    }

    /// Attach the server-owned queued Scribe runtime.
    #[must_use]
    pub fn with_scribe(mut self, scribe: Arc<ScribeImpl>) -> Self {
        self.scribe = Some(scribe);
        self
    }

    /// Attach the fully constructed server-owned Gate.
    #[must_use]
    pub fn with_gate(mut self, gate: Arc<ServerGate>) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Keep the dedicated Scribe coordination runtime alive with application
    /// state until the server performs its shutdown drain.
    #[must_use]
    pub fn with_scribe_coordination_runtime(
        mut self,
        runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        self.scribe_coordination_runtime = Some(runtime);
        self
    }

    /// Attach the single production Forge context used by the supervised worker.
    #[must_use]
    pub fn with_forge_context(mut self, context: ForgeContext) -> Self {
        self.forge_context = Some(Arc::new(context));
        self
    }

    /// Set the supervised Forge worker interval.
    #[must_use]
    pub fn with_forge_interval(mut self, interval: Duration) -> Self {
        self.forge_interval = interval;
        self
    }

    /// Flush the private Scribe runtime for the test harness only.
    #[cfg(feature = "test-support")]
    pub async fn flush_scribe_for_test(
        &self,
        mut conn: vala_sql::TenantConn<'_>,
    ) -> Result<(), vala_bifrost_redux::contracts::ScribeError> {
        let Some(scribe) = &self.scribe else {
            return Err(vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: "Scribe is not configured".to_owned(),
            });
        };
        let post_commit = scribe.force_seal(&mut conn).await?;
        conn.commit().await.map_err(|error| {
            vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: error.to_string(),
            }
        })?;
        scribe.complete_post_commit(post_commit).await
    }

    /// Trip the Scribe WAL breaker for a deterministic test-tier probe.
    #[cfg(feature = "test-support")]
    pub fn trip_scribe_wal_disk_full_for_test(&self) -> Result<(), String> {
        let Some(scribe) = &self.scribe else {
            return Err("Scribe is not configured".to_owned());
        };
        scribe.trip_wal_disk_full_for_test();
        Ok(())
    }

    /// Return the bounded Scribe ownership snapshot for test-tier inspection.
    #[cfg(feature = "test-support")]
    pub fn scribe_inspection_snapshot_for_test(
        &self,
    ) -> Result<vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot, String> {
        let Some(scribe) = &self.scribe else {
            return Err("Scribe is not configured".to_owned());
        };
        scribe
            .inspection_snapshot()
            .map_err(|error| error.to_string())
    }
}

/// Errors raised by [`AppState::production_validate`].
#[derive(Debug, thiserror::Error)]
pub enum ProductionValidationError {
    /// Stub allow policy hook is mounted in a production build.
    #[error(
        "authz.policy_hook is StubAllowPolicyHook in a production build; install a real PolicyHook"
    )]
    StubPolicyHook,
    /// Noop audit writer is mounted in a production build.
    #[error(
        "authz.audit_writer is NoopAuthzAuditWriter in a production build; install a real AuthzAuditWriter"
    )]
    NoopAuditWriter,
    /// Token verifier is absent in a production build.
    #[error("auth.token_verifier is None in a production build; auth-plan boot must install it")]
    MissingTokenVerifier,
    /// Preview auth is still enabled in a production build.
    #[error("auth.allow_preview is true in a production build; clear WYRD_AUTH_ALLOW_PREVIEW")]
    PreviewAuthEnabled,
}

impl AppState {
    /// Reject production-profile boots where stubs survived assembly.
    ///
    /// Development-profile boots short-circuit to `Ok(())` so unit tests work.
    pub fn production_validate(&self) -> Result<(), ProductionValidationError> {
        if !self.deployment_profile.is_production() {
            return Ok(());
        }
        if self.authz.policy_hook.is_stub_default() {
            return Err(ProductionValidationError::StubPolicyHook);
        }
        if self.authz.audit_writer.is_stub_default() {
            return Err(ProductionValidationError::NoopAuditWriter);
        }
        if self.auth.token_verifier.is_none() {
            return Err(ProductionValidationError::MissingTokenVerifier);
        }
        if self.auth.allow_preview {
            return Err(ProductionValidationError::PreviewAuthEnabled);
        }
        Ok(())
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use wyrd_auth_check::AuthzCheckContext;
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::policy::PolicyDecision;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    use super::{AppState, LimitsConfig, ProductionValidationError};

    #[tokio::test]
    async fn defaults_for_test_safe() {
        let state = test_state().await;

        assert_eq!(
            state.authz.policy_hook.evaluate(&context()).await,
            PolicyDecision::Allow
        );
        assert!(state.authz.audit_writer.is_stub_default());
    }

    #[tokio::test]
    async fn new_state_has_fresh_cancellation_token() {
        let state = test_state().await;
        assert!(!state.shutdown_token.is_cancelled());
        state.shutdown_token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    #[tokio::test]
    async fn with_shutdown_token_replaces_field() {
        let state = test_state().await;
        let token = tokio_util::sync::CancellationToken::new();
        let state = state.with_shutdown_token(token.clone());
        token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    #[tokio::test]
    async fn production_validate_passes_development_profile() {
        let state = test_state().await;
        assert!(state.production_validate().is_ok());
    }

    #[tokio::test]
    async fn production_validate_rejects_stub_on_production() {
        let state = test_state()
            .await
            .with_deployment_profile(crate::config::DeploymentProfile::Production);
        let err = state.production_validate().unwrap_err();
        assert!(matches!(err, ProductionValidationError::StubPolicyHook));
    }

    #[tokio::test]
    async fn with_limits_updates_all_fields() {
        let state = test_state().await;
        let limits = LimitsConfig {
            body_bytes: 2048,
            timeout: std::time::Duration::from_millis(1000),
            concurrency: 10,
        };
        let state = state.with_limits(limits);
        assert_eq!(state.limits.body_bytes, 2048);
        assert_eq!(state.limits.concurrency, 10);
    }

    async fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn principal(name: &str) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref(name),
                card_ref_scope: wyrd_spec::reference::CardRefScope::own(&card_ref(name)),
            },
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    fn context() -> AuthzCheckContext {
        let caller = principal("caller");
        let callee = principal("callee");
        let verified = VerifiedToken {
            principal: callee,
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&caller),
            }],
            exp: chrono::Utc::now(),
            iat: chrono::Utc::now(),
        };
        let request = wyrd_auth_check::AuthzCheckRequest {
            target: card_ref("callee"),
            action: "card_write".to_owned(),
            context: serde_json::json!({}),
        };
        let request_id = RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id");

        AuthzCheckContext::from_verified(&verified, request, None, request_id)
            .expect("test context is delegated")
    }
}
