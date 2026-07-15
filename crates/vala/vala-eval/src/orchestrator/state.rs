//! Per-run eval orchestration state machine.

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::ScenarioId;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, ConversationTurn, SimulatedUserMode, TurnDirective, TurnRole,
    UserTurnSubmission,
};
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{EvalScenario, MAX_HISTORY_TURNS};
use wyrd_spec::vala::ids::RunId;

use super::OrchestratorError;

/// Outward run lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    /// At least one scenario can still produce directives.
    Running,
    /// All scenarios have completed.
    Complete,
}

/// Directive returned by [`RunState::next`].
#[derive(Debug, Clone, PartialEq)]
pub struct NextDirective(pub TurnDirective);

/// Result of accepting a submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// The caller should poll [`RunState::next`] again.
    Continue,
    /// The run has no more work.
    RunComplete,
}

/// Per-scenario mutable cursor held server-side.
#[derive(Debug, Clone)]
pub struct ScenarioCursor {
    /// Run id shared by this scenario execution.
    pub run_id: RunId,
    /// Scenario definition.
    pub scenario: EvalScenario,
    /// Zero-indexed turn that will be assigned to the next agent directive.
    pub next_turn: u32,
    /// Conversation history retained server-side.
    pub history: Vec<ConversationTurn>,
    /// Next scripted user turn index.
    pub scripted_cursor: usize,
    /// Records emitted while this scenario ran.
    pub emitted_records: Vec<EvalRecordObservation>,
    /// Final agent response as JSON for passenger scoring.
    pub final_response: Value,
    /// Termination flag.
    pub terminated: bool,
    /// Whether the server simulator declared the goal achieved.
    pub goal_achieved: bool,
    /// Scenario start timestamp.
    pub started_at: DateTime<Utc>,
    /// Scenario completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    awaiting_agent_message: Option<String>,
    completed_agent_turns: u32,
}

impl ScenarioCursor {
    fn new(run_id: RunId, scenario: EvalScenario) -> Self {
        let initial_query = scenario.initial_query.clone();
        Self {
            run_id,
            scenario,
            next_turn: 0,
            history: Vec::new(),
            scripted_cursor: 0,
            emitted_records: Vec::new(),
            final_response: Value::Null,
            terminated: false,
            goal_achieved: false,
            started_at: Utc::now(),
            completed_at: None,
            awaiting_agent_message: Some(initial_query),
            completed_agent_turns: 0,
        }
    }

    /// True when this scenario should emit `ScenarioComplete`.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.terminated || self.completed_agent_turns >= self.scenario.max_turns
    }

    fn push(&mut self, role: TurnRole, content: String) -> Result<(), OrchestratorError> {
        if self.history.len() >= MAX_HISTORY_TURNS {
            return Err(OrchestratorError::Invariant {
                reason: format!(
                    "conversation history cannot exceed MAX_HISTORY_TURNS ({MAX_HISTORY_TURNS})"
                ),
            });
        }
        self.history.push(ConversationTurn { role, content });
        Ok(())
    }

    fn finish(mut self) -> Self {
        self.completed_at = Some(Utc::now());
        self
    }
}

/// Run-level state machine.
pub struct RunState {
    /// Core run id used by the eval protocol.
    pub run_id: RunId,
    /// Eval card under execution.
    pub eval_ref: CardRef,
    /// Source for non-scripted user turns.
    pub simulated_user: SimulatedUserMode,
    /// Active scenario cursors.
    pub scenarios: VecDeque<ScenarioCursor>,
    completed: VecDeque<ScenarioCursor>,
    /// Outstanding directive, if one has been issued and not answered.
    pub outstanding: Option<TurnDirective>,
    /// Run status.
    pub status: RunStatus,
    /// Run open timestamp.
    pub opened_at: DateTime<Utc>,
}

impl RunState {
    /// Open a fresh run.
    #[must_use]
    pub fn open(
        eval_ref: CardRef,
        simulated_user: SimulatedUserMode,
        scenarios: Vec<EvalScenario>,
    ) -> Self {
        let run_id = RunId::new();
        Self {
            run_id: run_id.clone(),
            eval_ref,
            simulated_user,
            scenarios: scenarios
                .into_iter()
                .map(|scenario| ScenarioCursor::new(run_id.clone(), scenario))
                .collect(),
            completed: VecDeque::new(),
            outstanding: None,
            status: RunStatus::Running,
            opened_at: Utc::now(),
        }
    }

    /// Return the next directive. Retry-safe while a directive is outstanding.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] if an internal state invariant is broken.
    // justification: orchestrator next() returns Result and is retry-safe on outstanding directives; does not match Iterator::next semantics
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<NextDirective, OrchestratorError> {
        if let Some(outstanding) = &self.outstanding {
            return Ok(NextDirective(outstanding.clone()));
        }

        if self.status == RunStatus::Complete {
            let directive = TurnDirective::RunComplete;
            self.outstanding = Some(directive.clone());
            return Ok(NextDirective(directive));
        }

        if self
            .scenarios
            .front()
            .is_some_and(ScenarioCursor::is_finished)
        {
            let Some(cursor) = self.scenarios.pop_front() else {
                return Err(OrchestratorError::Invariant {
                    reason: "finished head scenario must exist before pop".to_owned(),
                });
            };
            let scenario_id = cursor.scenario.id.clone();
            self.completed.push_back(cursor.finish());
            let directive = TurnDirective::ScenarioComplete { scenario_id };
            self.outstanding = Some(directive.clone());
            return Ok(NextDirective(directive));
        }

        let Some(head) = self.scenarios.front_mut() else {
            self.status = RunStatus::Complete;
            let directive = TurnDirective::RunComplete;
            self.outstanding = Some(directive.clone());
            return Ok(NextDirective(directive));
        };

        if let Some(message) = head.awaiting_agent_message.take() {
            head.push(TurnRole::User, message.clone())?;
            let directive = TurnDirective::AgentTurn {
                scenario_id: head.scenario.id.clone(),
                turn: head.next_turn,
                message,
                history: head.history.clone(),
            };
            self.outstanding = Some(directive.clone());
            return Ok(NextDirective(directive));
        }

        let turn = head.next_turn.saturating_add(1);
        if head.scripted_cursor < head.scenario.predefined_turns.len() {
            let message = head.scenario.predefined_turns[head.scripted_cursor].clone();
            head.scripted_cursor += 1;
            head.next_turn = turn;
            head.awaiting_agent_message = Some(message);
            return self.next();
        }

        let directive = TurnDirective::UserTurnNeeded {
            scenario_id: head.scenario.id.clone(),
            turn,
            history: head.history.clone(),
        };
        self.outstanding = Some(directive.clone());
        Ok(NextDirective(directive))
    }

    /// True when the outstanding `UserTurnNeeded` should be handled by the server.
    #[must_use]
    pub fn wants_server_simulated_turn(&self) -> bool {
        matches!(self.simulated_user, SimulatedUserMode::Server)
            && matches!(self.outstanding, Some(TurnDirective::UserTurnNeeded { .. }))
    }

    /// Borrow the active scenario, if any.
    #[must_use]
    pub fn current_scenario(&self) -> Option<&EvalScenario> {
        self.scenarios.front().map(|cursor| &cursor.scenario)
    }

    /// Borrow the active conversation history.
    #[must_use]
    pub fn current_history(&self) -> &[ConversationTurn] {
        self.scenarios
            .front()
            .map_or(&[], |cursor| cursor.history.as_slice())
    }

    /// Accept an agent submission.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the submission does not match the
    /// outstanding directive.
    pub fn submit_agent_turn(
        &mut self,
        sub: AgentTurnSubmission,
    ) -> Result<SubmissionOutcome, OrchestratorError> {
        let (expected_scenario, expected_turn) = match &self.outstanding {
            Some(TurnDirective::AgentTurn {
                scenario_id, turn, ..
            }) => (scenario_id.clone(), *turn),
            Some(TurnDirective::UserTurnNeeded { .. }) => {
                return Err(OrchestratorError::SubmissionKindMismatch {
                    expected: "user_turn",
                    got: "agent_turn",
                });
            }
            Some(TurnDirective::ScenarioComplete { .. } | TurnDirective::RunComplete) | None => {
                return Err(OrchestratorError::SubmissionKindMismatch {
                    expected: "none",
                    got: "agent_turn",
                });
            }
        };
        validate_submission(
            &sub.scenario_id,
            sub.turn,
            &expected_scenario,
            expected_turn,
        )?;

        let Some(head) = self.scenarios.front_mut() else {
            return Err(OrchestratorError::Invariant {
                reason: "agent submission requires an active scenario".to_owned(),
            });
        };
        head.push(TurnRole::Agent, sub.response.clone())?;
        head.final_response = serde_json::json!(sub.response);
        head.emitted_records.extend(sub.records);
        head.completed_agent_turns = head.completed_agent_turns.saturating_add(1);

        if head
            .scenario
            .termination_signal
            .as_ref()
            .is_some_and(|signal| !signal.is_empty() && sub.response.contains(signal))
        {
            head.terminated = true;
        }
        if head.completed_agent_turns >= head.scenario.max_turns {
            head.terminated = true;
        }

        self.outstanding = None;
        Ok(if self.status == RunStatus::Complete {
            SubmissionOutcome::RunComplete
        } else {
            SubmissionOutcome::Continue
        })
    }

    /// Accept a client-delegated user submission.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the submission does not match the
    /// outstanding directive.
    pub fn submit_user_turn(
        &mut self,
        sub: UserTurnSubmission,
    ) -> Result<SubmissionOutcome, OrchestratorError> {
        let (expected_scenario, expected_turn) = match &self.outstanding {
            Some(TurnDirective::UserTurnNeeded {
                scenario_id, turn, ..
            }) => (scenario_id.clone(), *turn),
            Some(TurnDirective::AgentTurn { .. }) => {
                return Err(OrchestratorError::SubmissionKindMismatch {
                    expected: "agent_turn",
                    got: "user_turn",
                });
            }
            Some(TurnDirective::ScenarioComplete { .. } | TurnDirective::RunComplete) | None => {
                return Err(OrchestratorError::SubmissionKindMismatch {
                    expected: "none",
                    got: "user_turn",
                });
            }
        };
        validate_submission(
            &sub.scenario_id,
            sub.turn,
            &expected_scenario,
            expected_turn,
        )?;

        let Some(head) = self.scenarios.front_mut() else {
            return Err(OrchestratorError::Invariant {
                reason: "user submission requires an active scenario".to_owned(),
            });
        };
        head.next_turn = sub.turn;
        head.awaiting_agent_message = Some(sub.message);
        self.outstanding = None;
        Ok(SubmissionOutcome::Continue)
    }

    /// Apply a server-simulated user turn.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the simulated turn does not match the
    /// outstanding directive.
    pub fn submit_simulated_user(
        &mut self,
        scenario_id: ScenarioId,
        turn: u32,
        message: String,
        goal_achieved: bool,
    ) -> Result<SubmissionOutcome, OrchestratorError> {
        self.submit_user_turn(UserTurnSubmission {
            scenario_id,
            turn,
            message,
        })?;
        if let Some(head) = self.scenarios.front_mut() {
            head.goal_achieved = goal_achieved;
            if goal_achieved {
                head.terminated = true;
            }
        }
        Ok(SubmissionOutcome::Continue)
    }

    /// Take a completed scenario cursor for scoring.
    ///
    /// Return a clone of a completed scenario cursor without removing it.
    ///
    /// Use this before scoring to ensure the cursor survives a scoring
    /// failure. Call [`Self::take_completed_scenario`] only after scoring and
    /// aggregation succeed.
    ///
    /// # Errors
    /// Returns [`OrchestratorError::ScenarioNotReady`] when the scenario has
    /// not completed yet.
    pub fn peek_completed_scenario(
        &self,
        scenario_id: &ScenarioId,
    ) -> Result<ScenarioCursor, OrchestratorError> {
        self.completed
            .iter()
            .find(|cursor| &cursor.scenario.id == scenario_id)
            .cloned()
            .ok_or_else(|| OrchestratorError::ScenarioNotReady {
                scenario_id: scenario_id.clone(),
            })
    }

    /// Remove a completed scenario cursor.
    ///
    /// Call this only after scoring and aggregation for the scenario have
    /// succeeded. On any scoring failure, leave the cursor in place so the
    /// next [`Self::next`] retry can attempt scoring again.
    ///
    /// # Errors
    /// Returns [`OrchestratorError::ScenarioNotReady`] when the scenario has not
    /// completed or was already taken.
    pub fn take_completed_scenario(
        &mut self,
        scenario_id: &ScenarioId,
    ) -> Result<ScenarioCursor, OrchestratorError> {
        let Some(index) = self
            .completed
            .iter()
            .position(|cursor| &cursor.scenario.id == scenario_id)
        else {
            return Err(OrchestratorError::ScenarioNotReady {
                scenario_id: scenario_id.clone(),
            });
        };
        self.completed
            .remove(index)
            .ok_or_else(|| OrchestratorError::Invariant {
                reason: "completed scenario position must remove".to_owned(),
            })
    }

    /// Acknowledge a scenario-complete directive.
    pub fn ack_scenario_complete(&mut self) {
        if matches!(
            self.outstanding,
            Some(TurnDirective::ScenarioComplete { .. })
        ) {
            self.outstanding = None;
        }
    }

    /// Acknowledge run completion.
    pub fn ack_run_complete(&mut self) {
        self.status = RunStatus::Complete;
        self.outstanding = None;
    }
}

fn validate_submission(
    got_scenario: &ScenarioId,
    got_turn: u32,
    expected_scenario: &ScenarioId,
    expected_turn: u32,
) -> Result<(), OrchestratorError> {
    if got_scenario != expected_scenario {
        return Err(OrchestratorError::ScenarioMismatch {
            got: got_scenario.clone(),
            expected: expected_scenario.clone(),
        });
    }
    if got_turn != expected_turn {
        return Err(OrchestratorError::TurnMismatch {
            got: got_turn,
            expected: expected_turn,
        });
    }
    Ok(())
}

/// Shared run state handle for HTTP and embedded callers.
pub type SharedRun = Arc<Mutex<RunState>>;
