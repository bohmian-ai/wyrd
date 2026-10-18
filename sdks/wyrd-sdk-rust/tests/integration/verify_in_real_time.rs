//! An assistant judges its own behavior while it runs: `observe().verify`
//! returns the `VerificationResult` of the Verifier bound to the observed
//! Card, with its verdict and per-requirement breakdown, and `submit` queues
//! the same judgment in the background or records drift samples for the
//! Verifier's schedule. Only `verifier:run` on the named Verifier authorizes
//! the call; an Agent check then calls the gateway as that same caller.

use std::collections::BTreeSet;
use std::time::Duration;
use wyrd_ai::bifrost::BifrostOptions;

use opentelemetry::trace::TraceContextExt as _;
use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::auth::Principals;
use wyrd_ai::bifrost::QueryParam;
use wyrd_ai::cards::{CardKind, CardRef, CardSelector, Cards, RegistrationReceipt};
use wyrd_ai::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Input, Inputs, Submission};
use wyrd_ai::{
    Bifrost, CallProvenance, Citation, ExecutionId, Gateway, GroupId, Locator, RequirementOutcome,
    SpanRecord, VerificationVerdict, WyrdClient, WyrdError, cli,
};
use wyrd_spec::ids::BindingId;
use wyrd_spec::verification::execution::VerificationExecutionState;
use wyrd_spec::verification::{VerificationExecutionStatus, VerifierReadiness};
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

use crate::action_timeline::service_telemetry;
use crate::local_development::configure_gateway;
use crate::support::{Deployment, ancestry, coordinates, fixture, hydrate, register, registered};
use crate::verifier_building_blocks::{completion, requirement, tool_call};

/// How long a test waits for a queued execution or a scheduled result.
const WAIT: Duration = Duration::from_secs(90);

/// The Verdict the local judge returns for every answer: passed, citing the
/// whole answer.
const AGREES: &str = r#"{"outcome":"passed","reason":"it agrees","citations":[{"slot":"answer","locator":{"json_pointer":""}}]}"#;

/// The assistant deployment: a server running the verification runtime whose
/// gateway deploys `gpt-4o` on a local judge that passes every answer, with
/// the assistant Service registered and hydrated. The gateway captures call
/// metadata, so every judge call lands in Bifrost.
pub(crate) struct Assistant {
    /// The deployment.
    pub(crate) deployment: Deployment,
    /// The local judge the `answer-judged` Agent check calls.
    judge: MockServer,
    /// The hydrated assistant bundle.
    bundle: TempDir,
    /// The registered assistant Service.
    pub(crate) service: CardRef,
    /// The registered `answer-check` assertion Verifier.
    answer_check: CardRef,
    /// The registered `answer-judged` Agent-check Verifier.
    answer_judged: CardRef,
    /// The registered `assistant-agent` Card the Verifiers judge.
    agent: CardRef,
}

/// `builder` running the verification runtime, its gateway provider root
/// at the local `judge`.
///
/// # Panics
/// Panics when the judge URL does not parse.
fn on_judge(builder: WyrdTestServerBuilder, judge: &MockServer) -> WyrdTestServerBuilder {
    builder
        .with_verification_runtime_for_test()
        .with_gateway_provider_root_for_test(judge.uri().parse().expect("judge URL parses"))
}

impl Assistant {
    /// Start the judge and the server, deploy the judge model on the gateway,
    /// then register the latency baseline, the latency Model, and the
    /// assistant, and hydrate the assistant.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    pub(crate) async fn start() -> Self {
        Box::pin(Self::start_with(WyrdTestServer::builder(), Duration::ZERO)).await
    }

    /// [`Self::start`] on a server `builder` configures, which also runs
    /// the verification runtime on the local judge, with the judge answering
    /// each call after `judge_delay`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start_with(builder: WyrdTestServerBuilder, judge_delay: Duration) -> Self {
        let judge = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(judge_delay)
                    .set_body_json(json!({
                        "id": "chatcmpl-judge",
                        "object": "chat.completion",
                        "created": 1,
                        "model": "gpt-4o",
                        "choices": [{
                            "index": 0,
                            "finish_reason": "stop",
                            "message": { "role": "assistant", "content": AGREES },
                        }],
                        "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 },
                    })),
            )
            .mount(&judge)
            .await;
        let deployment = Deployment::start_with(on_judge(builder, &judge)).await;
        configure_gateway(&deployment.admin()).await;
        Gateway::connect_with(&deployment.admin())
            .await
            .expect("handle connects")
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
        let cards = deployment.cards().await;
        register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
        register(&cards, "cards/verify_in_real_time/latency-model.yaml").await;
        let assistant = register(&cards, "cards/verify_in_real_time/assistant.yaml").await;
        let bundle = hydrate(&cards, &assistant.root).await;
        Self {
            deployment,
            judge,
            bundle,
            service: assistant.root.clone(),
            answer_check: registered(&assistant, "answer-check"),
            answer_judged: registered(&assistant, "answer-judged"),
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

    /// The assistant registered and hydrated in a new tenant `slug`, loaded
    /// as that tenant's assistant Service with Bifrost started, beside the
    /// bundle directory it was loaded from.
    ///
    /// # Panics
    /// Panics when the tenant, a registration, the hydration, the key, or
    /// Bifrost startup fails.
    async fn in_tenant(&self, slug: &str) -> (WyrdState, TempDir) {
        let admin = self
            .deployment
            .client(&self.deployment.other_tenant_admin(slug).await);
        let cards = Cards::connect_with(&admin).await.expect("handle connects");
        register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
        register(&cards, "cards/verify_in_real_time/latency-model.yaml").await;
        let assistant = register(&cards, "cards/verify_in_real_time/assistant.yaml").await;
        let bundle = hydrate(&cards, &assistant.root).await;
        let [kind, name, version, space] = coordinates(&assistant.root);
        let key = cli::issue_key(&kind, &name, &version, &space, None, None, Some(admin))
            .await
            .expect("the CLI issues the Service key");
        let state = WyrdState::from_path_with(
            bundle.path().join("bundle"),
            self.deployment.client(key.key.expose()),
        )
        .expect("bundle loads offline");
        state.start_bifrost().await.expect("Bifrost starts");
        (state, bundle)
    }

    /// Wait until the judge has received `calls` requests, so that many
    /// judged executions are in flight.
    ///
    /// # Panics
    /// Panics when the judge stops recording or the calls never arrive.
    async fn await_judge_calls(&self, calls: usize) {
        let deadline = std::time::Instant::now() + WAIT;
        while self
            .judge
            .received_requests()
            .await
            .expect("the judge records requests")
            .len()
            < calls
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the judge never received {calls} calls"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// The hydrated assistant, loaded with its server calls fixed to
    /// `client` and Bifrost not started.
    ///
    /// # Panics
    /// Panics when the bundle does not load.
    fn state_as(&self, client: WyrdClient) -> WyrdState {
        WyrdState::from_path_with(self.bundle.path().join("bundle"), client)
            .expect("bundle loads offline")
    }

    /// The principal id of the Service `service`; for the assistant, the
    /// identity every [`Self::state`] and [`Self::verify_over_http`]
    /// judgment is requested as.
    ///
    /// # Panics
    /// Panics when the CLI refuses to issue the Service key.
    async fn principal(&self, service: &CardRef) -> String {
        let [kind, name, version, space] = coordinates(service);
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
    /// id, subject/actor pair, and Run and Card attribution.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn judge_calls(&self) -> Vec<JudgeCall> {
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        Bifrost::connect_with(&self.deployment.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT call_id, sub_principal_id, act_principal_id, run_id, card_uid \
                 FROM vala.gateway.calls",
                &[],
            )
            .await
            .expect("judge calls read back")
    }

    /// Read the result `execution_id` wrote with the judge calls it joins on
    /// its verified Card: the result's subject, actor, and Run beside
    /// each joined call's subject, actor, and Run. Rows are already published by
    /// [`Self::judge_calls`].
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn result_with_its_judge_calls(&self, execution_id: &str) -> Vec<ResultJudgeCall> {
        Bifrost::connect_with(&self.deployment.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT r.sub_principal_id AS result_subject, \
                        r.act_principal_id AS result_actor, r.run_id AS result_run, \
                        c.sub_principal_id AS call_subject, \
                        c.act_principal_id AS call_actor, c.run_id AS call_run \
                   FROM vala.verification.results r \
                   JOIN vala.gateway.calls c ON c.card_uid = r.card_uid \
                  WHERE r.execution_id = $1",
                &[QueryParam::String(execution_id.to_owned())],
            )
            .await
            .expect("the result joins its judge calls")
    }

    /// Judge the answer `yes` with `answer-judged` over plain HTTP the way an
    /// agent does, as the assistant Service with `traceparent` and no Run,
    /// and return the result's `execution_id`.
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
                "verifier_card_uid": self.answer_judged.uid,
                "card_uid": self.agent.uid,
                "inputs": { "answer": { "kind": "json", "value": "yes" } },
            }))
            .send()
            .await
            .expect("the server answers");
        let status = response.status().as_u16();
        let result: serde_json::Value = response.json().await.expect("the result is JSON");
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["outcome"]["verdict"], "passed", "{result}");
        result["execution_id"]
            .as_str()
            .expect("the result names its execution")
            .to_owned()
    }
}

/// One judge call's `vala.gateway.calls` identity and attribution.
#[derive(Debug, PartialEq, Deserialize)]
struct JudgeCall {
    /// The call's UUID, which its gateway spans carry as `gateway_call_id`.
    call_id: String,
    /// The principal the judgment was for.
    sub_principal_id: String,
    /// The principal that made the call: the tenant SYSTEM executor.
    act_principal_id: String,
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
    /// The principal the result was for.
    result_subject: String,
    /// The principal that wrote the result.
    result_actor: String,
    /// The application Run on the result.
    result_run: Option<String>,
    /// The principal the judge call was for.
    call_subject: String,
    /// The principal that made the judge call.
    call_actor: String,
    /// The application Run on the judge call.
    call_run: Option<String>,
}

impl ResultJudgeCall {
    /// Assert `joined` is exactly one result and judge call, both for
    /// `principal` on `run`: the direct verify's result is performed by
    /// `principal` itself, and the judge call inside it by SYSTEM.
    ///
    /// # Panics
    /// Panics when the rows differ.
    fn assert_called_as_system(joined: &[Self], principal: &str, run: Option<&str>) {
        let [row] = joined else {
            panic!("exactly one joined judge call: {joined:?}");
        };
        assert_eq!(
            (
                row.result_subject.as_str(),
                row.call_subject.as_str(),
                row.result_run.as_deref(),
                row.call_run.as_deref(),
            ),
            (principal, principal, run, run),
            "{row:?}"
        );
        assert_eq!(
            row.result_actor, principal,
            "the caller performs its direct verify: {row:?}"
        );
        assert_ne!(
            row.call_actor, principal,
            "SYSTEM performs the judgment: {row:?}"
        );
    }
}

/// The trace identity of a caller's W3C `traceparent`.
const CALLER_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
/// The caller's span identity within [`CALLER_TRACE`].
const CALLER_SPAN: &str = "b7ad6b7169203331";

/// An answer the assertion expects passes, judged against the agent within
/// the caller's Run.
///
/// # Panics
/// Panics when the verify fails or the result differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_answer_passes_its_verifier() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");

    let result = run
        .observe()
        .verify("answer-check", json!("yes"))
        .await
        .expect("the answer is judged");

    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    assert_eq!(result.verifier.name.as_str(), "answer-check");
    assert_eq!(&result.subject, state.card_ref("agent").expect("agent"));
    assert_eq!(result.run_id.as_ref(), Some(run.run_id()));
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    assert_eq!(
        (requirement.id.as_str(), requirement.outcome),
        ("says-yes", RequirementOutcome::Passed)
    );
    assistant.deployment.shutdown().await;
}

/// An answer the assertion does not expect fails, and the failed requirement
/// carries the compared value; a failed verdict is a result, not an error.
///
/// # Panics
/// Panics when the verify fails or the result differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_answer_fails_its_verifier() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;

    let result = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-check", json!("no"))
        .await
        .expect("the answer is judged");

    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    assert_eq!(
        (requirement.outcome, requirement.actual.as_ref()),
        (RequirementOutcome::Failed, Some(&json!("no")))
    );
    assistant.deployment.shutdown().await;
}

/// The tool-less judge Agent of `answer-judged` judges the bound answer
/// through the server's provider upstream, here the local mock judge, and
/// its cited Verdict passes the requirement with the Agent's provenance.
///
/// # Panics
/// Panics when the verify fails, the result differs, or the judge did not
/// receive the claim and the answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judged_answer_passes_the_agent_check() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;

    let result = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("answer-judged", json!("yes"))
        .await
        .expect("the answer is judged");

    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    assert_eq!(requirement.outcome, RequirementOutcome::Passed);
    assert_eq!(
        requirement.citations,
        [Citation {
            slot: "answer".parse().expect("slot name"),
            locator: Locator::JsonPointer(String::new()),
        }]
    );
    assert!(
        matches!(
            &requirement.provenance,
            Some(CallProvenance::Agent { agent: Some(agent), .. }) if agent.name.as_str() == "judge-agent"
        ),
        "{:?}",
        requirement.provenance
    );
    let judged = assistant
        .judge
        .received_requests()
        .await
        .expect("the judge records requests");
    assert_eq!(judged.len(), 1);
    let body = String::from_utf8_lossy(&judged[0].body);
    assert!(
        body.contains("Claim: The answer agrees with the customer"),
        "the judge received the requirement statement: {body}"
    );
    assistant.deployment.shutdown().await;
}

/// An Agent check run in real time for a Run calls its model through the
/// gateway for the requesting principal, performed by the tenant SYSTEM
/// principal, attributed to that Run and the judged Card, so the call joins
/// the Run's other evidence.
///
/// # Panics
/// Panics when the verify, flush, or read fails or the attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judge_call_is_attributed_to_the_run_and_card() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");

    let result = run
        .observe()
        .verify("answer-judged", json!("yes"))
        .await
        .expect("the answer is judged");
    state.shutdown().await.expect("Bifrost stops");

    let calls = assistant.judge_calls().await;
    let principal = assistant.principal(&assistant.service).await;
    let run_id = run.run_id().to_string();
    assert!(
        calls.iter().all(|call| call.act_principal_id != principal),
        "SYSTEM performs every judge call: {calls:?}"
    );
    assert_eq!(
        calls
            .iter()
            .map(|call| (
                call.sub_principal_id.as_str(),
                call.run_id.as_deref(),
                call.card_uid.clone()
            ))
            .collect::<Vec<_>>(),
        [(
            principal.as_str(),
            Some(run_id.as_str()),
            result.subject.uid.as_ref().map(ToString::to_string)
        )],
        "{calls:?}"
    );
    ResultJudgeCall::assert_called_as_system(
        &assistant
            .result_with_its_judge_calls(&result.execution_id.to_string())
            .await,
        &principal,
        Some(&run_id),
    );
    assistant.deployment.shutdown().await;
}

/// A judgment requested over HTTP with a caller `traceparent` and no Run
/// judges in process within the caller's trace: the judge call's gateway span,
/// joined on its call id, descends from the caller's span, and the call
/// records the requesting principal as subject, SYSTEM as actor, and the
/// verified Card with a null Run rather than an invented one. The result
/// records the requesting principal as both subject and actor, since the
/// verify itself is direct work, and joins the call on that Card.
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

    let principal = assistant.principal(&assistant.service).await;
    let calls = assistant.judge_calls().await;
    let [call] = &calls[..] else {
        panic!("exactly one judge call: {calls:?}");
    };
    assert_eq!(
        (
            call.sub_principal_id.as_str(),
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
    ResultJudgeCall::assert_called_as_system(
        &assistant.result_with_its_judge_calls(&execution).await,
        &principal,
        None,
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
        .verify("answer-judged", json!("yes"))
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

/// A Run's `submit` made inside a sampled caller span queues its judgment,
/// and the background attempt is a trace of its own that links back to the
/// caller's span: the caller's timeline returns that linked
/// `verification.attempt` and, below it in the attempt's trace, the judge
/// call's gateway span.
///
/// # Panics
/// Panics when the submission is not queued, the wait, flush, or read fails,
/// or the attempt or its judge call is missing from the caller's timeline.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn traced_sdk_submit_links_its_attempt_to_the_callers_trace() {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
    };
    use tracing::Instrument as _;
    use tracing::instrument::WithSubscriber as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    use tracing_subscriber::layer::SubscriberExt as _;

    const SUBMIT_TRACE: &str = "1af7651916cd43dd8448eb211c80319c";
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
                TraceId::from_hex(SUBMIT_TRACE).expect("trace id"),
                SpanId::from_hex(CALLER_SPAN).expect("span id"),
                TraceFlags::SAMPLED,
                true,
                TraceState::default(),
            )),
        )
        .expect("the caller span joins the caller's trace");

    let Submission::Queued(execution) = run
        .observe()
        .submit("answer-judged", json!("yes"))
        .instrument(caller)
        .with_subscriber(traced)
        .await
        .expect("the answer is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    execution.wait(WAIT).await.expect("the execution settles");
    state.shutdown().await.expect("Bifrost stops");

    let calls = assistant.judge_calls().await;
    let timeline = assistant.deployment.timeline(SUBMIT_TRACE).await;
    let attempt = timeline
        .iter()
        .find(|span| span.name == "verification.attempt")
        .unwrap_or_else(|| panic!("the caller's timeline links the attempt: {timeline:#?}"));
    assert_ne!(attempt.trace_id.to_hex(), SUBMIT_TRACE, "{attempt:#?}");
    assert!(
        attempt
            .links
            .iter()
            .any(|link| link.trace_id.to_hex() == SUBMIT_TRACE),
        "the attempt links to the caller's trace: {attempt:#?}"
    );
    let [call] = calls.as_slice() else {
        panic!("exactly one judge call: {calls:?}");
    };
    let judged = call
        .span(&timeline)
        .unwrap_or_else(|| panic!("the judge call is in the timeline: {timeline:#?}"));
    assert_eq!(
        ancestry(&timeline, judged).last().map(|root| root.span_id),
        Some(attempt.span_id),
        "the judge call descends from the attempt: {timeline:#?}"
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

/// `submit` queues the judgment and returns its execution handle at once;
/// `wait` returns the failed result, and a handle rebuilt from the
/// execution ID alone, as another process would, reads the same result.
///
/// # Panics
/// Panics when the submission is not queued, a wait fails, or the results
/// differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn submitted_answer_is_judged_in_the_background_and_resumes_by_id() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");

    let Submission::Queued(execution) = run
        .observe()
        .submit("answer-check", json!("no"))
        .await
        .expect("the answer is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    let waited = execution.wait(WAIT).await.expect("the execution settles");
    let key = assistant.deployment.service_key(&assistant.service).await;
    let resumed = Execution::connect_with(*execution.id(), &assistant.deployment.client(&key))
        .await
        .expect("the handle rebuilds from its ID")
        .wait(WAIT)
        .await
        .expect("the resumed handle reads the result");

    assert_eq!(waited.outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(&waited.execution_id, execution.id());
    assert_eq!(waited.run_id.as_ref(), Some(run.run_id()));
    assert_eq!(resumed, waited);
    assistant.deployment.shutdown().await;
}

/// Another tenant cannot read a queued execution by its ID: the status read
/// is refused as not found until the wait gives up.
///
/// # Panics
/// Panics when the foreign wait returns a result or another error.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn another_tenant_cannot_resume_an_execution() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let Submission::Queued(execution) = state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .submit("answer-check", json!("yes"))
        .await
        .expect("the answer is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    execution.wait(WAIT).await.expect("the execution settles");
    let foreign = assistant
        .deployment
        .other_tenant_admin("other-tenant")
        .await;

    let refused = Execution::connect_with(*execution.id(), &assistant.deployment.client(&foreign))
        .await
        .expect("the handle rebuilds from its ID")
        .wait(Duration::from_secs(2))
        .await
        .expect_err("another tenant sees no execution");

    assert_eq!(refused.code(), "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT");
    assistant.deployment.shutdown().await;
}

/// A drift Verifier judges a scheduled window, so `verify` refuses it before
/// any call while `submit` records the samples for the schedule.
///
/// # Panics
/// Panics when the verify is not refused with the stable code or the
/// submission is not recorded.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn window_verifier_records_samples_and_refuses_verify() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("model").expect("model run opens");
    let observe = run.observe();

    let refused = observe
        .verify("latency-drift", json!({ "latency": 12.5 }))
        .await
        .expect_err("a window Verifier is judged on its schedule");
    let submitted = observe
        .submit("latency-drift", json!({ "latency": 12.5 }))
        .await
        .expect("the sample is recorded");

    assert_eq!(refused.code(), "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED");
    assert!(matches!(submitted, Submission::Recorded), "{submitted:?}");
    assistant.deployment.shutdown().await;
}

/// An SPC drift Verifier whose baseline can never fit is skipped when its
/// schedule comes due: the scheduler advances the cursor without admitting
/// an execution, so nothing is judged and no result is retained, and the
/// binding reports the baseline as not ready.
///
/// # Panics
/// Panics when a step fails, the occurrence is not skipped in time, or the
/// binding admitted an execution.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unfitted_baseline_is_skipped_on_schedule() {
    let assistant = Assistant::start().await;
    let cards = assistant.deployment.cards().await;
    let unfitted = register(&cards, "cards/verify_in_real_time/unfitted-assistant.yaml").await;
    let bundle = hydrate(&cards, &unfitted.root).await;
    let key = assistant.deployment.service_key(&unfitted.root).await;
    let state = WyrdState::from_path_with(
        bundle.path().join("bundle"),
        assistant.deployment.client(&key),
    )
    .expect("bundle loads");
    state.start_bifrost().await.expect("Bifrost starts");
    let run = state.run_for_card("model").expect("model run opens");
    let observe = run.observe();
    for _ in 0..5 {
        observe
            .submit("tier-drift", json!("gold"))
            .await
            .expect("the tier is recorded");
    }
    state.shutdown().await.expect("samples drain");
    let binding = assistant.deployment.binding(&unfitted.root).await;
    assistant
        .deployment
        .server()
        .make_binding_due(binding)
        .await
        .expect("the binding comes due");
    let due = assistant
        .deployment
        .binding_status(&binding)
        .await
        .next_run_at;

    let deadline = tokio::time::Instant::now() + WAIT;
    let status = loop {
        let status = assistant.deployment.binding_status(&binding).await;
        if status.next_run_at > due {
            break status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the due occurrence was not scheduled within {WAIT:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    assert_eq!(status.readiness, VerifierReadiness::BaselineNotReady);
    assert_eq!(status.last_execution_id, None, "no execution is admitted");
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
        .verify("not-bound-here", json!("yes"))
        .await
        .expect_err("an unbound Verifier is refused");

    assert_eq!(refused.code(), "WYRD_SDK_404_UNKNOWN_VERIFIER");
    assistant.deployment.shutdown().await;
}

/// A value for a slot the Verifier does not declare, or a file for its `json`
/// slot, fails before any call.
///
/// # Panics
/// Panics when a verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn input_that_does_not_fit_its_slot_fails_locally() {
    let assistant = Assistant::start().await;
    let state = assistant.state().await;
    let run = state.run_for_card("agent").expect("agent run opens");
    let observe = run.observe();

    for inputs in [
        Inputs::named([("reply", json!("yes"))]),
        Inputs::named([("answer", File::bytes(b"yes".to_vec()))]),
    ] {
        let refused = observe
            .verify("answer-check", inputs)
            .await
            .expect_err("the input does not fit");
        assert_eq!(refused.code(), "WYRD_VERIFICATION_400_INPUT_INVALID");
    }
    assistant.deployment.shutdown().await;
}

/// A principal holding only an unrelated `operators:invoke`, or
/// `verifier:run` on another Verifier, cannot verify with `answer-check`:
/// neither grant names it, and the binding is no grant.
///
/// # Panics
/// Panics when a verify succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_the_verifier_grant_is_refused() {
    let assistant = Assistant::start().await;
    let operators_only = assistant
        .deployment
        .scoped_key("operators_only", &["operators:invoke"])
        .await;
    let other_verifier = assistant
        .deployment
        .verifier_key("other_verifier", &assistant.answer_judged)
        .await;

    for key in [operators_only, other_verifier] {
        let refused = assistant
            .state_as(assistant.deployment.client(&key))
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .verify("answer-check", json!("yes"))
            .await
            .expect_err("no grant names answer-check");
        assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    }
    assistant.deployment.shutdown().await;
}

/// The retired verification contracts are unknown at the server: a principal
/// whose stored role still grants `evals:run` or `evals:invoke` is refused by
/// issuance before anything runs — the token endpoint's ordinary RFC 6749
/// `server_error` for a role that no longer decodes — and the retired Eval
/// tables are listed nowhere and refused for describe, read, and write exactly
/// as a never-existing namespace is, while the current verification table
/// still answers.
///
/// # Panics
/// Panics when a retired contract is accepted, a refusal carries another
/// code, or the current table does not answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn retired_verification_contracts_are_refused() {
    let assistant = Assistant::start().await;
    for action in ["run", "invoke"] {
        let stale = assistant
            .deployment
            .granted_key(
                &format!("evals_{action}"),
                &[json!({ "resource": "evals", "action": action, "scope": "all" })],
            )
            .await;
        let refused = assistant
            .state_as(assistant.deployment.client(&stale))
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .verify("answer-check", json!("yes"))
            .await
            .expect_err("a stale evals grant is no grant");
        assert_eq!(refused.code(), "WYRD_SPEC_500_INTERNAL", "{action}");
    }

    let bifrost = Bifrost::connect_with(&assistant.deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let listed = bifrost.list_tables().await.expect("tables list");
    assert!(
        listed.iter().all(|table| table.namespace != "vala.eval"),
        "retired tables are listed: {listed:?}"
    );
    // A retired name is refused exactly as a namespace that never existed.
    let refusals = async |namespace: &str, name: &str| {
        let fqn = format!("{namespace}.{name}");
        [
            bifrost
                .sql(&format!("SELECT * FROM {fqn}"), &[])
                .await
                .map(drop),
            bifrost.describe_table(namespace, name).await.map(drop),
            bifrost.writer_table(&fqn).await.map(drop),
        ]
        .map(|outcome| outcome.map_err(|error| WyrdError::from(error).code().to_owned()))
    };
    for name in ["result_items", "observations"] {
        let retired = refusals("vala.eval", name).await;
        assert!(retired.iter().all(Result::is_err), "{name}: {retired:?}");
        assert_eq!(retired, refusals("vala.absent", name).await, "{name}");
    }
    bifrost
        .sql("SELECT COUNT(*) AS n FROM vala.verification.results", &[])
        .await
        .expect("the current verification table answers");
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

    let result = run
        .observe()
        .verify("answer-check", json!("yes"))
        .await
        .expect("the granted Verifier judges");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    let refused = run
        .observe()
        .verify("answer-judged", json!("yes"))
        .await
        .expect_err("the grant names one Verifier");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assistant.deployment.shutdown().await;
}

/// Another tenant's administrator cannot verify this tenant's assistant with
/// either its assertion or its Agent-check Verifier.
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

    for verifier in ["answer-check", "answer-judged"] {
        let refused = run
            .observe()
            .verify(verifier, json!("yes"))
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

/// A registered Service whose component binds a `group_end` Verifier, with
/// its hydrated bundle loaded as the Service itself.
struct Grouped {
    /// The Service registration, naming every Card it registered.
    receipt: RegistrationReceipt,
    /// The hydrated bundle the Service state and any other caller load.
    bundle: TempDir,
    /// The Service's API key.
    key: String,
    /// The Service's state, started with Bifrost.
    state: WyrdState,
    /// The Service's `group_end` binding.
    binding: BindingId,
}

impl Assistant {
    /// Register and hydrate the Service fixture at `relative`, start its
    /// state with the Service's own key, and find the binding whose status
    /// pins `group_end` settings.
    ///
    /// # Panics
    /// Panics when a setup step fails or the Service has no `group_end`
    /// binding.
    async fn grouped(&self, relative: &str) -> Grouped {
        let cards = self.deployment.cards().await;
        let receipt = register(&cards, relative).await;
        let bundle = hydrate(&cards, &receipt.root).await;
        let key = self.deployment.service_key(&receipt.root).await;
        let state =
            WyrdState::from_path_with(bundle.path().join("bundle"), self.deployment.client(&key))
                .expect("bundle loads");
        state.start_bifrost().await.expect("Bifrost starts");
        let ids = cards
            .get(CardSelector::exact(receipt.root.clone()))
            .await
            .expect("the Service reads")
            .status
            .and_then(|status| status.verification)
            .expect("the Service serves verification status")
            .binding_ids;
        let mut binding = None;
        for id in ids {
            if self
                .deployment
                .binding_status(&id)
                .await
                .group_end
                .is_some()
            {
                binding = Some(id);
            }
        }
        Grouped {
            receipt,
            bundle,
            key,
            state,
            binding: binding.expect("the Service binds a group_end Verifier"),
        }
    }
}

impl Grouped {
    /// Judge `value` with `verifier` in a new Run of the Service's `agent`
    /// under `group`, as the Service.
    ///
    /// # Panics
    /// Panics when the Run does not open or the verify fails.
    async fn turn(&self, verifier: &str, group: &GroupId, value: serde_json::Value) {
        self.state
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .group_id(group.clone())
            .verify(verifier, value)
            .await
            .expect("the turn is judged");
    }

    /// Enqueue `body` over plain HTTP as the Service with `idempotency_key`,
    /// the way an agent replays a submission, and return the accepted
    /// execution.
    ///
    /// # Panics
    /// Panics when no token mints or the submission is not accepted.
    async fn enqueue_over_http(
        &self,
        deployment: &Deployment,
        idempotency_key: &str,
        body: &serde_json::Value,
    ) -> ExecutionId {
        let token = deployment
            .client(&self.key)
            .access_token()
            .await
            .expect("access token mints");
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/verification/enqueue",
                deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
            .header("idempotency-key", idempotency_key)
            .json(body)
            .send()
            .await
            .expect("the server answers");
        let status = response.status().as_u16();
        let accepted: serde_json::Value = response.json().await.expect("the body is JSON");
        assert_eq!(status, 202, "{accepted}");
        accepted["execution_id"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .expect("the response names its execution")
    }
}

/// A support conversation resumed over two Runs, a background submit, and an
/// agent's replayed HTTP enqueue, all sharing one `group_id`, is judged once
/// its `after` period expires: the group execution reads back its group and
/// owner and judges the Service's six turns in call order. The replay counts
/// once; an ungrouped key conflict joins no group, and a same-group verify
/// the administrator made with no owning Service and a turn from a principal
/// without `verifier:run` on the source Verifier are excluded, and the
/// binding status names the execution.
/// Binding the source Verifier itself to `group_end` is refused at
/// registration, naming the binding's `runs_on` field.
///
/// # Panics
/// Panics when a step fails, the group is not judged in time, or the
/// judgment or refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_support_resumes() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    let support = assistant
        .grouped("cards/verify_in_real_time/support.yaml")
        .await;
    let group = GroupId::new("conv-1").expect("group id");
    let turns: Vec<_> = (1..=6)
        .map(|turn| json!({"turn": turn, "answer": format!("answer {turn}")}))
        .collect();
    let first = support
        .state
        .run_for_card("agent")
        .expect("agent run opens");
    for turn in &turns[..3] {
        first
            .observe()
            .group_id(group.clone())
            .verify("turn-check", turn.clone())
            .await
            .expect("the turn is judged");
    }
    let resumed = support
        .state
        .run_for_card("agent")
        .expect("agent run opens");
    resumed
        .observe()
        .group_id(group.clone())
        .verify("turn-check", turns[3].clone())
        .await
        .expect("the resumed turn is judged");
    let Submission::Queued(queued) = resumed
        .observe()
        .group_id(group.clone())
        .submit("turn-check", turns[4].clone())
        .await
        .expect("the turn is submitted")
    else {
        panic!("a turn-check submission queues an execution");
    };
    queued.wait(WAIT).await.expect("the queued turn is judged");
    let body = |turn: &serde_json::Value| {
        json!({
            "verifier_card_uid": registered(&support.receipt, "turn-check").uid,
            "card_uid": registered(&support.receipt, "assistant-agent").uid,
            "group_id": group,
            "inputs": {"turn": {"kind": "json", "value": turn}},
        })
    };
    let replayed = support
        .enqueue_over_http(deployment, "support-turn-0006", &body(&turns[5]))
        .await;
    assert_eq!(
        support
            .enqueue_over_http(deployment, "support-turn-0006", &body(&turns[5]))
            .await,
        replayed,
        "the same key and body replay one execution"
    );
    let conflict = support
        .enqueue_over_http(
            deployment,
            "support-turn-0006",
            &json!({
                "verifier_card_uid": registered(&support.receipt, "turn-check").uid,
                "card_uid": registered(&support.receipt, "assistant-agent").uid,
                "inputs": {"turn": {"kind": "json", "value": {"answer": ""}}},
            }),
        )
        .await;
    let conflicted = Execution::connect_with(conflict, &deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect_err("the key conflict settles errored");
    assert_eq!(conflicted.code(), "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");
    Execution::connect_with(replayed, &deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the enqueued turn is judged");
    WyrdState::from_path_with(support.bundle.path().join("bundle"), deployment.admin())
        .expect("bundle loads")
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .group_id(group.clone())
        .verify("turn-check", json!({"answer": ""}))
        .await
        .expect("an ownerless turn is judged");
    let unauthorized = deployment
        .verifier_key("other_verifier", &assistant.answer_check)
        .await;
    let denied = WyrdState::from_path_with(
        support.bundle.path().join("bundle"),
        deployment.client(&unauthorized),
    )
    .expect("bundle loads")
    .run_for_card("agent")
    .expect("agent run opens")
    .observe()
    .group_id(group.clone())
    .verify("turn-check", json!({"answer": ""}))
    .await
    .expect_err("verifier:run on another Verifier is no grant");
    assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC", "{denied}");
    support.state.shutdown().await.expect("Bifrost stops");

    let judged = deployment
        .group_execution(&support.binding, None, WAIT)
        .await;

    assert_eq!(judged.group_id.as_ref(), Some(&group));
    assert_eq!(judged.owner_card_uid, support.receipt.root.uid);
    let slot = "turns".parse().expect("slot name");
    let snapshot = judged.group.expect("the execution pins its snapshot");
    let answered = "answered".parse().expect("slot name");
    assert_eq!(
        snapshot.sources[&answered].execution_ids, snapshot.sources[&slot].execution_ids,
        "every turn's input and result align: {snapshot:?}"
    );
    assert_eq!(snapshot.sources[&slot].execution_ids.len(), 6);
    assert_eq!(snapshot.sources[&slot].execution_ids[5], replayed);
    let outcome = judged.result.expect("the result is visible").outcome;
    assert_eq!(
        serde_json::to_value(&outcome.evidence[&slot]).expect("evidence encodes"),
        json!({"kind": "json", "value": turns}),
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Passed);
    let status = deployment.binding_status(&support.binding).await;
    assert_eq!(status.last_execution_id, Some(judged.execution_id));
    let refused = deployment
        .cards()
        .await
        .register_from_path(&fixture("cards/verify_in_real_time/support-unsourced.yaml"))
        .await
        .expect_err("a group_end binding needs a Verifier with sources");
    assert_eq!(
        refused.code(),
        "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH",
        "{refused}"
    );
    assert!(
        refused.to_string().contains("verified_by[0].runs_on"),
        "{refused}"
    );
    assistant.deployment.shutdown().await;
}

/// One retained `vala.verification.results` row of a group binding.
#[derive(Debug, Deserialize)]
struct GroupResultRow {
    /// The execution the result settled.
    execution_id: String,
    /// The result's identity, which a verdict-change dispatch names.
    result_id: String,
    /// The result's verdict.
    verdict: String,
}

/// An order conversation's refund promises are reviewed over their whole
/// history each time new acknowledged evidence lets the binding's `after`
/// period expire, with no finality cutoff; the review judges the latest
/// fact. The first review passes and dispatches nothing; a burst of turns
/// ending overdue debounces into one failed review that dispatches its
/// Operator once with the group and the passed result it changed from; a
/// further overdue turn keeps the verdict failed and dispatches nothing; and
/// a corrected paid-in-full fact passes again, dispatching the recovery with
/// the failed result it changed from. Every review stays retained history.
/// A `group_end` binding declaring `on_failure` is refused at registration,
/// naming the field.
///
/// # Panics
/// Panics when a step fails, a review is not judged in time, or a review,
/// dispatch, or refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_refund_changes_verdict() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    let mut hooks = deployment.hooks("refund-hooks", "refund-hook-token").await;
    let refund = assistant
        .grouped("cards/verify_in_real_time/refund.yaml")
        .await;
    let group = GroupId::new("order-1").expect("group id");
    let turn = |overdue: bool| {
        refund.turn(
            "refund-turn",
            &group,
            json!({"order": "A-1", "overdue": overdue}),
        )
    };
    let slot = "facts".parse().expect("slot name");

    turn(false).await;
    turn(false).await;
    let promised = deployment
        .group_execution(&refund.binding, None, WAIT)
        .await;
    for overdue in [false, false, true] {
        turn(overdue).await;
    }
    let overdue = deployment
        .group_execution(&refund.binding, Some(&promised.execution_id), WAIT)
        .await;
    turn(true).await;
    let unchanged = deployment
        .group_execution(&refund.binding, Some(&overdue.execution_id), WAIT)
        .await;
    turn(false).await;
    let recovered = deployment
        .group_execution(&refund.binding, Some(&unchanged.execution_id), WAIT)
        .await;

    let verdicts = [&promised, &overdue, &unchanged, &recovered].map(|review| {
        let result = review.result.as_ref().expect("the result is visible");
        (
            review.group.as_ref().expect("snapshot").sources[&slot]
                .execution_ids
                .len(),
            result.outcome.verdict,
            review.dispatches.len(),
        )
    });
    assert_eq!(
        verdicts,
        [
            (2, VerificationVerdict::Passed, 0),
            (5, VerificationVerdict::Failed, 1),
            (6, VerificationVerdict::Failed, 0),
            (7, VerificationVerdict::Passed, 1),
        ],
        "only the verdict changes dispatch"
    );
    let history: Vec<GroupResultRow> =
        Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT execution_id, result_id, verdict FROM vala.verification.results \
                 WHERE binding_id = $1 ORDER BY ended_at",
                &[QueryParam::String(refund.binding.to_string())],
            )
            .await
            .expect("the reviews read back");
    assert_eq!(
        history
            .iter()
            .map(|row| (row.execution_id.clone(), row.verdict.as_str()))
            .collect::<Vec<_>>(),
        [
            (promised.execution_id.to_string(), "passed"),
            (overdue.execution_id.to_string(), "failed"),
            (unchanged.execution_id.to_string(), "failed"),
            (recovered.execution_id.to_string(), "passed"),
        ],
        "every review stays retained"
    );
    for (changed, previous, verdict, previous_verdict) in
        [(1, 0, "failed", "passed"), (3, 2, "passed", "failed")]
    {
        let alert: serde_json::Value = tokio::time::timeout(WAIT, hooks.received.recv())
            .await
            .expect("the verdict change is delivered")
            .expect("the hook is listening")
            .body_json()
            .expect("the alert is JSON");
        assert_eq!(
            alert,
            json!({
                "execution_id": history[changed].execution_id,
                "result_id": history[changed].result_id,
                "verdict": verdict,
                "group_id": "order-1",
                "previous_result_id": history[previous].result_id,
                "previous_verdict": previous_verdict,
            })
        );
    }
    hooks.assert_quiet(Duration::from_secs(5)).await;
    refund.state.shutdown().await.expect("Bifrost stops");
    let refused = deployment
        .cards()
        .await
        .register_from_path(&fixture("cards/verify_in_real_time/refund-on-failure.yaml"))
        .await
        .expect_err("on_failure on a group_end binding is refused");
    assert_eq!(
        refused.code(),
        "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH",
        "{refused}"
    );
    assert!(
        refused.to_string().contains("verified_by[1].on_failure"),
        "{refused}"
    );
    assistant.deployment.shutdown().await;
}

/// The `coding-review` judge's statement, which every request of its
/// `spans-reviewed` check carries.
const LIVE_REVIEW: &str = "spans were reviewed live";

/// The coding judge's Verdict once it read the spans: passed, citing the
/// session's first trace ID in its original `traces` evidence.
const REVIEWED: &str = r#"{"outcome":"passed","reason":"the spans were read","citations":[{"slot":"traces","locator":{"json_pointer":"/0"}}]}"#;

/// What a Chat Completions request carrying tool results contains.
const TOOL_TURN: &str = r#""role":"tool""#;

/// Every request body the local `judge` received that contains `marker`,
/// in arrival order.
///
/// # Panics
/// Panics when the judge does not record requests.
async fn judged_bodies(judge: &MockServer, marker: &str) -> Vec<String> {
    judge
        .received_requests()
        .await
        .expect("the judge records requests")
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .filter(|body| body.contains(marker))
        .collect()
}

/// The coding review's tool-call answer, held on its first request until
/// the test releases it.
///
/// The first request is reported on `asked` with its body, the evidence the
/// group execution pinned, and is answered only once `release` fires; so the
/// test observes the pinned trace IDs, publishes later spans, and only then
/// lets the check run its live read. Every later request is answered at
/// once. Holding blocks one judge server thread for that first request.
struct HeldReview {
    /// The answer every request receives.
    response: ResponseTemplate,
    /// Where the first request's body is reported.
    asked: std::sync::mpsc::Sender<String>,
    /// The signal the first answer waits for; taken by that request.
    release: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl wiremock::Respond for HeldReview {
    /// Report and hold the first request, then answer it and every later
    /// one with `response`.
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        let first = self
            .release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(release) = first {
            drop(
                self.asked
                    .send(String::from_utf8_lossy(&request.body).into_owned()),
            );
            release.recv().unwrap_or_default();
        }
        self.response.clone()
    }
}

/// Script the `coding-judge` check Agent on the local judge: a request that
/// already carries tool results passes with [`REVIEWED`]; any other asks for
/// one `bifrost.query` of the span names stored for `trace`, the first such
/// answer held as [`HeldReview`] describes.
///
/// Returns where the first review request's body arrives and the sender
/// that releases its answer.
async fn script_coding_judge(
    judge: &MockServer,
    trace: &str,
) -> (
    std::sync::mpsc::Receiver<String>,
    std::sync::mpsc::Sender<()>,
) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(LIVE_REVIEW))
        .and(body_string_contains(TOOL_TURN))
        .respond_with(completion(
            json!({ "role": "assistant", "content": REVIEWED }),
        ))
        .with_priority(1)
        .mount(judge)
        .await;
    let query = tool_call(
        "call-spans",
        "bifrost.query",
        &json!({
            "sql": format!("SELECT name FROM vala.traces.spans WHERE trace_id = decode('{trace}', 'hex') ORDER BY name"),
            "max_rows": 10,
        }),
    );
    let (asked, first) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(LIVE_REVIEW))
        .respond_with(HeldReview {
            response: completion(
                json!({ "role": "assistant", "content": null, "tool_calls": [query] }),
            ),
            asked,
            release: std::sync::Mutex::new(Some(held)),
        })
        .with_priority(2)
        .mount(judge)
        .await;
    (first, release)
}

/// A coding session's prompt, edit, and test calls are judged per turn, and
/// once its `after` period expires the whole session is reviewed from only
/// its declared sources:
/// each call, its trace ID where one was recorded and null where none was,
/// and each call's test outcome, failed one included. The trace slot carries
/// the ID alone: the check Agent's first request names no span, and its
/// `bifrost.query`, under the owning Service's ordinary tool and query
/// grants, reads live a span exported only after the review pinned and sent
/// its trace IDs, which stay unchanged; once that grant
/// is revoked, a later session's read is denied with nothing returned. An empty `each` selection is inconclusive `empty_selection`
/// inline and after a step. Holding only one of the tool-invoke and query
/// grants, a session's read is denied on the missing one. A child-field
/// source, a source matching no or two bindings on its site, a source naming
/// an undeclared slot, a source cycle, and a typed or source slot named
/// `group_id` are refused at registration through the SDK and, for the
/// reserved slot, over direct HTTP, leaving nothing registered; a source
/// Verifier named by reference to its registered Card binds.
///
/// # Panics
/// Panics when a step fails, the session is not reviewed in time, or a
/// judgment, a judge request, or a refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_coding_uses_declared_sources() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    let coding = assistant
        .grouped("cards/verify_in_real_time/coding.yaml")
        .await;
    deployment
        .server()
        .seed_role(
            "live_reader",
            &[
                json!({ "resource": "tool", "action": "invoke", "scope": "all" }),
                json!({ "resource": "bifrost_query", "action": "read", "scope": "all" }),
            ],
        )
        .await
        .expect("role seeds");
    let principals = Principals::connect_with(&deployment.admin())
        .await
        .expect("principals connect");
    let principal = assistant
        .principal(&coding.receipt.root)
        .await
        .parse()
        .expect("principal id");
    principals
        .grant_role(&principal, "live_reader")
        .await
        .expect("the Service may read spans live");
    let group = GroupId::new("session-1").expect("group id");
    let (dispatch, provider) = service_telemetry(&deployment.client(&coding.key));
    let session = tracing::dispatcher::with_default(&dispatch, || {
        let session = tracing::info_span!("session", otel.name = "coding-session");
        tracing::info_span!(parent: &session, "edit", otel.name = "edit-file").in_scope(|| ());
        session
    });
    let trace = session
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    let (asked, release) = script_coding_judge(&assistant.judge, &trace).await;
    let calls = [
        json!({"call": "prompt", "tests": "passed"}),
        json!({"call": "edit", "tests": "failed"}),
        json!({"call": "test", "tests": "passed"}),
    ];
    let run = coding.state.run_for_card("agent").expect("agent run opens");
    let mut verdicts = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        let mut inputs = vec![("call", Input::from(call.clone()))];
        if index == 0 {
            inputs.push(("trace", Input::from(json!(trace))));
        }
        let judged = run
            .observe()
            .group_id(group.clone())
            .verify("coding-turn", Inputs::named(inputs))
            .await
            .expect("the call is judged");
        verdicts.push(judged.outcome.verdict);
    }
    assert_eq!(
        verdicts,
        [
            VerificationVerdict::Passed,
            VerificationVerdict::Failed,
            VerificationVerdict::Passed
        ]
    );
    let empty = run
        .observe()
        .verify("empty-each", json!([]))
        .await
        .expect("the empty list is judged");
    assert_eq!(empty.outcome.verdict, VerificationVerdict::Inconclusive);
    let reasons: Vec<_> = empty
        .outcome
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.id.as_str(),
                requirement.outcome,
                requirement.reason.as_str(),
            )
        })
        .collect();
    assert_eq!(
        reasons,
        [
            (
                "listed-ok",
                RequirementOutcome::Inconclusive,
                "empty_selection"
            ),
            (
                "items-ok",
                RequirementOutcome::Inconclusive,
                "empty_selection"
            ),
        ]
    );
    let pinned = tokio::task::block_in_place(|| asked.recv_timeout(WAIT))
        .expect("the review asks before any later span exists");
    assert!(pinned.contains(&trace), "the pinned trace ID: {pinned}");
    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info_span!(parent: &session, "late", otel.name = "late-test-run").in_scope(|| ());
    });
    drop(session);
    tokio::task::block_in_place(|| provider.force_flush()).expect("the spans export");
    release.send(()).expect("the held review waits");

    let judged = deployment
        .group_execution(&coding.binding, None, WAIT)
        .await;

    let outcome = judged.result.expect("the result is visible").outcome;
    let evidence = |slot: &str| {
        serde_json::to_value(&outcome.evidence[&slot.parse().expect("slot name")])
            .expect("evidence encodes")
    };
    assert_eq!(evidence("calls"), json!({"kind": "json", "value": calls}));
    assert_eq!(
        evidence("traces"),
        json!({"kind": "json", "value": [trace, null, null]})
    );
    assert_eq!(
        evidence("tests"),
        json!({"kind": "json", "value": ["passed", "failed", "passed"]})
    );
    let judged_requirements: Vec<_> = outcome
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement.outcome))
        .collect();
    assert_eq!(
        judged_requirements,
        [
            ("every-call-tested", RequirementOutcome::Failed),
            ("spans-reviewed", RequirementOutcome::Passed),
        ],
        "{:#?}",
        outcome.requirements
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Failed);
    let (reads, asks): (Vec<_>, Vec<_>) = judged_bodies(&assistant.judge, LIVE_REVIEW)
        .await
        .into_iter()
        .partition(|body| body.contains(TOOL_TURN));
    let [ask] = &asks[..] else {
        panic!("one review request before any tool result: {asks:?}");
    };
    assert!(
        ask.contains(&trace) && !ask.contains("edit-file"),
        "the trace slot carries its ID only: {ask}"
    );
    let [read] = &reads[..] else {
        panic!("one review request carries the live read: {reads:?}");
    };
    assert!(
        read.contains("edit-file") && read.contains("late-test-run"),
        "the check reads the spans live: {read}"
    );

    let all = |resource: &str, action: &str| json!({ "resource": resource, "action": action, "scope": "all" });
    deployment
        .server()
        .seed_role(
            "verification_reader",
            &[
                all("cards", "read"),
                all("verifier", "run"),
                all("bifrost_table", "read"),
                all("bifrost_table", "write"),
                all("bifrost_record", "write"),
                all("tool", "invoke"),
                all("gateway", "invoke"),
                json!({
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": { "bifrost": { "schema": { "catalog": "vala", "schema": "verification" } } },
                }),
            ],
        )
        .await
        .expect("role seeds");
    let mut after = judged.execution_id;
    for (granted, revoked, denial) in [
        (None, "live_reader", "lacks Tool/Invoke"),
        (
            Some("verification_reader"),
            "workload",
            "WYRD_VALA_403_QUERY_FORBIDDEN",
        ),
    ] {
        if let Some(role) = granted {
            principals
                .grant_role(&principal, role)
                .await
                .expect("the role is granted");
        }
        principals
            .revoke_role(&principal, revoked)
            .await
            .expect("the role is revoked");
        let later = GroupId::new(format!("session-{revoked}")).expect("group id");
        run.observe()
            .group_id(later.clone())
            .verify(
                "coding-turn",
                Inputs::named([
                    ("call", Input::from(calls[0].clone())),
                    ("trace", Input::from(json!(trace))),
                ]),
            )
            .await
            .expect("the call is judged");
        let review = deployment
            .group_execution(&coding.binding, Some(&after), WAIT)
            .await;
        after = review.execution_id;
        assert_eq!(review.group_id.as_ref(), Some(&later));
        let reads: Vec<_> = judged_bodies(&assistant.judge, LIVE_REVIEW)
            .await
            .into_iter()
            .filter(|body| body.contains(TOOL_TURN))
            .collect();
        let read = reads.last().expect("the review asked to read");
        assert!(
            read.contains(denial) && !read.contains("edit-file"),
            "without {revoked}, the read is denied with nothing returned: {read}"
        );
    }

    let cards = deployment.cards().await;
    for (relative, kind, code, reason) in [
        (
            "coding-child",
            CardKind::Service,
            "WYRD_LOADER_400_INVALID_ENVELOPE",
            "child fields",
        ),
        (
            "coding-unbound",
            CardKind::Service,
            "WYRD_SPEC_400_VALIDATION",
            "matches 0 bindings",
        ),
        (
            "support-ambiguous",
            CardKind::Service,
            "WYRD_SPEC_400_VALIDATION",
            "matches 2 bindings",
        ),
        (
            "support-undeclared",
            CardKind::Service,
            "WYRD_SPEC_400_VALIDATION",
            "does not declare",
        ),
        (
            "coding-cycle",
            CardKind::Service,
            "WYRD_SPEC_400_VALIDATION",
            "forms a source cycle",
        ),
        (
            "reserved-typed",
            CardKind::Verifier,
            "WYRD_LOADER_400_INVALID_ENVELOPE",
            "inputs.group_id",
        ),
        (
            "reserved-source",
            CardKind::Verifier,
            "WYRD_LOADER_400_INVALID_ENVELOPE",
            "inputs.group_id",
        ),
    ] {
        let refused = cards
            .register_from_path(&fixture(&format!(
                "cards/verify_in_real_time/{relative}.yaml"
            )))
            .await
            .expect_err(relative);
        assert_eq!(refused.code(), code, "{relative}: {refused}");
        assert!(
            format!("{refused:?}").contains(reason),
            "{relative}: {refused:?}"
        );
        let absent = cards
            .resolve_latest(
                kind,
                "default".parse().expect("space parses"),
                relative.parse().expect("name parses"),
            )
            .await
            .expect_err("a refused Card is not registered");
        assert_eq!(
            absent.code(),
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
            "{relative}"
        );
    }
    cards
        .register_from_path(&fixture("cards/verify_in_real_time/coding-external.yaml"))
        .await
        .expect("a registered source Verifier named by reference binds");
    let token = deployment
        .admin()
        .access_token()
        .await
        .expect("access token mints");
    for (relative, slot) in [
        ("reserved-typed", json!({"type": "json"})),
        ("reserved-source", json!("verifiers.turn-check.inputs.turn")),
    ] {
        let card = json!({
            "apiVersion": "wyrd/v1",
            "kind": "Verifier",
            "metadata": {"name": relative, "version": "1.0.0", "space": "default"},
            "spec": {
                "inputs": {"group_id": slot},
                "requirements": [{
                    "id": "present",
                    "statement": "The slot is present",
                    "check": {"kind": "assert", "path": "inputs.group_id", "operator": "is_not_null"},
                }],
            },
            "artifacts": [],
        });
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/cards",
                deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
            .header("Idempotency-Key", relative)
            .json(&json!({ "submissions": [card] }))
            .send()
            .await
            .expect("the server answers");
        let status = response.status().as_u16();
        let refused: serde_json::Value = response.json().await.expect("the body is JSON");
        assert_eq!(status, 400, "{relative}: {refused}");
        assert_eq!(refused["code"], "WYRD_SPEC_400_VALIDATION", "{refused}");
        assert!(refused.to_string().contains("inputs.group_id"), "{refused}");
    }
    coding.state.shutdown().await.expect("Bifrost stops");
    assistant.deployment.shutdown().await;
}

/// The frame judge's Verdict for a frame showing a badge: passed, citing the
/// frame, an image, as its one page.
const BADGED: &str = r#"{"outcome":"passed","reason":"a badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}"#;

/// The frame judge's Verdict for the frame showing no badge.
const NO_BADGE: &str = r#"{"outcome":"failed","reason":"no badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}"#;

/// Text every `frame-verifier` judge request carries.
const FRAME_CLAIM: &str = "Cite the frame.";

/// The label of the one frame whose person wears no badge.
const UNBADGED: &str = "no badge!";

/// [`UNBADGED`] in base64, as the judge's image data URL carries it.
const UNBADGED_BASE64: &str = "bm8gYmFkZ2Uh";

/// The label of the frame running when the server dies.
const SLOW: &str = "slow lens";

/// [`SLOW`] in base64, as the judge's image data URL carries it.
const SLOW_BASE64: &str = "c2xvdyBsZW5z";

/// Write a PNG frame into `dir` carrying the nine-byte `label` right after
/// the signature and a newline. Those nine bytes start the label on a base64
/// boundary, so its base64 appears verbatim in the image data URL the judge
/// receives.
///
/// # Panics
/// Panics when `label` is not nine bytes or the file cannot be written.
fn frame(dir: &TempDir, label: &str) -> std::path::PathBuf {
    assert_eq!(label.len(), 9, "a frame label is nine bytes");
    let path = dir.path().join(format!("{label}.png"));
    std::fs::write(
        &path,
        [b"\x89PNG\r\n\x1a\n\n".as_slice(), label.as_bytes()].concat(),
    )
    .expect("the frame writes");
    path
}

/// Script the `frame-judge` on the local judge: the [`UNBADGED`] frame fails,
/// the first [`SLOW`] request answers only after the server has died, and
/// every other frame passes with [`BADGED`].
async fn script_frame_judge(judge: &MockServer) {
    let verdict = |content: &str| completion(json!({ "role": "assistant", "content": content }));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(UNBADGED_BASE64))
        .respond_with(verdict(NO_BADGE))
        .with_priority(1)
        .mount(judge)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(SLOW_BASE64))
        .respond_with(verdict(BADGED).set_delay(Duration::from_secs(60)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(judge)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(FRAME_CLAIM))
        .respond_with(verdict(BADGED))
        .with_priority(2)
        .mount(judge)
        .await;
}

/// Submit each frame at `paths` to `frame-verifier` in one Run of `state`'s
/// `agent` under `group`, returning the queued executions in submission order.
///
/// # Panics
/// Panics when the Run does not open or a submission is not queued.
async fn submit_frames(
    state: &WyrdState,
    group: &GroupId,
    paths: &[std::path::PathBuf],
) -> Vec<Execution> {
    let run = state.run_for_card("agent").expect("agent run opens");
    let mut executions = Vec::new();
    for path in paths {
        let Submission::Queued(queued) = run
            .observe()
            .group_id(group.clone())
            .submit("frame-verifier", File::path(path))
            .await
            .expect("the frame is submitted")
        else {
            panic!("a frame submission queues an execution");
        };
        executions.push(queued);
    }
    executions
}

/// An application samples door events into PNG frames, submits each frame
/// under the event id as `group_id`, and reports the event fact; once the
/// event's `after` period expires `event-review` judges the whole frame list
/// from the visible frame outcomes. Zero frames are inconclusive
/// `empty_selection`; one unbadged frame of twenty fails the event, and
/// frames another Service or an unauthorized caller submits never join it;
/// twenty-one badged frames are judged like any other group and pass; and
/// exactly twenty frames pass even when the server dies while one is
/// running: ordinary per-item queue retry judges only that frame again, and
/// it counts once.
///
/// # Panics
/// Panics when a step fails, an event is not judged in time, or a judgment
/// or refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_sampled_frames_use_visible_outcomes() {
    let assistant = Box::pin(Assistant::start_with(
        WyrdTestServer::builder().with_shutdown_drain_for_test(Duration::from_secs(2)),
        Duration::ZERO,
    ))
    .await;
    script_frame_judge(&assistant.judge).await;
    let deployment = &assistant.deployment;
    let frames = assistant
        .grouped("cards/verify_in_real_time/frames.yaml")
        .await;
    let cards = deployment.cards().await;
    let other = register(&cards, "cards/verify_in_real_time/frames-other.yaml").await;
    let other_bundle = hydrate(&cards, &other.root).await;
    let other_state = WyrdState::from_path_with(
        other_bundle.path().join("bundle"),
        deployment.client(&deployment.service_key(&other.root).await),
    )
    .expect("bundle loads");
    other_state.start_bifrost().await.expect("Bifrost starts");
    let dir = TempDir::new().expect("frame directory");
    let outcomes = "outcomes".parse().expect("slot name");
    let event = "event".parse().expect("slot name");
    let fact = |id: &str| json!({ "event_id": id, "detected": true });
    let reason = |judged: &wyrd_spec::verification::execution::VerificationExecutionState| {
        let result = judged.result.as_ref().expect("the result is visible");
        (
            result.outcome.verdict,
            requirement(result, "every-frame-badged").reason.clone(),
        )
    };

    let empty = GroupId::new("door-44").expect("group id");
    frames.turn("event-fact", &empty, fact("door-44")).await;
    let judged = deployment
        .group_execution(&frames.binding, None, WAIT)
        .await;
    assert_eq!(judged.group_id.as_ref(), Some(&empty));
    assert_eq!(
        reason(&judged),
        (
            VerificationVerdict::Inconclusive,
            "empty_selection".to_owned()
        )
    );
    let evidence = &judged
        .result
        .as_ref()
        .expect("the result is visible")
        .outcome
        .evidence;
    assert_eq!(
        serde_json::to_value(&evidence[&outcomes]).expect("evidence encodes"),
        json!({ "kind": "json", "value": [] }),
    );

    let failing = GroupId::new("door-43").expect("group id");
    let mut paths: Vec<_> = (1..=19)
        .map(|index| frame(&dir, &format!("door43-{index:02}")))
        .collect();
    paths.insert(7, frame(&dir, UNBADGED));
    let executions = submit_frames(&frames.state, &failing, &paths).await;
    for foreign in submit_frames(&other_state, &failing, &[frame(&dir, "other-001")]).await {
        foreign
            .wait(WAIT)
            .await
            .expect("the other Service's frame is judged");
    }
    let unauthorized = deployment
        .verifier_key("frames_denied", &assistant.answer_check)
        .await;
    let denied = WyrdState::from_path_with(
        frames.bundle.path().join("bundle"),
        deployment.client(&unauthorized),
    )
    .expect("bundle loads")
    .run_for_card("agent")
    .expect("agent run opens")
    .observe()
    .group_id(failing.clone())
    .submit("frame-verifier", File::path(frame(&dir, "denied-01")))
    .await
    .expect_err("verifier:run on another Verifier is no grant");
    assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC", "{denied}");
    frames.turn("event-fact", &failing, fact("door-43")).await;
    for execution in &executions {
        execution.wait(WAIT).await.expect("the frame is judged");
    }
    let judged = deployment
        .group_execution(&frames.binding, Some(&judged.execution_id), WAIT)
        .await;
    assert_eq!(judged.group_id.as_ref(), Some(&failing));
    let snapshot = judged
        .group
        .as_ref()
        .expect("the execution pins its snapshot");
    // Frames ingested in one batch share its ingest time, so their list
    // order is by execution ID rather than by submission.
    let listed = &snapshot.sources[&outcomes].execution_ids;
    assert_eq!(listed.len(), 20);
    assert_eq!(
        listed.iter().collect::<BTreeSet<_>>(),
        executions
            .iter()
            .map(Execution::id)
            .collect::<BTreeSet<_>>(),
        "only the Service's own frames"
    );
    assert_eq!(snapshot.sources[&event].execution_ids.len(), 1);
    let unbadged = executions[7].id();
    let expected: Vec<_> = listed
        .iter()
        .map(|id| if id == unbadged { "failed" } else { "passed" })
        .collect();
    let outcome = &judged
        .result
        .as_ref()
        .expect("the result is visible")
        .outcome;
    assert_eq!(
        serde_json::to_value(&outcome.evidence[&outcomes]).expect("evidence encodes"),
        json!({ "kind": "json", "value": expected }),
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Failed);

    let many = GroupId::new("door-45").expect("group id");
    let paths: Vec<_> = (1..=21)
        .map(|index| frame(&dir, &format!("door45-{index:02}")))
        .collect();
    let executions = submit_frames(&frames.state, &many, &paths).await;
    frames.turn("event-fact", &many, fact("door-45")).await;
    for execution in &executions {
        execution.wait(WAIT).await.expect("the frame is judged");
    }
    let judged = deployment
        .group_execution(&frames.binding, Some(&judged.execution_id), WAIT)
        .await;
    assert_eq!(judged.group_id.as_ref(), Some(&many));
    let snapshot = judged
        .group
        .as_ref()
        .expect("the execution pins its snapshot");
    assert_eq!(
        snapshot.sources[&outcomes]
            .execution_ids
            .iter()
            .collect::<BTreeSet<_>>(),
        executions
            .iter()
            .map(Execution::id)
            .collect::<BTreeSet<_>>()
    );
    let outcome = &judged
        .result
        .as_ref()
        .expect("the result is visible")
        .outcome;
    assert_eq!(
        serde_json::to_value(&outcome.evidence[&outcomes]).expect("evidence encodes"),
        json!({ "kind": "json", "value": vec!["passed"; 21] }),
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Passed);

    let door = GroupId::new("door-42").expect("group id");
    let mut paths: Vec<_> = (1..=19)
        .map(|index| frame(&dir, &format!("door42-{index:02}")))
        .collect();
    paths.push(frame(&dir, SLOW));
    let executions = submit_frames(&frames.state, &door, &paths).await;
    frames.turn("event-fact", &door, fact("door-42")).await;
    for execution in &executions[..19] {
        execution.wait(WAIT).await.expect("the frame is judged");
    }
    let ids: Vec<_> = executions.iter().map(|execution| *execution.id()).collect();
    let mut after = judged.execution_id;
    frames.state.shutdown().await.expect("Bifrost stops");
    other_state.shutdown().await.expect("Bifrost stops");
    // Rows the dying process has not published are outbox loss; the
    // positive variant publishes the 19 judged frames first.
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let Assistant {
        deployment, judge, ..
    } = assistant;
    let deployment = deployment
        .restart(on_judge(WyrdTestServer::builder(), &judge))
        .await;
    Execution::connect_with(ids[19], &deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the running frame recovers and is judged");
    let judged = loop {
        let judged = deployment
            .group_execution(&frames.binding, Some(&after), WAIT)
            .await;
        assert_eq!(judged.group_id.as_ref(), Some(&door));
        if reason(&judged).0 == VerificationVerdict::Passed {
            break judged;
        }
        // An earlier snapshot may pin the running frame's outcome null; the
        // recovered result's acknowledgement makes the group due again.
        after = judged.execution_id;
    };
    let snapshot = judged
        .group
        .as_ref()
        .expect("the execution pins its snapshot");
    assert_eq!(
        snapshot.sources[&outcomes]
            .execution_ids
            .iter()
            .collect::<BTreeSet<_>>(),
        ids.iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(
        serde_json::to_value(
            &judged
                .result
                .as_ref()
                .expect("the result is visible")
                .outcome
                .evidence[&outcomes]
        )
        .expect("evidence encodes"),
        json!({ "kind": "json", "value": vec!["passed"; 20] }),
    );
    assert_eq!(judged_bodies(&judge, SLOW_BASE64).await.len(), 2);
    assert_eq!(
        judged_bodies(&judge, FRAME_CLAIM).await.len(),
        20 + 1 + 21 + 20 + 1,
        "every frame is judged once, and only the running one again"
    );
    deployment.shutdown().await;
}

/// One retained result's persisted `group_id` column.
#[derive(Debug, Deserialize, PartialEq)]
struct Grouping {
    /// The group the result belongs to, absent for an ungrouped turn.
    group_id: Option<String>,
}

/// The executions of `binding`'s retained group reviews, oldest first, read
/// as the administrator after publishing every captured row.
///
/// # Panics
/// Panics when a flush or the read fails.
async fn group_reviews(deployment: &Deployment, binding: &BindingId) -> Vec<ExecutionId> {
    /// One `vala.verification.results` row's execution identity.
    #[derive(Deserialize)]
    struct Review {
        /// The group execution the result settled.
        execution_id: String,
    }
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let rows: Vec<Review> = Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT execution_id FROM vala.verification.results \
             WHERE binding_id = $1 ORDER BY ended_at",
            &[QueryParam::String(binding.to_string())],
        )
        .await
        .expect("the reviews read back");
    rows.into_iter()
        .map(|row| {
            row.execution_id
                .parse()
                .expect("the row names an execution")
        })
        .collect()
}

/// Assert `binding` retains exactly `expected` group reviews for eight
/// seconds while the scheduler keeps polling: a handed-off group with no new
/// acknowledged activity is never judged again.
///
/// # Panics
/// Panics when a further review is retained.
async fn assert_no_new_review(
    deployment: &Deployment,
    binding: &BindingId,
    expected: &[ExecutionId],
) {
    let quiet = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < quiet {
        assert_eq!(
            group_reviews(deployment, binding).await,
            expected,
            "no group is reviewed twice"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Two support conversations run at once beside a drift sample, an
/// ungrouped turn, the same `group_id` in another tenant, and a turn judged
/// by a newer `turn-check` registered after the binding pinned its source.
/// Each conversation is reviewed once with only its own pinned turns, in
/// call order, the ungrouped turn reads back with no group, the drift sample
/// is recorded as usual, and later passes review no group a second time.
///
/// # Panics
/// Panics when a step fails, a review is not judged in time, or a review
/// differs or repeats.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_conversations_coexist_with_drift() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    let support = assistant
        .grouped("cards/verify_in_real_time/support.yaml")
        .await;
    let (a, b) = (
        GroupId::new("conv-a").expect("group id"),
        GroupId::new("conv-b").expect("group id"),
    );
    let turn = |answer: &str| json!({ "answer": answer });
    let slot = "turns".parse().expect("slot name");

    let drift = assistant.state().await;
    let sampled = drift
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .submit("latency-drift", json!({ "latency": 12.5 }))
        .await
        .expect("the drift sample is recorded");
    assert!(matches!(sampled, Submission::Recorded), "{sampled:?}");
    support.turn("turn-check", &a, turn("a1")).await;
    support.turn("turn-check", &b, turn("b1")).await;
    let Submission::Queued(queued) = support
        .state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .group_id(a.clone())
        .submit("turn-check", turn("a2"))
        .await
        .expect("the turn is submitted")
    else {
        panic!("a turn-check submission queues an execution");
    };
    support.turn("turn-check", &b, turn("")).await;
    let ungrouped = support
        .state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .verify("turn-check", turn("solo"))
        .await
        .expect("an ungrouped turn is judged");
    queued.wait(WAIT).await.expect("the queued turn is judged");

    let cards = deployment.cards().await;
    let newer = register(&cards, "cards/verify_in_real_time/turn-check-newer.yaml").await;
    let late = support
        .enqueue_over_http(
            deployment,
            "support-newer-turn",
            &json!({
                "verifier_card_uid": newer.root.uid,
                "card_uid": registered(&support.receipt, "assistant-agent").uid,
                "group_id": a,
                "inputs": {"turn": {"kind": "json", "value": turn("newer")}},
            }),
        )
        .await;
    Execution::connect_with(late, &deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the newer Verifier judges the turn");

    let tenant_b = deployment.client(&deployment.other_tenant_admin("tenant-b").await);
    let b_cards = Cards::connect_with(&tenant_b)
        .await
        .expect("handle connects");
    let b_support = register(&b_cards, "cards/verify_in_real_time/support.yaml").await;
    let b_bundle = hydrate(&b_cards, &b_support.root).await;
    let [kind, name, version, space] = coordinates(&b_support.root);
    let b_key = cli::issue_key(&kind, &name, &version, &space, None, None, Some(tenant_b))
        .await
        .expect("the CLI issues tenant B's Service key")
        .key;
    let b_state = WyrdState::from_path_with(
        b_bundle.path().join("bundle"),
        deployment.client(b_key.expose()),
    )
    .expect("bundle loads");
    b_state.start_bifrost().await.expect("Bifrost starts");
    b_state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .group_id(a.clone())
        .verify("turn-check", turn("tenant b"))
        .await
        .expect("tenant B's turn is judged");
    for state in [&support.state, &drift, &b_state] {
        state.shutdown().await.expect("Bifrost stops");
    }

    let deadline = tokio::time::Instant::now() + WAIT;
    let reviews = loop {
        let reviews = group_reviews(deployment, &support.binding).await;
        if reviews.len() >= 2 {
            break reviews;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "both conversations are reviewed within {WAIT:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let admin = deployment.admin();
    let mut judged = Vec::new();
    for id in &reviews {
        let state = Execution::connect_with(*id, &admin)
            .await
            .expect("the execution handle connects")
            .status()
            .await
            .expect("the review reads");
        let result = state.result.expect("the result is visible");
        judged.push((
            state.group_id.expect("a group review names its group"),
            state.group.expect("the review pins its snapshot").sources[&slot]
                .execution_ids
                .len(),
            serde_json::to_value(&result.outcome.evidence[&slot]).expect("evidence encodes"),
            result.outcome.verdict,
        ));
    }
    judged.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
    assert_eq!(
        judged,
        [
            (
                a,
                2,
                json!({"kind": "json", "value": [turn("a1"), turn("a2")]}),
                VerificationVerdict::Passed,
            ),
            (
                b,
                2,
                json!({"kind": "json", "value": [turn("b1"), turn("")]}),
                VerificationVerdict::Failed,
            ),
        ],
        "each conversation judges only its own pinned turns"
    );
    let solo: Vec<Grouping> = Bifrost::connect_with(&admin, BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT group_id FROM vala.verification.results WHERE execution_id = $1",
            &[QueryParam::String(ungrouped.execution_id.to_string())],
        )
        .await
        .expect("the ungrouped result reads back");
    assert_eq!(
        solo,
        [Grouping { group_id: None }],
        "the turn joins no group"
    );

    assert_no_new_review(deployment, &support.binding, &reviews).await;
    assistant.deployment.shutdown().await;
}

/// One evidence slot of `review`'s result as JSON.
///
/// # Panics
/// Panics when the review has no result or no such slot.
fn evidence(review: &VerificationExecutionState, slot: &str) -> serde_json::Value {
    let result = review.result.as_ref().expect("the result is visible");
    serde_json::to_value(&result.outcome.evidence[&slot.parse().expect("slot name")])
        .expect("evidence encodes")
}

/// A judged conversation's review is slow. Answers acknowledged while it
/// runs reach exactly one next, overlapping review, which pins a
/// still-running answer's judgment as null; once that judgment lands, its
/// acknowledged result reaches exactly one more review. The first review's
/// snapshot is unchanged by later evidence, every review stays retained,
/// and nothing is reviewed again. Throughout, a neighbouring conversation
/// bound under a one-year `after`, the longest a Trigger accepts, does not
/// come due and never holds up the late group's progress.
///
/// # Panics
/// Panics when a step fails, a review is not judged in time, or a review
/// differs or repeats.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_executions_include_late_evidence() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    for (marker, delay, priority) in [("LATE-REVIEW", 6, 1), ("late answer", 15, 2)] {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(marker))
            .respond_with(
                completion(json!({ "role": "assistant", "content": AGREES }))
                    .set_delay(Duration::from_secs(delay)),
            )
            .with_priority(priority)
            .mount(&assistant.judge)
            .await;
    }
    let forever = assistant
        .grouped("cards/verify_in_real_time/forever.yaml")
        .await;
    forever
        .turn(
            "turn-check",
            &GroupId::new("endless").expect("group id"),
            json!({ "answer": "still here" }),
        )
        .await;
    let late = assistant
        .grouped("cards/verify_in_real_time/late.yaml")
        .await;
    let group = GroupId::new("slow-1").expect("group id");
    late.turn("answer-judged", &group, json!("first answer"))
        .await;

    let deadline = tokio::time::Instant::now() + WAIT;
    let first_id = loop {
        deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        if let Some(id) = deployment
            .binding_status(&late.binding)
            .await
            .last_execution_id
        {
            break id;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the first review is enqueued"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    late.turn("answer-judged", &group, json!("second answer"))
        .await;
    let Submission::Queued(pending) = late
        .state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .group_id(group.clone())
        .submit("answer-judged", json!("late answer"))
        .await
        .expect("the answer is submitted")
    else {
        panic!("an answer-judged submission queues an execution");
    };
    let review = Execution::connect_with(first_id, &deployment.admin())
        .await
        .expect("the execution handle connects");
    assert_ne!(
        review.status().await.expect("the review reads").status,
        VerificationExecutionStatus::Completed,
        "both answers were recorded while the first review ran"
    );
    review.wait(WAIT).await.expect("the first review completes");
    let first = review.status().await.expect("the review reads");
    assert_eq!(
        evidence(&first, "answer"),
        json!({"kind": "json", "value": ["first answer"]})
    );

    let next = deployment
        .group_execution(&late.binding, Some(&first_id), WAIT)
        .await;
    assert_eq!(
        evidence(&next, "answer"),
        json!({"kind": "json", "value": ["first answer", "second answer", "late answer"]}),
        "one next review covers every answer recorded during the first"
    );
    assert_eq!(
        evidence(&next, "agreed"),
        json!({"kind": "json", "value": ["passed", "passed", null]}),
        "the still-running judgment is pinned null"
    );
    pending.wait(WAIT).await.expect("the late answer is judged");
    let settled = deployment
        .group_execution(&late.binding, Some(&next.execution_id), WAIT)
        .await;
    assert_eq!(
        evidence(&settled, "agreed"),
        json!({"kind": "json", "value": ["passed", "passed", "passed"]}),
        "the late judgment is judged by exactly one more review"
    );
    late.state.shutdown().await.expect("Bifrost stops");
    assert_eq!(
        Execution::connect_with(first_id, &deployment.admin())
            .await
            .expect("the execution handle connects")
            .status()
            .await
            .expect("the review reads")
            .group,
        first.group,
        "later evidence leaves the first snapshot unchanged"
    );
    assert_no_new_review(
        deployment,
        &late.binding,
        &[first_id, next.execution_id, settled.execution_id],
    )
    .await;
    assert_eq!(
        deployment
            .binding_status(&forever.binding)
            .await
            .last_execution_id,
        None,
        "a conversation under a one-year after does not come due"
    );
    forever.state.shutdown().await.expect("Bifrost stops");
    assistant.deployment.shutdown().await;
}

/// The distinct permissions retained as denied for `principal` as subject,
/// read as the administrator after publishing every captured row.
///
/// # Panics
/// Panics when the flush or the read fails.
async fn denials(deployment: &Deployment, principal: &str) -> Vec<String> {
    #[derive(Deserialize)]
    /// One denied permission.
    struct Denied {
        /// The permission the operation needed.
        permission: String,
    }
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let rows: Vec<Denied> = Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT DISTINCT permission FROM vala.system.audit_log \
             WHERE sub_principal_id = $1 AND outcome = 'denied'",
            &[QueryParam::String(principal.to_owned())],
        )
        .await
        .expect("the denials read back");
    rows.into_iter().map(|row| row.permission).collect()
}

/// A support conversation whose owning Service may run its turns but may
/// not query Bifrost is never reviewed: the snapshot reads run under the
/// Service's own grants and the denial is retained. A query grant scoped to
/// the trace tables is still no grant on the verification sources. A denied
/// read enqueues nothing and leaves the group pending; once the Service
/// holds its default role again, the next pass reviews the whole
/// conversation from its inputs and outcomes, and with no trace slot or
/// Agent check nothing else is queried.
///
/// # Panics
/// Panics when a step fails, a review happens while the reads are not
/// granted, a denial is not retained, or the review differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn grouped_reads_need_the_owner_grant() {
    let assistant = Assistant::start().await;
    let deployment = &assistant.deployment;
    let support = assistant
        .grouped("cards/verify_in_real_time/support.yaml")
        .await;
    let all = |resource: &str, action: &str| json!({ "resource": resource, "action": action, "scope": "all" });
    let runs = [
        all("cards", "read"),
        all("verifier", "run"),
        all("bifrost_table", "read"),
        all("bifrost_table", "write"),
        all("bifrost_record", "write"),
    ];
    let traces = json!({
        "resource": "bifrost_query",
        "action": "read",
        "scope": { "bifrost": { "schema": { "catalog": "vala", "schema": "traces" } } },
    });
    let server = deployment.server();
    server
        .seed_role("turns_only", &runs)
        .await
        .expect("role seeds");
    let mut traced = runs.to_vec();
    traced.push(traces);
    server
        .seed_role("traces_reader", &traced)
        .await
        .expect("role seeds");
    let principals = Principals::connect_with(&deployment.admin())
        .await
        .expect("principals connect");
    let owner = assistant.principal(&support.receipt.root).await;
    let principal = owner.parse().expect("principal id");
    let swap = |from: &'static str, to: &'static str| {
        let principals = &principals;
        let principal = &principal;
        async move {
            principals
                .grant_role(principal, to)
                .await
                .expect("the role is granted");
            principals
                .revoke_role(principal, from)
                .await
                .expect("the role is revoked");
        }
    };
    swap("workload", "turns_only").await;
    let group = GroupId::new("guarded").expect("group id");
    let turns: Vec<_> = (1..=2)
        .map(|turn| json!({"turn": turn, "answer": format!("answer {turn}")}))
        .collect();
    for turn in &turns {
        support.turn("turn-check", &group, turn.clone()).await;
    }

    assert_no_new_review(deployment, &support.binding, &[]).await;
    let denied = denials(deployment, &owner).await;
    assert!(
        denied
            .iter()
            .any(|permission| permission == "bifrost_query:read"),
        "the poll's query is denied as the owner: {denied:?}"
    );
    swap("turns_only", "traces_reader").await;
    assert_no_new_review(deployment, &support.binding, &[]).await;
    swap("traces_reader", "workload").await;
    let judged = deployment
        .group_execution(&support.binding, None, WAIT)
        .await;

    assert_eq!(judged.group_id.as_ref(), Some(&group));
    let snapshot = judged
        .group
        .as_ref()
        .expect("the execution pins its snapshot");
    assert_eq!(
        snapshot.sources[&"turns".parse().expect("slot name")]
            .execution_ids
            .len(),
        2,
        "{snapshot:?}"
    );
    assert_eq!(
        evidence(&judged, "turns"),
        json!({"kind": "json", "value": turns})
    );
    assert_eq!(
        evidence(&judged, "answered"),
        json!({"kind": "json", "value": ["passed", "passed"]})
    );
    let asked = assistant
        .judge
        .received_requests()
        .await
        .expect("the judge records requests");
    assert!(
        asked.is_empty(),
        "a review without a trace or Agent check queries nothing"
    );
    assistant.deployment.shutdown().await;
}

/// Scribe outbox capacity of the staging-pressure story: three quarters of
/// it, four writes, is what two direct judgments reserve.
const STAGING_CAPACITY: usize = 6;

/// How long the slow judge of the staging-pressure story holds each call.
const JUDGE_DELAY: Duration = Duration::from_secs(15);

/// Stable code of a call refused for this server's staging capacity.
const STAGING_FULL: &str = "WYRD_VERIFICATION_429_STAGING_FULL";

/// The count of one tenant's `vala.verification.execution_inputs` rows.
#[derive(Debug, Deserialize)]
struct InputRows {
    /// Rows the tenant has recorded.
    rows: i64,
}

/// A tenant's slow judgments fill the server's reserved staging share while
/// another tenant's submission is accepted and judged beside the first of
/// them, the bursting tenant offering normal calls one at a time until the
/// neighbor's execution completes; once the share is full, further verify and
/// submit calls, grouped ones included, are refused with typed retryable
/// pressure before staging anything. Once the burst drains, the identical
/// refused call is accepted and judged once, the refused grouped turn is
/// accepted and its group judged over exactly that one turn, and the
/// bursting tenant holds exactly one inputs row per accepted call.
///
/// The normal calls hold at most one reserved slot at a time, so beside the
/// first judgment's two the neighbor's execution always has its slot.
///
/// # Panics
/// Panics when a refusal is missing or untyped, the neighbor's execution
/// does not complete during the burst, the recovery fails, or the recorded
/// inputs differ from the accepted calls.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verification_staging_refuses_and_recovers() {
    let assistant = Box::pin(Assistant::start_with(
        WyrdTestServer::builder().with_scribe_outbox_capacity_for_test(STAGING_CAPACITY),
        JUDGE_DELAY,
    ))
    .await;
    let (neighbor, _neighbor_bundle) = assistant.in_tenant("burst-neighbor").await;
    let state = assistant.state().await;
    let support = assistant
        .grouped("cards/verify_in_real_time/support.yaml")
        .await;
    let group = GroupId::new("pressured").expect("group id");
    let server = assistant.deployment.server();
    server.flush_bifrost().await.expect("setup settles");

    let mut burst = Vec::new();
    let mut offered = 0;
    for in_flight in 1..=2 {
        let judging = state.clone();
        burst.push(tokio::spawn(async move {
            judging
                .run_for_card("agent")
                .expect("agent run opens")
                .observe()
                .verify("answer-judged", json!("yes"))
                .await
        }));
        assistant.await_judge_calls(in_flight).await;
        if in_flight == 1 {
            let Submission::Queued(accepted) = neighbor
                .run_for_card("agent")
                .expect("agent run opens")
                .observe()
                .submit("answer-check", json!("yes"))
                .await
                .expect("the neighbor's answer is accepted during the burst")
            else {
                panic!("Run evidence queues an execution");
            };
            let neighbor_waits = tokio::spawn(async move { accepted.wait(WAIT).await });
            loop {
                let Submission::Queued(normal) = state
                    .run_for_card("agent")
                    .expect("agent run opens")
                    .observe()
                    .submit("answer-check", json!("yes"))
                    .await
                    .expect("the bursting tenant's normal call is accepted beside the neighbor")
                else {
                    panic!("Run evidence queues an execution");
                };
                let normal = normal.wait(WAIT).await.expect("the normal call is judged");
                assert_eq!(normal.outcome.verdict, VerificationVerdict::Passed);
                offered += 1;
                if neighbor_waits.is_finished() {
                    break;
                }
            }
            let neighbor_result = neighbor_waits
                .await
                .expect("the neighbor's wait joins")
                .expect("the neighbor's accepted execution completes during the burst");
            assert_eq!(neighbor_result.outcome.verdict, VerificationVerdict::Passed);
            assert!(
                !burst[0].is_finished(),
                "the bursting tenant's judgment is still in flight"
            );
            server.flush_bifrost().await.expect("the neighbor settles");
        }
    }

    let refused_run = state.run_for_card("agent").expect("agent run opens");
    let refused_verify = refused_run
        .observe()
        .verify("answer-check", json!("yes"))
        .await
        .expect_err("a full server refuses the verify");
    let refused_submit = refused_run
        .observe()
        .submit("answer-check", json!("yes"))
        .await
        .expect_err("a full server refuses the submit after its own retries");
    let grouped_run = support
        .state
        .run_for_card("agent")
        .expect("agent run opens");
    let refused_grouped = grouped_run
        .observe()
        .group_id(group.clone())
        .verify("turn-check", json!({"answer": "pressured"}))
        .await
        .expect_err("a full server refuses the grouped verify");
    for refused in [&refused_verify, &refused_submit, &refused_grouped] {
        assert_eq!(refused.code(), STAGING_FULL, "{refused}");
        assert_eq!(refused.status(), 429);
        assert_eq!(
            refused.as_problem_json()["details"]["retry_after_seconds"],
            json!(1)
        );
    }

    assert!(
        burst.iter().all(|judging| !judging.is_finished()),
        "the bursting tenant's judgments are still in flight"
    );

    for judging in burst {
        let judged = judging
            .await
            .expect("the judgment joins")
            .expect("each accepted judgment completes");
        assert_eq!(judged.outcome.verdict, VerificationVerdict::Passed);
    }
    server.flush_bifrost().await.expect("the burst drains");
    let Submission::Queued(recovered) = refused_run
        .observe()
        .submit("answer-check", json!("yes"))
        .await
        .expect("the identical call is accepted once the burst drains")
    else {
        panic!("Run evidence queues an execution");
    };
    let recovered = recovered
        .wait(WAIT)
        .await
        .expect("the recovered call is judged");
    assert_eq!(recovered.outcome.verdict, VerificationVerdict::Passed);
    assert_eq!(recovered.run_id.as_ref(), Some(refused_run.run_id()));
    grouped_run
        .observe()
        .group_id(group.clone())
        .verify("turn-check", json!({"answer": "pressured"}))
        .await
        .expect("the identical grouped call is accepted once the burst drains");
    support.state.shutdown().await.expect("Bifrost stops");
    let judged = assistant
        .deployment
        .group_execution(&support.binding, None, WAIT)
        .await;
    assert_eq!(judged.group_id.as_ref(), Some(&group));
    assert_eq!(
        evidence(&judged, "turns"),
        json!({"kind": "json", "value": [{"answer": "pressured"}]}),
        "the refused grouped turn staged nothing"
    );

    server.flush_bifrost().await.expect("rows publish");
    let recorded = Bifrost::connect_with(&assistant.deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as::<InputRows>(
            "SELECT count(*) AS rows FROM vala.verification.execution_inputs",
            &[],
        )
        .await
        .expect("the inputs rows count");
    assert_eq!(
        recorded.iter().map(|count| count.rows).collect::<Vec<_>>(),
        [4 + offered],
        "the burst judgments, the normal calls, and the recovered and grouped calls; nothing of the refused calls"
    );
    assistant.deployment.shutdown().await;
}
