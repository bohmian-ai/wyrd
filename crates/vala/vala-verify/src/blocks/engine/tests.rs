//! IO-free engine behavior over scripted executors and in-memory evidence.
//!
//! Time is paused, so a block that waits forever makes its timeout or the
//! execution deadline fire at once instead of sleeping; overlap is proven
//! with barriers that only open when blocks really run together.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::TimeZone as _;
use serde_json::json;
use tokio::sync::{Barrier, Notify};
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus};
use wyrd_spec::verification::DriftWindow;
use wyrd_spec::verification::execution::Verdict;

use super::*;
use crate::InMemoryTraceSource;

/// One finished one-second span named `name` in `trace`.
///
/// Rebuilds a complete [`SpanRecord`] fixed at 2026-06-10 12:00:00-12:00:01
/// UTC with an OK status and one `gen_ai.system` attribute, so trace checks
/// read a deterministic span through the in-memory trace source.
///
/// # Panics
/// Panics when either static timestamp is not a single valid UTC instant,
/// which would mean the fixture literals themselves are wrong.
fn span(trace: TraceId, span: SpanId, name: &str) -> SpanRecord {
    let start_time = chrono::Utc
        .with_ymd_and_hms(2026, 6, 10, 12, 0, 0)
        .single()
        .expect("static timestamp is valid");
    let end_time = chrono::Utc
        .with_ymd_and_hms(2026, 6, 10, 12, 0, 1)
        .single()
        .expect("static timestamp is valid");
    let mut attributes = serde_json::Map::new();
    attributes.insert("gen_ai.system".to_owned(), json!("wyrd-test"));
    SpanRecord {
        trace_id: trace,
        span_id: span,
        parent_span_id: None,
        flags: 0,
        trace_state: String::new(),
        name: name.to_owned(),
        kind: SpanKind::Internal,
        start_time,
        end_time,
        duration_ms: 1000,
        status: SpanStatus::Ok,
        attributes,
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        scope: InstrumentationScope {
            name: "vala-verify-test".to_owned(),
            version: None,
            attributes: serde_json::Map::new(),
        },
        resource: Resource {
            service_name: "verify-test".to_owned(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
    }
}

/// What a scripted block does when called.
enum Behavior {
    /// Return these errors on successive attempts, then this output.
    Attempts(Vec<BlockError>, Value),
    /// Wait for every sibling at the barrier, then return this output.
    Together(Arc<Barrier>, Value),
    /// Never finish; notify when the call is dropped.
    Hang(Arc<Notify>),
    /// Panic with this message.
    Panic(&'static str),
}

/// Notifies when the hanging call that owns it is dropped.
struct DropSignal(Arc<Notify>);

impl Drop for DropSignal {
    /// Signal that the engine cancelled the call.
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

/// Scripted action and check executor recording every call in order.
#[derive(Default)]
struct Script {
    /// Action behavior by step ID; unscripted steps output `{}`.
    actions: HashMap<&'static str, Behavior>,
    /// Check result by requirement ID; unscripted checks error.
    checks: HashMap<&'static str, Result<CheckOutput, BlockError>>,
    /// `action:<id>` / `check:<id>` in call order.
    log: Mutex<Vec<String>>,
    /// Attempts made per step.
    attempts: Mutex<HashMap<String, usize>>,
}

impl Script {
    /// The calls made so far.
    ///
    /// # Panics
    /// Panics when a recording call panicked while holding the log lock.
    fn calls(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }

    /// Record one call.
    ///
    /// # Panics
    /// Panics when another recording call panicked while holding the log
    /// lock.
    fn record(&self, entry: String) {
        self.log.lock().expect("log").push(entry);
    }
}

#[async_trait]
impl ActionExecutor for Script {
    /// Run the step's scripted behavior.
    ///
    /// # Errors
    /// Returns the scripted error of the current attempt.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the log or attempts
    /// lock, and with the scripted message for a `Panic` step.
    ///
    /// # Cancellation
    /// A `Together` call waits at its barrier and a `Hang` call never
    /// finishes; dropping either is safe, and a dropped `Hang` call notifies
    /// its signal.
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        let id = call.id.as_str().to_owned();
        self.record(format!("action:{id}"));
        let attempt = {
            let mut attempts = self.attempts.lock().expect("attempts");
            let count = attempts.entry(id.clone()).or_default();
            *count += 1;
            *count - 1
        };
        let output = |output: &Value| ActionOutput {
            output: output.clone(),
            provenance: None,
            tool_calls: Vec::new(),
        };
        match self.actions.get(id.as_str()) {
            None => Ok(output(&json!({}))),
            Some(Behavior::Attempts(errors, value)) => match errors.get(attempt) {
                Some(error) => Err(error.clone()),
                None => Ok(output(value)),
            },
            Some(Behavior::Together(barrier, value)) => {
                barrier.wait().await;
                Ok(output(value))
            }
            Some(Behavior::Hang(dropped)) => {
                let _signal = DropSignal(Arc::clone(dropped));
                std::future::pending().await
            }
            Some(Behavior::Panic(message)) => panic!("{message}"),
        }
    }
}

#[async_trait]
impl CheckExecutor for Script {
    /// Return the requirement's scripted result.
    ///
    /// # Errors
    /// Returns the scripted error, or a terminal error when unscripted.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the log lock.
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
        self.record(format!("check:{}", call.id.as_str()));
        self.checks
            .get(call.id.as_str())
            .cloned()
            .unwrap_or_else(|| {
                Err(BlockError::Terminal {
                    reason: "unscripted check".to_owned(),
                })
            })
    }
}

/// A validated Verifier.
///
/// # Panics
/// Panics when `body` is not a valid Verifier spec.
fn verifier(body: Value) -> Arc<VerifierSpec> {
    let spec: VerifierSpec = serde_json::from_value(body).expect("Verifier");
    spec.validate().expect("valid Verifier");
    Arc::new(spec)
}

/// An engine over `script`, `traces`, and a concurrency bound.
///
/// # Panics
/// Panics when `concurrency` is zero or exceeds the semaphore's permit
/// limit.
fn engine(
    script: &Arc<Script>,
    traces: InMemoryTraceSource,
    concurrency: usize,
) -> VerificationEngine {
    VerificationEngine::new(
        Arc::clone(script) as Arc<dyn ActionExecutor>,
        Arc::clone(script) as Arc<dyn CheckExecutor>,
        Arc::new(traces),
        NonZeroUsize::new(concurrency).expect("positive"),
    )
}

/// Run `spec` with JSON inputs and a 60-second deadline.
///
/// Each key of the `inputs` object becomes a JSON evidence slot.
///
/// # Errors
/// Returns the engine's [`EngineError`] unchanged, including
/// [`EngineError::DeadlineExceeded`] once the 60-second deadline passes,
/// which cancels the running blocks.
///
/// # Panics
/// Panics when `inputs` is not an object or a key is not a valid slot name.
async fn run(
    engine: &VerificationEngine,
    spec: &Arc<VerifierSpec>,
    inputs: Value,
) -> Result<VerificationOutcome, EngineError> {
    let evidence = inputs
        .as_object()
        .expect("object")
        .iter()
        .map(|(slot, value)| {
            (
                SlotName::new(slot).expect("slot"),
                InputDeclaration::Json {
                    value: value.clone(),
                },
            )
        })
        .collect();
    engine
        .run(
            Arc::clone(spec),
            evidence,
            Instant::now() + Duration::from_secs(60),
        )
        .await
}

/// Outcome and reason of each requirement by ID.
fn outcomes(outcome: &VerificationOutcome) -> BTreeMap<&str, RequirementOutcome> {
    outcome
        .requirements
        .iter()
        .map(|result| (result.id.as_str(), result.outcome))
        .collect()
}

/// The result of one requirement.
///
/// # Panics
/// Panics when `outcome` has no requirement `id`.
fn requirement<'a>(outcome: &'a VerificationOutcome, id: &str) -> &'a RequirementResult {
    outcome
        .requirements
        .iter()
        .find(|result| result.id.as_str() == id)
        .expect("requirement result")
}

/// The result of one step.
///
/// # Panics
/// Panics when `outcome` has no step `id`.
fn step<'a>(outcome: &'a VerificationOutcome, id: &str) -> &'a StepResult {
    outcome
        .steps
        .iter()
        .find(|result| result.id.as_str() == id)
        .expect("step result")
}

/// A code check over `inputs`.
fn code_check(inputs: Value) -> Value {
    json!({"kind": "code", "entrypoint": "check", "source": {"inline": "def check(**kw): pass"}, "inputs": inputs})
}

/// A code action over `inputs`.
fn code_action(inputs: Value) -> Value {
    json!({"kind": "code", "entrypoint": "run", "source": {"inline": "def run(**kw): pass"}, "inputs": inputs})
}

/// A scripted Verdict.
///
/// # Panics
/// Panics when `citations` is not a JSON array of valid citations.
fn verdict(outcome: VerificationVerdict, citations: Value) -> CheckOutput {
    CheckOutput {
        verdict: Verdict {
            outcome,
            reason: "scripted".to_owned(),
            citations: serde_json::from_value(citations).expect("citations"),
            data: None,
        },
        actual: None,
        detail: None,
        provenance: None,
    }
}

/// A no-step Verifier judges declared inputs: a failed assertion is a
/// normal result with its actual value, an incomparable value is
/// inconclusive rather than failed, and trace checks read declared trace
/// evidence, including a trace that cannot be found.
///
/// # Panics
/// Panics when an outcome, actual value, or verdict differs.
#[tokio::test(start_paused = true)]
async fn no_step_verifier_judges_inputs_and_traces() {
    let trace = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").expect("trace");
    let absent = TraceId::from_hex("1af7651916cd43dd8448eb211c80319c").expect("trace");
    let traces = InMemoryTraceSource::new();
    traces
        .insert(
            trace,
            vec![span(
                trace,
                SpanId::from_hex("b7ad6b7169203331").expect("span id"),
                "retrieve",
            )],
        )
        .await;
    let assert = |path: &str, operator: &str, expected: Value| json!({"kind": "assert", "path": path, "operator": operator, "expected": expected});
    let trace_check = |slot: &str, expected: &str| json!({"kind": "trace", "on": format!("inputs.{slot}"), "span_selector": "$.spans[0].name", "operator": "equals", "expected": expected});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}, "trace": {"type": "trace"}, "lost": {"type": "trace"}},
        "requirements": [
            {"id": "no-guarantee", "statement": "No guarantees", "check": assert("inputs.answer.text", "not_contains", json!("guaranteed refund"))},
            {"id": "short", "statement": "Short", "check": assert("inputs.answer.words", "less_than", json!(10))},
            {"id": "typed", "statement": "Typed", "check": assert("inputs.answer.text", "greater_than", json!(1))},
            {"id": "retrieved", "statement": "Retrieved", "check": trace_check("trace", "retrieve")},
            {"id": "ranked", "statement": "Ranked", "check": trace_check("trace", "rank")},
            {"id": "lost", "statement": "Lost", "check": trace_check("lost", "retrieve")}
        ]
    }));
    let script = Arc::new(Script::default());
    let evidence = BTreeMap::from([
        (
            SlotName::new("answer").expect("slot"),
            InputDeclaration::Json {
                value: json!({"text": "A guaranteed refund applies.", "words": 4}),
            },
        ),
        (
            SlotName::new("trace").expect("slot"),
            InputDeclaration::Trace { trace_id: trace },
        ),
        (
            SlotName::new("lost").expect("slot"),
            InputDeclaration::Trace { trace_id: absent },
        ),
    ]);
    let outcome = engine(&script, traces, 4)
        .run(
            Arc::clone(&spec),
            evidence.clone(),
            Instant::now() + Duration::from_secs(60),
        )
        .await
        .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("no-guarantee", RequirementOutcome::Failed),
            ("short", RequirementOutcome::Passed),
            ("typed", RequirementOutcome::Inconclusive),
            ("retrieved", RequirementOutcome::Passed),
            ("ranked", RequirementOutcome::Failed),
            ("lost", RequirementOutcome::Inconclusive),
        ])
    );
    let failed = requirement(&outcome, "no-guarantee");
    assert_eq!(failed.actual, Some(json!("A guaranteed refund applies.")));
    assert_eq!(failed.check, spec.requirements[0].check);
    assert!(requirement(&outcome, "lost").reason.contains("not found"));
    assert_eq!(outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(outcome.evidence, evidence);
    assert!(outcome.steps.is_empty());
    assert!(
        script.calls().is_empty(),
        "deterministic checks call no executor"
    );
}

/// Context capture applies the existing rule to actual values: `hash`
/// stores the digest and `redact` stores nothing, without changing outcomes.
///
/// # Panics
/// Panics when a captured actual value differs.
#[tokio::test(start_paused = true)]
async fn context_capture_hashes_or_redacts_actual_values() {
    let script = Arc::new(Script::default());
    for (capture, expected) in [
        (
            "hash",
            Some(Value::String(
                crate::assertion::capture_actual(Some(json!(4)), ContextCapture::Hash)
                    .expect("hash")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .expect("digest"),
            )),
        ),
        ("redact", None),
    ] {
        let spec = verifier(json!({
            "inputs": {"answer": {"type": "json"}},
            "requirements": [{"id": "count", "statement": "Count", "check": {"kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": 4}}],
            "context_capture": capture
        }));
        let outcome = run(
            &engine(&script, InMemoryTraceSource::new(), 1),
            &spec,
            json!({"answer": 4}),
        )
        .await
        .expect("outcome");
        assert_eq!(outcome.verdict, VerificationVerdict::Passed);
        assert_eq!(requirement(&outcome, "count").actual, expected, "{capture}");
    }
}

/// Independent actions overlap: two steps meet at a barrier only when they
/// run together, so the bound of one concurrent block makes them miss the
/// deadline instead.
///
/// # Panics
/// Panics when the steps do not overlap under the wider bound, or overlap
/// under the bound of one.
#[tokio::test(start_paused = true)]
async fn independent_actions_overlap_within_the_concurrency_bound() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "a", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "b", "action": code_action(json!({"x": "inputs.answer"}))}
        ],
        "requirements": [{"id": "both", "statement": "Both ran", "check": {"kind": "assert", "path": "steps.b.output.ok", "operator": "equals", "expected": true}}]
    }));
    let script = || {
        let barrier = Arc::new(Barrier::new(2));
        Arc::new(Script {
            actions: HashMap::from([
                (
                    "a",
                    Behavior::Together(Arc::clone(&barrier), json!({"ok": true})),
                ),
                ("b", Behavior::Together(barrier, json!({"ok": true}))),
            ]),
            ..Script::default()
        })
    };
    let outcome = run(
        &engine(&script(), InMemoryTraceSource::new(), 2),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("overlapping steps finish");
    assert_eq!(outcome.verdict, VerificationVerdict::Passed);
    let serial = run(
        &engine(&script(), InMemoryTraceSource::new(), 1),
        &spec,
        json!({"answer": 1}),
    )
    .await;
    assert!(
        matches!(serial, Err(EngineError::DeadlineExceeded)),
        "{serial:?}"
    );
}

/// A panicking block fails the run with [`EngineError::Join`] carrying its
/// message, and the rest of its stage is dropped rather than left running.
///
/// # Panics
/// Panics when the run does not fail with the block's message or the
/// sibling block is not dropped.
#[tokio::test(start_paused = true)]
async fn a_panicking_block_fails_the_run_and_drops_its_stage() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "a", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "b", "action": code_action(json!({"x": "inputs.answer"}))}
        ],
        "requirements": [{"id": "ran", "statement": "Ran", "check": {"kind": "assert", "path": "steps.a.output.ok", "operator": "equals", "expected": true}}]
    }));
    let dropped = Arc::new(Notify::new());
    let script = Arc::new(Script {
        actions: HashMap::from([
            ("a", Behavior::Hang(Arc::clone(&dropped))),
            ("b", Behavior::Panic("block exploded")),
        ]),
        ..Script::default()
    });
    let ran = run(
        &engine(&script, InMemoryTraceSource::new(), 2),
        &spec,
        json!({"answer": 1}),
    )
    .await;
    assert!(
        matches!(&ran, Err(EngineError::Join(message)) if message == "block exploded"),
        "{ran:?}"
    );
    tokio::time::timeout(Duration::from_secs(1), dropped.notified())
        .await
        .expect("the hanging sibling is dropped");
}

/// A dependent waits for its upstream action and that step's requirements;
/// whole-Verifier requirements run once after every stage and every
/// verdict reaches the result exactly once, in plan order.
///
/// # Panics
/// Panics when calls run out of order or a verdict is missing or repeated.
#[tokio::test(start_paused = true)]
async fn dependents_wait_for_upstream_step_requirements() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "draft", "action": code_action(json!({"x": "inputs.answer"})),
             "requirements": [{"id": "draft-ok", "statement": "Draft ok", "check": code_check(json!({"draft": "steps.draft.output"}))}]},
            {"id": "review", "action": code_action(json!({"draft": "steps.draft.output", "gate": "steps.draft.requirements.draft-ok.outcome"})),
             "requirements": [{"id": "review-ok", "statement": "Review ok", "check": code_check(json!({"review": "steps.review.output"}))}]}
        ],
        "requirements": [{"id": "final", "statement": "Final", "check": code_check(json!({"review": "steps.review.output", "answer": "inputs.answer"}))}]
    }));
    let cited = json!([{"slot": "answer", "locator": {"json_pointer": "/text"}}]);
    let script = Arc::new(Script {
        actions: HashMap::from([
            (
                "draft",
                Behavior::Attempts(Vec::new(), json!({"text": "draft"})),
            ),
            (
                "review",
                Behavior::Attempts(Vec::new(), json!({"score": 5})),
            ),
        ]),
        checks: HashMap::from([
            (
                "draft-ok",
                Ok(verdict(VerificationVerdict::Passed, cited.clone())),
            ),
            (
                "review-ok",
                Ok(verdict(VerificationVerdict::Passed, cited.clone())),
            ),
            ("final", Ok(verdict(VerificationVerdict::Passed, cited))),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"text": "hi"}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        script.calls(),
        [
            "action:draft",
            "check:draft-ok",
            "action:review",
            "check:review-ok",
            "check:final"
        ]
    );
    let ids: Vec<_> = outcome
        .requirements
        .iter()
        .map(|r| (r.id.as_str(), r.step.as_ref().map(TaskId::as_str)))
        .collect();
    assert_eq!(
        ids,
        [
            ("draft-ok", Some("draft")),
            ("review-ok", Some("review")),
            ("final", None)
        ]
    );
    let steps: Vec<_> = outcome
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s.status, s.attempts))
        .collect();
    assert_eq!(
        steps,
        [
            ("draft", StepStatus::Succeeded, 1),
            ("review", StepStatus::Succeeded, 1)
        ]
    );
    assert_eq!(step(&outcome, "review").output, Some(json!({"score": 5})));
    assert_eq!(outcome.verdict, VerificationVerdict::Passed);
}

/// Each selector compares every wildcard match of a step output or input; an
/// empty selection is inconclusive `empty_selection` in step and inline
/// requirements alike, and a failing element fails the requirement.
///
/// # Panics
/// Panics when an `each` outcome differs.
#[tokio::test(start_paused = true)]
async fn each_assertion_compares_every_wildcard_match() {
    let each = |id: &str| json!({"id": format!("{id}-ok"), "statement": "No cell errors", "check": {"kind": "assert", "each": format!("steps.{id}.output.sheets[*].cells[*].error"), "operator": "is_null"}});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [{"id": "inline-ok", "statement": "No item errors", "check": {"kind": "assert", "each": "inputs.answer.items[*].error", "operator": "is_null"}}],
        "steps": [
            {"id": "clean", "action": code_action(json!({"x": "inputs.answer"})), "requirements": [each("clean")]},
            {"id": "broken", "action": code_action(json!({"x": "inputs.answer"})), "requirements": [each("broken")]},
            {"id": "empty", "action": code_action(json!({"x": "inputs.answer"})), "requirements": [each("empty")]}
        ]
    }));
    let sheets = |errors: Value| json!({"sheets": [{"cells": errors}]});
    let script = Arc::new(Script {
        actions: HashMap::from([
            (
                "clean",
                Behavior::Attempts(
                    Vec::new(),
                    sheets(json!([{"error": null}, {"error": null}])),
                ),
            ),
            (
                "broken",
                Behavior::Attempts(
                    Vec::new(),
                    sheets(json!([{"error": null}, {"error": "#REF!"}])),
                ),
            ),
            (
                "empty",
                Behavior::Attempts(Vec::new(), json!({"sheets": []})),
            ),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"items": []}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("inline-ok", RequirementOutcome::Inconclusive),
            ("clean-ok", RequirementOutcome::Passed),
            ("broken-ok", RequirementOutcome::Failed),
            ("empty-ok", RequirementOutcome::Inconclusive),
        ])
    );
    for id in ["inline-ok", "empty-ok"] {
        assert_eq!(requirement(&outcome, id).reason, "empty_selection");
    }
}

/// Intentional skips are not applicable through dependent steps, failures
/// and their blocked descendants are inconclusive, failure wins when both
/// causes reach one check, missing values never call a check, and a
/// status-only read is still judged.
///
/// # Panics
/// Panics when an outcome, step status, or call differs.
#[tokio::test(start_paused = true)]
async fn skip_causes_propagate_and_failure_wins() {
    let assert_present = |id: &str, path: &str| json!({"id": id, "statement": id, "check": {"kind": "assert", "path": path, "operator": "is_not_null"}});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "gated", "when": {"path": "inputs.answer.draft", "operator": "equals", "expected": true},
             "action": code_action(json!({"x": "inputs.answer"})),
             "requirements": [assert_present("gated-ok", "steps.gated.output")]},
            {"id": "after-gated", "action": code_action(json!({"x": "steps.gated.output"})),
             "requirements": [assert_present("after-gated-ok", "inputs.answer")]},
            {"id": "broken", "action": code_action(json!({"x": "inputs.answer"})),
             "requirements": [assert_present("broken-ok", "inputs.answer")]},
            {"id": "after-broken", "action": code_action(json!({"x": "steps.broken.output"}))},
            {"id": "transitive", "action": code_action(json!({"x": "steps.after-broken.output"})),
             "requirements": [assert_present("transitive-ok", "inputs.answer")]}
        ],
        "requirements": [
            assert_present("needs-gated", "steps.after-gated.output"),
            {"id": "mixed", "statement": "mixed", "check": code_check(json!({"a": "steps.after-gated.output", "b": "steps.transitive.output"}))},
            {"id": "status-read", "statement": "status", "check": {"kind": "assert", "path": "steps.broken.status", "operator": "equals", "expected": "failed"}},
            {"id": "skipped-status", "statement": "status", "check": {"kind": "assert", "path": "steps.after-broken.status", "operator": "equals", "expected": "skipped"}}
        ]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([(
            "broken",
            Behavior::Attempts(
                vec![BlockError::Terminal {
                    reason: "provider rejected".to_owned(),
                }],
                json!({}),
            ),
        )]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"draft": false}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("gated-ok", RequirementOutcome::NotApplicable),
            ("after-gated-ok", RequirementOutcome::NotApplicable),
            ("broken-ok", RequirementOutcome::Inconclusive),
            ("transitive-ok", RequirementOutcome::Inconclusive),
            ("needs-gated", RequirementOutcome::NotApplicable),
            ("mixed", RequirementOutcome::Inconclusive),
            ("status-read", RequirementOutcome::Passed),
            ("skipped-status", RequirementOutcome::Passed),
        ])
    );
    let statuses: BTreeMap<_, _> = outcome
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s.status))
        .collect();
    assert_eq!(
        statuses,
        BTreeMap::from([
            ("gated", StepStatus::Skipped),
            ("after-gated", StepStatus::Skipped),
            ("broken", StepStatus::Failed),
            ("after-broken", StepStatus::Skipped),
            ("transitive", StepStatus::Skipped),
        ])
    );
    assert_eq!(
        step(&outcome, "broken").reason.as_deref(),
        Some("provider rejected")
    );
    assert!(
        requirement(&outcome, "transitive-ok")
            .reason
            .contains("provider rejected")
    );
    assert!(
        !script.calls().iter().any(|call| call.starts_with("check:")),
        "no check runs on missing evidence: {:?}",
        script.calls()
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Inconclusive);
}

/// A requirement on an intentionally skipped step still sees its explicit
/// evidence gaps: reading a failed ancestor's output or an omitted required
/// slot is inconclusive with the failure's reason, never a quiet
/// not-applicable that lets `all` pass, while a requirement with no other
/// gap keeps the owner's skip. No check runs on missing evidence.
///
/// # Panics
/// Panics when an outcome, reason, verdict, or check call differs.
#[tokio::test(start_paused = true)]
async fn skipped_owner_keeps_failed_check_evidence() {
    let assert_present = |id: &str, path: &str| json!({"id": id, "statement": id, "check": {"kind": "assert", "path": path, "operator": "is_not_null"}});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}, "note": {"type": "json"}},
        "steps": [
            {"id": "broken", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "gated", "depends_on": ["broken"],
             "when": {"path": "inputs.answer.draft", "operator": "equals", "expected": true},
             "action": code_action(json!({"x": "inputs.answer"})),
             "requirements": [
                 {"id": "reads-broken", "statement": "reads-broken", "check": code_check(json!({"x": "steps.broken.output"}))},
                 assert_present("reads-missing", "inputs.note"),
                 assert_present("pure-skip", "inputs.answer")
             ]}
        ],
        "requirements": [assert_present("independent", "inputs.answer")]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([(
            "broken",
            Behavior::Attempts(
                vec![BlockError::Terminal {
                    reason: "provider rejected".to_owned(),
                }],
                json!({}),
            ),
        )]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"draft": false}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("reads-broken", RequirementOutcome::Inconclusive),
            ("reads-missing", RequirementOutcome::Inconclusive),
            ("pure-skip", RequirementOutcome::NotApplicable),
            ("independent", RequirementOutcome::Passed),
        ])
    );
    assert!(
        requirement(&outcome, "reads-broken")
            .reason
            .contains("provider rejected")
    );
    assert!(
        requirement(&outcome, "reads-missing")
            .reason
            .contains("`note` was not supplied")
    );
    assert!(
        !script.calls().iter().any(|call| call.starts_with("check:")),
        "no check runs on missing evidence: {:?}",
        script.calls()
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Inconclusive);
}

/// An elapsed deadline refuses work that would complete without
/// suspending, such as an empty Verifier or one whose only requirement
/// lacks evidence, instead of publishing a verdict outside the budget.
///
/// # Panics
/// Panics when an elapsed-deadline execution returns a verdict.
#[tokio::test(start_paused = true)]
async fn elapsed_deadline_refuses_ready_work() {
    let script = Arc::new(Script::default());
    let engine = engine(&script, InMemoryTraceSource::new(), 1);
    let missing = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [{"id": "r", "statement": "r", "check": {"kind": "assert", "path": "inputs.answer", "operator": "is_not_null"}}]
    }));
    for spec in [verifier(json!({})), missing] {
        let result = engine.run(spec, BTreeMap::new(), Instant::now()).await;
        assert!(
            matches!(result, Err(EngineError::DeadlineExceeded)),
            "{result:?}"
        );
    }
}

/// A missing required slot is inconclusive, an omitted optional slot is
/// `null` for an explicit null assertion, and a child of an omitted slot or
/// an absent output field is missing rather than `null`.
///
/// # Panics
/// Panics when an outcome differs.
#[tokio::test(start_paused = true)]
async fn missing_and_optional_evidence_resolve_honestly() {
    let assert = |id: &str, path: &str, operator: &str| json!({"id": id, "statement": id, "check": {"kind": "assert", "path": path, "operator": operator}});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}, "note": {"type": "json", "required": false}, "policy": {"type": "media", "formats": ["pdf"]}},
        "steps": [{"id": "pages", "action": {"kind": "extract", "from": "inputs.policy"},
                   "requirements": [assert("has-pages", "steps.pages.output.pages", "is_not_null")]}],
        "requirements": [
            assert("no-note", "inputs.note", "is_null"),
            assert("note-text", "inputs.note.text", "is_null"),
            assert("absent-field", "inputs.answer.missing", "is_null"),
            assert("present", "inputs.answer.text", "is_not_null")
        ]
    }));
    let script = Arc::new(Script::default());
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"text": "hi"}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("has-pages", RequirementOutcome::Inconclusive),
            ("no-note", RequirementOutcome::Passed),
            ("note-text", RequirementOutcome::Inconclusive),
            ("absent-field", RequirementOutcome::Inconclusive),
            ("present", RequirementOutcome::Passed),
        ])
    );
    assert!(
        step(&outcome, "pages")
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("`policy` was not supplied"))
    );
    assert!(
        script.calls().is_empty(),
        "extract without evidence is never called"
    );
}

/// Supplied evidence that does not conform to its declared slot is refused
/// before any block runs: an undeclared slot, a declaration of another kind,
/// a JSON value violating its slot schema, a file in an unaccepted format,
/// and a file above its slot's `max_bytes`. Conforming evidence runs.
///
/// # Panics
/// Panics when nonconforming evidence is not refused with the expected
/// error, a block runs for it, or the conforming control does not pass.
#[tokio::test(start_paused = true)]
async fn nonconforming_evidence_is_refused_before_any_block() {
    let spec = verifier(json!({
        "inputs": {
            "x": {"type": "json", "schema": {"type": "integer"}},
            "policy": {"type": "media", "formats": ["pdf"], "max_bytes": 10, "required": false}
        },
        "steps": [{"id": "draft", "action": code_action(json!({"x": "inputs.x"}))}],
        "requirements": [{"id": "x", "statement": "x", "check": {"kind": "assert", "path": "inputs.x", "operator": "is_not_null"}}]
    }));
    let file = |format: &str, size_bytes: u64| InputDeclaration::File {
        sha256: wyrd_spec::auth::Sha256Hex::digest(b"policy"),
        size_bytes,
        format: format.parse().expect("format"),
        filename: None,
    };
    let json_input = |value: Value| InputDeclaration::Json { value };
    let slot = |name: &str| SlotName::new(name).expect("slot");
    let cases = [
        (
            vec![("x", json_input(json!(1))), ("extra", json_input(json!(1)))],
            "declares no such input",
            false,
        ),
        (
            vec![(
                "x",
                InputDeclaration::Trace {
                    trace_id: TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").expect("trace"),
                },
            )],
            "not the slot's kind",
            false,
        ),
        (
            vec![("x", json_input(json!("one")))],
            "violates the slot schema",
            false,
        ),
        (
            vec![("x", json_input(json!(1))), ("policy", file("png", 5))],
            "does not accept",
            false,
        ),
        (
            vec![("x", json_input(json!(1))), ("policy", file("pdf", 11))],
            "10-byte limit",
            true,
        ),
    ];
    for (inputs, expected, too_large) in cases {
        let script = Arc::new(Script::default());
        let evidence = inputs
            .into_iter()
            .map(|(name, declaration)| (slot(name), declaration))
            .collect();
        let refused = engine(&script, InMemoryTraceSource::new(), 1)
            .run(
                Arc::clone(&spec),
                evidence,
                Instant::now() + Duration::from_secs(60),
            )
            .await;
        let ((Err(EngineError::InputInvalid(reason)), false)
        | (Err(EngineError::InputTooLarge(reason)), true)) = (&refused, too_large)
        else {
            panic!("expected a refusal containing {expected:?}: {refused:?}")
        };
        assert!(reason.contains(expected), "{reason}");
        assert!(
            script.calls().is_empty(),
            "no block ran: {:?}",
            script.calls()
        );
    }

    let script = Arc::new(Script::default());
    let outcome = engine(&script, InMemoryTraceSource::new(), 1)
        .run(
            Arc::clone(&spec),
            BTreeMap::from([
                (slot("x"), json_input(json!(1))),
                (slot("policy"), file("pdf", 10)),
            ]),
            Instant::now() + Duration::from_secs(60),
        )
        .await
        .expect("conforming evidence runs");
    assert_eq!(outcome.verdict, VerificationVerdict::Passed);
}

/// No requirements, or only not-applicable ones, is inconclusive.
///
/// # Panics
/// Panics when an empty or all-skipped Verifier does not stay inconclusive.
#[tokio::test(start_paused = true)]
async fn empty_and_all_skipped_verifiers_are_inconclusive() {
    let script = Arc::new(Script::default());
    let engine = engine(&script, InMemoryTraceSource::new(), 1);
    let empty = run(&engine, &verifier(json!({})), json!({}))
        .await
        .expect("outcome");
    assert_eq!(empty.verdict, VerificationVerdict::Inconclusive);
    let skipped = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "never", "when": {"path": "inputs.answer", "operator": "equals", "expected": 2},
                   "action": code_action(json!({"x": "inputs.answer"})),
                   "requirements": [{"id": "r", "statement": "r", "check": {"kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": 1}}]}]
    }));
    let outcome = run(&engine, &skipped, json!({"answer": 1}))
        .await
        .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([("r", RequirementOutcome::NotApplicable)])
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Inconclusive);
}

/// `all` and `pass_rate` count each applicable verdict once, keep
/// inconclusive results in the pass-rate denominator, and treat the
/// threshold as inclusive.
///
/// # Panics
/// Panics when a rollup differs from the approved rule.
#[test]
fn decide_rules_count_inconclusive_requirements() {
    use RequirementOutcome::{Failed as F, Inconclusive as I, NotApplicable as N, Passed as P};
    use VerificationVerdict::{Failed, Inconclusive, Passed};
    let rate = |threshold| Decide::PassRate { threshold };
    let mut one_of_ten = vec![P];
    one_of_ten.extend([I; 9]);
    for (rule, outcomes, expected) in [
        (Decide::All, vec![P, P, N], Passed),
        (Decide::All, vec![P, I, N], Inconclusive),
        (Decide::All, vec![P, I, F], Failed),
        (Decide::All, vec![N, N], Inconclusive),
        (Decide::All, vec![], Inconclusive),
        (rate(0.75), vec![P, P, P, F], Passed),
        (rate(0.75), vec![P, P, F, F], Failed),
        (rate(0.75), vec![P, P, I, F], Inconclusive),
        (rate(0.5), vec![P, F, N, N], Passed),
        (rate(1.0), vec![P, P, N], Passed),
        (rate(0.5), one_of_ten.clone(), Inconclusive),
        (rate(0.1), one_of_ten, Passed),
        (rate(0.5), vec![N], Inconclusive),
    ] {
        assert_eq!(
            decide(rule, outcomes.clone().into_iter()),
            expected,
            "{rule:?} {outcomes:?}"
        );
    }
}

/// Retryable failures spend the retry budget, terminal failures do not
/// retry, and attempts stay visible whether the step finally succeeds or
/// fails.
///
/// # Panics
/// Panics when an attempt count, status, or reason differs.
#[tokio::test(start_paused = true)]
async fn retries_spend_the_budget_and_keep_attempts_visible() {
    let retryable = || BlockError::Retryable {
        reason: "provider unavailable".to_owned(),
    };
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "recovers", "retries": 2, "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "exhausts", "retries": 1, "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "terminal", "retries": 3, "action": code_action(json!({"x": "inputs.answer"}))}
        ]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([
            (
                "recovers",
                Behavior::Attempts(vec![retryable(), retryable()], json!({"ok": true})),
            ),
            (
                "exhausts",
                Behavior::Attempts(vec![retryable(), retryable()], json!({})),
            ),
            (
                "terminal",
                Behavior::Attempts(
                    vec![BlockError::Terminal {
                        reason: "malformed output".to_owned(),
                    }],
                    json!({}),
                ),
            ),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let steps: BTreeMap<_, _> = outcome
        .steps
        .iter()
        .map(|s| (s.id.as_str(), (s.status, s.attempts, s.reason.clone())))
        .collect();
    assert_eq!(
        steps,
        BTreeMap::from([
            ("recovers", (StepStatus::Succeeded, 3, None)),
            (
                "exhausts",
                (
                    StepStatus::Failed,
                    2,
                    Some("provider unavailable".to_owned())
                )
            ),
            (
                "terminal",
                (StepStatus::Failed, 1, Some("malformed output".to_owned()))
            ),
        ])
    );
}

/// A step timeout ends and retries a hung attempt, the expired step fails
/// as `timed out` with its attempts, and an unrelated requirement still
/// finishes.
///
/// # Panics
/// Panics when the timed-out step or the unrelated requirement differs.
#[tokio::test(start_paused = true)]
async fn step_timeouts_fail_the_step_and_let_unrelated_work_finish() {
    let hung = Arc::new(Notify::new());
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "slow", "timeout_seconds": 5, "retries": 1, "action": code_action(json!({"x": "inputs.answer"})),
                   "requirements": [{"id": "slow-ok", "statement": "s", "check": {"kind": "assert", "path": "steps.slow.output", "operator": "is_not_null"}}]}],
        "requirements": [{"id": "unrelated", "statement": "u", "check": {"kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": 1}}]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([("slow", Behavior::Hang(Arc::clone(&hung)))]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let slow = step(&outcome, "slow");
    assert_eq!(
        (slow.status, slow.attempts, slow.reason.as_deref()),
        (StepStatus::Failed, 2, Some("timed out"))
    );
    assert_eq!(slow.duration_ms, 10_000);
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("slow-ok", RequirementOutcome::Inconclusive),
            ("unrelated", RequirementOutcome::Passed)
        ])
    );
}

/// The execution deadline is an execution error, not a fabricated check
/// failure, and it cancels the running action.
///
/// # Panics
/// Panics when the deadline does not surface or the hung call is not dropped.
#[tokio::test(start_paused = true)]
async fn execution_deadline_cancels_running_work() {
    let dropped = Arc::new(Notify::new());
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "hung", "action": code_action(json!({"x": "inputs.answer"}))}]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([("hung", Behavior::Hang(Arc::clone(&dropped)))]),
        ..Script::default()
    });
    let result = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &spec,
        json!({"answer": 1}),
    )
    .await;
    assert!(
        matches!(result, Err(EngineError::DeadlineExceeded)),
        "{result:?}"
    );
    tokio::time::timeout(Duration::from_secs(1), dropped.notified())
        .await
        .expect("the hung action is dropped on cancellation");
}

/// Agent and code passes need a citation of evidence the call could reach;
/// unreachable citations are dropped, a citation-less pass is inconclusive,
/// a check error is inconclusive with its reason, and a failed verdict is
/// kept as data.
///
/// # Panics
/// Panics when a citation outcome differs.
#[tokio::test(start_paused = true)]
async fn hosted_check_passes_need_reachable_citations() {
    let check = |id: &str| json!({"id": id, "statement": id, "check": code_check(json!({"answer": "inputs.answer", "claim": "requirement.statement"}))});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}, "policy": {"type": "json"}},
        "requirements": [check("cited"), check("uncited"), check("elsewhere"), check("errored"), check("rejected")]
    }));
    let answer = json!([{"slot": "answer", "locator": {"json_pointer": "/text"}}]);
    let policy = json!([{"slot": "policy", "locator": {"page": 1}}]);
    let script = Arc::new(Script {
        checks: HashMap::from([
            (
                "cited",
                Ok(verdict(VerificationVerdict::Passed, answer.clone())),
            ),
            (
                "uncited",
                Ok(verdict(VerificationVerdict::Passed, json!([]))),
            ),
            (
                "elsewhere",
                Ok(verdict(VerificationVerdict::Passed, policy)),
            ),
            (
                "errored",
                Err(BlockError::Terminal {
                    reason: "invalid verdict".to_owned(),
                }),
            ),
            (
                "rejected",
                Ok(verdict(VerificationVerdict::Failed, json!([]))),
            ),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"text": "hi"}, "policy": {}}),
    )
    .await
    .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("cited", RequirementOutcome::Passed),
            ("uncited", RequirementOutcome::Inconclusive),
            ("elsewhere", RequirementOutcome::Inconclusive),
            ("errored", RequirementOutcome::Inconclusive),
            ("rejected", RequirementOutcome::Failed),
        ])
    );
    assert_eq!(requirement(&outcome, "cited").citations.len(), 1);
    assert!(requirement(&outcome, "elsewhere").citations.is_empty());
    assert_eq!(requirement(&outcome, "errored").reason, "invalid verdict");
}

/// A one-column numeric sample batch.
fn samples(
    column: &str,
    values: impl IntoIterator<Item = f64>,
) -> arrow::record_batch::RecordBatch {
    use arrow::array::Float64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    let schema = Schema::new(vec![Field::new(column, DataType::Float64, true)]);
    let values: Float64Array = values.into_iter().collect::<Vec<_>>().into();
    arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(values)])
        .expect("sample batch")
}

/// A drift requirement over `inputs.<slot>` with the given method fields.
///
/// # Panics
/// Panics when `method` is not a JSON object.
fn drift_requirement(id: &str, slot: &str, method: Value) -> Value {
    let mut check = json!({"kind": "drift", "on": format!("inputs.{slot}"), "condition": {"kind": "Statistical"}});
    check
        .as_object_mut()
        .expect("object")
        .extend(method.as_object().expect("object").clone());
    json!({"id": id, "statement": id, "check": check})
}

/// Drift requirements score through `vala-drift`: Custom compares the
/// window mean, PSI scores against fitted state and keeps the report as
/// detail, and missing samples, unready baselines, and too few rows are
/// inconclusive with the scorer's reason, all with drift provenance.
///
/// # Panics
/// Panics when a drift outcome, detail, or provenance differs.
#[tokio::test(start_paused = true)]
async fn drift_requirements_score_through_vala_drift() {
    let custom = |id: &str, baseline: f64| {
        drift_requirement(
            id,
            "latency",
            json!({"method": "Custom", "signal": {"kind": "Metric", "name": "ms"},
            "profile": {"kind": "Custom", "metric_name": "ms", "baseline_value": baseline, "alert_threshold": 5.0}}),
        )
    };
    let psi = |id: &str, slot: &str| {
        drift_requirement(
            id,
            slot,
            json!({"method": "Psi",
            "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "name": "train", "version": "1.0.0"}, "features": ["x"]},
            "profile": {"kind": "Psi", "binning_strategy": {"kind": "EqualWidth", "n_bins": 4}, "threshold": {"kind": "Fixed", "value": 0.2}}}),
        )
    };
    let spec = verifier(json!({
        "inputs": {
            "latency": {"type": "samples", "features": ["ms"]},
            "shifted": {"type": "samples", "features": ["x"]},
            "few": {"type": "samples", "features": ["x"]},
            "unrecorded": {"type": "samples", "features": ["x"]}
        },
        "requirements": [
            custom("steady", 100.0), custom("slow", 50.0),
            psi("psi-shift", "shifted"), psi("psi-unready", "shifted"), psi("psi-few", "few"), psi("psi-none", "unrecorded")
        ]
    }));
    let check = |id: &str| match &spec
        .requirements
        .iter()
        .find(|r| r.id.as_str() == id)
        .expect("requirement")
        .check
    {
        Check::Drift(check) => check.spec.clone(),
        other => panic!("not a drift check: {other:?}"),
    };
    let baseline = vala_drift::fit_baseline(
        &samples("x", (0..200).map(|i| f64::from(i % 100))),
        &check("psi-shift"),
    )
    .expect("fitted baseline");
    let drift = DriftChecks::new(
        None,
        BTreeMap::from([
            (TaskId::new("psi-shift").expect("id"), baseline.clone()),
            (TaskId::new("psi-few").expect("id"), baseline),
        ]),
        BTreeMap::from([
            (
                SlotName::new("latency").expect("slot"),
                samples("ms", [99.0, 101.0, 100.0]),
            ),
            (
                SlotName::new("shifted").expect("slot"),
                samples("x", (0..200).map(|i| f64::from(75 + i % 25))),
            ),
            (
                SlotName::new("few").expect("slot"),
                samples("x", (0..10).map(f64::from)),
            ),
        ]),
    );
    let script = Arc::new(Script::default());
    let outcome = engine(&script, InMemoryTraceSource::new(), 4)
        .with_drift(drift)
        .run(
            Arc::clone(&spec),
            BTreeMap::new(),
            Instant::now() + Duration::from_secs(60),
        )
        .await
        .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("steady", RequirementOutcome::Passed),
            ("slow", RequirementOutcome::Failed),
            ("psi-shift", RequirementOutcome::Failed),
            ("psi-unready", RequirementOutcome::Inconclusive),
            ("psi-few", RequirementOutcome::Inconclusive),
            ("psi-none", RequirementOutcome::Inconclusive),
        ])
    );
    let shifted = requirement(&outcome, "psi-shift");
    assert!(
        matches!(shifted.detail, Some(RequirementDetail::Drift(_))),
        "{shifted:?}"
    );
    assert!(
        requirement(&outcome, "psi-unready")
            .reason
            .contains("not ready")
    );
    assert!(
        requirement(&outcome, "psi-none")
            .reason
            .contains("no samples")
    );
    assert!(
        outcome
            .requirements
            .iter()
            .all(|r| r.provenance == Some(CallProvenance::Drift { window: None })),
        "every drift result names its window"
    );
    assert!(script.calls().is_empty());
}

/// A caller-scored drift report maps without samples or fitted state, so
/// server-side window aggregates reach the same requirement projection.
///
/// # Panics
/// Panics when the pre-scored report is not the requirement's outcome.
#[tokio::test(start_paused = true)]
async fn drift_requirement_maps_caller_scored_report() {
    let spec = verifier(json!({
        "inputs": {"current": {"type": "samples", "features": ["x"]}},
        "requirements": [drift_requirement(
            "psi-shift",
            "current",
            json!({"method": "Psi",
            "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "name": "train", "version": "1.0.0"}, "features": ["x"]},
            "profile": {"kind": "Psi", "binning_strategy": {"kind": "EqualWidth", "n_bins": 4}, "threshold": {"kind": "Fixed", "value": 0.2}}}),
        )]
    }));
    let report: vala_drift::DriftReport = serde_json::from_value(json!({
        "method": "Psi",
        "features": {"x": {"feature": "x", "score": 0.9, "threshold": 0.2, "verdict": "Drift"}},
        "verdict": "Drift"
    }))
    .expect("report");
    let drift = DriftChecks::default().with_report(TaskId::new("psi-shift").expect("id"), report);
    let script = Arc::new(Script::default());
    let outcome = engine(&script, InMemoryTraceSource::new(), 1)
        .with_drift(drift)
        .run(
            spec,
            BTreeMap::new(),
            Instant::now() + Duration::from_secs(60),
        )
        .await
        .expect("outcome");
    let shifted = requirement(&outcome, "psi-shift");
    assert_eq!(shifted.outcome, RequirementOutcome::Failed);
    assert_eq!(shifted.reason, "1 of 1 features drifted (Drift)");
    assert!(matches!(shifted.detail, Some(RequirementDetail::Drift(_))));
}

/// A Custom drift window with a null measurement of its configured metric
/// is unscored and inconclusive, keeping its window and empty report, rather
/// than passing on the non-null mean; nulls in unrelated columns do not
/// matter, complete windows still pass or fail, and a missing metric column
/// is still the scorer's refusal.
///
/// # Panics
/// Panics when a Custom outcome, detail, or provenance differs.
#[tokio::test(start_paused = true)]
async fn custom_drift_needs_complete_measurements() {
    use arrow::array::{Float64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use chrono::{TimeZone, Utc};
    let custom = |id: &str, slot: &str, baseline: f64| {
        drift_requirement(
            id,
            slot,
            json!({"method": "Custom", "signal": {"kind": "Metric", "name": "ms"},
            "profile": {"kind": "Custom", "metric_name": "ms", "baseline_value": baseline, "alert_threshold": 5.0}}),
        )
    };
    let spec = verifier(json!({
        "inputs": {
            "holes": {"type": "samples", "features": ["ms"]},
            "noted": {"type": "samples", "features": ["ms"]},
            "renamed": {"type": "samples", "features": ["ms"]}
        },
        "requirements": [
            custom("holes", "holes", 100.0), custom("noted", "noted", 100.0),
            custom("noted-slow", "noted", 50.0), custom("renamed", "renamed", 100.0)
        ]
    }));
    let schema = Schema::new(vec![
        Field::new("ms", DataType::Float64, true),
        Field::new("note", DataType::Utf8, true),
    ]);
    let noted = arrow::record_batch::RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(Float64Array::from(vec![99.0, 101.0])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
        ],
    )
    .expect("noted batch");
    let holes = arrow::record_batch::RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("ms", DataType::Float64, true)])),
        vec![Arc::new(Float64Array::from(vec![Some(100.0), None]))],
    )
    .expect("holes batch");
    let window = DriftWindow {
        start: Utc
            .with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
            .single()
            .expect("start"),
        end: Utc
            .with_ymd_and_hms(2026, 10, 2, 0, 0, 0)
            .single()
            .expect("end"),
    };
    let drift = DriftChecks::new(
        Some(window),
        BTreeMap::new(),
        BTreeMap::from([
            (SlotName::new("holes").expect("slot"), holes),
            (SlotName::new("noted").expect("slot"), noted),
            (
                SlotName::new("renamed").expect("slot"),
                samples("latency", [100.0]),
            ),
        ]),
    );
    let script = Arc::new(Script::default());
    let outcome = engine(&script, InMemoryTraceSource::new(), 4)
        .with_drift(drift)
        .run(
            Arc::clone(&spec),
            BTreeMap::new(),
            Instant::now() + Duration::from_secs(60),
        )
        .await
        .expect("outcome");
    assert_eq!(
        outcomes(&outcome),
        BTreeMap::from([
            ("holes", RequirementOutcome::Inconclusive),
            ("noted", RequirementOutcome::Passed),
            ("noted-slow", RequirementOutcome::Failed),
            ("renamed", RequirementOutcome::Inconclusive),
        ])
    );
    assert_eq!(outcome.verdict, VerificationVerdict::Failed);
    let holes = requirement(&outcome, "holes");
    assert!(
        matches!(&holes.detail, Some(RequirementDetail::Drift(report)) if report.features.is_empty()),
        "{holes:?}"
    );
    assert!(
        requirement(&outcome, "renamed").reason.contains("ms"),
        "the scorer's missing-feature refusal is kept"
    );
    assert!(
        outcome.requirements.iter().all(|r| r.provenance
            == Some(CallProvenance::Drift {
                window: Some(window)
            })),
        "every drift result names its window"
    );
}

/// A block refused process capacity stops the whole execution busy: a busy
/// action spends none of its authored retries and leaves no step result,
/// and a busy check is never judged inconclusive. A retryable failure keeps
/// its authored retry.
///
/// # Panics
/// Panics when a busy block is retried, judged, or settled, or a retryable
/// failure loses its retry.
#[tokio::test(start_paused = true)]
async fn capacity_refusal_stops_the_execution_without_judging() {
    let busy = || BlockError::Busy {
        reason: "no worker".to_owned(),
    };
    let stepped = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "draft", "action": code_action(json!({"x": "inputs.answer"})), "retries": 2}]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([(
            "draft",
            Behavior::Attempts(vec![busy(), busy(), busy()], json!({})),
        )]),
        ..Script::default()
    });
    let refused = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &stepped,
        json!({"answer": "a"}),
    )
    .await;
    assert!(
        matches!(refused, Err(EngineError::Busy(ref reason)) if reason == "no worker"),
        "{refused:?}"
    );
    assert_eq!(
        script.calls(),
        ["action:draft"],
        "a busy action is not retried"
    );

    let checked = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [{"id": "judged", "statement": "Judged", "check": code_check(json!({"x": "inputs.answer"}))}]
    }));
    let script = Arc::new(Script {
        checks: HashMap::from([("judged", Err(busy()))]),
        ..Script::default()
    });
    let refused = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &checked,
        json!({"answer": "a"}),
    )
    .await;
    assert!(
        matches!(refused, Err(EngineError::Busy(_))),
        "a busy check is not judged: {refused:?}"
    );

    let script = Arc::new(Script {
        actions: HashMap::from([(
            "draft",
            Behavior::Attempts(
                vec![BlockError::Retryable {
                    reason: "provider".to_owned(),
                }],
                json!({}),
            ),
        )]),
        ..Script::default()
    });
    let retried = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &stepped,
        json!({"answer": "a"}),
    )
    .await
    .expect("a retryable failure keeps its retry");
    assert_eq!(script.calls(), ["action:draft", "action:draft"]);
    assert_eq!(step(&retried, "draft").attempts, 2);
}

/// Every action attempt and check is timed into the block histogram under
/// its block and kind: a judged assertion and a succeeded action record
/// `ok`, and a check whose executor failed records `error`.
///
/// # Panics
/// Panics when the outcome fails or the rendered exposition lacks a series.
#[tokio::test(start_paused = true)]
async fn every_block_is_timed_by_block_kind_and_outcome() {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [
            {"id": "yes", "statement": "Answered yes", "check": {"kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": "yes"}},
            {"id": "coded", "statement": "Code judged", "check": code_check(json!({"x": "inputs.answer"}))}
        ],
        "steps": [{"id": "work", "action": code_action(json!({"x": "inputs.answer"}))}]
    }));
    let script = Arc::new(Script::default());
    run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": "yes"}),
    )
    .await
    .expect("outcome");
    let text = handle.render();
    for series in [
        r#"block="check",kind="assert",outcome="ok""#,
        r#"block="check",kind="code",outcome="error""#,
        r#"block="action",kind="code",outcome="ok""#,
    ] {
        assert!(
            text.contains(&format!(
                "wyrd_verification_block_duration_seconds_count{{{series}}} 1"
            )),
            "{series}: {text}"
        );
    }
}

/// The execution's block clock records one interval per action attempt and
/// check, so the host can subtract the Verifier's own time from the whole
/// execution.
///
/// # Panics
/// Panics when the outcome fails or the clock holds another count.
#[tokio::test(start_paused = true)]
async fn block_clock_records_every_block_once() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [
            {"id": "yes", "statement": "Answered yes", "check": {"kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": "yes"}}
        ],
        "steps": [{"id": "work", "action": code_action(json!({"x": "inputs.answer"}))}]
    }));
    let script = Arc::new(Script::default());
    let clock = super::BlockClock::default();
    run(
        &engine(&script, InMemoryTraceSource::new(), 4).with_clock(clock.clone()),
        &spec,
        json!({"answer": "yes"}),
    )
    .await
    .expect("outcome");
    let intervals = clock.intervals();
    assert_eq!(intervals.len(), 2, "one action and one check");
    assert!(intervals.iter().all(|(start, end)| start <= end));
}

/// A trace source counting fetches per trace: `landed` returns its spans,
/// every other trace has not landed yet, and each fetch yields once so
/// concurrent checks really overlap the read.
#[derive(Default)]
struct CountingTraces {
    /// Spans of every landed trace.
    landed: HashMap<TraceId, Arc<Vec<SpanRecord>>>,
    /// Fetches made per trace.
    fetches: Mutex<HashMap<TraceId, usize>>,
}

impl CountingTraces {
    /// Fetches of `trace` so far.
    ///
    /// # Panics
    /// Panics when a fetch panicked while holding the count lock.
    fn fetches(&self, trace: TraceId) -> usize {
        self.fetches
            .lock()
            .expect("fetches")
            .get(&trace)
            .copied()
            .unwrap_or(0)
    }
}

#[async_trait]
impl TraceSource for CountingTraces {
    /// Count the fetch, yield, then return the landed spans.
    ///
    /// # Errors
    /// Returns [`TraceUnavailable::NotYetLanded`] for a trace not landed.
    ///
    /// # Panics
    /// Panics when another fetch panicked while holding the count lock.
    async fn fetch(
        &self,
        trace_id: TraceId,
        _deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        *self
            .fetches
            .lock()
            .expect("fetches")
            .entry(trace_id)
            .or_default() += 1;
        tokio::task::yield_now().await;
        self.landed
            .get(&trace_id)
            .cloned()
            .ok_or(TraceUnavailable::NotYetLanded { trace_id })
    }
}

/// A Verifier over trace slots `a`, `b` (naming the same trace as `a`), and
/// `late`: two concurrent checks on `a`, one on `b`, and one on `late`.
///
/// # Panics
/// Panics when the fixture is not a valid Verifier spec, as [`verifier`]
/// does.
fn shared_trace_verifier() -> Arc<VerifierSpec> {
    let check = |slot: &str, selector: &str, expected: Value| json!({"kind": "trace", "on": format!("inputs.{slot}"), "span_selector": selector, "operator": "equals", "expected": expected});
    verifier(json!({
        "inputs": {"a": {"type": "trace"}, "b": {"type": "trace"}, "late": {"type": "trace"}},
        "requirements": [
            {"id": "first", "statement": "Retrieves first", "check": check("a", "$.spans[0].name", json!("retrieve"))},
            {"id": "second", "statement": "Ranks second", "check": check("a", "$.spans[1].name", json!("rank"))},
            {"id": "same", "statement": "Same trace", "check": check("b", "$.spans[1].name", json!("rank"))},
            {"id": "late", "statement": "Late trace", "check": check("late", "$.spans[0].name", json!("retrieve"))}
        ]
    }))
}

/// Trace evidence of `shared_trace_verifier` over `landed` and `late`.
///
/// # Panics
/// Panics when a slot name is invalid.
fn trace_evidence(landed: TraceId, late: TraceId) -> BTreeMap<SlotName, InputDeclaration> {
    [("a", landed), ("b", landed), ("late", late)]
        .into_iter()
        .map(|(slot, trace_id)| {
            (
                SlotName::new(slot).expect("slot"),
                InputDeclaration::Trace { trace_id },
            )
        })
        .collect()
}

/// Concurrent checks and different slots naming one trace share one read
/// per attempt and judge the same collection, a different trace is read
/// separately, and a new run reads afresh.
///
/// # Panics
/// Panics when a fetch count or outcome differs.
#[tokio::test(start_paused = true)]
async fn trace_checks_share_one_attempt_read() {
    let landed = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").expect("trace");
    let late = TraceId::from_hex("1af7651916cd43dd8448eb211c80319c").expect("trace");
    let spans = vec![
        span(
            landed,
            SpanId::from_hex("b7ad6b7169203331").expect("span"),
            "retrieve",
        ),
        span(
            landed,
            SpanId::from_hex("b7ad6b7169203332").expect("span"),
            "rank",
        ),
    ];
    let traces = Arc::new(CountingTraces {
        landed: HashMap::from([(landed, Arc::new(spans))]),
        ..CountingTraces::default()
    });
    let script = Arc::new(Script::default());
    let engine = VerificationEngine::new(
        Arc::clone(&script) as Arc<dyn ActionExecutor>,
        Arc::clone(&script) as Arc<dyn CheckExecutor>,
        Arc::clone(&traces) as Arc<dyn TraceSource>,
        NonZeroUsize::new(4).expect("positive"),
    );
    for attempt in 1..=2 {
        let outcome = engine
            .run(
                shared_trace_verifier(),
                trace_evidence(landed, late),
                Instant::now() + Duration::from_secs(60),
            )
            .await
            .expect("outcome");
        assert_eq!(
            outcomes(&outcome),
            BTreeMap::from([
                ("first", RequirementOutcome::Passed),
                ("second", RequirementOutcome::Passed),
                ("same", RequirementOutcome::Passed),
                ("late", RequirementOutcome::Inconclusive),
            ])
        );
        assert_eq!(
            traces.fetches(landed),
            attempt,
            "one read of the landed trace"
        );
        assert_eq!(traces.fetches(late), attempt, "one read of the late trace");
    }
}

/// Only a deferring engine turns a not-yet-landed trace into
/// [`EngineError::EvidenceUnavailable`]; otherwise it is an inconclusive
/// requirement under the normal decision rule, and another read failure
/// (here a trace the source knows is absent) is never deferred.
///
/// # Panics
/// Panics when an engine result differs.
#[tokio::test(start_paused = true)]
async fn unlanded_traces_defer_only_a_deferring_engine() {
    let landed = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").expect("trace");
    let late = TraceId::from_hex("1af7651916cd43dd8448eb211c80319c").expect("trace");
    let script = Arc::new(Script::default());
    let engine = |traces: Arc<dyn TraceSource>| {
        VerificationEngine::new(
            Arc::clone(&script) as Arc<dyn ActionExecutor>,
            Arc::clone(&script) as Arc<dyn CheckExecutor>,
            traces,
            NonZeroUsize::new(4).expect("positive"),
        )
    };
    let deadline = || Instant::now() + Duration::from_secs(60);
    let deferred = engine(Arc::new(CountingTraces::default()))
        .with_unlanded_traces_deferred()
        .run(
            shared_trace_verifier(),
            trace_evidence(landed, late),
            deadline(),
        )
        .await;
    assert!(
        matches!(deferred, Err(EngineError::EvidenceUnavailable(ref reason)) if reason.contains("not yet landed")),
        "{deferred:?}"
    );
    let judged = engine(Arc::new(CountingTraces::default()))
        .run(
            shared_trace_verifier(),
            trace_evidence(landed, late),
            deadline(),
        )
        .await
        .expect("outcome");
    assert!(
        judged
            .requirements
            .iter()
            .all(|result| result.outcome == RequirementOutcome::Inconclusive
                && result.reason.contains("not yet landed")),
        "{judged:?}"
    );
    assert_eq!(judged.verdict, VerificationVerdict::Inconclusive);
    let failed = engine(Arc::new(InMemoryTraceSource::new()))
        .with_unlanded_traces_deferred()
        .run(
            shared_trace_verifier(),
            trace_evidence(landed, late),
            deadline(),
        )
        .await
        .expect("another read failure is judged, never deferred");
    assert_eq!(failed.verdict, VerificationVerdict::Inconclusive);
}
