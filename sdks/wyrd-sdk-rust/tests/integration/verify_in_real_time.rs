//! An assistant judges its own behavior while it runs: `observe().verify`
//! returns a `Judgment` from the Verifier bound to the observed Card.

use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::cards::CardRef;
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::{Bifrost, VerificationVerdict, VerifierCounts, VerifierKind, WyrdClient};
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
    /// The registered assistant Service.
    service: CardRef,
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
            service: assistant.root.clone(),
            latency_drift: registered(&assistant, "latency-drift"),
        }
    }

    /// The hydrated assistant, created with the Service's own client and
    /// started with Bifrost.
    ///
    /// The Service's key comes from the `issue_key` CLI function with no Role
    /// granted beyond its default, so Bifrost startup and every verify run
    /// with exactly what a newly registered Service holds.
    ///
    /// # Panics
    /// Panics when the key is not issued, the bundle does not load, or
    /// Bifrost does not start.
    async fn state(&self) -> WyrdState {
        let key = self.deployment.service_key(&self.service).await;
        let state = self.state_as(self.deployment.client(&key));
        state.start_bifrost().await.expect("Bifrost starts");
        state
    }

    /// The hydrated assistant, loaded with its server calls fixed to
    /// `client` and Bifrost not started.
    ///
    /// # Panics
    /// Panics when the bundle does not load.
    fn state_as(&self, client: WyrdClient) -> WyrdState {
        WyrdState::from_path_with_client(self.bundle.path().join("bundle"), client)
            .expect("bundle loads offline")
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
    let state = assistant.state().await;

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
    let state = assistant.state().await;

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
    let state = assistant.state().await;

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

/// One hundred latencies spread like the healthy baseline's 0 to 99 ms pass
/// the drift Verifier on the Model.
///
/// # Panics
/// Panics when the baseline does not fit, the verify fails, or the judgment
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn model_latency_like_the_baseline_passes_its_verifier() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    assistant.wait_for_baseline().await;
    let healthy = (0..100)
        .map(|row| Latency {
            latency: (row * 37) % 100,
        })
        .collect::<Vec<_>>();

    let judgment = state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .verify("latency-drift", &healthy)
        .await
        .expect("the latencies are judged");

    assert!(judgment.passed());
    assert_eq!(judgment.kind, VerifierKind::DriftPsi);
    assert_eq!(&judgment.subject, state.card_ref("model").expect("model"));
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
    let state = assistant.state().await;
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
            Some(VerifierCounts::Drift {
                drifted_features: 1,
                total_features: 1
            })
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
    let key = assistant.deployment.service_key(&unfitted.root).await;
    let state = WyrdState::from_path_with_client(
        bundle.path().join("bundle"),
        assistant.deployment.client(&key),
    )
    .expect("bundle loads");
    state.start_bifrost().await.expect("Bifrost starts");
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
    let state = assistant.state().await;

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

/// A list where the Verifier judges one answer fails before any call.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn input_of_the_wrong_shape_fails_locally() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;

    let refused = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-is-yes", &[Answer { answer: "yes" }])
        .await
        .expect_err("a list is not one answer");

    assert_eq!(refused.code(), "WYRD_SDK_400_INVALID_OBSERVATION");
    assistant.deployment.shutdown().await;
}

/// A state created with a machine principal holding only the `workload`
/// Role, which grants no `evals:run`, cannot verify.
///
/// # Panics
/// Panics when the verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_evals_run_is_refused() {
    let assistant = Assistant::start().await;
    let workload = assistant
        .deployment
        .key("workload_only", &["workload"])
        .await;
    let state = assistant.state_as(assistant.deployment.client(&workload));

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
    let foreign = assistant
        .deployment
        .other_tenant_admin("other-tenant")
        .await;
    let state = assistant.state_as(assistant.deployment.client(&foreign));
    state.start_bifrost().await.expect("Bifrost starts");

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
    let state = assistant.state().await;
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

    let observed = Bifrost::connect(&assistant.deployment.admin())
        .await
        .expect("Bifrost connects")
        .sql(
            "SELECT record_id FROM vala.eval.observations WHERE run_id = $1",
            &[QueryParam::String(run.run_id().to_string())],
        )
        .await
        .expect("observations read back");
    assert_eq!(observed.num_rows(), 0);
    assistant.deployment.shutdown().await;
}
