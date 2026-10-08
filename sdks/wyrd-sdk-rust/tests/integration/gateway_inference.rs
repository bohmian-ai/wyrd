//! Callers reach a model through the Wyrd Gateway: an OpenAI-compatible
//! client with a Wyrd access token, a registered Workflow loaded through
//! Cards, and the code-review example run from its file. The upstream sees
//! only the operator's provider key.
//!
//! No OpenAI Rust SDK is a dependency, so the OpenAI-compatible caller is a
//! plain Chat Completions request with a bearer token, which is the whole of
//! what such a client sends.

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::cards::{CardKind, CardSelector, RegistrationReceipt};
use wyrd_sdk::cli;
use wyrd_sdk::gateway::{
    GatewayOperation, ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_sdk::operator_connections::SecretBearer;
use wyrd_sdk::{Gateway, Workflow, WorkflowRunStatus, WyrdClient};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, register};

/// The provider key the operator submits; only the upstream may see it.
const PROVIDER_KEY: &str = "sk-native-upstream";

/// The code-review example directory, whose Workflow calls `openai/gpt-5-5`
/// through the Wyrd gateway.
fn example() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/workflows/code-review"
    ))
}

/// The example's checked-in run input.
///
/// # Panics
/// Panics when `input.json` cannot be read or is not a JSON object.
fn example_input() -> Map<String, Value> {
    serde_json::from_str(
        &std::fs::read_to_string(example().join("input.json")).expect("example input reads"),
    )
    .expect("example input is an object")
}

/// A Gateway in front of a local upstream that answers `hi`.
struct Inference {
    /// The deployment.
    deployment: Deployment,
    /// The upstream every deployment reaches.
    upstream: MockServer,
    /// The registered `ask` Workflow.
    ask: RegistrationReceipt,
}

impl Inference {
    /// Start the upstream and the server, write the `openai-key` credential
    /// with the CLI as the administrator, deploy `gpt-4o` and `gpt-5-5` on
    /// it, and register `ask`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-1",
                "object": "chat.completion",
                "created": 1,
                "model": "gpt-4o",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "hi" },
                    "finish_reason": "stop",
                }],
                "usage": { "prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15 },
            })))
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder().with_gateway_provider_root_for_test(
                upstream.uri().parse().expect("upstream URL parses"),
            ),
        )
        .await;
        put_managed(deployment.admin(), "openai-key", PROVIDER_KEY).await;
        let gateway = Gateway::with_client(deployment.admin());
        for model in ["gpt-4o", "gpt-5-5"] {
            gateway
                .put_deployment(&ProviderDeployment {
                    name: ProviderDeploymentName::new(model).expect("deployment name"),
                    model: ModelRef {
                        provider: ProviderId::new("openai").expect("provider"),
                        model: ModelId::new(model).expect("model"),
                    },
                    adapter: ProviderAdapter::OpenAi,
                    auth: ProviderAuth::Bearer {
                        credential: ProviderCredentialName::new("openai-key").expect("credential"),
                    },
                    capabilities: BTreeSet::from([GatewayOperation::ChatCompletions]),
                    routing_weight: NonZeroU32::MIN,
                })
                .await
                .expect("deployment puts");
        }
        let ask = register(&deployment.cards(), "cards/gateway_inference/ask.yaml").await;
        Self {
            deployment,
            upstream,
            ask,
        }
    }

    /// The `authorization` header of every request the upstream received.
    ///
    /// # Panics
    /// Panics when request recording is off.
    async fn upstream_authorizations(&self) -> Vec<Option<String>> {
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
                    .map(str::to_owned)
            })
            .collect()
    }

    /// Send one Chat Completions turn to `openai/gpt-4o` through the
    /// Gateway, the way an OpenAI client does, with `client`'s access token,
    /// and return the HTTP status and JSON body.
    ///
    /// # Panics
    /// Panics when no token mints or the Gateway does not answer with JSON.
    async fn ask_openai(&self, client: &WyrdClient) -> (u16, Value) {
        let token = client.access_token().await.expect("access token mints");
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                self.deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .bearer_auth(token.expose())
            .json(&json!({
                "model": "openai/gpt-4o",
                "max_completion_tokens": 16,
                "messages": [{ "role": "user", "content": "hi" }],
            }))
            .send()
            .await
            .expect("the Gateway answers");
        let status = response.status().as_u16();
        (status, response.json().await.expect("the reply is JSON"))
    }
}

/// Write the managed provider credential `name` holding `secret` with the
/// CLI as `client`, and return its redacted view as JSON.
///
/// # Panics
/// Panics when the write is refused.
async fn put_managed(client: WyrdClient, name: &str, secret: &str) -> Value {
    let view = cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: ProviderCredentialName::new(name).expect("credential name"),
            provider: ProviderId::new("openai").expect("provider"),
            source: ProviderCredentialWriteSource::ManagedSecret {
                secret: SecretBearer::new(secret.to_owned()),
            },
        },
        Some(client),
    )
    .await
    .expect("the CLI writes the credential");
    serde_json::to_value(view).expect("view serializes")
}

/// An OpenAI-compatible Chat Completions call with the caller's Wyrd access
/// token is answered by the upstream, which sees the provider key and no Wyrd
/// credential or header.
///
/// # Panics
/// Panics when a step fails or the reply or upstream request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn openai_client_calls_the_gateway_with_an_access_token() {
    let inference = Inference::start().await;
    let admin = inference.deployment.admin();
    let token = admin.access_token().await.expect("access token mints");

    let (status, reply) = inference.ask_openai(&admin).await;

    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["choices"][0]["message"]["content"], "hi");
    assert_eq!(reply["usage"]["total_tokens"], 15);
    let upstream = inference
        .upstream
        .received_requests()
        .await
        .expect("request recording is on");
    assert_eq!(upstream.len(), 1);
    assert_eq!(
        upstream[0]
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some(format!("Bearer {PROVIDER_KEY}").as_str())
    );
    for (name, value) in &upstream[0].headers {
        assert!(
            !name.as_str().starts_with("x-wyrd"),
            "{name} reached upstream"
        );
        assert_ne!(
            value.to_str().ok(),
            Some(token.expose()),
            "the access token reached upstream in {name}"
        );
    }
    inference.deployment.shutdown().await;
}

/// A caller holding only `gateway:read` cannot invoke a model.
///
/// # Panics
/// Panics when the call succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_gateway_invoke_is_refused() {
    let inference = Inference::start().await;
    let reader = inference.deployment.client(
        &inference
            .deployment
            .scoped_key("gateway_reader", &["gateway:read"])
            .await,
    );

    let (_, refused) = inference.ask_openai(&reader).await;

    assert_eq!(
        refused["error"]["code"], "WYRD_PERMISSION_403_DENIED_RBAC",
        "{refused}"
    );
    inference.deployment.shutdown().await;
}

/// The CLI issues a key scoped to the `ask-agent` Card and writes a managed
/// provider credential whose view never carries the secret.
///
/// # Panics
/// Panics when a CLI command fails, the key is not bound to the Agent, or the
/// view differs or leaks the secret.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cli_issues_a_card_scoped_key_and_writes_a_provider_credential() {
    let inference = Inference::start().await;
    let admin = inference.deployment.admin();

    let issued = cli::issue_key(
        "Agent",
        "ask-agent",
        "1.0.0",
        "default",
        None,
        None,
        Some(admin.clone()),
    )
    .await
    .expect("the CLI issues a key");
    let view = put_managed(admin, "managed-openai-key", "sk-managed").await;

    assert_eq!(
        (
            issued.card_ref.kind,
            issued.card_ref.name.as_str(),
            issued.card_ref.version.to_string(),
            issued.card_ref.space.as_ref().map(ToString::to_string)
        ),
        (
            CardKind::Agent,
            "ask-agent",
            "1.0.0".to_owned(),
            Some("default".to_owned())
        )
    );
    assert!(issued.key.expose().starts_with(&issued.prefix));
    assert_eq!(
        (&view["name"], &view["provider"], &view["state"]),
        (
            &json!("managed-openai-key"),
            &json!("openai"),
            &json!("active")
        )
    );
    assert!(!view.to_string().contains("sk-managed"), "{view}");
    inference.deployment.shutdown().await;
}

/// A registered Workflow loaded through Cards reaches the model through the
/// Gateway with its loading client, and the upstream sees the provider key.
///
/// # Panics
/// Panics when the load or run fails, the outputs differ, or the upstream
/// request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn loaded_workflow_calls_the_gateway_through_its_loading_client() {
    let inference = Inference::start().await;
    let workflow = inference
        .deployment
        .cards()
        .workflow()
        .load(&CardSelector::exact(inference.ask.root.clone()))
        .await
        .expect("the registered Workflow loads");

    let run = workflow
        .run(Map::from_iter([("question".to_owned(), Value::from("hi"))]))
        .await
        .expect("the run starts");

    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(json!(run.outputs), json!({ "answer": "hi" }));
    assert_eq!(
        inference.upstream_authorizations().await,
        [Some(format!("Bearer {PROVIDER_KEY}"))]
    );
    inference.deployment.shutdown().await;
}

/// The code-review example runs from its file through the Wyrd gateway as
/// the client it is loaded with: each of its three steps reaches the
/// upstream with the provider key.
///
/// # Panics
/// Panics when the load or run fails, the outputs differ, or the upstream
/// calls differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn example_workflow_runs_through_the_wyrd_gateway() {
    let inference = Inference::start().await;
    let example = Workflow::from_path_with_client(
        example().join("workflow.yaml"),
        inference.deployment.admin(),
    )
    .await
    .expect("the example loads");

    let run = example.run(example_input()).await.expect("the run starts");

    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(json!(run.outputs), json!({ "review": "hi" }));
    assert_eq!(
        inference.upstream_authorizations().await,
        vec![Some(format!("Bearer {PROVIDER_KEY}")); 3]
    );
    inference.deployment.shutdown().await;
}

/// Applying the example registers it without calling any model.
///
/// # Panics
/// Panics when the apply fails or the upstream received a call.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn applying_a_workflow_calls_no_model() {
    let inference = Inference::start().await;

    let applied = cli::apply(&example(), Some(inference.deployment.admin()))
        .await
        .expect("the example applies");

    assert_eq!(applied.root.kind, CardKind::Workflow);
    assert_eq!(inference.upstream_authorizations().await, []);
    inference.deployment.shutdown().await;
}

/// The applied example, loaded back through Cards, runs through the Wyrd
/// gateway.
///
/// # Panics
/// Panics when the apply, load, or run fails, the outputs differ, or the
/// upstream calls differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn registered_example_runs_through_the_gateway() {
    let inference = Inference::start().await;
    let applied = cli::apply(&example(), Some(inference.deployment.admin()))
        .await
        .expect("the example applies");
    let registered = inference
        .deployment
        .cards()
        .workflow()
        .load(&CardSelector::exact(applied.root))
        .await
        .expect("the registered example loads");

    let run = registered
        .run(example_input())
        .await
        .expect("the run starts");

    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(json!(run.outputs), json!({ "review": "hi" }));
    assert_eq!(
        inference.upstream_authorizations().await,
        vec![Some(format!("Bearer {PROVIDER_KEY}")); 3]
    );
    inference.deployment.shutdown().await;
}
