use std::sync::Arc;

use wyrd_auth_issue::IssuingKey;
use wyrd_crypt::SealingKeyring;
use wyrd_runtime::{PermissionCheck, RbacCheck};

use wyrd_auth::connections::HumanConnections;
use wyrd_auth::issuance::{TenantTokenIssuer, TokenExchangeSettings};
use wyrd_auth_verify::{ExternalVerifier, TokenVerifier};

use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};

/// Authentication handles: token issuance + verification + issuer/binding resolution.
#[derive(Clone, Default)]
pub struct ServerAuth {
    /// Ed25519 key used to sign access and API-key tokens. Required for any token-issuance route.
    pub issuing_key: Option<Arc<IssuingKey>>,
    /// Verifies bearer tokens on every authenticated request. Required in production
    /// (enforced by `AppState::production_validate`).
    pub token_verifier: Option<Arc<TokenVerifier>>,
    /// Verifies foreign OIDC ID tokens and workload assertions at issuance
    /// (login callback, platform login, `jwt-bearer`); never used per request.
    pub external_verifier: Option<Arc<ExternalVerifier<PgIssuerResolver>>>,
    /// Resolves trusted OIDC issuers from Postgres for foreign-OIDC verification paths.
    pub trusted_issuer_resolver: Option<Arc<PgIssuerResolver>>,
    /// Resolves workload-to-principal bindings from Postgres for the JWT-bearer exchange path.
    pub workload_binding_resolver: Option<Arc<PgWorkloadBindingResolver>>,
    /// Sealing keyring for provider client secrets at rest: one write key plus
    /// retained keys that still open older ciphertext during rotation.
    /// Required only while provider secrets are stored.
    pub sealing_key: Option<Arc<SealingKeyring>>,
    /// The tenant human-connection owner, built once at boot over the runtime
    /// store, keyring, screened HTTP, and deployment public origin. Human
    /// login, the callback, and connection administration all resolve human
    /// trust — and the callback URL, never request headers — through it.
    pub human_connections: Option<HumanConnections>,
    /// The production UI browser-session channel; `None` leaves
    /// `/internal/bff/v1/*` unmounted.
    pub bff: Option<crate::components::auth::bff::BffChannel>,
    /// TTL and delegation settings for token exchange responses.
    pub token_exchange_settings: TokenExchangeSettings,
}

impl ServerAuth {
    /// Build the one tenant issuance owner over the configured signing key.
    ///
    /// Every tenant grant route mints through this, so all five paths share
    /// the same activity checks, grant resolution, lifetimes, and audit.
    /// Returns `None` when no signing key is configured.
    #[must_use]
    pub fn tenant_issuer(&self) -> Option<TenantTokenIssuer> {
        self.issuing_key
            .clone()
            .map(|key| TenantTokenIssuer::new(key, self.token_exchange_settings.clone()))
    }
}

/// Authorization handles: RBAC evaluation.
#[derive(Clone)]
pub struct ServerAuthz {
    /// RBAC permission evaluator. Defaults to `RbacCheck`.
    pub permission_check: Arc<dyn PermissionCheck>,
}

impl Default for ServerAuthz {
    fn default() -> Self {
        Self {
            permission_check: Arc::new(RbacCheck),
        }
    }
}
