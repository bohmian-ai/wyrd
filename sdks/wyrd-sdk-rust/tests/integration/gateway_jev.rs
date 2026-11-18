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
use std::path::Path;

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
    Agent, CardRef, Check, ChoiceQuestion, ChoiceTransform, ComparisonOperator, Component,
    Execution, FieldValue, InputSlot, Inputs, JevPromptOptions, ProbabilityTransform, Prompt,
    Requirement, ScoreTransform, Service, VerdictTransform, Verifier,
};
use wyrd_ai::{CallProvenance, RequirementOutcome, VerificationVerdict};
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
    /// the CLI as the administrator, and deploy `jev` and the examples'
    /// `judge-pool`, each serving `system_one` and the `chat_completions`
    /// pass-through.
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
        let gateway = Gateway::connect_with(&deployment.admin())
            .await
            .expect("handle connects");
        for name in ["jev", "judge-pool"] {
            gateway
                .put_deployment(&ProviderDeployment {
                    name: ProviderDeploymentName::new(name).expect("deployment name"),
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
        }
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

/// The policy desk's questions: `refund` chooses a custom label,
/// `helpful` and `unresolved` are yes-or-no, `compliance` grades on three
/// ordered levels.
fn desk_questions() -> Value {
    json!({
        "refund": {"type": "choice", "instructions": "Does the reply follow the refund policy?",
                   "criteria": {"compliant": "It follows the policy.", "violation": "It breaks the policy.",
                                "unknown": "It cannot be judged."}},
        "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
        "unresolved": {"type": "noul", "instructions": "Does the reply leave the issue unresolved?"},
        "compliance": {"type": "score", "instructions": "How compliant is the reply?",
                       "criteria": ["Low", "Medium", "High"]},
    })
}

/// Jev's answers to the policy desk: the refund `label` at `confidence`,
/// `helpful` and `unresolved` probabilities, and the `compliance` score.
fn desk_answers(label: &str, confidence: f64, helpful: f64, unresolved: f64, score: f64) -> Value {
    json!({
        "refund": {"type": "choice", "choice": label, "confidence": confidence,
                   "probabilities": {label: confidence}},
        "helpful": {"type": "noul", "noul": helpful},
        "unresolved": {"type": "noul", "noul": unresolved},
        "compliance": {"type": "score", "score": score,
                       "legend": {"0": "Low", "1": "Medium", "2": "High"}},
    })
}

/// The `desk-judge` Jev Agent asking [`desk_questions`] about `${context}`.
///
/// # Panics
/// Panics when the Prompt does not build.
fn desk_judge() -> Agent {
    let questions = serde_json::from_value(desk_questions()).expect("native questions decode");
    Agent::new("desk-judge", "1.0.0")
        .prompt(
            Prompt::jev(
                "jev-latest",
                JevPromptOptions {
                    state: "${context}".to_owned(),
                    questions,
                },
            )
            .expect("the Jev prompt builds"),
        )
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "jev"}))
        .run_config(json!({"max_iterations": 1}))
}

/// The `policy-desk` Service: the [`desk_judge`] verified by `desk-policy`,
/// whose four requirements read the same answer through a custom Choice map,
/// Noul expected true and expected false, and `compliance >= 2`.
fn policy_desk() -> Service {
    policy_desk_at("1.0.0", "full")
}

/// [`policy_desk`] at `version`, whose `desk-policy` retains each actual
/// answer under `context_capture` `capture`.
fn policy_desk_at(version: &str, capture: &str) -> Service {
    let judge = desk_judge();
    let check = |transform| {
        Check::agent(judge.clone())
            .inputs([("context", "inputs.reply")])
            .verdict_transform(transform)
    };
    let probability = |question: &str, expected, pass_at, fail_at| {
        VerdictTransform::Probability(ProbabilityTransform {
            question: question.to_owned(),
            expected,
            pass_at,
            fail_at,
        })
    };
    let verifier = Verifier::new("desk-policy", version)
        .context_capture(capture)
        .inputs([("reply", InputSlot::json(json!({"type": "string"})))])
        .requirements([
            Requirement::new(
                "refund-policy",
                "The reply follows the refund policy.",
                check(VerdictTransform::Choice(ChoiceTransform {
                    question: "refund".to_owned(),
                    outcomes: Some(
                        serde_json::from_value(json!({
                            "compliant": "passed", "violation": "failed", "unknown": "inconclusive",
                        }))
                        .expect("outcomes decode"),
                    ),
                })),
            ),
            Requirement::new(
                "helpful",
                "The reply helps the customer.",
                check(probability("helpful", true, 0.75, 0.25)),
            ),
            Requirement::new(
                "resolved",
                "The reply resolves the issue.",
                check(probability("unresolved", false, 0.9, 0.1)),
            ),
            Requirement::new(
                "compliant",
                "The reply is at least highly compliant.",
                check(VerdictTransform::Score(ScoreTransform {
                    question: "compliance".to_owned(),
                    operator: ComparisonOperator::GreaterThanOrEquals,
                    expected: 2.0,
                })),
            ),
        ]);
    Service::new("policy-desk", version)
        .components([Component::new("desk-judge", judge).verified_by([verifier])])
}

/// Each acceptance policy reads its own question of one Jev answer: a custom
/// Choice map assigns every label its outcome whatever the confidence, Noul
/// bounds are inclusive for both expected polarities with an inconclusive
/// band between, and a Score comparison passes at its bound and fails just
/// below it. Each reason names the observed answer and the applied policy,
/// and the selected native answer is the requirement's actual value.
///
/// # Panics
/// Panics when declaration or verification fails, or an outcome, reason,
/// actual value, verdict, or upstream request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_policies_judge_choice_probability_and_score() {
    let jev = Jev::start().await;
    let cases = [
        (
            "DESK-PASS: the refund follows policy.",
            desk_answers("compliant", 0.3, 0.75, 0.02, 2.0),
            [RequirementOutcome::Passed; 4],
            VerificationVerdict::Passed,
            [
                "Question 'refund' selected 'compliant' (confidence 0.3), which the policy maps to passed.".to_owned(),
                "Question 'helpful' answered true with probability 0.75; the expected answer true has probability 0.75, at least pass_at 0.75.".to_owned(),
                format!("Question 'unresolved' answered true with probability 0.02; the expected answer false has probability {}, at least pass_at 0.9.", 1.0 - 0.02),
                "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.".to_owned(),
            ],
        ),
        (
            "DESK-FAIL: you get a refund whatever the policy says.",
            desk_answers("violation", 0.99, 0.25, 0.98, 1.8),
            [RequirementOutcome::Failed; 4],
            VerificationVerdict::Failed,
            [
                "Question 'refund' selected 'violation' (confidence 0.99), which the policy maps to failed.".to_owned(),
                "Question 'helpful' answered true with probability 0.25; the expected answer true has probability 0.25, at most fail_at 0.25.".to_owned(),
                format!("Question 'unresolved' answered true with probability 0.98; the expected answer false has probability {}, at most fail_at 0.1.", 1.0 - 0.98),
                "Question 'compliance' scored 1.8 on levels 0 to 2; greater_than_or_equals 2 does not hold.".to_owned(),
            ],
        ),
        (
            "DESK-UNSURE: we will see.",
            desk_answers("unknown", 0.5, 0.5, 0.5, 2.0),
            [
                RequirementOutcome::Inconclusive,
                RequirementOutcome::Inconclusive,
                RequirementOutcome::Inconclusive,
                RequirementOutcome::Passed,
            ],
            VerificationVerdict::Inconclusive,
            [
                "Question 'refund' selected 'unknown' (confidence 0.5), which the policy maps to inconclusive.".to_owned(),
                "Question 'helpful' answered true with probability 0.5; the expected answer true has probability 0.5, between fail_at 0.25 and pass_at 0.75.".to_owned(),
                "Question 'unresolved' answered true with probability 0.5; the expected answer false has probability 0.5, between fail_at 0.1 and pass_at 0.9.".to_owned(),
                "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.".to_owned(),
            ],
        ),
    ];
    for (marker, answers, ..) in &cases {
        let marker = marker.split(':').next().expect("a marker");
        jev.answer_when(
            marker,
            json!({"model": "jev-1.13.0", "answers": answers,
                   "usage": {"input_tokens": 60, "output_tokens": 8}}),
        )
        .await;
    }
    let state = WyrdState::declare_with(
        &policy_desk(),
        DeclareOptions {
            client: Some(jev.deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect("the policy desk declares");

    for (reply, answers, outcomes, verdict, reasons) in cases {
        let before = jev.received().await.len();
        let result = state
            .run_for_card("desk-judge")
            .expect("the judge's run opens")
            .observe()
            .verify("desk-policy", Inputs::named([("reply", json!(reply))]))
            .await
            .expect("the verification executes");

        for (((id, question), outcome), reason) in [
            ("refund-policy", "refund"),
            ("helpful", "helpful"),
            ("resolved", "unresolved"),
            ("compliant", "compliance"),
        ]
        .into_iter()
        .zip(outcomes)
        .zip(reasons)
        {
            let judged = requirement(&result, id);
            assert_eq!(
                (judged.outcome, judged.reason.as_str()),
                (outcome, reason.as_str()),
                "{reply}: {judged:#?}"
            );
            assert_eq!(judged.actual.as_ref(), Some(&answers[question]), "{id}");
            assert!(judged.citations.is_empty(), "{judged:#?}");
        }
        assert_eq!(result.outcome.verdict, verdict, "{reply}");
        let received = jev.received().await;
        assert!(received.len() > before, "{reply} reached Jev");
        for (_, body) in &received[before..] {
            assert_eq!(
                body,
                &json!({"model": "jev-latest", "state": reply, "questions": desk_questions()}),
            );
        }
    }
    jev.deployment.shutdown().await;
}

/// The `registered-desk` Service: `judge`, an Agent or the exact reference
/// to a registered one, verified by `registered-policy`, which reads its
/// answer through `transform`.
fn registered_desk(judge: impl Into<FieldValue> + Clone, transform: VerdictTransform) -> Service {
    let verifier = Verifier::new("registered-policy", "1.0.0")
        .inputs([("reply", InputSlot::json(json!({"type": "string"})))])
        .requirements([Requirement::new(
            "refund-policy",
            "The reply follows the refund policy.",
            Check::agent(judge.clone())
                .inputs([("context", "inputs.reply")])
                .verdict_transform(transform),
        )]);
    Service::new("registered-desk", "1.0.0")
        .components([Component::new("desk-judge", judge).verified_by([verifier])])
}

/// Local validation refuses a policy its in-graph Jev judge cannot answer.
/// Registration, not the client, decides whether a policy fits a judge no
/// local graph holds: a registered Jev Agent named only by its exact
/// reference passes local validation, and the server refuses a policy
/// selecting a question its Prompt does not declare, naming the field, before
/// Jev is called. A raw HTTP declaration that skips SDK validation is refused
/// the same way. The corrected definition declares and judges.
///
/// # Panics
/// Panics when a refused declaration is accepted, a refusal names another
/// field, Jev is called, or the corrected definition does not judge.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_registration_refuses_policies_no_local_graph_can_check() {
    let jev = Jev::start().await;
    jev.answer_when(
        "DESK-PASS",
        json!({"model": "jev-1.13.0", "answers": desk_answers("compliant", 0.3, 0.75, 0.02, 2.0)}),
    )
    .await;
    let options = || DeclareOptions {
        client: Some(jev.deployment.admin()),
        ..DeclareOptions::default()
    };
    WyrdState::declare_with(&policy_desk(), options())
        .await
        .expect("the judge registers");
    let asked = jev.received().await.len();

    let tone = || {
        VerdictTransform::Choice(ChoiceTransform {
            question: "tone".to_owned(),
            outcomes: None,
        })
    };
    let local = registered_desk(desk_judge(), tone())
        .validate()
        .expect_err("a local graph holding the judge refuses the policy");
    assert!(
        local.problem().details["diagnostics"][0]["message"]
            .as_str()
            .is_some_and(
                |message| message.contains("requirements[0].check.verdict_transform.question")
            ),
        "{local:?}"
    );

    let wrong = registered_desk(CardRef::agent("desk-judge", "1.0.0"), tone());
    wrong
        .validate()
        .expect("no local graph holds the registered judge's Prompt");
    let refused = WyrdState::declare_with(&wrong, options())
        .await
        .expect_err("registration refuses the policy");
    assert_eq!(refused.code(), "WYRD_SPEC_400_VALIDATION", "{refused}");
    assert!(
        refused
            .to_string()
            .contains("requirements[0].check.verdict_transform.question"),
        "{refused}"
    );

    let token = jev
        .deployment
        .admin()
        .access_token()
        .await
        .expect("access token mints");
    let raw = json!({"submissions": [
        {"apiVersion": "wyrd/v1", "kind": "Agent",
         "metadata": {"name": "bypass-judge", "version": "1.0.0"},
         "spec": {"prompt": {"provider": "jev", "model": "jev-latest", "format": "system_one",
                             "body": {"state": "${context}", "questions": desk_questions()}},
                  "execution_route": {"kind": "wyrd_gateway", "deployment": "jev"},
                  "run_config": {"max_iterations": 1}},
         "artifacts": []},
        {"apiVersion": "wyrd/v1", "kind": "Verifier",
         "metadata": {"name": "bypass-policy", "version": "1.0.0"},
         "spec": {"inputs": {"reply": {"type": "json", "schema": {"type": "string"}}},
                  "requirements": [{"id": "judged", "statement": "The reply is judged.",
                                    "check": {"kind": "agent",
                                              "agent": {"sibling": {"kind": "Agent", "name": "bypass-judge", "version": "1.0.0"}},
                                              "inputs": {"context": "inputs.reply"},
                                              "verdict_transform": {"kind": "probability", "question": "refund",
                                                                    "expected": true, "pass_at": 0.9, "fail_at": 0.1}}}]},
         "artifacts": []},
    ]});
    let response = reqwest::Client::new()
        .post(format!(
            "{}/v1/cards",
            jev.deployment
                .server()
                .base_url()
                .expect("bound server has a URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
        .header("idempotency-key", "bypass-policy")
        .json(&raw)
        .send()
        .await
        .expect("the server answers");
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("the refusal is JSON");
    assert_eq!(
        (status, body["code"].as_str()),
        (400, Some("WYRD_SPEC_400_VALIDATION")),
        "{body}"
    );
    assert!(
        body.to_string()
            .contains("requirements[0].check.verdict_transform.kind"),
        "{body}"
    );
    assert_eq!(
        jev.received().await.len(),
        asked,
        "no refused policy reached Jev"
    );

    let corrected = registered_desk(CardRef::agent("desk-judge", "1.0.0"), VerdictTransform::Choice(ChoiceTransform {
        question: "refund".to_owned(),
        outcomes: Some(
            serde_json::from_value(
                json!({"compliant": "passed", "violation": "failed", "unknown": "inconclusive"}),
            )
            .expect("outcomes decode"),
        ),
    }));
    corrected.validate().expect("the corrected desk validates");
    let state = WyrdState::declare_with(&corrected, options())
        .await
        .expect("the corrected policy registers");
    let result = state
        .run_for_card("desk-judge")
        .expect("the judge's run opens")
        .observe()
        .verify(
            "registered-policy",
            Inputs::named([("reply", json!("DESK-PASS: the refund follows policy."))]),
        )
        .await
        .expect("the verification executes");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    jev.deployment.shutdown().await;
}

/// A malformed native answer is a check error, never a judgment: a missing
/// selected answer, an answer of another form, an undeclared Choice label and
/// an out-of-range Noul, Score or Score confidence each make only the requirement reading it
/// inconclusive with the reason, with one Jev call per check and no retry.
///
/// # Panics
/// Panics when a malformed answer is judged, another requirement changes,
/// the verdict is not inconclusive, or a malformed answer is retried.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_malformed_answers_are_check_errors() {
    let jev = Jev::start().await;
    let valid = desk_answers("compliant", 0.3, 0.75, 0.02, 2.0);
    let corrupt = |question: &str, answer: Option<Value>| {
        let mut answers = valid.clone();
        match answer {
            Some(answer) => answers[question] = answer,
            None => {
                answers
                    .as_object_mut()
                    .expect("answers are an object")
                    .remove(question);
            }
        }
        answers
    };
    let cases = [
        (
            "MISSING",
            "refund-policy",
            corrupt("refund", None),
            "the decision response has no answer to question 'refund'",
        ),
        (
            "WRONG-FORM",
            "refund-policy",
            corrupt("refund", Some(json!({"type": "noul", "noul": 0.9}))),
            "the answer to question 'refund' is not the choice answer its policy reads",
        ),
        (
            "UNKNOWN-LABEL",
            "refund-policy",
            corrupt(
                "refund",
                Some(
                    json!({"type": "choice", "choice": "maybe", "confidence": 0.9,
                            "probabilities": {"maybe": 0.9}}),
                ),
            ),
            "the answer to question 'refund' selected an option the question does not declare",
        ),
        (
            "NOUL-RANGE",
            "helpful",
            corrupt("helpful", Some(json!({"type": "noul", "noul": 1.5}))),
            "the answer to question 'helpful' has a probability outside 0..1",
        ),
        (
            "SCORE-RANGE",
            "compliant",
            corrupt(
                "compliance",
                Some(
                    json!({"type": "score", "score": 3, "legend": {"0": "Low", "1": "Medium", "2": "High"}}),
                ),
            ),
            "the answer to question 'compliance' has a score outside its levels 0 to 2",
        ),
        (
            "SCORE-CONFIDENCE",
            "compliant",
            corrupt(
                "compliance",
                Some(json!({"type": "score", "score": 2, "confidence": 1.5,
                            "legend": {"0": "Low", "1": "Medium", "2": "High"}})),
            ),
            "the answer to question 'compliance' has a confidence or probability outside 0..1",
        ),
    ];
    for (marker, _, answers, _) in &cases {
        jev.answer_when(marker, json!({"model": "jev-1.13.0", "answers": answers}))
            .await;
    }
    let state = WyrdState::declare_with(
        &policy_desk(),
        DeclareOptions {
            client: Some(jev.deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect("the policy desk declares");

    for (marker, broken, _, reason) in cases {
        let before = jev.received().await.len();
        let result = state
            .run_for_card("desk-judge")
            .expect("the judge's run opens")
            .observe()
            .verify(
                "desk-policy",
                Inputs::named([("reply", json!(format!("{marker}: a reply")))]),
            )
            .await
            .expect("the verification executes");
        for id in ["refund-policy", "helpful", "resolved", "compliant"] {
            let judged = requirement(&result, id);
            if id == broken {
                assert_eq!(
                    judged.outcome,
                    RequirementOutcome::Inconclusive,
                    "{marker}: {judged:#?}"
                );
                assert!(judged.reason.contains(reason), "{marker}: {judged:#?}");
            } else {
                assert_eq!(
                    judged.outcome,
                    RequirementOutcome::Passed,
                    "{marker} {id}: {judged:#?}"
                );
            }
        }
        assert_eq!(
            result.outcome.verdict,
            VerificationVerdict::Inconclusive,
            "{marker}"
        );
        assert_eq!(
            jev.received().await.len(),
            before + 4,
            "{marker}: one Jev call per check, none retried"
        );
    }
    jev.deployment.shutdown().await;
}

/// A judged Jev check is retained, not just returned: after recording is
/// visible, the details read shows each requirement's verdict, reason and
/// selected native answer with the exact `desk-policy` version and the
/// `desk-judge` Agent and Jev route that answered. Under `context_capture:
/// hash` the same judgment keeps its verdict and reason while the raw answer
/// is retained only as a digest. Another tenant cannot read either.
///
/// # Panics
/// Panics when a judgment differs, the retained answer or provenance is
/// missing, a hashed answer is retained raw, or another tenant reads it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judgments_are_retained_under_capture_controls() {
    let jev = Jev::start().await;
    let answers = desk_answers("compliant", 0.3, 0.75, 0.02, 2.0);
    jev.answer_when(
        "DESK-PASS",
        json!({"model": "jev-1.13.0", "answers": answers}),
    )
    .await;
    let foreign = jev
        .deployment
        .client(&jev.deployment.other_tenant_admin("other-tenant").await);

    for (version, capture) in [("1.0.0", "full"), ("2.0.0", "hash")] {
        let state = WyrdState::declare_with(
            &policy_desk_at(version, capture),
            DeclareOptions {
                client: Some(jev.deployment.admin()),
                ..DeclareOptions::default()
            },
        )
        .await
        .expect("the policy desk declares");
        let result = state
            .run_for_card("desk-judge")
            .expect("the judge's run opens")
            .observe()
            .verify(
                "desk-policy",
                Inputs::named([("reply", json!("DESK-PASS: the refund follows policy."))]),
            )
            .await
            .expect("the verification executes");
        assert_eq!(
            result.outcome.verdict,
            VerificationVerdict::Passed,
            "{capture}"
        );

        let details = jev.deployment.details(result.execution_id).await;
        assert_eq!(details.verifier.version.to_string(), version, "{capture}");
        for (id, question) in [
            ("refund-policy", "refund"),
            ("helpful", "helpful"),
            ("resolved", "unresolved"),
            ("compliant", "compliance"),
        ] {
            let returned = requirement(&result, id);
            let retained = details
                .outcome
                .requirements
                .iter()
                .find(|requirement| requirement.result.id.as_str() == id)
                .unwrap_or_else(|| panic!("{id} is retained: {details:#?}"));
            assert_eq!(
                (retained.result.outcome, &retained.result.reason),
                (returned.outcome, &returned.reason),
                "{capture} {id}"
            );
            let actual = retained
                .result
                .actual
                .as_ref()
                .expect("the answer is retained");
            if capture == "full" {
                assert_eq!(actual, &answers[question], "{id}");
            } else {
                assert!(
                    actual.as_str().is_some_and(|digest| digest.len() == 64
                        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())),
                    "{id} retains only a digest: {actual}"
                );
            }
            assert!(
                matches!(
                    &retained.provenance,
                    Some(CallProvenance::Agent { agent: Some(agent), route, .. })
                        if agent.name.as_str() == "desk-judge"
                            && serde_json::to_value(route).expect("route encodes")["deployment"] == "jev"
                ),
                "{id}: {:?}",
                retained.provenance
            );
        }

        let refused = Execution::connect_with(result.execution_id, &foreign)
            .await
            .expect("the handle rebuilds from its ID")
            .details()
            .await
            .expect_err("another tenant reads no judgment");
        assert_eq!(refused.code(), "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
    }
    jev.deployment.shutdown().await;
}

/// The public `examples/gateway-setup/refund-desk.yaml` runs as written:
/// `no-refund-promise` asks Jev one canonical Choice question through the
/// `judge-pool` deployment and reads the selected label as the verdict, so
/// a reply promising a guaranteed refund fails and a compliant one passes,
/// each with the documented reason and no citations.
///
/// # Panics
/// Panics when the example does not validate, declare or judge as
/// documented.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_refund_example_judges_as_documented() {
    let jev = Jev::start().await;
    let choice = |label: &str| {
        json!({"model": "jev-1.13.0", "answers": {"refund_policy": {
            "type": "choice", "choice": label, "confidence": 1.0,
            "probabilities": {"passed": f64::from(u8::from(label == "passed")),
                              "failed": f64::from(u8::from(label == "failed")),
                              "inconclusive": 0.0}}}})
    };
    jev.answer_when("REFUND-PROMISE", choice("failed")).await;
    jev.answer_when("REFUND-POLICY", choice("passed")).await;
    let desk =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gateway-setup/refund-desk.yaml");
    let state = WyrdState::declare_with(
        desk.as_path(),
        DeclareOptions {
            client: Some(jev.deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect("the example declares");

    for (answer, label, verdict) in [
        (
            "REFUND-PROMISE: you are guaranteed a full refund.",
            "failed",
            VerificationVerdict::Failed,
        ),
        (
            "REFUND-POLICY: we refund when the policy allows it.",
            "passed",
            VerificationVerdict::Passed,
        ),
    ] {
        let result = state
            .run_for_card("jev")
            .expect("the judge's run opens")
            .observe()
            .verify(
                "no-refund-promise",
                Inputs::named([("answer", json!(answer))]),
            )
            .await
            .expect("the verification executes");
        let judged = requirement(&result, "no-refund-promise");
        assert_eq!(
            judged.reason,
            format!("Question 'refund_policy' selected '{label}' (confidence 1)."),
            "{answer}"
        );
        assert!(judged.citations.is_empty(), "{judged:#?}");
        assert_eq!(result.outcome.verdict, verdict, "{answer}");
    }
    jev.deployment.shutdown().await;
}
