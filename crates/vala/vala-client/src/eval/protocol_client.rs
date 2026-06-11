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

use super::routes;

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

/// Shared HTTP client for the server pull protocol.
///
/// One `ProtocolClient` may drive multiple sequential `run_eval` calls without
/// rebuilding the underlying connection pool. Bearer auth is per-run (issued by
/// the server on open) and lives inside `run_eval`, not here.
pub struct ProtocolClient {
    http: Client,
    server_url: Url,
}

impl ProtocolClient {
    /// Build a client for one server URL.
    ///
    /// # Errors
    /// Returns [`ProtocolClientError::HttpBuild`] when reqwest cannot
    /// initialize the connection pool or TLS stack.
    pub fn new(server_url: Url, request_timeout: Duration) -> Result<Self, ProtocolClientError> {
        let http = Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(|source| ProtocolClientError::HttpBuild {
                message: source.to_string(),
            })?;
        Ok(Self { http, server_url })
    }

    /// Run the server pull protocol to completion.
    ///
    /// # Errors
    /// Returns [`ProtocolClientError`] for transport, malformed protocol
    /// response, or callback failures.
    pub fn run_eval(
        &self,
        eval_ref: CardRef,
        simulated_user: SimulatedUserMode,
        mut agent_fn: AgentFn,
        mut simulated_user_fn: Option<SimulatedUserFn>,
    ) -> Result<RunSummary, ProtocolClientError> {
        let open_url = join_url(&self.server_url, routes::RUNS_BASE, "open")?;
        let open: EvalRunOpenResponse = self
            .http
            .post(open_url)
            .json(&EvalRunOpenRequest {
                eval_ref,
                simulated_user,
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
        let run_base = format!("{}/{}/", routes::RUNS_BASE, open.run_id);
        let next_url = join_url(&self.server_url, &format!("{}{}", run_base, routes::NEXT), "next")?;
        let agent_url = join_url(&self.server_url, &format!("{}{}", run_base, routes::AGENT_TURN), "agent-turn")?;
        let user_url = join_url(&self.server_url, &format!("{}{}", run_base, routes::USER_TURN), "user-turn")?;

        loop {
            let directive: TurnDirective = send_with_retry("next", || {
                self.http
                    .post(next_url.clone())
                    .header(reqwest::header::AUTHORIZATION, &bearer)
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
            })?
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
                    send_with_retry("agent-turn", || {
                        self.http
                            .post(agent_url.clone())
                            .header(reqwest::header::AUTHORIZATION, &bearer)
                            .json(&AgentTurnSubmission {
                                scenario_id: scenario_id.clone(),
                                turn,
                                response: output.response.clone(),
                                records: output.records.clone(),
                            })
                            .send()
                            .and_then(reqwest::blocking::Response::error_for_status)
                    })?;
                }
                TurnDirective::UserTurnNeeded {
                    scenario_id,
                    turn,
                    history,
                } => {
                    let callback = simulated_user_fn.as_mut().ok_or_else(|| {
                        ProtocolClientError::SimulatedUserFn {
                            message: "server requested a client-delegated user turn but no simulated_user_fn was supplied".to_string(),
                        }
                    })?;
                    let message = callback(&history)?;
                    send_with_retry("user-turn", || {
                        self.http
                            .post(user_url.clone())
                            .header(reqwest::header::AUTHORIZATION, &bearer)
                            .json(&UserTurnSubmission {
                                scenario_id: scenario_id.clone(),
                                turn,
                                message: message.clone(),
                            })
                            .send()
                            .and_then(reqwest::blocking::Response::error_for_status)
                    })?;
                }
                TurnDirective::ScenarioComplete { .. } => {}
                TurnDirective::RunComplete => break,
            }
        }

        Ok(RunSummary {
            run_id: open.run_id,
            server_url: self.server_url.to_string(),
        })
    }
}

/// Retry a request closure on network-level failures (timeout or connection error).
///
/// Does not retry on 5xx — agent-turn and user-turn are non-idempotent; a 5xx
/// may or may not have been processed by the server.
fn send_with_retry(
    operation: &'static str,
    mut f: impl FnMut() -> Result<reqwest::blocking::Response, reqwest::Error>,
) -> Result<reqwest::blocking::Response, ProtocolClientError> {
    const MAX_ATTEMPTS: u8 = 3;
    for attempt in 0..MAX_ATTEMPTS {
        match f() {
            Ok(response) => return Ok(response),
            Err(error)
                if (error.is_timeout() || error.is_connect()) && attempt + 1 < MAX_ATTEMPTS =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(http_error(operation, error)),
        }
    }
    unreachable!("loop exits via Ok or Err before exhausting attempts")
}

/// Map a URL parse failure to the typed [`ProtocolClientError::Url`] variant.
fn join_url(base: &Url, path: &str, operation: &'static str) -> Result<Url, ProtocolClientError> {
    base.join(path).map_err(|source| ProtocolClientError::Url {
        operation,
        message: source.to_string(),
    })
}

/// Map a reqwest error to [`ProtocolClientError::Http`], preserving the HTTP status code.
fn http_error(operation: &'static str, source: reqwest::Error) -> ProtocolClientError {
    ProtocolClientError::Http {
        operation,
        message: source.to_string(),
        status: source.status().map(|status| status.as_u16()),
    }
}
