//! Thin eval pull-protocol client.

use std::process::ExitCode;
use std::time::Duration;

use reqwest::Client;
use url::Url;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective, UserTurnSubmission,
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
    let access = format!(
        "Bearer {}",
        args.token
            .as_deref()
            .ok_or(WyrdCliError::ServerRequiresToken)?
    );
    let http = Client::builder()
        .timeout(Duration::from_secs(args.agent_timeout_secs.max(120)))
        .build()
        .map_err(|source| WyrdCliError::HttpBuild { source })?;
    let open_url = server_url
        .join("v1/eval/runs")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;
    let open: EvalRunOpenResponse = http
        .post(open_url)
        .header("x-wyrd-access-token", &access)
        .json(&EvalRunOpenRequest {
            eval_ref,
            simulated_user: args.simulated_user.into(),
        })
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|source| WyrdCliError::Http { source })?
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;
    let bearer = format!("Bearer {}", open.lease_token.as_str());
    let run_path = format!("v1/eval/runs/{}/", open.run_id);
    let next_url = server_url
        .join(&(run_path.clone() + "next"))
        .map_err(|source| WyrdCliError::UrlJoin { source })?;
    let agent_turn_url = server_url
        .join(&(run_path.clone() + "agent-turn"))
        .map_err(|source| WyrdCliError::UrlJoin { source })?;
    let user_turn_url = server_url
        .join(&(run_path + "user-turn"))
        .map_err(|source| WyrdCliError::UrlJoin { source })?;
    let agent = AgentClient::new(
        args.agent_url
            .clone()
            .ok_or(WyrdCliError::ServerRequiresAgentUrl)?,
        Duration::from_secs(args.agent_timeout_secs),
    )?;
    let scripted_user = match args.simulated_user {
        SimulatedUserCli::Server => None,
        SimulatedUserCli::Client => Some(run::load_scripted_user(
            args.simulated_user_script
                .as_deref()
                .ok_or(WyrdCliError::SimulatedUserScriptRequired)?,
        )?),
    };

    loop {
        let directive: TurnDirective = http
            .post(next_url.clone())
            .header(reqwest::header::AUTHORIZATION, &bearer)
            .header("x-wyrd-access-token", &access)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|source| WyrdCliError::Http { source })?
            .json()
            .await
            .map_err(|source| WyrdCliError::Http { source })?;

        match directive {
            TurnDirective::AgentTurn {
                scenario_id,
                turn,
                message,
                history,
            } => {
                let (response, records) = agent
                    .post_turn(Some(&scenario_id), Some(turn), &message, &history)
                    .await?;
                http.post(agent_turn_url.clone())
                    .header(reqwest::header::AUTHORIZATION, &bearer)
                    .header("x-wyrd-access-token", &access)
                    .json(&AgentTurnSubmission {
                        scenario_id,
                        turn,
                        response,
                        records,
                    })
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|source| WyrdCliError::Http { source })?;
            }
            TurnDirective::UserTurnNeeded {
                scenario_id, turn, ..
            } => {
                let script = scripted_user
                    .as_ref()
                    .ok_or(WyrdCliError::SimulatedUserScriptRequired)?;
                let message = run::scripted_message(script, &scenario_id, turn)?;
                http.post(user_turn_url.clone())
                    .header(reqwest::header::AUTHORIZATION, &bearer)
                    .header("x-wyrd-access-token", &access)
                    .json(&UserTurnSubmission {
                        scenario_id,
                        turn,
                        message,
                    })
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|source| WyrdCliError::Http { source })?;
            }
            TurnDirective::ScenarioComplete { .. } => {}
            TurnDirective::RunComplete => break,
        }
    }

    output::print_server_run_complete(&server_url, &open.run_id);
    Ok(ExitCode::SUCCESS)
}
