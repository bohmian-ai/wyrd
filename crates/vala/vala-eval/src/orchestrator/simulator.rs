//! Server-side simulated user support.

use std::sync::Arc;

use serde_json::json;
use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::ProviderRegistry;
use wyrd_spec::vala::eval::EvalScenario;
use wyrd_spec::vala::eval::protocol::{ConversationTurn, SimulatedUserTurn, TurnRole};

use super::OrchestratorError;

/// Prompt template for the server-side simulated user.
pub const SIMULATOR_PROMPT_TEMPLATE: &str = "\
You are role-playing a user interacting with an AI agent. Stay strictly in \
character; do not break the fourth wall, narrate your own actions, or refer \
to yourself as a simulator.\n\
\n\
PERSONA\n\
{{persona}}\n\
\n\
GOAL\n\
Your initial request was: {{initial_query}}\n\
You consider the conversation a success when: {{expected_outcome}}\n\
\n\
CONVERSATION SO FAR\n\
{{history}}\n\
\n\
TASK\n\
Write the next message YOU (the user) would send. Then judge whether your \
goal has been achieved by the agent's most recent response.\n\
\n\
Respond with a JSON object matching this schema exactly:\n\
{\n\
  \"message\": string,\n\
  \"goal_achieved\": boolean\n\
}\n";

/// Rendered simulator prompt and Skald prompt handle.
#[derive(Debug, Clone)]
pub struct SimulatorPrompt {
    /// Fully rendered prompt text.
    pub rendered: String,
    /// Runtime prompt carrying the structured-output schema.
    pub prompt: Prompt,
}

impl SimulatorPrompt {
    /// Render the simulator prompt for one scenario and history snapshot.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the prompt schema cannot be compiled.
    pub fn render(
        scenario: &EvalScenario,
        history: &[ConversationTurn],
    ) -> Result<Self, OrchestratorError> {
        let persona = scenario
            .simulated_user_persona
            .as_deref()
            .unwrap_or("A neutral user pursuing the goal below.");
        let expected_outcome = scenario
            .expected_outcome
            .as_deref()
            .unwrap_or("the agent has produced a complete, correct answer to the initial query");
        let rendered = SIMULATOR_PROMPT_TEMPLATE
            .replace("{{persona}}", persona)
            .replace("{{initial_query}}", &scenario.initial_query)
            .replace("{{expected_outcome}}", expected_outcome)
            .replace("{{history}}", &render_history(history));

        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["message", "goal_achieved"],
            "properties": {
                "message": { "type": "string" },
                "goal_achieved": { "type": "boolean" }
            }
        });
        let output =
            ResponseFormat::json_schema("simulated_user_turn", schema).map_err(|error| {
                OrchestratorError::SimulatorFailed {
                    attempts: 0,
                    reason: format!("simulator response schema rejected: {error}"),
                }
            })?;
        let prompt = openai_chat(
            "gpt-simulator",
            OpenAiChatOptions {
                messages: vec![rendered.clone()],
                output: Some(output),
                ..OpenAiChatOptions::default()
            },
        )
        .map_err(|error| OrchestratorError::SimulatorFailed {
            attempts: 0,
            reason: format!("simulator prompt build failed: {error}"),
        })?;

        Ok(Self { rendered, prompt })
    }
}

fn render_history(history: &[ConversationTurn]) -> String {
    if history.is_empty() {
        return "(no prior turns)".to_owned();
    }
    let mut output = String::new();
    for turn in history {
        let role = match turn.role {
            TurnRole::User => "USER",
            TurnRole::Agent => "AGENT",
        };
        output.push_str(role);
        output.push_str(": ");
        output.push_str(&turn.content);
        output.push('\n');
    }
    output
}

/// Bounded-retry server simulator over a Skald agent.
pub struct ServerSimulatedUser {
    agent: Arc<Agent>,
    providers: Arc<ProviderRegistry>,
    /// Maximum attempts per simulated user turn.
    pub max_attempts: u32,
}

impl ServerSimulatedUser {
    /// Construct a simulator from a configured agent and provider registry.
    #[must_use]
    pub fn new(agent: Arc<Agent>, providers: Arc<ProviderRegistry>) -> Self {
        Self {
            agent,
            providers,
            max_attempts: 2,
        }
    }

    /// Produce the next simulated user turn.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] after all attempts fail.
    pub async fn next_turn(
        &self,
        scenario: &EvalScenario,
        history: &[ConversationTurn],
    ) -> Result<SimulatedUserTurn, OrchestratorError> {
        let rendered = SimulatorPrompt::render(scenario, history)?;
        let mut last_error = "no simulator attempt made".to_owned();

        for attempt in 1..=self.max_attempts {
            match self
                .agent
                .run_prompt(self.providers.as_ref(), &rendered.prompt, &[], None)
                .await
            {
                Ok(run) => {
                    if let Some(map) = run.structured_output {
                        let value = serde_json::Value::Object(map);
                        match serde_json::from_value(value) {
                            Ok(turn) => return Ok(turn),
                            Err(error) => {
                                last_error =
                                    format!("attempt {attempt}: parse SimulatedUserTurn: {error}");
                            }
                        }
                    } else {
                        last_error =
                            format!("attempt {attempt}: skald agent returned no structured output");
                    }
                }
                Err(error) => {
                    last_error = format!("attempt {attempt}: {error}");
                }
            }
        }

        Err(OrchestratorError::SimulatorFailed {
            attempts: self.max_attempts,
            reason: last_error,
        })
    }
}
