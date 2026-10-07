//! An assistant judges its own behavior while it runs: `observe().verify`
//! returns a `Judgment` from the Verifier bound to the observed Card.

use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::Bifrost;
use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::cards::CardRef;
use wyrd_sdk::state::WyrdState;
use wyrd_spec::card::operator::VerifierCounts;
use wyrd_spec::verification::{VerificationVerdict, VerifierKind};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, hydrate, register, registered};

/// How long a test waits for the latency baseline to fit.
const BASELINE_FIT: Duration = Duration::from_secs(90);

/// One agent answer, the input the Eval Verifiers judge.
#[derive(Serialize)]
struct Answer {
    /// The answer text.
    answer: &'static str,
}

/// One observed request latency, the input `latency-drift` judges.
#[derive(Serialize)]
struct Latency {
    /// Milliseconds.
    latency: u32,
}

/// The assistant deployment: a server running the verification runtime whose
/// provider upstream is a local judge that passes every answer, with the
/// assistant Service registered and hydrated.
struct Assistant {
    /// The deployment.
    deployment: Deployment,
    /// The local judge the server's Eval judge calls.
    judge: MockServer,
    /// The hydrated assistant bundle.
    bundle: TempDir,
    /// The registered `latency-drift` Verifier.
    latency_drift: CardRef,
}

impl Assistant {
    /// Start the judge and the server, then register the latency baseline,
    /// the latency Model, and the assistant, and hydrate the assistant.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let judge = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-judge",
                "object": "chat.completion",
                "created": 1,
                "model": "gpt-test",
                "choices": [{
                    "index": 0,
                    "finish_reason": "stop",
                    "message": { "role": "assistant", "content": "{\"passed\":true}" },
                }],
                "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 },
            })))
            .mount(&judge)
            .await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    judge.uri().parse().expect("judge URL parses"),
                ),
        )
        .await;
        let cards = deployment.cards();
        register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
        register(&cards, "cards/verify_in_real_time/latency-model.yaml").await;
        let assistant = register(&cards, "cards/verify_in_real_time/assistant.yaml").await;
        let bundle = hydrate(&cards, &assistant.root).await;
        Self {
            deployment,
            judge,
            bundle,
            latency_drift: registered(&assistant, "latency-drift"),
        }
    }

    /// The hydrated assistant, started with Bifrost as the Service's own
    /// principal holding `roles` beyond `workload`.
    ///
    /// # Panics
    /// Panics when the bundle does not load or Bifrost does not start.
    async fn state(&self, roles: &[&str]) -> WyrdState {
        let state =
            WyrdState::from_path(&self.bundle.path().join("bundle")).expect("bundle loads offline");
        let key = self.deployment.service_key(state.root_ref(), roles).await;
        self.start_bifrost(&state, &key).await;
        state
    }

    /// Start Bifrost on `state` as the principal `key` names.
    ///
    /// # Panics
    /// Panics when Bifrost does not start.
    async fn start_bifrost(&self, state: &WyrdState, key: &str) {
        state
            .start_bifrost_with(&self.deployment.client(key), None)
            .await
            .expect("Bifrost starts");
    }

    /// Wait until the `latency-drift` baseline has fitted.
    ///
    /// # Panics
    /// Panics when the baseline does not fit in time.
    async fn wait_for_baseline(&self) {
        self.deployment
            .server()
            .wait_for_baseline(
                self.latency_drift
                    .uid
                    .as_ref()
                    .expect("registered Verifier has a UID"),
                BASELINE_FIT,
            )
            .await
            .expect("the latency baseline fits");
    }
}

/// An answer the assertion expects passes, judged against the agent.
///
/// # Panics
/// Panics when the verify fails or the judgment differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_answer_passes_its_verifier() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");

    assert!(judgment.passed());
    assert_eq!(judgment.kind, VerifierKind::EvalAssertion);
    assert_eq!(judgment.verifier.name.as_str(), "answer-is-yes");
    assert_eq!(&judgment.subject, state.card_ref("agent").expect("agent"));
    assistant.deployment.shutdown().await;
}

/// An answer the assertion does not expect fails.
///
/// # Panics
/// Panics when the verify fails or the judgment differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_answer_fails_its_verifier() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-yes", &Answer { answer: "no" })
        .await
        .expect("the answer is judged");

    assert_eq!(
        (judgment.verdict, judgment.kind),
        (VerificationVerdict::Failed, VerifierKind::EvalAssertion)
    );
    assistant.deployment.shutdown().await;
}

/// The LLM judge grades the answer through the server's provider upstream.
///
/// # Panics
/// Panics when the verify fails, the judgment differs, or the judge did not
/// receive the graded answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judged_answer_passes_the_llm_judge() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-judged", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");

    assert!(judgment.passed());
    assert_eq!(judgment.kind, VerifierKind::EvalLlmJudge);
    let graded = assistant
        .judge
        .received_requests()
        .await
        .expect("the judge records requests");
    assert_eq!(graded.len(), 1);
    assert!(
        String::from_utf8_lossy(&graded[0].body).contains("Grade the answer yes."),
        "the judge received the answer"
    );
    assistant.deployment.shutdown().await;
}

/// Latencies all at the slow end of the baseline drift, and the drift
/// Verifier judges the Model failed on its one feature.
///
/// # Panics
/// Panics when the baseline does not fit, the verify fails, or the judgment
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn model_latency_drift_is_judged_failed() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;
    assistant.wait_for_baseline().await;
    let slow = (0..100)
        .map(|_| Latency { latency: 99 })
        .collect::<Vec<_>>();

    let judgment = state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .verify("latency-drift", &slow)
        .await
        .expect("the latencies are judged");

    assert_eq!(
        (judgment.verdict, judgment.kind, judgment.counts),
        (
            VerificationVerdict::Failed,
            VerifierKind::DriftPsi,
            VerifierCounts::Drift {
                drifted_features: 1,
                total_features: 1
            }
        )
    );
    assistant.deployment.shutdown().await;
}

/// A drift Verifier whose baseline has not fitted refuses to judge.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verify_before_baseline_ready_is_refused() {
    let assistant = Assistant::start().await;
    let cards = assistant.deployment.cards();
    let unfitted = register(&cards, "cards/verify_in_real_time/unfitted-assistant.yaml").await;
    let bundle = hydrate(&cards, &unfitted.root).await;
    let state = WyrdState::from_path(&bundle.path().join("bundle")).expect("bundle loads");
    let key = assistant
        .deployment
        .service_key(&unfitted.root, &["admin"])
        .await;
    assistant.start_bifrost(&state, &key).await;
    let tiers = (0..5)
        .map(|_| json!({ "tier": "gold" }))
        .collect::<Vec<_>>();

    let refused = state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .verify("tier-drift", &tiers)
        .await
        .expect_err("an unfitted baseline is refused");

    assert_eq!(refused.code(), "WYRD_VERIFICATION_409_BASELINE_NOT_READY");
    assistant.deployment.shutdown().await;
}

/// Naming a Verifier not bound to the observed Card fails before any call.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unbound_verifier_fails_locally() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("not-bound-here", &Answer { answer: "yes" })
        .await
        .expect_err("an unbound Verifier is refused");

    assert_eq!(refused.code(), "WYRD_SDK_404_UNKNOWN_VERIFIER");
    assistant.deployment.shutdown().await;
}

/// A principal without `evals:run` cannot verify.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_evals_run_is_refused() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&[]).await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect_err("the workload role cannot verify");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assistant.deployment.shutdown().await;
}

/// Another tenant's administrator cannot verify this tenant's assistant.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn another_tenant_cannot_verify_the_assistant() {
    let assistant = Assistant::start().await;
    let state =
        WyrdState::from_path(&assistant.bundle.path().join("bundle")).expect("bundle loads");
    let foreign = assistant
        .deployment
        .other_tenant_admin("other-tenant")
        .await;
    assistant.start_bifrost(&state, &foreign).await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect_err("another tenant cannot verify");

    assert_eq!(refused.code(), "WYRD_VERIFICATION_404_TARGET_NOT_FOUND");
    assistant.deployment.shutdown().await;
}

/// Judging an input does not record it as an observation.
///
/// # Panics
/// Panics when the verify or the read-back fails, or an observation exists.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verify_records_no_observation() {
    let assistant = Assistant::start().await;
    let state = assistant.state(&["admin"]).await;
    let run = state.run_for_card("agent").expect("agent run opens");

    run.observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");
    state.shutdown().await.expect("Bifrost stops");
    assistant
        .deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");

    let observed = Bifrost::query_only(&assistant.deployment.admin())
        .sql(
            "SELECT record_id FROM vala.eval.observations WHERE run_id = $1",
            &[QueryParam::String(run.run_id().to_string())],
        )
        .await
        .expect("observations read back");
    assert_eq!(observed.num_rows(), 0);
    assistant.deployment.shutdown().await;
}
