//! A loopback GitHub for GitHub App connection journeys.
//!
//! [`GithubAppMock`] serves the parts of GitHub's protocol Wyrd speaks — the
//! App install page, OAuth authorization with PKCE, the token exchange, the
//! App's installation reads and installation tokens, and the authorizing
//! user's installation and repository reads — on one loopback origin, with
//! the REST API under `/api/v3`. Tests script GitHub's side: installations,
//! users, the browser's signed-in GitHub user, suspension and repository
//! access, and can hold GitHub's next answer to interleave Wyrd work with
//! it. Pull requests are scripted with [`PullRequest`] and read back by
//! installation tokens; every webhook delivery is logged with its
//! `X-GitHub-Delivery` id so Wyrd can list and redeliver failed ones, as
//! GitHub's App delivery API allows. [`GithubBrowser`] is a
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
/// JWTs with. It is not a signing key.
pub const APP_PUBLIC_KEY_PEM: &str = include_str!("github_test_app_key.pub.pem");
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

/// GitHub's next answer to one route, held until the test releases it.
///
/// The mock computes the answer from GitHub's state when the request
/// arrives, signals [`Self::arrived`], then waits for [`Self::release`], so
/// the held answer reflects GitHub as it was when asked.
#[derive(Debug, Clone, Default)]
pub struct HeldAnswer {
    /// Signalled once the held request has arrived and its answer is fixed.
    arrived: Arc<tokio::sync::Notify>,
    /// Signalled to let the held answer go.
    released: Arc<tokio::sync::Notify>,
}

impl HeldAnswer {
    /// Wait until the held request has arrived and its answer is fixed.
    pub async fn arrived(&self) {
        self.arrived.notified().await;
    }

    /// Let the held answer go.
    pub fn release(&self) {
        self.released.notify_one();
    }

    /// Signal arrival and wait for release; the route handler's side.
    async fn hold(&self) {
        self.arrived.notify_one();
        self.released.notified().await;
    }
}

/// One pull request as GitHub holds it, which the test edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// Target branch.
    pub base_ref: String,
    /// Target branch commit.
    pub base_sha: String,
    /// Source branch.
    pub head_ref: String,
    /// Source branch commit.
    pub head_sha: String,
    /// Source repository; `None` once a fork is deleted.
    pub head_repository: Option<u64>,
    /// Whether it is a draft.
    pub draft: bool,
    /// Whether it is open.
    pub open: bool,
}

impl PullRequest {
    /// An open, ready pull request from `head_sha` on `feature` of
    /// `repository` into `main` at `base_sha`.
    #[must_use]
    pub fn open(repository: u64, base_sha: &str, head_sha: &str) -> Self {
        Self {
            base_ref: "main".to_owned(),
            base_sha: base_sha.to_owned(),
            head_ref: "feature".to_owned(),
            head_sha: head_sha.to_owned(),
            head_repository: Some(repository),
            draft: false,
            open: true,
        }
    }

    /// GitHub's JSON shape of the pull request numbered `number`.
    fn json(&self, repository: u64, number: u64) -> Value {
        json!({
            "number": number,
            "state": if self.open { "open" } else { "closed" },
            "draft": self.draft,
            "base": {"ref": self.base_ref, "sha": self.base_sha, "repo": {"id": repository}},
            "head": {
                "ref": self.head_ref,
                "sha": self.head_sha,
                "repo": self.head_repository.map(|id| json!({"id": id})),
            },
        })
    }
}

/// One attempt in the App's webhook delivery log.
#[derive(Debug, Clone)]
struct DeliveryAttempt {
    /// Attempt id.
    id: u64,
    /// Delivery id, shared by its redeliveries.
    guid: String,
    /// Event name.
    event: String,
    /// The signed body.
    body: String,
    /// The Wyrd base URL it was sent to.
    server_base: String,
    /// The answer's status; 0 when no answer arrived.
    status_code: u16,
    /// Whether it was a redelivery.
    redelivery: bool,
    /// When it was sent.
    delivered_at: chrono::DateTime<chrono::Utc>,
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
    /// Held next answer of the installation token route.
    token_hold: Option<HeldAnswer>,
    /// Held next answer of the App installation read.
    installation_hold: Option<HeldAnswer>,
    /// Whether the next App installation read fails with a server error.
    installation_failure: bool,
    /// Token exchanges answered with a user token.
    exchanges: usize,
    /// Pull requests by (repository id, number).
    pulls: BTreeMap<(u64, u64), PullRequest>,
    /// Held next answer of the pull request read.
    pull_hold: Option<HeldAnswer>,
    /// Whether the next pull request read fails with a server error.
    pull_failure: bool,
    /// The App's webhook delivery log, oldest first.
    deliveries: Vec<DeliveryAttempt>,
    /// Wyrd's public base URL, where redeliveries are sent.
    public_url: Option<String>,
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
        let mut state = self.state();
        state.setup_url = Some(setup);
        state.public_url = Some(public_url.trim_end_matches('/').to_owned());
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

    /// Hold GitHub's answer to the next installation token request.
    #[must_use]
    pub fn hold_next_token(&self) -> HeldAnswer {
        let hold = HeldAnswer::default();
        self.state().token_hold = Some(hold.clone());
        hold
    }

    /// Hold GitHub's answer to the next App installation read.
    #[must_use]
    pub fn hold_next_installation_read(&self) -> HeldAnswer {
        let hold = HeldAnswer::default();
        self.state().installation_hold = Some(hold.clone());
        hold
    }

    /// Fail the next App installation read with `502 Bad Gateway`, as an
    /// unavailable GitHub would.
    pub fn fail_next_installation_read(&self) {
        self.state().installation_failure = true;
    }

    /// How many authorization codes GitHub exchanged for a user token.
    #[must_use]
    pub fn exchanges(&self) -> usize {
        self.state().exchanges
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

    /// Create or replace pull request `number` of `repository`.
    pub fn set_pull_request(&self, repository: u64, number: u64, pull_request: PullRequest) {
        self.state()
            .pulls
            .insert((repository, number), pull_request);
    }

    /// Edit pull request `number` of `repository` in place.
    ///
    /// # Panics
    /// Panics when the pull request does not exist.
    pub fn edit_pull_request(
        &self,
        repository: u64,
        number: u64,
        edit: impl FnOnce(&mut PullRequest),
    ) {
        edit(
            self.state()
                .pulls
                .get_mut(&(repository, number))
                .expect("the pull request exists"),
        );
    }

    /// The `pull_request` webhook body GitHub sends for `action` on pull
    /// request `number` of `repository` under `installation`, from its
    /// current state.
    ///
    /// # Panics
    /// Panics when the pull request does not exist.
    #[must_use]
    pub fn pull_request_event(
        &self,
        installation: u64,
        repository: u64,
        number: u64,
        action: &str,
    ) -> Value {
        let state = self.state();
        let pull_request = state
            .pulls
            .get(&(repository, number))
            .expect("the pull request exists");
        json!({
            "action": action,
            "number": number,
            "pull_request": pull_request.json(repository, number),
            "repository": {"id": repository},
            "installation": {"id": installation},
        })
    }

    /// Fail the next pull request read with `502 Bad Gateway`, as an
    /// unavailable GitHub would.
    pub fn fail_next_pull_request_read(&self) {
        self.state().pull_failure = true;
    }

    /// Hold GitHub's answer to the next pull request read.
    #[must_use]
    pub fn hold_next_pull_request_read(&self) -> HeldAnswer {
        let hold = HeldAnswer::default();
        self.state().pull_hold = Some(hold.clone());
        hold
    }

    /// Record that Wyrd's answer to the latest attempt of delivery `guid`
    /// never reached GitHub, as when Wyrd stopped before answering: GitHub
    /// then shows the delivery failed.
    pub fn lose_answer(&self, guid: &str) {
        if let Some(attempt) = self
            .state()
            .deliveries
            .iter_mut()
            .rev()
            .find(|attempt| attempt.guid == guid)
        {
            attempt.status_code = 0;
        }
    }

    /// Every attempt of delivery `guid`, oldest first, as (status,
    /// redelivery).
    #[must_use]
    pub fn delivery_attempts(&self, guid: &str) -> Vec<(u16, bool)> {
        self.state()
            .deliveries
            .iter()
            .filter(|attempt| attempt.guid == guid)
            .map(|attempt| (attempt.status_code, attempt.redelivery))
            .collect()
    }

    /// Redeliver delivery `guid` as GitHub's Redeliver does: the same body
    /// and delivery id; returns the status.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when the delivery is unknown or
    /// the request cannot be sent.
    pub async fn redeliver(&self, guid: &str) -> Result<StatusCode, WyrdTestServerError> {
        let attempt = self
            .state()
            .deliveries
            .iter()
            .rev()
            .find(|attempt| attempt.guid == guid)
            .cloned()
            .ok_or_else(|| WyrdTestServerError::Io(format!("no delivery {guid}")))?;
        let base = self.redelivery_base(&attempt);
        self.attempt(&base, &attempt.event, guid, attempt.body, true)
            .await
    }

    /// Deliver `body` as an attempt of delivery `guid`, signed, and record
    /// it in the delivery log.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
    async fn attempt(
        &self,
        server_base: &str,
        event: &str,
        guid: &str,
        body: String,
        redelivery: bool,
    ) -> Result<StatusCode, WyrdTestServerError> {
        let signature = format!(
            "sha256={}",
            hex::encode(wyrd_crypt::hmac_sha256(
                WEBHOOK_SECRET.as_bytes(),
                body.as_bytes()
            ))
        );
        let answered = deliver(
            server_base,
            event,
            Some(guid),
            Some(&signature),
            body.clone(),
        )
        .await;
        let mut state = self.state();
        state.serial += 1;
        let id = state.serial;
        state.deliveries.push(DeliveryAttempt {
            id,
            guid: guid.to_owned(),
            event: event.to_owned(),
            body,
            server_base: server_base.to_owned(),
            status_code: answered.as_ref().map_or(0, StatusCode::as_u16),
            redelivery,
            delivered_at: chrono::Utc::now(),
        });
        answered
    }

    /// Where a redelivery of `attempt` is sent: Wyrd's current public URL,
    /// as GitHub sends to the App's configured webhook, else where it was
    /// first sent.
    fn redelivery_base(&self, attempt: &DeliveryAttempt) -> String {
        self.state()
            .public_url
            .clone()
            .unwrap_or_else(|| attempt.server_base.clone())
    }

    /// Deliver webhook `event` with `payload` to `server_base` as a new
    /// delivery, signed and logged; returns the status and its delivery id.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
    pub async fn deliver_new(
        &self,
        server_base: &str,
        event: &str,
        payload: &Value,
    ) -> Result<(StatusCode, String), WyrdTestServerError> {
        let guid = uuid::Uuid::new_v4().to_string();
        let status = self
            .attempt(server_base, event, &guid, payload.to_string(), false)
            .await?;
        Ok((status, guid))
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
        self.deliver_new(server_base, event, payload)
            .await
            .map(|(status, _)| status)
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
        deliver(server_base, event, None, signature, payload.to_string()).await
    }
}

/// POST one webhook delivery.
///
/// # Errors
/// Returns [`WyrdTestServerError::Io`] when the request cannot be sent.
async fn deliver(
    server_base: &str,
    event: &str,
    guid: Option<&str>,
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
    if let Some(guid) = guid {
        request = request.header("x-github-delivery", guid);
    }
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
        .route(
            "/api/v3/repositories/{id}/pulls/{number}",
            get(pull_request),
        )
        .route("/api/v3/app/hook/deliveries", get(hook_deliveries))
        .route(
            "/api/v3/app/hook/deliveries/{id}/attempts",
            post(hook_redeliver),
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
/// an `error` body, and answers form-encoded unless the request accepts
/// `application/json`.
async fn access_token(
    State(weak): State<Weak>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let json_accepted = headers.get_all(header::ACCEPT).iter().any(|value| {
        value
            .to_str()
            .is_ok_and(|accept| accept.contains("application/json"))
    });
    let answer = |body: &Value| -> Response {
        if json_accepted {
            return axum::Json(body.clone()).into_response();
        }
        let encoded = body.as_object().map_or_else(String::new, |fields| {
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(
                    fields
                        .iter()
                        .map(|(name, value)| (name, value.as_str().unwrap_or_default())),
                )
                .finish()
        });
        (
            [(header::CONTENT_TYPE, "application/x-www-form-urlencoded")],
            encoded,
        )
            .into_response()
    };
    let mut state = mock.state();
    let refused = || answer(&json!({ "error": "bad_verification_code" }));
    if form.client_id != OAUTH_CLIENT_ID || form.client_secret != OAUTH_CLIENT_SECRET {
        return answer(&json!({ "error": "incorrect_client_credentials" }));
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
    state.exchanges += 1;
    answer(&json!({ "access_token": token, "token_type": "bearer", "scope": "" }))
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
    let (answer, hold) = {
        let mut state = mock.state();
        let failed = std::mem::take(&mut state.installation_failure);
        let answer = match state.installations.get(&id) {
            _ if failed => StatusCode::BAD_GATEWAY.into_response(),
            Some(installation) if installation.app_id == APP_ID => {
                axum::Json(installation_json(id, installation)).into_response()
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        };
        (answer, state.installation_hold.take())
    };
    if let Some(hold) = hold {
        hold.hold().await;
    }
    answer
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

/// `GET /api/v3/repositories/{id}/pulls/{number}`: an installation reads
/// a pull request of a repository its token reaches.
async fn pull_request(
    State(weak): State<Weak>,
    Path((repository, number)): Path<(u64, u64)>,
    headers: HeaderMap,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    let (answer, hold) = {
        let mut state = mock.state();
        let issued = bearer(&headers).is_some_and(|token| {
            token.starts_with(INSTALLATION_TOKEN_PREFIX) && state.issued.iter().any(|t| t == token)
        });
        let reachable = state.installations.values().any(|installation| {
            installation.app_id == APP_ID
                && !installation.suspended
                && installation.repositories.contains_key(&repository)
        });
        let failed = std::mem::take(&mut state.pull_failure);
        let answer = match state.pulls.get(&(repository, number)) {
            _ if !issued => StatusCode::UNAUTHORIZED.into_response(),
            _ if failed => StatusCode::BAD_GATEWAY.into_response(),
            Some(pull_request) if reachable => {
                axum::Json(pull_request.json(repository, number)).into_response()
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        };
        (answer, state.pull_hold.take())
    };
    if let Some(hold) = hold {
        hold.hold().await;
    }
    answer
}

/// Delivery log page query.
#[derive(Deserialize)]
struct DeliveriesQuery {
    /// Page size.
    per_page: Option<usize>,
    /// Attempts already listed.
    cursor: Option<usize>,
}

/// `GET /api/v3/app/hook/deliveries`: the App's delivery log, newest first,
/// paged by cursor with a `Link: rel="next"` header.
async fn hook_deliveries(
    State(weak): State<Weak>,
    Query(query): Query<DeliveriesQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(mock) = weak.mock() else {
        return gone();
    };
    if !app_authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let per_page = query.per_page.unwrap_or(30).max(1);
    let cursor = query.cursor.unwrap_or(0);
    let state = mock.state();
    let attempts: Vec<Value> = state
        .deliveries
        .iter()
        .rev()
        .skip(cursor)
        .take(per_page)
        .map(|attempt| {
            json!({
                "id": attempt.id,
                "guid": attempt.guid,
                "status_code": attempt.status_code,
                "redelivery": attempt.redelivery,
                "delivered_at": attempt.delivered_at,
                "event": attempt.event,
            })
        })
        .collect();
    let mut response = axum::Json(attempts).into_response();
    if cursor + per_page < state.deliveries.len() {
        let next = format!(
            "<{}api/v3/app/hook/deliveries?per_page={per_page}&cursor={}>; rel=\"next\"",
            mock.origin(),
            cursor + per_page
        );
        if let Ok(value) = next.parse() {
            response.headers_mut().insert(header::LINK, value);
        }
    }
    response
}

/// `POST /api/v3/app/hook/deliveries/{id}/attempts`: the App asks GitHub
/// to redeliver attempt `id`; the mock redelivers before answering.
async fn hook_redeliver(
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
    let attempt = mock
        .state()
        .deliveries
        .iter()
        .find(|attempt| attempt.id == id)
        .cloned();
    let Some(attempt) = attempt else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let base = mock.redelivery_base(&attempt);
    match mock
        .attempt(&base, &attempt.event, &attempt.guid, attempt.body, true)
        .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
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
    let (answer, hold) = mint(&mut mock.state(), id, body);
    if let Some(hold) = hold {
        hold.hold().await;
    }
    answer
}

/// GitHub's answer to one installation token request for installation
/// `id`, and the hold on it, if any.
fn mint(state: &mut MockState, id: u64, body: Value) -> (Response, Option<HeldAnswer>) {
    state.token_requests.push(body.clone());
    let Some(installation) = state
        .installations
        .get(&id)
        .filter(|installation| installation.app_id == APP_ID)
        .cloned()
    else {
        return (StatusCode::NOT_FOUND.into_response(), None);
    };
    if installation.suspended {
        return (StatusCode::FORBIDDEN.into_response(), None);
    }
    let Ok(request) = serde_json::from_value::<TokenRequest>(body) else {
        return (StatusCode::UNPROCESSABLE_ENTITY.into_response(), None);
    };
    if request
        .repository_ids
        .iter()
        .any(|repository| !installation.repositories.contains_key(repository))
    {
        return (StatusCode::UNPROCESSABLE_ENTITY.into_response(), None);
    }
    let token = issue(state, INSTALLATION_TOKEN_PREFIX);
    (
        (
            StatusCode::CREATED,
            axum::Json(json!({ "token": token, "expires_at": "2099-01-01T00:00:00Z" })),
        )
            .into_response(),
        state.token_hold.take(),
    )
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
