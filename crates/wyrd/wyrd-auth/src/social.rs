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

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use url::Url;
use uuid::Uuid;
pub use wyrd_auth_oidc::GithubEndpoints;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, CodeRedemption, GITHUB_NAMESPACE, GithubOAuth,
    MappedClaims, RelyingParty, ScreenedHttp,
};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, CallbackQuery, ConfigureSocialProviderRequest, EnableSocialProviderRequest,
    IssuerUrl, LoginMethodsResponse, LoginSettings, ProviderResponse, SecretBearer, Sha256Hex,
    SocialProvider, SocialProviderState, SocialProviderView, SocialProvidersResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{
    BrowserSessionAuthority, SocialAnchor, SocialLoginState, consume_social_login_state,
    insert_social_login_state, insert_user, insert_user_identity, linked_identity,
};
use wyrd_sql::queries::platform::signup::{
    PendingSignup, SignupAdmissionScope, SignupProof, admit_signup, finish_signup_creation,
    insert_proven_signup, insert_signup_attempt, proven_signup, purge_expired_signup_state,
    start_signup_creation, take_signup_attempt,
};
use wyrd_sql::queries::platform::social::{
    LoginSettingsRow, ProviderEnablement, SocialProviderRow, SocialTestState,
    insert_social_test_state, login_settings, put_login_settings, put_social_provider,
    set_social_provider_enabled, social_provider, social_providers, stamp_social_provider_tested,
    take_social_test_state,
};
use wyrd_sql::{OperatorPool, TenantConn};

use crate::browser_sessions::{
    BegunBrowserLogin, BrowserSessions, CreatedBrowserSession, create_session, random_secret,
};
use crate::callback::{
    LoginCompletion, audit_authorization_code_failure, provider_refusal, verify_response_issuer,
};
use crate::error::{relying_party_error, store_error};
use crate::issuance::TenantGrant;
use crate::login::{LOGIN_STATE_TTL, login_unavailable};
use crate::passwords::code_hash;

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
}

impl Default for SocialEndpoints {
    fn default() -> Self {
        Self {
            google_issuer: IssuerUrl::new(GOOGLE_ISSUER)
                .expect("invariant: Google's issuer is a valid issuer URL"),
            github: GithubEndpoints::default(),
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
        let authorization = self.authorize(provider, &row.client_id, origin).await?;
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
                    .authorize(client_id, redirect_uri.as_str())
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
            code,
            &state.redirect_uri,
            &state.code_verifier,
            Some(&state.nonce),
            response_issuer,
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
            code.into_secret_string(),
            &state.redirect_uri,
            &SecretString::from(state.code_verifier),
            state.nonce.as_deref(),
            response_issuer,
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
    /// it returns.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] or [`WyrdError::InvalidNonce`] when
    /// the provider refuses or verification fails, and the provider
    /// availability errors.
    async fn verify_code(
        &self,
        provider: &EnabledProvider,
        code: SecretString,
        redirect_uri: &str,
        code_verifier: &SecretString,
        nonce: Option<&str>,
        response_issuer: Option<&str>,
    ) -> Result<MappedClaims, WyrdError> {
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
                Ok(verified.identity)
            }
            SocialProvider::Github => self
                .github
                .identity(
                    &provider.client_id,
                    &provider.client_secret,
                    &code,
                    redirect_uri,
                    code_verifier,
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
        })
    }

    /// Replace the deployment login settings on the audited operator `conn`.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the write fails.
    pub async fn put_settings(
        &self,
        conn: &mut TenantConn<'_>,
        settings: LoginSettings,
    ) -> Result<(), WyrdError> {
        put_login_settings(
            conn,
            LoginSettingsRow {
                password_enabled: settings.password_enabled,
                signup_enabled: settings.signup_enabled,
            },
        )
        .await
        .map_err(store_error)
    }

    /// The sign-in methods Rust currently allows, given whether the tenant
    /// offers company SSO.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when a read fails.
    pub async fn login_methods(
        &self,
        company_sso: bool,
    ) -> Result<LoginMethodsResponse, WyrdError> {
        let settings = self.settings().await?;
        let rows = social_providers(&self.operator)
            .await
            .map_err(store_error)?;
        let providers: Vec<SocialProvider> = SocialProvider::ALL
            .into_iter()
            .filter(|provider| {
                rows.iter()
                    .any(|row| row.provider == provider.as_str() && row.enabled)
            })
            .collect();
        Ok(LoginMethodsResponse {
            password: settings.password_enabled,
            company_sso,
            signup: settings.signup_enabled && !providers.is_empty(),
            providers,
        })
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
        let authorization = social.authorize(provider, &row.client_id, origin).await?;
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

/// How long a proven signup stays completable after its provider return.
const SIGNUP_COMPLETION_TTL: std::time::Duration = std::time::Duration::from_mins(30);

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

/// A proven signup this completion now holds for creating its workspace.
#[derive(Debug, Clone)]
pub struct SignupCreation {
    /// The attempt id the tenant claim names.
    pub attempt: Uuid,
    /// The provider the identity was proven with.
    pub provider: SocialProvider,
    /// The exact provider revision of that proof.
    pub provider_revision: i64,
    /// The identity's issuer namespace.
    pub issuer: String,
    /// The identity's subject.
    pub subject: String,
    /// The provider's email, when it gave one.
    pub email: Option<String>,
    /// The identity's keyed creator pseudonym, recorded on the tenant.
    pub creator_key: [u8; 32],
    /// The tenant an earlier, failed completion of this attempt claimed, and
    /// its slug.
    pub claimed: Option<(DataTenantId, String)>,
}

impl SocialSignIn {
    /// Begin a public workspace signup through the enabled shared
    /// `provider`, from the browser that will hold the returned binding.
    ///
    /// The start is first admitted in the shared start window of its trusted
    /// `source`, before any other work. Signup must be enabled. The attempt
    /// binds the exact provider revision, the provider's social callback on
    /// `origin`, server-held PKCE and nonce, and only the SHA-256 of the
    /// browser binding and state. Nothing is created or audited.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when `source` is unknown
    /// or admission state is unavailable, [`WyrdError::SignupAttempts`] while
    /// the start window is exhausted, [`WyrdError::InvalidToken`] when signup
    /// or the provider is not available, and the provider and store
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
        if !self.settings().await?.signup_enabled {
            return Err(signup_unavailable());
        }
        let row = social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.enabled)
            .ok_or_else(|| invalid_token(&format!("{provider} sign-in is not available")))?;
        let authorization = self.authorize(provider, &row.client_id, origin).await?;
        let binding = SecretString::from(random_secret());
        insert_signup_attempt(
            &self.operator,
            Sha256Hex::digest(authorization.state.as_bytes()).as_bytes(),
            &PendingSignup {
                id: Uuid::now_v7(),
                provider: provider.as_str().to_owned(),
                provider_revision: row.revision,
                redirect_uri: authorization.redirect_uri.to_string(),
                code_verifier: authorization.code_verifier.expose_secret().to_owned(),
                nonce: Some(authorization.nonce),
                browser_binding_hash: Sha256Hex::digest(binding.expose_secret().as_bytes())
                    .as_bytes()
                    .to_vec(),
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(store_error)?;
        Ok(BegunBrowserLogin {
            authorization_url: authorization.url,
            binding,
        })
    }

    /// Finish the provider return of a signup `provider` sent to its social
    /// callback, for the browser presenting `browser_binding`, and return the
    /// completion secret that browser must present to create its workspace.
    ///
    /// Returns `Ok(None)` when the state names no pending signup. Otherwise
    /// the round-trip is consumed before any provider IO, the binding must
    /// match, signup and the exact provider revision must still be enabled,
    /// and the identity is verified. Only the identity and the secret's
    /// SHA-256 are kept; the provider's tokens are not.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] for another browser,
    /// [`WyrdError::InvalidToken`] when signup or the provider changed or
    /// verification fails, and the provider and store availability errors.
    pub async fn prove_signup(
        &self,
        provider: SocialProvider,
        query: &CallbackQuery,
        browser_binding: Option<&str>,
    ) -> Result<Option<SecretString>, WyrdError> {
        let state_hash = Sha256Hex::digest(query.state.as_bytes());
        let Some(attempt) =
            take_signup_attempt(&self.operator, provider.as_str(), state_hash.as_bytes())
                .await
                .map_err(store_error)?
        else {
            return Ok(None);
        };
        if !browser_binding.is_some_and(|binding| {
            attempt.browser_binding_hash == Sha256Hex::digest(binding.as_bytes()).as_bytes()
        }) {
            return Err(invalid_state(
                "this browser did not begin the signup; start again",
            ));
        }
        let code = match query.response()? {
            ProviderResponse::Code(code) => code.into_secret_string(),
            ProviderResponse::Error(error) => return Err(provider_refusal(&error)),
        };
        self.recheck_signup(provider, attempt.provider_revision)
            .await?;
        let enabled = self.revision(provider, attempt.provider_revision).await?;
        let identity = self
            .verify_code(
                &enabled,
                code,
                &attempt.redirect_uri,
                &SecretString::from(attempt.code_verifier.clone()),
                attempt.nonce.as_deref(),
                query.iss.as_deref(),
            )
            .await?;
        let completion = SecretString::from(random_secret());
        insert_proven_signup(
            &self.operator,
            &attempt,
            state_hash.as_bytes(),
            Sha256Hex::digest(completion.expose_secret().as_bytes()).as_bytes(),
            &SignupProof {
                issuer: self.issuer(provider),
                subject: &identity.subject,
                email: identity.email.as_deref(),
            },
            SIGNUP_COMPLETION_TTL,
        )
        .await
        .map_err(store_error)?;
        Ok(Some(completion))
    }

    /// Admit the proven signup held by `completion` to create its workspace,
    /// counting it against the shared windows of `source` and its identity.
    ///
    /// Signup and the exact provider revision are rechecked. Exactly one
    /// completion of an attempt runs at a time; a failed one may be retried
    /// by the same attempt, which resumes its own claim.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] for an unknown or expired proof,
    /// [`WyrdError::Conflict`] when the attempt already created its workspace
    /// (`details.tenant` names it) or another completion holds it,
    /// [`WyrdError::InvalidToken`] when signup or the provider changed,
    /// [`WyrdError::SignupAttempts`] while a window is exhausted, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the admission state cannot be
    /// read.
    pub async fn admit_signup(
        &self,
        completion: &str,
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
        match proven.status.as_str() {
            "completed" => {
                return Err(WyrdError::Conflict {
                    message: "this signup already created its workspace; sign in to it".to_owned(),
                    details: json!({ "reason": "workspace_created", "tenant": proven.tenant_slug }),
                });
            }
            "creating" => return Err(signup_busy()),
            _ => {}
        }
        let provider = SocialProvider::parse(&proven.provider)
            .ok_or_else(|| invalid_state("this signup names an unknown provider"))?;
        self.recheck_signup(provider, proven.provider_revision)
            .await?;
        let identity = identity_value(&proven.issuer, &proven.subject);
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
            provider,
            provider_revision: proven.provider_revision,
            issuer: proven.issuer,
            subject: proven.subject,
            email: proven.email,
            creator_key,
            claimed,
        })
    }

    /// Remove one bounded batch of expired signup admission windows and
    /// attempts, returning how many rows went; `0` once nothing has expired.
    ///
    /// Background auth maintenance repeats this; no public request purges.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn purge_expired_signups(&self) -> Result<u64, WyrdError> {
        purge_expired_signup_state(&self.operator, SIGNUP_WINDOW, SIGNUP_PURGE_BATCH)
            .await
            .map_err(store_error)
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
            .ok_or_else(|| WyrdError::AuthVerifyUnavailable {
                message: "signup needs the deployment sealing keyring".to_owned(),
                details: json!({ "reason": "sealing_key_missing" }),
            })
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

    /// Refuse unless signup is enabled and `revision` is still `provider`'s
    /// current, enabled revision.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] otherwise, and the store
    /// availability errors.
    async fn recheck_signup(
        &self,
        provider: SocialProvider,
        revision: i64,
    ) -> Result<(), WyrdError> {
        if !self.settings().await?.signup_enabled {
            return Err(signup_unavailable());
        }
        social_provider(&self.operator, provider.as_str())
            .await
            .map_err(store_error)?
            .filter(|row| row.revision == revision && row.enabled)
            .map(|_| ())
            .ok_or_else(revision_changed)
    }
}

impl BrowserSessions {
    /// Sign the creator of a just-activated workspace into it: `user_id`
    /// through the linked identity `identity_id`, anchored to the exact
    /// provider revision `creation` proved, audited as a login.
    ///
    /// # Errors
    /// Returns the issuance refusals of an inactive tenant or User and the
    /// store availability errors.
    pub async fn open_signup_session(
        &self,
        tenant_id: DataTenantId,
        user_id: Uuid,
        identity_id: Uuid,
        creation: &SignupCreation,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let anchor = SocialAnchor {
            provider: creation.provider,
            provider_revision: creation.provider_revision,
            identity_id,
        };
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant_id)
            .await
            .map_err(store_error)?;
        let verified = self
            .issuer
            .verify(&mut conn, user_id, TenantGrant::OidcLogin)
            .await?;
        let created =
            create_session(&mut conn, verified, BrowserSessionAuthority::Social(anchor)).await?;
        conn.commit().await.map_err(store_error)?;
        self.stage_login(&created.session, request_id);
        Ok(created)
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
    invalid_token("workspace signup is not available")
}

/// Another completion of this signup is running.
fn signup_busy() -> WyrdError {
    WyrdError::Conflict {
        message: "this signup is already creating its workspace; wait and retry".to_owned(),
        details: json!({ "reason": "signup_in_progress" }),
    }
}
