//! A developer runs Wyrd locally with only the administrator key `wyrd
//! setup` printed: they register and hydrate an assistant, invoke a model
//! through the Gateway, observe and verify a Run, export its spans through
//! `start_telemetry`, and query the evidence back. No key is
//! issued and nothing is flushed on the server's behalf.
//!
//! [`work`] is that whole workflow for one client, so the signed-in story
//! proves the same steps for a saved user login.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::cards::Cards;
use wyrd_sdk::cli;
use wyrd_sdk::gateway::{
    GatewayOperation, ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_sdk::observe::Run;
use wyrd_sdk::operator_connections::SecretBearer;
use wyrd_sdk::otel::{Telemetry, start_telemetry};
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::{Bifrost, Gateway, VerifierKind, WyrdClient};
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

use crate::support::{Deployment, hydrate, register};

/// The drift features the Run observes on its Model.
#[derive(Serialize)]
struct Features {
    /// A numeric series.
    latency: f64,
}

/// The answer the Run verifies on its Agent.
#[derive(Serialize)]
struct Answer {
    /// The answer text `answer-is-yes` expects.
    answer: &'static str,
}

/// One evidence row: which Card it is attributed to.
#[derive(Debug, Deserialize)]
struct Attributed {
    /// The stamped Card UID.
    card_uid: Option<String>,
}

/// The local upstream every Gateway call and its server.
pub struct Local {
    /// The deployment.
    pub deployment: Deployment,
    /// The upstream that answers `hi`; held so it outlives the deployment.
    _upstream: MockServer,
}

impl Local {
    /// Start a local upstream that answers `hi` and a server configured by
    /// `builder` that verifies in real time and routes the Gateway there.
    ///
    /// # Panics
    /// Panics when the server cannot start.
    pub async fn start(builder: WyrdTestServerBuilder) -> Self {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-local",
                "object": "chat.completion",
                "created": 1,
                "model": "gpt-4o",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "hi" },
                    "finish_reason": "stop",
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
            })))
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            builder
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    upstream.uri().parse().expect("upstream URL parses"),
                ),
        )
        .await;
        Self {
            deployment,
            _upstream: upstream,
        }
    }
}

/// What [`work`] leaves running for a caller to keep using.
pub struct Worked {
    /// The state the workflow ran, already shut down.
    pub state: WyrdState,
    /// The telemetry pipeline the workflow installed.
    pub telemetry: Telemetry,
    /// The Agent's registry UID.
    pub agent_uid: String,
}

/// Run the whole local workflow as `client` and assert each step.
///
/// # Panics
/// Panics when any step is refused or its evidence does not read back.
pub async fn work(deployment: &Deployment, client: &WyrdClient) -> Worked {
    configure_gateway(client).await;
    let cards = Cards::with_client(client.clone());
    register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
    register(&cards, "cards/verify_in_real_time/latency-model.yaml").await;
    let assistant = register(&cards, "cards/verify_in_real_time/assistant.yaml").await;
    let bundle = hydrate(&cards, &assistant.root).await;
    let state = WyrdState::from_path_with_client(bundle.path().join("bundle"), client.clone())
        .expect("the bundle loads");
    state.start_bifrost().await.expect("Bifrost starts");

    assert_eq!(invoke(deployment, client).await, "hi");

    let run = state.run();
    run.for_card("model")
        .expect("model view")
        .observe()
        .drift(&Features { latency: 12.5 }, None)
        .expect("drift emits");
    let judgment = run
        .for_card("agent")
        .expect("agent view")
        .observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");
    assert!(judgment.passed());
    assert_eq!(judgment.kind, VerifierKind::EvalAssertion);

    let agent_ref = state.card_ref("agent").expect("agent resolves");
    let agent_uid = agent_ref
        .uid
        .as_ref()
        .expect("hydrated Card carries its UID")
        .to_string();
    let run_id = run.run_id().to_string();
    let telemetry = start_telemetry(&state).expect("telemetry installs");
    start_telemetry(&state).expect("a second start is idempotent");
    export(&run.for_card("agent").expect("agent view")).await;
    telemetry.shutdown().expect("spans export");
    state.shutdown().await.expect("emits drain");

    let bifrost = Bifrost::connect(client).await.expect("Bifrost connects");
    let model_uid = state
        .card_ref("model")
        .expect("model resolves")
        .uid
        .as_ref()
        .expect("hydrated Card carries its UID")
        .to_string();
    for (table, uid) in [
        ("vala.drift.observations", &model_uid),
        ("vala.traces.spans", &agent_uid),
    ] {
        let rows: Vec<Attributed> = bifrost
            .sql_as(
                &format!("SELECT card_uid FROM {table} WHERE run_id = $1"),
                &[QueryParam::String(run_id.clone())],
            )
            .await
            .unwrap_or_else(|error| panic!("{table} reads back: {error}"));
        assert_eq!(rows.len(), 1, "{table}: {rows:?}");
        assert_eq!(rows[0].card_uid.as_ref(), Some(uid), "{table}");
    }
    Worked {
        state,
        telemetry,
        agent_uid,
    }
}

/// Start and end one `answer` span inside `run`'s scope, which attributes it
/// to that Run and Card.
pub async fn export(run: &Run) {
    run.scope(async { tracing::info_span!("answer").in_scope(|| ()) })
        .await;
}

/// Write the `openai-key` credential and deploy `gpt-4o` on it as `client`.
///
/// # Panics
/// Panics when the credential or deployment is refused.
async fn configure_gateway(client: &WyrdClient) {
    cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: ProviderCredentialName::new("openai-key").expect("credential name"),
            provider: ProviderId::new("openai").expect("provider"),
            source: ProviderCredentialWriteSource::ManagedSecret {
                secret: SecretBearer::new("sk-local-upstream".to_owned()),
            },
        },
        Some(client.clone()),
    )
    .await
    .expect("the CLI writes the credential");
    Gateway::with_client(client.clone())
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
}

/// Send one Chat Completions turn through the Gateway the way an OpenAI
/// client does, with a bearer `client` returns for this request, and return
/// the answer text.
///
/// # Panics
/// Panics when no token mints or the Gateway does not answer.
pub async fn invoke(deployment: &Deployment, client: &WyrdClient) -> String {
    let token = client.access_token().await.expect("access token mints");
    let reply: Value = reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            deployment
                .server()
                .base_url()
                .expect("bound server has a URL")
        ))
        .bearer_auth(token.expose())
        .json(&json!({
            "model": "openai/gpt-4o",
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .send()
        .await
        .expect("the Gateway answers")
        .error_for_status()
        .expect("the Gateway accepts the call")
        .json()
        .await
        .expect("the reply is JSON");
    reply["choices"][0]["message"]["content"]
        .as_str()
        .expect("the reply carries text")
        .to_owned()
}

/// The setup administrator key alone completes the local workflow.
///
/// # Panics
/// Panics when any step of [`work`] fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn admin_key_completes_the_local_workflow() {
    let local = Box::pin(Local::start(
        WyrdTestServer::builder().without_process_telemetry_for_test(),
    ))
    .await;
    let key = local
        .deployment
        .server()
        .tenant_admin_key()
        .await
        .expect("the setup administrator key");
    let client = local
        .deployment
        .client(secrecy::ExposeSecret::expose_secret(&key));

    work(&local.deployment, &client).await;

    local.deployment.shutdown().await;
}

/// `start_telemetry` refuses a process whose application already installed a
/// global subscriber, and leaves that subscriber in place.
///
/// # Panics
/// Panics when the bundle does not load or the refusal is not the catalog one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn start_telemetry_refuses_the_applications_own_subscriber() {
    let local = Box::pin(Local::start(
        WyrdTestServer::builder().without_process_telemetry_for_test(),
    ))
    .await;
    let key = local
        .deployment
        .server()
        .tenant_admin_key()
        .await
        .expect("the setup administrator key");
    let client = local
        .deployment
        .client(secrecy::ExposeSecret::expose_secret(&key));
    let cards = Cards::with_client(client.clone());
    register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
    register(&cards, "cards/verify_in_real_time/latency-model.yaml").await;
    let assistant = register(&cards, "cards/verify_in_real_time/assistant.yaml").await;
    let bundle = hydrate(&cards, &assistant.root).await;
    let state = WyrdState::from_path_with_client(bundle.path().join("bundle"), client)
        .expect("the bundle loads");
    tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default())
        .expect("the application installs its own subscriber");

    let refusal = start_telemetry(&state).expect_err("the application's subscriber stays");

    assert_eq!(refusal.code(), "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS");
    local.deployment.shutdown().await;
}
