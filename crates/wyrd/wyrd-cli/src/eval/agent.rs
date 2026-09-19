//! Client for the user-supplied agent endpoint.
//!
//! The agent under evaluation is not a Wyrd surface: it is whatever URL the
//! operator passed to `--agent-url`, and it must never receive the caller's Wyrd
//! credential. The calls therefore go through
//! [`WyrdClient::request_external_stream`], the shared client's credential-free
//! cross-origin seam, so the CLI still owns no HTTP client of its own while the
//! agent sees no Wyrd token.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;
use wyrd_client::WyrdClient;
use wyrd_spec::vala::eval::ScenarioId;
use wyrd_spec::vala::eval::protocol::ConversationTurn;
use wyrd_spec::vala::eval::record::EvalRecordObservation;

use crate::error::WyrdCliError;

/// Agent endpoint client.
pub struct AgentClient {
    /// Shared client, used only for its credential-free external seam and its
    /// connection pool.
    client: Arc<WyrdClient>,
    /// The operator-supplied agent endpoint every turn is posted to.
    endpoint: Url,
    /// Per-turn deadline from `--agent-timeout-secs`.
    ///
    /// Applied by this client rather than by the transport: the external seam
    /// deliberately bounds only connection setup, because its other callers
    /// stream large storage bodies that must not hit a total deadline. An agent
    /// turn is a small request/response and does need one.
    timeout: Duration,
}

/// One turn posted to the agent endpoint.
#[derive(Debug, Serialize)]
struct AgentTurnRequest<'a> {
    /// The message the agent must answer.
    message: &'a str,
    /// Conversation so far, oldest first.
    history: &'a [ConversationTurn],
    /// Scenario the turn belongs to, when the server named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_id: Option<&'a ScenarioId>,
    /// Turn index within the scenario, when the server named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    turn: Option<u32>,
}

/// The agent's answer and anything it observed while producing it.
#[derive(Debug, Deserialize)]
struct AgentTurnResponse {
    /// The agent's reply text.
    response: String,
    /// Observations the agent emitted, submitted onward with the turn.
    #[serde(default)]
    records: Vec<EvalRecordObservation>,
}

impl AgentClient {
    /// Build a client for one agent endpoint over the shared Wyrd client.
    #[must_use]
    pub fn new(client: Arc<WyrdClient>, endpoint: Url, timeout: Duration) -> Self {
        Self {
            client,
            endpoint,
            timeout,
        }
    }

    /// POST one turn to the agent endpoint.
    ///
    /// The request carries no Wyrd credential. The whole exchange, including
    /// reading the response body, is bounded by the configured timeout, so a
    /// hung agent stalls one turn rather than the run.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::AgentTurnFailed`] when the endpoint is
    /// unreachable, exceeds the timeout, answers with a non-success status, or
    /// returns a body that is not the expected JSON.
    pub async fn post_turn(
        &self,
        scenario_id: Option<&ScenarioId>,
        turn: Option<u32>,
        message: &str,
        history: &[ConversationTurn],
    ) -> Result<(String, Vec<EvalRecordObservation>), WyrdCliError> {
        let body = serde_json::to_vec(&AgentTurnRequest {
            message,
            history,
            scenario_id,
            turn,
        })
        .map_err(|source| WyrdCliError::AgentTurnFailed {
            detail: format!("request serialization failed: {source}"),
        })?;

        tokio::time::timeout(self.timeout, self.exchange(body))
            .await
            .map_err(|_| WyrdCliError::AgentTurnFailed {
                detail: format!("agent did not answer within {:?}", self.timeout),
            })?
    }

    /// Send one serialized turn and decode the agent's answer.
    ///
    /// Split from [`Self::post_turn`] so the whole exchange, request and body
    /// read together, sits inside one timeout.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::AgentTurnFailed`] for transport failures, a
    /// non-success status, or a malformed body.
    async fn exchange(
        &self,
        body: Vec<u8>,
    ) -> Result<(String, Vec<EvalRecordObservation>), WyrdCliError> {
        let response = self
            .client
            .request_external_stream(
                reqwest::Method::POST,
                self.endpoint.as_str(),
                Some(reqwest::Body::from(body)),
                &[("content-type", "application/json")],
            )
            .await
            .map_err(|source| WyrdCliError::AgentTurnFailed {
                detail: source.to_string(),
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(WyrdCliError::AgentTurnFailed {
                detail: format!("agent returned status {status}"),
            });
        }

        let decoded: AgentTurnResponse =
            response
                .json()
                .await
                .map_err(|source| WyrdCliError::AgentTurnFailed {
                    detail: format!("agent response was not the expected JSON: {source}"),
                })?;
        Ok((decoded.response, decoded.records))
    }
}
