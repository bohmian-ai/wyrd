//! One governed Agent invocation inside an application [`Run`].
//!
//! The view's Agent Card and Prompt come from the hydrated graph; the model
//! call goes through the public gateway as the state's client, carrying the
//! Run and Agent Card UID as gateway correlation, so gateway authorization,
//! accounting, capture, and audit apply. No provider credential is held here.

use std::sync::Arc;

use serde_json::json;
use skald_agent::{Agent, AgentError, run_config_from_agent_run_config_spec};
use skald_providers::ProviderError;
use skald_runtime::{ProviderRegistry, SkaldRuntimeError};
use skald_workflow::wyrd_gateway_registry;
use wyrd_spec::error::WyrdError;

use super::Run;
use crate::error::code_to_wyrd_error;
use crate::workflow::PublicWyrdGatewayCaller;

impl Run {
    /// Invoke this view's tool-free Agent once with string `variables` and
    /// return its final text.
    ///
    /// The Agent's Prompt is rendered with `variables` and sent through the
    /// gateway with this Run's `run_id` and the Agent Card UID as correlation.
    /// Every refusal below except a gateway one happens before any IO.
    ///
    /// # Arguments
    /// * `variables` - Prompt template variables as name/value pairs.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_CARD_KIND_MISMATCH` when this view is not an
    /// Agent; `WYRD_AGENT_422_VALIDATION` when the Agent declares tools or its
    /// Prompt's provider and model are not a gateway model identity;
    /// `WYRD_SDK_400_INVALID_STATE_BUNDLE` when its Prompt is unresolved; the
    /// gateway's own stable error (such as `WYRD_PERMISSION_403_DENIED_RBAC`
    /// or `WYRD_GATEWAY_404_MODEL_UNAVAILABLE`) when it refuses the call; and
    /// the Agent runtime's error for any other failure.
    pub async fn invoke(&self, variables: &[(&str, &str)]) -> Result<String, WyrdError> {
        let card = self.state.agent(&self.alias)?;
        if !card.spec.tool_names.is_empty() {
            return Err(invalid_agent(
                &self.alias,
                "Run.invoke supports only a tool-free Agent",
            ));
        }
        let prompt = self.state.resolve_agent_prompt(&self.alias)?;
        let caller = PublicWyrdGatewayCaller::new(self.state.client()?.clone())
            .with_subject(&self.run_id, self.subject_uid());
        let registry = wyrd_gateway_registry(Arc::new(caller), prompt).ok_or_else(|| {
            invalid_agent(
                &self.alias,
                "the Agent Prompt's provider and model are not a gateway model identity",
            )
        })?;
        let agent = Agent::new(skald_prompt::Prompt::from_native(prompt.clone()))
            .with_id(card.name.clone())
            .with_tools(std::iter::empty())
            .with_run_config(run_config_from_agent_run_config_spec(&card.spec.run_config))
            .with_provider_registry(Arc::new(registry));
        agent
            .run_prompt(&ProviderRegistry::default(), agent.prompt(), variables)
            .await
            .map(|run| run.output)
            .map_err(|error| agent_failure(&error))
    }
}

/// The stable refusal of an Agent `alias` that `Run::invoke` cannot run.
fn invalid_agent(alias: &str, reason: &str) -> WyrdError {
    WyrdError::AgentValidation {
        message: format!("Agent `{alias}`: {reason}"),
        details: json!({ "alias": alias }),
    }
}

/// An Agent failure as its public error: a gateway refusal keeps the
/// gateway's own stable code; every other failure uses the Agent catalog.
fn agent_failure(error: &AgentError) -> WyrdError {
    match error {
        AgentError::Provider(SkaldRuntimeError::Provider {
            source: ProviderError::RemoteProblem(problem),
            ..
        }) => code_to_wyrd_error(&problem.code, problem.message.clone(), json!({})),
        error => WyrdError::from(error),
    }
}
