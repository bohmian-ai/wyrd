//! The eval pull-protocol turn loop.
//!
//! Every Wyrd call here goes through [`EvalProtocol`], which owns the four
//! protocol routes and the per-run lease. This module owns only the loop: pull a
//! directive, get an answer from the agent or the script, report it back.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use url::Url;
use wyrd_client::eval::{
    AgentTurnSubmission, EvalProtocol, EvalRunOpenRequest, TurnDirective, UserTurnSubmission,
};

use crate::error::WyrdCliError;
use crate::eval::agent::AgentClient;
use crate::eval::output;
use crate::eval::run::{self, EvalRunArgs, SimulatedUserCli};

/// Run `--server <URL> --agent-url <URL>`.
///
/// # Errors
/// Returns an error when protocol calls, agent calls, or scripted user turns
/// fail.
pub async fn run_server(args: EvalRunArgs, server_url: Url) -> Result<ExitCode, WyrdCliError> {
    let (eval_ref, _) = run::load_eval_card(&args.eval)?;
    let agent_url = args
        .agent_url
        .clone()
        .ok_or(WyrdCliError::ServerRequiresAgentUrl)?;

    // One client for both planes: the protocol calls authenticate with it, and
    // the agent calls reuse its pool through the credential-free external seam.
    let client = Arc::new(crate::client::from_global(Some(server_url.as_str()))?);
    let agent = AgentClient::new(
        Arc::clone(&client),
        agent_url,
        Duration::from_secs(args.agent_timeout_secs),
    );
    let run_handle = EvalProtocol::with_shared(client)
        .open(&EvalRunOpenRequest {
            eval_ref,
            simulated_user: args.simulated_user.into(),
        })
        .await?;

    let scripted_user = match args.simulated_user {
        SimulatedUserCli::Server => None,
        SimulatedUserCli::Client => Some(run::load_scripted_user(
            args.simulated_user_script
                .as_deref()
                .ok_or(WyrdCliError::SimulatedUserScriptRequired)?,
        )?),
    };

    loop {
        match run_handle.next_directive().await? {
            TurnDirective::AgentTurn {
                scenario_id,
                turn,
                message,
                history,
            } => {
                let (response, records) = agent
                    .post_turn(Some(&scenario_id), Some(turn), &message, &history)
                    .await?;
                run_handle
                    .submit_agent_turn(&AgentTurnSubmission {
                        scenario_id,
                        turn,
                        response,
                        records,
                    })
                    .await?;
            }
            TurnDirective::UserTurnNeeded {
                scenario_id, turn, ..
            } => {
                let script = scripted_user
                    .as_ref()
                    .ok_or(WyrdCliError::SimulatedUserScriptRequired)?;
                let message = run::scripted_message(script, &scenario_id, turn)?;
                run_handle
                    .submit_user_turn(&UserTurnSubmission {
                        scenario_id,
                        turn,
                        message,
                    })
                    .await?;
            }
            TurnDirective::ScenarioComplete { .. } => {}
            TurnDirective::RunComplete => break,
        }
    }

    output::print_server_run_complete(&server_url, run_handle.run_id());
    Ok(ExitCode::SUCCESS)
}
