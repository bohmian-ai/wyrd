//! Local agent-driving flow for `wyrd eval run`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use vala_eval::orchestrator::{
    AgentTurnFn, AgentTurnReply, EmbeddedOrchestrator, OrchestratorError, SimulatedUserFn,
};

use crate::error::WyrdCliError;
use crate::eval::agent::AgentClient;
use crate::eval::output;
use crate::eval::run::{self, EvalRunArgs, SimulatedUserCli};

/// Run `--local --agent-url`.
///
/// # Errors
/// Returns an error when inputs, agent calls, orchestration, scoring, or output
/// writing fail.
pub async fn run_local_agent(args: EvalRunArgs) -> Result<ExitCode, WyrdCliError> {
    let (eval_ref, spec) = run::load_eval_card(&args.eval)?;
    let scenarios_path = args
        .scenarios
        .as_deref()
        .ok_or(WyrdCliError::ScenariosRequired)?;
    let scenarios = vala_eval::load_scenario_collection(scenarios_path)
        .map_err(|source| WyrdCliError::EvalPlan { source })?
        .scenarios;
    let judge = run::judge_for_spec(&spec, args.judge_mock)?;
    let scoring =
        vala_eval::orchestrator::ScenarioScoring::with_in_memory_traces(Arc::new(spec), judge)
            .map_err(|source| WyrdCliError::Orchestrator { source })?;
    let orchestrator = EmbeddedOrchestrator {
        scoring,
        simulator: None,
    };
    let agent_url = args
        .agent_url
        .clone()
        .ok_or(WyrdCliError::ServerRequiresAgentUrl)?;
    let agent = Arc::new(LocalAgent {
        client: AgentClient::new(agent_url, Duration::from_secs(args.agent_timeout_secs))?,
    });
    let simulated_user = match args.simulated_user {
        SimulatedUserCli::Server => None,
        SimulatedUserCli::Client => Some(Arc::new(ScriptedUser {
            rows: run::load_scripted_user(
                args.simulated_user_script
                    .as_deref()
                    .ok_or(WyrdCliError::SimulatedUserScriptRequired)?,
            )?,
        }) as Arc<dyn SimulatedUserFn>),
    };
    let outcome = orchestrator
        .drive(
            eval_ref,
            args.simulated_user.into(),
            scenarios,
            agent,
            simulated_user,
        )
        .await
        .map_err(|source| WyrdCliError::Orchestrator { source })?;
    let comparison = output::load_comparison(args.baseline.as_deref(), &outcome.run)?;
    let out_dir = output::write_results(args.out.as_ref(), &outcome.run, comparison.as_ref())?;
    output::print_local_summary(&outcome.run, comparison.as_ref(), Some(&out_dir));

    if args.fail_on_gate && !output::pass_gate_satisfied(&outcome.run) {
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

struct LocalAgent {
    client: AgentClient,
}

#[async_trait]
impl AgentTurnFn for LocalAgent {
    async fn invoke(
        &self,
        message: &str,
        history: &[wyrd_spec::vala::eval::protocol::ConversationTurn],
    ) -> Result<AgentTurnReply, OrchestratorError> {
        let (response, records) = self
            .client
            .post_turn(None, None, message, run::history_ref(history))
            .await
            .map_err(|error| OrchestratorError::EmbeddedCallback {
                reason: error.to_string(),
            })?;
        Ok(AgentTurnReply { response, records })
    }
}

struct ScriptedUser {
    rows: run::ScriptedUserRows,
}

#[async_trait]
impl SimulatedUserFn for ScriptedUser {
    async fn invoke(
        &self,
        scenario_id: &wyrd_spec::vala::eval::ScenarioId,
        turn: u32,
        history: &[wyrd_spec::vala::eval::protocol::ConversationTurn],
    ) -> Result<String, OrchestratorError> {
        let _ = history;
        run::scripted_message(&self.rows, scenario_id, turn).map_err(|error| {
            OrchestratorError::EmbeddedCallback {
                reason: error.to_string(),
            }
        })
    }
}

#[allow(dead_code)]
fn _assert_out_path(_: &PathBuf) {}
