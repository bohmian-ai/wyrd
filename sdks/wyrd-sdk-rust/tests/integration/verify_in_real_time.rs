//! An assistant judges its own behavior while it runs: `observe().verify`
//! returns the `VerificationResult` of the Verifier bound to the observed
//! Card, with its verdict and per-requirement breakdown, and `submit` queues
//! the same judgment in the background or records drift samples for the
//! Verifier's schedule. Only `verifier:run` on the named Verifier authorizes
//! the call; an Agent check then calls the gateway as that same caller.

use std::collections::BTreeSet;
use std::time::Duration;
use wyrd_ai::bifrost::BifrostOptions;

use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::bifrost::QueryParam;
use wyrd_ai::cards::{CardRef, Cards};
use wyrd_ai::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Inputs, Submission};
use wyrd_ai::{
    Bifrost, CallProvenance, Citation, Gateway, Locator, RequirementOutcome, SpanRecord,
    VerificationVerdict, WyrdClient, WyrdError, cli,
};
use wyrd_spec::verification::VerifierReadiness;
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

use crate::local_development::configure_gateway;
use crate::support::{Deployment, ancestry, coordinates, hydrate, register, registered};

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

impl Assistant {
    /// Start the judge and the server, deploy the judge model on the gateway,
    /// then register the latency baseline, the latency Model, and the
    /// assistant, and hydrate the assistant.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    pub(crate) async fn start() -> Self {
        Self::start_on(WyrdTestServer::builder(), Duration::ZERO).await
    }

    /// Start the assistant deployment on `builder`, with the local judge
    /// answering each call after `judge_delay`, as [`Self::start`] does.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start_on(builder: WyrdTestServerBuilder, judge_delay: Duration) -> Self {
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
        let deployment = Deployment::start_with(
            builder
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    judge.uri().parse().expect("judge URL parses"),
                ),
        )
        .await;
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
    let principal = assistant.principal().await;
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

    let principal = assistant.principal().await;
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
/// them; once the share is full, further verify and submit calls are refused
/// with typed retryable pressure before staging anything. Once the burst
/// drains, the identical refused call is accepted and judged once, and the
/// bursting tenant holds exactly one inputs row per accepted call.
///
/// # Panics
/// Panics when a refusal is missing or untyped, the neighbor's execution
/// does not complete during the burst, the recovery fails, or the recorded
/// inputs differ from the accepted calls.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verification_staging_refuses_and_recovers() {
    let assistant = Assistant::start_on(
        WyrdTestServer::builder().with_scribe_outbox_capacity_for_test(STAGING_CAPACITY),
        JUDGE_DELAY,
    )
    .await;
    let (neighbor, _neighbor_bundle) = assistant.in_tenant("burst-neighbor").await;
    let state = assistant.state().await;
    let server = assistant.deployment.server();
    server.flush_bifrost().await.expect("setup settles");

    let mut burst = Vec::new();
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
            let neighbor_result = accepted
                .wait(WAIT)
                .await
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
    for refused in [&refused_verify, &refused_submit] {
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
        [3],
        "two burst judgments and the recovered call; nothing of the refused calls"
    );
    assistant.deployment.shutdown().await;
}
