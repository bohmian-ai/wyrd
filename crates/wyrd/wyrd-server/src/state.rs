//! Shared axum application state.

use std::sync::Arc;

use arc_swap::ArcSwap;
use ipnetwork::IpNetwork;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use wyrd_auth_check::{PolicyHook, StubAllowPolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::TokenVerifier;
use wyrd_crypt::SecretKey;
use wyrd_runtime::{PermissionCheck, RbacCheck};
use wyrd_storage::StorageHandle;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use crate::auth::exchange_api_key::TokenExchangeSettings;
use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::config::DeploymentProfile;
use crate::eval::{EvalAuditWriter, EvalRuns, TracingEvalAuditWriter, new_run_map};
use crate::health::ReadinessSnapshot;

/// Production [`TokenVerifier`] specialization: SQL-backed permission resolution
/// (`SqlPermissionResolver`) plus Postgres-backed issuer resolution
/// (`PgIssuerResolver`). Aliased so the nested handle type stays readable across
/// `AppState`, the boot path, and the test harness.
pub type WyrdTokenVerifier = TokenVerifier<SqlPermissionResolver, PgIssuerResolver>;

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
    /// Runtime `wyrd_app` pool. Tenant-scoped traffic uses this pool and RLS
    /// applies on tenant tables.
    pub pool: PgPool,
    /// Optional audited `wyrd_platform_admin` pool for cross-tenant platform
    /// operations. Dedicated deployments may leave this unset.
    pub platform_admin_pool: Option<PgPool>,
    /// Process-wide artifact storage handle.
    pub storage: Arc<StorageHandle>,
    /// Runtime preview gate for auth routes that depend on card-registry principal projection.
    pub allow_preview_auth: bool,
    /// RBAC checker for auth route permission gates.
    pub permission_check: Arc<dyn PermissionCheck>,
    /// JWT issuer for auth token routes.
    pub issuing_key: Option<Arc<IssuingKey>>,
    /// JWT verifier for token-exchange routes.
    pub token_verifier: Option<Arc<WyrdTokenVerifier>>,
    /// Postgres-backed trusted OIDC issuer resolver for human login + federation.
    pub trusted_issuer_resolver: Option<Arc<PgIssuerResolver>>,
    /// Postgres-backed workload binding resolver for jwt-bearer exchanges.
    pub workload_binding_resolver: Option<Arc<PgWorkloadBindingResolver>>,
    /// Process-wide sealing key for decrypting issuer client secrets on read.
    pub sealing_key: Option<Arc<SecretKey>>,
    /// Policy hook for authz-check evaluation.
    pub policy_hook: Arc<dyn PolicyHook>,
    /// Audit-fact writer for authz-check decisions.
    pub audit_writer: Arc<dyn AuthzAuditWriter>,
    /// Trust gate for inbound Wyrd request ID propagation.
    pub trusted_request_id_propagation: bool,
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
    /// Parsed CIDR allowlist for the request-id trust gate.
    pub trusted_upstreams_parsed: Arc<[IpNetwork]>,
    /// Access/refresh token TTL settings for all exchange paths.
    pub token_exchange_settings: TokenExchangeSettings,
    /// Tenant-keyed in-memory eval run/lease/session map. Ephemeral, single-replica.
    pub eval_runs: EvalRuns,
    /// Audit sink for eval run open/complete events.
    pub eval_audit: Arc<dyn EvalAuditWriter>,
}

impl AppState {
    /// Build runtime state from the pools that survive boot.
    #[must_use]
    pub fn new(
        pool: PgPool,
        platform_admin_pool: Option<PgPool>,
        storage: Arc<StorageHandle>,
    ) -> Self {
        let (reporter, _service) = wyrd_tonic::tonic_health::server::health_reporter();
        Self {
            pool,
            platform_admin_pool,
            storage,
            allow_preview_auth: false,
            permission_check: Arc::new(RbacCheck),
            issuing_key: None,
            token_verifier: None,
            trusted_issuer_resolver: None,
            workload_binding_resolver: None,
            sealing_key: None,
            policy_hook: Arc::new(StubAllowPolicyHook),
            audit_writer: Arc::new(NoopAuthzAuditWriter),
            trusted_request_id_propagation: false,
            deployment_profile: DeploymentProfile::Development,
            shutdown_token: CancellationToken::new(),
            telemetry: Arc::new(wyrd_telemetry::init_test_only_no_global(
                wyrd_telemetry::TelemetryConfig::default(),
            )),
            limits: LimitsConfig::default(),
            grpc_health: reporter,
            readiness: Arc::new(ArcSwap::from_pointee(ReadinessSnapshot::initial())),
            trusted_upstreams_parsed: Arc::from(Vec::<IpNetwork>::new()),
            token_exchange_settings: TokenExchangeSettings::default(),
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

    /// Override the auth preview gate, primarily for tests and local config wiring.
    #[must_use]
    pub fn with_preview_auth(mut self, allow_preview_auth: bool) -> Self {
        self.allow_preview_auth = allow_preview_auth;
        self
    }

    /// Attach auth signing and verification handles.
    #[must_use]
    pub fn with_auth_handles(
        mut self,
        issuing_key: Arc<IssuingKey>,
        token_verifier: Arc<WyrdTokenVerifier>,
    ) -> Self {
        self.issuing_key = Some(issuing_key);
        self.token_verifier = Some(token_verifier);
        self
    }

    /// Attach the Postgres-backed trusted OIDC issuer resolver.
    #[must_use]
    pub fn with_trusted_issuer_resolver(
        mut self,
        trusted_issuer_resolver: Arc<PgIssuerResolver>,
    ) -> Self {
        self.trusted_issuer_resolver = Some(trusted_issuer_resolver);
        self
    }

    /// Attach the Postgres-backed workload binding resolver.
    #[must_use]
    pub fn with_workload_binding_resolver(
        mut self,
        workload_binding_resolver: Arc<PgWorkloadBindingResolver>,
    ) -> Self {
        self.workload_binding_resolver = Some(workload_binding_resolver);
        self
    }

    /// Attach the process-wide issuer-secret sealing key.
    #[must_use]
    pub fn with_sealing_key(mut self, sealing_key: Arc<SecretKey>) -> Self {
        self.sealing_key = Some(sealing_key);
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

    /// Attach the parsed CIDR allowlist for the request-id trust gate.
    #[must_use]
    pub fn with_trusted_upstreams_parsed(mut self, parsed: Arc<[IpNetwork]>) -> Self {
        self.trusted_upstreams_parsed = parsed;
        self
    }

    /// Toggle inbound Wyrd request-id propagation trust.
    #[must_use]
    pub fn with_trusted_request_id_propagation(mut self, trust: bool) -> Self {
        self.trusted_request_id_propagation = trust;
        self
    }

    /// Replace the storage handle.
    #[must_use]
    pub fn with_storage(mut self, storage: Arc<StorageHandle>) -> Self {
        self.storage = storage;
        self
    }

    /// Replace the policy hook.
    #[must_use]
    pub fn with_policy_hook(mut self, policy_hook: Arc<dyn PolicyHook>) -> Self {
        self.policy_hook = policy_hook;
        self
    }

    /// Replace the authz audit writer.
    #[must_use]
    pub fn with_audit_writer(mut self, audit_writer: Arc<dyn AuthzAuditWriter>) -> Self {
        self.audit_writer = audit_writer;
        self
    }

    /// Override access/refresh token TTL settings for all exchange paths.
    #[must_use]
    pub fn with_token_exchange_settings(mut self, settings: TokenExchangeSettings) -> Self {
        self.token_exchange_settings = settings;
        self
    }
}

/// Errors raised by [`AppState::production_validate`].
#[derive(Debug, thiserror::Error)]
pub enum ProductionValidationError {
    /// Stub allow policy hook is mounted in a production build.
    #[error(
        "AppState.policy_hook is StubAllowPolicyHook in a production build; install a real PolicyHook"
    )]
    StubPolicyHook,
    /// Noop audit writer is mounted in a production build.
    #[error(
        "AppState.audit_writer is NoopAuthzAuditWriter in a production build; install a real AuthzAuditWriter"
    )]
    NoopAuditWriter,
    /// Request-id propagation is enabled with no trusted upstream CIDRs.
    #[error("AppState.trusted_request_id_propagation=true requires non-empty trusted_upstreams")]
    UntrustedRequestIdEdge,
    /// Token verifier is absent in a production build.
    #[error(
        "AppState.token_verifier is None in a production build; auth-plan boot must install it"
    )]
    MissingTokenVerifier,
    /// Preview auth is still enabled in a production build.
    #[error(
        "AppState.allow_preview_auth is true in a production build; clear WYRD_AUTH_ALLOW_PREVIEW"
    )]
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
        if self.policy_hook.is_stub_default() {
            return Err(ProductionValidationError::StubPolicyHook);
        }
        if self.audit_writer.is_stub_default() {
            return Err(ProductionValidationError::NoopAuditWriter);
        }
        if self.trusted_request_id_propagation && self.trusted_upstreams_parsed.is_empty() {
            return Err(ProductionValidationError::UntrustedRequestIdEdge);
        }
        if self.token_verifier.is_none() {
            return Err(ProductionValidationError::MissingTokenVerifier);
        }
        if self.allow_preview_auth {
            return Err(ProductionValidationError::PreviewAuthEnabled);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

    use super::{AppState, LimitsConfig, ProductionValidationError};

    #[tokio::test]
    async fn defaults_for_test_safe() {
        let state = test_state();

        assert!(!state.trusted_request_id_propagation);
        assert!(state.trusted_upstreams_parsed.is_empty());
        assert_eq!(
            state.policy_hook.evaluate(&context()).await,
            PolicyDecision::Allow
        );
        assert!(state.audit_writer.is_stub_default());
    }

    #[tokio::test]
    async fn new_state_has_fresh_cancellation_token() {
        let state = test_state();
        assert!(!state.shutdown_token.is_cancelled());
        state.shutdown_token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    #[tokio::test]
    async fn with_shutdown_token_replaces_field() {
        let state = test_state();
        let token = tokio_util::sync::CancellationToken::new();
        let state = state.with_shutdown_token(token.clone());
        token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    #[tokio::test]
    async fn production_validate_passes_development_profile() {
        let state = test_state();
        assert!(state.production_validate().is_ok());
    }

    #[tokio::test]
    async fn production_validate_rejects_stub_on_production() {
        let state =
            test_state().with_deployment_profile(crate::config::DeploymentProfile::Production);
        let err = state.production_validate().unwrap_err();
        assert!(matches!(err, ProductionValidationError::StubPolicyHook));
    }

    #[tokio::test]
    async fn with_limits_updates_all_fields() {
        let state = test_state();
        let limits = LimitsConfig {
            body_bytes: 2048,
            timeout: std::time::Duration::from_millis(1000),
            concurrency: 10,
        };
        let state = state.with_limits(limits);
        assert_eq!(state.limits.body_bytes, 2048);
        assert_eq!(state.limits.concurrency, 10);
    }

    fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");

        AppState::new(
            app_pool,
            None,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
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
