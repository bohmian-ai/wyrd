//! A tenant reaches TypeSafe Jev through the Wyrd Gateway: native
//! `POST /v1/systemone` callers reach one Jev deployment with typed questions
//! and get typed answers back, OpenAI-shaped callers reach the same deployment
//! on `/v1/chat/completions` by sending the typed Jev request as their last
//! user message, and a Verifier uses that deployment as its LLM judge through
//! a native Jev Prompt. The local upstream stands in for `api.typesafe.ai` and
//! sees only the operator's Jev key.
//!
//! No Jev or OpenAI SDK is a dependency: each caller is a plain JSON POST with
//! a bearer token, which is the whole of what such a client sends.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::bifrost::QueryParam;
use wyrd_ai::cli;
use wyrd_ai::gateway::{
    DeploymentAccess, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    ProviderAdapter, ProviderAuth, ProviderCredentialName, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentName, ProviderId,
};
use wyrd_ai::operator_connections::SecretBearer;
use wyrd_ai::state::WyrdState;
use wyrd_ai::{Bifrost, Gateway, VerificationVerdict, VerifierCounts, VerifierKind, WyrdError};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, hydrate, register};

/// The Jev key the operator submits; only the upstream may see it.
const JEV_KEY: &str = "jev-operator-key";

/// The questions every call in this story asks.
fn questions() -> Value {
    json!({
        "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
        "tone": {"type": "choice", "instructions": "What is the tone of the reply?",
                 "criteria": {"polite": "courteous", "rude": "curt"}},
    })
}

/// Jev's answer: `helpful` with probability `noul`, tone `tone`.
fn answer(noul: f64, tone: &str) -> Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "helpful": {"type": "noul", "noul": noul},
            "tone": {"type": "choice", "choice": tone, "confidence": 0.8,
                     "probabilities": {"polite": 0.8, "rude": 0.2}},
        },
        "usage": {"input_tokens": 378, "output_tokens": 65},
    })
}

/// A Chat Completions call to the Jev model whose last user message is the
/// typed Jev request about `state`, after an earlier system turn.
fn chat(state: &str) -> Value {
    json!({"model": "jev/jev-latest", "messages": [
        {"role": "system", "content": "Judge the reply."},
        {"role": "user", "content": json!({"state": state, "questions": questions()}).to_string()},
    ]})
}

/// One customer question and the agent's reply, the input `reply-quality`
/// judges.
#[derive(Serialize)]
struct Reply {
    /// The customer's question.
    question: &'static str,
    /// The reply text.
    answer: &'static str,
}

/// The `state` the native Jev Prompt renders from a [`Reply`].
fn rendered(reply: &Reply) -> String {
    format!("Question: {}\nAnswer: {}", reply.question, reply.answer)
}

/// The typed questions the native Jev Prompt asks about every reply.
fn judge_questions() -> Value {
    json!({
        "helpful": {"type": "noul", "instructions": "Does the reply help the customer?",
                    "criteria": {"true": "it resolves or advances the request",
                                 "false": "it ignores or deflects the request"}},
        "tone": {"type": "choice", "instructions": "What is the tone of the reply?",
                 "criteria": {"polite": "courteous and respectful",
                              "rude": "curt or dismissive"}},
    })
}

/// A judgment's verification result beside the deployment's one Jev
/// `system_one` call: each journey owns its deployment, so the judge's call
/// is the only one. Correlating the call to the judgment's Run is owned by
/// the verify-attribution change, not by the Jev provider.
#[derive(Debug, PartialEq, Deserialize)]
struct JudgedCall {
    /// The result's Verifier implementation.
    implementation: String,
    /// The result's verdict.
    verdict: String,
    /// The Gateway operation the judge called.
    operation: String,
    /// The Gateway call's outcome.
    outcome: String,
    /// How many upstream attempts the Gateway call made.
    attempt_count: i64,
}

/// A Gateway with one Jev deployment in front of a local Jev upstream.
///
/// The upstream answers a helpful, polite verdict unless the state names a
/// scripted case: `Go away` is judged unhelpful and rude, `refused-form` is
/// refused with 400, `rate-limited` is throttled with 429, and `bad-key`
/// answers 401. Every refusal body echoes the operator key after `key cred`.
struct Jev {
    /// The deployment.
    deployment: Deployment,
    /// The local Jev upstream.
    upstream: MockServer,
}

impl Jev {
    /// Start the upstream and a verification-capable server, write the
    /// `jev-key` credential with the CLI as the administrator, deploy
    /// `jev/jev-latest` serving `system_one` and the `chat_completions`
    /// pass-through, and capture call metadata so every Gateway call lands
    /// in Bifrost.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        for (needle, status) in [
            ("refused-form", 400),
            ("rate-limited", 429),
            ("bad-key", 401),
        ] {
            Mock::given(method("POST"))
                .and(path("/v1/systemone"))
                .and(body_string_contains(needle))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_json(json!({"detail": format!("key cred {JEV_KEY}")})),
                )
                .with_priority(1)
                .mount(&upstream)
                .await;
        }
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_string_contains("Go away"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer(0.12, "rude")))
            .with_priority(1)
            .mount(&upstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer(0.93, "polite")))
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    upstream.uri().parse().expect("upstream URL parses"),
                ),
        )
        .await;
        cli::put_provider_credential(
            &ProviderCredentialWrite {
                name: ProviderCredentialName::new("jev-key").expect("credential name"),
                provider: ProviderId::new("jev").expect("provider"),
                source: ProviderCredentialWriteSource::ManagedSecret {
                    secret: SecretBearer::new(JEV_KEY.to_owned()),
                },
            },
            Some(deployment.admin()),
        )
        .await
        .expect("the CLI writes the credential");
        Gateway::with_client(deployment.admin())
            .put_deployment(&ProviderDeployment {
                name: ProviderDeploymentName::new("jev").expect("deployment name"),
                adapter: ProviderAdapter::Jev,
                auth: ProviderAuth::Bearer {
                    credential: ProviderCredentialName::new("jev-key").expect("credential"),
                },
                models: None,
                capabilities: Some(BTreeSet::from([
                    GatewayOperation::SystemOne,
                    GatewayOperation::ChatCompletions,
                ])),
                access: DeploymentAccess::Open,
            })
            .await
            .expect("deployment puts");
        Gateway::with_client(deployment.admin())
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
        Self {
            deployment,
            upstream,
        }
    }

    /// POST `body` to the Gateway route `route` with an administrator access
    /// token and return the HTTP status and JSON body.
    ///
    /// # Panics
    /// Panics when no token mints or the Gateway does not answer with JSON.
    async fn post(&self, route: &str, body: &Value) -> (u16, Value) {
        let token = self
            .deployment
            .admin()
            .access_token()
            .await
            .expect("access token mints");
        let response = reqwest::Client::new()
            .post(format!(
                "{}{route}",
                self.deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .bearer_auth(token.expose())
            .json(body)
            .send()
            .await
            .expect("the Gateway answers");
        let status = response.status().as_u16();
        (status, response.json().await.expect("the reply is JSON"))
    }

    /// Every request body the upstream received at `/v1/systemone`, with
    /// its `authorization` header.
    ///
    /// # Panics
    /// Panics when request recording is off or a body is not JSON.
    async fn received(&self) -> Vec<(Option<String>, Value)> {
        self.upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .iter()
            .map(|request| {
                assert_eq!(request.url.path(), "/v1/systemone");
                (
                    request
                        .headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned),
                    serde_json::from_slice(&request.body).expect("upstream body is JSON"),
                )
            })
            .collect()
    }

    /// Publish every captured row, then read the verification result
    /// `execution_id` wrote beside each `system_one` Gateway call this
    /// deployment made.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn judged_calls(&self, execution_id: &str) -> Vec<JudgedCall> {
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        Bifrost::connect(&self.deployment.admin())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT r.implementation, r.verdict, c.operation, c.outcome, \
                        CAST(c.attempt_count AS BIGINT) AS attempt_count \
                   FROM vala.verification.results r \
                  CROSS JOIN vala.gateway.calls c \
                  WHERE r.result_id = $1 AND c.operation = 'system_one'",
                &[QueryParam::String(execution_id.to_owned())],
            )
            .await
            .expect("the result and the Jev calls read")
    }

    /// Register and hydrate the `reviewer` Service, and load it acting as
    /// its own Card-scoped key.
    ///
    /// # Panics
    /// Panics when registration, hydration, key issue, or loading fails.
    async fn reviewer(&self) -> (WyrdState, tempfile::TempDir) {
        let cards = self.deployment.cards();
        let reviewer = register(&cards, "cards/gateway_jev/reviewer.yaml").await;
        let bundle = hydrate(&cards, &reviewer.root).await;
        let key = self.deployment.service_key(&reviewer.root).await;
        let state = WyrdState::from_path_with_client(
            bundle.path().join("bundle"),
            self.deployment.client(&key),
        )
        .expect("bundle loads offline");
        (state, bundle)
    }
}

/// A native `systemone` call reaches Jev with the operator key and the
/// deployment's model, and Jev's answers return intact.
///
/// # Panics
/// Panics when a step fails or the reply or upstream request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn native_systemone_answers_relay_intact() {
    let jev = Jev::start().await;

    let (status, reply) = jev
        .post(
            "/v1/systemone",
            &json!({"model": "jev/jev-latest", "state": "Thanks!", "questions": questions()}),
        )
        .await;

    assert_eq!((status, &reply), (200, &answer(0.93, "polite")));
    let received = jev.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0.as_deref(), Some("Bearer jev-operator-key"));
    assert_eq!(
        received[0].1,
        json!({"model": "jev-latest", "state": "Thanks!", "questions": questions()})
    );
    jev.deployment.shutdown().await;
}

/// A native body with an unknown question type, a non-text state, or an
/// unknown member is refused as an invalid request before Jev is called.
///
/// # Panics
/// Panics when a body is not refused with the stable code or Jev is reached.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn invalid_jev_questions_are_refused_before_jev() {
    let jev = Jev::start().await;

    for body in [
        json!({"model": "jev/jev-latest", "state": "s",
               "questions": {"q": {"type": "essay", "instructions": "x"}}}),
        json!({"model": "jev/jev-latest", "state": {"answer": "s"}, "questions": questions()}),
        json!({"model": "jev/jev-latest", "state": "s", "questions": questions(), "temperature": 0}),
    ] {
        let (status, reply) = jev.post("/v1/systemone", &body).await;
        assert_eq!(
            (status, reply["error"]["code"].as_str()),
            (400, Some("WYRD_GATEWAY_400_INVALID_REQUEST")),
            "{body}: {reply}"
        );
    }
    assert!(jev.received().await.is_empty(), "Jev was never called");
    jev.deployment.shutdown().await;
}

/// A Chat Completions call to the Jev model passes the typed Jev request in
/// its last user message through to Jev with the operator key and the
/// deployment's model, and returns a chat completion whose content is Jev's
/// typed answer as JSON with Jev's usage mapped; ordinary chat text is
/// refused as an invalid request before Jev is called.
///
/// # Panics
/// Panics when a step fails or the reply or upstream request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn chat_completions_pass_typed_jev_requests_through() {
    let jev = Jev::start().await;
    let state = "He said \"thanks\".\nAnything else?";

    let (status, reply) = jev.post("/v1/chat/completions", &chat(state)).await;

    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["object"], "chat.completion");
    let content = reply["choices"][0]["message"]["content"]
        .as_str()
        .expect("the answer is text");
    assert_eq!(
        serde_json::from_str::<Value>(content).expect("the content is the answer's JSON"),
        answer(0.93, "polite")
    );
    assert_eq!(
        (
            &reply["usage"]["prompt_tokens"],
            &reply["usage"]["completion_tokens"],
            &reply["usage"]["total_tokens"]
        ),
        (&json!(378), &json!(65), &json!(443))
    );
    let received = jev.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0.as_deref(), Some("Bearer jev-operator-key"));
    assert_eq!(
        received[0].1,
        json!({"model": "jev-latest", "state": state, "questions": questions()})
    );

    let (status, reply) = jev
        .post(
            "/v1/chat/completions",
            &json!({"model": "jev/jev-latest",
                    "messages": [{"role": "user", "content": "Is this reply helpful?"}]}),
        )
        .await;
    assert_eq!(
        (status, reply["error"]["code"].as_str()),
        (400, Some("WYRD_GATEWAY_400_INVALID_REQUEST")),
        "{reply}"
    );
    assert_eq!(
        jev.received().await.len(),
        1,
        "ordinary chat never reaches Jev"
    );
    jev.deployment.shutdown().await;
}

/// Jev refusals relay on both routes exactly like every other provider's:
/// the upstream status returns with Jev's body natively and an `OpenAI` error
/// envelope on Chat Completions, the operator key removed and no Wyrd code
/// invented. A validation refusal (400) and a refused key (401) are never
/// retried and do not cool the deployment down. Throttling (429) is proven
/// last on the Chat Completions route because it cools the one deployment
/// down; the 5xx statuses share its retry class and are pinned by the gateway
/// adapter's unit tests.
///
/// # Panics
/// Panics when a refusal is remapped, leaks the key, or a non-retryable
/// refusal is retried.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_refusals_relay_like_other_providers() {
    let jev = Jev::start().await;
    let native =
        |state: &str| json!({"model": "jev/jev-latest", "state": state, "questions": questions()});

    for (state, status) in [("refused-form", 400_u16), ("bad-key", 401)] {
        let before = jev.received().await.len();
        let (answered, reply) = jev.post("/v1/systemone", &native(state)).await;
        assert_eq!(answered, status, "{reply}");
        assert_eq!(
            reply["detail"], "key cred ",
            "Jev's body relays without the key"
        );
        let (answered, reply) = jev.post("/v1/chat/completions", &chat(state)).await;
        assert_eq!(answered, status, "{reply}");
        assert_eq!(reply["error"]["code"], Value::Null, "no Wyrd code: {reply}");
        assert!(!reply.to_string().contains(JEV_KEY), "{reply}");
        assert_eq!(
            jev.received().await.len(),
            before + 2,
            "{status}: a non-retryable refusal is never retried"
        );
    }

    let (answered, reply) = jev
        .post("/v1/chat/completions", &chat("rate-limited"))
        .await;
    assert_eq!(answered, 429, "{reply}");
    assert_eq!(reply["error"]["code"], Value::Null, "no Wyrd code: {reply}");
    assert!(!reply.to_string().contains(JEV_KEY), "{reply}");
    jev.deployment.shutdown().await;
}

/// A helpful, polite reply passes the `reply-quality` Verifier whose LLM
/// judge is the Jev deployment, asked through its native Jev Prompt.
///
/// # Panics
/// Panics when the verify fails, the judgment differs, or Jev did not
/// receive the reply as its state.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judge_passes_a_helpful_polite_reply() {
    let jev = Jev::start().await;
    let (state, _bundle) = jev.reviewer().await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify(
            "reply-quality",
            &Reply {
                question: "Where is my refund?",
                answer: "Your refund is on its way.",
            },
        )
        .await
        .expect("the reply is judged");

    assert_eq!(
        (judgment.verdict, judgment.kind),
        (VerificationVerdict::Passed, VerifierKind::EvalLlmJudge)
    );
    let received = jev.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].1["model"], "jev-latest");
    assert_eq!(
        received[0].1["state"],
        "Question: Where is my refund?\nAnswer: Your refund is on its way."
    );
    assert_eq!(
        received[0].1["questions"]["tone"]["criteria"]["polite"],
        "courteous and respectful"
    );
    jev.deployment.shutdown().await;
}

/// An unhelpful, rude reply fails the same Verifier: Jev's `noul` falls
/// below 0.5 and its `choice` is not `polite`.
///
/// # Panics
/// Panics when the verify fails or the judgment differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judge_fails_an_unhelpful_rude_reply() {
    let jev = Jev::start().await;
    let (state, _bundle) = jev.reviewer().await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify(
            "reply-quality",
            &Reply {
                question: "Where is my refund?",
                answer: "Go away.",
            },
        )
        .await
        .expect("the reply is judged");

    // The judge task itself passes; both threshold assertions fail.
    assert_eq!(
        (judgment.verdict, judgment.counts),
        (
            VerificationVerdict::Failed,
            Some(VerifierCounts::Eval {
                passed_tasks: 1,
                failed_tasks: 2,
                total_tasks: 3,
                pass_rate_percent: 33
            })
        )
    );
    jev.deployment.shutdown().await;
}

/// A question and reply holding quotes and newlines are judged: the native
/// Jev Prompt renders both into the typed `state` as escaped JSON text, so
/// Jev receives exactly the typed request the Prompt declares, with the
/// operator key. The judgment's verification result and the judge's one
/// `system_one` Gateway call are both recorded.
///
/// # Panics
/// Panics when the reply is not judged, Jev receives a different request,
/// or either row is missing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judge_judges_a_reply_with_quotes_and_newlines() {
    let jev = Jev::start().await;
    let (state, _bundle) = jev.reviewer().await;
    let reply = Reply {
        question: "Is my \"refund\" done?\nOrder 7",
        answer: "He said \"your refund is on its way\".\nAnything else?",
    };

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("reply-quality", &reply)
        .await
        .expect("a reply with quotes and newlines is judged");
    state.shutdown().await.expect("the reviewer stops");

    assert_eq!(judgment.verdict, VerificationVerdict::Passed);
    let received = jev.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0.as_deref(), Some("Bearer jev-operator-key"));
    assert_eq!(
        received[0].1,
        json!({"model": "jev-latest", "state": rendered(&reply), "questions": judge_questions()})
    );
    assert_eq!(
        jev.judged_calls(&judgment.execution_id.to_string()).await,
        [JudgedCall {
            implementation: "eval".to_owned(),
            verdict: "passed".to_owned(),
            operation: "system_one".to_owned(),
            outcome: "succeeded".to_owned(),
            attempt_count: 1,
        }]
    );
    jev.deployment.shutdown().await;
}

/// Jev refusing the judge's request with 400 fails the verify with the
/// typed verification dependency failure, without relaying Jev's body, and
/// the judge does not retry the terminal refusal although `reply-quality`
/// allows two retries: Jev is asked exactly once.
///
/// # Panics
/// Panics when the verify succeeds, the error is untyped, or Jev is asked
/// again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judge_refusal_is_a_typed_error_without_retry() {
    let jev = Jev::start().await;
    let (state, _bundle) = jev.reviewer().await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify(
            "reply-quality",
            &Reply {
                question: "Where is my refund?",
                answer: "refused-form",
            },
        )
        .await;

    assert!(
        matches!(&refused, Err(WyrdError::VerificationDependencyFailed { message, .. })
            if !message.contains("key cred")),
        "{refused:?}"
    );
    assert_eq!(
        jev.received().await.len(),
        1,
        "a refused judge is never retried"
    );
    jev.deployment.shutdown().await;
}
