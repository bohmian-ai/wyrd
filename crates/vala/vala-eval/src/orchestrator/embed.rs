//! Embedded local orchestrator driver.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, ConversationTurn, SimulatedUserMode, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{EvalScenario, ScenarioId};

use crate::{EvalResults, RunIdentity, ScenarioExecutionResults};

use super::{NextDirective, OrchestratorError, RunState, ScenarioScoring, ServerSimulatedUser};

/// Agent callback used by embedded local orchestration.
#[async_trait]
pub trait AgentTurnFn: Send + Sync {
    /// Execute one agent turn.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the agent callback fails.
    async fn invoke(
        &self,
        message: &str,
        history: &[ConversationTurn],
    ) -> Result<AgentTurnReply, OrchestratorError>;
}

/// Reply from [`AgentTurnFn`].
#[derive(Debug, Clone)]
pub struct AgentTurnReply {
    /// Agent response text.
    pub response: String,
    /// Eval records emitted during this turn.
    pub records: Vec<EvalRecordObservation>,
}

/// Client-delegated simulated user callback.
#[async_trait]
pub trait SimulatedUserFn: Send + Sync {
    /// Produce the next user turn.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the callback fails.
    async fn invoke(
        &self,
        scenario_id: &ScenarioId,
        turn: u32,
        history: &[ConversationTurn],
    ) -> Result<String, OrchestratorError>;
}

/// Final outcome from an embedded run.
pub struct EmbeddedOutcome {
    /// Per-scenario engine results.
    pub scenarios: Vec<ScenarioExecutionResults>,
    /// Aggregated run results.
    pub run: EvalResults,
}

/// In-process orchestrator used by local CLI and parity tests.
pub struct EmbeddedOrchestrator {
    /// Scoring engine.
    pub scoring: ScenarioScoring,
    /// Optional server-side simulated user.
    pub simulator: Option<Arc<ServerSimulatedUser>>,
}

impl EmbeddedOrchestrator {
    /// Drive an eval run to completion.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when state, callback, simulator, or scoring
    /// work fails.
    pub async fn drive(
        &self,
        eval_ref: CardRef,
        simulated_user: SimulatedUserMode,
        scenarios: Vec<EvalScenario>,
        agent: Arc<dyn AgentTurnFn>,
        client_simulated_user: Option<Arc<dyn SimulatedUserFn>>,
    ) -> Result<EmbeddedOutcome, OrchestratorError> {
        let mut state = RunState::open(eval_ref.clone(), simulated_user, scenarios);
        let mut scenario_results = Vec::new();
        let mut aggregation_inputs = Vec::new();

        loop {
            let NextDirective(directive) = state.next()?;
            match directive {
                TurnDirective::AgentTurn {
                    scenario_id,
                    turn,
                    message,
                    history,
                } => {
                    let reply = agent.invoke(&message, &history).await?;
                    state.submit_agent_turn(AgentTurnSubmission {
                        scenario_id,
                        turn,
                        response: reply.response,
                        records: reply.records,
                    })?;
                }
                TurnDirective::UserTurnNeeded {
                    scenario_id,
                    turn,
                    history,
                } => {
                    if state.wants_server_simulated_turn() {
                        let simulator = self.simulator.as_ref().ok_or_else(|| {
                            OrchestratorError::EmbeddedCallback {
                                reason: "SimulatedUserMode::Server requires a server simulator"
                                    .to_owned(),
                            }
                        })?;
                        let scenario = state.current_scenario().cloned().ok_or_else(|| {
                            OrchestratorError::Invariant {
                                reason: "server simulation requires an active scenario".to_owned(),
                            }
                        })?;
                        let turn_out = simulator.next_turn(&scenario, &history).await?;
                        state.submit_simulated_user(
                            scenario_id,
                            turn,
                            turn_out.message,
                            turn_out.goal_achieved,
                        )?;
                    } else {
                        let callback = client_simulated_user.as_ref().ok_or_else(|| {
                            OrchestratorError::EmbeddedCallback {
                                reason: "SimulatedUserMode::Client requires a simulated_user_fn"
                                    .to_owned(),
                            }
                        })?;
                        let message = callback.invoke(&scenario_id, turn, &history).await?;
                        state.submit_user_turn(UserTurnSubmission {
                            scenario_id,
                            turn,
                            message,
                        })?;
                    }
                }
                TurnDirective::ScenarioComplete { scenario_id } => {
                    let cursor = state.take_completed_scenario(&scenario_id)?;
                    let result = self.scoring.score_scenario(&cursor).await?;
                    aggregation_inputs.push(self.scoring.scenario_aggregation(&cursor, &result)?);
                    scenario_results.push(result);
                    state.ack_scenario_complete();
                }
                TurnDirective::RunComplete => {
                    state.ack_run_complete();
                    break;
                }
            }
        }

        let identity = RunIdentity {
            run_id: state.run_id,
            eval_ref,
            started_at: state.opened_at,
            ended_at: Utc::now(),
        };
        let run = self.scoring.finalize(identity, aggregation_inputs)?;
        Ok(EmbeddedOutcome {
            scenarios: scenario_results,
            run,
        })
    }
}
