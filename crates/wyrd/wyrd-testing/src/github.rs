//! A loopback GitHub for GitHub App connection journeys.
//!
//! [`GithubAppMock`] serves the parts of GitHub's protocol Wyrd speaks — the
//! App install page, OAuth authorization with PKCE, the token exchange, the
//! App's installation reads and installation tokens, and the authorizing
//! user's installation and repository reads — on one loopback origin, with
//! the REST API under `/api/v3` the way GitHub Enterprise Server serves it.
//! Tests script GitHub's side: installations, users, the browser's signed-in
//! GitHub user, suspension and repository access. [`GithubBrowser`] is a
//! browser signed in to Wyrd that follows the redirect chain across Wyrd and
//! the mock, keeping Wyrd's cookies. Webhooks are delivered signed with the
//! configured secret, or deliberately missigned.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use url::Url;
use wyrd_server::config::GithubConfig;

use crate::server::WyrdTestServerError;

/// The mock App's id.
pub const APP_ID: u64 = 4242;
/// Another App's id; its installations are never this App's.
pub const FOREIGN_APP_ID: u64 = 9999;
/// The mock App's slug.
pub const APP_SLUG: &str = "wyrd-test-app";
/// The mock App's OAuth client id.
pub const OAUTH_CLIENT_ID: &str = "Iv1.wyrdtestclient";
/// The mock App's OAuth client secret.
pub const OAUTH_CLIENT_SECRET: &str = "wyrd-test-oauth-client-secret";
/// The mock App's webhook secret.
pub const WEBHOOK_SECRET: &str = "wyrd-test-webhook-secret";
/// The mock App's RSA private key, PKCS#1 like the keys GitHub issues.
pub const APP_PRIVATE_KEY_PEM: &str = include_str!("github_test_app_key.pem");
/// The public half of [`APP_PRIVATE_KEY_PEM`], which the mock verifies App
/// JWTs with.
const APP_PUBLIC_KEY_PEM: &str = include_str!("github_test_app_key.pub.pem");
/// Prefix of every user token the mock issues.
pub const USER_TOKEN_PREFIX: &str = "gho_wyrdmock_";
/// Prefix of every installation token the mock issues.
pub const INSTALLATION_TOKEN_PREFIX: &str = "ghs_wyrdmock_";

/// One installation as GitHub holds it.
#[derive(Debug, Clone)]
struct Installation {
    /// The App it belongs to.
    app_id: u64,
    /// Whether GitHub suspended it.
    suspended: bool,
    /// Its selected repositories: id to `owner/name`.
    repositories: BTreeMap<u64, String>,
}

/// One GitHub user.
#[derive(Debug, Clone, Default)]
struct User {
    /// Installations the user can access.
    installations: BTreeSet<u64>,
    /// Repositories the user cannot access, whatever the installation.
    hidden: BTreeSet<u64>,
}

/// An issued, unexchanged authorization code.
#[derive(Debug, Clone)]
struct Code {
    /// The authorizing user.
    login: String,
    /// The S256 challenge it was issued for.
    challenge: String,
    /// The redirect URI it was issued for.
    redirect_uri: String,
}

/// Everything GitHub knows, as the test scripts it.
#[derive(Debug, Default)]
struct MockState {
    /// Installations by id.
    installations: BTreeMap<u64, Installation>,
    /// Users by login.
    users: BTreeMap<String, User>,
    /// The GitHub user the browser is signed in as.
    signed_in: Option<String>,
    /// The installation the install page selects.
    install_target: Option<u64>,
    /// Wyrd's setup URL, the App's configured post-install redirect.
    setup_url: Option<Url>,
    /// Outstanding authorization codes.
    codes: HashMap<String, Code>,
    /// User tokens to their login.
    tokens: HashMap<String, String>,
    /// Every installation token request body, in order.
    token_requests: Vec<Value>,
    /// Every code, token and secret the mock issued, for leak assertions.
    issued: Vec<String>,
    /// Counter for unique codes and tokens.
    serial: u64,
}

/// The shared mock owner.
#[derive(Debug)]
struct Inner {
    /// The loopback origin.
    origin: Url,
    /// GitHub's state.
    state: Mutex<MockState>,
    /// The serve task, aborted when the last handle drops.
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Inner {
    /// Stop serving.
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A loopback GitHub serving one App; cheap to clone.
#[derive(Debug, Clone)]
pub struct GithubAppMock {
    /// Shared owner.
    inner: Arc<Inner>,
}

impl GithubAppMock {
    /// Bind a loopback port and serve the mock on the shared Wyrd runtime.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Bind`] when no port can be bound.
    pub fn start() -> Result<Self, WyrdTestServerError> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| WyrdTestServerError::Bind(error.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| WyrdTestServerError::Bind(error.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|error| WyrdTestServerError::Bind(error.to_string()))?;
        let origin = Url::parse(&format!("http://{address}"))
            .map_err(|error| WyrdTestServerError::Bind(error.to_string()))?;
        let inner = Arc::new_cyclic(|weak: &std::sync::Weak<Inner>| {
            let router = router(weak.clone());
            let task = wyrd_runtime::runtime().spawn(async move {
                match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => {
                        if let Err(error) = axum::serve(listener, router).await {
                            tracing::error!(%error, "the GitHub mock stopped serving");
                        }
                    }
                    Err(error) => tracing::error!(%error, "the GitHub mock could not listen"),
                }
            });
            Inner {
                origin,
                state: Mutex::default(),
                task,
            }
        });
        Ok(Self { inner })
    }

    /// GitHub's state, recovering from a panicked holder.
    fn state(&self) -> MutexGuard<'_, MockState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The mock's web origin.
    #[must_use]
    pub fn origin(&self) -> &Url {
        &self.inner.origin
    }

    /// The host Wyrd reports in its connection view.
    #[must_use]
    pub fn host(&self) -> String {
        format!(
            "{}:{}",
            self.inner.origin.host_str().unwrap_or_default(),
            self.inner.origin.port().unwrap_or_default()
        )
    }

    /// A complete server configuration for this App, with `public_url`
    /// left for the test server to fill from its bound address.
    #[must_use]
    pub fn server_config(&self) -> GithubConfig {
        let origin = self.inner.origin.as_str().trim_end_matches('/');
        GithubConfig {
            app_id: Some(APP_ID),
            slug: Some(APP_SLUG.to_owned()),
            web_origin: Some(origin.to_owned()),
            api_origin: Some(format!("{origin}/api/v3")),
            public_url: None,
            oauth_client_id: Some(OAUTH_CLIENT_ID.to_owned()),
            private_key: Some(SecretString::from(APP_PRIVATE_KEY_PEM)),
            webhook_secret: Some(SecretString::from(WEBHOOK_SECRET)),
            oauth_client_secret: Some(SecretString::from(OAUTH_CLIENT_SECRET)),
        }
    }

    /// Record Wyrd's public base URL, which makes the install page redirect
    /// to its setup route.
    ///
    /// # Panics
    /// Panics when `public_url` joined with the setup path is not a URL.
    pub fn set_public_url(&self, public_url: &str) {
        let setup = Url::parse(&format!(
            "{}/v1/integrations/github/setup",
            public_url.trim_end_matches('/')
        ))
        .expect("the test server's public URL is a URL");
        self.state().setup_url = Some(setup);
    }

    /// Install this App as `id` with `repositories` (id, `owner/name`)
    /// selected, replacing any installation `id`.
    pub fn add_installation(&self, id: u64, repositories: &[(u64, &str)]) {
        self.insert_installation(id, APP_ID, repositories);
    }

    /// Install another App as `id`.
    pub fn add_foreign_installation(&self, id: u64) {
        self.insert_installation(id, FOREIGN_APP_ID, &[]);
    }

    /// Insert one installation.
    fn insert_installation(&self, id: u64, app_id: u64, repositories: &[(u64, &str)]) {
        self.state().installations.insert(
            id,
            Installation {
                app_id,
                suspended: false,
                repositories: repositories
                    .iter()
                    .map(|(id, name)| (*id, (*name).to_owned()))
                    .collect(),
            },
        );
    }

    /// Create GitHub user `login` with access to `installations`.
    pub fn add_user(&self, login: &str, installations: &[u64]) {
        self.state().users.insert(
            login.to_owned(),
            User {
                installations: installations.iter().copied().collect(),
                hidden: BTreeSet::new(),
            },
        );
    }

    /// Deny user `login` repository `repository_id`.
    pub fn hide_repository(&self, login: &str, repository_id: u64) {
        if let Some(user) = self.state().users.get_mut(login) {
            user.hidden.insert(repository_id);
        }
    }

    /// Sign the browser in to GitHub as `login`.
    pub fn sign_in(&self, login: &str) {
        self.state().signed_in = Some(login.to_owned());
    }

    /// Sign the browser out of GitHub, so OAuth authorization stops there.
    pub fn sign_out(&self) {
        self.state().signed_in = None;
    }

    /// Make the install page select installation `id`.
    pub fn install_target(&self, id: u64) {
        self.state().install_target = Some(id);
    }

    /// Set GitHub's suspension of installation `id`.
    pub fn set_suspended(&self, id: u64, suspended: bool) {
        if let Some(installation) = self.state().installations.get_mut(&id) {
            installation.suspended = suspended;
        }
    }

    /// Deselect repository `repository_id` from installation `id` on GitHub.
    pub fn deselect_repository(&self, id: u64, repository_id: u64) {
        if let Some(installation) = self.state().installations.get_mut(&id) {
            installation.repositories.remove(&repository_id);
        }
    }

    /// Every installation token request body Wyrd sent, in order.
    #[must_use]
    pub fn token_requests(&self) -> Vec<Value> {
        self.state().token_requests.clone()
    }

    /// Every code and token the mock issued plus the App's secrets: none of
    /// them may appear in anything Wyrd returns.
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        let mut secrets = self.state().issued.clone();
        secrets
            .extend([OAUTH_CLIENT_SECRET, WEBHOOK_SECRET, APP_PRIVATE_KEY_PEM].map(str::to_owned));
        secrets
    }

    /// Deliver webhook `event` with `payload` to `server_base`, signed with
    /// the configured secret; returns the status.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
    pub async fn deliver(
        &self,
        server_base: &str,
        event: &str,
        payload: &Value,
    ) -> Result<StatusCode, WyrdTestServerError> {
        let body = payload.to_string();
        let signature = format!(
            "sha256={}",
            hex::encode(wyrd_crypt::hmac_sha256(
                WEBHOOK_SECRET.as_bytes(),
                body.as_bytes()
            ))
        );
        deliver(server_base, event, Some(&signature), body).await
    }

    /// Deliver webhook `event` with `payload` carrying `signature` instead
    /// of a valid one; returns the status.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
    pub async fn deliver_with_signature(
        &self,
        server_base: &str,
        event: &str,
        payload: &Value,
        signature: Option<&str>,
    ) -> Result<StatusCode, WyrdTestServerError> {
        deliver(server_base, event, signature, payload.to_string()).await
    }
}

/// POST one webhook delivery.
///
/// # Errors
/// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
async fn deliver(
    server_base: &str,
    event: &str,
    signature: Option<&str>,
    body: String,
) -> Result<StatusCode, WyrdTestServerError> {
    let mut request = reqwest::Client::new()
        .post(format!(
            "{}/v1/integrations/github/webhook",
            server_base.trim_end_matches('/')
        ))
        .header("x-github-event", event)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body);
    if let Some(signature) = signature {
        request = request.header("x-hub-signature-256", signature);
    }
    let response = request
        .send()
        .await
        .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
    Ok(StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
}

/// The mock's routes over a weak handle to its owner.
fn router(inner: std::sync::Weak<Inner>) -> Router {
    Router::new()
        .route("/apps/{slug}/installations/new", get(install_page))
        .route("/login/oauth/authorize", get(authorize))
        .route("/login/oauth/access_token", post(access_token))
        .route("/api/v3/app/installations/{id}", get(app_installation))
        .route(
            "/api/v3/app/installations/{id}/access_tokens",
            post(installation_token),
        )
        .route("/api/v3/user/installations", get(user_installations))
        .route(
            "/api/v3/user/installations/{id}/repositories",
            get(user_repositories),
        )
        .with_state(Weak(inner))
}

/// Route state: the owner, which outlives every request it serves.
#[derive(Clone)]
struct Weak(std::sync::Weak<Inner>);

impl Weak {
    /// The mock, or `None` while it is being dropped.
    fn mock(&self) -> Option<GithubAppMock> {
        self.0.upgrade().map(|inner| GithubAppMock { inner })
    }
}

/// A response for a mock that is shutting down.
fn gone() -> Response {
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

/// A fresh unique value with `prefix`, recorded as issued.
fn issue(state: &mut MockState, prefix: &str) -> String {
    state.serial += 1;
    let value = format!("{prefix}{}_{}", state.serial, uuid::Uuid::now_v7().simple());
    state.issued.push(value.clone());
    value
}

/// Install page query.
#[derive(Deserialize)]
struct InstallQuery {
    /// Wyrd's transaction state, echoed to setup.
    state: Option<String>,
}

/// `GET /apps/{slug}/installations/new`: the user installs or reconfigures
/// the App, and GitHub redirects to the App's setup URL.
async fn install_page(
    State(weak): State<Weak>,
    Path(slug): Path<String>,
    Query(query): Query<InstallQuery>,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let state = mock.state();
    let (Some(mut setup), Some(target)) = (state.setup_url.clone(), state.install_target) else {
        return (StatusCode::NOT_FOUND, "no install target scripted").into_response();
    };
    if slug != APP_SLUG {
        return StatusCode::NOT_FOUND.into_response();
    }
    {
        let mut pairs = setup.query_pairs_mut();
        pairs
            .append_pair("installation_id", &target.to_string())
            .append_pair("setup_action", "install");
        if let Some(value) = &query.state {
            pairs.append_pair("state", value);
        }
    }
    Redirect::to(setup.as_str()).into_response()
}

/// OAuth authorization query.
#[derive(Deserialize)]
struct AuthorizeQuery {
    /// The App's client id.
    client_id: String,
    /// Where to return the code.
    redirect_uri: String,
    /// Wyrd's state, echoed.
    state: String,
    /// S256 challenge.
    code_challenge: String,
    /// Must be `S256`.
    code_challenge_method: String,
}

/// `GET /login/oauth/authorize`: the signed-in GitHub user authorizes the
/// App and GitHub redirects back with a code.
async fn authorize(State(weak): State<Weak>, Query(query): Query<AuthorizeQuery>) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let mut state = mock.state();
    if query.client_id != OAUTH_CLIENT_ID || query.code_challenge_method != "S256" {
        return (
            StatusCode::BAD_REQUEST,
            "unknown client or challenge method",
        )
            .into_response();
    }
    let Some(login) = state.signed_in.clone() else {
        return (StatusCode::UNAUTHORIZED, "no GitHub user signed in").into_response();
    };
    let Ok(mut redirect) = Url::parse(&query.redirect_uri) else {
        return (StatusCode::BAD_REQUEST, "redirect_uri").into_response();
    };
    let code = issue(&mut state, "code_");
    state.codes.insert(
        code.clone(),
        Code {
            login,
            challenge: query.code_challenge,
            redirect_uri: query.redirect_uri,
        },
    );
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &query.state);
    Redirect::to(redirect.as_str()).into_response()
}

/// Token exchange form.
#[derive(Deserialize)]
struct TokenForm {
    /// The App's client id.
    client_id: String,
    /// The App's client secret.
    client_secret: String,
    /// The authorization code.
    code: String,
    /// The redirect URI the code was issued for.
    redirect_uri: Option<String>,
    /// The PKCE verifier.
    code_verifier: String,
}

/// `POST /login/oauth/access_token`: a one-use code, the client secret and
/// the PKCE verifier buy a user token. GitHub answers refusals with 200 and
/// an `error` body.
async fn access_token(State(weak): State<Weak>, Form(form): Form<TokenForm>) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let mut state = mock.state();
    let refused = || axum::Json(json!({ "error": "bad_verification_code" })).into_response();
    if form.client_id != OAUTH_CLIENT_ID || form.client_secret != OAUTH_CLIENT_SECRET {
        return axum::Json(json!({ "error": "incorrect_client_credentials" })).into_response();
    }
    let Some(code) = state.codes.remove(&form.code) else {
        return refused();
    };
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(form.code_verifier.as_bytes()));
    if challenge != code.challenge
        || form
            .redirect_uri
            .is_some_and(|uri| uri != code.redirect_uri)
    {
        return refused();
    }
    let token = issue(&mut state, USER_TOKEN_PREFIX);
    state.tokens.insert(token.clone(), code.login);
    axum::Json(json!({ "access_token": token, "token_type": "bearer", "scope": "" }))
        .into_response()
}

/// The bearer token of `headers`.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Whether `headers` carry a valid RS256 App JWT for [`APP_ID`].
fn app_authenticated(headers: &HeaderMap) -> bool {
    /// The claims GitHub checks.
    #[derive(Deserialize)]
    struct Claims {
        /// The App id.
        iss: String,
    }
    let (Some(token), Ok(key)) = (
        bearer(headers),
        jsonwebtoken::DecodingKey::from_rsa_pem(APP_PUBLIC_KEY_PEM.as_bytes()),
    ) else {
        return false;
    };
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_required_spec_claims(&["exp", "iat", "iss"]);
    jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .is_ok_and(|data| data.claims.iss == APP_ID.to_string())
}

/// The user behind `headers`' token.
fn user(state: &MockState, headers: &HeaderMap) -> Option<(String, User)> {
    let login = state.tokens.get(bearer(headers)?)?;
    Some((login.clone(), state.users.get(login)?.clone()))
}

/// GitHub's JSON shape of installation `id`.
fn installation_json(id: u64, installation: &Installation) -> Value {
    json!({
        "id": id,
        "app_id": installation.app_id,
        "suspended_at": installation.suspended.then_some("2026-01-01T00:00:00Z"),
    })
}

/// `GET /api/v3/app/installations/{id}`: the App reads one of its
/// installations.
async fn app_installation(
    State(weak): State<Weak>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    if !app_authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = mock.state();
    match state.installations.get(&id) {
        Some(installation) if installation.app_id == APP_ID => {
            axum::Json(installation_json(id, installation)).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /api/v3/user/installations`: installations the user can access,
/// across Apps.
async fn user_installations(State(weak): State<Weak>, headers: HeaderMap) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let state = mock.state();
    let Some((_, user)) = user(&state, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let installations: Vec<Value> = user
        .installations
        .iter()
        .filter_map(|id| {
            state
                .installations
                .get(id)
                .map(|installation| installation_json(*id, installation))
        })
        .collect();
    axum::Json(json!({ "total_count": installations.len(), "installations": installations }))
        .into_response()
}

/// `GET /api/v3/user/installations/{id}/repositories`: the installation's
/// selected repositories the user can access.
async fn user_repositories(
    State(weak): State<Weak>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let state = mock.state();
    let Some((_, user)) = user(&state, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(installation) = state
        .installations
        .get(&id)
        .filter(|_| user.installations.contains(&id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let repositories: Vec<Value> = installation
        .repositories
        .iter()
        .filter(|(repository, _)| !user.hidden.contains(repository))
        .map(|(repository, name)| json!({ "id": repository, "full_name": name }))
        .collect();
    axum::Json(json!({ "total_count": repositories.len(), "repositories": repositories }))
        .into_response()
}

/// Installation token request body.
#[derive(Deserialize)]
struct TokenRequest {
    /// Repositories the token is narrowed to.
    #[serde(default)]
    repository_ids: Vec<u64>,
}

/// `POST /api/v3/app/installations/{id}/access_tokens`: the App mints a
/// narrowed installation token.
async fn installation_token(
    State(weak): State<Weak>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    if !app_authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut state = mock.state();
    state.token_requests.push(body.clone());
    let Some(installation) = state
        .installations
        .get(&id)
        .filter(|installation| installation.app_id == APP_ID)
        .cloned()
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if installation.suspended {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(request) = serde_json::from_value::<TokenRequest>(body) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    if request
        .repository_ids
        .iter()
        .any(|repository| !installation.repositories.contains_key(repository))
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    let token = issue(&mut state, INSTALLATION_TOKEN_PREFIX);
    (
        StatusCode::CREATED,
        axum::Json(json!({ "token": token, "expires_at": "2099-01-01T00:00:00Z" })),
    )
        .into_response()
}

/// The final answer of a browser navigation.
#[derive(Debug, Clone)]
pub struct BrowserAnswer {
    /// Final HTTP status.
    pub status: u16,
    /// Final body, parsed as JSON when it is JSON.
    pub body: Value,
    /// Every URL visited, in order, including the final one.
    pub visited: Vec<Url>,
}

impl BrowserAnswer {
    /// The `code` of a problem answer.
    #[must_use]
    pub fn problem_code(&self) -> Option<&str> {
        self.body.get("code").and_then(Value::as_str)
    }

    /// The `details.reason` of a problem answer.
    #[must_use]
    pub fn problem_reason(&self) -> Option<&str> {
        self.body
            .get("details")
            .and_then(|details| details.get("reason"))
            .and_then(Value::as_str)
    }

    /// The last Wyrd callback URL visited, for replay.
    #[must_use]
    pub fn callback_url(&self) -> Option<&Url> {
        self.visited
            .iter()
            .rev()
            .find(|url| url.path() == "/v1/integrations/github/callback")
    }
}

/// A browser signed in to one Wyrd tenant, following redirects across Wyrd
/// and the GitHub mock and keeping Wyrd's cookies by path.
#[derive(Debug)]
pub struct GithubBrowser {
    /// Redirect-free client; the browser follows hops itself.
    http: reqwest::Client,
    /// Wyrd's origin.
    wyrd: Url,
    /// Wyrd cookies: name to (value, path).
    cookies: BTreeMap<String, (String, String)>,
}

/// Most redirects one navigation follows.
const MAX_HOPS: usize = 10;

impl GithubBrowser {
    /// A browser at `server_base` signed in with `api_key` through the
    /// production browser API-key sign-in for tenant `tenant`.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Auth`] when sign-in refuses or sets no
    /// session cookie, or [`WyrdTestServerError::Io`] when it cannot be sent.
    pub async fn sign_in(
        server_base: &str,
        tenant: &str,
        api_key: &str,
    ) -> Result<Self, WyrdTestServerError> {
        let wyrd = Url::parse(server_base.trim_end_matches('/'))
            .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
        let mut browser = Self {
            http,
            wyrd,
            cookies: BTreeMap::new(),
        };
        let origin = browser.wyrd.origin().ascii_serialization();
        let response = browser
            .http
            .post(browser.wyrd.join("/auth/browser/api-key").map_err(io)?)
            .header(header::ORIGIN, origin)
            .json(&json!({ "tenant": tenant, "api_key": api_key }))
            .send()
            .await
            .map_err(io)?;
        let status = response.status();
        browser.keep_cookies(response.headers());
        if !status.is_success()
            || !browser
                .cookies
                .keys()
                .any(|name| name.starts_with("wyrd_session_"))
        {
            return Err(WyrdTestServerError::Auth(format!(
                "browser sign-in answered {status}"
            )));
        }
        Ok(browser)
    }

    /// Keep every `Set-Cookie` of a Wyrd response; `Max-Age=0` deletes.
    fn keep_cookies(&mut self, headers: &reqwest::header::HeaderMap) {
        for value in headers
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
        {
            let mut parts = value.split(';').map(str::trim);
            let Some((name, cookie)) = parts.next().and_then(|pair| pair.split_once('=')) else {
                continue;
            };
            let mut path = "/".to_owned();
            let mut expired = false;
            for attribute in parts {
                if let Some(value) = attribute.strip_prefix("Path=") {
                    value.clone_into(&mut path);
                }
                if attribute == "Max-Age=0" {
                    expired = true;
                }
            }
            if expired {
                self.cookies.remove(name);
            } else {
                self.cookies
                    .insert(name.to_owned(), (cookie.to_owned(), path));
            }
        }
    }

    /// The `Cookie` header for a Wyrd `path`.
    fn cookie_header(&self, path: &str) -> String {
        self.cookies
            .iter()
            .filter(|(_, (_, cookie_path))| path.starts_with(cookie_path.as_str()))
            .map(|(name, (value, _))| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Copy this browser's Wyrd cookie `name` from `other`, as when one
    /// browser's transaction cookie is stolen into another.
    pub fn copy_cookie_from(&mut self, other: &Self, name: &str) {
        if let Some(cookie) = other.cookies.get(name) {
            self.cookies.insert(name.to_owned(), cookie.clone());
        }
    }

    /// Whether this browser holds Wyrd cookie `name`.
    #[must_use]
    pub fn has_cookie(&self, name: &str) -> bool {
        self.cookies.contains_key(name)
    }

    /// Navigate to `url` and follow redirects until a final answer.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when a hop cannot be sent or the
    /// chain is longer than ten redirects.
    pub async fn open(&mut self, url: &str) -> Result<BrowserAnswer, WyrdTestServerError> {
        let mut current = Url::parse(url).map_err(io)?;
        let mut visited = Vec::new();
        for _ in 0..MAX_HOPS {
            visited.push(current.clone());
            let mut request = self.http.get(current.clone());
            let wyrd = current.origin() == self.wyrd.origin();
            if wyrd {
                let cookies = self.cookie_header(current.path());
                if !cookies.is_empty() {
                    request = request.header(header::COOKIE, cookies);
                }
            }
            let response = request.send().await.map_err(io)?;
            if wyrd {
                self.keep_cookies(response.headers());
            }
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        WyrdTestServerError::Io("redirect without Location".to_owned())
                    })?;
                current = current.join(location).map_err(io)?;
                continue;
            }
            let text = response.text().await.map_err(io)?;
            let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
            return Ok(BrowserAnswer {
                status: status.as_u16(),
                body,
                visited,
            });
        }
        Err(WyrdTestServerError::Io("too many redirects".to_owned()))
    }
}

/// An I/O failure of the browser.
fn io(error: impl std::fmt::Display) -> WyrdTestServerError {
    WyrdTestServerError::Io(error.to_string())
}
