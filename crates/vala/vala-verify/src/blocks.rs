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
    AgentCardResolver, AgentExecutor, EvidenceReader, PromptCardResolver, block_error,
};
pub use tools::{AgentToolset, ToolSession, provider_tool_name};

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use tokio::time::Instant;

use wyrd_spec::card::verifier::{Action, Check};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    CallProvenance, InputDeclaration, RequirementDetail, ToolCallRecord, Verdict,
};

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
    Json(Value),
    /// Pinned original evidence a media slot names; executors fetch bytes.
    Evidence(InputDeclaration),
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
    /// worker or a workspace, at admission.
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

/// Decodes a check's raw `response` as the Verdict its declared `schema`
/// describes.
///
/// The whole response must satisfy the exact declared schema, decode as a
/// [`Verdict`], and keep its custom `data` within the fixed bounds. Agent
/// and code checks share this rule; a refusal names the failing location
/// or limit, never the response contents.
///
/// # Errors
/// Returns [`BlockError::Terminal`], which makes the requirement
/// inconclusive, for an invalid schema, a response the schema refuses, a
/// response that is not a Verdict, or data beyond its bounds.
pub fn verdict_from(schema: &Value, response: Value) -> Result<Verdict, BlockError> {
    let terminal = |reason: String| BlockError::Terminal { reason };
    let validator = jsonschema::validator_for(schema)
        .map_err(|_| terminal("the declared Verdict format is not a valid schema".to_owned()))?;
    if let Some(error) = validator.iter_errors(&response).next() {
        return Err(terminal(format!(
            "the check response violates its declared Verdict format at `{}` (schema `{}`)",
            error.instance_path(),
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
                    output: value.clone(),
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
            value: CallValue::Json(json!({"slides": []})),
            sources: BTreeSet::from([SlotName::new("deck").expect("slot")]),
        };
        let call = BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs: BTreeMap::from([("from".to_owned(), input)]),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(5),
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

    /// A declared data schema keeps its own reference root and dialect once
    /// embedded in the Verdict envelope: `#/properties/reason` names the
    /// data's integer field, not the Verdict's string reason; 2020-12
    /// `prefixItems` still constrains a tuple; a declared draft-07 dialect
    /// still ignores a `$ref`'s siblings; and definitions named like the
    /// Verdict's own resolve to the author's.
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
                json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
                    "required": ["cite"],
                    "properties": {"cite": {"$ref": "#/definitions/Citation", "maximum": 1}},
                    "definitions": {"Citation": {"type": "integer"}}}),
                json!({"cite": 3}),
                vec![json!({"cite": {"slot": "deck"}})],
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
            let schema = Value::Object(Verdict::response_format(&format).expect("resolves"));
            let verdict = verdict_from(&schema, response(good.clone()))
                .unwrap_or_else(|error| panic!("{good}: {error}"));
            assert_eq!(serde_json::to_value(verdict.data).expect("data"), good);
            for bad in bad {
                assert!(
                    matches!(
                        verdict_from(&schema, response(bad.clone())),
                        Err(BlockError::Terminal { .. })
                    ),
                    "{bad}"
                );
            }
        }
    }
}
