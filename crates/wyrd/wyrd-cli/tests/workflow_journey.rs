//! Compiled-CLI journeys for `wyrd workflow`.

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read as _};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use assert_cmd::prelude::*;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::watch;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_client::{Workflows, WyrdClient};
use wyrd_runtime::Permission;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::card::workflow::{WorkflowRun, WorkflowRunStatus};
use wyrd_spec::ids::WorkflowRunId;
use wyrd_testing::WyrdTestServer;

/// Secret header value the local external gateway binding reads from a file.
const SECRET: &str = "cli-workflow-binding-secret";

/// Absolute path of a repository file.
fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative)
}

/// The compiled `wyrd <args>` with `config_home` as the only client
/// configuration and no ambient credential or endpoint.
///
/// # Panics
/// Panics when the compiled binary cannot be found.
fn wyrd(config_home: &Path, args: &[&str]) -> Command {
    let mut command = Command::cargo_bin("wyrd").expect("wyrd binary");
    command
        .args(args)
        .env("WYRD_CONFIG_HOME", config_home)
        .env_remove("WYRD_SERVER_URL")
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY");
    command
}

/// Runs `wyrd workflow <args>` through [`wyrd`].
///
/// # Panics
/// Panics when the compiled binary cannot be run.
fn workflow(config_home: &Path, args: &[&str]) -> Output {
    wyrd(config_home, &[&["workflow"], args].concat())
        .output()
        .expect("compiled workflow command runs")
}

/// Both output streams, for leak and diagnostic assertions.
fn streams(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The structured problem the CLI writes to stderr.
///
/// # Panics
/// Panics when stderr carries no problem JSON line.
fn problem(output: &Output) -> Value {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| panic!("CLI emits structured problem JSON: {}", streams(output)))
}

/// The run snapshot the CLI writes to stdout in JSON mode.
///
/// # Panics
/// Panics when stdout is not JSON.
fn run_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("stdout is a run snapshot: {}", streams(output)))
}

/// Copy the checked-in code-review example into a temporary directory with
/// its Workflow route replaced by `route`.
///
/// # Panics
/// Panics when the example cannot be copied.
fn code_review_with_route(route: &str) -> tempfile::TempDir {
    let bundle = repo("examples/workflows/code-review");
    let temp = tempfile::tempdir().expect("bundle tempdir");
    for dir in ["agents", "prompts"] {
        std::fs::create_dir(temp.path().join(dir)).expect("bundle directory");
        for file in ["security", "correctness", "final-reviewer"] {
            std::fs::copy(
                bundle.join(format!("{dir}/{file}.yaml")),
                temp.path().join(format!("{dir}/{file}.yaml")),
            )
            .expect("bundle file copies");
        }
    }
    let yaml = std::fs::read_to_string(bundle.join("workflow.yaml")).expect("workflow reads");
    assert!(
        yaml.contains("    kind: wyrd_gateway"),
        "example route moved"
    );
    std::fs::write(
        temp.path().join("workflow.yaml"),
        yaml.replacen("    kind: wyrd_gateway", route, 1),
    )
    .expect("workflow writes");
    temp
}

/// The `ext_gateway` route for `upstream`, naming the `review-gateway`
/// binding.
fn external_route(upstream: &MockServer) -> String {
    format!(
        "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}/v1\n    credential_binding: review-gateway",
        upstream.uri()
    )
}

/// Write a client configuration whose `review-gateway` binding speaks
/// `protocol` to `origin` and sends the owner-only secret file as
/// `x-review-secret`.
///
/// # Panics
/// Panics when the files cannot be written.
fn configure_binding(config_home: &Path, origin: &str, protocol: &str) {
    let secret = config_home.join("review-secret");
    std::fs::write(&secret, SECRET).expect("secret writes");
    #[cfg(unix)]
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
        .expect("secret restricts");
    std::fs::write(
        config_home.join("config.toml"),
        format!(
            "[workflow.external_gateway_bindings.review-gateway]\n\
             protocol = \"{protocol}\"\n\
             origin = \"{origin}\"\n\
             secret_headers = {{ x-review-secret = {{ source = \"file\", path = \"{}\" }} }}\n",
            secret.display()
        ),
    )
    .expect("config writes");
}

/// A Chat Completions upstream answering every call with `status` and `body`.
async fn chat_upstream(status: u16, body: Value) -> MockServer {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&upstream)
        .await;
    upstream
}

/// A Chat Completions success whose assistant message is `text`.
fn chat_completion(text: &str) -> Value {
    json!({
        "id": "resp",
        "object": "chat.completion",
        "created": 0,
        "model": "gpt-5-5",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
}

/// Every `wyrd workflow` choice is unambiguous and validated before any side
/// effect, and local runs print the portable run snapshot.
///
/// Malformed, mixed, or incomplete selectors, both inputs, non-object input,
/// a file source on the server, `--detach` locally, the `--server` option
/// these commands do not take, and a malformed run ID all fail with exit code
/// 64; the ones that parse carry `WYRD_CLI_400_INVALID_ARGUMENT`, which proves
/// no client was built. An invalid execution choice is refused before a
/// missing `--input-file` is read.
/// The actual code-review bundle then runs on a local `ext_gateway` binding
/// whose secret is read from a file at run time and never printed; a
/// Native mock Workflow renders named outputs and, only with `--steps`, its
/// step results. An unconfigured binding is refused before dispatch and a
/// failed run exits 2 with its snapshot.
///
/// # Panics
/// Panics when any invocation's exit code, problem, output, or upstream
/// traffic differs from the asserted contract.
#[tokio::test(flavor = "multi_thread")]
async fn workflow_cli_contract() {
    let config_home = tempfile::tempdir().expect("config home");
    let home = config_home.path();
    let local = repo("tests/fixtures/workflow-loading/shadowed/local-workflow.yaml");
    let local = local.to_str().expect("utf-8 fixture path");
    let uid = "0190d6a0-0000-7000-8000-000000000000";

    let usage: [&[&str]; 9] = [
        &["run"],
        &["run", "--file", local, "--uid", uid],
        &[
            "run",
            "--uid",
            uid,
            "--space",
            "s",
            "--name",
            "n",
            "--version",
            "1.0.0",
        ],
        &["run", "--space", "s", "--name", "n"],
        &["run", "--name", "n", "--version", "1.0.0"],
        &[
            "run",
            "--file",
            local,
            "--input",
            "{}",
            "--input-file",
            local,
        ],
        &["run", "--file", local, "--server", "http://127.0.0.1:9"],
        &["run", "--file", local, "--execution", "remote"],
        &["status"],
    ];
    for args in usage {
        let output = workflow(home, args);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{args:?}: {}",
            streams(&output)
        );
    }

    let missing_input = home.join("missing-input.json");
    let missing_input = missing_input.to_str().expect("utf-8 temp path");
    let invalid: [(&[&str], &str); 9] = [
        (&["run", "--file", local, "--input", "[1]"], "input"),
        (&["run", "--file", local, "--input", "{not json"], "input"),
        (&["run", "--file", local, "--execution", "server"], "file"),
        (&["run", "--file", local, "--detach"], "detach"),
        (
            &[
                "run",
                "--file",
                local,
                "--execution",
                "server",
                "--input-file",
                missing_input,
            ],
            "file",
        ),
        (
            &[
                "run",
                "--file",
                local,
                "--detach",
                "--input-file",
                missing_input,
            ],
            "detach",
        ),
        (&["run", "--uid", "not-a-uid"], "uid"),
        (&["status", "not-a-run"], "run-id"),
        (&["cancel", "not-a-run"], "run-id"),
    ];
    for (args, field) in invalid {
        let output = workflow(home, args);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{args:?}: {}",
            streams(&output)
        );
        let problem = problem(&output);
        assert_eq!(
            problem["code"], "WYRD_CLI_400_INVALID_ARGUMENT",
            "{problem}"
        );
        assert!(streams(&output).contains(field), "{args:?} names {field}");
        assert!(
            !streams(&output).contains("not json"),
            "input is never echoed"
        );
    }

    let native = workflow(
        home,
        &["run", "--file", local, "--input", r#"{"code":"diff"}"#],
    );
    assert_eq!(native.status.code(), Some(0), "{}", streams(&native));
    let text = String::from_utf8_lossy(&native.stdout);
    assert!(text.contains(" succeeded"), "{text}");
    assert!(
        text.contains("output security: local security review of diff"),
        "{text}"
    );
    assert!(!text.contains("step "), "steps only on request: {text}");
    let native = workflow(
        home,
        &[
            "run",
            "--file",
            local,
            "--input",
            r#"{"code":"diff"}"#,
            "--steps",
        ],
    );
    let text = String::from_utf8_lossy(&native.stdout);
    for step in ["security", "correctness", "final_review"] {
        assert!(text.contains(&format!("step {step}: succeeded")), "{text}");
    }

    let upstream = chat_upstream(200, chat_completion("REVIEWED")).await;
    let bundle = code_review_with_route(&external_route(&upstream));
    let entry = bundle.path().join("workflow.yaml");
    let entry = entry.to_str().expect("utf-8 bundle path");
    let input = repo("examples/workflows/code-review/input.json");
    let input = input.to_str().expect("utf-8 input path");

    let unconfigured = workflow(home, &["run", "--file", entry, "--input-file", input]);
    assert_eq!(
        problem(&unconfigured)["code"],
        "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"
    );
    assert!(
        upstream
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );

    configure_binding(home, &upstream.uri(), "openai_chat");
    let external = workflow(
        home,
        &[
            "run",
            "--file",
            entry,
            "--input-file",
            input,
            "--format",
            "json",
        ],
    );
    assert_eq!(external.status.code(), Some(0), "{}", streams(&external));
    let run = run_json(&external);
    assert_eq!(run["status"], "succeeded");
    assert_eq!(run["workflow"], Value::Null);
    assert_eq!(run["outputs"], json!({ "review": "REVIEWED" }));
    for step in ["security", "correctness", "final_review"] {
        assert_eq!(run["steps"][step]["status"], "succeeded", "{run}");
    }
    let requests = upstream.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|request| {
        request
            .headers
            .get("x-review-secret")
            .map(axum::http::HeaderValue::as_bytes)
            == Some(SECRET.as_bytes())
    }));
    assert!(!streams(&external).contains(SECRET));

    let refusing = chat_upstream(400, json!({ "error": { "message": "refused" } })).await;
    let refused_bundle = code_review_with_route(&external_route(&refusing));
    configure_binding(home, &refusing.uri(), "openai_chat");
    let refused_entry = refused_bundle.path().join("workflow.yaml");
    let failed = workflow(
        home,
        &[
            "run",
            "--file",
            refused_entry.to_str().expect("utf-8 bundle path"),
            "--input-file",
            input,
            "--format",
            "json",
        ],
    );
    assert_eq!(failed.status.code(), Some(2), "{}", streams(&failed));
    let run = run_json(&failed);
    assert_eq!(run["status"], "failed", "{run}");
    assert_eq!(run["outputs"], json!({}));
    assert!(!streams(&failed).contains(SECRET));
}

/// Role holding what a Workflow caller needs for every journey route.
const RUNNER_ROLE: &str = "cli_workflow_runner";

/// Provider key the tenant administrator submits for every deployment.
const PROVIDER_KEY: &str = "sk-cli-workflow-upstream";

/// Bound for every wait on the server or the upstream.
const PATIENCE: Duration = Duration::from_secs(30);

/// One provider request as the upstream received it.
#[derive(Debug, Clone)]
struct UpstreamCall {
    /// Request path, naming the provider dialect.
    path: String,
    /// `authorization` header, as the gateway sends its provider key.
    authorization: Option<String>,
    /// `x-review-secret` header, as an external binding sends its secret.
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
}

/// Loopback provider upstream answering `DONE` in `OpenAI` Chat Completions
/// and Responses, Anthropic Messages, and Gemini and Vertex
/// `generateContent`.
struct Upstream {
    /// Origin the gateway and external bindings reach it at.
    url: Url,
    /// State shared with the serving task.
    script: Arc<Script>,
}

impl Upstream {
    /// Bind a loopback listener and serve the scripted answers on it.
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
        });
        let app = axum::Router::new()
            .route("/v1/chat/completions", axum::routing::post(answer))
            .route("/v1/responses", axum::routing::post(answer))
            .route("/v1/messages", axum::routing::post(answer))
            .route("/v1beta/models/{call}", axum::routing::post(answer))
            .route(
                "/v1/projects/{project}/locations/{location}/publishers/google/models/{call}",
                axum::routing::post(answer),
            )
            .with_state(Arc::clone(&script));
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url, script }
    }

    /// Origin without a trailing slash, as bindings and routes name it.
    fn origin(&self) -> String {
        self.url.as_str().trim_end_matches('/').to_owned()
    }

    /// Make every response wait until [`Self::release`].
    fn hold(&self) {
        self.script.held.send_replace(true);
    }

    /// Let every waiting and later response proceed.
    fn release(&self) {
        self.script.held.send_replace(false);
    }

    /// Every request received so far.
    fn calls(&self) -> Vec<UpstreamCall> {
        self.script
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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

/// Record one request, wait while held, and answer `DONE` in the dialect
/// its path names.
///
/// # Panics
/// Panics if the hold sender is gone, which the script owns for its life.
async fn answer(
    State(script): State<Arc<Script>>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let path = uri.path().to_owned();
    script
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(UpstreamCall {
            path: path.clone(),
            authorization: header("authorization"),
            secret: header("x-review-secret"),
            body,
        });
    script.arrivals.send_modify(|arrived| *arrived += 1);
    script
        .held
        .subscribe()
        .wait_for(|held| !held)
        .await
        .expect("the hold sender lives with the script");
    Json(match path.as_str() {
        "/v1/chat/completions" => json!({
            "id": "chatcmpl-workflow",
            "object": "chat.completion",
            "created": 0,
            "model": "gpt-5-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "DONE" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        }),
        "/v1/responses" => json!({
            "id": "resp_workflow",
            "object": "response",
            "created_at": 1,
            "status": "completed",
            "model": "gpt-5-4",
            "output": [{ "type": "message", "id": "msg_workflow", "status": "completed", "role": "assistant",
                         "content": [{ "type": "output_text", "text": "DONE", "annotations": [] }] }],
            "usage": { "input_tokens": 5, "output_tokens": 2, "total_tokens": 7 }
        }),
        "/v1/messages" => json!({
            "id": "msg_workflow",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-5",
            "content": [{ "type": "text", "text": "DONE" }],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": { "input_tokens": 5, "output_tokens": 2 },
            "container": null
        }),
        _ => json!({
            "candidates": [{ "content": { "role": "model", "parts": [{ "text": "DONE" }] },
                             "finishReason": "STOP", "index": 0 }],
            "usageMetadata": { "promptTokenCount": 5, "candidatesTokenCount": 2, "totalTokenCount": 7 },
            "modelVersion": "gemini-2.5-flash",
            "responseId": "resp_workflow"
        }),
    })
}

/// A bound server whose built-in provider adapters reach one scripted
/// upstream, with an administrator, a runner, and a client configuration
/// home the compiled CLI uses.
struct Journey {
    /// Bound server under test.
    server: WyrdTestServer,
    /// Scripted upstream behind the gateway and external bindings.
    upstream: Upstream,
    /// Server base URL.
    base: String,
    /// Access token of the tenant administrator who deploys and applies.
    admin: String,
    /// Access token of the caller holding [`RUNNER_ROLE`].
    runner: String,
    /// Client configuration home of every CLI invocation.
    home: tempfile::TempDir,
}

impl Journey {
    /// Start the server, seed [`RUNNER_ROLE`] with Workflow run, Card read,
    /// and invoke on every built-in provider, bootstrap both principals, and
    /// deploy `openai/gpt-5-5` for Chat Completions.
    ///
    /// # Panics
    /// Panics when the server, role, principals, or deployment cannot be set
    /// up.
    async fn start() -> Self {
        let upstream = Upstream::start().await;
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_gateway_provider_root_for_test(upstream.url.clone())
                .start_bound(),
        )
        .await
        .expect("bound test server starts");
        let invoke = |provider: &str| {
            Permission::gateway_invoke(GatewayAccess::Provider {
                provider: provider.parse().expect("provider id"),
            })
        };
        server
            .seed_role(
                RUNNER_ROLE,
                &[
                    Permission::workflow_run(),
                    Permission::card_read(),
                    invoke("openai"),
                    invoke("anthropic"),
                    invoke("gemini"),
                    invoke("vertex"),
                ],
            )
            .await
            .expect("runner role seeds");
        let token = |bootstrap: wyrd_testing::Bootstrap| {
            bootstrap.jwt().expect("users carry a token").to_owned()
        };
        let admin = token(
            server
                .bootstrap_user("cli-workflow-admin", &["admin"])
                .await
                .expect("admin bootstraps"),
        );
        let runner = token(
            server
                .bootstrap_user("cli-workflow-runner", &[RUNNER_ROLE])
                .await
                .expect("runner bootstraps"),
        );
        let journey = Self {
            base: server.base_url().expect("server is bound").to_owned(),
            server,
            upstream,
            admin,
            runner,
            home: tempfile::tempdir().expect("config home"),
        };
        journey.deploy("openai", "authorization", "gpt-5-5", &["chat_completions"]);
        journey
    }

    /// The compiled `wyrd <args>` against this server as `token`.
    fn command(&self, token: &str, args: &[&str]) -> Command {
        let mut command = wyrd(self.home.path(), args);
        command
            .env("WYRD_SERVER_URL", &self.base)
            .env("WYRD_ACCESS_TOKEN", token);
        command
    }

    /// Run the compiled `wyrd <args>` as `token` to completion.
    ///
    /// # Panics
    /// Panics when the binary cannot be run.
    fn cli(&self, token: &str, args: &[&str]) -> Output {
        self.command(token, args)
            .output()
            .expect("compiled command runs")
    }

    /// Run `wyrd workflow run <args> --format json` as the runner on the
    /// blocking pool, so the scripted upstream keeps serving, and return its
    /// process code and snapshot.
    ///
    /// # Panics
    /// Panics when the command cannot run or prints no snapshot.
    async fn run(&self, args: &[&str]) -> (Option<i32>, Value) {
        let mut command = self.command(
            &self.runner,
            &[&["workflow", "run"], args, &["--format", "json"]].concat(),
        );
        let output = tokio::task::spawn_blocking(move || command.output())
            .await
            .expect("command task joins")
            .expect("compiled command runs");
        (output.status.code(), run_json(&output))
    }

    /// Submit [`PROVIDER_KEY`] as `{provider}-key` and deploy built-in
    /// `provider`'s `model` serving `capabilities`, authenticated by
    /// `header`, through the compiled `wyrd gateway` commands. A `vertex`
    /// deployment is placed in project `acme` at `us-central1`.
    ///
    /// # Panics
    /// Panics when either administration command fails.
    fn deploy(&self, provider: &str, header: &str, model: &str, capabilities: &[&str]) {
        let credential = format!("{provider}-key");
        let adapter = if provider == "vertex" {
            json!({ "vertex": { "project": "acme", "location": "us-central1" } })
        } else {
            json!(provider)
        };
        let auth = if header == "authorization" {
            json!({ "bearer": { "credential": credential } })
        } else {
            json!({ "api_key_header": { "header": header, "credential": credential } })
        };
        for (resource, body) in [
            (
                "credential",
                json!({
                    "name": credential,
                    "provider": provider,
                    "source": { "managed_secret": { "secret": PROVIDER_KEY } },
                }),
            ),
            (
                "deployment",
                json!({
                    "name": model.replace('.', "-"),
                    "model": { "provider": provider, "model": model },
                    "adapter": adapter,
                    "auth": auth,
                    "capabilities": capabilities,
                    "routing_weight": 1,
                }),
            ),
        ] {
            let document = self.home.path().join(format!("{resource}.json"));
            std::fs::write(&document, body.to_string()).expect("document writes");
            let output = self.cli(
                &self.admin,
                &[
                    "gateway",
                    resource,
                    "put",
                    "--file",
                    document.to_str().expect("utf-8 path"),
                ],
            );
            assert!(output.status.success(), "{resource}: {}", streams(&output));
        }
    }

    /// Register the Card tree at `path` with the compiled `wyrd apply` as the
    /// administrator and return each registered Card's exact reference by
    /// name.
    ///
    /// # Panics
    /// Panics when the apply fails or prints no receipt.
    fn apply(&self, path: &Path) -> HashMap<String, Value> {
        let output = self.cli(
            &self.admin,
            &[
                "apply",
                path.to_str().expect("utf-8 path"),
                "--format",
                "json",
            ],
        );
        assert!(output.status.success(), "apply: {}", streams(&output));
        let receipt: Value = serde_json::from_slice(&output.stdout).expect("receipt is JSON");
        receipt["outcomes"]
            .as_array()
            .expect("receipt lists outcomes")
            .iter()
            .map(|outcome| {
                let card_ref = outcome["card_ref"].clone();
                (
                    card_ref["name"].as_str().unwrap_or_default().to_owned(),
                    card_ref,
                )
            })
            .collect()
    }

    /// The shared Rust Workflow-run handle of this server as `token`.
    ///
    /// # Panics
    /// Panics when the client cannot build.
    fn workflows(&self, token: &str) -> Workflows {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: self.base.clone(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(token.to_owned())),
        )
        .expect("client auth builds");
        let transport =
            HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
        Workflows::new(WyrdClient::from_parts(auth, transport, config.grpc))
    }

    /// Wait for server run `run_id` to be terminal through the shared client.
    ///
    /// # Panics
    /// Panics when a poll fails or the run is not terminal within
    /// [`PATIENCE`].
    async fn terminal(&self, run_id: &str) -> WorkflowRun {
        let run_id: WorkflowRunId = run_id.parse().expect("run id parses");
        tokio::time::timeout(PATIENCE, self.workflows(&self.runner).wait(&run_id))
            .await
            .expect("run terminates")
            .expect("run polls")
    }

    /// Shut the server down.
    ///
    /// # Panics
    /// Panics when shutdown fails.
    async fn shutdown(self) {
        self.server
            .shutdown()
            .await
            .expect("test server shuts down");
    }
}

/// The run ID a server `wyrd workflow run` printed on acceptance.
///
/// # Panics
/// Panics when stderr carries no acceptance line.
fn accepted_id(stderr: &str) -> String {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("accepted workflow run "))
        .unwrap_or_else(|| panic!("acceptance is printed: {stderr}"))
        .to_owned()
}

/// Every provider request's path, in arrival order.
fn paths(calls: &[UpstreamCall]) -> Vec<&str> {
    calls.iter().map(|call| call.path.as_str()).collect()
}

/// Write a one-step `engineering` Workflow `name` whose Prompt has the
/// declarative `prompt` spec lines, whose Workflow route is `route`, and
/// whose step carries the extra `step` lines.
///
/// # Panics
/// Panics when a bundle file cannot be written.
fn declared_review(name: &str, prompt: &str, route: &str, step: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("bundle directory");
    let write = |file: &str, body: String| {
        std::fs::write(temp.path().join(file), body).expect("bundle file writes");
    };
    write(
        "prompt.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  space: engineering\n  name: {name}-prompt\n  version: \"1.0.0\"\nspec:\n{prompt}  messages:\n    - \"Answer about {{{{code}}}}\"\n"
        ),
    );
    write(
        "agent.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: {name}-agent\n  version: \"1.0.0\"\nspec:\n  prompt: ./prompt.yaml\n  tool_names: []\n  run_config:\n    max_iterations: 1\n"
        ),
    );
    write(
        "workflow.yaml",
        format!(
            "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: {name}\n  version: \"1.0.0\"\nspec:\n  llm_route:\n{route}\n  inputs:\n    code:\n      type: str\n      value: \"\"\n  steps:\n    - id: answer\n      action:\n        type: agent\n        target: ./agent.yaml\n      inputs:\n        code: input.code\n{step}  outputs:\n    answer: steps.answer.output.text\n"
        ),
    );
    temp
}

/// The actual code-review bundle runs from its file, registers through the
/// compiled `wyrd apply` without executing, and runs again from the
/// registry by exact identity and by UID with the same portable result; a
/// team's registered reviewers are reused by another bundle with a local
/// final reviewer, pinned against a later team release.
///
/// The file run reaches the public gateway with the caller's credential:
/// both independent reviewers are in flight together before the final
/// reviewer, which receives both reviews through its explicit bindings.
///
/// # Panics
/// Panics when a run, receipt, pinned identity, or upstream request differs
/// from the asserted journey.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn workflow_file_apply_registered_local() {
    let journey = Journey::start().await;
    let example = repo("examples/workflows/code-review/workflow.yaml");
    let example = example.to_str().expect("utf-8 example path");
    let input = repo("examples/workflows/code-review/input.json");
    let input = input.to_str().expect("utf-8 input path");

    journey.upstream.hold();
    let file_args = ["--file", example, "--input-file", input];
    let file_run = async {
        let (code, run) = journey.run(&file_args).await;
        assert_eq!(code, Some(0), "{run}");
        run
    };
    let release = async {
        journey.upstream.wait_arrivals(2).await;
        assert_eq!(
            paths(&journey.upstream.calls()),
            ["/v1/chat/completions"; 2],
            "both reviewers run before the final reviewer"
        );
        journey.upstream.release();
    };
    let (file_run, ()) = tokio::join!(file_run, release);
    assert_eq!(file_run["status"], "succeeded", "{file_run}");
    assert_eq!(file_run["workflow"], Value::Null);
    assert_eq!(file_run["outputs"], json!({ "review": "DONE" }));
    let calls = journey.upstream.calls();
    assert_eq!(calls.len(), 3);
    assert!(calls.iter().all(|call| {
        call.authorization.as_deref() == Some(format!("Bearer {PROVIDER_KEY}").as_str())
            && call.body["model"] == json!("gpt-5-5")
    }));
    assert!(
        calls[2].body["messages"]
            .to_string()
            .contains("Security review:\\nDONE\\n\\nCorrectness review:\\nDONE"),
        "{}",
        calls[2].body
    );

    let refs = journey.apply(Path::new(example).parent().expect("bundle directory"));
    assert_eq!(journey.upstream.calls().len(), 3, "apply executes nothing");
    let workflow_ref = &refs["code-review"];
    let uid = workflow_ref["uid"].as_str().expect("registered UID");
    for (selector, after) in [
        (
            vec![
                "--space",
                "engineering",
                "--name",
                "code-review",
                "--version",
                "1.0.0",
            ],
            6,
        ),
        (vec!["--uid", uid], 9),
    ] {
        let (code, run) = journey
            .run(&[selector.as_slice(), &["--input-file", input]].concat())
            .await;
        assert_eq!(code, Some(0), "{run}");
        assert_eq!(run["status"], "succeeded", "{run}");
        assert_eq!(run["outputs"], file_run["outputs"]);
        assert_eq!(run["workflow"]["uid"], json!(uid), "{run}");
        assert_eq!(run["workflow"]["version"], json!("1.0.0"));
        assert_eq!(journey.upstream.calls().len(), after);
    }

    let loading = |relative: &str| repo(&format!("tests/fixtures/workflow-loading/{relative}"));
    let team = journey.apply(&loading("team/security.yaml"));
    journey.apply(&loading("team/correctness.yaml"));
    let mixed = loading("mixed/workflow.yaml");
    let mixed = mixed.to_str().expect("utf-8 fixture path");
    let reviewed = json!({ "review": "final review of diff | registered security review of diff | registered correctness review of diff" });
    let (code, run) = journey
        .run(&["--file", mixed, "--input", r#"{"code":"diff"}"#])
        .await;
    assert_eq!(code, Some(0), "{run}");
    assert_eq!(run["outputs"], reviewed);
    let applied = journey.apply(&loading("mixed"));
    journey.apply(&loading("team-v2/security.yaml"));
    let (code, run) = journey
        .run(&[
            "--space",
            "workflow-loading",
            "--name",
            "code-review",
            "--version",
            "1.0.0",
            "--input",
            r#"{"code":"diff"}"#,
        ])
        .await;
    assert_eq!(code, Some(0), "{run}");
    assert_eq!(run["outputs"], reviewed, "the team release never floats in");
    assert_eq!(run["workflow"]["uid"], applied["code-review"]["uid"]);
    assert!(team["security-reviewer"]["uid"].is_string());
    assert_eq!(
        journey.upstream.calls().len(),
        9,
        "the mock provider stays local"
    );
    journey.shutdown().await;
}

/// A server run prints its ID on acceptance and waits for the terminal
/// snapshot; a detached run returns once accepted, is read with `status`,
/// and is cancelled idempotently with `cancel`; and an interrupted wait
/// reports the ID and exits 130 while the run continues to success,
/// neither cancelled nor resubmitted. A file source is refused for server
/// execution before any request.
///
/// # Panics
/// Panics when an exit code, printed ID, snapshot, or upstream request count
/// differs from the asserted lifecycle.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn workflow_server_detach_status_cancel() {
    let journey = Journey::start().await;
    let example = repo("examples/workflows/code-review");
    journey.apply(&example);
    let input = example.join("input.json");
    let input = input.to_str().expect("utf-8 input path");
    let server_run = [
        "workflow",
        "run",
        "--execution",
        "server",
        "--space",
        "engineering",
        "--name",
        "code-review",
        "--version",
        "1.0.0",
        "--input-file",
        input,
        "--format",
        "json",
    ];

    let mut waited = journey.command(&journey.runner, &server_run);
    let waited = tokio::task::spawn_blocking(move || waited.output())
        .await
        .expect("command task joins")
        .expect("compiled command runs");
    assert_eq!(waited.status.code(), Some(0), "{}", streams(&waited));
    let run = run_json(&waited);
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["outputs"], json!({ "review": "DONE" }));
    assert_eq!(
        accepted_id(&String::from_utf8_lossy(&waited.stderr)),
        run["run_id"].as_str().expect("run id")
    );
    assert_eq!(journey.upstream.calls().len(), 3);

    journey.upstream.hold();
    let detached = journey.cli(&journey.runner, &[&server_run[..], &["--detach"]].concat());
    assert_eq!(detached.status.code(), Some(0), "{}", streams(&detached));
    let accepted = run_json(&detached);
    assert_eq!(accepted["status"], "queued", "{accepted}");
    let run_id = accepted["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(
        accepted_id(&String::from_utf8_lossy(&detached.stderr)),
        run_id
    );
    journey.upstream.wait_arrivals(5).await;
    let status = journey.cli(
        &journey.runner,
        &["workflow", "status", &run_id, "--format", "json"],
    );
    assert_eq!(status.status.code(), Some(0), "{}", streams(&status));
    assert_eq!(run_json(&status)["status"], "running");
    let status = journey.cli(&journey.runner, &["workflow", "status", &run_id, "--steps"]);
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(
        text.contains(&format!("workflow run {run_id} running")),
        "{text}"
    );
    assert!(text.contains("step security: running"), "{text}");
    assert!(text.contains("step final_review: pending"), "{text}");
    let cancel = journey.cli(
        &journey.runner,
        &["workflow", "cancel", &run_id, "--format", "json"],
    );
    assert_eq!(cancel.status.code(), Some(0), "{}", streams(&cancel));
    let cancelled = journey.terminal(&run_id).await;
    assert_eq!(cancelled.status, WorkflowRunStatus::Cancelled);
    let again = journey.cli(
        &journey.runner,
        &["workflow", "cancel", &run_id, "--format", "json"],
    );
    assert_eq!(again.status.code(), Some(0), "{}", streams(&again));
    assert_eq!(
        serde_json::from_slice::<WorkflowRun>(&again.stdout).expect("cancel prints a run"),
        cancelled,
        "cancelling a finished run returns it unchanged"
    );
    let foreign = journey.cli(
        &journey.admin,
        &["workflow", "status", "0190d6a0-0000-7000-8000-000000000000"],
    );
    assert_eq!(problem(&foreign)["status"], 404, "{}", streams(&foreign));
    journey.upstream.release();

    journey.upstream.hold();
    let before = journey.upstream.calls().len();
    let mut interrupted = journey
        .command(&journey.runner, &server_run)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("compiled command spawns");
    let mut stderr = BufReader::new(interrupted.stderr.take().expect("piped stderr"));
    let run_id = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        stderr.read_line(&mut line).expect("stderr reads");
        (accepted_id(&line), stderr)
    });
    let (run_id, mut stderr) = run_id.await.expect("stderr task joins");
    journey.upstream.wait_arrivals(before + 2).await;
    let killed = Command::new("kill")
        .args(["-INT", &interrupted.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    let (status, rest) = tokio::task::spawn_blocking(move || {
        let status = interrupted.wait().expect("interrupted command exits");
        let mut rest = String::new();
        stderr.read_to_string(&mut rest).expect("stderr reads");
        (status, rest)
    })
    .await
    .expect("wait task joins");
    assert_eq!(status.code(), Some(130), "{rest}");
    assert!(
        rest.contains(&format!("workflow run {run_id} continues")),
        "{rest}"
    );
    let running = journey.cli(
        &journey.runner,
        &["workflow", "status", &run_id, "--format", "json"],
    );
    assert_eq!(
        run_json(&running)["status"],
        "running",
        "interruption cancels nothing"
    );
    journey.upstream.release();
    let finished = journey.terminal(&run_id).await;
    assert_eq!(finished.status, WorkflowRunStatus::Succeeded);
    assert_eq!(
        journey.upstream.calls().len(),
        before + 3,
        "the interrupted run is never resubmitted"
    );

    let refused = journey.cli(
        &journey.runner,
        &[
            "workflow",
            "run",
            "--execution",
            "server",
            "--file",
            example.join("workflow.yaml").to_str().expect("utf-8 path"),
        ],
    );
    assert_eq!(problem(&refused)["code"], "WYRD_CLI_400_INVALID_ARGUMENT");
    assert_eq!(journey.upstream.calls().len(), before + 3);
    journey.shutdown().await;
}

/// Registered Workflows run locally through the compiled CLI on every
/// route: the public gateway serves Chat, Responses, Anthropic, and Gemini
/// Prompts at their own provider paths with the deployment's key; a step
/// route overrides the Workflow route; a stored fallback is applied by the
/// gateway; a local `ext_gateway` binding receives its secret and no
/// provider key; and the built-in mock provider stays in process. A local
/// Vertex Prompt is refused before any dispatch while the same registered
/// Workflow succeeds on the server, and a binding of another protocol is
/// refused before any dispatch.
///
/// # Panics
/// Panics when an upstream path, header, model, run outcome, or refusal
/// differs from the asserted matrix.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn workflow_registered_route_protocol_matrix() {
    let journey = Journey::start().await;
    journey.deploy(
        "anthropic",
        "x-api-key",
        "claude-sonnet-5",
        &["chat_completions"],
    );
    journey.deploy(
        "gemini",
        "x-goog-api-key",
        "gemini-2.5-flash",
        &["chat_completions"],
    );
    journey.deploy(
        "openai",
        "authorization",
        "gpt-5-4",
        &["chat_completions", "responses"],
    );
    journey.deploy(
        "vertex",
        "authorization",
        "gemini-2.5-pro",
        &["chat_completions"],
    );
    let gateway = "    kind: wyrd_gateway";
    let external = format!(
        "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}/v1\n    credential_binding: review-gateway",
        journey.upstream.origin()
    );
    configure_binding(
        journey.home.path(),
        &journey.upstream.origin(),
        "openai_chat",
    );
    let registered = |name: &str| {
        vec![
            "--space".to_owned(),
            "engineering".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
            "--version".to_owned(),
            "1.0.0".to_owned(),
            "--input".to_owned(),
            r#"{"code":"x"}"#.to_owned(),
        ]
    };
    let bearer = format!("Bearer {PROVIDER_KEY}");

    let cases = [
        (
            "chat-review",
            "  provider: openai\n  model: gpt-5-5\n",
            gateway.to_owned(),
            "",
            "/v1/chat/completions",
            "gpt-5-5",
        ),
        (
            "responses-review",
            "  provider: openai\n  model: gpt-5-4\n  operation: responses\n",
            gateway.to_owned(),
            "",
            "/v1/responses",
            "gpt-5-4",
        ),
        (
            "anthropic-review",
            "  provider: anthropic\n  model: claude-sonnet-5\n",
            gateway.to_owned(),
            "",
            "/v1/messages",
            "claude-sonnet-5",
        ),
        (
            "gemini-review",
            "  provider: gemini\n  model: gemini-2.5-flash\n",
            gateway.to_owned(),
            "",
            "/v1beta/models/gemini-2.5-flash:generateContent",
            "",
        ),
        (
            "step-review",
            "  provider: openai\n  model: gpt-5-5\n",
            external.clone(),
            "      llm_route:\n        kind: wyrd_gateway\n",
            "/v1/chat/completions",
            "gpt-5-5",
        ),
        (
            "fallback-review",
            "  provider: openai\n  model: gpt-5-5-undeployed\n",
            gateway.to_owned(),
            "      fallback:\n        candidates:\n          - provider: openai\n            model: gpt-5-5-missing\n          - provider: openai\n            model: gpt-5-4\n",
            "/v1/chat/completions",
            "gpt-5-4",
        ),
    ];
    for (name, prompt, route, step, path, model) in cases {
        let bundle = declared_review(name, prompt, &route, step);
        journey.apply(&bundle.path().join("workflow.yaml"));
        let before = journey.upstream.calls().len();
        let args = registered(name);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, run) = journey.run(&args).await;
        assert_eq!(code, Some(0), "{name}: {run}");
        assert_eq!(run["outputs"], json!({ "answer": "DONE" }), "{name}: {run}");
        let calls = journey.upstream.calls();
        assert_eq!(calls.len(), before + 1, "{name}");
        let call = &calls[before];
        assert_eq!(call.path, path, "{name}");
        assert_eq!(call.secret, None, "{name}");
        if !model.is_empty() {
            assert_eq!(call.body["model"], json!(model), "{name}");
        }
        if call.authorization.is_some() {
            assert_eq!(
                call.authorization.as_deref(),
                Some(bearer.as_str()),
                "{name}"
            );
        }
    }

    let bundle = declared_review(
        "external-review",
        "  provider: openai\n  model: gpt-5-5\n",
        &external,
        "",
    );
    journey.apply(&bundle.path().join("workflow.yaml"));
    let before = journey.upstream.calls().len();
    let args = registered("external-review");
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, run) = journey.run(&args).await;
    assert_eq!(code, Some(0), "{run}");
    assert!(!run.to_string().contains(SECRET));
    let calls = journey.upstream.calls();
    assert_eq!(calls.len(), before + 1);
    assert_eq!(calls[before].secret.as_deref(), Some(SECRET));
    assert_eq!(
        calls[before].authorization, None,
        "no provider key leaves the gateway"
    );

    configure_binding(
        journey.home.path(),
        &journey.upstream.origin(),
        "anthropic_messages",
    );
    let mut refused = journey.command(
        &journey.runner,
        &[&["workflow", "run"], args.as_slice()].concat(),
    );
    let refused = tokio::task::spawn_blocking(move || refused.output())
        .await
        .expect("command task joins")
        .expect("compiled command runs");
    assert_eq!(
        problem(&refused)["code"],
        "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED"
    );
    assert_eq!(
        journey.upstream.calls().len(),
        before + 1,
        "refused before dispatch"
    );

    let native = repo("tests/fixtures/workflow-loading/shadowed/local-workflow.yaml");
    journey.apply(&native);
    let (code, run) = journey
        .run(&[
            "--space",
            "workflow-loading",
            "--name",
            "local-review",
            "--version",
            "1.0.0",
            "--input",
            r#"{"code":"x"}"#,
        ])
        .await;
    assert_eq!(code, Some(0), "{run}");
    assert_eq!(
        run["outputs"]["security"],
        json!("local security review of x")
    );
    assert_eq!(
        journey.upstream.calls().len(),
        before + 1,
        "native stays in process"
    );

    let vertex = declared_review(
        "vertex-review",
        "  provider: vertex\n  model: gemini-2.5-pro\n",
        gateway,
        "",
    );
    journey.apply(&vertex.path().join("workflow.yaml"));
    let args = registered("vertex-review");
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, run) = journey.run(&args).await;
    assert_eq!(code, Some(2), "{run}");
    assert_eq!(run["status"], "failed", "{run}");
    assert_eq!(
        journey.upstream.calls().len(),
        before + 1,
        "local Vertex dispatches nothing"
    );
    let (code, run) = journey
        .run(&[&["--execution", "server"], args.as_slice()].concat())
        .await;
    assert_eq!(code, Some(0), "{run}");
    assert_eq!(run["outputs"], json!({ "answer": "DONE" }));
    let calls = journey.upstream.calls();
    assert_eq!(calls.len(), before + 2);
    assert_eq!(
        calls[before + 1].path,
        "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent"
    );
    journey.shutdown().await;
}
