//! Thin synchronous client for the server-hosted Vala eval pull protocol.
//!
//! This module opens a run, loops on `POST /api/v1/eval/runs/{run_id}/next`,
//! and delegates only agent and optional client-simulated user turns to caller
//! callbacks. Scenario loading, turn cursor, termination, scoring, and result
//! storage stay server-side.

use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, ConversationTurn, EvalRunOpenRequest, EvalRunOpenResponse,
    SimulatedUserMode, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::ids::RunId;

/// Output returned by the caller's agent callback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTurnOutput {
    /// Agent response text to submit for the current turn.
    pub response: String,
    /// Eval records emitted by the agent for this turn.
    #[serde(default)]
    pub records: Vec<EvalRecordObservation>,
}

/// Callback invoked for each server-requested agent turn.
pub type AgentFn = Box<
    dyn FnMut(&str, &[ConversationTurn]) -> Result<AgentTurnOutput, ProtocolClientError> + Send,
>;

/// Callback invoked for client-delegated simulated-user turns.
pub type SimulatedUserFn =
    Box<dyn FnMut(&[ConversationTurn]) -> Result<String, ProtocolClientError> + Send>;

/// Minimal summary returned when the server emits `RunComplete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Existing Wyrd/Vala run identifier returned by the server.
    pub run_id: RunId,
    /// Server base URL that owns the run.
    pub server_url: String,
}

/// Configuration for one protocol-client run.
#[derive(Debug, Clone)]
pub struct RunEvalConfig {
    /// Wyrd server base URL.
    pub server_url: Url,
    /// Eval card reference to run.
    pub eval_ref: CardRef,
    /// Source for non-scripted user turns.
    pub simulated_user: SimulatedUserMode,
    /// Per-request HTTP timeout.
    pub request_timeout: Duration,
}

/// Errors raised by the protocol client.
#[derive(Debug, Error)]
pub enum ProtocolClientError {
    /// HTTP client construction failed.
    #[error("failed to build HTTP client: {message}")]
    HttpBuild {
        /// Transport detail.
        message: String,
    },
    /// HTTP request or response status failed.
    #[error("HTTP {operation} failed: {message}")]
    Http {
        /// Protocol operation being attempted.
        operation: &'static str,
        /// Transport detail.
        message: String,
        /// HTTP status code when the server returned one.
        status: Option<u16>,
    },
    /// URL joining failed.
    #[error("invalid protocol URL for {operation}: {message}")]
    Url {
        /// Protocol operation being attempted.
        operation: &'static str,
        /// URL parse detail.
        message: String,
    },
    /// Server response did not match the locked protocol.
    #[error("malformed protocol response for {operation}: {message}")]
    Malformed {
        /// Protocol operation being decoded.
        operation: &'static str,
        /// Decode detail.
        message: String,
    },
    /// `agent_fn` failed.
    #[error("agent_fn raised: {message}")]
    AgentFn {
        /// Callback detail.
        message: String,
    },
    /// `simulated_user_fn` failed or was required but absent.
    #[error("simulated_user_fn failed: {message}")]
    SimulatedUserFn {
        /// Callback detail.
        message: String,
    },
}

/// Run the server pull protocol to completion.
///
/// # Errors
/// Returns [`ProtocolClientError`] for transport, malformed protocol response,
/// or callback failures.
pub fn run_eval(
    config: RunEvalConfig,
    mut agent_fn: AgentFn,
    mut simulated_user_fn: Option<SimulatedUserFn>,
) -> Result<RunSummary, ProtocolClientError> {
    let http = Client::builder()
        .timeout(config.request_timeout)
        .build()
        .map_err(|source| ProtocolClientError::HttpBuild {
            message: source.to_string(),
        })?;

    let open_url = join_url(&config.server_url, "api/v1/eval/runs", "open")?;
    let open: EvalRunOpenResponse = http
        .post(open_url)
        .json(&EvalRunOpenRequest {
            eval_ref: config.eval_ref,
            simulated_user: config.simulated_user,
        })
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|source| http_error("open", source))?
        .json()
        .map_err(|source| ProtocolClientError::Malformed {
            operation: "open",
            message: source.to_string(),
        })?;

    let bearer = format!("Bearer {}", open.lease_token.as_str());
    let run_path = format!("api/v1/eval/runs/{}/", open.run_id);
    let next_url = join_url(&config.server_url, &(run_path.clone() + "next"), "next")?;
    let agent_url = join_url(
        &config.server_url,
        &(run_path.clone() + "agent-turn"),
        "agent-turn",
    )?;
    let user_url = join_url(&config.server_url, &(run_path + "user-turn"), "user-turn")?;

    loop {
        let directive: TurnDirective = http
            .post(next_url.clone())
            .header(reqwest::header::AUTHORIZATION, &bearer)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| http_error("next", source))?
            .json()
            .map_err(|source| ProtocolClientError::Malformed {
                operation: "next",
                message: source.to_string(),
            })?;

        match directive {
            TurnDirective::AgentTurn {
                scenario_id,
                turn,
                message,
                history,
            } => {
                let output = agent_fn(&message, &history)?;
                http.post(agent_url.clone())
                    .header(reqwest::header::AUTHORIZATION, &bearer)
                    .json(&AgentTurnSubmission {
                        scenario_id,
                        turn,
                        response: output.response,
                        records: output.records,
                    })
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
                    .map_err(|source| http_error("agent-turn", source))?;
            }
            TurnDirective::UserTurnNeeded {
                scenario_id,
                turn,
                history,
            } => {
                let callback =
                    simulated_user_fn
                        .as_mut()
                        .ok_or_else(|| ProtocolClientError::SimulatedUserFn {
                            message: "server requested a client-delegated user turn but no simulated_user_fn was supplied".to_string(),
                        })?;
                let message = callback(&history)?;
                http.post(user_url.clone())
                    .header(reqwest::header::AUTHORIZATION, &bearer)
                    .json(&UserTurnSubmission {
                        scenario_id,
                        turn,
                        message,
                    })
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
                    .map_err(|source| http_error("user-turn", source))?;
            }
            TurnDirective::ScenarioComplete { .. } => {}
            TurnDirective::RunComplete => break,
        }
    }

    Ok(RunSummary {
        run_id: open.run_id,
        server_url: config.server_url.to_string(),
    })
}

fn join_url(base: &Url, path: &str, operation: &'static str) -> Result<Url, ProtocolClientError> {
    base.join(path).map_err(|source| ProtocolClientError::Url {
        operation,
        message: source.to_string(),
    })
}

fn http_error(operation: &'static str, source: reqwest::Error) -> ProtocolClientError {
    ProtocolClientError::Http {
        operation,
        message: source.to_string(),
        status: source.status().map(|status| status.as_u16()),
    }
}
