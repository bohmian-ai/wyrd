use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{
    Agent, AgentError, CallbackOutcome, ConversationTurn, FinishReason, Journal, JournalError,
    JournalEvent, Role, RunConfig, SessionError, SessionId, SessionMemory, SessionTurn,
};
use skald_prompt::Prompt;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};
use skald_tool::{AgentTool, ToolError};
use wyrd_spec::error::WyrdError;

#[tokio::test]
async fn session_recent_fires_once_per_run() {
    let session = Arc::new(RecordingSession::new());
    let journal = Arc::new(RecordingJournal::new());
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({}))]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"ok": true})))
        .with_session(session.clone())
        .with_journal(journal)
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect("run ok");

    assert_eq!(run.iterations, 2);
    assert_eq!(session.recent_call_count(), 1);
}

#[tokio::test]
async fn session_is_not_called_without_session_id() {
    let session = Arc::new(RecordingSession::new());
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session.clone())
        .with_journal(journal);

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(session.recent_call_count(), 0);
    assert_eq!(session.append_count(), 0);
}

#[tokio::test]
async fn session_recent_limit_defaults_to_50_and_honors_config() {
    let default_session = Arc::new(RecordingSession::new());
    let default_agent = Agent::from_resolved("default", test_prompt())
        .with_session(default_session.clone())
        .with_journal(Arc::new(RecordingJournal::new()));
    let default_providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));

    default_agent
        .run_with(&default_providers, Some(SessionId::new("default")), "hello")
        .await
        .expect("default run ok");

    let configured_session = Arc::new(RecordingSession::new());
    let configured_agent = Agent::from_resolved("configured", test_prompt())
        .with_session(configured_session.clone())
        .with_journal(Arc::new(RecordingJournal::new()))
        .with_run_config(RunConfig {
            session_recent_limit: Some(7),
            ..Default::default()
        });
    let configured_providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));

    configured_agent
        .run_with(
            &configured_providers,
            Some(SessionId::new("configured")),
            "hello",
        )
        .await
        .expect("configured run ok");

    assert_eq!(default_session.recent_limits(), vec![50]);
    assert_eq!(configured_session.recent_limits(), vec![7]);
}

#[tokio::test]
async fn recent_turns_seed_conversation_before_new_user_input() {
    let session = Arc::new(RecordingSession::new().seed_recent(vec![
        SessionTurn {
            role: Role::User,
            content: "first question".to_owned(),
            call_id: None,
        },
        SessionTurn {
            role: Role::Assistant,
            content: "first answer".to_owned(),
            call_id: None,
        },
    ]));
    let providers = registry(RecordingProvider::new(vec![openai_text_response(
        "second answer",
    )]));
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session)
        .with_journal(Arc::new(RecordingJournal::new()));

    let run = agent
        .run_with(&providers, Some(SessionId::new("s1")), "second question")
        .await
        .expect("run ok");

    let turns = run.conversation.turns();
    assert_eq!(turns.len(), 4);
    assert!(matches!(&turns[0], ConversationTurn::User { content } if content == "first question"));
    assert!(matches!(&turns[1], ConversationTurn::Assistant { .. }));
    assert!(
        matches!(&turns[2], ConversationTurn::User { content } if content == "second question")
    );
    assert!(matches!(&turns[3], ConversationTurn::Assistant { .. }));
}

#[tokio::test]
async fn user_turn_appends_after_recent_and_before_model_work() {
    let session = Arc::new(RecordingSession::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session.clone())
        .with_journal(Arc::new(RecordingJournal::new()));

    agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect("run ok");

    let operations = session.operations();
    assert!(matches!(operations.first(), Some(SessionOperation::Recent)));
    assert!(matches!(
        operations.get(1),
        Some(SessionOperation::Append(turn))
            if turn.role == Role::User && turn.content == "hello" && turn.call_id.is_none()
    ));
}

#[tokio::test]
async fn assistant_turn_appends_after_each_successful_provider_response() {
    let session = Arc::new(RecordingSession::new());
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({}))]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"ok": true})))
        .with_session(session.clone())
        .with_journal(Arc::new(RecordingJournal::new()))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect("run ok");

    let assistant_turns: Vec<_> = session
        .appends()
        .into_iter()
        .filter(|turn| turn.role == Role::Assistant)
        .collect();
    assert_eq!(assistant_turns.len(), 2);
    assert_eq!(assistant_turns[0].content, "");
    assert_eq!(assistant_turns[1].content, "done");
}

#[tokio::test]
async fn tool_turn_appends_only_after_successful_tool_invocation() {
    let session = Arc::new(RecordingSession::new());
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![
            tool_call("c1", "tool_ok", json!({"a": 1})),
            tool_call("c2", "tool_fail", json!({"b": 2})),
        ]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(fixed_tool("tool_ok", json!({"ok": true})))
        .add_tool(Arc::new(FailingTool))
        .with_session(session.clone())
        .with_journal(Arc::new(RecordingJournal::new()))
        .with_run_config(RunConfig {
            max_iterations: 3,
            tool_concurrency_cap: Some(2),
            ..Default::default()
        });

    agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect("run ok");

    let tool_turns: Vec<_> = session
        .appends()
        .into_iter()
        .filter(|turn| turn.role == Role::Tool)
        .collect();
    assert_eq!(tool_turns.len(), 1);
    assert_eq!(tool_turns[0].call_id.as_deref(), Some("c1"));
}

#[tokio::test]
async fn session_recent_failure_propagates_and_journals_agent_error() {
    let session = Arc::new(RecordingSession::new().fail_recent());
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("unused")]));
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session)
        .with_journal(journal.clone());

    let error = agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect_err("recent failure should propagate");

    assert!(matches!(error, AgentError::SessionRecentFailed { .. }));
    assert_eq!(error.code(), "SKALD_SESSION_500_RECENT");
    assert!(matches!(
        journal.events().last(),
        Some(JournalEvent::AgentError { code, .. }) if code == "SKALD_SESSION_500_RECENT"
    ));
}

#[tokio::test]
async fn session_append_failure_propagates_and_journals_agent_error() {
    let session = Arc::new(RecordingSession::new().fail_on_append(2));
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session)
        .with_journal(journal.clone());

    let error = agent
        .run_with(&providers, Some(SessionId::new("s1")), "hello")
        .await
        .expect_err("append failure should propagate");

    assert!(matches!(error, AgentError::SessionAppendFailed { .. }));
    assert_eq!(error.code(), "SKALD_SESSION_500_APPEND");
    assert!(matches!(
        journal.events().last(),
        Some(JournalEvent::AgentError { code, .. }) if code == "SKALD_SESSION_500_APPEND"
    ));
}

#[tokio::test]
async fn journal_records_exactly_one_start_and_finish_on_success() {
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt()).with_journal(journal.clone());

    agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    let events = journal.events();
    assert!(matches!(
        events.first(),
        Some(JournalEvent::AgentStart { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(JournalEvent::AgentFinish { .. })
    ));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, JournalEvent::AgentStart { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, JournalEvent::AgentFinish { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn provider_error_journals_synthetic_model_result_then_agent_error() {
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(Vec::new()));
    let agent = Agent::from_resolved("test", test_prompt()).with_journal(journal.clone());

    let error = agent
        .run_with(&providers, None, "hello")
        .await
        .expect_err("empty provider queue should fail");

    assert_eq!(error.code(), "SKALD_AGENT_502_PROVIDER");
    let events = journal.events();
    assert!(matches!(events[0], JournalEvent::AgentStart { .. }));
    assert!(matches!(events[1], JournalEvent::Iteration { index: 0 }));
    assert!(matches!(events[2], JournalEvent::ModelCall { .. }));
    assert!(matches!(
        events[3],
        JournalEvent::ModelResult {
            synthetic: true,
            ..
        }
    ));
    assert!(matches!(
        events[4],
        JournalEvent::AgentError { ref code, .. } if code == "SKALD_AGENT_502_PROVIDER"
    ));
}

#[tokio::test]
async fn before_model_abort_journals_synthetic_model_result_then_agent_finish() {
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("unused")]));
    let agent = Agent::from_resolved("test", test_prompt())
        .before_model(Arc::new(|_ctx, _request| {
            CallbackOutcome::Abort(WyrdError::AgentCallbackAborted {
                message: "test abort".to_owned(),
                details: serde_json::json!({}),
            })
        }))
        .with_journal(journal.clone());

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::CallbackAborted);
    let events = journal.events();
    assert!(matches!(events[0], JournalEvent::AgentStart { .. }));
    assert!(matches!(events[1], JournalEvent::Iteration { index: 0 }));
    assert!(matches!(events[2], JournalEvent::ModelCall { .. }));
    assert!(matches!(
        events[3],
        JournalEvent::ModelResult {
            synthetic: true,
            ref finish_reason,
            ..
        } if finish_reason == "callback_aborted"
    ));
    assert!(matches!(events[4], JournalEvent::AgentFinish { .. }));
}

#[tokio::test]
async fn tool_results_are_journaled_and_conversation_order_is_deterministic() {
    let journal = Arc::new(RecordingJournal::new());
    let calls = (0..5)
        .map(|index| tool_call(format!("c{index}"), "sleeper", json!({ "i": index })))
        .collect();
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(calls),
        openai_text_response("done"),
    ]));
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(Arc::new(SleeperTool { max: 5 }))
        .with_journal(journal.clone())
        .with_run_config(RunConfig {
            max_iterations: 3,
            tool_concurrency_cap: Some(5),
            ..Default::default()
        });

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    let tool_result_ids: Vec<_> = run
        .conversation
        .turns()
        .iter()
        .filter_map(|turn| match turn {
            ConversationTurn::ToolResult { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_result_ids, vec!["c0", "c1", "c2", "c3", "c4"]);

    let events = journal.events();
    let tool_call_count = events
        .iter()
        .filter(|event| matches!(event, JournalEvent::ToolCall { .. }))
        .count();
    let tool_result_count = events
        .iter()
        .filter(|event| matches!(event, JournalEvent::ToolResult { ok: true, .. }))
        .count();
    assert_eq!(tool_call_count, 5);
    assert_eq!(tool_result_count, 5);
}

#[tokio::test]
async fn same_session_id_round_trips_history_across_runs() {
    let session = Arc::new(RecordingSession::new());
    let agent = Agent::from_resolved("test", test_prompt())
        .with_session(session.clone())
        .with_journal(Arc::new(RecordingJournal::new()));
    let session_id = SessionId::new("hitl");

    let first_providers = registry(RecordingProvider::new(vec![openai_text_response(
        "what color?",
    )]));
    let first = agent
        .run_with(&first_providers, Some(session_id.clone()), "hello")
        .await
        .expect("first run ok");
    assert_eq!(first.finish_reason, FinishReason::ModelStopped);

    let second_providers = registry(RecordingProvider::new(vec![openai_text_response(
        "blue ok",
    )]));
    let second = agent
        .run_with(&second_providers, Some(session_id), "blue")
        .await
        .expect("second run ok");

    let turns = second.conversation.turns();
    assert_eq!(turns.len(), 4);
    assert!(matches!(&turns[0], ConversationTurn::User { content } if content == "hello"));
    assert!(matches!(&turns[1], ConversationTurn::Assistant { .. }));
    assert!(matches!(&turns[2], ConversationTurn::User { content } if content == "blue"));
    assert!(matches!(&turns[3], ConversationTurn::Assistant { .. }));
    assert_eq!(session.recent_call_count(), 2);
}

#[derive(Debug, Clone, PartialEq)]
enum SessionOperation {
    Recent,
    Append(SessionTurn),
}

#[derive(Default)]
struct SessionRecording {
    recent_calls: usize,
    recent_limits: Vec<usize>,
    appends: Vec<SessionTurn>,
    seed: Option<Vec<SessionTurn>>,
    fail_recent: bool,
    fail_on_append: Option<usize>,
    operations: Vec<SessionOperation>,
}

#[derive(Clone, Default)]
struct RecordingSession {
    inner: Arc<Mutex<SessionRecording>>,
}

impl RecordingSession {
    fn new() -> Self {
        Self::default()
    }

    fn seed_recent(self, turns: Vec<SessionTurn>) -> Self {
        self.with_inner(|inner| inner.seed = Some(turns));
        self
    }

    fn fail_recent(self) -> Self {
        self.with_inner(|inner| inner.fail_recent = true);
        self
    }

    fn fail_on_append(self, nth: usize) -> Self {
        self.with_inner(|inner| inner.fail_on_append = Some(nth));
        self
    }

    fn recent_call_count(&self) -> usize {
        self.inner().recent_calls
    }

    fn recent_limits(&self) -> Vec<usize> {
        self.inner().recent_limits.clone()
    }

    fn append_count(&self) -> usize {
        self.inner().appends.len()
    }

    fn appends(&self) -> Vec<SessionTurn> {
        self.inner().appends.clone()
    }

    fn operations(&self) -> Vec<SessionOperation> {
        self.inner().operations.clone()
    }

    fn with_inner(&self, update: impl FnOnce(&mut SessionRecording)) {
        match self.inner.lock() {
            Ok(mut guard) => update(&mut guard),
            Err(poisoned) => update(&mut poisoned.into_inner()),
        }
    }

    fn inner(&self) -> MutexGuard<'_, SessionRecording> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl SessionMemory for RecordingSession {
    async fn recent(
        &self,
        _session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionTurn>, SessionError> {
        let mut inner = self.inner();
        inner.recent_calls += 1;
        inner.recent_limits.push(limit);
        inner.operations.push(SessionOperation::Recent);
        if inner.fail_recent {
            return Err(SessionError::RecentFailed("test recent failure".to_owned()));
        }
        Ok(inner.seed.clone().unwrap_or_else(|| inner.appends.clone()))
    }

    async fn append(&self, _session_id: &SessionId, turn: SessionTurn) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let next = inner.appends.len() + 1;
        if Some(next) == inner.fail_on_append {
            return Err(SessionError::AppendFailed(format!(
                "test append failure #{next}"
            )));
        }
        inner
            .operations
            .push(SessionOperation::Append(turn.clone()));
        inner.appends.push(turn);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct RecordingJournal {
    events: Arc<Mutex<Vec<JournalEvent>>>,
}

impl RecordingJournal {
    fn new() -> Self {
        Self::default()
    }

    fn events(&self) -> Vec<JournalEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[async_trait]
impl Journal for RecordingJournal {
    async fn append(&self, event: JournalEvent) -> Result<(), JournalError> {
        match self.events.lock() {
            Ok(mut guard) => guard.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
        Ok(())
    }
}

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<ProviderResponse>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }

    fn responses(&self) -> MutexGuard<'_, VecDeque<ProviderResponse>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn send(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses()
            .pop_front()
            .ok_or_else(|| ProviderError::bad_request("recording", "response queue is empty"))
    }

    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "recording",
            "recording provider does not stream",
        ))
    }

    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

#[derive(Debug)]
struct FixedTool {
    name: String,
    output: Value,
}

#[async_trait]
impl AgentTool for FixedTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "fixed test tool"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        })
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Ok(self.output.clone())
    }
}

#[derive(Debug)]
struct FailingTool;

#[async_trait]
impl AgentTool for FailingTool {
    fn name(&self) -> &str {
        "tool_fail"
    }

    fn description(&self) -> &str {
        "failing test tool"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::Invocation {
            detail: "test failure".to_owned(),
            cause: None,
        })
    }
}

#[derive(Debug)]
struct SleeperTool {
    max: u64,
}

#[async_trait]
impl AgentTool for SleeperTool {
    fn name(&self) -> &str {
        "sleeper"
    }

    fn description(&self) -> &str {
        "sleeps in inverse index order"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "i": { "type": "integer" } },
            "required": ["i"],
            "additionalProperties": false
        })
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let index = args.get("i").and_then(Value::as_u64).unwrap_or(0);
        let delay = self.max.saturating_sub(index);
        tokio::time::sleep(Duration::from_millis(delay * 10)).await;
        Ok(json!({ "i": index }))
    }
}

fn registry(provider: RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider));
    providers
}

fn fixed_tool(name: &str, output: Value) -> Arc<dyn AgentTool> {
    Arc::new(FixedTool {
        name: name.to_owned(),
        output,
    })
}

fn test_prompt() -> Arc<Prompt> {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text("system".to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    Arc::new(Prompt::from_native(
        SpecPrompt::new(request, "gpt-4o", None, ResponseType::Text).expect("test prompt builds"),
    ))
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_text".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

fn openai_tool_call_response(calls: Vec<OpenAiToolCall>) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_call".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: None,
                name: None,
                tool_calls: Some(calls),
                tool_call_id: None,
                refusal: None,
            },
            finish_reason: Some("tool_calls".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

fn tool_call(id: impl Into<String>, name: impl Into<String>, args: Value) -> OpenAiToolCall {
    OpenAiToolCall {
        id: id.into(),
        kind: "function".to_owned(),
        function: OpenAiToolFunctionCall {
            name: name.into(),
            arguments: args.to_string(),
        },
    }
}
