//! The recoverable state of one Verifier execution.
//!
//! [`ExecutionState`] is the complete logical state an execution resumes
//! from: every invocation attempt and its settlement, every accepted
//! publication with the exact File descriptors that hold its bytes, the
//! exact execution identity, and the selected terminal outcome or the
//! failure the execution ended with. Each accepted revision is one
//! `bifrost.verification.execution_events` row whose value is this state
//! serialized whole, so recovery reads exactly one row. Runtime handles,
//! credentials, and model conversations are never part of it, and file bytes
//! stay in verification File storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::TaskId;
use super::execution::{
    InvocationSettlement, Producer, Publication, Speculation, VerificationOutcome,
};
use super::{VerificationError, VerificationExecutionStatus};
use crate::auth::Sha256Hex;
use crate::ids::{CardUid, GroupId, VerificationResultId};
use crate::reference::CardRef;
use crate::storage::StoredObjectRef;

/// The state format this release writes and reads.
///
/// A stored state of any other format is refused rather than reinterpreted,
/// so a worker never resumes, or restarts as fresh, state it cannot read.
/// Format 2 added the recorded identity and the terminal failure; format 3
/// added the control records of fanout, repetition, and speculation, so a
/// worker that cannot honour them refuses the state; format 1 never left
/// development and is refused.
pub const EXECUTION_STATE_FORMAT: u32 = 3;

/// The complete recoverable state of one execution at one accepted revision.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionState {
    /// The state format, [`EXECUTION_STATE_FORMAT`] when written.
    pub format: u32,
    /// The exact execution identity every revision records, so retained
    /// reads never consult the registry; `None` only before the first
    /// revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ExecutionIdentity>,
    /// The last entry sequence assigned; each new invocation attempt or
    /// publication takes the next one, so listings keep commit order.
    pub sequence: i64,
    /// Every invocation attempt still recorded, in sequence order. An
    /// attempt refused capacity before it ran is removed again.
    pub invocations: Vec<InvocationRecord>,
    /// Every accepted publication, in sequence order.
    pub publications: Vec<AcceptedPublication>,
    /// Every scheduling decision recovery must reproduce rather than make
    /// again: frozen fanout items and settled speculative dispositions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlRecord>,
    /// The selected terminal outcome, once the execution produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalSelection>,
    /// How the execution ended without an outcome, once it did; never
    /// recorded beside a [`Self::terminal`] outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<TerminalFailure>,
}

/// The exact identities an execution was accepted for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIdentity {
    /// The exact Verifier executed.
    pub verifier: CardRef,
    /// The exact subject judged.
    pub subject: CardRef,
    /// The deployment environment tag the execution was invoked with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_environment: Option<String>,
    /// Caller group the execution belongs to, or the group it judges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<GroupId>,
    /// Binding owner of a background execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_card_uid: Option<CardUid>,
}

/// How an execution ended without an outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalFailure {
    /// `cancelled`, `timed_out`, or `errored`.
    pub status: VerificationExecutionStatus,
    /// The sanitized terminal error.
    pub error: VerificationError,
    /// When the execution ended.
    pub ended_at: DateTime<Utc>,
}

/// One recorded invocation attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationRecord {
    /// The attempt's place in commit order.
    pub sequence: i64,
    /// The invoked step or requirement and its one-based attempt.
    pub producer: Producer,
    /// The committed settlement; `None` while the attempt is unsettled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<InvocationSettlement>,
}

/// One recorded scheduling decision of an execution.
///
/// A decision is recorded before anything it releases runs, and never
/// changes once recorded: recovery reads it back instead of deciding again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlRecord {
    /// The complete ordered array an `items` fanout step expands, frozen
    /// before its first item is dispatched.
    Items {
        /// The fanout step.
        step: TaskId,
        /// Every item, in original order.
        items: Vec<Value>,
    },
    /// A speculative step's settled disposition.
    Disposition {
        /// The speculative step.
        step: TaskId,
        /// Admitted, discarded, or blocked.
        speculation: Speculation,
        /// Why the condition decided as it did.
        reason: String,
    },
}

impl ExecutionState {
    /// The recorded disposition of `step`'s contribution: what its
    /// speculative guard decided, or `none` when nothing was recorded.
    #[must_use]
    pub fn speculation(&self, step: &TaskId) -> Speculation {
        self.controls
            .iter()
            .find_map(|control| match control {
                ControlRecord::Disposition {
                    step: decided,
                    speculation,
                    ..
                } if decided == step => Some(*speculation),
                _ => None,
            })
            .unwrap_or(Speculation::None)
    }
}

impl ControlRecord {
    /// The step this decision is about.
    #[must_use]
    pub const fn step(&self) -> &TaskId {
        match self {
            Self::Items { step, .. } | Self::Disposition { step, .. } => step,
        }
    }

    /// Whether `other` is the same kind of decision about the same step, so
    /// at most one of them may be recorded.
    #[must_use]
    pub fn decides_same(&self, other: &Self) -> bool {
        self.step() == other.step() && std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// One accepted publication and its private custody.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedPublication {
    /// The publication's place in commit order.
    pub sequence: i64,
    /// The publication as public reads return it.
    pub publication: Publication,
    /// The accepting call's identity, stable across its replays.
    pub call_id: String,
    /// Digest of the accepting call's canonical arguments.
    pub arguments: Sha256Hex,
    /// The completed File descriptor of each manifest file, in manifest
    /// order; empty for JSON. Private server metadata, never a public read.
    pub objects: Vec<StoredObjectRef>,
    /// The bytes the publication retains, charged to the execution's budget.
    pub retained_bytes: u64,
}

/// The terminal outcome an execution selected, which every later entry
/// delivers again instead of executing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSelection {
    /// The result identity every delivery of the outcome carries.
    pub result_id: VerificationResultId,
    /// When the selected run started.
    pub started_at: DateTime<Utc>,
    /// When it ended.
    pub ended_at: DateTime<Utc>,
    /// The outcome.
    pub outcome: VerificationOutcome,
}

/// Why a stored state cannot be resumed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionStateError {
    /// The state names a format this release does not read.
    #[error("execution state format {found} is not format {EXECUTION_STATE_FORMAT}")]
    Incompatible {
        /// The stored format.
        found: u64,
    },
    /// The state does not decode as its format.
    #[error("execution state is malformed: {0}")]
    Malformed(String),
}

impl ExecutionState {
    /// An execution's state before any revision was accepted.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            format: EXECUTION_STATE_FORMAT,
            ..Self::default()
        }
    }

    /// Decode a stored state, refusing any format other than
    /// [`EXECUTION_STATE_FORMAT`] before reading the rest.
    ///
    /// # Errors
    /// Returns [`ExecutionStateError::Incompatible`] for another or missing
    /// format, and [`ExecutionStateError::Malformed`] for a state that does
    /// not decode.
    pub fn decode(value: Value) -> Result<Self, ExecutionStateError> {
        let found = value.get("format").and_then(Value::as_u64).unwrap_or(0);
        if found != u64::from(EXECUTION_STATE_FORMAT) {
            return Err(ExecutionStateError::Incompatible { found });
        }
        serde_json::from_value(value)
            .map_err(|error| ExecutionStateError::Malformed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlRecord, EXECUTION_STATE_FORMAT, ExecutionState, ExecutionStateError, Speculation,
        TaskId,
    };

    /// A step's recorded disposition is what inspection reports; a step
    /// with none recorded reports `none`.
    ///
    /// # Panics
    /// Panics when a recorded disposition is not reported, or an
    /// undecided step reports one.
    #[test]
    fn recorded_dispositions_are_reported() {
        let critic = TaskId::new("critic").expect("valid step id");
        let reader = TaskId::new("reader").expect("valid step id");
        let state = ExecutionState {
            controls: vec![ControlRecord::Disposition {
                step: critic.clone(),
                speculation: Speculation::Discarded,
                reason: "the reader failed".to_owned(),
            }],
            ..ExecutionState::fresh()
        };
        assert_eq!(state.speculation(&critic), Speculation::Discarded);
        assert_eq!(state.speculation(&reader), Speculation::None);
    }

    /// Only [`EXECUTION_STATE_FORMAT`] decodes: a newer, missing, or
    /// earlier format is incompatible and a state that does not decode is
    /// malformed, never fresh.
    ///
    /// # Panics
    /// Panics when another format decodes, or a refusal is misclassified.
    #[test]
    fn only_the_current_format_decodes() {
        let fresh = ExecutionState::fresh();
        let stored = serde_json::to_value(&fresh).expect("state serializes");
        assert_eq!(ExecutionState::decode(stored), Ok(fresh));
        let newer = serde_json::json!({ "format": EXECUTION_STATE_FORMAT + 1, "future": [] });
        assert_eq!(
            ExecutionState::decode(newer),
            Err(ExecutionStateError::Incompatible {
                found: u64::from(EXECUTION_STATE_FORMAT + 1)
            })
        );
        assert!(matches!(
            ExecutionState::decode(serde_json::json!({})),
            Err(ExecutionStateError::Incompatible { found: 0 })
        ));
        for earlier in 1..EXECUTION_STATE_FORMAT {
            assert_eq!(
                ExecutionState::decode(serde_json::json!({ "format": earlier, "sequence": 0 })),
                Err(ExecutionStateError::Incompatible {
                    found: u64::from(earlier)
                }),
                "format {earlier} predates the recorded controls"
            );
        }
        assert!(matches!(
            ExecutionState::decode(
                serde_json::json!({ "format": EXECUTION_STATE_FORMAT, "sequence": "x" })
            ),
            Err(ExecutionStateError::Malformed(_))
        ));
    }
}
