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
//! a 100-dollar limit, and the judge passes every answer with a citation
//! unless the answer names a scripted case.

use std::path::{Path, PathBuf};
use std::time::Duration;

use opentelemetry::trace::TraceContextExt as _;
use serde_json::{Value, json};
use tempfile::TempDir;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::cards::CardRef;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Input, Inputs, Submission};
use wyrd_ai::{
    CallProvenance, Citation, Locator, MediaFormat, RequirementOutcome, RequirementResult,
    StepResult, StepStatus, VerificationResult, VerificationVerdict, WyrdClient,
};
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

/// The policy desk deployment: a verification-capable server whose gateway
/// deploys `gpt-4o` on the local upstream, with `policy-desk` registered and
/// hydrated and acting as its own Card-scoped key.
struct PolicyDesk {
    /// The deployment.
    deployment: Deployment,
    /// The local model upstream.
    upstream: MockServer,
    /// The hydrated bundle; kept so the state's files stay on disk.
    _bundle: TempDir,
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
            _bundle: bundle,
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

    /// Export one span named `name` as the Service, wait until its trace is
    /// stored, and return the trace ID as 32 hex characters.
    ///
    /// # Panics
    /// Panics when the span does not export or its trace is not stored.
    async fn record_trace(&self, name: &str) -> String {
        let (dispatch, provider) = service_telemetry(&self.client);
        let trace = tracing::dispatcher::with_default(&dispatch, || {
            let span = tracing::info_span!("lookup", otel.name = %name);
            span.in_scope(|| ());
            span.context().span().span_context().trace_id().to_string()
        });
        tokio::task::block_in_place(|| provider.force_flush()).expect("the span exports");
        assert!(
            !self.deployment.timeline(&trace).await.is_empty(),
            "the trace is stored"
        );
        trace
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
    let trace = desk.record_trace("lookup-policy").await;

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
        "follows-policy",
        "names-the-policy",
    ] {
        assert_eq!(
            requirement(&result, id).outcome,
            RequirementOutcome::Passed,
            "{id}"
        );
    }
    let cited = [Citation {
        slot: "answer".parse().expect("slot name"),
        locator: Locator::JsonPointer("/text".to_owned()),
    }];
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
    let guessed = desk.record_trace("guess-policy").await;

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
                    "cards/verifier_building_blocks/evidence/{name}"
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
