//! Shared axum application state.

use sqlx::PgPool;
use std::sync::Arc;
use wyrd_auth_check::{PolicyHook, StubAllowPolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::TokenVerifier;
use wyrd_runtime::{PermissionCheck, RbacCheck};
use wyrd_storage::StorageHandle;

use crate::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use crate::auth::permission_resolver::SqlPermissionResolver;

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
    pub token_verifier: Option<Arc<TokenVerifier<SqlPermissionResolver>>>,
    /// Policy hook for `/v1/authz/check`.
    pub policy_hook: Arc<dyn PolicyHook>,
    /// Audit-fact writer for `/v1/authz/check`.
    pub audit_writer: Arc<dyn AuthzAuditWriter>,
    /// Trust gate for inbound Wyrd request ID propagation.
    pub trusted_request_id_propagation: bool,
    /// Trusted upstream allowlist reserved for the mesh integration.
    pub trusted_upstreams: Vec<String>,
}

impl AppState {
    /// Build runtime state from the pools that survive boot.
    #[must_use]
    pub fn new(
        pool: PgPool,
        platform_admin_pool: Option<PgPool>,
        storage: Arc<StorageHandle>,
    ) -> Self {
        Self {
            pool,
            platform_admin_pool,
            storage,
            allow_preview_auth: cfg!(debug_assertions),
            permission_check: Arc::new(RbacCheck),
            issuing_key: None,
            token_verifier: None,
            policy_hook: Arc::new(StubAllowPolicyHook),
            audit_writer: Arc::new(NoopAuthzAuditWriter),
            trusted_request_id_propagation: false,
            trusted_upstreams: Vec::new(),
        }
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
        token_verifier: Arc<TokenVerifier<SqlPermissionResolver>>,
    ) -> Self {
        self.issuing_key = Some(issuing_key);
        self.token_verifier = Some(token_verifier);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use object_store::local::LocalFileSystem;
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

    use super::AppState;

    #[tokio::test]
    async fn defaults_for_test_safe() {
        let state = test_state();

        assert!(!state.trusted_request_id_propagation);
        assert!(state.trusted_upstreams.is_empty());
        assert_eq!(
            state.policy_hook.evaluate(&context()).await,
            PolicyDecision::Allow
        );
        assert!(state.audit_writer.is_stub_default());
    }

    fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let object_store =
            Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));

        AppState::new(
            app_pool,
            None,
            Arc::new(StorageHandle::new(
                BackendSigner::Local(signer),
                object_store,
            )),
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
