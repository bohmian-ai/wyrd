//! OpenAI Responses requests drive the Agent tool loop in their native shape.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{
    Agent, ConversationTurn, FinishReason, Role, SessionError, SessionId, SessionMemory,
    SessionTurn,
};
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesResponse,
    OpenAiResponsesSettings, OpenAiResponsesTool,
};
use skald_spec::{
    MessageNum, Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};
use skald_tool::{AgentTool, ToolError};

/// Tool returning a fixed answer so the second request is fully predictable.
struct LookupTool;

#[async_trait]
impl AgentTool for LookupTool {
    /// Tool name the scripted function call targets.
    fn name(&self) -> &str {
        "lookup"
    }

    /// Fixed description emitted into the Responses tool declaration.
    fn description(&self) -> &str {
        "returns a fixed answer"
    }

    /// Accepts any object.
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    /// Any output.
    fn output_schema(&self) -> Value {
        json!({})
    }

    /// Returns the fixed answer.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Ok(json!({ "answer": 42 }))
    }
}

/// Responses request with `input` items and, when set, the lookup tool.
fn responses_request(input: Vec<OpenAiResponseItem>, tools: bool) -> ProviderRequest {
    ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
        model: "gpt-4o".to_owned(),
        input: input.into(),
        instructions: Some("be helpful".to_owned()),
        text: None,
        tools: tools.then(|| {
            vec![OpenAiResponsesTool::Function {
                name: "lookup".to_owned(),
                description: Some("returns a fixed answer".to_owned()),
                parameters: json!({ "type": "object" }).as_object().cloned(),
                strict: None,
            }]
        }),
        tool_choice: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        stream: None,
        settings: OpenAiResponsesSettings::default(),
    })
}

/// Responses answer carrying `output` items.
fn responses_answer(output: Vec<OpenAiResponseItem>) -> ProviderResponse {
    ProviderResponse::OpenAiResponses(OpenAiResponsesResponse {
        id: "resp".to_owned(),
        object: "response".to_owned(),
        model: "gpt-4o".to_owned(),
        status: "completed".to_owned(),
        created_at: 0,
        output,
        usage: None,
        previous_response_id: None,
    })
}

/// Message item with one text part.
fn message(role: &str, part: OpenAiResponseContentPart) -> OpenAiResponseItem {
    OpenAiResponseItem::Message {
        role: role.to_owned(),
        content: vec![part],
    }
}

/// The scripted function call.
fn lookup_call() -> OpenAiResponseItem {
    OpenAiResponseItem::FunctionCall {
        call_id: "c1".to_owned(),
        name: "lookup".to_owned(),
        arguments: r#"{"q":"x"}"#.to_owned(),
    }
}

/// A Responses Agent calls a tool and the next request replays the user
/// input, the function call, and its output as native input items; reasoning
/// items are not replayed.
#[tokio::test]
async fn agent_run_executes_openai_responses_tool_loop() {
    let user = message(
        "user",
        OpenAiResponseContentPart::InputText {
            text: "hello".to_owned(),
        },
    );
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(responses_request(vec![user.clone()], true))
        .respond_with(responses_answer(vec![
            OpenAiResponseItem::Reasoning {
                summary: None,
                encrypted_content: None,
            },
            lookup_call(),
        ]))
        .expect_request(responses_request(
            vec![
                user,
                lookup_call(),
                OpenAiResponseItem::FunctionCallOutput {
                    call_id: "c1".to_owned(),
                    output: r#"{"answer":42}"#.to_owned(),
                },
            ],
            true,
        ))
        .respond_with(responses_answer(vec![message(
            "assistant",
            OpenAiResponseContentPart::OutputText {
                text: "done".to_owned(),
            },
        )]));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock.clone()));
    let prompt = Prompt::from_native(
        SpecPrompt::new(
            responses_request(Vec::new(), false),
            "gpt-4o",
            None,
            ResponseType::Text,
        )
        .expect("Responses prompt builds"),
    );
    let agent = Agent::new(prompt)
        .with_id("r")
        .add_tool(Arc::new(LookupTool));

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("Responses loop runs");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 2);
    assert_eq!(run.output, "done");
    assert_eq!(mock.remaining(), 0);
    assert!(run.conversation.turns().iter().all(|turn| !matches!(
        turn,
        ConversationTurn::Assistant { message } if !matches!(message, MessageNum::OpenAiResponses(_))
    )));
}

/// Session memory replaying one prior assistant turn.
struct PriorTurnSession;

#[async_trait]
impl SessionMemory for PriorTurnSession {
    /// The single prior assistant turn.
    async fn recent(
        &self,
        _session_id: &SessionId,
        _limit: usize,
    ) -> Result<Vec<SessionTurn>, SessionError> {
        Ok(vec![SessionTurn {
            role: Role::Assistant,
            content: "earlier".to_owned(),
            call_id: None,
        }])
    }

    /// Drops appended turns.
    async fn append(
        &self,
        _session_id: &SessionId,
        _turn: SessionTurn,
    ) -> Result<(), SessionError> {
        Ok(())
    }
}

/// A Responses Agent seeds prior session turns as native Responses output
/// messages ahead of the new user input.
#[tokio::test]
async fn responses_session_turns_seed_native_items() {
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(responses_request(
            vec![
                message(
                    "assistant",
                    OpenAiResponseContentPart::OutputText {
                        text: "earlier".to_owned(),
                    },
                ),
                message(
                    "user",
                    OpenAiResponseContentPart::InputText {
                        text: "hello".to_owned(),
                    },
                ),
            ],
            false,
        ))
        .respond_with(responses_answer(vec![message(
            "assistant",
            OpenAiResponseContentPart::OutputText {
                text: "again".to_owned(),
            },
        )]));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock.clone()));
    let prompt = Prompt::from_native(
        SpecPrompt::new(
            responses_request(Vec::new(), false),
            "gpt-4o",
            None,
            ResponseType::Text,
        )
        .expect("Responses prompt builds"),
    );
    let agent = Agent::new(prompt)
        .with_id("r")
        .with_session(Arc::new(PriorTurnSession));

    let run = agent
        .run_with(&providers, Some(SessionId::new("s")), "hello")
        .await
        .expect("Responses session run");

    assert_eq!(run.output, "again");
    assert_eq!(mock.remaining(), 0);
}
