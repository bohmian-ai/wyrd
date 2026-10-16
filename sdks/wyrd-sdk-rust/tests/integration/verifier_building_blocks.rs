//! Verifier building blocks, end to end: one `policy-review` Verifier runs
//! every action kind (`extract`, `agent`, `code`) and every Run check kind
//! (`assert`, `trace`, `agent`, `code`) over an answer, its refund policy
//! PDF, and the trace that produced it; `document-formats` extracts each of
//! the nine extractable formats into its locked output. Drift checks run on
//! schedules in `scheduled_drift_alerts_operator`, `observe_a_run`, and
//! `verify_in_real_time`.
//!
//! The local upstream stands in for the model provider: the refund reader
//! first calls `fs.read` on its workspace copy of the policy and then answers
//! a 100-dollar limit, the judge passes every answer with a citation unless
//! the answer names a scripted case, and the `tool-review` investigator
//! calls the Card, Bifrost, and code tools at once and then finishes. Scripted
//! answer texts also fail the reader's provider, schema, and deadline and the
//! code step and check; scoped and delegated principals exercise every tool
//! grant.

use std::path::{Path, PathBuf};
use std::time::Duration;

use opentelemetry::trace::TraceContextExt as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use serde_json::{Value, json};
use tempfile::TempDir;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::CardRef;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Input, Inputs, Submission};
use wyrd_ai::{
    Bifrost, CallProvenance, Citation, Locator, MediaFormat, RequirementOutcome, RequirementResult,
    StepResult, StepStatus, VerificationResult, VerificationVerdict, WyrdClient,
};
use wyrd_spec::auth::TokenAudience;
use wyrd_testing::server::WyrdTestServer;

use crate::action_timeline::service_telemetry;
use crate::local_development::configure_gateway;
use crate::support::{Deployment, fixture, hydrate, register};

/// How long a test waits for a queued execution.
const WAIT: Duration = Duration::from_secs(90);

/// The judge's Verdict for an answer naming no scripted case: passed, citing
/// the answer text.
const FOLLOWS: &str = r#"{"outcome":"passed","reason":"within the policy","citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}"#;

/// The judge's Verdict for an answer containing `REFUSE-POLICY`: failed,
/// citing the answer text.
const BREAKS: &str = r#"{"outcome":"failed","reason":"contradicts the policy","citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}"#;

/// The judge's Verdict for an answer containing `UNCITED`: a pass with no
/// citation, which must not count as one.
const UNCITED: &str = r#"{"outcome":"passed","reason":"trust me","citations":[]}"#;

/// The judge's Verdict for an answer containing `JUDGE-FOREIGN-CITE`: a pass
/// citing a JSON pointer the original answer does not have.
const FOREIGN_CITE: &str = r#"{"outcome":"passed","reason":"see the note","citations":[{"slot":"answer","locator":{"json_pointer":"/note"}}]}"#;

/// The three built-ins the investigator calls, in call order.
const TOOLS: [&str; 3] = ["cards.get_by_ref", "bifrost.query", "code.run"];

/// The stable code of a permission denial, carried by every denied tool
/// call's error.
const DENIED: &str = "WYRD_PERMISSION_403_DENIED_RBAC";

/// The permissions every `tool-review` principal holds: run any Verifier and
/// invoke the gateway its investigator calls the model through.
const VERIFIES: [&str; 2] = ["verifier:run", "gateway:invoke"];

/// The policy every review reads: the support desk's checked-in refund
/// policy.
fn policy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/support-desk/service/refund-policy.pdf")
}

/// A Chat Completions reply whose assistant `message` is the given object.
fn completion(message: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "chatcmpl-blocks",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{ "index": 0, "finish_reason": "stop", "message": message }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
    }))
}

/// Mount `response` for the refund reader's Chat Completions requests whose
/// answer under review contains `marker`, ahead of the reader's scripted
/// tool turn, which is mounted after it at the same priority.
async fn fail_reader(upstream: &MockServer, marker: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("REFUND RULES"))
        .and(body_string_contains(marker))
        .respond_with(response)
        .with_priority(1)
        .mount(upstream)
        .await;
}

/// One assistant tool call of `tool` with JSON `arguments`, as the model
/// returns it: under the provider-facing name, which spells the dotted wire
/// name with underscores.
fn tool_call(id: &str, tool: &str, arguments: &Value) -> Value {
    json!({
        "id": id,
        "type": "function",
        "function": { "name": tool.replace('.', "_"), "arguments": arguments.to_string() },
    })
}

/// Script the investigator: a request that already carries tool results
/// finishes with `{"done": true}`; any other asks for one call of each of
/// [`TOOLS`] in a single turn.
async fn script_investigator(upstream: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("INVESTIGATE"))
        .and(body_string_contains(r#""role":"tool""#))
        .respond_with(completion(
            json!({ "role": "assistant", "content": r#"{"done":true}"# }),
        ))
        .with_priority(1)
        .mount(upstream)
        .await;
    let calls = [
        tool_call(
            "call-card",
            TOOLS[0],
            &json!({ "card_ref": {
                "kind": "Service", "name": "policy-desk", "version": "1.0.0", "space": "default"
            } }),
        ),
        tool_call(
            "call-query",
            TOOLS[1],
            &json!({ "sql": "SELECT execution_id FROM vala.verification.results", "max_rows": 1 }),
        ),
        tool_call("call-code", TOOLS[2], &json!({ "source": "6 * 7" })),
    ];
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("INVESTIGATE"))
        .respond_with(completion(
            json!({ "role": "assistant", "content": null, "tool_calls": calls }),
        ))
        .with_priority(2)
        .mount(upstream)
        .await;
}

/// Mount the answer `content` for Chat Completions requests whose body
/// contains `marker`, ranked by `priority` (lower wins).
async fn answer(upstream: &MockServer, marker: &str, content: &str, priority: u8) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(marker))
        .respond_with(completion(
            json!({ "role": "assistant", "content": content }),
        ))
        .with_priority(priority)
        .mount(upstream)
        .await;
}

/// The `policy-review` inputs: an answer with `text` offering `refund`
/// dollars, the refund policy, and the trace that produced it, when there is
/// one.
fn review(text: &str, refund: u32, trace: Option<&str>) -> Inputs {
    let mut inputs = vec![
        (
            "answer",
            Input::from(json!({ "text": text, "refund": refund })),
        ),
        ("policy", Input::from(File::path(policy()))),
    ];
    if let Some(trace) = trace {
        inputs.push(("trace", Input::from(json!(trace))));
    }
    Inputs::named(inputs)
}

/// The citation of the answer's text, which every cited Verdict of the
/// story names.
///
/// # Panics
/// Panics when `answer` is not a slot name.
fn answer_text() -> [Citation; 1] {
    [Citation {
        slot: "answer".parse().expect("slot name"),
        locator: Locator::JsonPointer("/text".to_owned()),
    }]
}

/// The requirement `id` of `result`.
///
/// # Panics
/// Panics when the result has no such requirement.
fn requirement<'a>(result: &'a VerificationResult, id: &str) -> &'a RequirementResult {
    result
        .outcome
        .requirements
        .iter()
        .find(|requirement| requirement.id.as_str() == id)
        .unwrap_or_else(|| panic!("requirement {id}: {result:#?}"))
}

/// The step `id` of `result`.
///
/// # Panics
/// Panics when the result has no such step.
fn step<'a>(result: &'a VerificationResult, id: &str) -> &'a StepResult {
    result
        .outcome
        .steps
        .iter()
        .find(|step| step.id.as_str() == id)
        .unwrap_or_else(|| panic!("step {id}: {result:#?}"))
}

/// One retained result's subject and actor.
#[derive(Debug, PartialEq, Deserialize)]
struct Attribution {
    /// The principal the result was for.
    sub_principal_id: String,
    /// The principal that requested it.
    act_principal_id: String,
}

/// The policy desk deployment: a verification-capable server whose gateway
/// deploys `gpt-4o` on the local upstream, with `policy-desk` registered and
/// hydrated and acting as its own Card-scoped key.
struct PolicyDesk {
    /// The deployment.
    deployment: Deployment,
    /// The local model upstream.
    upstream: MockServer,
    /// The hydrated bundle, which every state loads from.
    bundle: TempDir,
    /// The registered `policy-desk` Service.
    root: CardRef,
    /// The Service's own client.
    client: WyrdClient,
    /// The hydrated Service.
    state: WyrdState,
}

impl PolicyDesk {
    /// Start the upstream and the server, deploy `openai` on the upstream,
    /// register and hydrate `policy-desk`, and load it as its own key.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        fail_reader(&upstream, "PROVIDER-DOWN", ResponseTemplate::new(500)).await;
        fail_reader(
            &upstream,
            "OFF-SCHEMA",
            completion(json!({ "role": "assistant", "content": r#"{"wrong":1}"# })),
        )
        .await;
        fail_reader(
            &upstream,
            "SLOW-READER",
            completion(json!({ "role": "assistant", "content": r#"{"max_refund":100}"# }))
                .set_delay(Duration::from_secs(15)),
        )
        .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains("REFUND RULES"))
            .respond_with(completion(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-read-policy",
                    "type": "function",
                    "function": {
                        "name": "fs.read",
                        "arguments": "{\"path\":\"/inputs/policy.pdf\"}",
                    },
                }],
            })))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&upstream)
            .await;
        answer(&upstream, "REFUND RULES", r#"{"max_refund":100}"#, 2).await;
        answer(&upstream, "REFUSE-POLICY", BREAKS, 3).await;
        answer(&upstream, "UNCITED", UNCITED, 3).await;
        answer(&upstream, "JUDGE-MALFORMED", "not a verdict", 3).await;
        answer(&upstream, "JUDGE-FOREIGN-CITE", FOREIGN_CITE, 3).await;
        script_investigator(&upstream).await;
        answer(&upstream, "Claim:", FOLLOWS, 4).await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .without_process_telemetry_for_test()
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    upstream.uri().parse().expect("upstream URL parses"),
                ),
        )
        .await;
        configure_gateway(&deployment.admin()).await;
        let cards = deployment.cards().await;
        let desk = register(&cards, "cards/verifier_building_blocks/policy-desk.yaml").await;
        let bundle = hydrate(&cards, &desk.root).await;
        let client = deployment.client(&deployment.service_key(&desk.root).await);
        let state = WyrdState::from_path_with(bundle.path().join("bundle"), client.clone())
            .expect("bundle loads offline");
        state.start_bifrost().await.expect("Bifrost starts");
        Self {
            deployment,
            upstream,
            bundle,
            root: desk.root,
            client,
            state,
        }
    }

    /// Verify `inputs` with `policy-review` now.
    ///
    /// # Panics
    /// Panics when the verify is refused.
    async fn review(&self, inputs: Inputs) -> VerificationResult {
        self.state
            .run()
            .observe()
            .verify("policy-review", inputs)
            .await
            .expect("the review runs")
    }

    /// Export a span named `outer`, carrying `policy_id`, with one child
    /// named `inner` started inside it, as the Service; wait until both are
    /// stored and return the trace ID as 32 hex characters.
    ///
    /// Stored spans order by start time, so `outer` is the trace document's
    /// `spans[0]` and `inner` its `spans[1]`.
    ///
    /// # Panics
    /// Panics when the spans do not export or are not both stored.
    async fn record_trace(&self, [outer, inner]: [&str; 2], policy_id: &str) -> String {
        let (dispatch, provider) = service_telemetry(&self.client);
        let trace = tracing::dispatcher::with_default(&dispatch, || {
            let span = tracing::info_span!("lookup", otel.name = %outer, policy_id = %policy_id);
            span.in_scope(|| tracing::info_span!("draft", otel.name = %inner).in_scope(|| ()));
            span.context().span().span_context().trace_id().to_string()
        });
        tokio::task::block_in_place(|| provider.force_flush()).expect("the spans export");
        assert_eq!(
            self.deployment.timeline(&trace).await.len(),
            2,
            "both spans are stored"
        );
        trace
    }

    /// The hydrated Service, loaded with its server calls fixed to `client`
    /// and Bifrost not started.
    ///
    /// # Panics
    /// Panics when the bundle does not load.
    fn state_as(&self, client: WyrdClient) -> WyrdState {
        WyrdState::from_path_with(self.bundle.path().join("bundle"), client)
            .expect("bundle loads offline")
    }

    /// Ask the investigator `question` with `tool-review` now, as `client`.
    ///
    /// # Panics
    /// Panics when the verify is refused.
    async fn investigate(&self, client: WyrdClient, question: &str) -> VerificationResult {
        self.state_as(client)
            .run()
            .observe()
            .verify("tool-review", json!(question))
            .await
            .expect("the investigation runs")
    }

    /// Assert the investigation of `question` finished after calling each of
    /// [`TOOLS`] once, in order, where exactly the `allowed` calls returned
    /// output and every other call was denied with [`DENIED`] and returned
    /// nothing. The model's tool turn shows the policy desk's Card only when
    /// the Card read was allowed.
    ///
    /// # Panics
    /// Panics when the step, a call, or the model's tool turn differs.
    async fn assert_tools(&self, result: &VerificationResult, question: &str, allowed: &[&str]) {
        let investigate = step(result, "investigate");
        assert_eq!(investigate.status, StepStatus::Succeeded, "{question}");
        let called: Vec<Value> = investigate
            .tool_calls
            .iter()
            .map(|call| serde_json::to_value(call.tool).expect("the tool serializes"))
            .collect();
        assert_eq!(called, TOOLS.map(|tool| json!(tool)), "{question}");
        for (call, tool) in investigate.tool_calls.iter().zip(TOOLS) {
            if allowed.contains(&tool) {
                assert!(call.error.is_none(), "{question} {tool}: {call:?}");
                assert!(call.output_sha256.is_some(), "{question} {tool}");
                assert!(call.returned_bytes > 0, "{question} {tool}");
            } else {
                assert!(
                    call.error
                        .as_deref()
                        .is_some_and(|error| error.contains(DENIED)),
                    "{question} {tool}: {call:?}"
                );
                assert_eq!(
                    (call.output_sha256.as_ref(), call.returned_bytes),
                    (None, 0),
                    "{question} {tool}"
                );
            }
        }
        let turn = self
            .upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .find(|body| body.contains(question) && body.contains(r#""role":"tool""#))
            .expect("the model received the tool results");
        assert_eq!(
            turn.contains("document-formats"),
            allowed.contains(&TOOLS[0]),
            "{question}: the Card reaches the model only when its read is allowed"
        );
    }
}

/// A conforming answer with its policy and lookup trace passes: the extract,
/// Agent, and code steps succeed in plan order and feed the assertion,
/// trace, Agent, and code checks, each with its evidence and provenance.
///
/// # Panics
/// Panics when the review is refused or any step or requirement differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn policy_review_runs_every_action_and_check_kind() {
    let desk = PolicyDesk::start().await;
    let trace = desk
        .record_trace(["lookup-policy", "draft-answer"], "refunds")
        .await;

    let result = desk
        .review(review("Refunds follow our policy.", 50, Some(&trace)))
        .await;

    assert_eq!(
        result.outcome.verdict,
        VerificationVerdict::Passed,
        "{result:#?}"
    );
    assert_eq!(result.subject, desk.root);
    let text = step(&result, "policy-text");
    assert_eq!(text.status, StepStatus::Succeeded);
    assert_eq!(
        text.output.as_ref().map(|output| &output["format"]),
        Some(&json!("pdf"))
    );
    assert!(matches!(
        text.provenance,
        Some(CallProvenance::Extract { .. })
    ));
    let rules = step(&result, "refund-rules");
    assert_eq!(
        (rules.status, rules.output.as_ref()),
        (StepStatus::Succeeded, Some(&json!({ "max_refund": 100 })))
    );
    let tools: Vec<Value> = rules
        .tool_calls
        .iter()
        .map(|call| serde_json::to_value(call.tool).expect("the tool serializes"))
        .collect();
    assert_eq!(tools, [json!("fs.read")]);
    let facts = step(&result, "answer-facts");
    assert_eq!(
        (facts.status, facts.output.as_ref()),
        (
            StepStatus::Succeeded,
            Some(&json!({ "within_limit": true }))
        )
    );
    assert!(matches!(
        facts.provenance,
        Some(CallProvenance::Code { .. })
    ));
    for id in [
        "policy-has-text",
        "limit-found",
        "refund-within-limit",
        "policy-looked-up",
        "answer-drafted-after-lookup",
        "lookup-read-refunds",
        "follows-policy",
        "names-the-policy",
    ] {
        assert_eq!(
            requirement(&result, id).outcome,
            RequirementOutcome::Passed,
            "{id}"
        );
    }
    let cited = answer_text();
    assert_eq!(requirement(&result, "follows-policy").citations, cited);
    assert_eq!(requirement(&result, "names-the-policy").citations, cited);
    let judged = desk
        .upstream
        .received_requests()
        .await
        .expect("the upstream records requests");
    let judge = judged
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .find(|body| body.contains("Claim:"))
        .expect("the judge was asked");
    assert!(
        !judge.contains("\"tools\""),
        "the judge is offered no tools: {judge}"
    );
    desk.deployment.shutdown().await;
}

/// A refund over the policy limit fails the deterministic assertion on the
/// code step's output, and with it the review.
///
/// # Panics
/// Panics when the review is refused or the outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn refund_over_the_limit_fails_the_assertion() {
    let desk = PolicyDesk::start().await;

    let result = desk
        .review(review("Refunds follow our policy.", 500, None))
        .await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    let over = requirement(&result, "refund-within-limit");
    assert_eq!(
        (over.outcome, over.actual.as_ref()),
        (RequirementOutcome::Failed, Some(&json!(false)))
    );
    desk.deployment.shutdown().await;
}

/// The judge failing the answer fails `follows-policy`, citing the answer.
///
/// # Panics
/// Panics when the review is refused or the outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn judge_verdict_fails_the_agent_check() {
    let desk = PolicyDesk::start().await;

    let result = desk
        .review(review(
            "REFUSE-POLICY: no refunds, policy or not.",
            50,
            None,
        ))
        .await;

    let judged = requirement(&result, "follows-policy");
    assert_eq!(judged.outcome, RequirementOutcome::Failed);
    assert_eq!(judged.citations.len(), 1, "{judged:?}");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    desk.deployment.shutdown().await;
}

/// A judge that passes without citing any evidence cannot pass the
/// requirement or the review.
///
/// # Panics
/// Panics when the review is refused or the uncited pass counts.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn uncited_judge_verdict_cannot_pass() {
    let desk = PolicyDesk::start().await;

    let result = desk
        .review(review("UNCITED: refunds follow our policy.", 50, None))
        .await;

    assert_ne!(
        requirement(&result, "follows-policy").outcome,
        RequirementOutcome::Passed
    );
    assert_ne!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A code check that raises cannot pass its requirement or the review.
///
/// # Panics
/// Panics when the review is refused or the raised check counts as a pass.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn raising_code_check_cannot_pass() {
    let desk = PolicyDesk::start().await;

    let result = desk.review(review("raise", 50, None)).await;

    assert_ne!(
        requirement(&result, "names-the-policy").outcome,
        RequirementOutcome::Passed
    );
    assert_ne!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A code step that reaches outside its sandbox fails, and the assertion
/// that depends on its output cannot pass.
///
/// # Panics
/// Panics when the review is refused, the step succeeds, or the dependent
/// requirement passes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn forbidden_code_access_cannot_pass_dependent_work() {
    let desk = PolicyDesk::start().await;

    let result = desk.review(review("read the host", 50, None)).await;

    assert_eq!(step(&result, "answer-facts").status, StepStatus::Failed);
    assert_ne!(
        requirement(&result, "refund-within-limit").outcome,
        RequirementOutcome::Passed
    );
    assert_ne!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A trace whose first span is not the policy lookup fails the trace check;
/// an omitted optional trace leaves it inconclusive, never passed.
///
/// # Panics
/// Panics when a review is refused or a trace outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn trace_check_fails_a_guess_and_needs_its_trace() {
    let desk = PolicyDesk::start().await;
    let guessed = desk
        .record_trace(["guess-policy", "draft-answer"], "refunds")
        .await;

    let failed = desk
        .review(review("Refunds follow our policy.", 50, Some(&guessed)))
        .await;
    let untraced = desk
        .review(review("Refunds follow our policy.", 50, None))
        .await;

    assert_eq!(
        requirement(&failed, "policy-looked-up").outcome,
        RequirementOutcome::Failed
    );
    assert_eq!(
        requirement(&untraced, "policy-looked-up").outcome,
        RequirementOutcome::Inconclusive
    );
    assert_ne!(untraced.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A review without its required policy is inconclusive, never passed.
///
/// # Panics
/// Panics when the review is refused or the verdict differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn missing_policy_is_inconclusive() {
    let desk = PolicyDesk::start().await;

    let result = desk
        .review(Inputs::named([(
            "answer",
            json!({ "text": "Refunds follow our policy.", "refund": 50 }),
        )]))
        .await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Inconclusive);
    desk.deployment.shutdown().await;
}

/// Bytes declared as a PDF that are not one fail the extract step, and no
/// work that depends on it passes.
///
/// # Panics
/// Panics when the review is refused, the step succeeds, or the review
/// passes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn malformed_policy_cannot_pass() {
    let desk = PolicyDesk::start().await;

    let result = desk
        .review(Inputs::named([
            (
                "answer",
                Input::from(json!({ "text": "Refunds follow our policy.", "refund": 50 })),
            ),
            (
                "policy",
                Input::from(File::bytes(b"not a pdf".to_vec()).with_format(MediaFormat::Pdf)),
            ),
        ]))
        .await;

    assert_eq!(step(&result, "policy-text").status, StepStatus::Failed);
    assert_ne!(
        requirement(&result, "policy-has-text").outcome,
        RequirementOutcome::Passed
    );
    assert_ne!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A policy above the slot's 64 KiB `max_bytes` is refused before any
/// upload or execution.
///
/// # Panics
/// Panics when the review is not refused with the stable code or the
/// upstream was called.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn oversized_policy_is_refused_locally() {
    let desk = PolicyDesk::start().await;

    let refused = desk
        .state
        .run()
        .observe()
        .verify(
            "policy-review",
            Inputs::named([
                (
                    "answer",
                    Input::from(json!({ "text": "Refunds follow our policy.", "refund": 50 })),
                ),
                (
                    "policy",
                    Input::from(File::bytes(vec![b'%'; 65_537]).with_format(MediaFormat::Pdf)),
                ),
            ]),
        )
        .await
        .expect_err("the policy exceeds max_bytes");

    assert_eq!(refused.code(), "WYRD_VERIFICATION_413_INPUT_TOO_LARGE");
    assert!(
        desk.upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .is_empty(),
        "nothing ran"
    );
    desk.deployment.shutdown().await;
}

/// A review submitted in the background uploads its policy, queues, and is
/// read back by its execution ID from a handle rebuilt with the Service's
/// client.
///
/// # Panics
/// Panics when the submission is not queued or a wait fails or differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn submitted_review_is_judged_in_the_background() {
    let desk = PolicyDesk::start().await;

    let Submission::Queued(execution) = desk
        .state
        .run()
        .observe()
        .submit(
            "policy-review",
            review("Refunds follow our policy.", 500, None),
        )
        .await
        .expect("the review is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    let resumed = Execution::connect_with(*execution.id(), &desk.client)
        .await
        .expect("the handle rebuilds from its ID");
    let result = resumed.wait(WAIT).await.expect("the review settles");
    let status = resumed.status().await.expect("the settled status reads");

    assert_eq!(
        status.result.as_ref(),
        Some(&result),
        "status carries the result wait returned"
    );
    assert!(status.error.is_none());
    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(
        requirement(&result, "refund-within-limit").outcome,
        RequirementOutcome::Failed
    );
    desk.deployment.shutdown().await;
}

/// Each of the nine extractable formats extracts into its exact locked
/// output, which the downstream assertion consumes.
///
/// # Panics
/// Panics when a document is refused or its output or requirement differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn every_format_extracts_its_locked_output() {
    let desk = PolicyDesk::start().await;
    let documents = [
        (
            "sample.pdf",
            json!({"format": "pdf", "pages": [{"number": 1, "text": "Refunds"}]}),
        ),
        (
            "sample.pptx",
            json!({"format": "pptx", "slides": [{"number": 1, "title": null, "paragraphs": ["a"],
                   "tables": [{"rows": [["x", ""]]}], "notes": ""}]}),
        ),
        (
            "sample.docx",
            json!({"format": "docx", "blocks": [
                {"kind": "paragraph", "text": "Refund policy", "style": "Heading1"},
                {"kind": "table", "rows": [["Window", "Days"], ["Standard", "30"]]}]}),
        ),
        (
            "sample.xlsx",
            json!({"format": "xlsx", "sheets": [{"name": "Refunds", "cells": [
                {"address": "A1", "value": "Days", "formula": null, "error": null},
                {"address": "B2", "value": 30, "formula": null, "error": null},
                {"address": "C3", "value": null, "formula": "=1/0", "error": "#DIV/0!"},
                {"address": "D4", "value": true, "formula": null, "error": null}]}]}),
        ),
        (
            "sample.csv",
            json!({"format": "csv", "rows": [["a", "b"], ["1", "2"]]}),
        ),
        (
            "sample.json",
            json!({"format": "json", "value": {"nested": [1, null]}}),
        ),
        ("sample.txt", json!({"format": "text", "text": ""})),
        (
            "sample.md",
            json!({"format": "markdown", "text": "# Title"}),
        ),
        ("sample.html", json!({"format": "html", "text": "<p>x</p>"})),
    ];

    for (name, expected) in documents {
        let result = desk
            .state
            .run()
            .observe()
            .verify(
                "document-formats",
                File::path(fixture(&format!(
                    "cards/verifier_building_blocks/samples/{name}"
                ))),
            )
            .await
            .unwrap_or_else(|error| panic!("{name} is read: {error}"));
        assert_eq!(
            step(&result, "read").output.as_ref(),
            Some(&expected),
            "{name}"
        );
        assert_eq!(
            requirement(&result, "extracted").outcome,
            RequirementOutcome::Passed,
            "{name}"
        );
    }
    desk.deployment.shutdown().await;
}

/// A reader whose provider fails, whose answer breaks its declared output
/// schema, or that outlives its step's five-second deadline fails its step:
/// the code step needing its rules is skipped, every requirement depending
/// on them is inconclusive, and with no other failure the review is
/// inconclusive, never failed or passed.
///
/// # Panics
/// Panics when a review is refused or a step, requirement, or verdict
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn failed_reader_leaves_its_dependents_inconclusive() {
    let desk = PolicyDesk::start().await;

    for marker in ["PROVIDER-DOWN", "OFF-SCHEMA", "SLOW-READER"] {
        let result = desk
            .review(review(
                &format!("{marker}: refunds follow our policy."),
                50,
                None,
            ))
            .await;

        let rules = step(&result, "refund-rules");
        assert_eq!(rules.status, StepStatus::Failed, "{marker}: {rules:?}");
        assert!(rules.reason.is_some(), "{marker}: the failure says why");
        assert_eq!(
            step(&result, "answer-facts").status,
            StepStatus::Skipped,
            "{marker}"
        );
        for id in ["limit-found", "refund-within-limit", "follows-policy"] {
            assert_eq!(
                requirement(&result, id).outcome,
                RequirementOutcome::Inconclusive,
                "{marker} {id}"
            );
        }
        assert_eq!(
            result.outcome.verdict,
            VerificationVerdict::Inconclusive,
            "{marker}"
        );
    }
    desk.deployment.shutdown().await;
}

/// A code step that raises or outlives its five-second deadline fails with
/// its reason, and the assertion on its output is inconclusive, never
/// passed.
///
/// # Panics
/// Panics when a review is refused or the step or requirement differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn failed_code_step_leaves_its_assertion_inconclusive() {
    let desk = PolicyDesk::start().await;

    for text in ["fail the facts", "spin the facts"] {
        let result = desk.review(review(text, 50, None)).await;

        let facts = step(&result, "answer-facts");
        assert_eq!(facts.status, StepStatus::Failed, "{text}: {facts:?}");
        assert!(facts.reason.is_some(), "{text}: the failure says why");
        assert_eq!(
            requirement(&result, "refund-within-limit").outcome,
            RequirementOutcome::Inconclusive,
            "{text}"
        );
        assert_ne!(
            result.outcome.verdict,
            VerificationVerdict::Passed,
            "{text}"
        );
    }
    desk.deployment.shutdown().await;
}

/// Span order and attributes are judged on the stored trace: a draft
/// recorded before its lookup fails both order requirements while its
/// `policy_id` still passes, and a lookup of another policy fails only the
/// attribute requirement.
///
/// # Panics
/// Panics when a review is refused or a trace outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn trace_check_judges_span_order_and_attributes() {
    let desk = PolicyDesk::start().await;
    let reversed = desk
        .record_trace(["draft-answer", "lookup-policy"], "refunds")
        .await;
    let shipping = desk
        .record_trace(["lookup-policy", "draft-answer"], "shipping")
        .await;

    let cases = [
        (
            reversed,
            [
                RequirementOutcome::Failed,
                RequirementOutcome::Failed,
                RequirementOutcome::Passed,
            ],
        ),
        (
            shipping,
            [
                RequirementOutcome::Passed,
                RequirementOutcome::Passed,
                RequirementOutcome::Failed,
            ],
        ),
    ];
    for (trace, expected) in cases {
        let result = desk
            .review(review("Refunds follow our policy.", 50, Some(&trace)))
            .await;

        let judged = [
            "policy-looked-up",
            "answer-drafted-after-lookup",
            "lookup-read-refunds",
        ]
        .map(|id| requirement(&result, id).outcome);
        assert_eq!(judged, expected, "{trace}");
        assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    }
    desk.deployment.shutdown().await;
}

/// A judge whose output is not a closed Verdict, or whose pass cites a
/// location the original answer does not have, leaves `follows-policy`
/// inconclusive with its reason; neither counts as a pass.
///
/// # Panics
/// Panics when a review is refused or the judged outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn invalid_judge_verdicts_are_inconclusive() {
    let desk = PolicyDesk::start().await;

    for marker in ["JUDGE-MALFORMED", "JUDGE-FOREIGN-CITE"] {
        let result = desk
            .review(review(
                &format!("{marker}: refunds follow our policy."),
                50,
                None,
            ))
            .await;

        let judged = requirement(&result, "follows-policy");
        assert_eq!(
            judged.outcome,
            RequirementOutcome::Inconclusive,
            "{marker}: {judged:?}"
        );
        assert!(!judged.reason.is_empty(), "{marker}: it says why");
        assert_eq!(
            result.outcome.verdict,
            VerificationVerdict::Inconclusive,
            "{marker}"
        );
    }
    desk.deployment.shutdown().await;
}

/// The code check's valid failed Verdict fails `names-the-policy` citing the
/// answer; a malformed Verdict, a citation of a slot the Verifier declares
/// no input for, forbidden host access, and a check outliving the code
/// deadline are each inconclusive with their reason.
///
/// # Panics
/// Panics when a review is refused or a code-check outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn code_check_fails_with_citations_and_is_inconclusive_otherwise() {
    let desk = PolicyDesk::start().await;

    let failed = desk.review(review("Refunds are fine.", 50, None)).await;
    let named = requirement(&failed, "names-the-policy");
    assert_eq!(
        (named.outcome, &named.citations[..]),
        (RequirementOutcome::Failed, &answer_text()[..])
    );
    assert_eq!(failed.outcome.verdict, VerificationVerdict::Failed);

    for text in [
        "CODE-MALFORMED",
        "CODE-FOREIGN-CITE",
        "CODE-FORBIDDEN",
        "CODE-SPIN",
    ] {
        let result = desk.review(review(text, 50, None)).await;

        let named = requirement(&result, "names-the-policy");
        assert_eq!(
            named.outcome,
            RequirementOutcome::Inconclusive,
            "{text}: {named:?}"
        );
        assert!(!named.reason.is_empty(), "{text}: it says why");
        assert_ne!(
            result.outcome.verdict,
            VerificationVerdict::Passed,
            "{text}"
        );
    }
    desk.deployment.shutdown().await;
}

/// The hosted investigator's Card, Bifrost, and code tools each need both a
/// covering tool grant and the capability's own grant: a principal holding
/// both reads all three; one without the tool grant is denied every call
/// even with its Card and query reads; one without the Card and query reads
/// may only run code; and an exact `cards.get_by_ref` tool grant covers no
/// other tool. A denied call returns nothing to the model.
///
/// # Panics
/// Panics when a principal does not seed, an investigation is refused, or a
/// tool call differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn hosted_tools_need_the_tool_and_domain_grants() {
    let desk = PolicyDesk::start().await;
    let cases: [(&str, &[&str], &[&str]); 3] = [
        (
            "all_tools",
            &["tool:invoke", "cards:read", "bifrost_query:read"],
            &TOOLS,
        ),
        ("no_tool_grant", &["cards:read", "bifrost_query:read"], &[]),
        ("no_domain_grant", &["tool:invoke"], &[TOOLS[2]]),
    ];

    for (name, extra, allowed) in cases {
        let permissions = [&VERIFIES[..], extra].concat();
        let key = desk.deployment.scoped_key(name, &permissions).await;
        let result = desk.investigate(desk.deployment.client(&key), name).await;
        desk.assert_tools(&result, name, allowed).await;
    }
    let key = desk
        .deployment
        .granted_key("card_tool_only", &grants(json!({ "tool": TOOLS[0] })))
        .await;
    let result = desk
        .investigate(desk.deployment.client(&key), "card_tool_only")
        .await;
    desk.assert_tools(&result, "card_tool_only", &[TOOLS[0]])
        .await;
    desk.deployment.shutdown().await;
}

/// A principal that may call every tool, acting through a delegate granted
/// only the exact `cards.get_by_ref` tool, keeps only the narrower
/// authority: the Card read succeeds, the query and code calls are denied
/// with nothing returned, and the retained result names the original
/// subject and the delegate as actor.
///
/// # Panics
/// Panics when a principal does not seed, the delegation or investigation
/// is refused, a tool call differs, or the attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn delegated_tools_keep_the_narrower_authority() {
    let desk = PolicyDesk::start().await;
    let subject = desk
        .deployment
        .granted("tool_subject", &grants(json!("all")))
        .await;
    let actor = desk
        .deployment
        .granted("tool_delegate", &grants(json!({ "tool": TOOLS[0] })))
        .await;
    let token = desk
        .deployment
        .server()
        .exchange_api_key(subject.api_key().expect("the subject has a key"))
        .await
        .expect("the subject's token exchanges");
    let delegated = desk
        .deployment
        .client(
            actor
                .api_key()
                .expect("the delegate has a key")
                .expose_secret(),
        )
        .on_behalf_of(SecretString::from(token), TokenAudience::Wyrd)
        .await
        .expect("the delegate acts for the subject");

    let result = desk.investigate(delegated, "delegated").await;

    desk.assert_tools(&result, "delegated", &[TOOLS[0]]).await;
    desk.deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let attributed: Vec<Attribution> =
        Bifrost::connect_with(&desk.deployment.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as(
                "SELECT sub_principal_id, act_principal_id \
                   FROM vala.verification.results WHERE execution_id = $1",
                &[QueryParam::String(result.execution_id.to_string())],
            )
            .await
            .expect("the result reads back");
    assert_eq!(
        attributed,
        [Attribution {
            sub_principal_id: subject.id().to_string(),
            act_principal_id: actor.id().to_string(),
        }]
    );
    desk.deployment.shutdown().await;
}

/// The grants of a principal that verifies, invokes the gateway, reads Cards
/// and Bifrost, and may call the tools `tool_scope` covers: `"all"` or one
/// exact `{"tool": name}`.
fn grants(tool_scope: Value) -> [Value; 5] {
    [
        json!({ "resource": "verifier", "action": "run", "scope": "all" }),
        json!({ "resource": "gateway", "action": "invoke", "scope": "all" }),
        json!({ "resource": "tool", "action": "invoke", "scope": tool_scope }),
        json!({ "resource": "cards", "action": "read", "scope": "all" }),
        json!({ "resource": "bifrost_query", "action": "read", "scope": "all" }),
    ]
}
