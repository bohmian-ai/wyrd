//! HTTP client for the user-supplied agent endpoint.

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;
use wyrd_spec::vala::eval::ScenarioId;
use wyrd_spec::vala::eval::protocol::ConversationTurn;
use wyrd_spec::vala::eval::record::EvalRecordObservation;

use crate::error::WyrdCliError;

/// Agent endpoint client.
pub struct AgentClient {
    http: Client,
    endpoint: Url,
}

#[derive(Debug, Serialize)]
struct AgentTurnRequest<'a> {
    message: &'a str,
    history: &'a [ConversationTurn],
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_id: Option<&'a ScenarioId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AgentTurnResponse {
    response: String,
    #[serde(default)]
    records: Vec<EvalRecordObservation>,
}

impl AgentClient {
    /// Build a client for one agent endpoint.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn new(endpoint: Url, timeout: Duration) -> Result<Self, WyrdCliError> {
        Ok(Self {
            http: Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|source| WyrdCliError::HttpBuild { source })?,
            endpoint,
        })
    }

    /// POST one turn to the agent endpoint.
    ///
    /// # Errors
    /// Returns an error when the endpoint fails or returns malformed JSON.
    pub async fn post_turn(
        &self,
        scenario_id: Option<&ScenarioId>,
        turn: Option<u32>,
        message: &str,
        history: &[ConversationTurn],
    ) -> Result<(String, Vec<EvalRecordObservation>), WyrdCliError> {
        let body = AgentTurnRequest {
            message,
            history,
            scenario_id,
            turn,
        };
        let response: AgentTurnResponse = self
            .http
            .post(self.endpoint.clone())
            .json(&body)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|source| WyrdCliError::Http { source })?
            .json()
            .await
            .map_err(|source| WyrdCliError::Http { source })?;
        Ok((response.response, response.records))
    }
}
