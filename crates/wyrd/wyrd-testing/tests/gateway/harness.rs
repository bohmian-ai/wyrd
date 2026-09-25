//! Shared journey harness for the gateway ingress journeys.
//!
//! One bound [`WyrdTestServer`] whose built-in adapters target a local
//! `wiremock` upstream, the callers a journey uses, administration helpers for
//! credentials and deployments, and the assertions every dialect shares:
//! stable refusal envelopes, settled ledger usage, and the operator provider
//! key never accompanied by a caller token upstream.

//! Real-server HTTP protocol journeys for the provider-native gateway ingress.
//!
//! Each journey boots a bound [`WyrdTestServer`] whose built-in Anthropic and
//! Gemini adapters reach one local mock upstream, configures deployments
//! through the public administration routes, and drives the native routes with
//! the headers an unmodified provider SDK sends. It pins native success and
//! stream bytes, fail-closed truncated streams, native refusal envelopes with
//! `wyrd-request-id`, zero upstream calls for every refusal, the provider key
//! (never the caller token) upstream, and ledger usage equal to the mock's.

use std::time::Duration;

use reqwest::{Client, Response};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use url::Url;
use wiremock::{MockServer, Request};
use wyrd_runtime::Permission;
use wyrd_spec::auth::GatewayAccess;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

/// Anthropic Messages stream with usage in `message_start` and `message_delta`.
pub(crate) const ANTHROPIC_EVENTS: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-5\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

/// Anthropic stream that ends before `message_stop`.
pub(crate) const ANTHROPIC_TRUNCATED: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_3\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-5\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
);

/// Gemini SSE stream whose last event carries the finish reason and usage.
pub(crate) const GEMINI_EVENTS: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"h\"}]},\"index\":0}],\"modelVersion\":\"gemini-2.5-flash\"}\r\n\r\n",
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"i\"}]},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":7,\"candidatesTokenCount\":2,\"totalTokenCount\":9},\"modelVersion\":\"gemini-2.5-flash\"}\r\n\r\n",
);

/// Gemini SSE stream that ends before any candidate finishes.
pub(crate) const GEMINI_TRUNCATED: &str = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"h\"}]},\"index\":0}]}\r\n\r\n";

/// Provider key the tenant administrator submits; only it may reach the
/// upstream.
pub(crate) const PROVIDER_KEY: &str = "sk-native-upstream";

/// Providers the journeys deploy, and so the only ones the invoking principal
/// is granted. It holds no administration permission at all.
const INVOKED_PROVIDERS: [&str; 4] = ["openai", "anthropic", "gemini", "acme"];

/// Role name carrying exactly those invoke permissions.
const INVOKER_ROLE: &str = "gateway_invoker";

/// One bound server, its mock upstream, and the callers a journey uses.
pub(crate) struct Journey {
    /// Bound server whose built-in adapters target `upstream`.
    pub(crate) server: WyrdTestServer,
    /// Mock Anthropic and Gemini API.
    pub(crate) upstream: MockServer,
    /// Plain HTTP client standing in for the SDK transport.
    pub(crate) http: Client,
    /// Server base URL.
    pub(crate) base: String,
    /// Access token of the tenant administrator, who configures the gateway
    /// and submits provider keys but is not the principal that invokes.
    pub(crate) admin: String,
    /// Access token of an ordinary service holding only the invoke scope its
    /// work needs, the principal every journey calls the gateway as.
    pub(crate) caller: String,
    /// Access token of a principal without gateway permissions.
    pub(crate) reader: String,
    /// Durable API keys of the admin, caller, and reader, in that order,
    /// re-exchanged for fresh access tokens when the server restarts.
    api_keys: [SecretString; 3],
}

impl Journey {
    /// Boots the server against a fresh mock upstream and provisions the
    /// principals of the production sequence: a tenant administrator, an
    /// ordinary service granted only [`INVOKED_PROVIDERS`] invoke scope, and
    /// a principal with no gateway permission at all.
    ///
    /// # Panics
    /// Panics when startup, role seeding, bootstrap, or token exchange fails.
    pub(crate) async fn start() -> Self {
        let upstream = MockServer::start().await;
        let server = Box::pin(Self::builder(&upstream).start_bound())
            .await
            .expect("test server starts");
        let invoke: Vec<Permission> = INVOKED_PROVIDERS
            .iter()
            .map(|provider| {
                Permission::gateway_invoke(GatewayAccess::Provider {
                    provider: provider.parse().expect("provider id"),
                })
            })
            .collect();
        server
            .seed_role(INVOKER_ROLE, &invoke)
            .await
            .expect("invoker role seeds");
        let api_keys = [
            api_key(&server, "native_admin", &["admin"]).await,
            api_key(&server, "native_caller", &[INVOKER_ROLE]).await,
            api_key(&server, "native_reader", &["reader"]).await,
        ];
        Self::connect(server, upstream, api_keys).await
    }

    /// Restarts the server over the same Postgres and storage, as a process
    /// restart does, and re-exchanges the durable API keys for access tokens
    /// signed by the restarted process.
    ///
    /// # Panics
    /// Panics when shutdown, restart, or token exchange fails.
    pub(crate) async fn restart(self) -> Self {
        let server = Box::pin(self.server.restart_bound(Self::builder(&self.upstream)))
            .await
            .expect("test server restarts");
        Self::connect(server, self.upstream, self.api_keys).await
    }

    /// Server builder whose built-in adapters target `upstream`.
    ///
    /// # Panics
    /// Panics when the mock URL does not parse.
    fn builder(upstream: &MockServer) -> WyrdTestServerBuilder {
        let root = Url::parse(&upstream.uri()).expect("mock url");
        WyrdTestServer::builder().with_gateway_provider_root_for_test(root)
    }

    /// Assembles a journey over a started `server`, exchanging each API key
    /// for an access token.
    ///
    /// # Panics
    /// Panics when the server is unbound or an exchange fails.
    async fn connect(
        server: WyrdTestServer,
        upstream: MockServer,
        api_keys: [SecretString; 3],
    ) -> Self {
        let base = server.base_url().expect("bound url").to_owned();
        let admin = exchange(&server, &api_keys[0]).await;
        let caller = exchange(&server, &api_keys[1]).await;
        let reader = exchange(&server, &api_keys[2]).await;
        Self {
            server,
            upstream,
            http: Client::new(),
            base,
            admin,
            caller,
            reader,
            api_keys,
        }
    }

    /// Puts one administration document at `route` as the admin caller and
    /// returns the answer, refusal included.
    ///
    /// # Panics
    /// Panics when the request cannot be sent.
    pub(crate) async fn try_put(&self, route: &str, body: Value) -> Response {
        self.http
            .put(format!("{}/v1/admin/gateway/{route}", self.base))
            .header("x-wyrd-access-token", format!("Bearer {}", self.admin))
            .json(&body)
            .send()
            .await
            .expect("admin request sends")
    }

    /// Puts one administration document at `route` as the admin caller.
    ///
    /// # Panics
    /// Panics when the request fails or is refused.
    pub(crate) async fn put(&self, route: &str, body: Value) {
        let response = self.try_put(route, body).await;
        let status = response.status();
        assert!(
            status.is_success(),
            "{route}: {status} {}",
            response.text().await.unwrap_or_default()
        );
    }

    /// Submits [`PROVIDER_KEY`] as the tenant's managed credential for
    /// `provider` and stores one built-in deployment of `model`
    /// authenticated by `header`, the order a tenant administrator follows.
    pub(crate) async fn deploy(
        &self,
        provider: &str,
        header: &str,
        model: &str,
        capabilities: &[&str],
    ) {
        let credential = format!("{provider}-key");
        let name = model.replace('.', "-");
        self.put(
            &format!("provider-credentials/{credential}"),
            json!({
                "name": credential,
                "provider": provider,
                "source": {"managed_secret": {"secret": PROVIDER_KEY}},
            }),
        )
        .await;
        let auth = if header == "authorization" {
            json!({"bearer": {"credential": credential}})
        } else {
            json!({"api_key_header": {"header": header, "credential": credential}})
        };
        self.put(
            &format!("provider-deployments/{name}"),
            json!({
                "name": name,
                "model": {"provider": provider, "model": model},
                "adapter": provider,
                "auth": auth,
                "capabilities": capabilities,
                "routing_weight": 1,
            }),
        )
        .await;
    }

    /// Posts `body` to `route` with `headers`.
    ///
    /// # Panics
    /// Panics when the request cannot be sent.
    pub(crate) async fn post(
        &self,
        route: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Response {
        let mut request = self.http.post(format!("{}{route}", self.base)).json(body);
        for (name, value) in headers {
            request = request.header(*name, value);
        }
        request.send().await.expect("ingress request sends")
    }

    /// Upstream requests received so far.
    pub(crate) async fn upstream_calls(&self) -> Vec<Request> {
        self.upstream
            .received_requests()
            .await
            .expect("request recording is on")
    }

    /// Normalized usage of every `call_accounted` ledger entry, oldest first.
    ///
    /// # Panics
    /// Panics when the tenant ledger cannot be read.
    pub(crate) async fn accounted_usage(&self) -> Vec<Value> {
        let mut conn = self
            .server
            .pg_fixture()
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant conn");
        let rows: Vec<Value> = sqlx::query_scalar(
            "SELECT entry->'call_accounted'->'normalized_usage' \
               FROM wyrd.gateway_accounting_entries \
              WHERE entry ? 'call_accounted' ORDER BY recorded_at, entry_id",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("ledger reads");
        conn.commit().await.expect("ledger read commits");
        rows
    }

    /// Outcome and normalized usage of every `attempt_accounted` ledger entry,
    /// oldest first.
    ///
    /// Call it after [`Self::settled_usage`] has drained the call tasks, so the
    /// attempt entries of the settled call are all present.
    ///
    /// # Panics
    /// Panics when the tenant ledger cannot be read.
    pub(crate) async fn accounted_attempts(&self) -> Vec<(Value, Value)> {
        let mut conn = self
            .server
            .pg_fixture()
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant conn");
        let rows: Vec<(Value, Value)> = sqlx::query_as(
            "SELECT entry->'attempt_accounted'->'outcome', \
                    entry->'attempt_accounted'->'normalized_usage' \
               FROM wyrd.gateway_accounting_entries \
              WHERE entry ? 'attempt_accounted' ORDER BY recorded_at, entry_id",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("ledger reads");
        conn.commit().await.expect("ledger read commits");
        rows
    }

    /// Waits for every gateway call task, then reads the settled usage.
    ///
    /// A call settles on its tracked task after its last byte reaches the
    /// caller, so the ledger is complete once the tracker drains. The bound
    /// covers a capture outage, where settlement waits out the embedded
    /// client's own bounded publication retry.
    ///
    /// # Panics
    /// Panics when the tasks do not drain within thirty seconds.
    pub(crate) async fn settled_usage(&self) -> Vec<Value> {
        let tasks = &self.server.state().gateway_tasks;
        tasks.close();
        tokio::time::timeout(Duration::from_secs(30), tasks.wait())
            .await
            .expect("gateway tasks drain");
        tasks.reopen();
        self.accounted_usage().await
    }
}

/// Mints an access token for a service principal holding `roles`.
///
/// # Panics
/// Panics when bootstrap returns a user or token exchange fails.
pub(crate) async fn token(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    exchange(server, &api_key(server, name, roles).await).await
}

/// Bootstraps a service principal holding `roles` and returns its durable API
/// key.
///
/// # Panics
/// Panics when bootstrap fails or returns a user.
async fn api_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> SecretString {
    let Bootstrap::Machine { api_key, .. } = server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    SecretString::from(api_key.expose_secret().to_owned())
}

/// Exchanges `key` for an access token signed by `server`.
///
/// # Panics
/// Panics when the exchange fails.
async fn exchange(server: &WyrdTestServer, key: &SecretString) -> String {
    server
        .exchange_api_key(key)
        .await
        .unwrap_or_else(|error| panic!("api key exchanges: {error}"))
}

/// Token usage list the ledger records.
pub(crate) fn tokens(input: u64, output: u64) -> Value {
    json!([
        {"dimension": "input_tokens", "unit": "tokens", "quantity": input.to_string()},
        {"dimension": "output_tokens", "unit": "tokens", "quantity": output.to_string()},
    ])
}

/// Asserts `response` is a gateway refusal with `status`, `wyrd-request-id`,
/// and the stable Wyrd `code` that `code_of` reads from a well-formed native
/// envelope, returning the body.
///
/// # Panics
/// Panics when the status, request id, envelope, or code differs.
pub(crate) async fn refusal(
    response: Response,
    status: u16,
    code: &str,
    code_of: fn(&Value) -> Option<&str>,
) -> Value {
    let actual = response.status().as_u16();
    let request_id = response.headers().contains_key("wyrd-request-id");
    let body: Value = response.json().await.expect("refusal is JSON");
    assert_eq!((actual, code_of(&body)), (status, Some(code)), "{body}");
    assert!(request_id, "refusal carries wyrd-request-id: {body}");
    body
}

/// Wyrd code of an `OpenAI` error envelope, or `None` when `body` is not one.
pub(crate) fn openai_code(body: &Value) -> Option<&str> {
    let error = &body["error"];
    (error["type"].is_string() && error["message"].is_string())
        .then(|| error["code"].as_str())
        .flatten()
}

/// Wyrd code of an Anthropic error envelope, or `None` when `body` is not one.
pub(crate) fn anthropic_code(body: &Value) -> Option<&str> {
    let error = &body["error"];
    (body["type"] == "error" && error["type"].is_string() && error["message"].is_string())
        .then(|| error["code"].as_str())
        .flatten()
}

/// Wyrd code of a Google error envelope's `ErrorInfo`, or `None` when `body`
/// is not one.
pub(crate) fn google_code(body: &Value) -> Option<&str> {
    let error = &body["error"];
    let info = &error["details"][0];
    (error["code"].is_u64()
        && error["message"].is_string()
        && error["status"].is_string()
        && info["@type"] == "type.googleapis.com/google.rpc.ErrorInfo"
        && info["domain"] == "wyrd")
        .then(|| info["reason"].as_str())
        .flatten()
}

/// Asserts no upstream request carried `token` and every one carried the
/// operator's provider key in `header`.
///
/// # Panics
/// Panics when a caller credential leaks or the provider key is missing.
pub(crate) fn assert_provider_credentials(
    calls: &[Request],
    header: &str,
    expected: &str,
    token: &str,
) {
    for call in calls {
        assert_eq!(
            call.headers
                .get(header)
                .and_then(|value| value.to_str().ok()),
            Some(expected),
            "{:?}",
            call.headers
        );
        for (name, value) in &call.headers {
            let value = value.to_str().unwrap_or_default();
            assert!(!value.contains(token), "caller token forwarded in {name}");
            assert!(
                !name.as_str().starts_with("x-wyrd"),
                "wyrd header forwarded: {name}"
            );
        }
        assert!(
            !call.url.as_str().contains(token),
            "caller token in {}",
            call.url
        );
    }
}
