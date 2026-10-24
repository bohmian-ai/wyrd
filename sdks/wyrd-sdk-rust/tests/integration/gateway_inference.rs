//! Callers reach a model through the Wyrd Gateway: an OpenAI-compatible
//! client with a Wyrd access token and a hydrated Agent's Run. The upstream
//! sees only the operator's provider key.
//!
//! No OpenAI Rust SDK is a dependency, so the OpenAI-compatible caller is a
//! plain Chat Completions request with a bearer token, which is the whole of
//! what such a client sends.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::cards::{CardRef, DeclarationReceipt};
use wyrd_ai::cli;
use wyrd_ai::gateway::{
    DeploymentAccess, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    ProviderAdapter, ProviderAuth, ProviderAuthHeader, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_ai::operator_connections::SecretBearer;
use wyrd_ai::{Gateway, WyrdClient};
use wyrd_spec::auth::TokenAudience;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

use wyrd_ai::state::WyrdState;

use crate::support::{Deployment, ancestry, declare, declared, pull, runtime_account};

/// The provider key the operator submits; only the upstream may see it.
const PROVIDER_KEY: &str = "sk-native-upstream";

/// A Gateway in front of a local upstream that answers `hi`.
struct Inference {
    /// The deployment.
    deployment: Deployment,
    /// The upstream every deployment reaches.
    upstream: MockServer,
    /// The registered `ask-agent` Agent and its Prompt.
    ask: DeclarationReceipt,
}

impl Inference {
    /// Start the upstream and the server, write the `openai-key` credential
    /// with the CLI as the administrator, deploy the `openai`
    /// deployment on it, capture call metadata, and register `ask-agent`.
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
        put_openai(&deployment, "openai", PROVIDER_KEY, DeploymentAccess::Open).await;
        Gateway::connect_with(&deployment.admin())
            .await
            .expect("handle connects")
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
        let ask = declare(
            &deployment.cards().await,
            "cards/gateway_inference/ask-agent.yaml",
        )
        .await;
        Self {
            deployment,
            upstream,
            ask,
        }
    }

    /// Every retained `vala.gateway.calls` row's attribution, sorted, after
    /// publishing captured rows.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn calls(&self) -> Vec<CallAttribution> {
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        let mut calls: Vec<CallAttribution> =
            wyrd_ai::Bifrost::connect_with(&self.deployment.admin(), BifrostOptions::default())
                .await
                .expect("Bifrost connects")
                .sql_as(
                    "SELECT sub_principal_id, act_principal_id, run_id, card_uid \
                     FROM vala.gateway.calls",
                    &[],
                )
                .await
                .expect("calls read back");
        calls.sort();
        calls
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
        self.ask_openai_with(client, &[]).await
    }

    /// [`Self::ask_openai`], sending `traceparent` with [`CALLER_STATE`] as
    /// the caller's W3C trace context, the way an instrumented HTTP client
    /// does.
    ///
    /// # Panics
    /// Panics when no token mints or the Gateway does not answer with JSON.
    async fn ask_openai_traced(&self, client: &WyrdClient, traceparent: &str) -> (u16, Value) {
        self.ask_openai_with(
            client,
            &[("traceparent", traceparent), ("tracestate", CALLER_STATE)],
        )
        .await
    }

    /// [`Self::ask_openai`], sending each of `headers` with the request.
    ///
    /// # Panics
    /// Panics when no token mints or the Gateway does not answer with JSON.
    async fn ask_openai_with(&self, client: &WyrdClient, headers: &[(&str, &str)]) -> (u16, Value) {
        let body = json!({
            "model": "openai/gpt-4o",
            "max_completion_tokens": 16,
            "messages": [{ "role": "user", "content": "hi" }],
        });
        self.post(client, "/v1/chat/completions", &body, headers)
            .await
    }

    /// Ask the `alias` Agent of `state` `hi` through `deployment`, the way an
    /// application calls its model: render the Agent's Prompt, address
    /// `<deployment>/<Prompt model>` on the gateway ingress the Prompt's
    /// provider speaks, and correlate the call to a fresh Run of the Agent
    /// with the `wyrd-run-id` and `wyrd-card-uid` headers. The
    /// `wyrd-prompt-provider` header marks it an Agent call, so a deployment
    /// whose adapter the Prompt's provider is incompatible with refuses it
    /// before dispatch instead of translating it. Returns the HTTP status and
    /// JSON body.
    ///
    /// # Panics
    /// Panics when the alias is not a hydrated Agent, its Prompt does not
    /// render, no token mints, or the Gateway does not answer with JSON.
    async fn ask_agent(&self, state: &WyrdState, alias: &str, deployment: &str) -> (u16, Value) {
        let run = state.run_for_card(alias).expect("agent view resolves");
        let prompt = &state.agent(alias).expect("agent").prompt().metadata.prompt;
        let mut request = serde_json::to_value(
            prompt
                .render(&[("question", "hi")])
                .expect("the Prompt renders"),
        )
        .expect("the request serializes");
        let mut body = request["body"].take();
        let model = format!("{deployment}/{}", prompt.model);
        let path = match request["provider"].as_str() {
            Some("anthropic_message") => "/v1/messages".to_owned(),
            Some("jev_system_one") => "/v1/systemone".to_owned(),
            Some("gemini_generate_content") => format!(
                "/v1beta/models/{deployment}/{}:generateContent",
                prompt.model.replace('/', "%2F")
            ),
            _ => "/v1/chat/completions".to_owned(),
        };
        if !path.starts_with("/v1beta") {
            body["model"] = json!(model);
        }
        let provider = prompt.provider();
        let correlation = [
            ("wyrd-run-id", run.run_id().as_str()),
            ("wyrd-card-uid", run.subject_uid().as_str()),
            ("wyrd-prompt-provider", provider.as_str()),
        ];
        self.post(
            state.client().expect("the state has a client"),
            &path,
            &body,
            &correlation,
        )
        .await
    }

    /// POST `body` to the Gateway at `path` with `client`'s access token and
    /// each of `headers`, and return the HTTP status and JSON body.
    ///
    /// # Panics
    /// Panics when no token mints or the Gateway does not answer with JSON.
    async fn post(
        &self,
        client: &WyrdClient,
        path: &str,
        body: &Value,
        headers: &[(&str, &str)],
    ) -> (u16, Value) {
        let token = client.access_token().await.expect("access token mints");
        let mut request = reqwest::Client::new()
            .post(format!(
                "{}{path}",
                self.deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .bearer_auth(token.expose());
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request
            .json(body)
            .send()
            .await
            .expect("the Gateway answers");
        let status = response.status().as_u16();
        (status, response.json().await.expect("the reply is JSON"))
    }
}

/// The trace identity of a caller's W3C `traceparent`.
const CALLER_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// The caller's span identity within [`CALLER_TRACE`].
const CALLER_SPAN: &str = "00f067aa0ba902b7";
/// The vendor trace state the caller sends with its `traceparent`.
const CALLER_STATE: &str = "wyrd=journey";

/// Scope of the gateway's captured model-call attempt spans.
const GATEWAY_SCOPE: &str = "wyrd.gateway";

/// Write the managed `provider` credential `name` holding `secret` with the
/// CLI as `client`, and return its redacted view as JSON.
///
/// # Panics
/// Panics when the write is refused.
async fn put_managed(client: WyrdClient, provider: &str, name: &str, secret: &str) -> Value {
    let view = cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: ProviderCredentialName::new(name).expect("credential name"),
            provider: ProviderId::new(provider).expect("provider"),
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

/// Write the `{name}-key` credential holding `secret` as the administrator
/// and deploy `name` on the OpenAI adapter with it, serving Chat Completions
/// for every model under `access`.
///
/// # Panics
/// Panics when the credential or deployment is refused.
pub(crate) async fn put_openai(
    deployment: &Deployment,
    name: &str,
    secret: &str,
    access: DeploymentAccess,
) {
    let credential = format!("{name}-key");
    put_managed(deployment.admin(), "openai", &credential, secret).await;
    Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .put_deployment(&ProviderDeployment {
            name: ProviderDeploymentName::new(name).expect("deployment name"),
            adapter: ProviderAdapter::OpenAi,
            auth: ProviderAuth::Bearer {
                credential: ProviderCredentialName::new(&credential).expect("credential"),
            },
            models: None,
            capabilities: Some(BTreeSet::from([GatewayOperation::ChatCompletions])),
            access,
        })
        .await
        .expect("deployment puts");
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

/// A caller's `traceparent` puts the gateway's one captured attempt in the
/// caller's trace under the exported `gateway.call` span, which descends
/// through the request span from the caller's span; the attempt's own
/// tracing span is never exported a second time.
///
/// # Panics
/// Panics when the call, flush, or read fails, the attempt is not captured
/// exactly once, or the chain to the caller is broken.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_span_joins_the_callers_trace() {
    let inference = Inference::start().await;

    let (status, reply) = inference
        .ask_openai_traced(
            &inference.deployment.admin(),
            &format!("00-{CALLER_TRACE}-{CALLER_SPAN}-01"),
        )
        .await;

    assert_eq!(status, 200, "{reply}");
    let timeline = inference.deployment.timeline(CALLER_TRACE).await;
    let [attempt] = timeline
        .iter()
        .filter(|span| span.trace_id.to_hex() == CALLER_TRACE && span.scope.name == GATEWAY_SCOPE)
        .collect::<Vec<_>>()[..]
    else {
        panic!("exactly one attempt row joins the caller: {timeline:#?}");
    };
    assert_eq!(
        attempt.trace_state, CALLER_STATE,
        "the caller's state propagates"
    );
    assert!(
        timeline.iter().all(|span| span.name != "gateway.attempt"),
        "{timeline:#?}"
    );
    let chain = ancestry(&timeline, attempt);
    let names: Vec<&str> = chain.iter().map(|span| span.name.as_str()).collect();
    assert_eq!(names.get(1), Some(&"gateway.call"), "{names:?}");
    assert_eq!(names.last(), Some(&"request"), "{names:?} in {timeline:#?}");
    assert_eq!(
        chain
            .last()
            .and_then(|span| span.parent_span_id)
            .map(|span| span.to_hex())
            .as_deref(),
        Some(CALLER_SPAN),
        "{names:?}"
    );
    inference.deployment.shutdown().await;
}

/// A malformed `traceparent` is ignored: the call succeeds and its span
/// starts a trace of its own.
///
/// # Panics
/// Panics when the call, flush, or read fails or the span joined a trace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn malformed_traceparent_is_ignored_by_the_gateway() {
    let inference = Inference::start().await;

    let (status, reply) = inference
        .ask_openai_traced(
            &inference.deployment.admin(),
            &format!("00-{CALLER_TRACE}-{CALLER_SPAN}-zz"),
        )
        .await;

    assert_eq!(status, 200, "{reply}");
    assert!(
        inference.deployment.timeline(CALLER_TRACE).await.is_empty(),
        "nothing joined the caller's trace"
    );
    let calls =
        wyrd_ai::Bifrost::connect_with(&inference.deployment.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as::<CallId>("SELECT call_id FROM vala.gateway.calls", &[])
            .await
            .expect("calls read back");
    let [call] = &calls[..] else {
        panic!("exactly one call is recorded: {calls:?}");
    };
    let own_trace = inference
        .deployment
        .timeline(&call.call_id.replace('-', ""))
        .await;
    let [attempt] = &own_trace[..] else {
        panic!("the call's own trace holds its one attempt: {own_trace:#?}");
    };
    assert_eq!(
        (attempt.scope.name.as_str(), attempt.parent_span_id),
        (GATEWAY_SCOPE, None)
    );
    inference.deployment.shutdown().await;
}

/// One `vala.gateway.calls` row's identity.
#[derive(Debug, serde::Deserialize)]
struct CallId {
    /// The call's UUID, whose bytes are its trace id when no caller's trace
    /// was continued.
    call_id: String,
}

/// One `vala.gateway.calls` row's subject/actor pair and attribution.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
struct CallAttribution {
    /// The principal the call was for.
    sub_principal_id: String,
    /// The principal that made the call.
    act_principal_id: String,
    /// The application Run the call is attributed to.
    run_id: Option<String>,
    /// The Card UID the call is attributed to.
    card_uid: Option<String>,
}

/// Every direct call records its caller as both subject and actor plus whichever of `wyrd-run-id` and
/// `wyrd-card-uid` it carried: neither, the Card alone, the Run alone, or
/// both, each absent one as null. The caller is the owning Service's runtime
/// account run at that Service, whose scope admits its `ask-agent` component.
///
/// # Panics
/// Panics when a call, flush, or read fails or a row's attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_call_records_whichever_attribution_it_carries() {
    let inference = Inference::start().await;
    let service = declare(
        &inference.deployment.cards().await,
        "cards/gateway_inference/ask-service.yaml",
    )
    .await
    .root;
    let (id, key) = runtime_account(&inference.deployment.admin(), &service).await;
    let caller = inference
        .deployment
        .client(&key)
        .for_workload(service.uid.expect("a registered Card has a UID"));
    let card = declared(&inference.ask, "ask-agent")
        .uid
        .expect("a registered Card has a UID")
        .to_string();

    for headers in [
        vec![],
        vec![("wyrd-card-uid", card.as_str())],
        vec![("wyrd-run-id", "run-alone")],
        vec![
            ("wyrd-run-id", "run-with-card"),
            ("wyrd-card-uid", card.as_str()),
        ],
    ] {
        let (status, reply) = inference.ask_openai_with(&caller, &headers).await;
        assert_eq!(status, 200, "{headers:?}: {reply}");
    }

    let calls = inference.calls().await;
    let call = |run_id: Option<&str>, card_uid: Option<&str>| CallAttribution {
        sub_principal_id: id.to_string(),
        act_principal_id: id.to_string(),
        run_id: run_id.map(str::to_owned),
        card_uid: card_uid.map(str::to_owned),
    };
    let mut expected = vec![
        call(None, None),
        call(None, Some(&card)),
        call(Some("run-alone"), None),
        call(Some("run-with-card"), Some(&card)),
    ];
    expected.sort();
    assert_eq!(calls, expected);
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

/// The CLI writes a managed provider credential whose view never carries the
/// secret.
///
/// # Panics
/// Panics when the CLI command fails, or the view differs or leaks the secret.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cli_writes_a_provider_credential() {
    let inference = Inference::start().await;
    let admin = inference.deployment.admin();

    let view = put_managed(admin, "openai", "managed-openai-key", "sk-managed").await;

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

/// A hydrated Service's Agent, called as the Service's own key on its
/// declared `openai` deployment with the Agent's Run correlation, is
/// answered through the Gateway; the upstream sees the provider key and no
/// Run or Card correlation header. A `viewer` without gateway invoke is
/// refused with the gateway's own code before upstream IO.
///
/// # Panics
/// Panics when a step fails or an answer, refusal, or upstream request
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn run_invokes_its_agent_through_the_gateway() {
    let inference = Inference::start().await;
    let cards = inference.deployment.cards().await;
    let service = declare(&cards, "cards/gateway_inference/ask-service.yaml").await;
    let bundle = pull(&cards, &service.root).await;
    let state = |client: WyrdClient| {
        WyrdState::from_path_with(bundle.path().join("bundle"), client)
            .expect("bundle loads offline")
    };

    let service_client = inference.deployment.service_client(&service.root).await;
    let (status, reply) = inference
        .ask_agent(&state(service_client), "agent", "openai")
        .await;

    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["choices"][0]["message"]["content"], "hi");
    let requests = inference
        .upstream
        .received_requests()
        .await
        .expect("request recording is on");
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].headers.get("wyrd-run-id").is_none()
            && requests[0].headers.get("wyrd-card-uid").is_none(),
        "correlation never reaches the provider"
    );
    assert_eq!(
        inference.upstream_authorizations().await,
        vec![Some(format!("Bearer {PROVIDER_KEY}"))]
    );

    let viewer = inference
        .deployment
        .client(&inference.deployment.key("agent_viewer", &["viewer"]).await);
    let (status, refused) = inference.ask_agent(&state(viewer), "agent", "openai").await;
    assert_eq!(
        (status, &refused["error"]["code"]),
        (403, &json!("WYRD_PERMISSION_403_DENIED_RBAC")),
        "{refused}"
    );
    assert_eq!(
        inference
            .upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .len(),
        1,
        "refusals precede upstream IO"
    );
    inference.deployment.shutdown().await;
}

/// One Agent called concurrently on its declared `openai` deployment and
/// on `openai-backup` reaches each deployment's own credential. The `granted`
/// deployment `openai-granted` is authorized on its own, so a caller holding
/// only an unscoped gateway grant is refused before upstream IO.
///
/// # Panics
/// Panics when a step fails or an answer, refusal, or upstream credential
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_routes_and_pre_run_overrides() {
    /// Upstream credential that proves a call reached the backup deployment.
    const BACKUP_KEY: &str = "sk-backup-upstream";
    let inference = Inference::start().await;
    put_openai(
        &inference.deployment,
        "openai-backup",
        BACKUP_KEY,
        DeploymentAccess::Open,
    )
    .await;
    put_openai(
        &inference.deployment,
        "openai-granted",
        BACKUP_KEY,
        DeploymentAccess::Granted,
    )
    .await;
    let cards = inference.deployment.cards().await;
    let service = declare(&cards, "cards/gateway_inference/ask-service.yaml").await;
    let bundle = pull(&cards, &service.root).await;
    let client = inference.deployment.service_client(&service.root).await;
    let state = WyrdState::from_path_with(bundle.path().join("bundle"), client)
        .expect("bundle loads offline");

    let (declared, backup) = tokio::join!(
        inference.ask_agent(&state, "agent", "openai"),
        inference.ask_agent(&state, "agent", "openai-backup")
    );

    for (status, reply) in [declared, backup] {
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["choices"][0]["message"]["content"], "hi");
    }
    let mut credentials = inference.upstream_authorizations().await;
    credentials.sort();
    assert_eq!(
        credentials,
        vec![
            Some(format!("Bearer {BACKUP_KEY}")),
            Some(format!("Bearer {PROVIDER_KEY}"))
        ]
    );

    let (status, refused) = inference.ask_agent(&state, "agent", "openai-granted").await;
    assert_eq!(
        (status, &refused["error"]["code"]),
        (403, &json!("WYRD_PERMISSION_403_DENIED_RBAC")),
        "{refused}"
    );
    assert_eq!(
        inference.upstream_authorizations().await.len(),
        2,
        "the refusal precedes upstream IO"
    );
    inference.deployment.shutdown().await;
}

/// Agents whose Anthropic, Google, and Jev Prompts route to deployments named
/// `claude-prod`, `vertex` (a Gemini deployment), and `judge-pool` reach
/// their native ingress with the full address: each upstream receives the
/// complete Prompt model, including the Google model's slash, and the Jev
/// Agent answers in one call with its keyed answers. An OpenAI Agent's
/// Prompt addressed to the Anthropic or Gemini deployment it is not
/// compatible with is refused before any upstream IO.
///
/// # Panics
/// Panics when a step fails or an answer, refusal, or upstream target
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_routes_reach_named_native_ingress() {
    let inference = Inference::start().await;
    for (name, provider, adapter, header, operation) in [
        (
            "claude-prod",
            "anthropic",
            ProviderAdapter::Anthropic,
            Some("x-api-key"),
            GatewayOperation::ChatCompletions,
        ),
        (
            "vertex",
            "gemini",
            ProviderAdapter::Gemini,
            Some("x-goog-api-key"),
            GatewayOperation::ChatCompletions,
        ),
        (
            "judge-pool",
            "jev",
            ProviderAdapter::Jev,
            None,
            GatewayOperation::SystemOne,
        ),
    ] {
        put_named(
            &inference.deployment,
            name,
            provider,
            adapter,
            header,
            operation,
        )
        .await;
    }
    for (upstream_path, reply) in [
        (
            "/v1/messages",
            json!({
                "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-4-5",
                "content": [{ "type": "text", "text": "hi" }], "stop_reason": "end_turn",
                "usage": { "input_tokens": 3, "output_tokens": 1 },
            }),
        ),
        (
            "/v1beta/models/tuned%2Fgemini-2.5-flash:generateContent",
            json!({
                "candidates": [{ "content": { "role": "model", "parts": [{ "text": "hi" }] }, "finishReason": "STOP" }],
                "usageMetadata": { "promptTokenCount": 3, "candidatesTokenCount": 1, "totalTokenCount": 4 },
            }),
        ),
        (
            "/v1/systemone",
            json!({
                "model": "jev-latest",
                "answers": { "helpful": { "type": "noul", "noul": 0.9 } },
                "usage": { "input_tokens": 3, "output_tokens": 1 },
            }),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(upstream_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&inference.upstream)
            .await;
    }
    let cards = inference.deployment.cards().await;
    let service = declare(&cards, "cards/gateway_inference/routed-service.yaml").await;
    let bundle = pull(&cards, &service.root).await;
    let client = inference.deployment.service_client(&service.root).await;
    let state = WyrdState::from_path_with(bundle.path().join("bundle"), client)
        .expect("bundle loads offline");

    let (status, claude) = inference.ask_agent(&state, "claude", "claude-prod").await;
    assert_eq!((status, &claude["content"][0]["text"]), (200, &json!("hi")));
    let (status, gemini) = inference.ask_agent(&state, "gemini", "vertex").await;
    assert_eq!(
        (
            status,
            &gemini["candidates"][0]["content"]["parts"][0]["text"]
        ),
        (200, &json!("hi"))
    );
    let (status, jev) = inference.ask_agent(&state, "jev", "judge-pool").await;
    assert_eq!(
        (status, &jev["answers"]),
        (200, &json!({ "helpful": { "type": "noul", "noul": 0.9 } }))
    );

    let asks = declare(&cards, "cards/gateway_inference/ask-service.yaml").await;
    let asks = WyrdState::from_path_with(
        pull(&cards, &asks.root).await.path().join("bundle"),
        inference.deployment.service_client(&asks.root).await,
    )
    .expect("bundle loads offline");
    for deployment in ["claude-prod", "vertex"] {
        let (status, refused) = inference.ask_agent(&asks, "agent", deployment).await;
        assert_eq!(
            (status, &refused["error"]["code"]),
            (400, &json!("WYRD_GATEWAY_400_INVALID_REQUEST")),
            "{deployment}: {refused}"
        );
    }

    let targets = inference
        .upstream
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .map(|request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            (request.url.path().to_owned(), body["model"].clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            ("/v1/messages".to_owned(), json!("claude-sonnet-4-5")),
            (
                "/v1beta/models/tuned%2Fgemini-2.5-flash:generateContent".to_owned(),
                Value::Null
            ),
            ("/v1/systemone".to_owned(), json!("jev-latest")),
        ]
    );
    inference.deployment.shutdown().await;
}

/// An external OpenAI-compatible gateway onboarded as the deployment
/// `support-gateway` receives the addressed model unchanged, including its
/// own slashes, authenticated by the uploaded `external` credential and never
/// by the caller's Wyrd token; once that credential is revoked the call is
/// refused before reaching the gateway.
///
/// # Panics
/// Panics when a setup step fails, or a status, upstream body, or upstream
/// header differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn external_deployment_preserves_model_and_credentials() {
    /// Bearer credential the external deployment must present upstream.
    const EXTERNAL_KEY: &str = "sk-external-gateway";
    /// Upstream model id the external deployment must forward unchanged.
    const MODEL: &str = "meta-llama/Llama-3.3-70B";
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gateway/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": MODEL,
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4},
        })))
        .mount(&upstream)
        .await;
    let deployment = Deployment::start_with(
        WyrdTestServer::builder().with_gateway_provider_root_for_test(
            upstream.uri().parse().expect("upstream URL parses"),
        ),
    )
    .await;
    let credential = ProviderCredentialName::new("external-key").expect("credential name");
    cli::put_provider_credential(
        &ProviderCredentialWrite {
            name: credential.clone(),
            provider: ProviderId::new("external").expect("provider"),
            source: ProviderCredentialWriteSource::ManagedSecret {
                secret: SecretBearer::new(EXTERNAL_KEY.to_owned()),
            },
        },
        Some(deployment.admin()),
    )
    .await
    .expect("the CLI writes the credential");
    Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .put_deployment(&ProviderDeployment {
            name: ProviderDeploymentName::new("support-gateway").expect("deployment name"),
            adapter: ProviderAdapter::OpenAiCompatible {
                base_url: format!("{}/gateway/v1", upstream.uri())
                    .parse()
                    .expect("base URL parses"),
            },
            auth: ProviderAuth::Bearer {
                credential: credential.clone(),
            },
            models: None,
            capabilities: None,
            access: DeploymentAccess::Open,
        })
        .await
        .expect("deployment puts");
    let caller = deployment.client(&deployment.key("external_caller", &["workload"]).await);
    let token = caller.access_token().await.expect("access token mints");
    let call = || {
        reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .bearer_auth(token.expose())
            .json(&json!({
                "model": format!("support-gateway/{MODEL}"),
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
    };

    let answered = call().await.expect("the gateway answers");
    assert_eq!(answered.status().as_u16(), 200);
    let received = upstream
        .received_requests()
        .await
        .expect("request recording is on");
    assert_eq!(received.len(), 1);
    let body: Value = received[0].body_json().expect("upstream body is JSON");
    assert_eq!(
        body["model"], MODEL,
        "the model reaches the gateway unchanged"
    );
    assert_eq!(
        received[0]
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some(format!("Bearer {EXTERNAL_KEY}").as_str()),
        "the gateway sees only the uploaded credential"
    );
    assert!(
        received[0]
            .headers
            .values()
            .all(|value| !value.to_str().unwrap_or_default().contains(token.expose())),
        "no Wyrd token is forwarded"
    );

    cli::revoke_provider_credential(&credential, Some(deployment.admin()))
        .await
        .expect("the credential revokes");
    let refused = call().await.expect("the gateway answers");
    let status = refused.status().as_u16();
    let refusal: Value = refused.json().await.expect("the refusal is JSON");
    assert_eq!(
        (status, refusal["error"]["code"].as_str()),
        (502, Some("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE")),
        "a revoked credential serves nothing"
    );
    assert_eq!(
        upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .len(),
        1,
        "the refused call never reaches the gateway"
    );
    deployment.shutdown().await;
}

/// Write a `provider` credential for `name` as the administrator, then
/// deploy `name` on `adapter` with it, sent in `header` or as a bearer
/// token, serving `operation` for every model.
///
/// # Panics
/// Panics when the credential or deployment is refused.
async fn put_named(
    deployment: &Deployment,
    name: &str,
    provider: &str,
    adapter: ProviderAdapter,
    header: Option<&str>,
    operation: GatewayOperation,
) {
    let credential = format!("{name}-key");
    put_managed(deployment.admin(), provider, &credential, PROVIDER_KEY).await;
    let credential = ProviderCredentialName::new(&credential).expect("credential");
    Gateway::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .put_deployment(&ProviderDeployment {
            name: ProviderDeploymentName::new(name).expect("deployment name"),
            adapter,
            auth: match header {
                Some(header) => ProviderAuth::ApiKeyHeader {
                    header: ProviderAuthHeader::new(header).expect("header"),
                    credential,
                },
                None => ProviderAuth::Bearer { credential },
            },
            models: None,
            capabilities: Some(BTreeSet::from([operation])),
            access: DeploymentAccess::Open,
        })
        .await
        .expect("deployment puts");
}

/// Native Anthropic, Gemini, and Jev clients reach deployments whose names
/// are not provider names: each upstream receives the complete model, the
/// Gemini call keeps its method on the model resource, and each reply relays.
///
/// # Panics
/// Panics when a call is refused or an upstream sees a rewritten target.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn native_ingress_preserves_named_deployments() {
    let inference = Inference::start().await;
    for (name, provider, adapter, header, operation) in [
        (
            "claude-prod",
            "anthropic",
            ProviderAdapter::Anthropic,
            Some("x-api-key"),
            GatewayOperation::ChatCompletions,
        ),
        (
            "gemini-team",
            "gemini",
            ProviderAdapter::Gemini,
            Some("x-goog-api-key"),
            GatewayOperation::ChatCompletions,
        ),
        (
            "judge-pool",
            "jev",
            ProviderAdapter::Jev,
            None,
            GatewayOperation::SystemOne,
        ),
    ] {
        put_named(
            &inference.deployment,
            name,
            provider,
            adapter,
            header,
            operation,
        )
        .await;
    }
    for (upstream_path, reply) in [
        (
            "/v1/messages",
            json!({
                "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-4-5",
                "content": [{ "type": "text", "text": "hi" }], "stop_reason": "end_turn",
                "usage": { "input_tokens": 3, "output_tokens": 1 },
            }),
        ),
        (
            "/v1beta/models/gemini-2.5-flash:generateContent",
            json!({
                "candidates": [{ "content": { "role": "model", "parts": [{ "text": "hi" }] }, "finishReason": "STOP" }],
                "usageMetadata": { "promptTokenCount": 3, "candidatesTokenCount": 1, "totalTokenCount": 4 },
            }),
        ),
        (
            "/v1/systemone",
            json!({
                "model": "jev-latest",
                "answers": { "helpful": { "type": "noul", "noul": 0.9 } },
                "usage": { "input_tokens": 3, "output_tokens": 1 },
            }),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(upstream_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .mount(&inference.upstream)
            .await;
    }
    let base = inference
        .deployment
        .server()
        .base_url()
        .expect("bound server has a URL");
    let token = inference
        .deployment
        .admin()
        .access_token()
        .await
        .expect("access token mints");
    let calls = [
        (
            "/v1/messages",
            json!({
                "model": "claude-prod/claude-sonnet-4-5", "max_tokens": 16,
                "messages": [{ "role": "user", "content": "hi" }],
            }),
        ),
        (
            "/v1beta/models/gemini-team/gemini-2.5-flash:generateContent",
            json!({
                "contents": [{ "role": "user", "parts": [{ "text": "hi" }] }],
            }),
        ),
        (
            "/v1/systemone",
            json!({
                "model": "judge-pool/jev-latest", "state": "hi",
                "questions": { "helpful": { "type": "noul", "instructions": "Is it helpful?" } },
            }),
        ),
    ];

    for (ingress, body) in calls {
        let response = reqwest::Client::new()
            .post(format!("{base}{ingress}"))
            .bearer_auth(token.expose())
            .json(&body)
            .send()
            .await
            .expect("the Gateway answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("the reply reads");
        assert_eq!(status, 200, "{ingress} answers: {text}");
    }

    let received = inference
        .upstream
        .received_requests()
        .await
        .expect("request recording is on");
    let targets = received
        .iter()
        .map(|request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            (request.url.path().to_owned(), body["model"].clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            ("/v1/messages".to_owned(), json!("claude-sonnet-4-5")),
            (
                "/v1beta/models/gemini-2.5-flash:generateContent".to_owned(),
                Value::Null
            ),
            ("/v1/systemone".to_owned(), json!("jev-latest")),
        ]
    );
    inference.deployment.shutdown().await;
}

/// A Gateway with `ask-service` and the unrelated `observed-model`
/// registered, the Service family's runtime account, and its key.
///
/// # Panics
/// Panics when a setup step fails.
async fn stable_account() -> (Inference, CardRef, CardRef, String, String) {
    let inference = Inference::start().await;
    let cards = inference.deployment.cards().await;
    let service = declare(&cards, "cards/gateway_inference/ask-service.yaml").await;
    let outside = declare(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let (id, key) = runtime_account(&inference.deployment.admin(), &service.root).await;
    (inference, service.root, outside.root, id.to_string(), key)
}

/// The exact UID string of a registered `card`.
///
/// # Panics
/// Panics when `card` carries no UID.
fn uid(card: &CardRef) -> String {
    card.uid
        .as_ref()
        .expect("a registered Card has a UID")
        .to_string()
}

/// A family account's key alone invokes a model but attributes to no Card;
/// run as its exact Service root it attributes to the root's Agent and to no
/// Card outside the root's graph. Each answered call records the account as
/// subject and actor.
///
/// # Panics
/// Panics when a call's status or code, or a retained row, differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn stable_account_gateway_calls_keep_execution_identity() {
    let (inference, root, outside, id, key) = stable_account().await;
    let agent = uid(&declared(&inference.ask, "ask-agent"));
    let key_only = inference.deployment.client(&key);
    let rooted = key_only.for_workload(root.uid.clone().expect("a registered root has a UID"));
    let outside = uid(&outside);

    for (client, card, expected) in [
        (&key_only, None, None),
        (
            &key_only,
            Some(agent.as_str()),
            Some("WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED"),
        ),
        (&rooted, Some(agent.as_str()), None),
        (
            &rooted,
            Some(outside.as_str()),
            Some("WYRD_VALA_403_BIFROST_CARD_SCOPE"),
        ),
    ] {
        let headers: Vec<_> = card
            .map(|card| ("wyrd-card-uid", card))
            .into_iter()
            .collect();
        let (status, reply) = inference.ask_openai_with(client, &headers).await;
        match expected {
            None => assert_eq!(status, 200, "{card:?}: {reply}"),
            Some(code) => assert_eq!(reply["error"]["code"], code, "{card:?}: {reply}"),
        }
    }

    let call = |card_uid: Option<&str>| CallAttribution {
        sub_principal_id: id.clone(),
        act_principal_id: id.clone(),
        run_id: None,
        card_uid: card_uid.map(str::to_owned),
    };
    let mut expected = vec![call(None), call(Some(&agent))];
    expected.sort();
    assert_eq!(inference.calls().await, expected);
    inference.deployment.shutdown().await;
}

/// A User acting through the Service's rooted account reaches only the
/// intersection of the User's Card access with the root's graph: the root's
/// Agent is answered and recorded with the User as subject and the account as
/// actor, and a Card outside the graph is refused.
///
/// # Panics
/// Panics when the delegation fails or a call's outcome or retained row
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn user_through_service_gateway_delegation_keeps_the_intersection() {
    let (inference, root, outside, id, key) = stable_account().await;
    let agent = uid(&declared(&inference.ask, "ask-agent"));
    let Bootstrap::User { id: user, jwt, .. } = inference
        .deployment
        .server()
        .bootstrap_user("gateway-user", &["workload"])
        .await
        .expect("user signs in")
    else {
        panic!("user bootstrap returned a machine principal");
    };
    let delegated = inference
        .deployment
        .client(&key)
        .for_workload(root.uid.clone().expect("a registered root has a UID"))
        .on_behalf_of(secrecy::SecretString::from(jwt), TokenAudience::Wyrd)
        .await
        .expect("the account acts for the User");

    let (status, reply) = inference
        .ask_openai_with(&delegated, &[("wyrd-card-uid", agent.as_str())])
        .await;
    assert_eq!(status, 200, "{reply}");
    let outside = uid(&outside);
    let (_, refused) = inference
        .ask_openai_with(&delegated, &[("wyrd-card-uid", outside.as_str())])
        .await;
    assert_eq!(
        refused["error"]["code"], "WYRD_VALA_403_BIFROST_CARD_SCOPE",
        "{refused}"
    );

    assert_eq!(
        inference.calls().await,
        [CallAttribution {
            sub_principal_id: user.to_string(),
            act_principal_id: id,
            run_id: None,
            card_uid: Some(agent),
        }]
    );
    inference.deployment.shutdown().await;
}
