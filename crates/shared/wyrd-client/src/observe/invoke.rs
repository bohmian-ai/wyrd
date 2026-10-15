//! One governed Agent invocation inside an application [`Run`].
//!
//! The view's Agent Card and Prompt come from the hydrated graph. On a
//! `wyrd_gateway` route the model call goes through the public gateway as the
//! state's client, carrying the Run and Agent Card UID as gateway correlation,
//! so gateway authorization, accounting, capture, and audit apply; a `native`
//! route uses this host's provider configuration.

use std::sync::Arc;

use serde_json::json;
use skald_agent::{
    Agent, AgentError, run_config_from_agent_run_config_spec, wyrd_gateway_registry,
};
use skald_providers::ProviderError;
use skald_runtime::SkaldRuntimeError;
use skald_spec::ProviderName;
use wyrd_spec::ExecutionRoute;
use wyrd_spec::error::WyrdError;

use super::Run;
use crate::error::code_to_wyrd_error;
use crate::gateway_caller::PublicWyrdGatewayCaller;

impl Run {
    /// Invoke this view's tool-free Agent once on its declared execution
    /// route with string `variables` and return its final text.
    ///
    /// Equivalent to [`Self::invoke_on`] with the Agent Card's
    /// `execution_route`.
    ///
    /// # Arguments
    /// * `variables` - Prompt template variables as name/value pairs.
    ///
    /// # Errors
    /// Returns the errors of [`Self::invoke_on`].
    pub async fn invoke(&self, variables: &[(&str, &str)]) -> Result<String, WyrdError> {
        let route = self
            .state
            .agent(&self.alias)?
            .card()
            .spec
            .execution_route
            .clone();
        self.invoke_on(&route, variables).await
    }

    /// Invoke this view's tool-free Agent once on `route`, which replaces the
    /// Agent's declared route whole for this invocation only, and return its
    /// final text.
    ///
    /// The Agent's Prompt is rendered with `variables`. A `native` route calls
    /// the Prompt's provider with this host's provider configuration. A
    /// `wyrd_gateway` route sends every model call to `<deployment>/<Prompt
    /// model>` through the public gateway as the state's client, with this
    /// Run's `run_id` and the Agent Card UID as correlation, so the target is
    /// authorized for the caller exactly as any other gateway call. The Card,
    /// its hash, and concurrent invocations are unaffected. Every refusal below
    /// except a gateway or provider one happens before any IO.
    ///
    /// # Arguments
    /// * `route` - Execution route for this invocation.
    /// * `variables` - Prompt template variables as name/value pairs.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_CARD_KIND_MISMATCH` when this view is not an
    /// Agent; `WYRD_AGENT_422_VALIDATION` when the Agent declares tools, its
    /// Prompt model is not a gateway model identifier, or a gateway route
    /// targets a Vertex Prompt, which the public gateway ingress does not
    /// serve; `WYRD_SDK_400_INVALID_STATE_BUNDLE` when its Prompt is
    /// unresolved; the gateway's own stable error (such as
    /// `WYRD_PERMISSION_403_DENIED_RBAC` or `WYRD_GATEWAY_404_MODEL_UNAVAILABLE`)
    /// when it refuses the call; and the Agent runtime's error for any other
    /// failure, including a native provider this host has not configured.
    pub async fn invoke_on(
        &self,
        route: &ExecutionRoute,
        variables: &[(&str, &str)],
    ) -> Result<String, WyrdError> {
        let hydrated = self.state.agent(&self.alias)?;
        let card = hydrated.card();
        if !card.spec.tool_names.is_empty() {
            return Err(invalid_agent(
                &self.alias,
                "Run.invoke supports only a tool-free Agent",
            ));
        }
        let prompt = &hydrated.prompt().metadata.prompt;
        let mut agent = Agent::new(skald_prompt::Prompt::from_native(prompt.clone()))
            .with_id(card.name.clone())
            .with_tools(std::iter::empty())
            .with_run_config(run_config_from_agent_run_config_spec(&card.spec.run_config));
        if let ExecutionRoute::WyrdGateway { deployment } = route {
            if prompt.provider() == ProviderName::Vertex {
                return Err(invalid_agent(
                    &self.alias,
                    "a local wyrd_gateway route cannot target a Vertex Prompt",
                ));
            }
            let caller = PublicWyrdGatewayCaller::new(self.state.client()?.clone())
                .with_subject(Some(&self.run_id), Some(self.subject_uid()));
            let registry =
                wyrd_gateway_registry(Arc::new(caller), prompt, deployment).ok_or_else(|| {
                    invalid_agent(
                        &self.alias,
                        "the Agent Prompt's model is not a gateway model identifier",
                    )
                })?;
            agent = agent.with_provider_registry(Arc::new(registry));
        }
        agent
            .run_prompt(
                &skald_runtime::default_registry(),
                agent.prompt(),
                variables,
            )
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
