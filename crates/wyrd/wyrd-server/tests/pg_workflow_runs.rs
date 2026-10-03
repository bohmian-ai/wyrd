//! Client→server journeys for accepted server Workflow runs.
//!
//! Every journey boots a bound server whose built-in OpenAI adapter reaches
//! one local scripted upstream, configures the gateway through the public
//! administration routes, registers real Workflow bundles through the shared
//! Rust client, and drives `/v1/workflow-runs` over HTTP. The upstream records
//! each request and can hold every response, so a journey acts while steps are
//! in flight without sleeping for them.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use chrono::Duration as ChronoDuration;
use reqwest::{RequestBuilder, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::watch;
use url::Url;
use wyrd_auth_verify::WyrdAuthVerifySettings;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_runtime::Permission;
use wyrd_server::config::ServerWorkflowConfig;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::card::workflow::{WorkflowRun, WorkflowRunStatus, WorkflowStepStatus};
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::seed_query_fixture;
use wyrd_testing::server::{BifrostQueryResourceSnapshot, WyrdTestServer, WyrdTestServerBuilder};

/// Role holding exactly what an ordinary Workflow caller needs.
const RUNNER_ROLE: &str = "workflow_runner";

/// Provider key the tenant administrator submits for the gateway deployment.
const PROVIDER_KEY: &str = "sk-workflow-upstream";

/// Header every `/v1` route reads the caller's access token from.
const ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// Header an external gateway binding sends its secret in.
const SECRET_HEADER: &str = "x-review-secret";

/// Secret an external gateway binding reads from its file.
const EXTERNAL_SECRET: &str = "external-s3cret";

/// Workflow-level route YAML selecting the governed gateway.
const WYRD_GATEWAY: &str = "    kind: wyrd_gateway";

/// Bound for every wait on the server or the upstream.
const PATIENCE: Duration = Duration::from_secs(30);

/// A settled Oracle query owns none of the resources it borrowed.
const RELEASED: BifrostQueryResourceSnapshot = BifrostQueryResourceSnapshot {
    admission_slots: 0,
    memory_bytes: 0,
    peer_slots: 0,
};

/// One chat completion request as the upstream received it.
#[derive(Debug, Clone)]
struct UpstreamCall {
    /// `authorization` header, as the gateway sends its provider key.
    authorization: Option<String>,
    /// [`SECRET_HEADER`], as an external binding sends its secret.
    secret: Option<String>,
    /// Request body.
    body: Value,
}

/// Shared state of the scripted upstream.
struct Script {
    /// Every request in arrival order.
    calls: Mutex<Vec<UpstreamCall>>,
    /// Number of requests received so far.
    arrivals: watch::Sender<usize>,
    /// While true, every response waits.
    held: watch::Sender<bool>,
    /// Assistant messages answered in order; `DONE` text once exhausted.
    replies: Mutex<VecDeque<Value>>,
}

/// Local OpenAI-compatible upstream serving `POST /v1/chat/completions`.
struct Upstream {
    /// Origin the gateway and external bindings reach it at.
    url: Url,
    /// State shared with the serving task.
    script: Arc<Script>,
}

impl Upstream {
    /// Bind a loopback listener and serve the scripted completions on it.
    ///
    /// # Panics
    /// Panics when the listener cannot bind.
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("upstream binds");
        let url = Url::parse(&format!(
            "http://{}",
            listener.local_addr().expect("upstream has an address")
        ))
        .expect("upstream URL parses");
        let script = Arc::new(Script {
            calls: Mutex::default(),
            arrivals: watch::Sender::new(0),
            held: watch::Sender::new(false),
            replies: Mutex::default(),
        });
        let app = axum::Router::new()
            .route("/v1/chat/completions", axum::routing::post(complete))
            .with_state(Arc::clone(&script));
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url, script }
    }

    /// Make every response wait until [`Self::release`].
    fn hold(&self) {
        self.script.held.send_replace(true);
    }

    /// Let every waiting and later response proceed.
    fn release(&self) {
        self.script.held.send_replace(false);
    }

    /// Queue one assistant message as the next answer.
    fn reply(&self, message: Value) {
        self.script
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(message);
    }

    /// Every request received so far.
    fn calls(&self) -> Vec<UpstreamCall> {
        self.script
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Number of requests received so far.
    fn arrivals(&self) -> usize {
        *self.script.arrivals.borrow()
    }

    /// Wait until at least `count` requests have arrived.
    ///
    /// # Panics
    /// Panics when they do not arrive within [`PATIENCE`].
    async fn wait_arrivals(&self, count: usize) {
        let mut arrivals = self.script.arrivals.subscribe();
        tokio::time::timeout(PATIENCE, arrivals.wait_for(|arrived| *arrived >= count))
            .await
            .unwrap_or_else(|_| panic!("{count} upstream requests arrive"))
            .expect("the arrival counter lives with the script");
    }
}

/// Record one completion request, wait while held, and answer the next
/// scripted message with usage.
///
/// # Panics
/// Panics if the hold sender is gone, which the script owns for its life.
async fn complete(
    State(script): State<Arc<Script>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    script
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(UpstreamCall {
            authorization: header("authorization"),
            secret: header(SECRET_HEADER),
            body,
        });
    script.arrivals.send_modify(|arrived| *arrived += 1);
    script
        .held
        .subscribe()
        .wait_for(|held| !*held)
        .await
        .expect("the hold sender lives with the script");
    let message = script
        .replies
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front()
        .unwrap_or_else(|| text("DONE"));
    let finish_reason = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else {
        "stop"
    };
    Json(json!({
        "id": "chatcmpl-workflow",
        "object": "chat.completion",
        "created": 0,
        "model": "gpt-5-5",
        "choices": [{ "index": 0, "message": message, "finish_reason": finish_reason }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
    }))
}

/// An assistant text message.
fn text(content: &str) -> Value {
    json!({ "role": "assistant", "content": content })
}

/// An assistant message calling each `(id, tool, arguments)`.
fn tool_calls(calls: &[(&str, &str, Value)]) -> Value {
    let calls: Vec<Value> = calls
        .iter()
        .map(|(id, name, arguments)| {
            json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": arguments.to_string() }
            })
        })
        .collect();
    json!({ "role": "assistant", "content": null, "tool_calls": calls })
}

/// One bootstrapped service principal and an access token for it.
struct Principal {
    /// Bootstrap record, carrying the principal id and durable API key.
    bootstrap: Bootstrap,
    /// Access token exchanged when the principal was created or the server
    /// last restarted.
    token: String,
}

impl Principal {
    /// The principal's durable API key.
    ///
    /// # Panics
    /// Panics when the principal is not a machine principal.
    fn api_key(&self) -> &SecretString {
        self.bootstrap
            .api_key()
            .expect("service principals carry a key")
    }
}

/// One retained authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decision {
    /// Principal the decision was made for.
    principal: String,
    /// Permission evaluated.
    permission: String,
    /// `allowed` or `denied`.
    outcome: String,
    /// Resource the operation named.
    resource: String,
    /// Recorded detail, empty when none.
    detail: String,
}

/// A bound server, its scripted upstream, and the principals a journey uses.
struct Fixture {
    /// Bound server under test.
    server: WyrdTestServer,
    /// Scripted upstream behind the gateway and external bindings.
    upstream: Upstream,
    /// Plain HTTP client for the Workflow-run routes.
    http: reqwest::Client,
    /// Server base URL.
    base: String,
    /// Tenant administrator who configures the gateway and registers Cards.
    admin: Principal,
    /// Ordinary caller holding [`RUNNER_ROLE`].
    runner: Principal,
}

impl Fixture {
    /// Start with `config` and the default token lifetime.
    async fn start(config: ServerWorkflowConfig) -> Self {
        Self::start_with(config, |builder| builder).await
    }

    /// Start a server built by `edit` with `config`, seed the runner role,
    /// bootstrap the administrator and runner, deploy `openai/gpt-5-5`, and
    /// register the checked-in code-review bundle.
    ///
    /// # Panics
    /// Panics when the server, role, principals, deployment, or registration
    /// cannot be set up.
    async fn start_with(
        config: ServerWorkflowConfig,
        edit: impl FnOnce(WyrdTestServerBuilder) -> WyrdTestServerBuilder,
    ) -> Self {
        let upstream = Upstream::start().await;
        let server = Box::pin(edit(Self::builder(&upstream, config)).start_bound())
            .await
            .expect("bound test server starts");
        server
            .seed_role(
                RUNNER_ROLE,
                &[
                    Permission::workflow_run(),
                    Permission::card_read(),
                    Permission::bifrost_query_read(),
                    Permission::bifrost_table_read(),
                    openai_invoke(),
                ],
            )
            .await
            .expect("runner role seeds");
        let admin = bootstrap(&server, "workflow-admin", &["admin"]).await;
        let runner = bootstrap(&server, "workflow-runner", &[RUNNER_ROLE]).await;
        let fixture = Self {
            base: server.base_url().expect("bound URL").to_owned(),
            server,
            upstream,
            http: reqwest::Client::new(),
            admin,
            runner,
        };
        fixture.deploy().await;
        fixture.register(&code_review()).await;
        fixture
    }

    /// Server builder whose OpenAI adapter reaches `upstream`.
    fn builder(upstream: &Upstream, config: ServerWorkflowConfig) -> WyrdTestServerBuilder {
        WyrdTestServer::builder()
            .with_gateway_provider_root_for_test(upstream.url.clone())
            .with_workflow_config_for_test(config)
    }

    /// Restart over the same database with `config`, as a process restart
    /// does, and re-exchange every principal's key.
    ///
    /// # Panics
    /// Panics when the restart or an exchange fails.
    async fn restart(self, config: ServerWorkflowConfig) -> Self {
        let server = Box::pin(
            self.server
                .restart_bound(Self::builder(&self.upstream, config)),
        )
        .await
        .expect("test server restarts");
        let admin = reauthenticate(&server, self.admin).await;
        let runner = reauthenticate(&server, self.runner).await;
        Self {
            base: server.base_url().expect("bound URL").to_owned(),
            server,
            upstream: self.upstream,
            http: self.http,
            admin,
            runner,
        }
    }

    /// Bootstrap a service principal holding `roles`.
    async fn principal(&self, name: &str, roles: &[&str]) -> Principal {
        bootstrap(&self.server, name, roles).await
    }

    /// A fresh access token for `principal`, carrying its current grants.
    ///
    /// # Panics
    /// Panics when the exchange fails.
    async fn token(&self, principal: &Principal) -> String {
        self.server
            .exchange_api_key(principal.api_key())
            .await
            .expect("api key exchanges")
    }

    /// Submit the provider key and one `openai/gpt-5-5` deployment as the
    /// administrator.
    ///
    /// # Panics
    /// Panics when either administration request is refused.
    async fn deploy(&self) {
        let token = self.token(&self.admin).await;
        for (route, body) in [
            (
                "provider-credentials/openai-key",
                json!({
                    "name": "openai-key",
                    "provider": "openai",
                    "source": { "managed_secret": { "secret": PROVIDER_KEY } },
                }),
            ),
            (
                "provider-deployments/gpt-5-5",
                json!({
                    "name": "gpt-5-5",
                    "model": { "provider": "openai", "model": "gpt-5-5" },
                    "adapter": "openai",
                    "auth": { "bearer": { "credential": "openai-key" } },
                    "capabilities": ["chat_completions"],
                    "routing_weight": 1,
                }),
            ),
        ] {
            let response = self
                .http
                .put(format!("{}/v1/admin/gateway/{route}", self.base))
                .header(ACCESS_TOKEN_HEADER, format!("Bearer {token}"))
                .json(&body)
                .send()
                .await
                .expect("admin request sends");
            let status = response.status();
            assert!(
                status.is_success(),
                "{route}: {status} {}",
                response.text().await.unwrap_or_default()
            );
        }
    }

    /// Register the bundle rooted at `workflow` as the administrator through
    /// the shared Rust client.
    ///
    /// # Panics
    /// Panics when the client cannot build or registration fails.
    async fn register(&self, workflow: &Path) {
        let token = self.token(&self.admin).await;
        let config = ClientConfig {
            http: HttpConfig {
                base_url: self.base.clone(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(token)),
        )
        .expect("client auth builds");
        let transport =
            HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
        Cards::with_client(WyrdClient::from_parts(auth, transport, config.grpc))
            .register_from_path(workflow)
            .await
            .unwrap_or_else(|error| panic!("{} registers: {error}", workflow.display()));
    }

    /// The create request for `key` and raw `body` as `token`.
    fn create_request(&self, token: &str, key: &str, body: &Value) -> RequestBuilder {
        self.http
            .post(format!("{}/v1/workflow-runs", self.base))
            .header(ACCESS_TOKEN_HEADER, format!("Bearer {token}"))
            .header(IDEMPOTENCY_KEY_HEADER, key)
            .json(body)
    }

    /// Submit `body` under `key` as `token`.
    async fn create(&self, token: &str, key: &str, body: &Value) -> (StatusCode, Value) {
        send(self.create_request(token, key, body)).await
    }

    /// Submit `body` under a fresh key as `token` and return the accepted
    /// queued run.
    ///
    /// # Panics
    /// Panics unless the server answers `202` with a queued run.
    async fn accept(&self, token: &str, body: &Value) -> WorkflowRun {
        let (status, run) = self.create(token, &new_key(), body).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{run}");
        let run: WorkflowRun = serde_json::from_value(run).expect("accepted body is a run");
        assert_eq!(run.status, WorkflowRunStatus::Queued);
        run
    }

    /// Read run `run_id` as `token`.
    async fn get(&self, token: &str, run_id: &str) -> (StatusCode, Value) {
        send(
            self.http
                .get(format!("{}/v1/workflow-runs/{run_id}", self.base))
                .header(ACCESS_TOKEN_HEADER, format!("Bearer {token}")),
        )
        .await
    }

    /// Cancel run `run_id` as `token`.
    async fn cancel(&self, token: &str, run_id: &str) -> (StatusCode, Value) {
        send(
            self.http
                .post(format!("{}/v1/workflow-runs/{run_id}/cancel", self.base))
                .header(ACCESS_TOKEN_HEADER, format!("Bearer {token}")),
        )
        .await
    }

    /// Poll run `run` as `token` until it is terminal.
    ///
    /// # Panics
    /// Panics when a read fails or the run is not terminal within
    /// [`PATIENCE`].
    async fn terminal(&self, token: &str, run: &WorkflowRun) -> WorkflowRun {
        let run_id = run.run_id.to_string();
        tokio::time::timeout(PATIENCE, async {
            loop {
                let (status, body) = self.get(token, &run_id).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                let run: WorkflowRun = serde_json::from_value(body).expect("body is a run");
                if run.status.is_terminal() {
                    return run;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("run {run_id} terminates"))
    }

    /// Every retained decision of `operation` in decision order.
    ///
    /// # Panics
    /// Panics when publication or the retained read fails.
    async fn decisions(&self, operation: &str) -> Vec<Decision> {
        let tenant = self.server.data_tenant_id();
        self.server
            .await_audit_published(tenant)
            .await
            .expect("audit publishes");
        self.server
            .retained_audit_records(
                tenant,
                "audit_principal_id, permission, outcome, resource, detail",
                &format!("operation = '{operation}'"),
            )
            .await
            .expect("retained audit reads")
            .into_iter()
            .map(|record| {
                let mut fields = record.into_iter().map(Option::unwrap_or_default);
                let mut next = || fields.next().unwrap_or_default();
                Decision {
                    principal: next(),
                    permission: next(),
                    outcome: next(),
                    resource: next(),
                    detail: next(),
                }
            })
            .collect()
    }

    /// Wait for every gateway call task, whose settlement records the call's
    /// ledger entry and invoke decision.
    ///
    /// # Panics
    /// Panics when the tasks do not drain within [`PATIENCE`].
    async fn settle_gateway(&self) {
        let tasks = &self.server.state().gateway_tasks;
        tasks.close();
        tokio::time::timeout(PATIENCE, tasks.wait())
            .await
            .expect("gateway tasks drain");
        tasks.reopen();
    }

    /// Number of gateway calls the ledger has accounted, once every gateway
    /// call task has settled.
    ///
    /// # Panics
    /// Panics when the tasks do not drain or the ledger cannot be read.
    async fn accounted_calls(&self) -> i64 {
        self.settle_gateway().await;
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.gateway_accounting_entries WHERE entry ? 'call_accounted'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("ledger reads");
        conn.commit().await.expect("ledger read commits");
        count
    }

    /// Make every create decision of this tenant fail to append its audit
    /// row, until [`Self::restore_create_audit`].
    ///
    /// The trigger is scoped to this tenant and operation, so concurrent
    /// fixtures sharing the database are unaffected.
    ///
    /// # Panics
    /// Panics when the trigger cannot be installed.
    async fn fail_create_audit(&self) {
        let pool = self
            .server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        for statement in [
            "CREATE OR REPLACE FUNCTION vala.wyrd_test_fail_workflow_create() RETURNS trigger \
             LANGUAGE plpgsql AS $$ BEGIN \
             IF NEW.operation = 'workflow.run.create' AND NEW.data_tenant_id = TG_ARGV[0]::uuid THEN \
             RAISE EXCEPTION 'test fault: workflow create audit refused'; \
             END IF; RETURN NEW; END $$"
                .to_owned(),
            format!(
                "CREATE OR REPLACE TRIGGER wyrd_test_fail_workflow_create_{tenant} \
                 BEFORE INSERT ON vala.audit_staging FOR EACH ROW \
                 EXECUTE FUNCTION vala.wyrd_test_fail_workflow_create('{uuid}')",
                tenant = self.server.data_tenant_id().as_uuid().simple(),
                uuid = self.server.data_tenant_id().as_uuid(),
            ),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&pool)
                .await
                .expect("audit fault installs");
        }
    }

    /// Remove the fault [`Self::fail_create_audit`] installed.
    ///
    /// # Panics
    /// Panics when the trigger cannot be dropped.
    async fn restore_create_audit(&self) {
        let pool = self
            .server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS wyrd_test_fail_workflow_create_{} ON vala.audit_staging",
            self.server.data_tenant_id().as_uuid().simple()
        )))
        .execute(&pool)
        .await
        .expect("audit fault clears");
    }
}

/// Bootstrap a service principal holding `roles` on `server` and exchange
/// its key.
///
/// # Panics
/// Panics when bootstrap or the exchange fails.
async fn bootstrap(server: &WyrdTestServer, name: &str, roles: &[&str]) -> Principal {
    let bootstrap = server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps");
    let key = SecretString::from(
        bootstrap
            .api_key()
            .expect("service principals carry a key")
            .expose_secret()
            .to_owned(),
    );
    let token = server.exchange_api_key(&key).await.expect("key exchanges");
    Principal { bootstrap, token }
}

/// The decisions among `decisions` made for `principal` with `outcome`.
fn made_for<'a>(
    decisions: &'a [Decision],
    principal: &Principal,
    outcome: &str,
) -> Vec<&'a Decision> {
    let id = principal.bootstrap.id().to_string();
    decisions
        .iter()
        .filter(|decision| decision.principal == id && decision.outcome == outcome)
        .collect()
}

/// `principal` with a token signed by the restarted `server`.
///
/// # Panics
/// Panics when the exchange fails.
async fn reauthenticate(server: &WyrdTestServer, principal: Principal) -> Principal {
    let token = server
        .exchange_api_key(principal.api_key())
        .await
        .expect("key exchanges after restart");
    Principal { token, ..principal }
}

/// Invoke permission for every `openai` model.
///
/// # Panics
/// Panics if `openai` stops being a valid provider id.
fn openai_invoke() -> Permission {
    Permission::gateway_invoke(GatewayAccess::Provider {
        provider: "openai".parse().expect("provider id"),
    })
}

/// Send `request` and return its status and JSON body (`null` when empty).
///
/// # Panics
/// Panics when the request cannot be sent.
async fn send(request: RequestBuilder) -> (StatusCode, Value) {
    let response = request.send().await.expect("request sends");
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// Assert `answer` is a problem with `status` and `code`, returning the body.
///
/// # Panics
/// Panics when the status or code differs.
#[track_caller]
fn problem(answer: (StatusCode, Value), status: StatusCode, code: &str) -> Value {
    let (actual, body) = answer;
    assert_eq!(
        (actual, body["code"].as_str()),
        (status, Some(code)),
        "{body}"
    );
    body
}

/// A fresh idempotency key.
fn new_key() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// A run request for `name@1.0.0` in `engineering` with input `code`.
fn run_request(name: &str, code: &str) -> Value {
    json!({
        "workflow": { "kind": "Workflow", "name": name, "version": "1.0.0", "space": "engineering" },
        "input": { "code": code },
    })
}

/// The checked-in code-review bundle's Workflow file.
fn code_review() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/workflows/code-review/workflow.yaml")
}

/// Copy the code-review bundle renamed to `name`, applying `edit` to its
/// Workflow file.
///
/// # Panics
/// Panics when a bundle file cannot be read or written.
fn code_review_variant(name: &str, edit: impl Fn(String) -> String) -> TempDir {
    let source = code_review();
    let root = source.parent().expect("bundle directory");
    let temp = TempDir::new().expect("bundle directory creates");
    for dir in ["agents", "prompts"] {
        std::fs::create_dir(temp.path().join(dir)).expect("bundle directory creates");
        for card in ["security", "correctness", "final-reviewer"] {
            let file = format!("{dir}/{card}.yaml");
            std::fs::copy(root.join(&file), temp.path().join(&file)).expect("card copies");
        }
    }
    let workflow = std::fs::read_to_string(&source)
        .expect("workflow reads")
        .replacen("name: code-review", &format!("name: {name}"), 1);
    std::fs::write(temp.path().join("workflow.yaml"), edit(workflow)).expect("workflow writes");
    temp
}

/// Write a one-step Workflow `name` whose Agent declares `tools` and whose
/// Workflow-level route is `route`, with a system prompt padded by `padding`
/// bytes.
///
/// # Panics
/// Panics when a bundle file cannot be written.
fn single_step(name: &str, tools: &str, route: &str, padding: usize) -> TempDir {
    let temp = TempDir::new().expect("bundle directory creates");
    let write = |file: &str, body: String| {
        std::fs::write(temp.path().join(file), body).expect("bundle file writes");
    };
    write(
        "prompt.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  space: engineering\n  name: {name}-prompt\n  version: \"1.0.0\"\nspec:\n  model: gpt-5-5\n  request:\n    model: gpt-5-5\n    messages:\n      - role: system\n        content: \"You answer for {name}.{}\"\n      - role: user\n        content: \"Answer about {{{{code}}}}\"\n  variables: [code]\n  response_type: text\n",
            " ".repeat(padding)
        ),
    );
    write(
        "agent.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: {name}-agent\n  version: \"1.0.0\"\nspec:\n  prompt: ./prompt.yaml\n  tool_names: {tools}\n  run_config:\n    max_iterations: 4\n"
        ),
    );
    write(
        "workflow.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: {name}\n  version: \"1.0.0\"\nspec:\n  llm_route:\n{route}\n  inputs:\n    code:\n      type: str\n      value: \"\"\n  steps:\n    - id: answer\n      action:\n        type: agent\n        target: ./agent.yaml\n      inputs:\n        code: input.code\n  outputs:\n    answer: steps.answer.output.text\n"
        ),
    );
    temp
}

/// Workflow-level route YAML selecting the external binding `binding` at
/// `upstream`.
fn external_route(upstream: &Upstream, binding: &str) -> String {
    format!(
        "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}v1\n    credential_binding: {binding}",
        upstream.url
    )
}

/// Workflow bounds from the defaults overlaid with `overrides`.
///
/// # Panics
/// Panics when `overrides` is not a valid partial configuration.
fn config(overrides: Value) -> ServerWorkflowConfig {
    serde_json::from_value(overrides).expect("workflow configuration decodes")
}

/// Decode a run body.
///
/// # Panics
/// Panics when `body` is not a run.
fn run_of(body: Value) -> WorkflowRun {
    serde_json::from_value(body).expect("body is a run")
}

/// Status of step `step` in `run`.
///
/// # Panics
/// Panics when the run has no such step.
fn step_status(run: &WorkflowRun, step: &str) -> WorkflowStepStatus {
    run.steps
        .get(step)
        .unwrap_or_else(|| panic!("run has step {step}"))
        .status
}

#[tokio::test(flavor = "multi_thread")]
/// Every refusal of a create happens before any run exists or any provider
/// or tool is called. Unauthenticated callers are refused first; principals
/// without `workflows:run` (reader, runtime_admin) are refused and audited
/// before any run lookup; request-shape, route-override, and timeout errors
/// are refused before authorization; an unknown version, a native route,
/// undeclarable tools, an unbound external route, an oversized input, and a
/// full tenant are refused after an audited allow; and an unrecordable
/// decision refuses the create. Writer, agent, and admin principals are
/// accepted, with live gateway grants still applying to the run.
///
/// # Panics
/// Panics when any status, code, upstream call, or retained decision differs.
async fn admission_is_audited_and_side_effect_free_on_refusal() {
    let fixture = Fixture::start(config(json!({
        "max_active_per_tenant": 1,
        "max_input_bytes": 4096,
    })))
    .await;
    let external = external_route(&fixture.upstream, "absent-binding");
    for (name, tools, route) in [
        ("native-review", "[]", "    kind: native"),
        ("shell-review", "[shell.exec]", WYRD_GATEWAY),
        ("twice-review", "[cards.get, cards.get]", WYRD_GATEWAY),
        ("external-review", "[]", external.as_str()),
    ] {
        let bundle = single_step(name, tools, route, 0);
        fixture.register(&bundle.path().join("workflow.yaml")).await;
    }
    let reader = fixture.principal("workflow-reader", &["reader"]).await;
    let runtime_admin = fixture
        .principal("workflow-runtime-admin", &["runtime_admin"])
        .await;
    let runner = &fixture.runner.token;
    let request = run_request("code-review", "fn secret_marker() {}");
    let unknown = uuid::Uuid::now_v7().to_string();

    let (status, _) = send(
        fixture
            .http
            .post(format!("{}/v1/workflow-runs", fixture.base))
            .header(IDEMPOTENCY_KEY_HEADER, new_key())
            .json(&request),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for denied in [&reader, &runtime_admin] {
        let denial = "WYRD_PERMISSION_403_DENIED_RBAC";
        problem(
            fixture.create(&denied.token, &new_key(), &request).await,
            StatusCode::FORBIDDEN,
            denial,
        );
        problem(
            fixture.get(&denied.token, &unknown).await,
            StatusCode::FORBIDDEN,
            denial,
        );
        problem(
            fixture.cancel(&denied.token, &unknown).await,
            StatusCode::FORBIDDEN,
            denial,
        );
    }

    let shape = "WYRD_WORKFLOW_422_RUN_REQUEST";
    let unprocessable = StatusCode::UNPROCESSABLE_ENTITY;
    let missing_key = problem(
        send(
            fixture
                .http
                .post(format!("{}/v1/workflow-runs", fixture.base))
                .header(ACCESS_TOKEN_HEADER, format!("Bearer {runner}"))
                .json(&request),
        )
        .await,
        unprocessable,
        shape,
    );
    assert_eq!(missing_key["details"]["field"], IDEMPOTENCY_KEY_HEADER);
    let mut route_override = request.clone();
    route_override["llm_route"] = json!({ "kind": "native" });
    let mut no_space = request.clone();
    no_space["workflow"]
        .as_object_mut()
        .expect("workflow ref is an object")
        .remove("space");
    let mut agent_ref = request.clone();
    agent_ref["workflow"]["kind"] = json!("Agent");
    let mut zero_timeout = request.clone();
    zero_timeout["timeout_seconds"] = json!(0);
    let mut long_timeout = request.clone();
    long_timeout["timeout_seconds"] = json!(7201);
    for (body, field) in [
        (json!({ "workflow": 1 }), "body"),
        (route_override, "body"),
        (no_space, "workflow"),
        (agent_ref, "workflow"),
        (zero_timeout, "timeout_seconds"),
        (long_timeout, "timeout_seconds"),
    ] {
        let refusal = problem(
            fixture.create(runner, &new_key(), &body).await,
            unprocessable,
            shape,
        );
        assert_eq!(refusal["details"]["field"], field, "{refusal}");
    }

    let mut unknown_version = request.clone();
    unknown_version["workflow"]["version"] = json!("9.9.9");
    let oversized = run_request("code-review", &"x".repeat(4096));
    let authorized_refusals = [
        (
            unknown_version,
            StatusCode::NOT_FOUND,
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        ),
        (
            run_request("native-review", "x"),
            unprocessable,
            "WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED",
        ),
        (
            run_request("shell-review", "x"),
            unprocessable,
            "WYRD_WORKFLOW_422_TOOL_UNAVAILABLE",
        ),
        (
            run_request("twice-review", "x"),
            unprocessable,
            "WYRD_WORKFLOW_422_TOOL_UNAVAILABLE",
        ),
        (
            run_request("external-review", "x"),
            StatusCode::SERVICE_UNAVAILABLE,
            "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
        ),
        (
            oversized,
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_WORKFLOW_413_INPUT_TOO_LARGE",
        ),
    ];
    let mut runner_allowed = authorized_refusals.len();
    for (body, status, code) in authorized_refusals {
        problem(
            fixture.create(runner, &new_key(), &body).await,
            status,
            code,
        );
    }

    fixture.fail_create_audit().await;
    let unrecorded = fixture.create(runner, &new_key(), &request).await;
    fixture.restore_create_audit().await;
    problem(
        unrecorded,
        StatusCode::INTERNAL_SERVER_ERROR,
        "WYRD_VALA_500_AUDIT_UNAVAILABLE",
    );
    problem(
        fixture.get(runner, &unknown).await,
        StatusCode::NOT_FOUND,
        "WYRD_WORKFLOW_404_RUN_NOT_FOUND",
    );
    assert_eq!(
        fixture.upstream.arrivals(),
        0,
        "no refusal reached a provider"
    );

    fixture.upstream.hold();
    let held = fixture.accept(runner, &request).await;
    fixture.upstream.wait_arrivals(2).await;
    problem(
        fixture.create(runner, &new_key(), &request).await,
        StatusCode::TOO_MANY_REQUESTS,
        "WYRD_WORKFLOW_429_RUN_CAPACITY",
    );
    runner_allowed += 2;
    fixture.upstream.release();
    let finished = fixture.terminal(runner, &held).await;
    assert_eq!(finished.status, WorkflowRunStatus::Succeeded);
    assert_eq!(finished.outputs["review"], json!("DONE"));
    assert_eq!(fixture.upstream.arrivals(), 3);

    let writer = fixture.principal("workflow-writer", &["writer"]).await;
    let agent = fixture.principal("workflow-agent", &["agent"]).await;
    for ungoverned in [&writer, &agent] {
        let run = fixture.accept(&ungoverned.token, &request).await;
        let run = fixture.terminal(&ungoverned.token, &run).await;
        assert_eq!(
            run.status,
            WorkflowRunStatus::Failed,
            "the gateway still refuses a caller without invoke permission"
        );
    }
    assert_eq!(fixture.upstream.arrivals(), 3);
    let run = fixture.accept(&fixture.admin.token, &request).await;
    let run = fixture.terminal(&fixture.admin.token, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded);
    assert_eq!(fixture.upstream.arrivals(), 6);

    let resource = "workflow:engineering/code-review@1.0.0";
    let creates = fixture.decisions("workflow.run.create").await;
    let reads = fixture.decisions("workflow.run.read").await;
    for denied in [&reader, &runtime_admin] {
        let create = made_for(&creates, denied, "denied");
        assert_eq!(create.len(), 1, "{create:?}");
        assert_eq!(create[0].permission, "workflows:run");
        assert_eq!(create[0].resource, resource);
        let read = made_for(&reads, denied, "denied");
        assert_eq!(read.len(), 1, "{read:?}");
        assert_eq!(read[0].resource, format!("workflow-run:{unknown}"));
    }
    let allowed = made_for(&creates, &fixture.runner, "allowed");
    assert_eq!(allowed.len(), runner_allowed, "{allowed:?}");
    for accepted in [&writer, &agent, &fixture.admin] {
        let allowed = made_for(&creates, accepted, "allowed");
        assert_eq!(allowed.len(), 1, "{allowed:?}");
    }
    assert!(
        creates
            .iter()
            .all(|decision| !decision.detail.contains("secret_marker")),
        "decisions carry no request input"
    );
}

#[tokio::test(flavor = "multi_thread")]
/// Concurrent creates under one key share one preparation and one run: the
/// creator gets `202`, every other `200` with the same run, and the provider
/// sees exactly one run's calls. A changed request under the key conflicts,
/// while another principal's identical key is its own run that the first
/// principal cannot see. A failed preparation caches nothing and frees its
/// slot, so the same key later succeeds. A creator that disconnects
/// mid-preparation strands nothing: a waiter and the run see it through.
///
/// # Panics
/// Panics when any status, run identity, or upstream call count differs.
async fn tracked_preparation_replay_and_disconnect() {
    let fixture = Fixture::start(config(json!({ "max_active_per_tenant": 2 }))).await;
    let runner = &fixture.runner.token;
    let request = run_request("code-review", "fn shared() {}");

    fixture.upstream.hold();
    let key = new_key();
    let answers =
        futures_util::future::join_all((0..4).map(|_| fixture.create(runner, &key, &request)))
            .await;
    let statuses: Vec<StatusCode> = answers.iter().map(|(status, _)| *status).collect();
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::ACCEPTED)
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        3,
        "{statuses:?}"
    );
    let runs: Vec<WorkflowRun> = answers.into_iter().map(|(_, body)| run_of(body)).collect();
    assert!(runs.iter().all(|run| run.run_id == runs[0].run_id));
    let first = runs[0].clone();
    problem(
        fixture
            .create(runner, &key, &run_request("code-review", "fn changed() {}"))
            .await,
        StatusCode::CONFLICT,
        "WYRD_WORKFLOW_409_IDEMPOTENCY_CONFLICT",
    );
    let other = fixture
        .principal("workflow-runner-two", &[RUNNER_ROLE])
        .await;
    let (status, theirs) = fixture.create(&other.token, &key, &request).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{theirs}");
    let theirs = run_of(theirs);
    assert_ne!(theirs.run_id, first.run_id);
    problem(
        fixture.get(&other.token, &first.run_id.to_string()).await,
        StatusCode::NOT_FOUND,
        "WYRD_WORKFLOW_404_RUN_NOT_FOUND",
    );
    fixture.upstream.wait_arrivals(4).await;
    fixture.upstream.release();
    assert_eq!(
        fixture.terminal(runner, &first).await.status,
        WorkflowRunStatus::Succeeded
    );
    assert_eq!(
        fixture.terminal(&other.token, &theirs).await.status,
        WorkflowRunStatus::Succeeded
    );
    assert_eq!(fixture.upstream.arrivals(), 6, "one run per scoped key");
    let (status, replay) = fixture.create(runner, &key, &request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run_of(replay).run_id, first.run_id);

    let late_key = new_key();
    let late = run_request("late-review", "x");
    for _ in 0..2 {
        problem(
            fixture.create(runner, &late_key, &late).await,
            StatusCode::NOT_FOUND,
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        );
    }
    let bundle = single_step("late-review", "[]", WYRD_GATEWAY, 0);
    fixture.register(&bundle.path().join("workflow.yaml")).await;
    let (status, late_run) = fixture.create(runner, &late_key, &late).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "a failed preparation is not cached"
    );
    assert_eq!(
        fixture.terminal(runner, &run_of(late_run)).await.status,
        WorkflowRunStatus::Succeeded
    );
    assert_eq!(fixture.upstream.arrivals(), 7);

    let workflows = &fixture.server.state().workflows;
    workflows.stall_next_preparation_for_test();
    let detached_key = new_key();
    let creator = tokio::spawn(send(fixture.create_request(
        runner,
        &detached_key,
        &request,
    )));
    tokio::time::timeout(PATIENCE, workflows.wait_preparation_stall_for_test())
        .await
        .expect("the creator's preparation stops at the gate");
    creator.abort();
    assert!(creator.await.is_err(), "the creator disconnected");
    let waiter = tokio::spawn(send(fixture.create_request(
        runner,
        &detached_key,
        &request,
    )));
    workflows.release_preparation_for_test();
    let (status, recovered) = waiter.await.expect("waiter finishes");
    assert_eq!(status, StatusCode::OK, "{recovered}");
    let recovered = fixture.terminal(runner, &run_of(recovered)).await;
    assert_eq!(recovered.status, WorkflowRunStatus::Succeeded);
    assert_eq!(fixture.upstream.arrivals(), 10);
}

#[tokio::test(flavor = "multi_thread")]
/// An accepted run keeps the authority it was accepted with after its
/// submission token expires and its grants are revoked: the remaining step
/// still reaches the gateway as the captured caller. Every later request
/// needs current authentication and permission — an expired token is
/// refused, a token minted without the grant is denied for read, cancel,
/// and replay — and a restored grant reads the run and replays it without
/// starting another.
///
/// # Panics
/// Panics when any status, run outcome, upstream call count, or gateway
/// decision differs.
async fn accepted_authority_outlives_submission_only() {
    let fixture = Fixture::start_with(ServerWorkflowConfig::default(), |builder| {
        builder
            .with_access_ttl(ChronoDuration::seconds(2))
            .with_auth_verify_settings(WyrdAuthVerifySettings {
                allowed_clock_skew: Duration::ZERO,
            })
    })
    .await;
    let request = run_request("code-review", "fn authority() {}");
    let submitted = fixture.token(&fixture.runner).await;
    fixture.upstream.hold();
    let key = new_key();
    let (status, run) = fixture.create(&submitted, &key, &request).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{run}");
    let run = run_of(run);
    let run_id = run.run_id.to_string();
    fixture.upstream.wait_arrivals(2).await;

    fixture
        .server
        .revoke_role(&fixture.runner.bootstrap, RUNNER_ROLE)
        .await
        .expect("runner role revokes");
    // Access-token expiry is wall-clock: the token must outlive its two
    // second lifetime with zero allowed skew.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let expired = "WYRD_AUTH_401_TOKEN_EXPIRED";
    problem(
        fixture.get(&submitted, &run_id).await,
        StatusCode::UNAUTHORIZED,
        expired,
    );
    problem(
        fixture.create(&submitted, &key, &request).await,
        StatusCode::UNAUTHORIZED,
        expired,
    );
    let revoked = fixture.token(&fixture.runner).await;
    let denial = "WYRD_PERMISSION_403_DENIED_RBAC";
    problem(
        fixture.get(&revoked, &run_id).await,
        StatusCode::FORBIDDEN,
        denial,
    );
    problem(
        fixture.cancel(&revoked, &run_id).await,
        StatusCode::FORBIDDEN,
        denial,
    );
    problem(
        fixture.create(&revoked, &key, &request).await,
        StatusCode::FORBIDDEN,
        denial,
    );

    fixture.upstream.release();
    fixture.upstream.wait_arrivals(3).await;
    fixture
        .server
        .grant_role(&fixture.runner.bootstrap, RUNNER_ROLE)
        .await
        .expect("runner role grants again");
    let restored = fixture.token(&fixture.runner).await;
    let finished = fixture.terminal(&restored, &run).await;
    assert_eq!(finished.status, WorkflowRunStatus::Succeeded);
    assert_eq!(finished.outputs["review"], json!("DONE"));
    let (status, replay) = fixture.create(&restored, &key, &request).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(run_of(replay), finished, "a replay never refreshes the run");
    assert_eq!(fixture.upstream.arrivals(), 3);
    fixture.settle_gateway().await;
    let invocations = fixture.decisions("gateway.invoke").await;
    let invocations = made_for(&invocations, &fixture.runner, "allowed");
    assert_eq!(
        invocations.len(),
        3,
        "each step, including the one after revocation, took its own gateway decision: {invocations:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
/// A registered Agent declaring both built-in tools reads seeded Bifrost rows
/// and a registered Card through the owning services, inheriting its own
/// space for the Card read, and the model sees the complete results.
/// Malformed arguments, non-SELECT SQL, an exceeded result ceiling, and an
/// unknown Card reach the model only as redacted stable codes; a call to an
/// undeclared tool fails the run. A caller without Card or query grants is
/// denied, and grants added after acceptance do not widen the run.
/// Cancelling a run while its query is held settles the query's resources
/// before the cancel answers, and a later query still runs.
///
/// # Panics
/// Panics when a run outcome, model-visible tool result, audit decision, or
/// query resource release differs.
async fn declared_tools_use_captured_scopes_and_owned_services() {
    let fixture = Fixture::start(ServerWorkflowConfig::default()).await;
    let seeded = seed_query_fixture(&fixture.server, "workflow-tools")
        .await
        .expect("query fixture seeds");
    let bundle = single_step("tool-review", "[bifrost.query, cards.get]", WYRD_GATEWAY, 0);
    fixture.register(&bundle.path().join("workflow.yaml")).await;
    let runner = &fixture.runner.token;
    let request = run_request("tool-review", "seeded rows");
    let select = format!("SELECT id, value FROM {} ORDER BY id", seeded.table);
    let prompt = json!({ "kind": "Prompt", "name": "tool-review-prompt", "version": "1.0.0" });

    fixture.upstream.reply(tool_calls(&[
        ("query", "bifrost.query", json!({ "sql": select })),
        ("card", "cards.get", prompt.clone()),
    ]));
    fixture.upstream.reply(text("ANSWERED"));
    let run = fixture.accept(runner, &request).await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    assert_eq!(run.outputs["answer"], json!("ANSWERED"));
    let calls = fixture.upstream.calls();
    assert_eq!(calls.len(), 2);
    let offered = calls[0].body["tools"].to_string();
    assert!(
        offered.contains("bifrost.query") && offered.contains("cards.get"),
        "{offered}"
    );
    let results = calls[1].body["messages"].to_string();
    assert!(
        results.contains("first") && results.contains("second"),
        "{results}"
    );
    assert!(
        results.contains("wyrd/v1"),
        "the Card envelope reaches the model: {results}"
    );

    fixture.upstream.reply(tool_calls(&[
        (
            "unknown-key",
            "bifrost.query",
            json!({ "sql": select, "path": "analytical" }),
        ),
        (
            "delete",
            "bifrost.query",
            json!({ "sql": format!("DELETE FROM {}", seeded.table) }),
        ),
        (
            "ceiling",
            "bifrost.query",
            json!({ "sql": select, "max_bytes": 1 }),
        ),
        (
            "absent",
            "cards.get",
            json!({ "kind": "Prompt", "name": "absent-prompt", "version": "1.0.0" }),
        ),
        (
            "tenant",
            "cards.get",
            json!({ "kind": "Prompt", "name": "x", "version": "1.0.0", "tenant": "other" }),
        ),
    ]));
    fixture.upstream.reply(text("HANDLED"));
    let run = fixture.accept(runner, &request).await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    let results = fixture.upstream.calls()[3].body["messages"].to_string();
    for code in [
        "WYRD_TOOL_422_INPUT",
        "WYRD_VALA_400_QUERY_INVALID_SQL",
        "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        "WYRD_REGISTRY_404_CARD_NOT_FOUND",
    ] {
        assert!(results.contains(code), "{code}: {results}");
    }
    assert!(
        !results.contains("first"),
        "no refused call leaks rows: {results}"
    );

    fixture
        .upstream
        .reply(tool_calls(&[("shell", "shell.exec", json!({}))]));
    let run = fixture.accept(runner, &request).await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Failed, "{run:?}");

    fixture
        .server
        .seed_role(
            "workflow_limited",
            &[Permission::workflow_run(), openai_invoke()],
        )
        .await
        .expect("limited role seeds");
    let limited = fixture
        .principal("workflow-limited", &["workflow_limited"])
        .await;
    fixture.upstream.hold();
    let before = fixture.upstream.arrivals();
    let run = fixture.accept(&limited.token, &request).await;
    fixture.upstream.wait_arrivals(before + 1).await;
    fixture
        .server
        .grant_role(&limited.bootstrap, RUNNER_ROLE)
        .await
        .expect("runner role grants");
    fixture.upstream.reply(tool_calls(&[
        ("query", "bifrost.query", json!({ "sql": select })),
        ("card", "cards.get", prompt),
    ]));
    fixture.upstream.reply(text("LIMITED"));
    fixture.upstream.release();
    let run = fixture.terminal(&limited.token, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    let results = fixture.upstream.calls()[before + 1].body["messages"].to_string();
    assert!(
        results.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
        "the run keeps its accepted grants: {results}"
    );
    assert!(
        !results.contains("first") && !results.contains("wyrd/v1"),
        "{results}"
    );

    fixture.server.stall_next_query_after_schema();
    fixture.upstream.reply(tool_calls(&[(
        "held",
        "bifrost.query",
        json!({ "sql": select }),
    )]));
    let run = fixture.accept(runner, &request).await;
    let query_id = fixture
        .server
        .wait_query_schema_stall()
        .await
        .expect("the tool query reaches its schema stall");
    let (status, cancelled) = fixture.cancel(runner, &run.run_id.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(run_of(cancelled).status, WorkflowRunStatus::Cancelled);
    assert_eq!(
        fixture
            .server
            .wait_bifrost_query_resources_released(&query_id, RELEASED)
            .await
            .expect("query resources inspect"),
        RELEASED,
        "the cancelled run's query returned every resource"
    );
    let before = fixture.upstream.arrivals();
    fixture.upstream.reply(tool_calls(&[(
        "again",
        "bifrost.query",
        json!({ "sql": select }),
    )]));
    fixture.upstream.reply(text("AGAIN"));
    let run = fixture.accept(runner, &request).await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    assert!(
        fixture.upstream.calls()[before + 1].body["messages"]
            .to_string()
            .contains("second")
    );

    let card_reads = fixture.decisions("card.read.ref").await;
    let allowed = made_for(&card_reads, &fixture.runner, "allowed");
    assert_eq!(allowed.len(), 2, "{allowed:?}");
    let denied = made_for(&card_reads, &limited, "denied");
    assert_eq!(denied.len(), 1, "{denied:?}");
}

#[tokio::test(flavor = "multi_thread")]
/// A `wyrd_gateway` step reaches the provider only through the governed
/// gateway — with the deployment's provider key, a ledger entry, and an
/// audited invoke decision — while an `ext_gateway` step goes straight to
/// its tenant-assigned binding with the binding's secret header and no
/// gateway key, ledger entry, or decision. A binding assigned to another
/// tenant is unavailable before any call, and no secret reaches a run.
///
/// # Panics
/// Panics when an upstream header, ledger count, decision count, run
/// outcome, or refusal differs.
async fn server_routes_keep_gateway_and_external_ownership() {
    let fixture = Fixture::start(ServerWorkflowConfig::default()).await;
    let secrets = TempDir::new().expect("secret directory creates");
    let secret = secrets.path().join("review-secret");
    std::fs::write(&secret, EXTERNAL_SECRET).expect("secret writes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
            .expect("secret restricts");
    }
    let origin = fixture
        .upstream
        .url
        .as_str()
        .trim_end_matches('/')
        .to_owned();
    let binding = |tenant: String| {
        json!({
            "tenant": tenant,
            "protocol": "openai_chat",
            "origin": origin,
            "secret_headers": { SECRET_HEADER: { "source": "file", "path": secret } },
        })
    };
    let bound = config(json!({
        "external_gateway_bindings": {
            "review-gateway": binding(fixture.server.data_tenant_id().to_string()),
            "foreign-gateway": binding(uuid::Uuid::now_v7().to_string()),
        }
    }));
    let fixture = fixture.restart(bound).await;
    for (name, route) in [
        (
            "external-review",
            external_route(&fixture.upstream, "review-gateway"),
        ),
        (
            "foreign-review",
            external_route(&fixture.upstream, "foreign-gateway"),
        ),
        ("gateway-review", WYRD_GATEWAY.to_owned()),
    ] {
        let bundle = single_step(name, "[]", &route, 0);
        fixture.register(&bundle.path().join("workflow.yaml")).await;
    }
    let runner = &fixture.runner.token;
    let accounted = fixture.accounted_calls().await;

    let run = fixture
        .accept(runner, &run_request("external-review", "x"))
        .await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    assert!(
        !serde_json::to_string(&run)
            .expect("run serializes")
            .contains(EXTERNAL_SECRET)
    );
    let calls = fixture.upstream.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].secret.as_deref(), Some(EXTERNAL_SECRET));
    assert!(
        !calls[0]
            .authorization
            .as_deref()
            .unwrap_or_default()
            .contains(PROVIDER_KEY)
    );
    assert_eq!(
        fixture.accounted_calls().await,
        accounted,
        "no gateway ledger entry"
    );

    let run = fixture
        .accept(runner, &run_request("gateway-review", "x"))
        .await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
    let calls = fixture.upstream.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].authorization.as_deref(),
        Some(format!("Bearer {PROVIDER_KEY}").as_str())
    );
    assert_eq!(calls[1].secret, None);
    assert_eq!(fixture.accounted_calls().await, accounted + 1);

    problem(
        fixture
            .create(runner, &new_key(), &run_request("foreign-review", "x"))
            .await,
        StatusCode::SERVICE_UNAVAILABLE,
        "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
    );
    assert_eq!(fixture.upstream.arrivals(), 2);
    let invocations = fixture.decisions("gateway.invoke").await;
    let invocations = made_for(&invocations, &fixture.runner, "allowed");
    assert_eq!(invocations.len(), 1, "{invocations:?}");
}

#[tokio::test(flavor = "multi_thread")]
/// Cancelling a running run commits one whole terminal snapshot — no step
/// left pending or running — that later upstream answers cannot change, and
/// frees its slot. A run deadline times the run out the same way. A cancel
/// racing completion returns exactly the snapshot that won. Retention evicts
/// the oldest terminal run and its key, expires runs after 24 hours, and an
/// evicted, expired, foreign, unknown, or malformed id is the same 404. A
/// restart loses every run without resuming its calls, and shutdown cancels
/// a mid-preparation create and a running run within the shared deadline.
///
/// # Panics
/// Panics when a snapshot, status, refusal, upstream count, or shutdown
/// outcome differs.
async fn lifecycle_races_retention_and_shutdown() {
    let bounds = json!({
        "max_active_per_tenant": 2,
        "max_retained_per_tenant": 2,
        "max_retained_global": 4,
    });
    let fixture = Fixture::start(config(bounds.clone())).await;
    let runner = fixture.runner.token.clone();
    let request = run_request("code-review", "fn lifecycle() {}");

    fixture.upstream.hold();
    let first_key = new_key();
    let (status, first) = fixture.create(&runner, &first_key, &request).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    let first = run_of(first);
    let first_id = first.run_id.to_string();
    fixture.upstream.wait_arrivals(2).await;
    let (status, cancelled) = fixture.cancel(&runner, &first_id).await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let cancelled = run_of(cancelled);
    assert_eq!(cancelled.status, WorkflowRunStatus::Cancelled);
    assert_eq!(
        step_status(&cancelled, "security"),
        WorkflowStepStatus::Cancelled
    );
    assert_eq!(
        step_status(&cancelled, "correctness"),
        WorkflowStepStatus::Cancelled
    );
    assert_eq!(
        step_status(&cancelled, "final_review"),
        WorkflowStepStatus::Unstarted
    );
    fixture.upstream.release();
    let (_, again) = fixture.cancel(&runner, &first_id).await;
    assert_eq!(
        run_of(again),
        cancelled,
        "a terminal run is returned unchanged"
    );

    fixture.upstream.hold();
    let mut short = request.clone();
    short["timeout_seconds"] = json!(1);
    let timed = fixture.accept(&runner, &short).await;
    let timed = fixture.terminal(&runner, &timed).await;
    fixture.upstream.release();
    assert_eq!(timed.status, WorkflowRunStatus::TimedOut);
    assert_eq!(
        timed.error.as_ref().map(|error| error.code.as_str()),
        Some("WYRD_WORKFLOW_504_RUN_TIMEOUT")
    );
    assert!(timed.steps.values().all(|step| !matches!(
        step.status,
        WorkflowStepStatus::Pending | WorkflowStepStatus::Running
    )));
    let (_, read) = fixture.get(&runner, &first_id).await;
    assert_eq!(
        run_of(read),
        cancelled,
        "late upstream answers change nothing"
    );

    let raced = fixture.accept(&runner, &request).await;
    let (status, won) = fixture.cancel(&runner, &raced.run_id.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{won}");
    let won = run_of(won);
    assert!(matches!(
        won.status,
        WorkflowRunStatus::Succeeded | WorkflowRunStatus::Cancelled
    ));
    let (_, read) = fixture.get(&runner, &raced.run_id.to_string()).await;
    assert_eq!(
        run_of(read),
        won,
        "the cancel answers the snapshot that won"
    );

    let not_found = |answer: (StatusCode, Value)| {
        let body = problem(
            answer,
            StatusCode::NOT_FOUND,
            "WYRD_WORKFLOW_404_RUN_NOT_FOUND",
        );
        (body["title"].clone(), body["detail"].clone())
    };
    let evicted = not_found(fixture.get(&runner, &first_id).await);
    let other = fixture
        .principal("workflow-runner-two", &[RUNNER_ROLE])
        .await;
    for answer in [
        fixture
            .get(&runner, &uuid::Uuid::now_v7().to_string())
            .await,
        fixture.get(&runner, "not-a-run").await,
        fixture.cancel(&runner, "not-a-run").await,
        fixture.get(&other.token, &timed.run_id.to_string()).await,
        fixture
            .cancel(&other.token, &timed.run_id.to_string())
            .await,
    ] {
        assert_eq!(
            not_found(answer),
            evicted,
            "every hidden run looks the same"
        );
    }
    let (status, reused) = fixture.create(&runner, &first_key, &request).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "eviction removed the key: {reused}"
    );
    let reused = run_of(reused);
    assert_ne!(reused.run_id, first.run_id);
    fixture.terminal(&runner, &reused).await;
    fixture
        .server
        .state()
        .workflows
        .advance_clock_for_test(Duration::from_secs(25 * 60 * 60));
    assert_eq!(
        not_found(fixture.get(&runner, &reused.run_id.to_string()).await),
        evicted,
        "a run expires 24 hours after it terminated"
    );

    fixture.upstream.hold();
    let lost = fixture.accept(&runner, &request).await;
    fixture
        .upstream
        .wait_arrivals(fixture.upstream.arrivals() + 2)
        .await;
    let fixture = fixture.restart(config(bounds)).await;
    let runner = fixture.runner.token.clone();
    let after_restart = fixture.upstream.arrivals();
    assert_eq!(
        not_found(fixture.get(&runner, &lost.run_id.to_string()).await),
        evicted,
        "a restart loses process-local runs"
    );
    fixture.upstream.release();
    let fresh = fixture.accept(&runner, &request).await;
    assert_eq!(
        fixture.terminal(&runner, &fresh).await.status,
        WorkflowRunStatus::Succeeded
    );
    assert_eq!(
        fixture.upstream.arrivals(),
        after_restart + 3,
        "the lost run never resumed"
    );

    fixture.upstream.hold();
    let running = fixture.accept(&runner, &request).await;
    assert_eq!(running.status, WorkflowRunStatus::Queued);
    fixture.upstream.wait_arrivals(after_restart + 5).await;
    let other = fixture
        .principal("workflow-runner-three", &[RUNNER_ROLE])
        .await;
    let workflows = &fixture.server.state().workflows;
    workflows.stall_next_preparation_for_test();
    let preparing = tokio::spawn(send(fixture.create_request(
        &other.token,
        &new_key(),
        &request,
    )));
    tokio::time::timeout(PATIENCE, workflows.wait_preparation_stall_for_test())
        .await
        .expect("the preparation stops at the gate");
    tokio::time::timeout(PATIENCE, fixture.server.shutdown_and_inspect())
        .await
        .expect("shutdown finishes")
        .expect("Workflow runs drained within the shutdown deadline");
    problem(
        preparing.await.expect("the preparing create answers"),
        StatusCode::SERVICE_UNAVAILABLE,
        "WYRD_WORKFLOW_503_RUN_UNAVAILABLE",
    );
}

#[tokio::test(flavor = "multi_thread")]
/// Graph bounds refuse a Workflow with too many steps, too many dependency
/// edges, or too many resolved body bytes, and an
/// oversized input, all before any provider call and without leaking the
/// only active slot. A step result over its bound fails the run with a
/// complete bounded snapshot that carries none of the oversized output.
/// While a preparation is held, Card reads and direct gateway calls stay
/// serviceable.
///
/// # Panics
/// Panics when a refusal, run outcome, snapshot size, or sibling request
/// differs.
async fn graph_and_snapshot_limits_preserve_sibling_services() {
    let bounds = config(json!({
        "max_active_per_tenant": 1,
        "max_steps_per_run": 4,
        "max_dependency_edges_per_run": 2,
        "max_resolved_graph_bytes": 16384,
        "max_input_bytes": 2048,
        "max_step_result_bytes": 2048,
        "max_run_bytes": 65536,
    }));
    let max_run_bytes = bounds.max_run_bytes;
    let fixture = Fixture::start(bounds).await;
    let runner = &fixture.runner.token;
    let extra_step = |id: &str| {
        format!(
            "    - id: {id}\n      action:\n        type: agent\n        target:\n          kind: Agent\n          name: security-reviewer\n          version: \"1.0.0\"\n      inputs:\n        code: input.code\n\n    - id: correctness"
        )
    };
    let wide = code_review_variant("wide-review", |yaml| {
        yaml.replacen("    - id: correctness", &extra_step("extra_one"), 1)
            .replacen("    - id: correctness", &extra_step("extra_two"), 1)
    });
    let dense = code_review_variant("dense-review", |yaml| {
        yaml.replacen("    - id: correctness", &extra_step("extra_one"), 1)
            .replacen(
                "depends_on: [security, correctness]",
                "depends_on: [security, correctness, extra_one]",
                1,
            )
    });
    let heavy = single_step("heavy-review", "[]", WYRD_GATEWAY, 20_000);
    let bulky = single_step("bulky-review", "[]", WYRD_GATEWAY, 0);
    for bundle in [&wide, &dense, &heavy, &bulky] {
        fixture.register(&bundle.path().join("workflow.yaml")).await;
    }
    let too_large = "WYRD_WORKFLOW_413_GRAPH_TOO_LARGE";
    for (name, bound) in [
        ("wide-review", "max_steps_per_run"),
        ("dense-review", "max_dependency_edges_per_run"),
        ("heavy-review", "max_resolved_graph_bytes"),
    ] {
        let refusal = problem(
            fixture
                .create(runner, &new_key(), &run_request(name, "x"))
                .await,
            StatusCode::PAYLOAD_TOO_LARGE,
            too_large,
        );
        assert!(refusal.to_string().contains(bound), "{name}: {refusal}");
    }
    problem(
        fixture
            .create(
                runner,
                &new_key(),
                &run_request("code-review", &"x".repeat(2048)),
            )
            .await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "WYRD_WORKFLOW_413_INPUT_TOO_LARGE",
    );
    assert_eq!(fixture.upstream.arrivals(), 0);

    let oversized = "y".repeat(4096);
    fixture.upstream.reply(text(&oversized));
    let run = fixture
        .accept(runner, &run_request("bulky-review", "x"))
        .await;
    let run = fixture.terminal(runner, &run).await;
    assert_eq!(run.status, WorkflowRunStatus::Failed, "{run:?}");
    let step = &run.steps["answer"];
    assert_eq!(step.status, WorkflowStepStatus::Failed);
    assert_eq!(
        step.error.as_ref().map(|error| error.code.as_str()),
        Some("WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE")
    );
    let snapshot = serde_json::to_string(&run).expect("run serializes");
    assert!(!snapshot.contains(&oversized));
    assert!(run.canonical_len() <= max_run_bytes);

    let workflows = &fixture.server.state().workflows;
    workflows.stall_next_preparation_for_test();
    let creator = tokio::spawn(send(fixture.create_request(
        runner,
        &new_key(),
        &run_request("code-review", "fn sibling() {}"),
    )));
    tokio::time::timeout(PATIENCE, workflows.wait_preparation_stall_for_test())
        .await
        .expect("the preparation stops at the gate");
    let (status, card) = send(
        fixture
            .http
            .get(format!(
                "{}/v1/cards/Workflow/engineering/code-review/latest",
                fixture.base
            ))
            .header(ACCESS_TOKEN_HEADER, format!("Bearer {runner}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{card}");
    let (status, completion) = send(
        fixture
            .http
            .post(format!("{}/v1/chat/completions", fixture.base))
            .header("authorization", format!("Bearer {runner}"))
            .json(&json!({
                "model": "openai/gpt-5-5",
                "messages": [{ "role": "user", "content": "sibling" }],
            })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completion}");
    workflows.release_preparation_for_test();
    let (status, run) = creator.await.expect("creator answers");
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "the only slot was never leaked: {run}"
    );
    assert_eq!(
        fixture.terminal(runner, &run_of(run)).await.status,
        WorkflowRunStatus::Succeeded
    );
}
