use std::sync::Arc;

use crate::scribe_outbox::ScribeOutbox;
use wyrd_auth_issue::IssuingKey;
use wyrd_crypt::SealingKeyring;
use wyrd_runtime::{PermissionCheck, RbacCheck};

use wyrd_auth::authority::AuthorityCache;
use wyrd_auth::connections::HumanConnections;
use wyrd_auth::issuance::{TenantTokenIssuer, TokenExchangeSettings};
use wyrd_auth::mail::ApplicationMail;
use wyrd_auth::platform_login::PlatformLogin;
use wyrd_auth::social::SocialSignIn;
use wyrd_auth_verify::{ExternalVerifier, TokenVerifier};

use crate::auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};

/// Authentication and authorization: token issuance and verification,
/// issuer/binding resolution, human and platform logins, and the permission
/// check every authorized operation evaluates.
#[derive(Clone)]
pub struct ServerAuth {
    /// Ed25519 key used to sign access and API-key tokens. Required for any token-issuance route.
    pub issuing_key: Option<Arc<IssuingKey>>,
    /// Verifies bearer tokens on every authenticated request. Required in production
    /// (enforced by `AppState::production_validate`).
    pub token_verifier: Option<Arc<TokenVerifier>>,
    /// Verifies workload assertions at RFC 7523 `jwt-bearer` issuance; never
    /// used per request. Human ID tokens verify through the relying party
    /// owned by [`Self::human_connections`] and [`Self::platform_login`].
    pub external_verifier: Option<Arc<ExternalVerifier<PgIssuerResolver>>>,
    /// Resolves a tenant's trusted workload issuers from Postgres for `jwt-bearer`.
    pub trusted_issuer_resolver: Option<Arc<PgIssuerResolver>>,
    /// Resolves workload-to-principal bindings from Postgres for the JWT-bearer exchange path.
    pub workload_binding_resolver: Option<Arc<PgWorkloadBindingResolver>>,
    /// Sealing keyring for provider client secrets at rest: one write key plus
    /// retained keys that still open older ciphertext during rotation.
    /// Required only while provider secrets are stored.
    pub sealing_key: Option<Arc<SealingKeyring>>,
    /// The tenant human-connection owner, built once at boot over the runtime
    /// store, keyring, and screened HTTP. Human login, the callback, and
    /// connection administration all resolve human trust through it; the
    /// callback URL comes from each request's origin.
    pub human_connections: Option<HumanConnections>,
    /// The platform federated-login owner, built once at boot when the
    /// platform plane and a signing key exist. The begin and callback routes
    /// share it, and with it one relying-party provider cache.
    pub platform_login: Option<PlatformLogin>,
    /// The shared social sign-in owner, built once at boot when the platform
    /// plane exists. Operator provider management, the social callbacks and
    /// the login-method listing share it and its provider cache.
    pub social_sign_in: Option<SocialSignIn>,
    /// The deployment's application mail, built once at boot when the
    /// platform plane exists. Operator relay management and invitation
    /// delivery share it; without it nothing is emailed.
    pub application_mail: Option<ApplicationMail>,
    /// TTL and delegation settings for token exchange responses.
    pub token_exchange_settings: TokenExchangeSettings,
    /// RBAC permission evaluator; an embedding deployment may supply its own
    /// at startup.
    pub permission_check: Arc<dyn PermissionCheck>,
    /// Process-local current authority, one snapshot per tenant. Every
    /// request's verified identity resolves its grants here from memory.
    pub authority: Arc<AuthorityCache>,
    /// The deployment's trusted ingress, through which every per-source
    /// admission window derives a request's source.
    pub trusted_ingress: crate::auth::source::TrustedIngress,
}

impl Default for ServerAuth {
    /// No issuance, verification, resolvers, or logins, with RBAC permission
    /// checks; unit-test shells start from this.
    fn default() -> Self {
        Self {
            issuing_key: None,
            token_verifier: None,
            external_verifier: None,
            trusted_issuer_resolver: None,
            workload_binding_resolver: None,
            sealing_key: None,
            human_connections: None,
            platform_login: None,
            social_sign_in: None,
            application_mail: None,
            token_exchange_settings: TokenExchangeSettings::default(),
            permission_check: Arc::new(RbacCheck),
            authority: Arc::default(),
            trusted_ingress: crate::auth::source::TrustedIngress::default(),
        }
    }
}

impl ServerAuth {
    /// Build the one tenant issuance owner over the configured signing key.
    ///
    /// Every tenant grant route mints through this, so all five paths share
    /// the same activity checks, grant resolution, lifetimes, and audit, which
    /// the issuer stages on `audit`, the process audit outbox. Returns `None`
    /// when no signing key is configured.
    #[must_use]
    pub fn tenant_issuer(&self, audit: &Arc<ScribeOutbox>) -> Option<TenantTokenIssuer> {
        self.issuing_key.clone().map(|key| {
            TenantTokenIssuer::new(
                key,
                self.token_exchange_settings.clone(),
                Arc::clone(audit) as _,
            )
        })
    }

    /// The current-authority propagation worker over `postgres`, which runs
    /// until `shutdown`.
    ///
    /// It applies every `wyrd_authorization` notification, so a committed
    /// access change stops a stale snapshot from serving as soon as it is
    /// heard, and confirms each cached tenant's revision every
    /// [`CONFIRM_INTERVAL`](wyrd_auth::authority::CONFIRM_INTERVAL), which
    /// bounds propagation when a notification is missed and is the only
    /// renewal of a snapshot's freshness. Requests never renew it.
    pub(crate) fn refresh_worker(
        &self,
        postgres: &wyrd_sql::WyrdPostgres,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> crate::state::EngineWorker {
        let postgres = postgres.clone();
        let authority = Arc::clone(&self.authority);
        (
            "authorization_cache",
            Box::pin(async move {
                let listener = postgres.listener(wyrd_sql::queries::auth::AUTHORIZATION_CHANNEL);
                let listen = listener.watch(
                    shutdown.clone(),
                    |_| {},
                    |payload| authority.notify(payload),
                );
                let confirm = async {
                    let mut ticks = tokio::time::interval(wyrd_auth::authority::CONFIRM_INTERVAL);
                    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        ticks.tick().await;
                        authority.confirm(&postgres).await;
                    }
                };
                tokio::select! {
                    () = shutdown.cancelled() => {}
                    () = listen => {}
                    () = confirm => {}
                }
            }),
        )
    }

    /// The auth maintenance worker, which runs until `shutdown`.
    ///
    /// Every [`MAINTENANCE_INTERVAL`] it drains expired signup state one
    /// bounded batch at a time, so no public request ever purges. `None`
    /// without the platform plane, where there is no signup state.
    pub(crate) fn maintenance_worker(
        &self,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Option<crate::state::EngineWorker> {
        let social = self.social_sign_in.clone()?;
        Some((
            "auth_maintenance",
            Box::pin(async move {
                let drain = async {
                    let mut ticks = tokio::time::interval(MAINTENANCE_INTERVAL);
                    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        ticks.tick().await;
                        loop {
                            match social.purge_expired_signups().await {
                                Ok(0) => break,
                                Ok(_) => {}
                                Err(error) => {
                                    tracing::warn!(%error, "expired signup state purge failed");
                                    break;
                                }
                            }
                        }
                    }
                };
                tokio::select! {
                    () = shutdown.cancelled() => {}
                    () = drain => {}
                }
            }),
        ))
    }
}

/// How often auth maintenance purges expired state.
const MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_mins(1);
