//! ExternalSecret journeys against a real `vault server -dev`.
//!
//! The KV v2 status mapping is pinned by unit tests against a mock Vault; what
//! only a real server can prove is the rest of REQ-027A–D: that the gateway's
//! one read-secret-version request actually reads a real KV v2 secret, that a
//! rotation, soft delete, or destroy applies to the very next admitted call
//! because nothing is cached, and that every unresolved outcome fails closed
//! before any provider is dispatched.
//!
//! The dev server holds its secrets in memory and is torn down with the test,
//! so no credential outlives the process or reaches a fixture.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde_json::{Value, json};
use url::Url;
use wyrd_testing::server::{
    TEST_GATEWAY_SECRET_BACKEND, TEST_GATEWAY_VAULT_TOKEN_VARIABLE, WyrdTestServer,
};

use crate::harness::token;

/// Root token the dev server is launched with.
///
/// The value is a fixed test constant, never an operator credential, and the
/// lane exports it as [`TEST_GATEWAY_VAULT_TOKEN_VARIABLE`] so no test mutates
/// the process environment.
const ROOT_TOKEN: &str = "wyrd-test-vault-root";

/// Variable holding a syntactically valid token Vault does not accept.
const DENIED_TOKEN_VARIABLE: &str = "WYRD_TEST_GATEWAY_VAULT_TOKEN_DENIED";

/// KV v2 path, within the harness's assigned `openai` prefix, and the key the
/// journey's credential reference points at.
const SECRET_PATH: &str = "openai/journey";

/// Provider key the first secret version carries.
const FIRST_KEY: &str = "sk-vault-first";

/// Provider key the rotated secret version carries.
const ROTATED_KEY: &str = "sk-vault-rotated";

/// Bound the dev server's listener must become healthy within.
const READY_BOUND: Duration = Duration::from_secs(30);

/// A `vault server -dev` child process and its address.
///
/// Dropping the handle kills the child, so a panicking assertion never leaves
/// a listener behind for the next run to collide with. The server is launched
/// with `-dev-no-store-token` so concurrent journeys never contend over the
/// shared `~/.vault-token` helper file.
struct DevVault {
    /// The running dev server.
    child: Child,
    /// Root of the dev server's HTTP API.
    address: Url,
    /// Client used for KV v2 administration.
    http: Client,
}

impl Drop for DevVault {
    /// Kills the dev server, ignoring a child that already exited.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl DevVault {
    /// Launches a dev server on a free loopback port and waits for its health
    /// endpoint to report an unsealed, initialized node.
    ///
    /// # Panics
    ///
    /// Panics when no free port can be reserved, the `vault` binary is not on
    /// `PATH`, or the listener is not healthy within [`READY_BOUND`].
    async fn start() -> Self {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
            listener.local_addr().expect("the reserved address").port()
        };
        let listen = format!("127.0.0.1:{port}");
        let child = Command::new("vault")
            .args([
                "server",
                "-dev",
                "-dev-no-store-token",
                &format!("-dev-root-token-id={ROOT_TOKEN}"),
                &format!("-dev-listen-address={listen}"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("`vault` is on PATH; the lane installs it through mise");
        let address = Url::parse(&format!("http://{listen}")).expect("the dev server url");
        let vault = Self {
            child,
            address,
            http: Client::new(),
        };
        vault.await_ready().await;
        vault
    }

    /// Polls `sys/health` until the dev server answers, bounded by
    /// [`READY_BOUND`].
    ///
    /// # Panics
    ///
    /// Panics when the server is still unhealthy at the bound.
    async fn await_ready(&self) {
        let deadline = Instant::now() + READY_BOUND;
        let health = self
            .address
            .join("v1/sys/health")
            .expect("the health route");
        while Instant::now() < deadline {
            if let Ok(response) = self.http.get(health.clone()).send().await
                && response.status().is_success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the dev Vault server was not healthy within {READY_BOUND:?}");
    }

    /// Sends one root-authenticated KV v2 request and returns its status.
    ///
    /// # Panics
    ///
    /// Panics when the request cannot be sent.
    async fn kv(&self, method: reqwest::Method, route: &str, body: Value) -> reqwest::StatusCode {
        let url = self.address.join(route).expect("a KV v2 route");
        self.http
            .request(method, url)
            .header("x-vault-token", ROOT_TOKEN)
            .json(&body)
            .send()
            .await
            .expect("the dev server answers")
            .status()
    }

    /// Writes a new latest version of [`SECRET_PATH`] carrying `key`.
    ///
    /// # Panics
    ///
    /// Panics when the write is refused.
    async fn write_secret(&self, key: &str) {
        let status = self
            .kv(
                reqwest::Method::POST,
                &format!("v1/secret/data/{SECRET_PATH}"),
                json!({"data": {"api_key": key}}),
            )
            .await;
        assert!(status.is_success(), "KV v2 write refused: {status}");
    }

    /// Soft-deletes `version`, leaving recoverable metadata behind.
    ///
    /// # Panics
    ///
    /// Panics when the delete is refused.
    async fn soft_delete(&self, version: u32) {
        let status = self
            .kv(
                reqwest::Method::POST,
                &format!("v1/secret/delete/{SECRET_PATH}"),
                json!({"versions": [version]}),
            )
            .await;
        assert!(status.is_success(), "KV v2 soft delete refused: {status}");
    }

    /// Permanently destroys `version`.
    ///
    /// # Panics
    ///
    /// Panics when the destroy is refused.
    async fn destroy(&self, version: u32) {
        let status = self
            .kv(
                reqwest::Method::POST,
                &format!("v1/secret/destroy/{SECRET_PATH}"),
                json!({"versions": [version]}),
            )
            .await;
        assert!(status.is_success(), "KV v2 destroy refused: {status}");
    }
}

/// One bound server whose Vault backend resolves against `vault`, its mock
/// provider upstream, and an admin caller.
struct VaultJourney {
    /// Bound server under test, held so it stays up for the whole journey.
    _server: WyrdTestServer,
    /// Mock OpenAI upstream the deployment dispatches to.
    upstream: wiremock::MockServer,
    /// Server base URL.
    base: String,
    /// Access token of a principal allowed to administer and invoke.
    admin: String,
    /// Plain HTTP client standing in for the SDK transport.
    http: Client,
}

impl VaultJourney {
    /// Boots a server whose Vault backend reads `token_variable` at `address`,
    /// with a mock OpenAI upstream that echoes a fixed completion.
    ///
    /// # Panics
    ///
    /// Panics when startup, bootstrap, or token exchange fails.
    async fn start(address: Option<&Url>, token_variable: &str) -> Self {
        let upstream = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-vault",
                "object": "chat.completion",
                "created": 1,
                "model": "gpt-4o",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
                "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
            })))
            .mount(&upstream)
            .await;
        let root = Url::parse(&upstream.uri()).expect("the mock url");
        let mut builder = WyrdTestServer::builder().with_gateway_provider_root_for_test(root);
        if let Some(address) = address {
            builder = builder.with_gateway_vault_backend_for_test(address.clone(), token_variable);
        }
        let server = Box::pin(builder.start_bound())
            .await
            .expect("server starts");
        let base = server.base_url().expect("bound url").to_owned();
        let admin = token(&server, "vault_admin", &["admin"]).await;
        Self {
            _server: server,
            upstream,
            base,
            admin,
            http: Client::new(),
        }
    }

    /// Puts one administration document at `route`, returning the response.
    ///
    /// # Panics
    ///
    /// Panics when the request cannot be sent.
    async fn put(&self, route: &str, body: Value) -> reqwest::Response {
        self.http
            .put(format!("{}/v1/admin/gateway/{route}", self.base))
            .header("x-wyrd-access-token", format!("Bearer {}", self.admin))
            .json(&body)
            .send()
            .await
            .expect("the admin request sends")
    }

    /// Stores an ExternalSecret credential over `reference` in `backend` and
    /// one bearer-authenticated OpenAI deployment using it.
    ///
    /// # Panics
    ///
    /// Panics when either administration write is refused.
    async fn deploy_external_secret(&self, backend: &str, reference: &str) {
        let credential = self
            .put(
                "provider-credentials/vault-openai",
                json!({
                    "name": "vault-openai",
                    "provider": "openai",
                    "source": {"external_secret": {"backend": backend, "reference": reference}},
                }),
            )
            .await;
        assert!(
            credential.status().is_success(),
            "credential write refused: {}",
            credential.text().await.unwrap_or_default()
        );
        let deployment = self
            .put(
                "provider-deployments/gpt-4o",
                json!({
                    "name": "gpt-4o",
                    "model": {"provider": "openai", "model": "gpt-4o"},
                    "adapter": "openai",
                    "auth": {"bearer": {"credential": "vault-openai"}},
                    "capabilities": ["chat_completions"],
                    "routing_weight": 1,
                }),
            )
            .await;
        assert!(
            deployment.status().is_success(),
            "deployment write refused: {}",
            deployment.text().await.unwrap_or_default()
        );
    }

    /// Calls the OpenAI-compatible ingress once as the admin caller.
    ///
    /// # Panics
    ///
    /// Panics when the request cannot be sent.
    async fn call(&self) -> reqwest::Response {
        self.http
            .post(format!("{}/v1/chat/completions", self.base))
            .header("authorization", format!("Bearer {}", self.admin))
            .json(&json!({
                "model": "openai/gpt-4o",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await
            .expect("the ingress request sends")
    }

    /// Bearer credentials every upstream request carried, in order.
    ///
    /// # Panics
    ///
    /// Panics when the mock is not recording requests.
    async fn upstream_bearers(&self) -> Vec<String> {
        self.upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .iter()
            .map(|request| {
                request
                    .headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }
}

/// Proves a real Vault KV v2 secret authenticates a provider call, that a
/// rotation applies to the next call with no cache, and that a soft-deleted
/// and then destroyed latest version fails the call closed.
///
/// Together these are the REQ-027B and REQ-027D outcomes that only a real
/// server can show: the gateway reads the latest version through one
/// read-secret-version request and re-reads it per resolution, so the
/// credential in flight is always the one Vault holds now.
///
/// # Panics
///
/// Panics when a status, upstream credential, or dispatch expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle and a `vault` binary"]
async fn vault_credentials_rotate_and_fail_closed_when_the_latest_version_goes_away() {
    assert_eq!(
        std::env::var(TEST_GATEWAY_VAULT_TOKEN_VARIABLE).as_deref(),
        Ok(ROOT_TOKEN),
        "run through `mise run test:gateway:vault`, which exports the dev root token"
    );
    let vault = DevVault::start().await;
    vault.write_secret(FIRST_KEY).await;
    let journey =
        VaultJourney::start(Some(&vault.address), TEST_GATEWAY_VAULT_TOKEN_VARIABLE).await;
    journey
        .deploy_external_secret(
            TEST_GATEWAY_SECRET_BACKEND,
            &format!("{SECRET_PATH}#api_key"),
        )
        .await;

    let first = journey.call().await;
    assert_eq!(first.status().as_u16(), 200, "the first version resolves");
    assert_eq!(
        journey.upstream_bearers().await,
        vec![format!("Bearer {FIRST_KEY}")],
        "the provider receives the value Vault holds, never a caller token"
    );

    vault.write_secret(ROTATED_KEY).await;
    let rotated = journey.call().await;
    assert_eq!(rotated.status().as_u16(), 200);
    assert_eq!(
        journey.upstream_bearers().await,
        vec![
            format!("Bearer {FIRST_KEY}"),
            format!("Bearer {ROTATED_KEY}"),
        ],
        "the next call reads Vault again and carries the rotated value"
    );

    vault.soft_delete(2).await;
    let soft_deleted = journey.call().await;
    assert_eq!(
        soft_deleted.status().as_u16(),
        502,
        "a soft-deleted latest version fails the call closed"
    );
    assert_eq!(
        journey.upstream_bearers().await.len(),
        2,
        "an unresolved credential reaches no provider"
    );

    vault.destroy(2).await;
    let destroyed = journey.call().await;
    assert_eq!(
        destroyed.status().as_u16(),
        502,
        "a destroyed latest version fails the call closed"
    );
    assert_eq!(
        journey.upstream_bearers().await.len(),
        2,
        "an unresolved credential reaches no provider"
    );
}

/// Proves a denied token and an unreachable Vault both fail the call closed,
/// and that an invalid reference and an undeclared backend are refused at
/// administration time.
///
/// REQ-027A and REQ-027C put the reference grammar and backend declaration on
/// the write path, so a bad credential never reaches dispatch at all; REQ-027D
/// keeps every unresolved read fail-closed without telling the caller which
/// Vault outcome occurred.
///
/// # Panics
///
/// Panics when a status, refusal code, or dispatch expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle and a `vault` binary"]
async fn vault_refuses_bad_references_and_fails_closed_on_denied_or_unreachable_reads() {
    let vault = DevVault::start().await;
    vault.write_secret(FIRST_KEY).await;

    let denied = VaultJourney::start(Some(&vault.address), DENIED_TOKEN_VARIABLE).await;
    denied
        .deploy_external_secret(
            TEST_GATEWAY_SECRET_BACKEND,
            &format!("{SECRET_PATH}#api_key"),
        )
        .await;
    assert_eq!(
        denied.call().await.status().as_u16(),
        502,
        "a denied Vault token fails the call closed"
    );
    assert!(
        denied.upstream_bearers().await.is_empty(),
        "a denied token reaches no provider"
    );

    let unreachable = VaultJourney::start(None, TEST_GATEWAY_VAULT_TOKEN_VARIABLE).await;
    unreachable
        .deploy_external_secret(
            TEST_GATEWAY_SECRET_BACKEND,
            &format!("{SECRET_PATH}#api_key"),
        )
        .await;
    assert_eq!(
        unreachable.call().await.status().as_u16(),
        502,
        "an unreachable Vault fails the call closed"
    );
    assert!(
        unreachable.upstream_bearers().await.is_empty(),
        "an unreachable Vault reaches no provider"
    );

    for reference in [
        "no-key-separator",
        "openai//journey#api_key",
        "openai/../journey#api_key",
        "openai/journey#",
    ] {
        let refused = unreachable
            .put(
                "provider-credentials/bad-reference",
                json!({
                    "name": "bad-reference",
                    "provider": "openai",
                    "source": {
                        "external_secret": {
                            "backend": TEST_GATEWAY_SECRET_BACKEND,
                            "reference": reference,
                        }
                    },
                }),
            )
            .await;
        assert_eq!(
            refused.status().as_u16(),
            400,
            "`{reference}` is not a valid KV v2 reference"
        );
    }

    let undeclared = unreachable
        .put(
            "provider-credentials/undeclared-backend",
            json!({
                "name": "undeclared-backend",
                "provider": "openai",
                "source": {
                    "external_secret": {
                        "backend": "not-declared",
                        "reference": format!("{SECRET_PATH}#api_key"),
                    }
                },
            }),
        )
        .await;
    assert_eq!(
        undeclared.status().as_u16(),
        400,
        "a credential naming an undeclared backend is refused"
    );
    let body: Value = undeclared.json().await.expect("the refusal is JSON");
    assert_eq!(
        body.get("code").and_then(Value::as_str),
        Some("WYRD_GATEWAY_400_INVALID_CONFIGURATION"),
        "the refusal carries the stable configuration code: {body}"
    );
}
