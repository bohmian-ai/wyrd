//! Callers reach a model through the Wyrd Gateway: an OpenAI-compatible
//! client with a Wyrd access token, and a registered Workflow loaded through
//! Cards. The upstream sees only the operator's provider key.
//!
//! No OpenAI Rust SDK is a dependency, so the OpenAI-compatible caller is a
//! plain Chat Completions request with a bearer token, which is the whole of
//! what such a client sends.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use serde_json::{Map, Value, json};
use skald_workflow::WorkflowRunStatus;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::Gateway;
use wyrd_sdk::cards::{CardSelector, RegistrationReceipt};
use wyrd_sdk::cli;
use wyrd_sdk::gateway::{
    GatewayOperation, ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_sdk::operator_connections::SecretBearer;
use wyrd_testing::server::WyrdTestServer;

use crate::support::{self, Deployment, register};

/// The provider key the operator submits; only the upstream may see it.
const PROVIDER_KEY: &str = "sk-native-upstream";

/// A Gateway in front of a local upstream that answers `hi`.
struct Inference {
    /// The deployment.
    deployment: Deployment,
    /// The upstream the `openai` deployment reaches.
    upstream: MockServer,
    /// The registered `ask` Workflow.
    ask: RegistrationReceipt,
}

impl Inference {
    /// Start the upstream and the server, write the `openai-key` credential
    /// with the CLI from the `test` child, deploy `gpt-4o` on it, and
    /// register `ask`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start(test: &str) -> Self {
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
        deployment
            .run_child(
                test,
                &deployment.key("cli_admin", &["admin"]).await,
                &[("PHASE", "setup")],
            )
            .await;
        Gateway::new(deployment.admin())
            .put_deployment(&ProviderDeployment {
                name: ProviderDeploymentName::new("gpt-4o").expect("deployment name"),
                model: ModelRef {
                    provider: ProviderId::new("openai").expect("provider"),
                    model: ModelId::new("gpt-4o").expect("model"),
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
}

/// Write the managed provider credential `name` holding `secret` with the
/// CLI, and return its redacted view as JSON.
///
/// # Panics
/// Panics when the write is refused.
async fn put_managed(name: &str, secret: &str) -> Value {
    let view = cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: ProviderCredentialName::new(name).expect("credential name"),
            provider: ProviderId::new("openai").expect("provider"),
            source: ProviderCredentialWriteSource::ManagedSecret {
                secret: SecretBearer::new(secret.to_owned()),
            },
        },
        None,
    )
    .await
    .expect("the CLI writes the credential");
    serde_json::to_value(view).expect("view serializes")
}

/// The setup a [`Inference::start`] child runs: the CLI writes `openai-key`.
///
/// # Panics
/// Panics when the write is refused.
async fn setup_child() {
    put_managed("openai-key", PROVIDER_KEY).await;
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
    if support::is_child() {
        setup_child().await;
        return;
    }
    let inference =
        Inference::start("gateway_inference::openai_client_calls_the_gateway_with_an_access_token")
            .await;
    let token = inference
        .deployment
        .admin()
        .access_token()
        .await
        .expect("access token mints");

    let reply: Value = reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            inference
                .deployment
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
        .expect("the Gateway answers")
        .error_for_status()
        .expect("the call succeeds")
        .json()
        .await
        .expect("the reply is JSON");

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
        assert!(
            !value.to_str().unwrap_or_default().contains(token.expose()),
            "the access token reached upstream in {name}"
        );
    }
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
    if support::is_child() {
        if std::env::var("PHASE").as_deref() == Ok("setup") {
            setup_child().await;
            return;
        }
        let issued = cli::issue_key("Agent", "ask-agent", "1.0.0", "default", None, None, None)
            .await
            .expect("the CLI issues a key");
        support::report(&json!({
            "card_ref": issued.card_ref,
            "prefixed": issued.key.expose().starts_with(&issued.prefix),
            "view": put_managed("managed-openai-key", "sk-managed").await,
        }));
        return;
    }
    let test = "gateway_inference::cli_issues_a_card_scoped_key_and_writes_a_provider_credential";
    let inference = Inference::start(test).await;

    let outcome = inference
        .deployment
        .run_child(
            test,
            &inference.deployment.key("cli_issuer", &["admin"]).await,
            &[("PHASE", "issue")],
        )
        .await;

    assert_eq!(
        outcome["card_ref"],
        json!({ "kind": "Agent", "name": "ask-agent", "version": "1.0.0", "space": "default" })
    );
    assert_eq!(outcome["prefixed"], true);
    assert_eq!(
        (
            &outcome["view"]["name"],
            &outcome["view"]["provider"],
            &outcome["view"]["state"]
        ),
        (
            &json!("managed-openai-key"),
            &json!("openai"),
            &json!("active")
        )
    );
    assert!(!outcome.to_string().contains("sk-managed"), "{outcome}");
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
    if support::is_child() {
        setup_child().await;
        return;
    }
    let inference = Inference::start(
        "gateway_inference::loaded_workflow_calls_the_gateway_through_its_loading_client",
    )
    .await;
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
