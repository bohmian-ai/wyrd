//! Operator-managed shared social sign-in.
//!
//! [`SocialSignIn`] is the one owner of the deployment's shared Google and
//! GitHub.com clients and its login settings. Platform administration
//! authorizes each operation and hands the audited operator transaction in;
//! this owner validates, seals and stores. A stored client is a new, untested,
//! disabled revision. [`SocialSignIn::begin_test`] starts a real provider
//! sign-in bound to that exact revision and its tester, the provider-specific
//! social callback finishes it through [`SocialSignIn::complete_test`], and
//! only then may the revision be enabled. Rust's login-method listing offers
//! only enabled providers.
//!
//! Google is an OpenID Connect provider at a fixed issuer, verified by the
//! same relying party company SSO uses. Provider and key-set metadata are
//! cached process-wide by that relying party, never rebuilt per request.
//! GitHub.com is an OAuth provider: [`GithubOAuth`] redeems the code and reads
//! the authenticated user's numeric ID, recorded under the fixed
//! [`GITHUB_NAMESPACE`].

use std::sync::Arc;

use base64::Engine as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use url::Url;
use uuid::Uuid;
pub use wyrd_auth_oidc::GithubEndpoints;
use wyrd_auth_oidc::turnstile::{TURNSTILE_SITEVERIFY, Turnstile};
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, CodeRedemption, GITHUB_NAMESPACE, GithubOAuth,
    MappedClaims, RelyingParty, ScreenedHttp,
};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, CallbackQuery, ConfigureSocialProviderRequest, CreationMethods, EmailMethods,
    EnableSocialProviderRequest, IssuerUrl, LocalCredentials, LoginMethodsResponse, LoginSettings,
    ProviderResponse, SecretBearer, Sha256Hex, SignInMethods, SignupChallenge, SocialProvider,
    SocialProviderState, SocialProviderView, SocialProvidersResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{
    BrowserSessionAuthority, SocialAnchor, SocialLoginState, consume_social_login_state,
    insert_social_login_state, insert_user, insert_user_identity, linked_identity,
    local_credential_for_update,
};
use wyrd_sql::queries::platform::signup::{
    SignupAdmissionScope, SignupProof, admit_signup, attach_signup_contact, finish_signup_creation,
    insert_proven_signup, proven_signup, purge_expired_signup_state, renew_signup_completion,
    signup_clock, start_signup_creation,
};
use wyrd_sql::queries::platform::social::{
    LoginSettingsRow, ProviderEnablement, SocialProviderRow, SocialTestState,
    insert_social_test_state, login_settings, mail_relay, put_login_settings, put_social_provider,
    set_social_provider_enabled, signup_challenge, social_provider, social_providers,
    stamp_social_provider_tested, take_social_test_state,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

use crate::browser_sessions::{
    BegunBrowserLogin, BrowserSessions, CreatedBrowserSession, create_session, random_secret,
};
use crate::callback::{
    LoginCompletion, audit_authorization_code_failure, provider_refusal, verify_response_issuer,
};
use crate::error::{relying_party_error, store_error};
use crate::issuance::TenantGrant;
use crate::login::{LOGIN_STATE_TTL, login_unavailable};
use crate::passwords::{canonical_username, code_hash, hash_password, new_password};

/// Google's OpenID Connect issuer.
pub const GOOGLE_ISSUER: &str = "https://accounts.google.com";

/// The provider endpoints this deployment talks to.
///
/// Production always uses the real Google and GitHub.com endpoints;
/// development and tests name controlled local providers.
#[derive(Debug, Clone)]
pub struct SocialEndpoints {
    /// Google's OIDC issuer.
    pub google_issuer: IssuerUrl,
    /// GitHub's OAuth and API endpoints.
    pub github: GithubEndpoints,
    /// Cloudflare Turnstile's Siteverify endpoint.
    pub turnstile_siteverify: Url,
}

impl Default for SocialEndpoints {
    fn default() -> Self {
        Self {
            google_issuer: IssuerUrl::new(GOOGLE_ISSUER)
                .expect("invariant: Google's issuer is a valid issuer URL"),
            github: GithubEndpoints::default(),
            turnstile_siteverify: Url::parse(TURNSTILE_SITEVERIFY)
                .expect("invariant: Cloudflare's Siteverify URL parses"),
        }
    }
}

/// The deployment's shared social sign-in owner.
///
/// Cheap to clone; clones share the relying party's provider cache.
#[derive(Clone)]
pub struct SocialSignIn {
    /// Operator pool; `platform.*` admits only operator sessions.
    operator: OperatorPool,
    /// Sealing keyring for client secrets; `None` refuses configuration.
    keyring: Option<Arc<SealingKeyring>>,
    /// Screened relying party for OIDC providers.
    relying_party: RelyingParty,
    /// Screened GitHub.com OAuth client.
    github: GithubOAuth,
    /// Screened transport for Siteverify.
    http: ScreenedHttp,
    /// Provider endpoints.
    endpoints: SocialEndpoints,
}

impl std::fmt::Debug for SocialSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocialSignIn")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

/// The current revision of an enabled provider, with its client credentials
/// opened for one provider exchange.
#[derive(Debug, Clone)]
pub struct EnabledProvider {
    /// Which provider.
    pub provider: SocialProvider,
    /// The revision a login binds and its credentials anchor to.
    pub revision: i64,
    /// OAuth client id.
    pub client_id: String,
    /// Client secret, opened from its sealed envelope.
    pub client_secret: SecretString,
}

impl SocialSignIn {
    /// Build the owner over the operator pool, keyring, screened HTTP and
    /// provider endpoints.
    #[must_use]
    pub fn new(
        operator: OperatorPool,
        keyring: Option<Arc<SealingKeyring>>,
        http: ScreenedHttp,
        endpoints: SocialEndpoints,
    ) -> Self {
        Self {
            operator,
            keyring,
            relying_party: RelyingParty::new(http),
            github: GithubOAuth::new(http, endpoints.github.clone()),
            http,
            endpoints,
        }
    }

    /// Every shared provider as management reads it, with callback URLs on
    /// `origin` and no secrets.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails and
    /// [`WyrdError::Validation`] when `origin` cannot form a callback.
    pub async fn views(&self, origin: &Url) -> Result<SocialProvidersResponse, WyrdError> {
        let rows = social_providers(&self.operator)
            .await
            .map_err(store_error)?;
        let providers = SocialProvider::ALL
            .into_iter()
            .map(|provider| {
                let row = rows.iter().find(|row| row.provider == provider.as_str());
                view(provider, row, origin)
            })
            .collect::<Result<_, _>>()?;
        Ok(SocialProvidersResponse { providers })
    }

    /// Seal and store `provider`'s client on the audited operator `conn` as
    /// a new untested, disabled revision.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an empty client id or secret or a
    /// deployment without a sealing keyring, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the write fails.
    pub async fn configure(
        &self,
        conn: &mut TenantConn<'_>,
        provider: SocialProvider,
        request: &ConfigureSocialProviderRequest,
        origin: &Url,
    ) -> Result<SocialProviderView, WyrdError> {
        let client_id = request.client_id.trim();
        if client_id.is_empty() || request.client_secret.expose().trim().is_empty() {
            return Err(validation(
                "client_id and client_secret are required",
                "client_id",
            ));
        }
        let keyring = self.keyring.as_ref().ok_or_else(|| {
            validation(
                "this deployment has no sealing key, so provider secrets cannot be stored",
                "client_secret",
            )
        })?;
        let sealed = keyring
            .seal(request.client_secret.expose().as_bytes())
            .map_err(|_| validation("the client secret could not be sealed", "client_secret"))?;
        let row = put_social_provider(conn, provider.as_str(), client_id, &sealed)
            .await
            .map_err(store_error)?;
        view(provider, Some(&row), origin)
    }

    /// Enable or disable exactly the named revision of `provider` on the
    /// audited operator `conn`.
    ///
    /// # Errors
    /// Returns [`WyrdError::NotFound`] for an unconfigured provider and
    /// [`WyrdError::Conflict`] for a revision that is not current or, when
    /// enabling, not tested.
    pub async fn set_enabled(
        &self,
        conn: &mut TenantConn<'_>,
        provider: SocialProvider,
        request: EnableSocialProviderRequest,
        origin: &Url,
    ) -> Result<SocialProviderView, WyrdError> {
        let revision = i64::try_from(request.expected_revision).unwrap_or(i64::MAX);
        match set_social_provider_enabled(conn, provider.as_str(), revision, request.enabled)
            .await
            .map_err(store_error)?
        {
            ProviderEnablement::Changed(row) => view(provider, Some(&row), origin),
            ProviderEnablement::Unconfigured => Err(unconfigured(provider)),
            ProviderEnablement::StaleRevision => Err(conflict(
                "the provider was reconfigured; reread it and retry",
                "revision_stale",
            )),
            ProviderEnablement::Untested => Err(conflict(
                "test this provider revision with a real sign-in before enabling it",
                "provider_untested",
            )),
        }
    }

    /// Begin a real provider sign-in testing exactly `expected_revision` of
    /// `provider`, for the platform principal `tester`, and return the
    /// provider authorization URL.
    ///
    /// # Errors
    /// Returns [`WyrdError::NotFound`] for an unconfigured provider,
    /// [`WyrdError::Conflict`] for a revision that is not current,
    /// [`WyrdError::DiscoveryUnavailable`] when the provider cannot be
    /// discovered, and [`WyrdError::AuthVerifyUnavailable`] when the store
    /// fails.
    pub async fn begin_test(
        &self,
        provider: SocialProvider,
        expected_revision: u64,
        tester: Uuid,
        origin: &Url,
    ) -> Result<AbsoluteUrl, WyrdError> {
        let row = social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .ok_or_else(|| unconfigured(provider))?;
        if u64::try_from(row.revision).ok() != Some(expected_revision) {
            return Err(conflict(
                "the provider was reconfigured; reread it and retry",
                "revision_stale",
            ));
        }
        let authorization = self
            .authorize(provider, &row.client_id, origin, false)
            .await?;
        insert_social_test_state(
            &self.operator,
            Sha256Hex::digest(authorization.state.as_bytes()).as_bytes(),
            &SocialTestState {
                provider: provider.as_str().to_owned(),
                provider_revision: row.revision,
                redirect_uri: authorization.redirect_uri.to_string(),
                code_verifier: authorization.code_verifier.expose_secret().to_owned(),
                nonce: Some(authorization.nonce),
                tester_principal_id: tester,
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(store_error)?;
        Ok(authorization.url)
    }

    /// Build `provider`'s authorization redirect for `client_id`, returning to
    /// its social callback on `origin`, with fresh state, nonce and PKCE.
    /// Workspace `acquisition` alone asks GitHub for `user:email`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `origin` cannot form a callback,
    /// [`WyrdError::DiscoveryUnavailable`] when the provider cannot be
    /// discovered, and [`WyrdError::NotFound`] for a provider without an
    /// adapter.
    async fn authorize(
        &self,
        provider: SocialProvider,
        client_id: &str,
        origin: &Url,
        acquisition: bool,
    ) -> Result<SocialAuthorization, WyrdError> {
        let redirect_uri = social_callback(provider, origin)?;
        match provider {
            SocialProvider::Google => {
                let metadata = self
                    .relying_party
                    .discover(&self.endpoints.google_issuer)
                    .await
                    .map_err(relying_party_error)?;
                let authorization = self
                    .relying_party
                    .authorize(&metadata, client_id, redirect_uri.as_str())
                    .map_err(relying_party_error)?;
                Ok(SocialAuthorization {
                    url: AbsoluteUrl::new(authorization.url.as_str().to_owned())
                        .map_err(|_| invalid_state("the provider authorization URL is invalid"))?,
                    state: authorization.state,
                    code_verifier: authorization.code_verifier,
                    nonce: authorization.nonce,
                    redirect_uri,
                })
            }
            SocialProvider::Github => {
                let authorization = self
                    .github
                    .authorize(client_id, redirect_uri.as_str(), acquisition)
                    .map_err(relying_party_error)?;
                Ok(SocialAuthorization {
                    url: AbsoluteUrl::new(authorization.url.as_str().to_owned())
                        .map_err(|_| invalid_state("the provider authorization URL is invalid"))?,
                    state: authorization.state,
                    code_verifier: authorization.code_verifier,
                    nonce: authorization.nonce,
                    redirect_uri,
                })
            }
        }
    }

    /// The issuer namespace `provider`'s identities are recorded under.
    fn issuer(&self, provider: SocialProvider) -> &str {
        match provider {
            SocialProvider::Google => self.endpoints.google_issuer.as_str(),
            SocialProvider::Github => GITHUB_NAMESPACE,
        }
    }

    /// Verify a tenant sign-in's provider return against the exact revision
    /// its transaction bound, which must still be current and enabled.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the provider was disabled or
    /// replaced meanwhile or the return fails verification,
    /// [`WyrdError::InvalidNonce`] for a wrong nonce, and the provider and
    /// store availability errors.
    async fn verify_login(
        &self,
        provider: SocialProvider,
        state: &SocialLoginState,
        code: SecretString,
        response_issuer: Option<&str>,
    ) -> Result<MappedClaims, WyrdError> {
        let row = social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.revision == state.provider_revision && row.enabled)
            .ok_or_else(revision_changed)?;
        let enabled = self.open(provider, row)?;
        self.verify_code(
            &enabled,
            ProviderReturn {
                code,
                redirect_uri: &state.redirect_uri,
                code_verifier: &state.code_verifier,
                nonce: Some(&state.nonce),
                response_issuer,
            },
            false,
        )
        .await
    }

    /// Finish an operator provider test for `provider` whose provider return
    /// carried `response`, `state_key` and optional RFC 9207 `iss`.
    ///
    /// Returns `Ok(false)` when `state_key` names no pending test, so the
    /// callback may try the tenant login states instead. The test state is
    /// consumed before any provider IO; the code is redeemed with the exact
    /// tested revision's client and the ID token fully verified; and only that
    /// revision is marked tested.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the provider refused, the
    /// revision changed meanwhile, or verification fails, and the store and
    /// provider availability errors.
    pub async fn complete_test(
        &self,
        provider: SocialProvider,
        response: ProviderResponse,
        state_key: &str,
        response_issuer: Option<&str>,
    ) -> Result<bool, WyrdError> {
        let state_hash = Sha256Hex::digest(state_key.as_bytes());
        let Some(state) =
            take_social_test_state(&self.operator, provider.as_str(), state_hash.as_bytes())
                .await
                .map_err(store_error)?
        else {
            return Ok(false);
        };
        let code = match response {
            ProviderResponse::Code(code) => code,
            ProviderResponse::Error(error) => {
                tracing::info!(%provider, error, "provider refused the test sign-in");
                return Err(invalid_token("the provider refused the sign-in"));
            }
        };
        let enabled = self.revision(provider, state.provider_revision).await?;
        self.verify_code(
            &enabled,
            ProviderReturn {
                code: code.into_secret_string(),
                redirect_uri: &state.redirect_uri,
                code_verifier: &SecretString::from(state.code_verifier),
                nonce: state.nonce.as_deref(),
                response_issuer,
            },
            false,
        )
        .await?;
        if !stamp_social_provider_tested(&self.operator, provider.as_str(), state.provider_revision)
            .await
            .map_err(store_error)?
        {
            return Err(revision_changed());
        }
        Ok(true)
    }

    /// Redeem `code` at `provider`'s token endpoint and verify the identity
    /// it returns. For workspace `acquisition` the identity's email is its
    /// provider-verified address only: Google's under a validated
    /// `email_verified`, GitHub's from its email API; `None` without one.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] or [`WyrdError::InvalidNonce`] when
    /// the provider refuses or verification fails, and the provider
    /// availability errors.
    async fn verify_code(
        &self,
        provider: &EnabledProvider,
        returned: ProviderReturn<'_>,
        acquisition: bool,
    ) -> Result<MappedClaims, WyrdError> {
        let ProviderReturn {
            code,
            redirect_uri,
            code_verifier,
            nonce,
            response_issuer,
        } = returned;
        match provider.provider {
            SocialProvider::Google => {
                let issuer = &self.endpoints.google_issuer;
                let metadata = self
                    .relying_party
                    .cached(issuer)
                    .await
                    .map_err(relying_party_error)?;
                verify_response_issuer(
                    response_issuer,
                    issuer.as_str(),
                    metadata
                        .additional_metadata()
                        .authorization_response_iss_parameter_supported,
                )?;
                let client_auth = ClientAuth::SecretPost(provider.client_secret.clone());
                let verified = self
                    .relying_party
                    .redeem(
                        issuer,
                        metadata,
                        code,
                        &CodeRedemption {
                            client_id: &provider.client_id,
                            client_auth: &client_auth,
                            claim_mapping: &oidc_claims(),
                            redirect_uri,
                            code_verifier,
                            nonce: nonce.unwrap_or_default(),
                        },
                    )
                    .await
                    .map_err(relying_party_error)?;
                let mut identity = verified.identity;
                if acquisition && verified.claims["email_verified"] != json!(true) {
                    identity.email = None;
                }
                Ok(identity)
            }
            SocialProvider::Github => self
                .github
                .identity(
                    &provider.client_id,
                    &provider.client_secret,
                    &code,
                    redirect_uri,
                    code_verifier,
                    acquisition,
                )
                .await
                .map_err(relying_party_error),
        }
    }

    /// The stored row of `provider` at exactly `revision`, with its secret
    /// opened.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the provider is gone or at
    /// another revision, and [`WyrdError::Internal`] when the secret does not
    /// open.
    async fn revision(
        &self,
        provider: SocialProvider,
        revision: i64,
    ) -> Result<EnabledProvider, WyrdError> {
        let row = social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.revision == revision)
            .ok_or_else(revision_changed)?;
        self.open(provider, row)
    }

    /// Open `row`'s sealed secret.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] when no held key opens it.
    fn open(
        &self,
        provider: SocialProvider,
        row: SocialProviderRow,
    ) -> Result<EnabledProvider, WyrdError> {
        let secret = self
            .keyring
            .as_ref()
            .and_then(|keyring| keyring.open(&row.client_secret_enc).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| WyrdError::Internal {
                message: "the provider client secret could not be opened".to_owned(),
                details: json!({ "provider": provider.as_str() }),
            })?;
        Ok(EnabledProvider {
            provider,
            revision: row.revision,
            client_id: row.client_id,
            client_secret: SecretString::from(secret),
        })
    }

    /// The deployment login settings.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the read fails.
    pub async fn settings(&self) -> Result<LoginSettings, WyrdError> {
        let row = login_settings(&self.operator).await.map_err(store_error)?;
        Ok(LoginSettings {
            password_enabled: row.password_enabled,
            signup_enabled: row.signup_enabled,
            signup_workspace_cap: u16::try_from(row.signup_workspace_cap)
                .ok()
                .and_then(std::num::NonZeroU16::new)
                .ok_or_else(|| WyrdError::AuthVerifyUnavailable {
                    message: "the stored workspace cap is out of range".to_owned(),
                    details: json!({}),
                })?,
            signup_challenge: row
                .signup_challenge_site_key
                .map(|site_key| SignupChallenge {
                    site_key,
                    secret: None,
                }),
        })
    }

    /// Replace the deployment login settings on the audited operator `conn`.
    ///
    /// A signup challenge's secret is sealed under the deployment keyring;
    /// a challenge sent without one keeps the stored secret.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a challenge with no secret
    /// stored or sent, an empty site key or secret, or a deployment without
    /// a sealing key, and [`WyrdError::AuthVerifyUnavailable`] when the write
    /// fails.
    pub async fn put_settings(
        &self,
        conn: &mut TenantConn<'_>,
        settings: &LoginSettings,
    ) -> Result<(), WyrdError> {
        let challenge = settings.signup_challenge.as_ref();
        if challenge.is_some_and(|challenge| {
            challenge.site_key.trim().is_empty()
                || challenge
                    .secret
                    .as_ref()
                    .is_some_and(|secret| secret.expose().is_empty())
        }) {
            return Err(validation(
                "a signup challenge needs a site key and secret",
                "signup_challenge",
            ));
        }
        let sealed = match challenge.and_then(|challenge| challenge.secret.as_ref()) {
            Some(secret) => Some(
                self.keyring
                    .as_ref()
                    .ok_or_else(|| {
                        validation(
                            "this deployment has no sealing key, so a challenge secret cannot be stored",
                            "signup_challenge.secret",
                        )
                    })?
                    .seal(secret.expose().as_bytes())
                    .map_err(|_| {
                        validation(
                            "the challenge secret could not be sealed",
                            "signup_challenge.secret",
                        )
                    })?,
            ),
            None => None,
        };
        put_login_settings(
            conn,
            &LoginSettingsRow {
                password_enabled: settings.password_enabled,
                signup_enabled: settings.signup_enabled,
                signup_workspace_cap: i32::from(settings.signup_workspace_cap.get()),
                signup_challenge_site_key: challenge.map(|challenge| challenge.site_key.clone()),
            },
            sealed.as_deref(),
        )
        .await
        .map_err(|error| match error {
            SqlError::CheckViolation { .. } => validation(
                "a signup challenge needs its secret the first time it is set",
                "signup_challenge.secret",
            ),
            error => store_error(error),
        })
    }

    /// Require a browser workspace creation from `source` to pass the
    /// configured signup challenge with `token`, solved on `hostname` for
    /// the `signup` action; nothing is required when no challenge is
    /// configured.
    ///
    /// # Errors
    /// Returns [`WyrdError::SignupChallenge`] for a missing token or one
    /// Siteverify refuses, reports already used, or reports solved on
    /// another hostname or for another action, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the challenge settings,
    /// keyring or Siteverify are unavailable.
    pub async fn require_challenge(
        &self,
        token: Option<&str>,
        hostname: &str,
        source: Option<std::net::IpAddr>,
    ) -> Result<(), WyrdError> {
        let Some((_, sealed)) = signup_challenge(&self.operator)
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let refused = || WyrdError::SignupChallenge {
            message: "complete the human check on the signup page, then try again".to_owned(),
            details: json!({ "field": "challenge_token" }),
        };
        let token = token
            .filter(|token| !token.is_empty())
            .ok_or_else(refused)?;
        let unavailable = || WyrdError::AuthVerifyUnavailable {
            message: "the signup challenge cannot be checked right now; try again".to_owned(),
            details: json!({ "reason": "challenge_unavailable" }),
        };
        let secret = self
            .keyring
            .as_ref()
            .and_then(|keyring| keyring.open(&sealed).ok())
            .and_then(|plain| String::from_utf8(plain).ok())
            .map(SecretString::from)
            .ok_or_else(unavailable)?;
        let answer = Turnstile::new(self.http, self.endpoints.turnstile_siteverify.clone())
            .verify(&secret, token, source)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "signup challenge unavailable");
                unavailable()
            })?;
        if answer.success
            && answer.hostname.as_deref() == Some(hostname)
            && answer.action.as_deref() == Some("signup")
        {
            Ok(())
        } else {
            tracing::info!(codes = ?answer.error_codes, "signup challenge refused");
            Err(refused())
        }
    }

    /// The sign-in methods Rust currently allows: the deployment's settings,
    /// enabled providers and mail relay, narrowed by `tenant` when a tenant's
    /// sign-in page asks. Every login, enrollment, reset and creation path
    /// refuses a method this report does not offer.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when a read fails.
    pub async fn login_methods(
        &self,
        tenant: Option<TenantLogin>,
    ) -> Result<LoginMethodsResponse, WyrdError> {
        let settings = self.settings().await?;
        let rows = social_providers(&self.operator)
            .await
            .map_err(store_error)?;
        let enabled: Vec<SocialProvider> = SocialProvider::ALL
            .into_iter()
            .filter(|provider| {
                rows.iter()
                    .any(|row| row.provider == provider.as_str() && row.enabled)
            })
            .collect();
        let mail = mail_relay(&self.operator)
            .await
            .map_err(store_error)?
            .is_some();
        Ok(available_methods(settings, enabled, mail, tenant))
    }
}

/// The fixed social callback of `provider` on `origin`.
///
/// # Errors
/// Returns [`WyrdError::Validation`] when `origin` cannot form a URL.
pub fn social_callback(provider: SocialProvider, origin: &Url) -> Result<Url, WyrdError> {
    origin
        .join(provider.callback_path())
        .map_err(|_| validation("the request origin cannot form a callback", "origin"))
}

/// Project `provider`'s stored `row` (if any) for management.
fn view(
    provider: SocialProvider,
    row: Option<&SocialProviderRow>,
    origin: &Url,
) -> Result<SocialProviderView, WyrdError> {
    let callback = social_callback(provider, origin)?;
    let callback_url = AbsoluteUrl::new(callback.to_string())
        .map_err(|_| validation("the request origin cannot form a callback", "origin"))?;
    Ok(SocialProviderView {
        provider,
        client_id: row.map(|row| row.client_id.clone()),
        revision: row.and_then(|row| u64::try_from(row.revision).ok()),
        state: row.map(|row| match (row.enabled, row.tested) {
            (true, _) => SocialProviderState::Enabled,
            (false, true) => SocialProviderState::Tested,
            (false, false) => SocialProviderState::Untested,
        }),
        registration_instructions: instructions(provider, &callback),
        callback_url,
    })
}

/// Provider-console registration steps for `provider`.
fn instructions(provider: SocialProvider, callback: &Url) -> String {
    match provider {
        SocialProvider::Google => format!(
            "In Google Cloud Console, open APIs & Services > Credentials, create an \
             OAuth client ID of type Web application, and add {callback} as an authorized \
             redirect URI. Configure the OAuth consent screen with the openid, email and \
             profile scopes only. Paste the client ID and client secret here, then test."
        ),
        SocialProvider::Github => format!(
            "In GitHub, open Settings > Developer settings > OAuth Apps, register a new \
             OAuth App (not a GitHub App) and set its authorization callback URL to \
             {callback}. Generate a client secret, paste the client ID and secret here, \
             then test. Wyrd requests no scope, so it reads only the public profile."
        ),
    }
}

impl BrowserSessions {
    /// Begin a sign-in for `tenant_route_key` through the enabled shared
    /// `provider`, from the browser that will hold the returned
    /// login-transaction binding: a returning sign-in, or with `invitation`
    /// the enrollment that redeems that invitation link's code. Beginning
    /// spends no invitation.
    ///
    /// The tenant transaction binds the purpose, the invitation code's
    /// SHA-256 when there is one, the exact provider revision
    /// and client, the provider's social callback on `origin`, server-held
    /// PKCE and nonce, and only the SHA-256 of the browser binding and state.
    /// Nothing is audited: initiation evaluates no principal permission.
    ///
    /// # Errors
    /// Returns [`WyrdError::OneTimeCodeInvalid`] for a malformed invitation
    /// code, [`WyrdError::InvalidToken`] for an unknown tenant or a provider
    /// that is not enabled, [`WyrdError::InvalidState`] for a reused state,
    /// and the provider and store availability errors.
    pub async fn begin_social(
        &self,
        social: &SocialSignIn,
        tenant_route_key: &TenantSlug,
        provider: SocialProvider,
        invitation: Option<&SecretBearer>,
        origin: &Url,
    ) -> Result<BegunBrowserLogin, WyrdError> {
        let invitation = invitation.map(code_hash).transpose()?;
        let postgres = self.connections.postgres();
        let tenant = postgres
            .resolve_tenant_slug(tenant_route_key)
            .await
            .map_err(store_error)?
            .ok_or_else(login_unavailable)?;
        let row = social_provider(&social.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.enabled)
            .ok_or_else(|| invalid_token(&format!("{provider} sign-in is not available")))?;
        let authorization = social
            .authorize(provider, &row.client_id, origin, false)
            .await?;
        let binding = SecretString::from(random_secret());
        let state = SocialLoginState {
            provider_revision: row.revision,
            issuer: social.issuer(provider).to_owned(),
            client_id: row.client_id,
            redirect_uri: authorization.redirect_uri.to_string(),
            code_verifier: authorization.code_verifier,
            nonce: authorization.nonce,
            browser_binding: Sha256Hex::digest(binding.expose_secret().as_bytes()),
            invitation,
        };
        let mut conn = postgres.tenant_conn(tenant).await.map_err(store_error)?;
        if !insert_social_login_state(
            &mut conn,
            &Sha256Hex::digest(authorization.state.as_bytes()),
            provider.as_str(),
            &state,
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(store_error)?
        {
            return Err(invalid_state(
                "this sign-in state was already used; start again",
            ));
        }
        conn.commit().await.map_err(store_error)?;
        Ok(BegunBrowserLogin {
            authorization_url: authorization.url,
            binding,
        })
    }

    /// Finish a tenant sign-in `provider` returned to its social callback with
    /// `query` (its code or error, state and optional RFC 9207 `iss`) and the
    /// binding the calling browser presented.
    ///
    /// Returns `Ok(None)` when `state_key` names no pending tenant sign-in.
    /// Otherwise the transaction is consumed and committed before any provider
    /// IO, the browser binding must match, the bound revision must still be
    /// current and enabled, and the provider identity is verified. A
    /// returning sign-in admits only an identity already linked to a tenant
    /// User under (issuer, subject); an email never selects a User, and a
    /// stranger is refused with account guidance. An enrollment refuses an
    /// identity that already has a User with sign-in guidance; otherwise one
    /// tenant transaction creates the User, redeems the bound invitation,
    /// links the identity and creates the session, so a refusal leaves none
    /// of them. Every refusal is audited without blocking and returned as
    /// [`LoginCompletion::Refused`]; a sign-in is audited as a login.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the tenant lookup
    /// fails.
    pub async fn complete_social(
        &self,
        social: &SocialSignIn,
        provider: SocialProvider,
        query: &CallbackQuery,
        browser_binding: Option<&str>,
        request_id: &str,
    ) -> Result<Option<LoginCompletion>, WyrdError> {
        let state_hash = Sha256Hex::digest(query.state.as_bytes());
        let Some(tenant_id) = self
            .connections
            .postgres()
            .login_state_tenant(&state_hash)
            .await
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        let error = match self
            .finish_social(
                social,
                tenant_id,
                provider,
                &state_hash,
                query,
                browser_binding,
            )
            .await
        {
            Ok(created) => {
                self.stage_login(&created.session, request_id);
                return Ok(Some(LoginCompletion::SignedIn(created)));
            }
            Err(error) => error,
        };
        audit_authorization_code_failure(self.issuer.audit(), tenant_id, request_id, &error);
        Ok(Some(LoginCompletion::Refused { tenant_id, error }))
    }

    /// The tenant part of [`Self::complete_social`]: the committed session.
    ///
    /// # Errors
    /// Returns every refusal [`Self::complete_social`] reports as
    /// [`LoginCompletion::Refused`].
    async fn finish_social(
        &self,
        social: &SocialSignIn,
        tenant_id: DataTenantId,
        provider: SocialProvider,
        state_hash: &Sha256Hex,
        query: &CallbackQuery,
        browser_binding: Option<&str>,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let response_issuer = query.iss.as_deref();
        let postgres = self.connections.postgres();
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let state = consume_social_login_state(&mut conn, state_hash, provider.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let state = state.ok_or_else(|| {
            invalid_state("this sign-in has expired or was already used; start again")
        })?;
        if !browser_binding
            .is_some_and(|binding| Sha256Hex::digest(binding.as_bytes()) == state.browser_binding)
        {
            tracing::info!(%provider, "social callback reached a browser that did not begin it");
            return Err(invalid_state(
                "this browser did not begin the sign-in; start a new sign-in",
            ));
        }
        let code = match query.response()? {
            ProviderResponse::Code(code) => code.into_secret_string(),
            ProviderResponse::Error(error) => {
                verify_response_issuer(response_issuer, &state.issuer, false)?;
                return Err(provider_refusal(&error));
            }
        };
        let identity = social
            .verify_login(provider, &state, code, response_issuer)
            .await?;
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let linked = linked_identity(&mut conn, &state.issuer, &identity.subject)
            .await
            .map_err(store_error)?;
        let (user_id, identity_id) = match (state.invitation, linked) {
            (None, Some(linked)) => (linked.user_id, linked.id),
            (None, None) => {
                return Err(WyrdError::InvalidToken {
                    message: format!(
                        "no account in this workspace uses this {provider} sign-in; \
                         accept an invitation or create a workspace"
                    ),
                    details: json!({ "reason": "no_account" }),
                });
            }
            (Some(_), Some(_)) => return Err(account_exists(provider)),
            (Some(code), None) => {
                let user_id = Uuid::now_v7();
                insert_user(&mut conn, user_id, identity.email.as_deref(), "oidc", None)
                    .await
                    .map_err(store_error)?;
                self.redeem_invitation(&mut conn, &code, user_id).await?;
                let identity_id =
                    insert_user_identity(&mut conn, &state.issuer, &identity.subject, user_id)
                        .await
                        .map_err(|error| match &error {
                            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
                                account_exists(provider)
                            }
                            _ => store_error(error),
                        })?;
                (user_id, identity_id)
            }
        };
        let verified = self
            .issuer
            .verify(&mut conn, user_id, TenantGrant::OidcLogin)
            .await?;
        let created = create_session(
            &mut conn,
            verified,
            BrowserSessionAuthority::Social(SocialAnchor {
                provider,
                provider_revision: state.provider_revision,
                identity_id,
            }),
        )
        .await?;
        conn.commit().await.map_err(store_error)?;
        Ok(created)
    }
}

/// A provider redirect issued for one stored client.
struct SocialAuthorization {
    /// The provider authorization URL.
    url: AbsoluteUrl,
    /// Raw state; only its SHA-256 is stored.
    state: String,
    /// Server-held PKCE verifier.
    code_verifier: SecretString,
    /// Server-held nonce.
    nonce: String,
    /// The provider's social callback.
    redirect_uri: Url,
}

/// What a tenant contributes to its sign-in methods.
#[derive(Debug, Clone, Copy, Default)]
pub struct TenantLogin {
    /// The tenant has an Active company SSO connection.
    pub company_sso: bool,
    /// The tenant requires company SSO, which leaves no password or social
    /// fallback even while its connection is unavailable.
    pub requires_company_sso: bool,
}

/// The methods `settings`, the `enabled` providers and a configured `mail`
/// relay make usable, for `tenant` or, without one, the deployment root.
///
/// Password, returning providers and reset belong to a tenant and are never
/// offered at the root; creation and discovery are deployment-wide.
fn available_methods(
    settings: LoginSettings,
    enabled: Vec<SocialProvider>,
    mail: bool,
    tenant: Option<TenantLogin>,
) -> LoginMethodsResponse {
    let open = tenant.is_some_and(|tenant| !tenant.requires_company_sso);
    let password = open && settings.password_enabled;
    let local_signup = settings.signup_enabled && settings.password_enabled && mail;
    let signup_providers = if settings.signup_enabled {
        enabled.clone()
    } else {
        Vec::new()
    };
    LoginMethodsResponse {
        sign_in: SignInMethods {
            password,
            company_sso: tenant.is_some_and(|tenant| tenant.company_sso),
            providers: if open { enabled } else { Vec::new() },
        },
        email: EmailMethods {
            email_discovery: mail,
            password_reset: password && mail,
        },
        creation: CreationMethods {
            signup: local_signup || !signup_providers.is_empty(),
            local_signup,
            email_verification_required: true,
            signup_providers,
        },
    }
}

/// Claim mapping of an OIDC social provider: the standard subject and email,
/// and never groups, because social sign-in grants no role.
fn oidc_claims() -> ClaimMapping {
    ClaimMapping {
        subject: ClaimPath::new("sub"),
        email: Some(ClaimPath::new("email")),
        groups: None,
    }
}

/// An enrollment whose provider identity already has a User here.
fn account_exists(provider: SocialProvider) -> WyrdError {
    WyrdError::InvalidToken {
        message: format!(
            "this {provider} sign-in already has an account in this workspace; \
             sign in with it, or ask an administrator for more access"
        ),
        details: json!({ "reason": "account_exists" }),
    }
}

/// A refused provider return.
fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// A provider configuration changed while its sign-in was in flight.
fn revision_changed() -> WyrdError {
    invalid_token("the provider configuration changed while the sign-in was in progress")
}

/// A malformed transaction.
fn invalid_state(message: &str) -> WyrdError {
    WyrdError::InvalidState {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// An unconfigured provider.
fn unconfigured(provider: SocialProvider) -> WyrdError {
    WyrdError::NotFound {
        message: format!("{provider} sign-in is not configured"),
        details: json!({ "provider": provider.as_str() }),
    }
}

/// A request that conflicts with the provider's current state.
fn conflict(message: &str, reason: &str) -> WyrdError {
    WyrdError::Conflict {
        message: message.to_owned(),
        details: json!({ "reason": reason }),
    }
}

/// A caller-correctable request field.
fn validation(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: json!({ "field": field }),
    }
}

/// How long a proven signup stays completable after its proof.
const SIGNUP_COMPLETION_TTL: std::time::Duration = std::time::Duration::from_mins(5);

/// Signup starts admitted per trusted source in one window.
const SIGNUP_START_ATTEMPTS: i32 = 20;

/// Workspace creations admitted per trusted source in one window.
const SIGNUP_SOURCE_ATTEMPTS: i32 = 20;

/// Workspace creations admitted per provider identity in one window.
const SIGNUP_IDENTITY_ATTEMPTS: i32 = 5;

/// Length of one signup admission window.
const SIGNUP_WINDOW: std::time::Duration = std::time::Duration::from_hours(1);

/// Most expired windows, and most expired attempts, one purge batch removes.
const SIGNUP_PURGE_BATCH: i64 = 500;

/// How long a spent proof's digest outlives its completion window. A proof
/// is recorded no earlier than its pre-proof envelope was sealed, and no
/// envelope lives longer than an email link, so the digest outlasts every
/// envelope that could still present it and a replay records nothing.
const SIGNUP_SPENT_RETENTION: std::time::Duration =
    SIGNUP_EMAIL_TTL.saturating_sub(SIGNUP_COMPLETION_TTL);

/// A pre-proof signup flow, sealed into its browser's cookie instead of
/// stored: nothing about an unproven visitor is written to PostgreSQL.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "purpose", rename_all = "snake_case")]
enum SignupEnvelope {
    /// A provider round-trip begun for workspace creation.
    SignupSocial {
        /// The OAuth `state` the callback must return.
        state: String,
        /// The provider it was begun with.
        provider: SocialProvider,
        /// The exact provider revision it was begun under.
        revision: i64,
        /// The provider's social callback.
        redirect_uri: String,
        /// PKCE verifier.
        code_verifier: String,
        /// OpenID nonce.
        nonce: String,
        /// Random browser binding; its digest marks the proof.
        binding: String,
        /// Database-clock deadline, in Unix seconds.
        expires_at: i64,
    },
    /// A mailbox proof sent for workspace creation.
    SignupEmail {
        /// SHA-256 of the proof the message carries; the cookie never holds
        /// the proof itself.
        proof_hash: Sha256Hex,
        /// The address in comparison form.
        address: String,
        /// Random browser binding; its digest marks the proof.
        binding: String,
        /// Database-clock deadline, in Unix seconds.
        expires_at: i64,
        /// The provider attempt this mailbox becomes the verified contact
        /// of, when its provider verified none; otherwise the proof is its
        /// own email signup.
        #[serde(default)]
        attempt: Option<Uuid>,
    },
}

/// How long an email signup proof stays usable.
pub const SIGNUP_EMAIL_TTL: std::time::Duration = std::time::Duration::from_mins(15);

/// Email signup starts admitted per address in one window, whatever their
/// source.
const SIGNUP_ADDRESS_ATTEMPTS: i32 = 5;

/// Signup without the deployment sealing keyring, which refuses rather than
/// store or send plain state.
fn sealing_key_missing() -> WyrdError {
    WyrdError::AuthVerifyUnavailable {
        message: "signup needs the deployment sealing keyring".to_owned(),
        details: json!({ "reason": "sealing_key_missing" }),
    }
}

/// One provider return to redeem: its code and the round-trip values the
/// redemption must match.
struct ProviderReturn<'a> {
    /// The authorization code.
    code: SecretString,
    /// The callback the authorization named.
    redirect_uri: &'a str,
    /// The PKCE verifier.
    code_verifier: &'a SecretString,
    /// The OIDC nonce, for providers that echo one.
    nonce: Option<&'a str>,
    /// The RFC 9207 `iss` the return carried.
    response_issuer: Option<&'a str>,
}

/// A proven signup this completion now holds for creating its workspace.
#[derive(Debug, Clone)]
pub struct SignupCreation {
    /// The attempt id the tenant claim names.
    pub attempt: Uuid,
    /// How the creator will sign in to the workspace.
    pub credential: SignupCredential,
    /// The identity's keyed creator pseudonym, recorded on the tenant.
    pub creator_key: [u8; 32],
    /// The tenant an earlier, failed completion of this attempt claimed, and
    /// its slug.
    pub claimed: Option<(DataTenantId, String)>,
}

/// The sign-in a completed signup gives the workspace's creator, fixed by
/// the kind of proof it consumed.
#[derive(Debug, Clone)]
pub enum SignupCredential {
    /// A provider identity, linked to the creator's User.
    Social {
        /// The provider the identity was proven with.
        provider: SocialProvider,
        /// The exact provider revision of that proof.
        provider_revision: i64,
        /// The identity's issuer namespace.
        issuer: String,
        /// The identity's subject.
        subject: String,
        /// The verified contact: the provider's verified email, or a
        /// mailbox proven to Wyrd for this attempt.
        email: String,
    },
    /// A tenant-local password, chosen at completion after an email proof.
    Local {
        /// The canonical username.
        username: String,
        /// The password's PHC verifier.
        verifier: String,
        /// The proven address, the creator's verified contact.
        email: String,
    },
}

impl SignupCredential {
    /// The creator's verified contact address.
    #[must_use]
    pub fn contact(&self) -> &str {
        match self {
            Self::Social { email, .. } | Self::Local { email, .. } => email,
        }
    }
}

impl SocialSignIn {
    /// Begin a public workspace signup through the enabled shared
    /// `provider`, from the browser that will hold the returned binding.
    ///
    /// The start is first admitted in the shared start window of its trusted
    /// `source`, before any other work. Signup must offer the provider. The
    /// returned binding is the sealed round-trip: state, PKCE verifier,
    /// nonce, the exact provider revision, the provider's social callback on
    /// `origin`, a browser binding and a five-minute deadline. Nothing is
    /// stored, created or audited.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when `source` is unknown
    /// or admission state is unavailable, [`WyrdError::SignupAttempts`] while
    /// the start window is exhausted, [`WyrdError::MethodUnavailable`] when
    /// signup or the provider is not available, and the provider and store
    /// availability errors.
    pub async fn begin_signup(
        &self,
        provider: SocialProvider,
        origin: &Url,
        source: Option<std::net::IpAddr>,
    ) -> Result<BegunBrowserLogin, WyrdError> {
        let source = source.ok_or_else(source_unknown)?;
        let start = self.pseudonym("start", crate::admission::source_key(source).as_bytes())?;
        self.admit(&[(SignupAdmissionScope::Start, start, SIGNUP_START_ATTEMPTS)])
            .await?;
        self.require_signup_provider(provider).await?;
        let row = social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.enabled)
            .ok_or_else(|| invalid_token(&format!("{provider} sign-in is not available")))?;
        let authorization = self
            .authorize(provider, &row.client_id, origin, true)
            .await?;
        let binding = self.seal_envelope(&SignupEnvelope::SignupSocial {
            state: authorization.state,
            provider,
            revision: row.revision,
            redirect_uri: authorization.redirect_uri.to_string(),
            code_verifier: authorization.code_verifier.expose_secret().to_owned(),
            nonce: authorization.nonce,
            binding: random_secret(),
            expires_at: self.envelope_deadline(LOGIN_STATE_TTL).await?,
        })?;
        Ok(BegunBrowserLogin {
            authorization_url: authorization.url,
            binding,
        })
    }

    /// Begin a public workspace signup by proving the mailbox `typed`, from
    /// the browser that will hold the returned flow cookie.
    ///
    /// The start is admitted in the shared start window of its trusted
    /// `source` and its address's own window before any other work; then
    /// local signup must be offered. The address is mailed a link whose
    /// fragment carries a one-use proof; the returned sealed cookie holds the
    /// proof's digest, the canonical address, a browser binding and a
    /// fifteen-minute database-clock deadline. Nothing is stored.
    ///
    /// When this browser's `completion` holds a live provider attempt whose
    /// provider verified no email, the proof instead becomes that attempt's
    /// verified contact; then signup and a mail relay must be available, but
    /// not password sign-in.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an unparsable address,
    /// [`WyrdError::SignupAttempts`] while a window is exhausted,
    /// [`WyrdError::MethodUnavailable`] when local signup is not offered, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the source is unknown or
    /// admission state, the clock or mail is unavailable.
    pub async fn begin_email_signup(
        &self,
        mail: &crate::mail::ApplicationMail,
        typed: &str,
        origin: &Url,
        source: Option<std::net::IpAddr>,
        completion: Option<&str>,
    ) -> Result<SecretString, WyrdError> {
        let source = source.ok_or_else(source_unknown)?;
        let address = crate::mail::EmailAddress::parse(typed, "email")?;
        let start = self.pseudonym("start", crate::admission::source_key(source).as_bytes())?;
        let mailbox = self.pseudonym("address", address.canonical.as_bytes())?;
        self.admit(&[
            (SignupAdmissionScope::Start, start, SIGNUP_START_ATTEMPTS),
            (
                SignupAdmissionScope::Address,
                mailbox,
                SIGNUP_ADDRESS_ATTEMPTS,
            ),
        ])
        .await?;
        let attempt = match completion {
            Some(completion) => proven_signup(
                &self.operator,
                Sha256Hex::digest(completion.as_bytes()).as_bytes(),
            )
            .await
            .map_err(store_error)?
            .filter(|proven| {
                proven.proof_kind == "social" && proven.status == "proven" && proven.email.is_none()
            })
            .map(|proven| proven.id),
            None => None,
        };
        let offered = if attempt.is_some() {
            self.settings().await?.signup_enabled
                && mail_relay(&self.operator)
                    .await
                    .map_err(store_error)?
                    .is_some()
        } else {
            self.login_methods(None).await?.creation.local_signup
        };
        if !offered {
            return Err(signup_unavailable());
        }
        let proof = random_secret();
        let cookie = self.seal_envelope(&SignupEnvelope::SignupEmail {
            proof_hash: Sha256Hex::digest(proof.as_bytes()),
            address: address.canonical.clone(),
            binding: random_secret(),
            expires_at: self.envelope_deadline(SIGNUP_EMAIL_TTL).await?,
            attempt,
        })?;
        let mut link = origin
            .join("/signup/verify")
            .map_err(|_| invalid_state("the deployment origin is invalid"))?;
        link.set_fragment(Some(&format!("proof={proof}")));
        mail.send_signup_proof(&address, link.as_str(), &proof)
            .await?;
        Ok(cookie)
    }

    /// Record the mailbox proof `proof` for the browser presenting the flow
    /// `cookie`, and return the completion secret that browser must present
    /// to create its workspace.
    ///
    /// The proof must match the cookie's digest before its deadline, and
    /// local signup must still be offered. It is recorded once; the same
    /// browser presenting it again, as after a lost response, gets a fresh
    /// completion secret for the same proof, and any other flow is refused.
    /// A proof begun for a provider attempt instead records the address as
    /// that still-proven attempt's contact, once, and rotates its completion
    /// secret.
    ///
    /// # Errors
    /// Returns [`WyrdError::OneTimeCodeInvalid`] for a missing or foreign
    /// cookie, a wrong or spent proof, [`WyrdError::InvalidState`] once
    /// expired, [`WyrdError::InvalidToken`] when local signup is no longer
    /// offered, and the store availability errors.
    pub async fn verify_email_signup(
        &self,
        cookie: Option<&str>,
        proof: &SecretBearer,
    ) -> Result<SecretString, WyrdError> {
        let refused = || WyrdError::OneTimeCodeInvalid {
            message: "this email link is not valid in this browser; start again".to_owned(),
            details: json!({}),
        };
        let Some(SignupEnvelope::SignupEmail {
            proof_hash,
            address,
            binding,
            expires_at,
            attempt,
        }) = cookie.and_then(|cookie| self.open_envelope(cookie))
        else {
            return Err(refused());
        };
        if code_hash(proof).ok() != Some(proof_hash) {
            return Err(refused());
        }
        self.require_live(expires_at).await?;
        if let Some(attempt) = attempt {
            let completion = SecretString::from(random_secret());
            let attached = attach_signup_contact(
                &self.operator,
                attempt,
                &address,
                Sha256Hex::digest(completion.expose_secret().as_bytes()).as_bytes(),
            )
            .await
            .map_err(store_error)?;
            return if attached {
                Ok(completion)
            } else {
                Err(refused())
            };
        }
        if !self.login_methods(None).await?.creation.local_signup {
            return Err(signup_unavailable());
        }
        let completion = SecretString::from(random_secret());
        let completion_hash = Sha256Hex::digest(completion.expose_secret().as_bytes());
        let binding_hash = Sha256Hex::digest(binding.as_bytes());
        let recorded = insert_proven_signup(
            &self.operator,
            &SignupProof {
                id: Uuid::now_v7(),
                kind: "email",
                proof_hash: proof_hash.as_bytes(),
                provider: None,
                browser_binding_hash: binding_hash.as_bytes(),
                completion_hash: completion_hash.as_bytes(),
                identity: None,
                email: Some(&address),
            },
            SIGNUP_COMPLETION_TTL,
        )
        .await
        .map_err(store_error)?;
        if recorded
            || renew_signup_completion(
                &self.operator,
                proof_hash.as_bytes(),
                binding_hash.as_bytes(),
                completion_hash.as_bytes(),
            )
            .await
            .map_err(store_error)?
        {
            Ok(completion)
        } else {
            Err(refused())
        }
    }

    /// Finish the provider return of a signup `provider` sent to its social
    /// callback, for the browser presenting `browser_binding`, and return the
    /// completion secret that browser must present to create its workspace.
    ///
    /// Returns `Ok(None)` unless `browser_binding` is this provider's sealed
    /// signup round-trip for the returned state. The deadline is checked
    /// against the database clock before any provider IO, signup and the
    /// exact provider revision must still be offered, and the identity is
    /// verified. The proof is recorded once per state: of concurrent or
    /// replayed returns, one records it and the others are refused. Only the
    /// identity and digests are kept; the provider's tokens are not.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] for an expired or already used
    /// round-trip, [`WyrdError::MethodUnavailable`] when signup or the
    /// provider is no longer offered, [`WyrdError::InvalidToken`] when the
    /// provider revision changed or verification fails, and the provider and
    /// store availability errors.
    pub async fn prove_signup(
        &self,
        provider: SocialProvider,
        query: &CallbackQuery,
        browser_binding: Option<&str>,
    ) -> Result<Option<SecretString>, WyrdError> {
        let Some(SignupEnvelope::SignupSocial {
            state,
            provider: began,
            revision,
            redirect_uri,
            code_verifier,
            nonce,
            binding,
            expires_at,
        }) = browser_binding.and_then(|cookie| self.open_envelope(cookie))
        else {
            return Ok(None);
        };
        if began != provider || state != query.state {
            return Ok(None);
        }
        self.require_live(expires_at).await?;
        let code = match query.response()? {
            ProviderResponse::Code(code) => code.into_secret_string(),
            ProviderResponse::Error(error) => return Err(provider_refusal(&error)),
        };
        self.recheck_signup(provider, revision).await?;
        let enabled = self.revision(provider, revision).await?;
        let identity = self
            .verify_code(
                &enabled,
                ProviderReturn {
                    code,
                    redirect_uri: &redirect_uri,
                    code_verifier: &SecretString::from(code_verifier),
                    nonce: Some(nonce.as_str()),
                    response_issuer: query.iss.as_deref(),
                },
                true,
            )
            .await?;
        let completion = SecretString::from(random_secret());
        let recorded = insert_proven_signup(
            &self.operator,
            &SignupProof {
                id: Uuid::now_v7(),
                kind: "social",
                proof_hash: Sha256Hex::digest(state.as_bytes()).as_bytes(),
                provider: Some((provider.as_str(), revision)),
                browser_binding_hash: Sha256Hex::digest(binding.as_bytes()).as_bytes(),
                completion_hash: Sha256Hex::digest(completion.expose_secret().as_bytes())
                    .as_bytes(),
                identity: Some((self.issuer(provider), &identity.subject)),
                email: identity.email.as_deref(),
            },
            SIGNUP_COMPLETION_TTL,
        )
        .await
        .map_err(store_error)?;
        if !recorded {
            return Err(invalid_state("this signup was already used; start again"));
        }
        Ok(Some(completion))
    }

    /// Admit the proven signup held by `completion` to create its workspace,
    /// counting it against the shared windows of `source` and its identity.
    ///
    /// An email proof needs `local` credentials inside the password policy,
    /// which become the creator's tenant-local sign-in; a social proof
    /// refuses them. Signup and, for a social proof, the exact provider
    /// revision are rechecked. Exactly one completion of an attempt runs at
    /// a time; a failed one, or its own run interrupted past the
    /// provisioning-claim staleness bound, may be retried by the same
    /// attempt, which resumes its own claim.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for credentials the proof kind or
    /// the password policy refuses, [`WyrdError::InvalidState`] for an
    /// unknown or expired proof, [`WyrdError::Conflict`] when the attempt
    /// already created its workspace (`details.tenant` names it) or another
    /// completion holds it, [`WyrdError::InvalidToken`] when signup or the
    /// provider changed, [`WyrdError::SignupAttempts`] while a window is
    /// exhausted, and [`WyrdError::AuthVerifyUnavailable`] when the
    /// admission state cannot be read.
    pub async fn admit_signup(
        &self,
        completion: &str,
        local: Option<&LocalCredentials>,
        source: Option<std::net::IpAddr>,
    ) -> Result<SignupCreation, WyrdError> {
        let source = source.ok_or_else(source_unknown)?;
        let proven = proven_signup(
            &self.operator,
            Sha256Hex::digest(completion.as_bytes()).as_bytes(),
        )
        .await
        .map_err(store_error)?
        .ok_or_else(|| invalid_state("this signup has expired; start again"))?;
        if proven.status == "completed" {
            return Err(WyrdError::Conflict {
                message: "this signup already created its workspace; sign in to it".to_owned(),
                details: json!({ "reason": "workspace_created", "tenant": proven.tenant_slug }),
            });
        }
        let (credential, identity) = match (proven.proof_kind.as_str(), local) {
            ("email", Some(local)) => {
                let address = proven
                    .email
                    .ok_or_else(|| invalid_state("this signup's proof names no address"))?;
                let username = canonical_username(&local.username)?;
                let password = new_password(
                    &local.password.clone().into_secret_string(),
                    "local_credentials.password",
                )?;
                if !self.login_methods(None).await?.creation.local_signup {
                    return Err(signup_unavailable());
                }
                let verifier = hash_password(password).await?;
                let identity = identity_value("mailto", &address);
                (
                    SignupCredential::Local {
                        username,
                        verifier,
                        email: address,
                    },
                    identity,
                )
            }
            ("email", None) => {
                return Err(credentials_refused(
                    "an email signup needs local_credentials for its administrator",
                ));
            }
            (_, Some(_)) => {
                return Err(credentials_refused(
                    "a provider signup signs in with its provider; omit local_credentials",
                ));
            }
            (_, None) => {
                let (Some(provider), Some(provider_revision), Some(issuer), Some(subject)) = (
                    proven.provider.as_deref().and_then(SocialProvider::parse),
                    proven.provider_revision,
                    proven.issuer,
                    proven.subject,
                ) else {
                    return Err(invalid_state(
                        "this signup's proof cannot create a workspace; start again",
                    ));
                };
                let email = proven.email.ok_or_else(|| WyrdError::Conflict {
                    message: "your provider did not verify an email address; verify one by \
                              email to create the workspace"
                        .to_owned(),
                    details: json!({ "reason": "contact_required" }),
                })?;
                self.recheck_signup(provider, provider_revision).await?;
                let identity = identity_value(&issuer, &subject);
                (
                    SignupCredential::Social {
                        provider,
                        provider_revision,
                        issuer,
                        subject,
                        email,
                    },
                    identity,
                )
            }
        };
        let creator_key = self.pseudonym("creator", &identity)?;
        self.admit(&[
            (
                SignupAdmissionScope::Identity,
                self.pseudonym("identity", &identity)?,
                SIGNUP_IDENTITY_ATTEMPTS,
            ),
            (
                SignupAdmissionScope::Source,
                self.pseudonym("source", crate::admission::source_key(source).as_bytes())?,
                SIGNUP_SOURCE_ATTEMPTS,
            ),
        ])
        .await?;
        if !start_signup_creation(&self.operator, proven.id)
            .await
            .map_err(store_error)?
        {
            return Err(signup_busy());
        }
        let claimed = match (proven.tenant_id, proven.tenant_slug) {
            (Some(id), Some(slug)) => Some((
                DataTenantId::try_from(id)
                    .map_err(|_| invalid_state("this signup names an invalid tenant"))?,
                slug,
            )),
            _ => None,
        };
        Ok(SignupCreation {
            attempt: proven.id,
            credential,
            creator_key,
            claimed,
        })
    }

    /// Remove one bounded batch of expired signup admission windows and
    /// attempts, returning how many rows went; `0` once nothing has expired.
    /// An attempt stays past its completion window until no envelope that
    /// could replay its proof is still live.
    ///
    /// Background auth maintenance repeats this; no public request purges.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn purge_expired_signups(&self) -> Result<u64, WyrdError> {
        purge_expired_signup_state(
            &self.operator,
            SIGNUP_WINDOW,
            SIGNUP_SPENT_RETENTION,
            SIGNUP_PURGE_BATCH,
        )
            .await
            .map_err(store_error)
    }

    /// `envelope` sealed under the deployment keyring, as a cookie value.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] without a keyring and
    /// [`WyrdError::Internal`] when sealing fails.
    fn seal_envelope(&self, envelope: &SignupEnvelope) -> Result<SecretString, WyrdError> {
        let keyring = self.keyring.as_ref().ok_or_else(sealing_key_missing)?;
        let plain = serde_json::to_vec(envelope).map_err(|error| WyrdError::Internal {
            message: format!("signup envelope encoding failed: {error}"),
            details: json!({}),
        })?;
        let sealed = keyring.seal(&plain).map_err(|error| WyrdError::Internal {
            message: format!("signup envelope sealing failed: {error}"),
            details: json!({}),
        })?;
        Ok(SecretString::from(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sealed),
        ))
    }

    /// The envelope sealed in `cookie`; `None` for any value this keyring did
    /// not seal as an envelope, so a tampered or foreign cookie proves
    /// nothing.
    fn open_envelope(&self, cookie: &str) -> Option<SignupEnvelope> {
        let sealed = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(cookie)
            .ok()?;
        let plain = self.keyring.as_ref()?.open(&sealed).ok()?;
        serde_json::from_slice(&plain).ok()
    }

    /// The database-clock deadline, in Unix seconds, `ttl` from now.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the clock read fails.
    async fn envelope_deadline(&self, ttl: std::time::Duration) -> Result<i64, WyrdError> {
        let now = signup_clock(&self.operator).await.map_err(store_error)?;
        Ok(now.timestamp().saturating_add_unsigned(ttl.as_secs()))
    }

    /// Refuse an envelope whose `deadline` the database clock has passed.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] once expired and
    /// [`WyrdError::AuthVerifyUnavailable`] when the clock read fails.
    async fn require_live(&self, deadline: i64) -> Result<(), WyrdError> {
        let now = signup_clock(&self.operator).await.map_err(store_error)?;
        if now.timestamp() < deadline {
            Ok(())
        } else {
            Err(invalid_state("this signup has expired; start again"))
        }
    }

    /// The keyring pseudonym of `value` in `scope`, which signup admission
    /// and creator accounting store instead of the value.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the deployment has no
    /// sealing keyring, which refuses signup rather than storing plain keys.
    fn pseudonym(&self, scope: &str, value: &[u8]) -> Result<[u8; 32], WyrdError> {
        self.keyring
            .as_ref()
            .map(|keyring| keyring.pseudonym(scope, value))
            .ok_or_else(sealing_key_missing)
    }

    /// Count one attempt in every `(scope, key, limit)` window and refuse with
    /// the longest wait when any is exhausted.
    ///
    /// # Errors
    /// Returns [`WyrdError::SignupAttempts`] with `retry_after_seconds`, and
    /// [`WyrdError::AuthVerifyUnavailable`] when admission state is
    /// unavailable, which refuses rather than admits.
    async fn admit(
        &self,
        windows: &[(SignupAdmissionScope, [u8; 32], i32)],
    ) -> Result<(), WyrdError> {
        let mut wait = None;
        for (scope, key, limit) in windows {
            let refused = admit_signup(&self.operator, *scope, key, *limit, SIGNUP_WINDOW)
                .await
                .map_err(store_error)?;
            wait = wait.max(refused);
        }
        match wait {
            None => Ok(()),
            Some(wait) => Err(WyrdError::SignupAttempts {
                message: "too many workspace signups; wait before retrying".to_owned(),
                details: json!({ "retry_after_seconds": wait.as_secs() }),
            }),
        }
    }

    /// Record the end of `attempt`'s creation: `completed`, or `failed` so
    /// the same attempt may retry.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the write fails.
    pub async fn finish_signup(&self, attempt: Uuid, completed: bool) -> Result<(), WyrdError> {
        finish_signup_creation(&self.operator, attempt, completed)
            .await
            .map_err(store_error)
    }

    /// Refuse unless the methods report offers `provider` for workspace
    /// creation.
    ///
    /// # Errors
    /// Returns [`WyrdError::MethodUnavailable`] when it does not, and the
    /// store availability errors.
    async fn require_signup_provider(&self, provider: SocialProvider) -> Result<(), WyrdError> {
        if self
            .login_methods(None)
            .await?
            .creation
            .signup_providers
            .contains(&provider)
        {
            Ok(())
        } else {
            Err(signup_unavailable())
        }
    }

    /// Refuse unless signup is enabled and `revision` is still `provider`'s
    /// current, enabled revision.
    ///
    /// # Errors
    /// Returns [`WyrdError::MethodUnavailable`] when signup or the provider is
    /// no longer offered, [`WyrdError::InvalidToken`] when the revision
    /// changed, and the store availability errors.
    async fn recheck_signup(
        &self,
        provider: SocialProvider,
        revision: i64,
    ) -> Result<(), WyrdError> {
        self.require_signup_provider(provider).await?;
        social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.revision == revision && row.enabled)
            .map(|_| ())
            .ok_or_else(revision_changed)
    }
}

impl BrowserSessions {
    /// Sign the creator of a just-activated workspace into it as `user_id`,
    /// audited as a login: through the linked identity `identity_id`
    /// anchored to the exact provider revision a social `creation` proved,
    /// or with a password session at the User's current password revision
    /// for a local one.
    ///
    /// # Errors
    /// Returns the issuance refusals of an inactive tenant or User and the
    /// store availability errors.
    pub async fn open_signup_session(
        &self,
        tenant_id: DataTenantId,
        user_id: Uuid,
        identity_id: Option<Uuid>,
        creation: &SignupCreation,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant_id)
            .await
            .map_err(store_error)?;
        let (grant, authority) = match (&creation.credential, identity_id) {
            (
                SignupCredential::Social {
                    provider,
                    provider_revision,
                    ..
                },
                Some(identity_id),
            ) => (
                TenantGrant::OidcLogin,
                BrowserSessionAuthority::Social(SocialAnchor {
                    provider: *provider,
                    provider_revision: *provider_revision,
                    identity_id,
                }),
            ),
            (SignupCredential::Local { .. }, None) => {
                let revision = local_credential_for_update(&mut conn, user_id)
                    .await
                    .map_err(store_error)?
                    .ok_or_else(|| invalid_state("the workspace's creator has no password"))?
                    .password_revision;
                (
                    TenantGrant::PasswordLogin,
                    BrowserSessionAuthority::Password(revision),
                )
            }
            _ => {
                return Err(invalid_state(
                    "the workspace's creator has no matching sign-in",
                ));
            }
        };
        let verified = self.issuer.verify(&mut conn, user_id, grant).await?;
        let created = create_session(&mut conn, verified, authority).await?;
        conn.commit().await.map_err(store_error)?;
        self.stage_login(&created.session, request_id);
        Ok(created)
    }
}

/// Credentials the proof kind refuses, naming the field.
fn credentials_refused(message: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: json!({ "field": "local_credentials" }),
    }
}

/// A signup request whose trusted source cannot be determined, which no
/// per-source window could count.
/// The admission value of a provider identity: its issuer namespace and
/// subject, length-delimited so no two identities share one.
fn identity_value(issuer: &str, subject: &str) -> Vec<u8> {
    let mut value = Vec::with_capacity(8 + issuer.len() + subject.len());
    value.extend_from_slice(&(issuer.len() as u64).to_be_bytes());
    value.extend_from_slice(issuer.as_bytes());
    value.extend_from_slice(subject.as_bytes());
    value
}

fn source_unknown() -> WyrdError {
    WyrdError::AuthVerifyUnavailable {
        message: "this request's source cannot be determined, so signup cannot admit it".to_owned(),
        details: json!({ "reason": "source_unknown" }),
    }
}

/// Public signup is switched off.
fn signup_unavailable() -> WyrdError {
    WyrdError::MethodUnavailable {
        message: "workspace signup is not available".to_owned(),
        details: json!({ "reason": "method_unavailable" }),
    }
}

/// Another completion of this signup is running.
fn signup_busy() -> WyrdError {
    WyrdError::Conflict {
        message: "this signup is already creating its workspace; wait and retry".to_owned(),
        details: json!({ "reason": "signup_in_progress" }),
    }
}
