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
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{
    Producer, ProducerInvocation, PublicationCollection, Speculation, Verdict,
};

use super::*;
use crate::InMemoryTraceSource;
use wyrd_spec::verification::state::InvocationRecord;

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
    /// Wait `delays[item]` seconds (none when absent), record
    /// `done:<call>`, then fail terminally when the item is in `fail`, or
    /// return the call's inputs as one JSON object.
    Echo {
        /// Seconds each item waits, by item index (0 without fanout).
        delays: Vec<u64>,
        /// Items that fail.
        fail: Vec<u32>,
    },
}

/// The log label of one call: its block ID, then `[item]` and `#iteration`
/// when the call has them.
fn label<B>(call: &BlockCall<'_, B>) -> String {
    let item = call
        .item_index
        .map(|item| format!("[{item}]"))
        .unwrap_or_default();
    let iteration = call
        .iteration_index
        .map(|iteration| format!("#{iteration}"))
        .unwrap_or_default();
    format!("{}{item}{iteration}", call.id.as_str())
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
    /// Each call's workspace by block ID.
    mounts: Mutex<HashMap<String, BTreeMap<String, CallInput>>>,
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

    /// Record one call's workspace.
    ///
    /// # Panics
    /// Panics when another recording call panicked while holding the
    /// mounts lock.
    fn mount(&self, id: &TaskId, workspace: &BTreeMap<String, CallInput>) {
        self.mounts
            .lock()
            .expect("mounts")
            .insert(id.as_str().to_owned(), workspace.clone());
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
        let label = label(&call);
        self.record(format!("action:{label}"));
        self.mount(call.id, &call.workspace);
        let attempt = {
            let mut attempts = self.attempts.lock().expect("attempts");
            let count = attempts.entry(label.clone()).or_default();
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
            Some(Behavior::Echo { delays, fail }) => {
                let item = call.item_index.unwrap_or(0);
                let delay = delays.get(item as usize).copied().unwrap_or(0);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                self.record(format!("done:{label}"));
                if fail.contains(&item) {
                    return Err(BlockError::Terminal {
                        reason: format!("item {item} failed"),
                    });
                }
                let inputs = call
                    .inputs
                    .iter()
                    .map(|(name, input)| (name.clone(), json(input)))
                    .collect();
                Ok(output(&Value::Object(inputs)))
            }
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
        self.record(format!("check:{}", label(&call)));
        self.mount(call.id, &call.workspace);
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

/// One Artifact holding `docs/a.txt`, `docs/b.txt`, and `docsx/c.txt`,
/// selected by exact file, `/`-bounded directory, or whole collection.
struct Kit;

#[async_trait]
impl EvidenceReader for Kit {
    /// No native media.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn media(
        &self,
        _evidence: &InputDeclaration,
    ) -> Result<skald_spec::MediaRef, BlockError> {
        Err(BlockError::Terminal {
            reason: "no media".to_owned(),
        })
    }

    /// Keep every reachable citation.
    ///
    /// # Errors
    /// Never fails.
    async fn cited(
        &self,
        reachable: &BTreeSet<SlotName>,
        citations: Vec<Citation>,
    ) -> Result<Vec<Citation>, BlockError> {
        Ok(citations
            .into_iter()
            .filter(|citation| reachable.contains(&citation.slot))
            .collect())
    }

    /// Select the kit's files.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a selection matching nothing.
    async fn select(
        &self,
        _card: &wyrd_spec::reference::CardRef,
        path: Option<&str>,
    ) -> Result<Vec<String>, BlockError> {
        let files = ["docs/a.txt", "docs/b.txt", "docsx/c.txt"];
        let selected: Vec<String> = files
            .iter()
            .filter(|file| {
                path.is_none_or(|path| {
                    **file == path
                        || file
                            .strip_prefix(path)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            })
            .map(|file| (*file).to_owned())
            .collect();
        if selected.is_empty() {
            return Err(BlockError::Terminal {
                reason: "no such file".to_owned(),
            });
        }
        Ok(selected)
    }
}

/// An inline Agent declaring `fs.read` over `workspace`.
fn reader_agent(workspace: Value) -> Value {
    json!({
        "kind": "agent",
        "agent": {
            "prompt": {"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "Judge"}], "response_format": "verdict"},
            "execution_route": {"kind": "wyrd_gateway", "deployment": "openai-primary"},
            "tool_names": ["fs.read"],
        },
        "workspace": workspace,
    })
}

/// Fixed Artifact selections mount exactly their files at selected-relative
/// paths for steps and checks alike; each file is one pinned evidence slot
/// shared by every selection reaching it, and a citation into it survives.
/// Without a reader, or for a selection matching nothing, the run is refused.
///
/// # Panics
/// Panics when a mount, slot, or refusal differs.
#[tokio::test(start_paused = true)]
async fn fixed_attachments_mount_selected_files_as_pinned_slots() {
    let kit = |path: Option<&str>| {
        let mut artifact = json!({"card": {"kind": "Artifact", "name": "kit", "version": "1.0.0"}});
        if let Some(path) = path {
            artifact["path"] = json!(path);
        }
        json!({"artifact": artifact})
    };
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "read", "action": reader_agent(json!({
            "/inputs/kit": kit(Some("docs")),
            "/inputs/one.txt": kit(Some("docs/a.txt")),
            "/inputs/answer.json": "inputs.answer",
        }))}],
        "requirements": [{"id": "judged", "statement": "s", "check": reader_agent(json!({"/inputs/all": kit(None)}))}],
    }));
    let cited = json!([{"slot": "attachment_2", "locator": {"lines": {"start": 1, "end": 1}}}]);
    let script = Arc::new(Script {
        checks: HashMap::from([("judged", Ok(verdict(VerificationVerdict::Passed, cited)))]),
        ..Script::default()
    });
    let hosted = engine(&script, InMemoryTraceSource::new(), 2).with_evidence(Arc::new(Kit));
    let outcome = run(&hosted, &spec, json!({"answer": {}}))
        .await
        .expect("outcome");
    let slot = |name: &str| BTreeSet::from([SlotName::new(name).expect("slot")]);
    let mounts: HashMap<String, BTreeMap<String, BTreeSet<SlotName>>> = script
        .mounts
        .lock()
        .expect("mounts")
        .iter()
        .map(|(id, workspace)| {
            let sources = workspace
                .iter()
                .map(|(path, input)| (path.clone(), input.sources.clone()))
                .collect();
            (id.clone(), sources)
        })
        .collect();
    assert_eq!(
        mounts["read"],
        BTreeMap::from([
            ("/inputs/answer.json".to_owned(), slot("answer")),
            ("/inputs/kit/a.txt".to_owned(), slot("attachment_0")),
            ("/inputs/kit/b.txt".to_owned(), slot("attachment_1")),
            ("/inputs/one.txt".to_owned(), slot("attachment_0")),
        ])
    );
    assert_eq!(
        mounts["judged"],
        BTreeMap::from([
            ("/inputs/all/docs/a.txt".to_owned(), slot("attachment_0")),
            ("/inputs/all/docs/b.txt".to_owned(), slot("attachment_1")),
            ("/inputs/all/docsx/c.txt".to_owned(), slot("attachment_2")),
        ])
    );
    assert_eq!(
        outcome.evidence[&SlotName::new("attachment_2").expect("slot")],
        InputDeclaration::Artifact {
            artifact: serde_json::from_value(
                json!({"kind": "Artifact", "name": "kit", "version": "1.0.0"})
            )
            .expect("card"),
            path: Some("docsx/c.txt".to_owned()),
        }
    );
    assert_eq!(
        requirement(&outcome, "judged").outcome,
        RequirementOutcome::Passed
    );

    let readerless = engine(&script, InMemoryTraceSource::new(), 2);
    assert!(matches!(
        run(&readerless, &spec, json!({"answer": {}})).await,
        Err(EngineError::InputInvalid(_))
    ));
    let missing = verifier(json!({
        "requirements": [{"id": "judged", "statement": "s", "check": reader_agent(json!({"/inputs/x": kit(Some("doc"))}))}],
    }));
    assert!(matches!(
        run(&hosted, &missing, json!({})).await,
        Err(EngineError::InputInvalid(_))
    ));
}

/// Publication references resolve to typed host mounts: a named member to
/// the successful attempt and its sources, a collection to every settled
/// producer's honest status and attempts, and a named member of a step with
/// no successful attempt to a gap that skips its consumer before it runs.
///
/// # Panics
/// Panics when a resolved mount, attempt record, or gap differs.
#[tokio::test(start_paused = true)]
async fn publication_references_resolve_to_attempts_and_statuses() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "review", "retries": 1, "action": reader_agent(json!({"/inputs/a.json": "inputs.answer"}))},
            {"id": "broken", "action": reader_agent(json!({"/inputs/a.json": "inputs.answer"}))},
            {"id": "gated", "when": {"path": "inputs.answer.go", "operator": "equals", "expected": true},
             "action": reader_agent(json!({"/inputs/a.json": "inputs.answer"}))},
            {"id": "consume", "action": reader_agent(json!({
                "/inputs/report": "steps.review.publications.report",
                "/inputs/reviews": "steps.review.publications",
                "/inputs/broken": "steps.broken.publications",
                "/inputs/gated": "steps.gated.publications",
            }))},
            {"id": "blocked", "action": reader_agent(json!({"/inputs/report": "steps.broken.publications.report"}))},
        ],
        "requirements": [{"id": "ran", "statement": "s", "check": {"kind": "assert", "path": "inputs.answer", "operator": "is_not_null"}}],
    }));
    let flaky = BlockError::Retryable {
        reason: "flaky".to_owned(),
    };
    let broken = BlockError::Terminal {
        reason: "broken".to_owned(),
    };
    let script = Arc::new(Script {
        actions: HashMap::from([
            ("review", Behavior::Attempts(vec![flaky], json!({}))),
            ("broken", Behavior::Attempts(vec![broken], json!({}))),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": {"go": false}}),
    )
    .await
    .expect("outcome");
    let consumed = script.mounts.lock().expect("mounts")["consume"].clone();
    let collection = |status, reason: Option<&str>, invocations: Vec<ProducerInvocation>| {
        CallValue::Publications {
            step: TaskId::new("x").expect("task"),
            collection: PublicationCollection {
                status,
                reason: reason.map(str::to_owned),
                speculation: Speculation::None,
                invocations,
                entries: Vec::new(),
            },
        }
    };
    let invocation = |attempt, status, reason: Option<&str>| ProducerInvocation {
        item_index: None,
        iteration_index: None,
        attempt,
        status,
        reason: reason.map(str::to_owned),
    };
    let answer = BTreeSet::from([SlotName::new("answer").expect("slot")]);
    let observed: BTreeMap<&str, (CallValue, BTreeSet<SlotName>)> = consumed
        .iter()
        .map(|(path, input)| {
            let value = match &input.value {
                CallValue::Publications { collection, .. } => CallValue::Publications {
                    step: TaskId::new("x").expect("task"),
                    collection: collection.clone(),
                },
                value => value.clone(),
            };
            (path.as_str(), (value, input.sources.clone()))
        })
        .collect();
    assert_eq!(
        observed,
        BTreeMap::from([
            (
                "/inputs/report",
                (
                    CallValue::Publication {
                        producer: Producer {
                            step_id: TaskId::new("review").expect("task"),
                            item_index: None,
                            iteration_index: None,
                            attempt: 2,
                        },
                        name: PublicationName::new("report").expect("name"),
                    },
                    answer.clone(),
                ),
            ),
            (
                "/inputs/reviews",
                (
                    collection(
                        StepStatus::Succeeded,
                        None,
                        vec![
                            invocation(1, StepStatus::Failed, Some(crate::blocks::scope::RETRIED)),
                            invocation(2, StepStatus::Succeeded, None),
                        ],
                    ),
                    answer,
                ),
            ),
            (
                "/inputs/broken",
                (
                    collection(
                        StepStatus::Failed,
                        Some("broken"),
                        vec![invocation(1, StepStatus::Failed, Some("broken"))],
                    ),
                    BTreeSet::new(),
                ),
            ),
            (
                "/inputs/gated",
                (
                    collection(
                        StepStatus::Skipped,
                        Some("when `inputs.answer.go` was false"),
                        Vec::new(),
                    ),
                    BTreeSet::new(),
                ),
            ),
        ])
    );
    let blocked = step(&outcome, "blocked");
    assert_eq!(blocked.status, StepStatus::Skipped);
    assert!(
        blocked
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("no successful attempt to publish")),
        "{blocked:?}"
    );
    assert!(!script.calls().contains(&"action:blocked".to_owned()));
}

/// An in-memory invocation record keyed by task and attempt.
#[derive(Default)]
struct Ledger {
    /// Every recorded attempt and its settlement, once it has one.
    attempts: Mutex<BTreeMap<Producer, Option<InvocationSettlement>>>,
    /// Every recorded scheduling decision, in order.
    controls: Mutex<Vec<ControlRecord>>,
}

impl Ledger {
    /// The execution state a reclaimed execution reads back.
    ///
    /// # Panics
    /// Panics when a recording call panicked while holding a lock.
    fn recorded(&self) -> ExecutionState {
        ExecutionState {
            invocations: (1_i64..)
                .zip(self.attempts.lock().expect("ledger").iter())
                .map(|(sequence, (producer, settled))| InvocationRecord {
                    sequence,
                    producer: producer.clone(),
                    settlement: settled.clone(),
                })
                .collect(),
            controls: self.controls.lock().expect("ledger").clone(),
            ..ExecutionState::default()
        }
    }

    /// Each recorded attempt and whether it settled.
    ///
    /// # Panics
    /// Panics when a recording call panicked while holding the lock.
    fn attempts(&self) -> Vec<(String, u32, bool)> {
        self.attempts
            .lock()
            .expect("ledger")
            .iter()
            .map(|(producer, settled)| {
                (
                    producer.step_id.as_str().to_owned(),
                    producer.attempt,
                    settled.is_some(),
                )
            })
            .collect()
    }
}

#[async_trait]
impl Checkpoints for Ledger {
    /// Record the attempt unsettled.
    ///
    /// # Errors
    /// Never returns an error.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the lock.
    async fn begin(&self, producer: &Producer) -> Result<(), String> {
        self.attempts
            .lock()
            .expect("ledger")
            .insert(producer.clone(), None);
        Ok(())
    }

    /// Remove the attempt.
    ///
    /// # Errors
    /// Never returns an error.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the lock.
    async fn forget(&self, producer: &Producer) -> Result<(), String> {
        self.attempts.lock().expect("ledger").remove(producer);
        Ok(())
    }

    /// Record the attempt's settlement.
    ///
    /// # Errors
    /// Never returns an error.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the lock.
    async fn settle(
        &self,
        producer: &Producer,
        settled: &InvocationSettlement,
    ) -> Result<(), String> {
        self.attempts
            .lock()
            .expect("ledger")
            .insert(producer.clone(), Some(settled.clone()));
        Ok(())
    }

    /// Record the decision.
    ///
    /// # Errors
    /// Never returns an error.
    ///
    /// # Panics
    /// Panics when another call panicked while holding the lock.
    async fn control(&self, control: &ControlRecord) -> Result<(), String> {
        self.controls.lock().expect("ledger").push(control.clone());
        Ok(())
    }
}

/// A reclaimed execution restores a settled action and its requirement
/// without calling them again, retries an interrupted attempt within the
/// step's remaining retries, and fails a step whose interrupted attempts
/// spent them without dispatching it.
///
/// # Panics
/// Panics when a block runs again, an attempt is replenished, or a restored
/// result differs.
#[tokio::test(start_paused = true)]
async fn checkpoints_restore_settled_blocks_and_count_interrupted_attempts() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "review", "action": code_action(json!({"x": "inputs.answer"})),
             "requirements": [{"id": "agrees", "statement": "the review agrees",
                 "check": code_check(json!({"review": "steps.review.output"}))}]},
            {"id": "late", "retries": 1, "depends_on": ["review"],
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "stuck", "depends_on": ["review"],
             "action": code_action(json!({"x": "inputs.answer"}))}
        ]
    }));
    let passes = || Ok(verdict(VerificationVerdict::Passed, json!([])));
    let hung = Arc::new(Notify::new());
    let crashed = Arc::new(Script {
        actions: HashMap::from([
            ("review", Behavior::Attempts(Vec::new(), json!("reviewed"))),
            ("late", Behavior::Hang(Arc::clone(&hung))),
            ("stuck", Behavior::Hang(Arc::clone(&hung))),
        ]),
        checks: HashMap::from([("agrees", passes())]),
        ..Script::default()
    });
    let ledger = Arc::new(Ledger::default());
    let first = engine(&crashed, InMemoryTraceSource::new(), 4).with_checkpoints(
        Arc::clone(&ledger) as Arc<dyn Checkpoints>,
        &ExecutionState::default(),
    );
    assert!(matches!(
        run(&first, &spec, json!({"answer": 1})).await,
        Err(EngineError::DeadlineExceeded)
    ));
    let settled = |task: &str, attempt| (task.to_owned(), attempt, true);
    let open = |task: &str, attempt| (task.to_owned(), attempt, false);
    assert_eq!(
        ledger.attempts(),
        vec![
            settled("agrees", 1),
            open("late", 1),
            settled("review", 1),
            open("stuck", 1)
        ]
    );

    let recovered = Arc::new(Script {
        actions: HashMap::from([("late", Behavior::Attempts(Vec::new(), json!("late")))]),
        ..Script::default()
    });
    let second = engine(&recovered, InMemoryTraceSource::new(), 4).with_checkpoints(
        Arc::clone(&ledger) as Arc<dyn Checkpoints>,
        &ledger.recorded(),
    );
    let outcome = run(&second, &spec, json!({"answer": 1}))
        .await
        .expect("outcome");
    assert_eq!(recovered.calls(), vec!["action:late"]);
    assert_eq!(step(&outcome, "review").output, Some(json!("reviewed")));
    assert_eq!(step(&outcome, "review").attempts, 1);
    assert_eq!(
        requirement(&outcome, "agrees").step,
        Some(step(&outcome, "review").id.clone())
    );
    assert_eq!(
        (
            step(&outcome, "late").status,
            step(&outcome, "late").attempts
        ),
        (StepStatus::Succeeded, 2)
    );
    let stuck = step(&outcome, "stuck");
    assert_eq!(
        (stuck.status, stuck.attempts, stuck.reason.as_deref()),
        (StepStatus::Failed, 1, Some(ATTEMPTS_INTERRUPTED))
    );
    assert_eq!(
        ledger.attempts(),
        vec![
            settled("agrees", 1),
            open("late", 1),
            settled("late", 2),
            settled("review", 1),
            settled("stuck", 1)
        ]
    );
}

/// An unavailable exact definition stops the whole execution: the action is
/// not retried or settled, and the check is never judged inconclusive.
///
/// # Panics
/// Panics when an unavailable block is retried, judged, or settled.
#[tokio::test(start_paused = true)]
async fn unavailable_definition_stops_the_execution_without_judging() {
    let gone = || BlockError::Unavailable {
        reason: "agent/reviewer@1 is unavailable".to_owned(),
    };
    let stepped = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [{"id": "draft", "action": code_action(json!({"x": "inputs.answer"})), "retries": 2}]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([("draft", Behavior::Attempts(vec![gone(), gone()], json!({})))]),
        ..Script::default()
    });
    let refused = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &stepped,
        json!({"answer": "a"}),
    )
    .await;
    assert!(
        matches!(refused, Err(EngineError::DefinitionUnavailable(ref reason)) if reason.contains("agent/reviewer@1")),
        "{refused:?}"
    );
    assert_eq!(script.calls(), ["action:draft"], "it is not retried");

    let checked = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "requirements": [{"id": "judged", "statement": "Judged", "check": code_check(json!({"x": "inputs.answer"}))}]
    }));
    let script = Arc::new(Script {
        checks: HashMap::from([("judged", Err(gone()))]),
        ..Script::default()
    });
    let refused = run(
        &engine(&script, InMemoryTraceSource::new(), 1),
        &checked,
        json!({"answer": "a"}),
    )
    .await;
    assert!(
        matches!(refused, Err(EngineError::DefinitionUnavailable(_))),
        "it is not judged: {refused:?}"
    );
}

/// The position of the first `entry` in `calls`.
///
/// # Panics
/// Panics when `calls` holds no `entry`.
fn at(calls: &[String], entry: &str) -> usize {
    calls
        .iter()
        .position(|call| call == entry)
        .unwrap_or_else(|| panic!("{entry} is not in {calls:?}"))
}

/// An echoing step behavior: each item waits its delay in seconds.
fn echo(delays: &[u64]) -> Behavior {
    Behavior::Echo {
        delays: delays.to_vec(),
        fail: Vec::new(),
    }
}

/// A step is released as soon as its own prerequisites complete, while an
/// unrelated step still runs. An `any_succeeded` step starts after its
/// first successful candidate; when every candidate failed or was skipped
/// it starts nothing and keeps the strongest cause; and it never waives an
/// ordinary binding.
///
/// # Panics
/// Panics when a successor waits for an unrelated step, or a step without a
/// successful candidate or with a missing binding runs.
#[tokio::test(start_paused = true)]
async fn ready_successors_do_not_wait_for_unrelated_steps() {
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "a", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "slow", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "fails", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "skipped", "when": {"path": "inputs.answer", "operator": "equals", "expected": 2},
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "after_a", "depends_on": ["a"], "action": code_action(json!({"x": "steps.a.output"}))},
            {"id": "any", "depends_on": {"any_succeeded": ["fails", "a", "slow"]},
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "none", "depends_on": {"any_succeeded": ["fails", "skipped"]},
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "strict", "depends_on": {"any_succeeded": ["a"]},
             "action": code_action(json!({"x": "steps.fails.output"}))}
        ]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([
            ("a", echo(&[1])),
            ("slow", echo(&[30])),
            (
                "fails",
                Behavior::Attempts(
                    vec![BlockError::Terminal {
                        reason: "broken".to_owned(),
                    }],
                    json!(null),
                ),
            ),
            ("after_a", echo(&[0])),
            ("any", echo(&[0])),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 8),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let calls = script.calls();
    assert!(
        at(&calls, "action:after_a") < at(&calls, "done:slow"),
        "{calls:?}"
    );
    assert!(
        at(&calls, "action:any") < at(&calls, "done:slow"),
        "{calls:?}"
    );
    assert!(
        at(&calls, "done:a") < at(&calls, "action:after_a"),
        "{calls:?}"
    );
    assert_eq!(
        step(&outcome, "after_a").output,
        Some(json!({"x": {"x": 1}}))
    );
    for unrun in ["none", "strict"] {
        assert!(
            !calls.contains(&format!("action:{unrun}")),
            "{unrun}: {calls:?}"
        );
        assert_eq!(step(&outcome, unrun).status, StepStatus::Skipped, "{unrun}");
    }
    assert_eq!(step(&outcome, "none").reason.as_deref(), Some("broken"));
}

/// An `items` fanout freezes its whole array before dispatch, keeps
/// duplicates at distinct indices, judges each item's requirements, and
/// returns the outputs in item order whatever order they finish in. An
/// empty array succeeds with `[]` and no call; a non-array or an array over
/// `max_items` fails without a call; a failed item fails the step without a
/// partial aggregate. A `count` fanout retries an item as more attempts of
/// that item, not as more items. Recovery expands the recorded items, not a
/// re-resolved array.
///
/// # Panics
/// Panics when an expansion, order, call, status, or record differs.
#[tokio::test(start_paused = true)]
async fn array_fanout_freezes_and_orders_items() {
    let fanout = |items: &str, max: u32| json!({"items": items, "max_items": max});
    let spec = verifier(json!({
        "inputs": {"doc": {"type": "json"}, "answer": {"type": "json"}},
        "steps": [
            {"id": "map", "foreach": fanout("inputs.doc.sections", 5),
             "action": code_action(json!({"s": "item", "i": "item_index"})),
             "requirements": [{"id": "per", "statement": "each item ran",
                 "check": {"kind": "assert", "path": "steps.map.output.i", "operator": "greater_than_or_equals", "expected": 0}}]},
            {"id": "empty", "foreach": fanout("inputs.doc.none", 5),
             "action": code_action(json!({"s": "item"}))},
            {"id": "wide", "foreach": fanout("inputs.doc.sections", 2),
             "action": code_action(json!({"s": "item"}))},
            {"id": "scalar", "foreach": fanout("inputs.answer", 5),
             "action": code_action(json!({"s": "item"}))},
            {"id": "partial", "foreach": fanout("inputs.doc.sections", 5),
             "action": code_action(json!({"s": "item"}))},
            {"id": "sweep", "retries": 1, "foreach": {"count": 2}, "action": code_action(json!({}))}
        ]
    }));
    let script = || {
        Arc::new(Script {
            actions: HashMap::from([
                ("map", echo(&[3, 2, 1])),
                (
                    "partial",
                    Behavior::Echo {
                        delays: Vec::new(),
                        fail: vec![1],
                    },
                ),
                (
                    "sweep",
                    Behavior::Attempts(
                        vec![BlockError::Retryable {
                            reason: "flaky".to_owned(),
                        }],
                        json!("ok"),
                    ),
                ),
            ]),
            ..Script::default()
        })
    };
    let first = script();
    let ledger = Arc::new(Ledger::default());
    let outcome = run(
        &engine(&first, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::clone(&ledger) as Arc<dyn Checkpoints>,
            &ExecutionState::default(),
        ),
        &spec,
        json!({"doc": {"sections": ["b", "a", "a"], "none": []}, "answer": 3}),
    )
    .await
    .expect("outcome");
    let calls = first.calls();
    assert_eq!(
        step(&outcome, "map").output,
        Some(json!([{"s": "b", "i": 0}, {"s": "a", "i": 1}, {"s": "a", "i": 2}]))
    );
    assert!(
        at(&calls, "done:map[2]") < at(&calls, "done:map[0]"),
        "{calls:?}"
    );
    assert_eq!(
        outcome
            .requirements
            .iter()
            .filter(|result| result.id.as_str() == "per")
            .map(|result| (result.item_index, result.outcome))
            .collect::<Vec<_>>(),
        (0..3)
            .map(|item| (Some(item), RequirementOutcome::Passed))
            .collect::<Vec<_>>()
    );
    let empty = step(&outcome, "empty");
    assert_eq!(
        (empty.status, empty.output.clone()),
        (StepStatus::Succeeded, Some(json!([])))
    );
    for (refused, reason) in [
        ("wide", "more than max_items 2"),
        ("scalar", "is not an array"),
    ] {
        let refused_step = step(&outcome, refused);
        assert_eq!(refused_step.status, StepStatus::Failed, "{refused}");
        assert!(
            refused_step
                .reason
                .as_deref()
                .is_some_and(|text| text.contains(reason)),
            "{refused_step:?}"
        );
    }
    for unrun in ["empty", "wide", "scalar"] {
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with(&format!("action:{unrun}"))),
            "{unrun}: {calls:?}"
        );
    }
    let partial = step(&outcome, "partial");
    assert_eq!(
        (partial.status, partial.output.clone()),
        (StepStatus::Failed, None)
    );
    assert!(
        partial
            .reason
            .as_deref()
            .is_some_and(|text| text.starts_with("item 1:")),
        "{partial:?}"
    );
    let sweep = step(&outcome, "sweep");
    assert_eq!(
        (sweep.status, sweep.output.clone(), sweep.attempts),
        (StepStatus::Succeeded, Some(json!(["ok", "ok"])), 4)
    );
    for item in ["action:sweep[0]", "action:sweep[1]"] {
        assert_eq!(
            calls.iter().filter(|call| *call == item).count(),
            2,
            "{calls:?}"
        );
    }
    assert!(!calls.iter().any(|call| call.starts_with("action:sweep[2]")));
    let frozen = ControlRecord::Items {
        step: TaskId::new("map").expect("task"),
        items: vec![json!("b"), json!("a"), json!("a")],
    };
    let recorded = ledger.recorded();
    assert!(
        recorded.controls.contains(&frozen),
        "{:?}",
        recorded.controls
    );

    let recovered = script();
    let controls_only = ExecutionState {
        controls: recorded.controls,
        ..ExecutionState::default()
    };
    let outcome = run(
        &engine(&recovered, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::new(Ledger::default()) as Arc<dyn Checkpoints>,
            &controls_only,
        ),
        &spec,
        json!({"doc": {"sections": ["z"], "none": []}, "answer": 3}),
    )
    .await
    .expect("recovered outcome");
    assert_eq!(
        step(&outcome, "map").output,
        Some(json!([{"s": "b", "i": 0}, {"s": "a", "i": 1}, {"s": "a", "i": 2}]))
    );
}

/// A repeat runs one iteration at a time, exposing the zero-based index
/// and the previous output (`null` first), judges each iteration's
/// requirements before `until`, and stops when `until` holds; consumers read
/// the final output. Exhausting `max_iterations` or an unevaluable `until`
/// fails the step. Composed with fanout, each item repeats independently and
/// the outputs keep item order.
///
/// # Panics
/// Panics when an iteration, order, output, or status differs.
#[tokio::test(start_paused = true)]
async fn repeat_refines_one_iteration_at_a_time() {
    let until = |path: &str, expected: Value| json!({"path": path, "operator": "greater_than_or_equals", "expected": expected});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "refine", "repeat": {"max_iterations": 4, "until": until("iteration.output.n", json!(2))},
             "action": code_action(json!({"n": "iteration.index", "prev": "iteration.previous"})),
             "requirements": [{"id": "seen", "statement": "each iteration ran",
                 "check": {"kind": "assert", "path": "steps.refine.output.n", "operator": "greater_than_or_equals", "expected": 0}}]},
            {"id": "exhaust", "repeat": {"max_iterations": 2, "until": until("iteration.output.n", json!(5))},
             "action": code_action(json!({"n": "iteration.index"}))},
            {"id": "broken", "repeat": {"max_iterations": 3, "until": {"path": "iteration.output.n", "operator": "greater_than_or_equals", "expected": "high"}},
             "action": code_action(json!({"n": "iteration.index"}))},
            {"id": "grid", "foreach": {"count": 2},
             "repeat": {"max_iterations": 3, "until": until("iteration.output.n", json!(1))},
             "action": code_action(json!({"n": "iteration.index", "i": "item_index"}))},
            {"id": "reader", "action": code_action(json!({"x": "steps.refine.output.n"}))}
        ]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([
            ("refine", echo(&[1])),
            ("exhaust", echo(&[])),
            ("broken", echo(&[])),
            ("grid", echo(&[1, 2])),
            ("reader", echo(&[])),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 8),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let calls = script.calls();
    assert_eq!(
        step(&outcome, "refine").output,
        Some(json!({"n": 2, "prev": {"n": 1, "prev": {"n": 0, "prev": null}}}))
    );
    assert!(
        at(&calls, "done:refine#0") < at(&calls, "action:refine#1"),
        "{calls:?}"
    );
    assert!(
        at(&calls, "done:refine#1") < at(&calls, "action:refine#2"),
        "{calls:?}"
    );
    assert!(!calls.contains(&"action:refine#3".to_owned()), "{calls:?}");
    assert_eq!(
        outcome
            .requirements
            .iter()
            .filter(|result| result.id.as_str() == "seen")
            .map(|result| result.iteration_index)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2)]
    );
    assert_eq!(step(&outcome, "reader").output, Some(json!({"x": 2})));
    for (failed, reason) in [
        ("exhaust", "max_iterations 2"),
        ("broken", "cannot be evaluated"),
    ] {
        let failed_step = step(&outcome, failed);
        assert_eq!(
            (failed_step.status, failed_step.output.clone()),
            (StepStatus::Failed, None)
        );
        assert!(
            failed_step
                .reason
                .as_deref()
                .is_some_and(|text| text.contains(reason)),
            "{failed_step:?}"
        );
    }
    assert!(!calls.contains(&"action:broken#1".to_owned()), "{calls:?}");
    assert_eq!(
        step(&outcome, "grid").output,
        Some(json!([{"n": 1, "i": 0}, {"n": 1, "i": 1}]))
    );
    for item in 0..2 {
        assert!(
            at(&calls, &format!("done:grid[{item}]#0"))
                < at(&calls, &format!("action:grid[{item}]#1")),
            "{calls:?}"
        );
    }
}

/// A speculative step runs while its guard is pending; its consumers wait
/// for the disposition. A true guard admits its output, a known-false guard
/// makes no call, a guard that turns false cancels running work and
/// withholds it, and an unevaluable guard blocks it as a failure. Every
/// disposition decided after the step started is recorded, and a recorded
/// disposition wins over a re-evaluated guard.
///
/// # Panics
/// Panics when a call, order, admission, or record differs.
#[tokio::test(start_paused = true)]
async fn speculative_steps_withhold_content_until_admitted() {
    let guard = |path: &str, expected: Value| json!({"path": path, "operator": "equals", "expected": expected});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "compare", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "extra", "speculative": true, "when": guard("steps.compare.output.x", json!(1)),
             "action": code_action(json!({"y": "inputs.answer"}))},
            {"id": "consumer", "action": code_action(json!({"e": "steps.extra.output"}))},
            {"id": "early", "speculative": true, "when": guard("inputs.answer", json!(99)),
             "action": code_action(json!({"y": "inputs.answer"}))},
            {"id": "dropped", "speculative": true, "when": guard("steps.compare.output.x", json!(2)),
             "action": code_action(json!({"y": "inputs.answer"}))},
            {"id": "blocked", "speculative": true, "when": guard("steps.compare.output.missing", json!(1)),
             "action": code_action(json!({"y": "inputs.answer"}))}
        ]
    }));
    let cancelled = Arc::new(Notify::new());
    let script = || {
        Arc::new(Script {
            actions: HashMap::from([
                ("compare", echo(&[5])),
                ("extra", echo(&[1])),
                ("consumer", echo(&[])),
                ("dropped", Behavior::Hang(Arc::clone(&cancelled))),
                ("blocked", echo(&[])),
            ]),
            ..Script::default()
        })
    };
    let first = script();
    let ledger = Arc::new(Ledger::default());
    let outcome = run(
        &engine(&first, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::clone(&ledger) as Arc<dyn Checkpoints>,
            &ExecutionState::default(),
        ),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let calls = first.calls();
    assert!(
        at(&calls, "done:extra") < at(&calls, "done:compare"),
        "{calls:?}"
    );
    assert!(
        at(&calls, "done:compare") < at(&calls, "action:consumer"),
        "{calls:?}"
    );
    assert_eq!(
        step(&outcome, "consumer").output,
        Some(json!({"e": {"y": 1}}))
    );
    assert!(!calls.contains(&"action:early".to_owned()), "{calls:?}");
    assert!(calls.contains(&"action:dropped".to_owned()), "{calls:?}");
    tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
        .await
        .expect("the discarded work is cancelled");
    for (id, status, attempts) in [
        ("early", StepStatus::Skipped, 0),
        ("dropped", StepStatus::Skipped, 1),
        ("blocked", StepStatus::Failed, 1),
    ] {
        let withheld = step(&outcome, id);
        assert_eq!(
            (withheld.status, withheld.output.clone(), withheld.attempts),
            (status, None, attempts),
            "{id}"
        );
    }
    let disposition = |id: &str| {
        ledger
            .recorded()
            .controls
            .iter()
            .find_map(|control| match control {
                ControlRecord::Disposition {
                    step, speculation, ..
                } if step.as_str() == id => Some(*speculation),
                _ => None,
            })
    };
    assert_eq!(disposition("extra"), Some(Speculation::Admitted));
    assert_eq!(disposition("dropped"), Some(Speculation::Discarded));
    assert_eq!(disposition("blocked"), Some(Speculation::Blocked));
    assert_eq!(disposition("early"), None);

    let recovered = script();
    let controls_only = ExecutionState {
        controls: ledger.recorded().controls,
        ..ExecutionState::default()
    };
    let outcome = run(
        &engine(&recovered, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::new(Ledger::default()) as Arc<dyn Checkpoints>,
            &controls_only,
        ),
        &spec,
        json!({"answer": 2}),
    )
    .await
    .expect("recovered outcome");
    assert_eq!(step(&outcome, "extra").output, Some(json!({"y": 2})));
    assert_eq!(step(&outcome, "dropped").status, StepStatus::Skipped);
    assert!(!recovered.calls().contains(&"action:dropped".to_owned()));

    let resumed = script();
    let outcome = run(
        &engine(&resumed, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::new(Ledger::default()) as Arc<dyn Checkpoints>,
            &ledger.recorded(),
        ),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("resumed outcome");
    for (id, status) in [
        ("dropped", StepStatus::Skipped),
        ("blocked", StepStatus::Failed),
    ] {
        let withheld = step(&outcome, id);
        assert_eq!(
            (withheld.status, withheld.attempts),
            (status, 1),
            "{id}: a recorded disposition keeps the attempt that ran"
        );
        assert!(!resumed.calls().contains(&format!("action:{id}")), "{id}");
    }
}

/// A step graph whose only cycle runs through a speculative guard and a
/// fanout's items is refused before any block runs, so every potential
/// edge is checked rather than only ordinary bindings.
///
/// # Panics
/// Panics when the cycle is not refused or a block runs.
#[tokio::test(start_paused = true)]
async fn invalid_step_controls_reject_before_dispatch() {
    let spec: VerifierSpec = serde_json::from_value(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "split", "foreach": {"items": "steps.merge.output", "max_items": 3},
             "action": code_action(json!({"s": "item"}))},
            {"id": "merge", "speculative": true,
             "when": {"path": "steps.split.output", "operator": "equals", "expected": []},
             "action": code_action(json!({"x": "inputs.answer"}))}
        ]
    }))
    .expect("Verifier");
    let refusal = spec.validate().expect_err("the cycle is refused");
    assert!(refusal.to_string().contains("steps"), "{refusal}");
    let script = Arc::new(Script::default());
    let refused = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &Arc::new(spec),
        json!({"answer": 1}),
    )
    .await;
    assert!(matches!(refused, Err(EngineError::Plan(_))), "{refused:?}");
    assert!(script.calls().is_empty());
}

/// `limits.concurrency` and `foreach.concurrency` only lower the engine's
/// bound, and fanout never multiplies it: two blocks that must run together
/// finish only when every bound reaching them admits both.
///
/// # Panics
/// Panics when a bound admits more blocks than it allows, or a wide enough
/// bound does not let them overlap.
#[tokio::test(start_paused = true)]
async fn run_and_fanout_bounds_only_lower_engine_capacity() {
    let pair = |limits: Value| {
        verifier(json!({
            "inputs": {"answer": {"type": "json"}},
            "limits": limits,
            "steps": [
                {"id": "a", "action": code_action(json!({"x": "inputs.answer"}))},
                {"id": "b", "action": code_action(json!({"x": "inputs.answer"}))}
            ]
        }))
    };
    let swept = |foreach: Value| {
        verifier(json!({
            "inputs": {"answer": {"type": "json"}},
            "steps": [{"id": "a", "foreach": foreach, "action": code_action(json!({"x": "inputs.answer"}))}]
        }))
    };
    let together = |steps: &[&'static str]| {
        let barrier = Arc::new(Barrier::new(2));
        Arc::new(Script {
            actions: steps
                .iter()
                .map(|id| (*id, Behavior::Together(Arc::clone(&barrier), json!({}))))
                .collect(),
            ..Script::default()
        })
    };
    let cases = [
        (pair(json!({"concurrency": 2})), &["a", "b"][..], 4, true),
        (pair(json!({"concurrency": 1})), &["a", "b"][..], 4, false),
        (pair(json!({"concurrency": 4})), &["a", "b"][..], 1, false),
        (swept(json!({"count": 2})), &["a"][..], 4, true),
        (
            swept(json!({"count": 2, "concurrency": 1})),
            &["a"][..],
            4,
            false,
        ),
        (
            swept(json!({"count": 2, "concurrency": 2})),
            &["a"][..],
            1,
            false,
        ),
    ];
    for (spec, steps, capacity, overlaps) in cases {
        let script = together(steps);
        let outcome = run(
            &engine(&script, InMemoryTraceSource::new(), capacity),
            &spec,
            json!({"answer": 1}),
        )
        .await;
        assert_eq!(
            outcome.is_ok(),
            overlaps,
            "{:?} with capacity {capacity}: {outcome:?}",
            spec.limits
        );
        if !overlaps {
            assert!(
                matches!(outcome, Err(EngineError::DeadlineExceeded)),
                "{outcome:?}"
            );
        }
    }
}

/// An inline Agent action or check reading the `answer` input.
fn answer_agent() -> Value {
    reader_agent(json!({"/inputs/answer.json": "inputs.answer"}))
}

/// `limits.max_agent_invocations` counts every Agent action and check
/// attempt, retries included: once spent, a later Agent action fails and an
/// Agent check is inconclusive without being called, while code blocks still
/// run. A reclaimed execution counts the attempts it already recorded, so
/// recovery never replenishes the budget.
///
/// # Panics
/// Panics when a spent budget dispatches an Agent block, a code block is
/// refused, or recovery replenishes the budget.
#[tokio::test(start_paused = true)]
async fn agent_budget_counts_retries_and_is_never_replenished() {
    let retryable = || BlockError::Retryable {
        reason: "provider unavailable".to_owned(),
    };
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "limits": {"max_agent_invocations": 3},
        "steps": [
            {"id": "first", "retries": 2, "action": answer_agent()},
            {"id": "second", "depends_on": ["first"], "action": answer_agent()},
            {"id": "plain", "depends_on": ["first"], "action": code_action(json!({"x": "inputs.answer"}))}
        ],
        "requirements": [
            {"id": "judged", "statement": "judged", "check": answer_agent()},
            {"id": "coded", "statement": "coded", "check": {"kind": "assert", "path": "steps.plain.status", "operator": "equals", "expected": "succeeded"}}
        ]
    }));
    let script = Arc::new(Script {
        actions: HashMap::from([(
            "first",
            Behavior::Attempts(vec![retryable(), retryable()], json!({})),
        )]),
        checks: HashMap::from([(
            "judged",
            Ok(verdict(VerificationVerdict::Passed, json!([]))),
        )]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 4),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let calls = script.calls();
    assert_eq!(
        calls.iter().filter(|call| *call == "action:first").count(),
        3
    );
    assert!(!calls.contains(&"action:second".to_owned()), "{calls:?}");
    assert!(!calls.contains(&"check:judged".to_owned()), "{calls:?}");
    assert!(calls.contains(&"action:plain".to_owned()), "{calls:?}");
    let second = step(&outcome, "second");
    assert_eq!(
        (second.status, second.attempts, second.reason.as_deref()),
        (StepStatus::Failed, 0, Some(BUDGET_SPENT))
    );
    let judged = requirement(&outcome, "judged");
    assert_eq!(
        (judged.outcome, judged.reason.as_str()),
        (RequirementOutcome::Inconclusive, BUDGET_SPENT)
    );
    let coded = requirement(&outcome, "coded");
    assert_eq!(coded.outcome, RequirementOutcome::Passed, "{coded:?}");

    let resumable = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "limits": {"max_agent_invocations": 2},
        "steps": [{"id": "first", "retries": 3, "action": answer_agent()}]
    }));
    let hung = Arc::new(Notify::new());
    let crashed = Arc::new(Script {
        actions: HashMap::from([("first", Behavior::Hang(hung))]),
        ..Script::default()
    });
    let ledger = Arc::new(Ledger::default());
    let interrupted = engine(&crashed, InMemoryTraceSource::new(), 4).with_checkpoints(
        Arc::clone(&ledger) as Arc<dyn Checkpoints>,
        &ExecutionState::default(),
    );
    assert!(matches!(
        run(&interrupted, &resumable, json!({"answer": 1})).await,
        Err(EngineError::DeadlineExceeded)
    ));
    let recovered = Arc::new(Script {
        actions: HashMap::from([("first", Behavior::Attempts(vec![retryable()], json!({})))]),
        ..Script::default()
    });
    let resumed = engine(&recovered, InMemoryTraceSource::new(), 4).with_checkpoints(
        Arc::clone(&ledger) as Arc<dyn Checkpoints>,
        &ledger.recorded(),
    );
    let outcome = run(&resumed, &resumable, json!({"answer": 1}))
        .await
        .expect("outcome");
    assert_eq!(recovered.calls(), ["action:first"]);
    let first = step(&outcome, "first");
    assert_eq!(
        (first.status, first.attempts, first.reason.as_deref()),
        (StepStatus::Failed, 2, Some(BUDGET_SPENT))
    );
}

/// A `when` that reads missing data, cannot compare, or finds a value of
/// another JSON type than it expects fails its step without a call rather
/// than skipping it as false, and a speculative
/// step whose true guard admits a failed action stays visible as failed and
/// fails, not skips, its consumer.
///
/// # Panics
/// Panics when an unevaluable condition skips or runs its step, or an
/// admitted failure is hidden.
#[tokio::test(start_paused = true)]
async fn condition_errors_and_admitted_failures_stay_failures() {
    let when = |path: &str, operator: &str, expected: Value| json!({"path": path, "operator": operator, "expected": expected});
    let spec = verifier(json!({
        "inputs": {"answer": {"type": "json"}},
        "steps": [
            {"id": "compare", "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "missing", "when": when("steps.compare.output.absent", "equals", json!(1)),
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "mistyped", "when": when("steps.compare.output.x", "greater_than", json!("high")),
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "unlike", "when": when("steps.compare.output.x", "equals", json!(true)),
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "extra", "speculative": true, "when": when("steps.compare.output.x", "equals", json!(1)),
             "action": code_action(json!({"x": "inputs.answer"}))},
            {"id": "consumer", "action": code_action(json!({"e": "steps.extra.output"}))}
        ]
    }));
    let ledger = Arc::new(Ledger::default());
    let script = Arc::new(Script {
        actions: HashMap::from([
            ("compare", echo(&[1])),
            (
                "extra",
                Behavior::Attempts(
                    vec![BlockError::Terminal {
                        reason: "the extra review broke".to_owned(),
                    }],
                    json!(null),
                ),
            ),
        ]),
        ..Script::default()
    });
    let outcome = run(
        &engine(&script, InMemoryTraceSource::new(), 8).with_checkpoints(
            Arc::clone(&ledger) as Arc<dyn Checkpoints>,
            &ExecutionState::default(),
        ),
        &spec,
        json!({"answer": 1}),
    )
    .await
    .expect("outcome");
    let calls = script.calls();
    for refused in ["missing", "mistyped", "unlike"] {
        assert_eq!(
            step(&outcome, refused).status,
            StepStatus::Failed,
            "{refused}"
        );
        assert!(
            !calls.contains(&format!("action:{refused}")),
            "{refused}: {calls:?}"
        );
    }
    let extra = step(&outcome, "extra");
    assert_eq!(
        (extra.status, extra.reason.as_deref()),
        (StepStatus::Failed, Some("the extra review broke"))
    );
    assert!(
        ledger.recorded().controls.iter().any(|control| matches!(
            control,
            ControlRecord::Disposition { step, speculation: Speculation::Admitted, .. }
                if step.as_str() == "extra"
        )),
        "{:?}",
        ledger.recorded().controls
    );
    assert_ne!(step(&outcome, "consumer").status, StepStatus::Succeeded);
    assert!(!calls.contains(&"action:consumer".to_owned()), "{calls:?}");
}
