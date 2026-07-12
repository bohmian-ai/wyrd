use std::sync::Arc;

use wyrd_auth_check::{PolicyHook, StubAllowPolicyHook};
use wyrd_auth_issue::{AuditSealKey, IssuingKey};
use wyrd_crypt::SecretKey;
use wyrd_runtime::{PermissionCheck, RbacCheck};

use crate::auth::exchange_api_key::TokenExchangeSettings;
use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::components::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use crate::state::WyrdTokenVerifier;

/// Authentication handles: token issuance + verification + issuer/binding resolution.
#[derive(Clone, Default)]
pub struct ServerAuth {
    /// When `true`, preview-auth paths are active. Must be `false` in production
    /// (enforced by `AppState::production_validate`). Set via `WYRD_AUTH_ALLOW_PREVIEW`.
    pub allow_preview: bool,
    /// Ed25519 key used to sign access and API-key tokens. Required for any token-issuance route.
    pub issuing_key: Option<Arc<IssuingKey>>,
    /// Verifies bearer tokens on every authenticated request. Required in production
    /// (enforced by `AppState::production_validate`).
    pub token_verifier: Option<Arc<WyrdTokenVerifier>>,
    /// Resolves trusted OIDC issuers from Postgres for foreign-OIDC verification paths.
    pub trusted_issuer_resolver: Option<Arc<PgIssuerResolver>>,
    /// Resolves workload-to-principal bindings from Postgres for the JWT-bearer exchange path.
    pub workload_binding_resolver: Option<Arc<PgWorkloadBindingResolver>>,
    /// Symmetric key used to seal client secrets at rest. Required when trusted issuers carry secrets.
    pub sealing_key: Option<Arc<SecretKey>>,
    /// Dedicated Ed25519 audit-seal key. Not the JWT issuing key.
    /// Loaded from `WYRD_AUDIT_SEAL_KEY`. Required for audit sealing and verification.
    pub audit_seal_key: Option<Arc<AuditSealKey>>,
    /// TTL and delegation settings for token exchange responses.
    pub token_exchange_settings: TokenExchangeSettings,
}

/// Authorization handles: policy decision + RBAC evaluation + decision audit.
#[derive(Clone)]
pub struct ServerAuthz {
    /// RBAC permission evaluator. Defaults to `RbacCheck`.
    pub permission_check: Arc<dyn PermissionCheck>,
    /// Policy hook called after RBAC for additional allow/deny logic. Defaults to
    /// `StubAllowPolicyHook`; must be replaced in production
    /// (enforced by `AppState::production_validate`).
    pub policy_hook: Arc<dyn PolicyHook>,
    /// Audit writer for authorization decisions. Defaults to `NoopAuthzAuditWriter`;
    /// must be replaced in production (enforced by `AppState::production_validate`).
    pub audit_writer: Arc<dyn AuthzAuditWriter>,
}

impl Default for ServerAuthz {
    fn default() -> Self {
        Self {
            permission_check: Arc::new(RbacCheck),
            policy_hook: Arc::new(StubAllowPolicyHook),
            audit_writer: Arc::new(NoopAuthzAuditWriter),
        }
    }
}
