//! Injected execution seams for Verifier building blocks.
//!
//! The engine resolves every reference a block binds, then hands the block to
//! an [`ActionExecutor`] (steps, which produce data) or a [`CheckExecutor`]
//! (requirement checks, which produce a [`Verdict`]). Executors receive
//! values, never references, and return outputs rather than writing shared
//! state, mirroring the stage walk.
//!
//! The engine owns the deadline and cancellation: it races each call against
//! [`BlockCall::deadline`] and drops the future to cancel it, so an executor
//! must release its resources on drop. Executors are constructed per
//! execution with the accepted caller authority; [`Position`] decides what a
//! call may do inside that authority, such as whether an Agent gets tools.
//! A [`BlockError`] is either retryable, spending the step's retry budget,
//! or terminal, making the block inconclusive at once.

pub(crate) mod agent;
mod tools;

pub use agent::{
    AGENT_PHASE_DURATION_SECONDS, AgentCardResolver, AgentExecutor, EvidenceReader,
    PromptCardResolver, block_error,
};
pub use tools::{AgentToolset, ToolSession, provider_tool_name};

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use skald_spec::{PreparedSchemas, SharedSchema};
use thiserror::Error;
use tokio::time::Instant;

use wyrd_spec::card::verifier::{Action, Check, PathSegment};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::{
    CallProvenance, InputDeclaration, InvocationSettlement, Producer, PublicationCollection,
    RequirementDetail, ToolCallRecord, Verdict,
};
use wyrd_spec::verification::state::ControlRecord;
use wyrd_spec::verification::{PublicationName, TaskId};

mod drift;
mod engine;
mod scope;

pub use drift::DriftChecks;
pub use engine::{BLOCK_DURATION_SECONDS, BlockClock, EngineError, VerificationEngine};

/// Where a block sits in the Verifier, which fixes what it may read and do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position<'a> {
    /// A step action; Agents may use their declared tools.
    Step,
    /// A step requirement's check, run right after that step succeeds.
    StepCheck {
        /// The owning step.
        step: &'a TaskId,
    },
    /// A Verifier requirement's check, run after every step settled.
    VerifierCheck,
}

/// One resolved binding value.
#[derive(Debug, Clone, PartialEq)]
pub enum CallValue {
    /// JSON data: inline input, step output, status, or verdict field.
    Json(BoundJson),
    /// Pinned original evidence a media slot names; executors fetch bytes.
    Evidence(InputDeclaration),
    /// `steps.<id>.publications.<name>`: what `producer`, the step's
    /// successful attempt, published under `name`. The host mounts it, or
    /// refuses the call before it runs when no such publication exists.
    Publication {
        /// The successful producer attempt.
        producer: Producer,
        /// The logical name.
        name: PublicationName,
    },
    /// `steps.<id>.publications`: the settled step's collection, whose
    /// `entries` the host fills from the execution's admitted publications.
    Publications {
        /// The producer step.
        step: TaskId,
        /// Its status and attempts, with no entries yet.
        collection: PublicationCollection,
    },
}

/// The JSON one binding reads, dereferencing to the selected value.
///
/// A selection naming one value of the run's pinned evidence or of a settled
/// step's output points at that value where it is held, so binding copies
/// nothing however large it is. A wildcard selection, which builds a new
/// array, and a constant or derived value, such as a step status, are owned.
#[derive(Debug, Clone)]
pub struct BoundJson(Bound);

/// Where a [`BoundJson`] value is held.
#[derive(Debug, Clone)]
enum Bound {
    /// A value the binding owns.
    Owned(Value),
    /// The value at `path` in the JSON evidence `slot`.
    Evidence {
        /// The run's pinned evidence.
        evidence: Arc<BTreeMap<SlotName, InputDeclaration>>,
        /// The JSON slot selected.
        slot: SlotName,
        /// Field and index selectors below the slot's value.
        path: Vec<PathSegment>,
    },
    /// The value at `path` in a settled step's output.
    Output {
        /// The step's output, shared with the ledger.
        output: Arc<Value>,
        /// Field and index selectors below the output.
        path: Vec<PathSegment>,
    },
}

impl BoundJson {
    /// The value at `path` in the JSON evidence `slot`, or `None` when the
    /// slot is not JSON or `path` selects nothing.
    pub(crate) fn evidence(
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        slot: &SlotName,
        path: &[PathSegment],
    ) -> Option<Self> {
        let Some(InputDeclaration::Json { value }) = evidence.get(slot) else {
            return None;
        };
        scope::find(value, path)?;
        Some(Self(Bound::Evidence {
            evidence: Arc::clone(evidence),
            slot: slot.clone(),
            path: path.to_vec(),
        }))
    }

    /// The value at `path` in a step's `output`, or `None` when `path`
    /// selects nothing.
    pub(crate) fn output(output: &Arc<Value>, path: &[PathSegment]) -> Option<Self> {
        scope::find(output, path)?;
        Some(Self(Bound::Output {
            output: Arc::clone(output),
            path: path.to_vec(),
        }))
    }
}

impl From<Value> for BoundJson {
    /// A binding that owns `value`.
    fn from(value: Value) -> Self {
        Self(Bound::Owned(value))
    }
}

impl Deref for BoundJson {
    type Target = Value;

    /// The selected value, found again by its selectors over data no one
    /// mutates.
    ///
    /// # Panics
    /// Panics when the selected value is gone, which cannot happen: a shared
    /// binding is built only once its selectors resolve, over immutable
    /// shared data.
    fn deref(&self) -> &Value {
        match &self.0 {
            Bound::Owned(value) => value,
            Bound::Evidence {
                evidence,
                slot,
                path,
            } => match evidence.get(slot) {
                Some(InputDeclaration::Json { value }) => scope::find(value, path),
                _ => None,
            }
            .expect("a bound evidence value stays selectable"),
            Bound::Output { output, path } => {
                scope::find(output, path).expect("a bound step output stays selectable")
            }
        }
    }
}

impl PartialEq for BoundJson {
    /// Bindings are equal when their selected values are.
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

/// A resolved binding plus the original input slots it derives from, so
/// citations returned through it can be checked against reachable evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct CallInput {
    /// The value.
    pub value: CallValue,
    /// Original slots behind the value; empty for constants such as
    /// `requirement.statement`.
    pub sources: BTreeSet<SlotName>,
}

/// One block invocation handed to an executor.
#[derive(Debug, Clone)]
pub struct BlockCall<'a, B> {
    /// Step or requirement ID.
    pub id: &'a TaskId,
    /// Where the block sits.
    pub position: Position<'a>,
    /// The authored action or check.
    pub block: &'a B,
    /// Resolved values by authored input name; an `extract` action binds its
    /// source as `from`.
    pub inputs: BTreeMap<String, CallInput>,
    /// Resolved workspace files by authored path; empty unless the block is
    /// an Agent with a workspace.
    pub workspace: BTreeMap<String, CallInput>,
    /// When the engine cancels this attempt.
    pub deadline: Instant,
    /// The one-based attempt this call is, counting the block's recorded
    /// attempts, so a host attributes what the attempt retains to it.
    pub attempt: u32,
    /// The zero-based fanout item this call serves, when its step fans out.
    pub item_index: Option<u32>,
    /// The zero-based repeat iteration this call serves, when its step
    /// repeats.
    pub iteration_index: Option<u32>,
}

impl<B> BlockCall<'_, B> {
    /// The invocation attempt this call is: its block, item, iteration, and
    /// attempt, the identity its retained work is attributed to.
    #[must_use]
    pub fn producer(&self) -> Producer {
        Producer {
            step_id: self.id.clone(),
            item_index: self.item_index,
            iteration_index: self.iteration_index,
            attempt: self.attempt,
        }
    }
}

/// Data one step action produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutput {
    /// The output `steps.<id>.output` reads; may embed citations.
    pub output: Value,
    /// What executed the action.
    pub provenance: Option<CallProvenance>,
    /// Tools an Agent action called, in call order.
    pub tool_calls: Vec<ToolCallRecord>,
}

/// The judgment one requirement check produced.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckOutput {
    /// Outcome, reason, and citations; the engine downgrades a citation-less
    /// pass to inconclusive.
    pub verdict: Verdict,
    /// The compared actual value, when the check has one.
    pub actual: Option<Value>,
    /// Check-specific detail such as drift statistics.
    pub detail: Option<RequirementDetail>,
    /// What executed the check.
    pub provenance: Option<CallProvenance>,
}

/// Why a block produced no output. Never a judgment about the subject.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlockError {
    /// Transient failure, such as a provider outage, a later attempt may
    /// clear.
    #[error("retryable block failure: {reason}")]
    Retryable {
        /// Bounded, secret-free cause.
        reason: String,
    },
    /// Failure another attempt cannot clear, such as malformed model output
    /// or a provider rejection.
    #[error("terminal block failure: {reason}")]
    Terminal {
        /// Bounded, secret-free cause.
        reason: String,
    },
    /// The process refused the block a shared resource, such as a Python
    /// worker, a workspace, or a gateway call it cannot serve yet, at
    /// admission.
    ///
    /// That is pressure on the host, not a failure of the block, so it spends
    /// no authored retry and is never judged: the engine stops the whole
    /// execution with [`EngineError::Busy`]
    /// so its host can run it again later.
    #[error("block refused process capacity: {reason}")]
    Busy {
        /// Bounded, secret-free cause.
        reason: String,
    },
    /// An exact definition the block requires, such as its Agent or Prompt
    /// Card, is unavailable.
    ///
    /// The block cannot run as declared, so it is never judged and spends
    /// no authored retry: the engine stops the whole execution with
    /// [`EngineError::DefinitionUnavailable`].
    #[error("a required definition is unavailable: {reason}")]
    Unavailable {
        /// Bounded, secret-free cause naming the definition.
        reason: String,
    },
}

impl BlockError {
    /// True when the engine may spend retry budget on this error.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }
}

/// Runs data-producing step actions.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Run one action attempt.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] for a transient failure the step's
    /// retry budget may repeat, and [`BlockError::Terminal`] otherwise.
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError>;
}

/// Runs verdict-producing requirement checks.
#[async_trait]
pub trait CheckExecutor: Send + Sync {
    /// Run one check.
    ///
    /// # Errors
    /// Returns a [`BlockError`] when no verdict was produced; the engine
    /// records the requirement as inconclusive with the error as reason.
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError>;
}

/// The durable record of one queued execution's block invocations.
///
/// The engine records each action or check attempt before dispatching it and
/// commits the block's settlement before its dependents run, so a reclaimed
/// execution restores what settled and counts what never did. The host fences
/// both writes with the execution's lease; direct verify runs without one.
#[async_trait]
pub trait Checkpoints: Send + Sync {
    /// Record that the invocation attempt `producer` starts: a block, its
    /// item and iteration, and its one-based attempt.
    ///
    /// # Errors
    /// Returns a bounded reason when the record cannot commit, such as a lost
    /// lease; the attempt is then not dispatched.
    async fn begin(&self, producer: &Producer) -> Result<(), String>;

    /// Forget the unsettled attempt `producer`, which was refused process
    /// capacity or its definition, so it spends nothing. A host keeps the
    /// record of an attempt that already retained work, which stays spent.
    ///
    /// # Errors
    /// Returns a bounded reason when the record cannot be removed.
    async fn forget(&self, producer: &Producer) -> Result<(), String>;

    /// Commit `settled` as the outcome of the attempt `producer`.
    ///
    /// # Errors
    /// Returns a bounded reason when the settlement cannot commit; the block's
    /// dependents then do not run.
    async fn settle(
        &self,
        producer: &Producer,
        settled: &InvocationSettlement,
    ) -> Result<(), String>;

    /// Record a scheduling decision recovery must reproduce rather than make
    /// again, such as a step's frozen fanout items or a speculative step's
    /// disposition, before anything depends on it.
    ///
    /// # Errors
    /// Returns a bounded reason when the decision cannot commit; nothing that
    /// depends on it then runs.
    async fn control(&self, control: &ControlRecord) -> Result<(), String>;
}

/// Decodes a check's raw `response` as the Verdict its declared `schema`
/// describes.
///
/// The whole response must satisfy the exact declared schema, decode as a
/// [`Verdict`], and keep its custom `data` within the fixed bounds. Agent
/// and code checks share this rule; a refusal names the failing schema
/// location or limit, never the response contents or a path into them,
/// since returned dictionary keys are response contents. `schemas` reuses
/// the schema's compiled validator when an equal schema was prepared before;
/// the response is validated either way.
///
/// # Errors
/// Returns [`BlockError::Terminal`], which makes the requirement
/// inconclusive, for an invalid schema, a response the schema refuses, a
/// response that is not a Verdict, or data beyond its bounds.
pub fn verdict_from(
    schemas: &PreparedSchemas,
    schema: &SharedSchema,
    response: Value,
) -> Result<Verdict, BlockError> {
    let terminal = |reason: String| BlockError::Terminal { reason };
    let validator = schemas
        .validator(schema)
        .ok_or_else(|| terminal("the declared Verdict format is not a valid schema".to_owned()))?;
    if let Some(error) = validator.iter_errors(&response).next() {
        return Err(terminal(format!(
            "the check response violates its declared Verdict format at schema `{}`",
            error.schema_path()
        )));
    }
    let verdict: Verdict = serde_json::from_value(response)
        .map_err(|_| terminal("the check response is not a Verdict".to_owned()))?;
    verdict.check_data_bounds().map_err(terminal)?;
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    //! Executor seam calls and the tool-record handoff into step results.

    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    /// Echoes its `from` input so the test can observe what the engine
    /// handed over.
    struct Echo;

    #[async_trait]
    impl ActionExecutor for Echo {
        /// Return the `from` JSON value, or a terminal error when absent.
        ///
        /// # Errors
        /// Returns [`BlockError::Terminal`] when `from` is missing or is
        /// evidence rather than JSON.
        async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
            match call.inputs.get("from").map(|input| &input.value) {
                Some(CallValue::Json(value)) => Ok(ActionOutput {
                    output: Value::clone(value),
                    provenance: None,
                    tool_calls: Vec::new(),
                }),
                _ => Err(BlockError::Terminal {
                    reason: "from is not JSON".to_owned(),
                }),
            }
        }
    }

    /// A dynamically dispatched action executor receives resolved values with
    /// their source slots and returns data, not a verdict.
    ///
    /// # Panics
    /// Panics when the call does not reach the executor intact.
    #[tokio::test]
    async fn action_executor_receives_resolved_inputs() {
        let executor: Arc<dyn ActionExecutor> = Arc::new(Echo);
        let id = TaskId::new("structure").expect("id");
        let action: Action =
            serde_json::from_value(json!({"kind": "extract", "from": "inputs.deck"}))
                .expect("action");
        let input = CallInput {
            value: CallValue::Json(json!({"slides": []}).into()),
            sources: BTreeSet::from([SlotName::new("deck").expect("slot")]),
        };
        let call = BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs: BTreeMap::from([("from".to_owned(), input)]),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(5),
            attempt: 1,
            item_index: None,
            iteration_index: None,
        };
        let output = executor.execute(call).await.expect("output");
        assert_eq!(output.output, json!({"slides": []}));
    }

    /// A tool record an action returns is the record a step result carries:
    /// its digest and returned byte count survive the result wire round trip.
    ///
    /// # Panics
    /// Panics when the step result does not round-trip the record unchanged.
    #[test]
    fn action_tool_calls_carry_into_step_results() {
        use wyrd_spec::card::agent::BuiltinTool;
        use wyrd_spec::verification::execution::{StepResult, StepStatus};
        let record: ToolCallRecord = serde_json::from_value(json!({
            "tool": "fs.read", "arguments": {"path": "/inputs/a.pdf"},
            "output_sha256": "c".repeat(64), "returned_bytes": 512
        }))
        .expect("record");
        let output = ActionOutput {
            output: json!({"cap": 24}),
            provenance: None,
            tool_calls: vec![record.clone()],
        };
        let step = StepResult {
            id: TaskId::new("findings").expect("id"),
            status: StepStatus::Succeeded,
            output: Some(output.output),
            attempts: 1,
            duration_ms: 5,
            reason: None,
            provenance: output.provenance,
            tool_calls: output.tool_calls,
        };
        let wire = serde_json::to_value(&step).expect("serialize");
        let parsed: StepResult = serde_json::from_value(wire).expect("deserialize");
        assert_eq!(parsed.tool_calls, vec![record]);
        assert_eq!(parsed.tool_calls[0].tool, BuiltinTool::FsRead);
        assert_eq!(parsed.tool_calls[0].returned_bytes, 512);
    }

    /// A declared data schema keeps its own reference root once embedded in
    /// the Verdict envelope: `#/properties/reason` names the data's integer
    /// field, not the Verdict's string reason, with or without a declared
    /// 2020-12 `$schema`; `prefixItems` still constrains a tuple; a `$ref`'s
    /// siblings still apply; and definitions named like the Verdict's own
    /// resolve to the author's.
    ///
    /// # Panics
    /// Panics when a composed schema accepts bad data or refuses good data.
    #[test]
    fn verdict_data_keeps_its_schema_resource() {
        let cases = [
            (
                json!({"type": "object", "additionalProperties": false,
                    "required": ["reason", "copy", "pair"],
                    "properties": {
                        "reason": {"type": "integer"},
                        "copy": {"$ref": "#/properties/reason"},
                        "pair": {"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]}}}),
                json!({"reason": 1, "copy": 2, "pair": ["a", 1]}),
                vec![
                    json!({"reason": 1, "copy": "wrong", "pair": ["a", 1]}),
                    json!({"reason": 1, "copy": 2, "pair": [1, "a"]}),
                ],
            ),
            (
                json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "type": "object",
                    "required": ["reason", "copy"],
                    "properties": {"reason": {"type": "integer"}, "copy": {"$ref": "#/properties/reason"}}}),
                json!({"reason": 1, "copy": 2}),
                vec![json!({"reason": 1, "copy": "wrong"})],
            ),
            (
                json!({"type": "object", "required": ["cite"],
                    "properties": {"cite": {"$ref": "#/definitions/Citation", "maximum": 1}},
                    "definitions": {"Citation": {"type": "integer"}}}),
                json!({"cite": 1}),
                vec![json!({"cite": 3}), json!({"cite": {"slot": "deck"}})],
            ),
            (
                json!({"type": "object", "required": ["cite"],
                    "properties": {"cite": {"$ref": "#/$defs/Citation"}},
                    "$defs": {"Citation": {"type": "string"}}}),
                json!({"cite": "p3"}),
                vec![json!({"cite": 3})],
            ),
        ];
        let response = |data: Value| json!({"outcome": "passed", "reason": "judged", "citations": [], "data": data});
        for (data, good, bad) in cases {
            let format: skald_spec::NamedResponseFormat =
                serde_json::from_value(json!({"kind": "verdict", "data": data})).expect("format");
            let schema = SharedSchema::new(Value::Object(
                Verdict::response_format(&format).expect("resolves"),
            ));
            let verdict =
                verdict_from(&PreparedSchemas::default(), &schema, response(good.clone()))
                    .unwrap_or_else(|error| panic!("{good}: {error}"));
            assert_eq!(serde_json::to_value(verdict.data).expect("data"), good);
            for bad in bad {
                assert!(
                    matches!(
                        verdict_from(&PreparedSchemas::default(), &schema, response(bad.clone())),
                        Err(BlockError::Terminal { .. })
                    ),
                    "{bad}"
                );
            }
        }
    }

    /// A warm prepared Verdict format still refuses a response its schema
    /// refuses, a non-Verdict, and custom data beyond the depth bound, and a
    /// changed format is judged by its own schema rather than a stale one.
    ///
    /// # Panics
    /// Panics when a warm format accepts what it refuses cold, or a changed
    /// format reuses the previous validator.
    #[test]
    fn prepared_verdict_keeps_validation_and_data_bounds() {
        let schemas = PreparedSchemas::default();
        let format = |data: Value| {
            let format: skald_spec::NamedResponseFormat =
                serde_json::from_value(json!({"kind": "verdict", "data": data})).expect("format");
            SharedSchema::new(Value::Object(
                Verdict::response_format(&format).expect("resolves"),
            ))
        };
        let integers = format(json!({"type": "object", "additionalProperties": true}));
        let response = |data: Value| json!({"outcome": "passed", "reason": "judged", "citations": [], "data": data});
        let mut deep = json!(1);
        for _ in 0..=wyrd_spec::verification::execution::MAX_VERDICT_DATA_DEPTH {
            deep = json!({"n": deep});
        }
        for _ in 0..2 {
            let verdict =
                verdict_from(&schemas, &integers, response(json!({"n": 1}))).expect("valid");
            assert_eq!(
                verdict.outcome,
                wyrd_spec::verification::VerificationVerdict::Passed
            );
            for refused in [
                json!({"outcome": "passed", "reason": "judged", "citations": [], "data": 1}),
                json!({"outcome": "maybe", "reason": "judged", "citations": []}),
                response(deep.clone()),
            ] {
                assert!(
                    matches!(
                        verdict_from(&schemas, &integers, refused.clone()),
                        Err(BlockError::Terminal { .. })
                    ),
                    "{refused}"
                );
            }
        }
        let strings = format(json!({"type": "object", "additionalProperties": {"type": "string"}}));
        assert!(verdict_from(&schemas, &strings, response(json!({"n": 1}))).is_err());
        assert!(verdict_from(&schemas, &strings, response(json!({"n": "1"}))).is_ok());
        assert!(verdict_from(&schemas, &integers, response(json!({"n": 1}))).is_ok());
    }

    /// A refusal names where the declared schema failed, never where the
    /// returned data did, so a dictionary key the check returned stays out
    /// of the retained reason.
    ///
    /// # Panics
    /// Panics when the refusal is not terminal or echoes the returned key.
    #[test]
    fn verdict_refusal_omits_returned_keys() {
        let format: skald_spec::NamedResponseFormat = serde_json::from_value(json!({
            "kind": "verdict",
            "data": {"type": "object", "additionalProperties": {"type": "integer"}}
        }))
        .expect("format");
        let schema = SharedSchema::new(Value::Object(
            Verdict::response_format(&format).expect("resolves"),
        ));
        let secret = "customer-secret@example.test";
        let response = json!({"outcome": "passed", "reason": "judged", "citations": [],
            "data": {"nested": 1, secret: "wrong"}});
        match verdict_from(&PreparedSchemas::default(), &schema, response) {
            Err(BlockError::Terminal { reason }) => {
                assert!(!reason.contains("customer-secret"), "{reason}");
            }
            other => panic!("expected a terminal refusal, got {other:?}"),
        }
    }
}
