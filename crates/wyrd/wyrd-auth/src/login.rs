//! Tenant human OIDC login: initiation, one-use login state, and redemption
//! of completed logins.
//!
//! A login begins with [`HumanConnections::begin_login`], which resolves the
//! tenant from its route key, requires its Active connection, and writes a
//! login-state row keyed by the SHA-256 of a random state value that exists
//! only in the returned authorization URL. The common callback
//! ([`crate::callback::AuthorizationCodeExchange`]) consumes that row, issues
//! the session, and stores it sealed against the login's initiation binding;
//! [`HumanConnections::redeem_completion`] hands it once to the BFF or CLI
//! holding that binding. No request header ever selects a tenant, connection, or redirect.

use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;
use wyrd_auth_oidc::{OidcProvider, ScreenedHttp};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, BeginLogin, BeginLoginResponse, LoginInitiation, Sha256Hex, TokenResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{LoginState, insert_login_state, redeem_login_completion};
use wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app;

use crate::callback::discover_provider;
use crate::connections::HumanConnections;
use crate::error::{screen_error, store_error};

/// Path of the fixed same-origin BFF route a completed browser login is
/// redirected to: `{public_origin}/login/complete`, with no query string.
///
/// The redirect carries no capability. The BFF redeems the completion with
/// its `HttpOnly` flow cookie through [`HumanConnections::redeem_completion`].
pub const LOGIN_COMPLETE_PATH: &str = "/login/complete";

/// Lifetime of a persisted login-state row: the browser must complete the `IdP`
/// round-trip and reach `/auth/callback` within this window or the state is
/// gone.
const LOGIN_STATE_TTL: Duration = Duration::from_mins(5);

/// Redemption window of a completed login: the BFF or CLI must redeem the
/// sealed session within this window after the callback issues it.
pub(crate) const LOGIN_COMPLETION_TTL: Duration = Duration::from_mins(2);

impl HumanConnections {
    /// Begin a tenant human SSO login and return the provider authorization
    /// URL.
    ///
    /// Everything that can refuse without the network runs first, before any
    /// provider IO or state write: exactly one initiation binding
    /// ([`BeginLogin::initiation`]), a configured public origin, a deployment
    /// sealing keyring (completed logins are sealed at rest, so a keyless
    /// deployment cannot offer human SSO), and a known CLI handoff
    /// ([`known_initiation`]). The route key then resolves the tenant, and
    /// the tenant must have an Active connection; an unknown tenant and a
    /// tenant without one fail with the same generic refusal, so the endpoint
    /// cannot enumerate tenants or their login configuration.
    ///
    /// Discovery runs through this owner's screened HTTP capability and the
    /// discovered authorization endpoint is screened by scheme. The redirect
    /// URI is always the deployment's configured callback. The state row binds
    /// the exact connection revision, issuer, and client id, the PKCE verifier,
    /// the nonce, and the initiation binding; only the SHA-256 of the random
    /// state is stored, and the raw state is returned only inside the URL.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when both or neither binding is
    /// present, no public origin is configured, or no sealing keyring is
    /// configured; [`WyrdError::InvalidState`] for a CLI handoff this server
    /// does not know or a flow binding already recorded for another login;
    /// [`WyrdError::InvalidToken`] when the route key names no active tenant
    /// or the tenant has no Active connection;
    /// [`WyrdError::DiscoveryUnavailable`] when the issuer is refused by
    /// screening, discovery fails, or the authorization endpoint's scheme is
    /// refused; and [`WyrdError::AuthVerifyUnavailable`] when a store read or
    /// the state write fails. No URL is returned unless its state row is
    /// durable; cancellation before the commit persists nothing.
    pub async fn begin_login(&self, request: &BeginLogin) -> Result<BeginLoginResponse, WyrdError> {
        let initiation = request.initiation()?;
        let redirect_uri = self.require_callback()?.clone();
        self.require_keyring()?;
        let initiation = known_initiation(initiation)?;
        let tenant = resolve_by_slug_for_app(self.postgres().app_pool(), &request.tenant_route_key)
            .await
            .map_err(store_error)?
            .ok_or_else(login_unavailable)?;
        let active = self
            .active_connection(tenant)
            .await?
            .ok_or_else(login_unavailable)?;
        let trusted = &active.trusted;
        let provider = discover_provider(&trusted.issuer, self.http()).await?;
        let authorization_endpoint = browser_authorization_endpoint(provider, self.http())?;
        let state_key = auth_state_key();
        let code_verifier = pkce_verifier();
        let nonce = auth_nonce();
        let authorization_url = build_authorization_url(
            &authorization_endpoint,
            &trusted.client_id,
            redirect_uri.as_str(),
            &state_key,
            code_verifier.expose_secret(),
            &nonce,
        );
        let authorization_url = AbsoluteUrl::new(authorization_url.as_str().to_owned())
            .map_err(|_| invalid_token("authorization URL is invalid"))?;
        let row = LoginState {
            connection: active.binding,
            issuer: trusted.issuer.to_string(),
            client_id: trusted.client_id.clone(),
            redirect_uri: redirect_uri.to_string(),
            code_verifier,
            nonce,
            initiation,
        };
        let mut conn = self
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let state_hash = Sha256Hex::digest(state_key.as_bytes());
        if !insert_login_state(&mut conn, &state_hash, &row, LOGIN_STATE_TTL)
            .await
            .map_err(store_error)?
        {
            return Err(WyrdError::InvalidState {
                message: "this login binding was already used; start a new login".to_owned(),
                details: json!({ "reason": "login_binding_reused" }),
            });
        }
        conn.commit().await.map_err(store_error)?;
        Ok(BeginLoginResponse { authorization_url })
    }
}

impl HumanConnections {
    /// Redeem the completed login bound to `initiation` in `tenant`, once.
    ///
    /// The callback stores each issued session sealed under the deployment
    /// keyring against the login's initiation binding. The BFF redeems a
    /// browser login with the flow id hash from its `HttpOnly` cookie, and the
    /// CLI redeems its handoff; this is the primitive the BFF completion route
    /// and the CLI handoff claim wrap, and it has no HTTP route of its own.
    ///
    /// One tenant transaction deletes the completed, unexpired row recorded
    /// for exactly this binding and returns its sealed session, which is then
    /// opened. The delete commits before the session is opened, so a second
    /// redemption finds nothing even if this one fails afterwards; that login
    /// is then lost and the person signs in again.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `sealing_key_missing`
    /// when no sealing keyring is configured, [`WyrdError::InvalidState`] when
    /// no completed, unexpired login is recorded for the binding (never
    /// completed, already redeemed, expired, or another tenant's),
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// [`WyrdError::Internal`] when the sealed session cannot be opened with
    /// this keyring or decoded.
    pub async fn redeem_completion(
        &self,
        tenant: DataTenantId,
        initiation: LoginInitiation,
    ) -> Result<TokenResponse, WyrdError> {
        let keyring = self.require_keyring()?;
        let mut conn = self
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let sealed = redeem_login_completion(&mut conn, &initiation)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let sealed = sealed.ok_or_else(|| WyrdError::InvalidState {
            message: "no completed login is waiting for this binding".to_owned(),
            details: json!({ "reason": "login_completion_missing" }),
        })?;
        let opened = keyring.open(&sealed).map_err(|_| {
            tracing::error!(tenant_id = %tenant, "login completion could not be opened");
            completion_unusable()
        })?;
        serde_json::from_slice(&opened).map_err(|_| completion_unusable())
    }
}

/// Seal an issued session for storage until its redemption.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the session cannot be serialized or
/// sealed.
pub(crate) fn seal_completion(
    keyring: &SealingKeyring,
    token: &TokenResponse,
) -> Result<Vec<u8>, WyrdError> {
    let plaintext = serde_json::to_vec(token).map_err(|_| completion_unusable())?;
    keyring.seal(&plaintext).map_err(|_| completion_unusable())
}

/// Admit a begin request's initiation binding for storage.
///
/// This is the one place a CLI handoff is checked. CLI handoffs are issued by
/// a server-owned handoff table that does not exist yet, so every handoff id
/// is unknown and refused here — before any provider IO or state write. The
/// handoff owner replaces the `Cli` arm with a lookup of its row, leaving the
/// browser path and the rest of login unchanged.
///
/// # Errors
/// Returns [`WyrdError::InvalidState`] with reason `unknown_cli_handoff` for
/// every CLI handoff id.
pub(crate) fn known_initiation(initiation: LoginInitiation) -> Result<LoginInitiation, WyrdError> {
    match initiation {
        LoginInitiation::Browser(_) => Ok(initiation),
        LoginInitiation::Cli(_) => Err(WyrdError::InvalidState {
            message: "the CLI login handoff is unknown or expired; start a new login".to_owned(),
            details: json!({ "reason": "unknown_cli_handoff" }),
        }),
    }
}

/// The one refusal for a route key that names no active tenant or a tenant
/// with no Active connection, so neither can be told apart.
fn login_unavailable() -> WyrdError {
    WyrdError::InvalidToken {
        message: "SSO login is not available for this tenant".to_owned(),
        details: json!({}),
    }
}

/// A stored completion that this process cannot open or decode.
fn completion_unusable() -> WyrdError {
    WyrdError::Internal {
        message: "the login completion could not be processed; start a new login".to_owned(),
        details: json!({}),
    }
}

/// Take the discovered authorization endpoint only if `http`'s policy permits
/// sending a browser there.
///
/// Discovery is fresh on every login, so a provider can change this endpoint
/// after its connection was tested. The deployment's scheme rule is reapplied
/// before any login state exists: production refuses cleartext, so state,
/// nonce, and PKCE challenge never travel over `http`, while a permissive
/// deployment keeps its local `http` providers.
///
/// # Errors
/// Returns [`WyrdError::DiscoveryUnavailable`] — the redacted screening
/// refusal — when the policy refuses the endpoint's scheme.
pub(crate) fn browser_authorization_endpoint(
    provider: OidcProvider,
    http: ScreenedHttp,
) -> Result<Url, WyrdError> {
    let endpoint = provider.metadata.authorization_endpoint;
    http.screen_scheme(&endpoint)
        .map_err(|error| screen_error(&error))?;
    Ok(endpoint)
}

/// Generate an unguessable login-state key.
pub(crate) fn auth_state_key() -> String {
    random_b64url(32)
}

/// Generate the nonce the returned ID token must echo.
pub(crate) fn auth_nonce() -> String {
    random_b64url(32)
}

/// Generate a PKCE code verifier.
pub(crate) fn pkce_verifier() -> SecretString {
    SecretString::from(random_b64url(48))
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn random_b64url(bytes: usize) -> String {
    let mut buf = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Build the provider authorization URL for one login attempt.
///
/// Takes the client identifier rather than a whole trusted issuer because the
/// platform control plane's connection is not a tenant's issuer, and the URL
/// depends on nothing else about the issuer's trust configuration.
pub(crate) fn build_authorization_url(
    authorization_endpoint: &Url,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_verifier: &str,
    nonce: &str,
) -> Url {
    let mut url = authorization_endpoint.clone();
    let challenge = pkce_challenge(code_verifier);
    let mut query = url.query_pairs_mut();
    query.append_pair("response_type", "code");
    query.append_pair("client_id", client_id);
    query.append_pair("redirect_uri", redirect_uri);
    query.append_pair("scope", "openid profile email");
    query.append_pair("code_challenge", &challenge);
    query.append_pair("code_challenge_method", "S256");
    query.append_pair("state", state);
    query.append_pair("nonce", nonce);
    drop(query);
    url
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

/// Login initiation refusals and one-use completion redemption against a real
/// tenant store.
#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use secrecy::SecretString;
    use url::Url;
    use uuid::Uuid;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::TenantSlug;
    use wyrd_spec::auth::{BeginLogin, LoginInitiation, Sha256Hex, TokenResponse, TokenType};
    use wyrd_spec::error::WyrdError;
    use wyrd_sql::queries::auth::{
        LoginState, complete_login_state, consume_login_state, insert_login_state,
    };
    use wyrd_sql::row_types::auth::HumanConnectionBinding;

    use super::{LOGIN_COMPLETION_TTL, seal_completion};
    use crate::connections::HumanConnections;

    /// The deterministic keyring these tests seal completions under.
    fn keyring() -> Arc<SealingKeyring> {
        Arc::new(SealingKeyring::new(SecretKey::from_bytes([9_u8; 32])))
    }

    /// A connection owner over `fixture` with the given keyring and a public
    /// origin.
    ///
    /// # Panics
    /// Panics when the fixed origin does not parse.
    fn connections(fixture: &PgFixture, keyring: Option<Arc<SealingKeyring>>) -> HumanConnections {
        let origin = Url::parse("https://wyrd.example.com").expect("origin parses");
        HumanConnections::new(
            fixture.wyrd_postgres().clone(),
            keyring,
            ScreenedHttp::allowing_internal(),
            Some(&origin),
        )
    }

    /// A begin request for `slug` with the given bindings.
    ///
    /// # Panics
    /// Panics when `slug` is not a valid tenant slug.
    fn begin(slug: &str, flow: Option<&[u8]>, handoff: Option<Uuid>) -> BeginLogin {
        BeginLogin {
            tenant_route_key: TenantSlug::new(slug).expect("slug is valid"),
            browser_flow_hash: flow.map(Sha256Hex::digest),
            cli_handoff_id: handoff,
        }
    }

    /// Number of login-state rows in the fixture tenant.
    ///
    /// # Panics
    /// Panics when the count query fails.
    async fn state_rows(fixture: &PgFixture) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_login_state")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("state count runs")
    }

    /// The stable `details.reason` of a validation or state refusal.
    fn reason(error: &WyrdError) -> Option<String> {
        let (WyrdError::Validation { details, .. } | WyrdError::InvalidState { details, .. }) =
            error
        else {
            return None;
        };
        details
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
    }

    /// A deployment without a sealing keyring refuses to begin a human login
    /// with `sealing_key_missing`, before resolving the tenant or writing
    /// state. The journey server always configures a keyring, so this refusal
    /// is proven here.
    ///
    /// # Panics
    /// Panics when the begin is accepted or refused differently.
    #[tokio::test]
    async fn begin_without_a_sealing_key_is_refused_before_any_state() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let error = connections(&fixture, None)
            .begin_login(&begin(fixture.tenant_slug(), Some(b"flow"), None))
            .await
            .expect_err("a keyless deployment refuses login");

        assert!(matches!(error, WyrdError::Validation { .. }));
        assert_eq!(reason(&error).as_deref(), Some("sealing_key_missing"));
        assert_eq!(state_rows(&fixture).await, 0);
    }

    /// Both bindings, neither binding, and an unknown CLI handoff are refused
    /// before any tenant resolution, provider IO, or state write.
    ///
    /// # Panics
    /// Panics when any of them is accepted or refused differently.
    #[tokio::test]
    async fn begin_refuses_ambiguous_bindings_and_unknown_handoffs() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let owner = connections(&fixture, Some(keyring()));
        let slug = fixture.tenant_slug();

        for request in [
            begin(slug, Some(b"flow"), Some(Uuid::now_v7())),
            begin(slug, None, None),
        ] {
            let error = owner.begin_login(&request).await.expect_err("refused");
            assert!(matches!(error, WyrdError::Validation { .. }), "{error:?}");
        }
        let error = owner
            .begin_login(&begin(slug, None, Some(Uuid::now_v7())))
            .await
            .expect_err("an unknown handoff is refused");
        assert!(matches!(error, WyrdError::InvalidState { .. }));
        assert_eq!(reason(&error).as_deref(), Some("unknown_cli_handoff"));
        assert_eq!(state_rows(&fixture).await, 0);
    }

    /// An unknown tenant and a tenant with no Active connection get the same
    /// generic refusal, so the endpoint does not enumerate tenants.
    ///
    /// # Panics
    /// Panics when either begin is accepted or the refusals differ.
    #[tokio::test]
    async fn unknown_tenant_and_no_connection_are_indistinguishable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let owner = connections(&fixture, Some(keyring()));

        let unknown = owner
            .begin_login(&begin("no-such-tenant", Some(b"flow-a"), None))
            .await
            .expect_err("unknown tenant refuses");
        let unconfigured = owner
            .begin_login(&begin(fixture.tenant_slug(), Some(b"flow-b"), None))
            .await
            .expect_err("tenant without a connection refuses");

        assert_eq!(unknown.code(), unconfigured.code());
        assert_eq!(unknown.to_string(), unconfigured.to_string());
        assert_eq!(state_rows(&fixture).await, 0);
    }

    /// A completed login is redeemed exactly once, only by its own binding,
    /// and opens to the sealed session; a flow binding cannot be recorded
    /// twice.
    ///
    /// # Panics
    /// Panics when any step of the store round-trip misbehaves.
    #[tokio::test]
    async fn a_completed_login_is_redeemed_once_by_its_binding() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let keyring = keyring();
        let flow = Sha256Hex::digest(b"flow-id");
        let hash = Sha256Hex::digest(b"raw-state");
        let row = LoginState {
            connection: HumanConnectionBinding {
                connection_id: Uuid::now_v7(),
                connection_revision: 1,
            },
            issuer: "https://idp.example.com".to_owned(),
            client_id: "wyrd".to_owned(),
            redirect_uri: "https://wyrd.example.com/auth/callback".to_owned(),
            code_verifier: SecretString::from("verifier"),
            nonce: "nonce".to_owned(),
            initiation: LoginInitiation::Browser(flow),
        };
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        assert!(
            insert_login_state(&mut conn, &hash, &row, Duration::from_mins(5))
                .await
                .expect("state inserts")
        );
        assert!(
            !insert_login_state(
                &mut conn,
                &Sha256Hex::digest(b"other-state"),
                &row,
                Duration::from_mins(5)
            )
            .await
            .expect("duplicate binding is a no-op"),
            "a flow binding is recorded against one login only"
        );
        let consumed = consume_login_state(&mut conn, &hash)
            .await
            .expect("consume runs")
            .expect("state is pending");
        assert_eq!(consumed.nonce, "nonce");
        assert!(
            consume_login_state(&mut conn, &hash)
                .await
                .expect("replay runs")
                .is_none(),
            "a state is consumed once"
        );
        let token: TokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": "access",
            "token_type": "Bearer",
            "expires_at": "2030-01-01T00:00:00Z",
        }))
        .expect("token response decodes");
        let sealed = seal_completion(&keyring, &token).expect("completion seals");
        assert!(
            complete_login_state(&mut conn, &hash, &sealed, LOGIN_COMPLETION_TTL)
                .await
                .expect("completion stores")
        );
        conn.commit().await.expect("commit");

        let completions = connections(&fixture, Some(keyring));
        let wrong = completions
            .redeem_completion(
                tenant,
                LoginInitiation::Browser(Sha256Hex::digest(b"other")),
            )
            .await
            .expect_err("another binding redeems nothing");
        assert!(matches!(wrong, WyrdError::InvalidState { .. }));
        let redeemed = completions
            .redeem_completion(tenant, LoginInitiation::Browser(flow))
            .await
            .expect("the binding redeems its completion");
        assert_eq!(redeemed.token_type, TokenType::Bearer);
        assert_eq!(redeemed.access_token.expose(), "access");
        let again = completions
            .redeem_completion(tenant, LoginInitiation::Browser(flow))
            .await
            .expect_err("a completion is redeemed once");
        assert!(matches!(again, WyrdError::InvalidState { .. }));
        assert_eq!(state_rows(&fixture).await, 0);
    }
}

/// The browser destination a login returns is screened by scheme after fresh
/// discovery, independent of the address rules server fetches use.
#[cfg(test)]
mod destination_tests {
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{AddressPolicy, OidcProvider, ScreenedHttp};
    use wyrd_spec::error::WyrdError;

    use super::browser_authorization_endpoint;

    /// Discover a loopback provider whose authorization endpoint is cleartext
    /// `http`, as a provider could republish after its connection was tested.
    ///
    /// # Panics
    /// Panics when the mock provider cannot be discovered.
    async fn cleartext_provider() -> (MockServer, OidcProvider) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["RS256"],
            })))
            .mount(&server)
            .await;
        let issuer_url = Url::parse(&issuer).expect("mock issuer parses");
        let client = ScreenedHttp::allowing_internal()
            .client_for(&issuer_url)
            .await
            .expect("screened client");
        let provider = OidcProvider::discover(issuer_url, client)
            .await
            .expect("mock provider is discovered");
        (server, provider)
    }

    /// Production refuses a discovered cleartext authorization endpoint with
    /// the redacted discovery refusal, so no login state is written for it.
    ///
    /// # Panics
    /// Panics when the endpoint is accepted or refused with another error.
    #[tokio::test]
    async fn production_refuses_a_discovered_cleartext_authorization_endpoint() {
        let (_server, provider) = cleartext_provider().await;

        let error = browser_authorization_endpoint(
            provider,
            ScreenedHttp::new(AddressPolicy::BlockInternal),
        )
        .expect_err("a cleartext browser destination is refused");

        assert!(
            matches!(error, WyrdError::DiscoveryUnavailable { .. }),
            "{error:?}"
        );
    }

    /// A permissive deployment keeps its local `http` provider's endpoint.
    ///
    /// # Panics
    /// Panics when the local endpoint is refused.
    #[tokio::test]
    async fn a_permissive_deployment_keeps_a_local_http_authorization_endpoint() {
        let (server, provider) = cleartext_provider().await;

        let endpoint = browser_authorization_endpoint(provider, ScreenedHttp::allowing_internal())
            .expect("a local provider endpoint is kept");

        assert_eq!(endpoint.as_str(), format!("{}/authorize", server.uri()));
    }
}
