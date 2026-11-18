//! A tenant reaches TypeSafe Jev through the Wyrd Gateway: native
//! `POST /v1/systemone` callers reach one Jev deployment with typed questions
//! and get typed answers back, OpenAI-shaped callers reach the same deployment
//! on `/v1/chat/completions` by sending the typed Jev request as their last
//! user message. The local upstream stands in for `api.typesafe.ai` and sees
//! only the operator's Jev key.
//!
//! No Jev or OpenAI SDK is a dependency: each caller is a plain JSON POST with
//! a bearer token, which is the whole of what such a client sends.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::Gateway;
use wyrd_ai::cli;
use wyrd_ai::gateway_admin::{
    DeploymentAccess, GatewayOperation, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_ai::operator_connections::SecretBearer;
use wyrd_ai::state::{DeclareOptions, WyrdState};
use wyrd_ai::verify::{
    Agent, Check, ChoiceQuestion, ChoiceTransform, Component, InputSlot, Inputs, JevPromptOptions,
    Prompt, Requirement, Service, VerdictTransform, Verifier,
};
use wyrd_ai::{RequirementOutcome, VerificationVerdict};
use wyrd_testing::server::WyrdTestServer;

use crate::support::Deployment;
use crate::verifier_building_blocks::requirement;

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

/// A Gateway with one Jev deployment in front of a local Jev upstream.
///
/// The upstream answers a helpful, polite verdict unless the state names a
/// scripted case: `refused-form` is refused with 400, `rate-limited` is
/// throttled with 429, and `bad-key` answers 401. Every refusal body echoes the operator key after `key cred`.
struct Jev {
    /// The deployment.
    deployment: Deployment,
    /// The local Jev upstream.
    upstream: MockServer,
}

impl Jev {
    /// Start the upstream and the server, write the `jev-key` credential with
    /// the CLI as the administrator, and deploy `jev/jev-latest` serving
    /// `system_one` and the `chat_completions` pass-through.
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
        Gateway::connect_with(&deployment.admin())
            .await
            .expect("handle connects")
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

    /// Answer `systemone` requests whose state contains `marker` with `body`,
    /// ahead of the default answer.
    async fn answer_when(&self, marker: &str, body: Value) {
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_string_contains(marker))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .with_priority(1)
            .mount(&self.upstream)
            .await;
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

/// The Choice question the refund judge asks, with the canonical outcome
/// labels as its options.
fn refund_question() -> ChoiceQuestion {
    ChoiceQuestion {
        instructions: "Does the reply promise a refund the policy does not allow?".to_owned(),
        criteria: [
            ("passed", "The reply makes no refund promise."),
            ("failed", "The reply promises a refund."),
            ("inconclusive", "The reply cannot be judged."),
        ]
        .into_iter()
        .map(|(label, description)| (label.to_owned(), description.to_owned()))
        .collect(),
    }
}

/// Jev's answer to the refund question: `choice` with confidence 0.9.
fn refund_answer(choice: &str) -> Value {
    let probabilities: serde_json::Map<String, Value> = ["passed", "failed", "inconclusive"]
        .into_iter()
        .map(|label| {
            (
                label.to_owned(),
                json!(if label == choice { 0.9 } else { 0.05 }),
            )
        })
        .collect();
    json!({
        "model": "jev-1.13.0",
        "answers": {"refund_policy": {
            "type": "choice", "choice": choice, "confidence": 0.9,
            "probabilities": probabilities,
        }},
        "usage": {"input_tokens": 50, "output_tokens": 5},
    })
}

/// The `refund-judge` Agent: a Jev `system_one` Prompt asking the refund
/// question about the bound `${context}`, routed to the `jev` deployment.
///
/// # Panics
/// Panics when the Prompt does not build.
fn refund_judge() -> Agent {
    let prompt = Prompt::jev(
        "jev-latest",
        JevPromptOptions {
            state: "${context}".to_owned(),
            questions: [("refund_policy".to_owned(), refund_question().into())].into(),
        },
    )
    .expect("the Jev prompt builds");
    Agent::new("refund-judge", "1.0.0")
        .prompt(prompt)
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "jev"}))
        .run_config(json!({"max_iterations": 1}))
}

/// `refund-promise`: one requirement judged by `judge`'s Choice answer,
/// read through `transform`, over the `reply` slot.
fn refund_verifier(judge: &Agent, transform: VerdictTransform) -> Verifier {
    Verifier::new("refund-promise", "1.0.0")
        .inputs([("reply", InputSlot::json(json!({"type": "string"})))])
        .requirements([Requirement::new(
            "no-refund-promise",
            "The reply makes no refund promise the policy does not allow.",
            Check::agent(judge.clone())
                .inputs([("context", "inputs.reply")])
                .verdict_transform(transform),
        )])
}

/// A Jev Choice check authored with the public Rust Prompt and transform
/// types judges each support reply in one upstream call: the selected
/// `passed`, `failed`, or `inconclusive` label becomes the requirement and
/// execution verdict, with a reason naming the answer, no citations, and no
/// response schema or caller parser.
///
/// # Panics
/// Panics when declaration or verification fails, a verdict, reason, or
/// citation differs, or the upstream request is not the bound Jev request.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_choice_check_becomes_the_verdict() {
    let jev = Jev::start().await;
    for (marker, choice) in [
        ("CASE-PASS", "passed"),
        ("CASE-FAIL", "failed"),
        ("CASE-UNSURE", "inconclusive"),
    ] {
        jev.answer_when(marker, refund_answer(choice)).await;
    }
    let judge = refund_judge();
    let transform = VerdictTransform::Choice(ChoiceTransform {
        question: "refund_policy".to_owned(),
        outcomes: None,
    });
    let service = Service::new("refund-desk", "1.0.0").components([Component::new(
        "refund-judge",
        judge.clone(),
    )
    .verified_by([refund_verifier(&judge, transform)])]);
    let state = WyrdState::declare_with(
        &service,
        DeclareOptions {
            client: Some(jev.deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect("the Jev check declares");

    for (reply, outcome, verdict) in [
        (
            "CASE-PASS: \"Thanks\" for asking.\nRefunds follow the policy.",
            RequirementOutcome::Passed,
            VerificationVerdict::Passed,
        ),
        (
            "CASE-FAIL: you will get a full refund today.",
            RequirementOutcome::Failed,
            VerificationVerdict::Failed,
        ),
        (
            "CASE-UNSURE: maybe.",
            RequirementOutcome::Inconclusive,
            VerificationVerdict::Inconclusive,
        ),
    ] {
        let before = jev.received().await.len();
        let result = state
            .run_for_card("refund-judge")
            .expect("the judge's run opens")
            .observe()
            .verify("refund-promise", Inputs::named([("reply", json!(reply))]))
            .await
            .expect("the verification executes");

        let judged = requirement(&result, "no-refund-promise");
        let choice = format!("{outcome:?}").to_lowercase();
        assert_eq!(judged.outcome, outcome, "{reply}: {judged:#?}");
        assert_eq!(
            judged.reason,
            format!("Question 'refund_policy' selected '{choice}' (confidence 0.9)."),
        );
        assert!(judged.citations.is_empty(), "{judged:#?}");
        assert_eq!(result.outcome.verdict, verdict);
        let received = jev.received().await;
        assert_eq!(received.len(), before + 1, "one Jev call per verification");
        assert_eq!(
            received[before].1,
            json!({"model": "jev-latest", "state": reply, "questions": {
                "refund_policy": native_refund_question(),
            }}),
        );
    }
    jev.deployment.shutdown().await;
}

/// The refund question as the native Jev question it lowers to.
fn native_refund_question() -> Value {
    json!({
        "type": "choice",
        "instructions": "Does the reply promise a refund the policy does not allow?",
        "criteria": {
            "passed": "The reply makes no refund promise.",
            "failed": "The reply promises a refund.",
            "inconclusive": "The reply cannot be judged.",
        },
    })
}
