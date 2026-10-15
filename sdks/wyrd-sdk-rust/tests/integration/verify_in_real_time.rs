//! An assistant judges its own behavior while it runs: `observe().verify`
//! returns a `Judgment` from the Verifier bound to the observed Card, whether
//! that Verifier is an Eval, a Drift, or a single task check. Only
//! `verifier:run` on the named Verifier authorizes the call; an LLM judge
//! then calls the gateway as that same caller.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::bifrost::QueryParam;
use wyrd_ai::cards::CardRef;
use wyrd_ai::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::state::WyrdState;
use wyrd_ai::{
    Bifrost, Gateway, SpanRecord, VerificationVerdict, VerifierCounts, VerifierKind, WyrdClient,
    cli,
};
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::configure_gateway;
use crate::support::{Deployment, ancestry, coordinates, hydrate, register, registered};

/// How long a test waits for the latency baseline to fit.
const BASELINE_FIT: Duration = Duration::from_secs(90);

/// One agent answer, the input the Eval Verifiers judge.
#[derive(Serialize)]
pub(crate) struct Answer {
    /// The answer text.
    pub(crate) answer: &'static str,
}

/// One observed request latency, the input `latency-drift` judges.
#[derive(Serialize)]
struct Latency {
    /// Milliseconds.
    latency: u32,
}

/// The assistant deployment: a server running the verification runtime whose
/// gateway deploys `gpt-4o` on a local judge that passes every answer, with
/// the assistant Service registered and hydrated. The gateway captures call
/// metadata, so every judge call lands in Bifrost.
pub(crate) struct Assistant {
    /// The deployment.
    pub(crate) deployment: Deployment,
    /// The local judge the server's Eval judge calls.
    judge: MockServer,
    /// The hydrated assistant bundle.
    bundle: TempDir,
    /// The registered assistant Service.
    pub(crate) service: CardRef,
    /// The registered `latency-drift` Verifier.
    latency_drift: CardRef,
    /// The registered `answer-is-yes` Eval Verifier.
    answer_is_yes: CardRef,
    /// The registered `answer-check` task Verifier.
    answer_check: CardRef,
    /// The registered `answer-graded` task LLM-judge Verifier.
    answer_graded: CardRef,
    /// The registered `assistant-agent` Card the Verifiers judge.
    agent: CardRef,
}

impl Assistant {
    /// Start the judge and the server, deploy the judge model on the gateway,
    /// then register the latency baseline, the latency Model, and the
    /// assistant, and hydrate the assistant.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    pub(crate) async fn start() -> Self {
        let judge = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-judge",
                "object": "chat.completion",
                "created": 1,
                "model": "gpt-4o",
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
        configure_gateway(&deployment.admin()).await;
        Gateway::with_client(deployment.admin())
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
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
            answer_is_yes: registered(&assistant, "answer-is-yes"),
            answer_check: registered(&assistant, "answer-check"),
            answer_graded: registered(&assistant, "answer-graded"),
            agent: registered(&assistant, "assistant-agent"),
        }
    }

    /// The hydrated assistant, created with the Service's own client and
    /// started with Bifrost.
    ///
    /// The Service's key comes from the `issue_key` CLI function with no Role
    /// granted beyond its default `workload`, so Bifrost startup, every
    /// verify, and every judge's gateway call run with exactly what a newly
    /// registered Service holds.
    ///
    /// # Panics
    /// Panics when the key is not issued, the bundle does not load, or
    /// Bifrost does not start.
    pub(crate) async fn state(&self) -> WyrdState {
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

    /// The principal id of the assistant Service, the identity every
    /// [`Self::state`] and [`Self::verify_over_http`] judgment is requested as.
    ///
    /// # Panics
    /// Panics when the CLI refuses to issue the Service key.
    async fn principal(&self) -> String {
        let [kind, name, version, space] = coordinates(&self.service);
        cli::issue_key(
            &kind,
            &name,
            &version,
            &space,
            None,
            None,
            Some(self.deployment.admin()),
        )
        .await
        .expect("the CLI issues the Service key")
        .principal_id
        .to_string()
    }

    /// Publish every captured row, then read each `vala.gateway.calls` row's
    /// id, requesting principal, and Run and Card attribution.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn judge_calls(&self) -> Vec<JudgeCall> {
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        Bifrost::connect(&self.deployment.admin())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT call_id, caller_principal_id, run_id, card_uid FROM vala.gateway.calls",
                &[],
            )
            .await
            .expect("judge calls read back")
    }

    /// Read the result `execution_id` wrote with the judge calls it joins on
    /// its subject Card: the result's requesting principal and Run beside
    /// each joined call's principal and Run. Rows are already published by
    /// [`Self::judge_calls`].
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn result_with_its_judge_calls(&self, execution_id: &str) -> Vec<ResultJudgeCall> {
        Bifrost::connect(&self.deployment.admin())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT r.requested_by_principal_id AS requester, r.run_id AS result_run, \
                        c.caller_principal_id AS caller, c.run_id AS call_run \
                   FROM vala.verification.results r \
                   JOIN vala.gateway.calls c ON c.card_uid = r.subject_card_uid \
                  WHERE r.result_id = $1",
                &[QueryParam::String(execution_id.to_owned())],
            )
            .await
            .expect("the result joins its judge calls")
    }

    /// Judge `answer` with `answer-graded` over plain HTTP the way an agent
    /// does, as the assistant Service with `traceparent` and no Run, and
    /// return the judgment's `execution_id`.
    ///
    /// # Panics
    /// Panics when no token mints or the judgment is refused.
    async fn verify_over_http(&self, traceparent: &str) -> String {
        let key = self.deployment.service_key(&self.service).await;
        let token = self
            .deployment
            .client(&key)
            .access_token()
            .await
            .expect("access token mints");
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/verification/execute",
                self.deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
            .header("traceparent", traceparent)
            .json(&json!({
                "verifier_card_uid": self.answer_graded.uid,
                "card_uid": self.agent.uid,
                "input": { "kind": "task_context", "context": { "answer": "yes" } },
            }))
            .send()
            .await
            .expect("the server answers");
        let status = response.status().as_u16();
        let judgment: serde_json::Value = response.json().await.expect("the judgment is JSON");
        assert_eq!(status, 200, "{judgment}");
        judgment["execution_id"]
            .as_str()
            .expect("the judgment names its execution")
            .to_owned()
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

/// One judge call's `vala.gateway.calls` identity and attribution.
#[derive(Debug, PartialEq, Deserialize)]
struct JudgeCall {
    /// The call's UUID, which its gateway spans carry as `gateway_call_id`.
    call_id: String,
    /// The principal that requested the judgment.
    caller_principal_id: String,
    /// The application Run the call is attributed to.
    run_id: Option<String>,
    /// The Card UID the call is attributed to.
    card_uid: Option<String>,
}

impl JudgeCall {
    /// This call's gateway span in `timeline`, joined on its call id.
    fn span<'a>(&self, timeline: &'a [SpanRecord]) -> Option<&'a SpanRecord> {
        timeline.iter().find(|span| {
            span.attributes
                .get("gateway_call_id")
                .and_then(serde_json::Value::as_str)
                == Some(self.call_id.as_str())
        })
    }
}

/// One verification result beside one judge call joined to it on the
/// subject Card.
#[derive(Debug, PartialEq, Deserialize)]
struct ResultJudgeCall {
    /// The principal that requested the execution, on the result.
    requester: Option<String>,
    /// The application Run on the result.
    result_run: Option<String>,
    /// The principal that made the judge call.
    caller: String,
    /// The application Run on the judge call.
    call_run: Option<String>,
}

/// The trace identity of a caller's W3C `traceparent`.
const CALLER_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
/// The caller's span identity within [`CALLER_TRACE`].
const CALLER_SPAN: &str = "b7ad6b7169203331";

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

/// A task Verifier's one assertion passes the expected answer and fails any
/// other, with no counts: a single check has nothing to count.
///
/// # Panics
/// Panics when a verify fails or a judgment differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_answer_is_judged_by_its_task_check() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");

    for (answer, verdict) in [
        ("yes", VerificationVerdict::Passed),
        ("no", VerificationVerdict::Failed),
    ] {
        let judgment = run
            .observe()
            .verify("answer-check", &Answer { answer })
            .await
            .expect("the answer is judged");
        assert_eq!(
            (judgment.verdict, judgment.kind, judgment.counts),
            (verdict, VerifierKind::TaskAssertion, None)
        );
        assert_eq!(judgment.verifier.name.as_str(), "answer-check");
    }
    assistant.deployment.shutdown().await;
}

/// A task Verifier's LLM judge grades the answer through the server's
/// provider upstream, here the local mock judge.
///
/// # Panics
/// Panics when the verify fails, the judgment differs, or the judge did not
/// receive the graded answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn graded_answer_passes_the_task_llm_judge() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;

    let judgment = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-graded", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");

    assert_eq!(
        (judgment.verdict, judgment.kind, judgment.counts),
        (
            VerificationVerdict::Passed,
            VerifierKind::TaskLlmJudge,
            None
        )
    );
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

/// A judge Verifier run in real time for a Run calls its model through the
/// gateway as the requesting principal, attributed to that Run and the judged
/// Card, so the call joins the Run's other evidence.
///
/// # Panics
/// Panics when the verify, flush, or read fails or the attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judge_call_is_attributed_to_the_run_and_card() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");

    let judgment = run
        .observe()
        .verify("answer-graded", &Answer { answer: "yes" })
        .await
        .expect("the answer is judged");
    state.shutdown().await.expect("Bifrost stops");

    let calls = assistant.judge_calls().await;
    let principal = assistant.principal().await;
    let run_id = run.run_id().to_string();
    assert_eq!(
        calls
            .iter()
            .map(|call| (
                call.caller_principal_id.as_str(),
                call.run_id.as_deref(),
                call.card_uid.clone()
            ))
            .collect::<Vec<_>>(),
        [(
            principal.as_str(),
            Some(run_id.as_str()),
            judgment.subject.uid.as_ref().map(ToString::to_string)
        )],
        "{calls:?}"
    );
    assert_eq!(
        assistant
            .result_with_its_judge_calls(&judgment.execution_id.to_string())
            .await,
        [ResultJudgeCall {
            requester: Some(principal.clone()),
            result_run: Some(run_id.clone()),
            caller: principal,
            call_run: Some(run_id),
        }]
    );
    assistant.deployment.shutdown().await;
}

/// A judgment requested over HTTP with a caller `traceparent` and no Run
/// judges in process within the caller's trace: the judge call's gateway span,
/// joined on its call id, descends from the caller's span, and the call
/// records the requesting principal and the subject Card with a null Run
/// rather than an invented one. The
/// result records the same principal and joins the call on that Card.
///
/// # Panics
/// Panics when the verify, flush, or read fails or the call differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card() {
    let assistant = Assistant::start().await;

    let execution = assistant
        .verify_over_http(&format!("00-{CALLER_TRACE}-{CALLER_SPAN}-01"))
        .await;

    let principal = assistant.principal().await;
    let calls = assistant.judge_calls().await;
    let [call] = &calls[..] else {
        panic!("exactly one judge call: {calls:?}");
    };
    assert_eq!(
        (
            call.caller_principal_id.as_str(),
            call.run_id.as_deref(),
            call.card_uid.clone()
        ),
        (
            principal.as_str(),
            None,
            assistant.agent.uid.as_ref().map(ToString::to_string)
        )
    );
    let timeline = assistant.deployment.timeline(CALLER_TRACE).await;
    let span = call
        .span(&timeline)
        .expect("the judge call joins the caller's trace");
    let chain = ancestry(&timeline, span);
    assert_eq!(
        chain
            .last()
            .and_then(|root| root.parent_span_id)
            .map(|parent| parent.to_hex())
            .as_deref(),
        Some(CALLER_SPAN),
        "{chain:#?}"
    );
    assert_eq!(
        assistant.result_with_its_judge_calls(&execution).await,
        [ResultJudgeCall {
            requester: Some(principal.clone()),
            result_run: None,
            caller: principal,
            call_run: None,
        }]
    );
    assistant.deployment.shutdown().await;
}

/// A Run's verify made inside a `tracing` span that carries an OpenTelemetry
/// context sends that context with the request, so the judge's gateway span
/// joins the caller's trace. The OpenTelemetry layer is scoped to the verify
/// so the journey does not contend for the process-global subscriber a
/// `WYRD_LOG` test server installs.
///
/// # Panics
/// Panics when the verify, flush, or read fails or the judge call is in
/// another trace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn traced_sdk_verify_joins_the_callers_trace() {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
    };
    use tracing::Instrument as _;
    use tracing::instrument::WithSubscriber as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    use tracing_subscriber::layer::SubscriberExt as _;

    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");
    let traced = tracing::Dispatch::new(
        tracing_subscriber::registry().with(
            tracing_opentelemetry::layer().with_tracer(
                opentelemetry_sdk::trace::SdkTracerProvider::builder()
                    .build()
                    .tracer("journey"),
            ),
        ),
    );
    let caller = tracing::dispatcher::with_default(&traced, || tracing::info_span!("caller"));
    caller
        .set_parent(
            opentelemetry::Context::new().with_remote_span_context(SpanContext::new(
                TraceId::from_hex(CALLER_TRACE).expect("trace id"),
                SpanId::from_hex(CALLER_SPAN).expect("span id"),
                TraceFlags::SAMPLED,
                true,
                TraceState::default(),
            )),
        )
        .expect("the caller span joins the caller's trace");

    run.observe()
        .verify("answer-graded", &Answer { answer: "yes" })
        .instrument(caller)
        .with_subscriber(traced)
        .await
        .expect("the answer is judged");
    state.shutdown().await.expect("Bifrost stops");

    let calls = assistant.judge_calls().await;
    let timeline = assistant.deployment.timeline(CALLER_TRACE).await;
    assert!(
        calls.len() == 1 && calls[0].span(&timeline).is_some(),
        "{calls:?} in {timeline:#?}"
    );
    assistant.deployment.shutdown().await;
}

/// A malformed `traceparent` is ignored: the judgment succeeds and its judge
/// call keeps a trace of its own.
///
/// # Panics
/// Panics when the verify, flush, or read fails or the call joined a trace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn malformed_traceparent_is_ignored_by_the_judge_call() {
    let assistant = Assistant::start().await;

    assistant
        .verify_over_http(&format!("00-{CALLER_TRACE}-{CALLER_SPAN}"))
        .await;

    let calls = assistant.judge_calls().await;
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        assistant.deployment.timeline(CALLER_TRACE).await.is_empty(),
        "nothing joined the caller's trace"
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

/// A principal holding only `evals:run`, or `verifier:run` on another
/// Verifier, cannot verify with `answer-check`: neither grant names it, and
/// the binding is no grant.
///
/// # Panics
/// Panics when a verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_the_verifier_grant_is_refused() {
    let assistant = Assistant::start().await;
    let evals_only = assistant
        .deployment
        .scoped_key("evals_only", &["evals:run"])
        .await;
    let other_verifier = assistant
        .deployment
        .verifier_key("other_verifier", &assistant.answer_is_yes)
        .await;

    for key in [evals_only, other_verifier] {
        let refused = assistant
            .state_as(assistant.deployment.client(&key))
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .verify("answer-check", &Answer { answer: "yes" })
            .await
            .expect_err("no grant names answer-check");
        assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    }
    assistant.deployment.shutdown().await;
}

/// A Card-bound principal granted `verifier:run` on exactly `answer-check`
/// verifies with it, whatever Card it is bound to, and with nothing else.
///
/// # Panics
/// Panics when the granted verify is refused or the other one is not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn exact_verifier_grant_verifies_only_that_verifier() {
    let assistant = Assistant::start().await;
    let key = assistant
        .deployment
        .verifier_key("exact_verifier", &assistant.answer_check)
        .await;
    let state = assistant.state_as(assistant.deployment.client(&key));
    let run = state.run_for_card("agent").expect("agent run opens");

    let judgment = run
        .observe()
        .verify("answer-check", &Answer { answer: "yes" })
        .await
        .expect("the granted Verifier judges");
    assert!(judgment.passed());
    let refused = run
        .observe()
        .verify("answer-is-yes", &Answer { answer: "yes" })
        .await
        .expect_err("the grant names one Verifier");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assistant.deployment.shutdown().await;
}

/// Another tenant's administrator cannot verify this tenant's assistant with
/// either its Eval or its task Verifier.
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
    let run = state.run_for_card("agent").expect("agent run opens");

    for verifier in ["answer-is-yes", "answer-check"] {
        let refused = run
            .observe()
            .verify(verifier, &Answer { answer: "yes" })
            .await
            .expect_err("another tenant cannot verify");
        assert_eq!(
            refused.code(),
            "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
            "{verifier}"
        );
    }
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
