use std::sync::Arc;

use wyrd_auth_check::{PolicyHook, StubAllowPolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_crypt::SecretKey;
use wyrd_runtime::{PermissionCheck, RbacCheck};

use crate::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use crate::auth::exchange_api_key::TokenExchangeSettings;
use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use crate::state::WyrdTokenVerifier;

/// Authentication handles: token issuance + verification + issuer/binding resolution.
#[derive(Clone)]
pub struct ServerAuth {
    pub allow_preview: bool,
    pub issuing_key: Option<Arc<IssuingKey>>,
    pub token_verifier: Option<Arc<WyrdTokenVerifier>>,
    pub trusted_issuer_resolver: Option<Arc<PgIssuerResolver>>,
    pub workload_binding_resolver: Option<Arc<PgWorkloadBindingResolver>>,
    pub sealing_key: Option<Arc<SecretKey>>,
    pub token_exchange_settings: TokenExchangeSettings,
}

impl Default for ServerAuth {
    fn default() -> Self {
        Self {
            allow_preview: false,
            issuing_key: None,
            token_verifier: None,
            trusted_issuer_resolver: None,
            workload_binding_resolver: None,
            sealing_key: None,
            token_exchange_settings: TokenExchangeSettings::default(),
        }
    }
}

/// Authorization handles: policy decision + RBAC evaluation + decision audit.
#[derive(Clone)]
pub struct ServerAuthz {
    pub permission_check: Arc<dyn PermissionCheck>,
    pub policy_hook: Arc<dyn PolicyHook>,
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
