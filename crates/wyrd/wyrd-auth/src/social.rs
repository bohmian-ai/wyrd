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

use std::sync::Arc;

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, CodeRedemption, MappedClaims, RelyingParty, ScreenedHttp,
};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::auth::{
    AbsoluteUrl, ConfigureSocialProviderRequest, EnableSocialProviderRequest, IssuerUrl,
    LoginMethodsResponse, LoginSettings, ProviderResponse, Sha256Hex, SocialProvider,
    SocialProviderState, SocialProviderView, SocialProvidersResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::platform::social::{
    LoginSettingsRow, ProviderEnablement, SocialProviderRow, SocialTestState,
    insert_social_test_state, login_settings, put_login_settings, put_social_provider,
    set_social_provider_enabled, social_provider, social_providers, stamp_social_provider_tested,
    take_social_test_state,
};
use wyrd_sql::{OperatorPool, TenantConn};

use crate::callback::verify_response_issuer;
use crate::error::{relying_party_error, store_error};
use crate::login::LOGIN_STATE_TTL;

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
}

impl Default for SocialEndpoints {
    fn default() -> Self {
        Self {
            google_issuer: IssuerUrl::new(GOOGLE_ISSUER)
                .expect("invariant: Google's issuer is a valid issuer URL"),
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
        let redirect_uri = social_callback(provider, origin)?;
        let (url, state, verifier, nonce) = match provider {
            SocialProvider::Google => {
                let metadata = self
                    .relying_party
                    .discover(&self.endpoints.google_issuer)
                    .await
                    .map_err(relying_party_error)?;
                let authorization = self
                    .relying_party
                    .authorize(&metadata, &row.client_id, redirect_uri.as_str())
                    .map_err(relying_party_error)?;
                (
                    authorization.url,
                    authorization.state,
                    authorization.code_verifier,
                    Some(authorization.nonce),
                )
            }
            SocialProvider::Github => return Err(unconfigured(provider)),
        };
        let authorization_url = AbsoluteUrl::new(url.as_str().to_owned())
            .map_err(|_| invalid_state("the provider authorization URL is invalid"))?;
        insert_social_test_state(
            &self.operator,
            Sha256Hex::digest(state.as_bytes()).as_bytes(),
            &SocialTestState {
                provider: provider.as_str().to_owned(),
                provider_revision: row.revision,
                redirect_uri: redirect_uri.to_string(),
                code_verifier: verifier.expose_secret().to_owned(),
                nonce,
                tester_principal_id: tester,
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(store_error)?;
        Ok(authorization_url)
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
            SocialProvider::Github => Err(unconfigured(provider.provider)),
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
             then test. Wyrd requests only the user:email scope."
        ),
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
