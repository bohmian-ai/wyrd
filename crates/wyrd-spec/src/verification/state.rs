//! The recoverable state of one Verifier execution.
//!
//! [`ExecutionState`] is the complete logical state an execution resumes
//! from: every invocation attempt and its settlement, every accepted
//! publication with the exact File descriptors that hold its bytes, and the
//! selected terminal outcome. Each accepted revision is one
//! `bifrost.verification.execution_events` row whose value is this state
//! serialized whole, so recovery reads exactly one row. Runtime handles,
//! credentials, and model conversations are never part of it, and file bytes
//! stay in verification File storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::execution::{InvocationSettlement, Producer, Publication, VerificationOutcome};
use crate::auth::Sha256Hex;
use crate::ids::VerificationResultId;
use crate::storage::StoredObjectRef;

/// The state format this release writes and reads.
///
/// A stored state of any other format is refused rather than reinterpreted,
/// so a worker never resumes, or restarts as fresh, state it cannot read.
pub const EXECUTION_STATE_FORMAT: u32 = 1;

/// The complete recoverable state of one execution at one accepted revision.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionState {
    /// The state format, [`EXECUTION_STATE_FORMAT`] when written.
    pub format: u32,
    /// The last entry sequence assigned; each new invocation attempt or
    /// publication takes the next one, so listings keep commit order.
    pub sequence: i64,
    /// Every invocation attempt still recorded, in sequence order. An
    /// attempt refused capacity before it ran is removed again.
    pub invocations: Vec<InvocationRecord>,
    /// Every accepted publication, in sequence order.
    pub publications: Vec<AcceptedPublication>,
    /// The selected terminal outcome, once the execution produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalSelection>,
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
    use super::{EXECUTION_STATE_FORMAT, ExecutionState, ExecutionStateError};

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
        assert!(matches!(
            ExecutionState::decode(
                serde_json::json!({ "format": EXECUTION_STATE_FORMAT, "sequence": "x" })
            ),
            Err(ExecutionStateError::Malformed(_))
        ));
    }
}
