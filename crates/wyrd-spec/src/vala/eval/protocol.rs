//! Server-hosted eval pull-protocol wire shapes.
//!
//! All orchestration logic lives in the server-side eval orchestrator. The
//! client owns one thing: executing the agent under test for one turn, and in
//! [`SimulatedUserMode::Client`], generating the next user message. Everything
//! else, including scenario loading, turn cursor, conversation history,
//! simulated-user generation, judge invocation, scoring, aggregation, and
//! comparison, stays server-side.
//!
//! The protocol carries history explicitly on each
//! [`TurnDirective::AgentTurn`] payload; there is no implicit propagation.
//! Trace-side correlation does not appear in this module.
//!
//! Wire-shape examples for [`TurnDirective`]:
//!
//! ```json
//! {"kind":"agent_turn","scenario_id":"happy_path","turn":0,"message":"Hello","history":[]}
//! ```
//!
//! ```json
//! {"kind":"user_turn_needed","scenario_id":"happy_path","turn":1,"history":[]}
//! ```
//!
//! ```json
//! {"kind":"scenario_complete","scenario_id":"happy_path"}
//! ```
//!
//! ```json
//! {"kind":"run_complete"}
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::reference::CardRef;

use super::ids::{LeaseToken, RunId, ScenarioId};
use super::record::EvalRecordObservation;

/// Maximum conversation history entries carried on a single [`TurnDirective`].
///
/// Each completed agent turn produces two entries: a `User` push when the
/// server advances past the user message and an `Agent` push on agent-turn
/// submission. The cap is therefore `MAX_TURNS_HARD_CAP * 2` so that a
/// scenario running the full 256-turn limit never triggers the guard.
pub const MAX_HISTORY_TURNS: usize = super::scenario::MAX_TURNS_HARD_CAP as usize * 2;

/// Client request to open and lease a new eval run against `eval_ref`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EvalRunOpenRequest {
    /// Reference to the Eval card under evaluation.
    pub eval_ref: CardRef,
    /// Who generates non-scripted user turns.
    pub simulated_user: SimulatedUserMode,
}

/// Server response to a successful [`EvalRunOpenRequest`].
///
/// `run_id` is the existing core [`RunId`]. An eval execution is a `Run` of
/// the referenced Eval card under the Card/Spec/Run/Observation ontology, so no
/// eval-specific run-id noun is introduced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EvalRunOpenResponse {
    /// Server-generated handle for the leased eval run.
    pub run_id: RunId,
    /// Opaque lease the client returns on every subsequent call.
    pub lease_token: LeaseToken,
}

/// Selects who supplies the next user message when the next turn is not
/// already scripted via `EvalScenario.predefined_turns`.
///
/// Scripted turns always come from `predefined_turns` server-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SimulatedUserMode {
    /// Server-simulated persona drives non-scripted user turns.
    Server,
    /// Client supplies non-scripted user turns.
    Client,
}

/// The server's next directive for the eval pull protocol.
///
/// This is a closed kind-tagged enum. Adding a variant is a breaking protocol
/// change, so no open-ended enum marker is used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnDirective {
    /// Run the agent for one turn against `message` and `history`, then post
    /// the response and any emitted eval records via [`AgentTurnSubmission`].
    AgentTurn {
        /// Scenario the agent is currently progressing.
        scenario_id: ScenarioId,
        /// Zero-indexed turn cursor within the scenario.
        turn: u32,
        /// User message the agent should respond to this turn.
        message: String,
        /// Conversation so far. Bounded by [`MAX_HISTORY_TURNS`].
        history: Vec<ConversationTurn>,
    },
    /// The server delegates the next user message to the client.
    UserTurnNeeded {
        /// Scenario the user is currently progressing.
        scenario_id: ScenarioId,
        /// Zero-indexed turn cursor.
        turn: u32,
        /// Conversation so far. Bounded by [`MAX_HISTORY_TURNS`].
        history: Vec<ConversationTurn>,
    },
    /// Server scored the scenario and advanced.
    ScenarioComplete {
        /// The scenario that just finished.
        scenario_id: ScenarioId,
    },
    /// No more scenarios. The run is finished; results live server-side.
    RunComplete,
}

/// One turn of conversation history carried by [`TurnDirective`].
///
/// `wyrd-spec` cannot depend on Skald, so the protocol's history shape is
/// intentionally minimal: plain `{role, content}` text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConversationTurn {
    /// Who produced this turn.
    pub role: TurnRole,
    /// Verbatim turn content.
    pub content: String,
}

/// Speaker role on a [`ConversationTurn`].
///
/// Tool and system roles stay inside the agent runtime layer; the eval
/// protocol cares only about the user and agent dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    /// Human or simulated-user turn.
    User,
    /// Agent-under-test turn.
    Agent,
}

/// Client reply to [`TurnDirective::AgentTurn`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AgentTurnSubmission {
    /// Scenario that produced the directive being answered.
    pub scenario_id: ScenarioId,
    /// Turn cursor that produced the directive being answered.
    pub turn: u32,
    /// Verbatim agent response for this turn.
    pub response: String,
    /// Any eval records the agent emitted while producing `response`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<EvalRecordObservation>,
}

/// Client reply to [`TurnDirective::UserTurnNeeded`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct UserTurnSubmission {
    /// Scenario that produced the directive being answered.
    pub scenario_id: ScenarioId,
    /// Turn cursor that produced the directive being answered.
    pub turn: u32,
    /// User message the client generated.
    pub message: String,
}

/// Structured output of the server-side simulated-user judge call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SimulatedUserTurn {
    /// Next user message the simulator produced.
    pub message: String,
    /// Simulator's verdict on goal achievement after this turn.
    pub goal_achieved: bool,
}

/// Wire-validation errors raised by protocol shapes.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Conversation history exceeded [`MAX_HISTORY_TURNS`].
    #[error("conversation history length {got} exceeds MAX_HISTORY_TURNS {max}")]
    HistoryTooLong {
        /// Observed history length.
        got: usize,
        /// Static cap.
        max: usize,
    },
}

impl TurnDirective {
    /// Validate pure wire bounds.
    ///
    /// Turn monotonicity is server-enforced; this function only checks bounds
    /// visible from a single directive.
    ///
    /// # Errors
    /// Returns [`ProtocolError::HistoryTooLong`] when the carried history
    /// exceeds [`MAX_HISTORY_TURNS`].
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let history_len = match self {
            TurnDirective::AgentTurn { history, .. }
            | TurnDirective::UserTurnNeeded { history, .. } => history.len(),
            TurnDirective::ScenarioComplete { .. } | TurnDirective::RunComplete => 0,
        };
        if history_len > MAX_HISTORY_TURNS {
            return Err(ProtocolError::HistoryTooLong {
                got: history_len,
                max: MAX_HISTORY_TURNS,
            });
        }
        Ok(())
    }
}
