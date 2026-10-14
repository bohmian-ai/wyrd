//! A tenant reaches TypeSafe Jev through the Wyrd Gateway: native
//! `POST /v1/systemone` callers and OpenAI-shaped Chat Completions callers
//! both reach one Jev deployment, and a Verifier uses that deployment as its
//! LLM judge. The local upstream stands in for `api.typesafe.ai` and sees
//! only the operator's Jev key.
//!
//! No Jev SDK is a dependency: a Jev caller is a plain JSON POST with a bearer
//! token, which is the whole of what such a client sends.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use serde::Serialize;
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::cli;
use wyrd_ai::gateway::{
    GatewayOperation, ModelId, ModelRef, ProviderAdapter, ProviderAuth, ProviderCredentialName,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentName, ProviderId,
};
use wyrd_ai::operator_connections::SecretBearer;
use wyrd_ai::state::WyrdState;
use wyrd_ai::{Gateway, VerificationVerdict, VerifierCounts, VerifierKind};
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

/// One agent reply, the input `reply-quality` judges.
#[derive(Serialize)]
struct Reply {
    /// The reply text.
    answer: &'static str,
}

/// A Gateway with one Jev deployment in front of a local Jev upstream.
///
/// The upstream answers a helpful, polite verdict unless the state names a
/// scripted case: `Go away` is judged unhelpful and rude, `rate-limited` is
/// throttled with 429, `overloaded` answers 529, and `bad-key` answers 401.
struct Jev {
    /// The deployment.
    deployment: Deployment,
    /// The local Jev upstream.
    upstream: MockServer,
}

impl Jev {
    /// Start the upstream and a verification-capable server, write the
    /// `jev-key` credential with the CLI as the administrator, and deploy
    /// `jev/jev-latest` serving `system_one` and `chat_completions`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        for (needle, status) in [("rate-limited", 429), ("overloaded", 529), ("bad-key", 401)] {
            Mock::given(method("POST"))
                .and(path("/v1/systemone"))
                .and(body_string_contains(needle))
                .respond_with(
                    ResponseTemplate::new(status).set_body_json(json!({"detail": "key cred"})),
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
                name: ProviderDeploymentName::new("jev-latest").expect("deployment name"),
                model: ModelRef {
                    provider: ProviderId::new("jev").expect("provider"),
                    model: ModelId::new("jev-latest").expect("model"),
                },
                adapter: ProviderAdapter::Jev,
                auth: ProviderAuth::Bearer {
                    credential: ProviderCredentialName::new("jev-key").expect("credential"),
                },
                capabilities: BTreeSet::from([
                    GatewayOperation::SystemOne,
                    GatewayOperation::ChatCompletions,
                ]),
                routing_weight: NonZeroU32::MIN,
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
            &json!({"model": "jev-latest", "state": "Thanks!", "questions": questions()}),
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

/// An OpenAI-shaped call whose one user message is the JSON question set
/// reaches Jev as `systemone` and returns the answers as assistant JSON.
///
/// # Panics
/// Panics when a step fails or the reply or upstream request differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn chat_completions_reach_jev_as_questions() {
    let jev = Jev::start().await;
    let content = json!({"state": "Thanks!", "questions": questions()}).to_string();

    let (status, reply) = jev
        .post(
            "/v1/chat/completions",
            &json!({"model": "jev/jev-latest",
                    "messages": [{"role": "user", "content": content}]}),
        )
        .await;

    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["object"], "chat.completion");
    assert_eq!(reply["model"], "jev-1.13.0");
    assert_eq!(reply["choices"][0]["finish_reason"], "stop");
    let answered: Value = serde_json::from_str(
        reply["choices"][0]["message"]["content"]
            .as_str()
            .expect("content is text"),
    )
    .expect("content is JSON");
    let expected = answer(0.93, "polite");
    assert_eq!(
        answered,
        json!({"model": "jev-1.13.0", "answers": expected["answers"]})
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
    assert_eq!(
        received[0].1,
        json!({"model": "jev-latest", "state": "Thanks!", "questions": questions()})
    );
    jev.deployment.shutdown().await;
}

/// Ordinary chat, a streamed request, and a native body with an unknown
/// question form are refused before Jev is called.
///
/// # Panics
/// Panics when a call is not refused with the stable code or Jev is reached.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn ordinary_chat_is_refused_before_jev() {
    let jev = Jev::start().await;
    let content = json!({"state": "Thanks!", "questions": questions()}).to_string();

    for (route, body) in [
        (
            "/v1/chat/completions",
            json!({"model": "jev/jev-latest", "messages": [{"role": "user", "content": "hi"}]}),
        ),
        (
            "/v1/chat/completions",
            json!({"model": "jev/jev-latest", "stream": true,
                   "messages": [{"role": "user", "content": content}]}),
        ),
        (
            "/v1/systemone",
            json!({"model": "jev-latest", "state": "s",
                   "questions": {"q": {"type": "essay", "instructions": "x"}}}),
        ),
    ] {
        let (status, reply) = jev.post(route, &body).await;
        assert_eq!(status, 400, "{route} {reply}");
        assert_eq!(reply["error"]["code"], "WYRD_GATEWAY_400_INVALID_REQUEST");
    }
    assert!(jev.received().await.is_empty(), "Jev was never called");
    jev.deployment.shutdown().await;
}

/// Jev throttling, overload, and a refused key map to stable Wyrd errors
/// that never relay Jev's own body.
///
/// # Panics
/// Panics when a refusal maps to another status or code, or leaks the body.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_refusals_map_to_stable_errors() {
    let jev = Jev::start().await;

    for (state, status, code) in [
        ("rate-limited", 429, "WYRD_GATEWAY_429_UPSTREAM_THROTTLED"),
        ("overloaded", 502, "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"),
        ("bad-key", 502, "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"),
    ] {
        let (answered, reply) = jev
            .post(
                "/v1/systemone",
                &json!({"model": "jev-latest", "state": state, "questions": questions()}),
            )
            .await;
        assert_eq!(
            (answered, reply["error"]["code"].as_str()),
            (status, Some(code)),
            "{state}: {reply}"
        );
        assert!(!reply.to_string().contains("key cred"), "{reply}");
    }
    jev.deployment.shutdown().await;
}

/// A helpful, polite reply passes the `reply-quality` Verifier whose LLM
/// judge is the Jev deployment, through the OpenAI-shaped projection.
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
    assert_eq!(received[0].1["state"], "Your refund is on its way.");
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
        .verify("reply-quality", &Reply { answer: "Go away." })
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

/// A reply whose text would break the question JSON the judge Prompt
/// interpolates it into is refused before Jev is called, never judged on
/// a mangled state.
///
/// # Panics
/// Panics when the reply is judged or Jev is reached.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn jev_judge_refuses_a_reply_that_breaks_the_question_json() {
    let jev = Jev::start().await;
    let (state, _bundle) = jev.reviewer().await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify(
            "reply-quality",
            &Reply {
                answer: "He said \"no\".",
            },
        )
        .await
        .expect_err("a reply that breaks the question JSON is not judged");

    assert_eq!(refused.code(), "WYRD_VERIFICATION_502_DEPENDENCY_FAILED");
    assert!(jev.received().await.is_empty(), "Jev was never called");
    jev.deployment.shutdown().await;
}
