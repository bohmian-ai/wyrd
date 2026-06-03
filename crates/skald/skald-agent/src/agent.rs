//! Live agent: identity and prompt-bound bounded loop configuration.
//!
//! Build an [`Agent`] directly once the resolved prompt is ready, then run the
//! bounded tool loop.

use std::fmt;
use std::sync::Arc;

use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;
use skald_tool::AgentTool;

use crate::callbacks::{
    AfterAgentFn, AfterModelFn, AfterToolFn, BeforeAgentFn, BeforeModelFn, BeforeToolFn,
};
use crate::error::AgentResult;
use crate::journal::{Journal, NoopJournal};
use crate::run::RunConfig;
use crate::session::{NoSession, SessionId, SessionMemory};

/// Live agent ready to run the bounded tool loop.
///
/// ```compile_fail
/// use skald_agent::Agent;
///
/// fn prompt_is_resolved_native(agent: &Agent) {
///     let _: skald_prompt::Prompt = agent.prompt.clone();
/// }
/// ```
#[derive(Clone)]
pub struct Agent {
    /// Stable id used for tracing and diagnostics.
    pub id: String,
    /// Resolved native prompt.
    ///
    /// Provider and model identities live in `prompt.request` and `prompt.model`.
    pub prompt: Arc<Prompt>,
    /// Loop execution settings.
    pub run_config: RunConfig,
    /// Per-agent runtime-local tool cache.
    pub(crate) tools: Vec<Arc<dyn AgentTool>>,
    /// Callbacks fired before the agent run begins.
    pub(crate) before_agent: Vec<BeforeAgentFn>,
    /// Callbacks fired after the agent run completes.
    pub(crate) after_agent: Vec<AfterAgentFn>,
    /// Callbacks fired before provider model calls.
    pub(crate) before_model: Vec<BeforeModelFn>,
    /// Callbacks fired after provider model calls.
    pub(crate) after_model: Vec<AfterModelFn>,
    /// Callbacks fired before tool invocations.
    pub(crate) before_tool: Vec<BeforeToolFn>,
    /// Callbacks fired after tool invocations.
    pub(crate) after_tool: Vec<AfterToolFn>,
    /// Session memory backend for this agent.
    pub(crate) session: Arc<dyn SessionMemory>,
    /// Run journal backend for this agent.
    pub(crate) journal: Arc<dyn Journal>,
}

impl fmt::Debug for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Agent")
            .field("id", &self.id)
            .field("prompt_model", &self.prompt.native().model)
            .field("run_config", &self.run_config)
            .field("tool_names", &self.tool_names())
            .finish()
    }
}

impl Agent {
    /// Builds a new runnable agent.
    pub fn new(id: impl Into<String>, prompt: Arc<Prompt>) -> Self {
        Self {
            id: id.into(),
            prompt,
            run_config: RunConfig::default(),
            tools: Vec::new(),
            before_agent: Vec::new(),
            after_agent: Vec::new(),
            before_model: Vec::new(),
            after_model: Vec::new(),
            before_tool: Vec::new(),
            after_tool: Vec::new(),
            session: Arc::new(NoSession),
            journal: Arc::new(NoopJournal),
        }
    }

    /// Rebuilds the agent with a new prompt.
    pub fn with_prompt(mut self, prompt: Arc<Prompt>) -> Self {
        self.prompt = prompt;
        self
    }

    /// Rebuilds the agent with a new run configuration.
    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Appends one runtime-local tool to the agent cache.
    pub fn add_tool(mut self, tool: Arc<dyn AgentTool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Replaces the full runtime-local tool cache.
    pub fn set_tools(mut self, tools: Vec<Arc<dyn AgentTool>>) -> Self {
        self.tools = tools;
        self
    }

    /// Returns tool names in their current cache order.
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        self.tools
            .iter()
            .map(|tool| tool.name().to_owned())
            .collect()
    }

    /// Registers a callback fired before the agent run begins.
    pub fn before_agent(mut self, callback: BeforeAgentFn) -> Self {
        self.before_agent.push(callback);
        self
    }

    /// Registers a callback fired after the agent run completes.
    pub fn after_agent(mut self, callback: AfterAgentFn) -> Self {
        self.after_agent.push(callback);
        self
    }

    /// Registers a callback fired before one provider request is sent.
    pub fn before_model(mut self, callback: BeforeModelFn) -> Self {
        self.before_model.push(callback);
        self
    }

    /// Registers a callback fired after one provider response is received.
    pub fn after_model(mut self, callback: AfterModelFn) -> Self {
        self.after_model.push(callback);
        self
    }

    /// Registers a callback fired before one tool invocation.
    pub fn before_tool(mut self, callback: BeforeToolFn) -> Self {
        self.before_tool.push(callback);
        self
    }

    /// Registers a callback fired after one tool invocation.
    pub fn after_tool(mut self, callback: AfterToolFn) -> Self {
        self.after_tool.push(callback);
        self
    }

    /// Replaces the session memory backend.
    pub fn with_session(mut self, session: Arc<dyn SessionMemory>) -> Self {
        self.session = session;
        self
    }

    /// Replaces the run journal backend.
    pub fn with_journal(mut self, journal: Arc<dyn Journal>) -> Self {
        self.journal = journal;
        self
    }

    /// Run the bounded tool loop against a live provider registry.
    pub async fn run(
        &self,
        providers: &ProviderRegistry,
        session_id: Option<SessionId>,
        input: &str,
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run(self, providers, session_id, input).await
    }

    /// Run the bounded tool loop driven by a rendered prompt with variable
    /// substitution.
    pub async fn run_prompt(
        &self,
        providers: &ProviderRegistry,
        prompt: &Prompt,
        vars: &[(&str, &str)],
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run_prompt(self, providers, prompt, vars).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::json;
    use skald_prompt::Prompt;
    use skald_spec::{
        Prompt as SpecPrompt, ProviderRequest, ResponseType,
        wire::openai_chat::{
            OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent,
        },
    };

    use super::Agent;
    use crate::callbacks::CallbackOutcome;
    use crate::journal::{Journal, JournalError, JournalEvent};
    use crate::session::{Role, SessionError, SessionId, SessionMemory, SessionTurn};

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
            SpecPrompt::new(request, "gpt-4o", None, ResponseType::Text)
                .expect("test prompt should build"),
        ))
    }

    #[test]
    fn agent_new_has_default_field_lengths() {
        let agent = Agent::new("a", test_prompt());

        assert!(agent.tools.is_empty());
        assert_eq!(agent.before_agent.len(), 0);
        assert_eq!(agent.after_agent.len(), 0);
        assert_eq!(agent.before_model.len(), 0);
        assert_eq!(agent.after_model.len(), 0);
        assert_eq!(agent.before_tool.len(), 0);
        assert_eq!(agent.after_tool.len(), 0);
    }

    #[test]
    fn agent_all_six_callbacks_append_independently() {
        let agent = Agent::new("a", test_prompt())
            .before_agent(Arc::new(|_, input| {
                CallbackOutcome::ReplaceWith(input.to_owned())
            }))
            .after_agent(Arc::new(|_, run| CallbackOutcome::ReplaceWith(run.clone())))
            .before_model(Arc::new(|_, request| {
                CallbackOutcome::ReplaceWith(request.clone())
            }))
            .after_model(Arc::new(|_, response| {
                CallbackOutcome::ReplaceWith(response.clone())
            }))
            .before_tool(Arc::new(|_, _, args| {
                CallbackOutcome::ReplaceWith(args.clone())
            }))
            .after_tool(Arc::new(|_, _, _| CallbackOutcome::Continue));

        assert_eq!(agent.before_agent.len(), 1);
        assert_eq!(agent.after_agent.len(), 1);
        assert_eq!(agent.before_model.len(), 1);
        assert_eq!(agent.after_model.len(), 1);
        assert_eq!(agent.before_tool.len(), 1);
        assert_eq!(agent.after_tool.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_default_session_and_journal_drop_successfully() {
        let agent = Agent::new("a", test_prompt());
        let recent = agent
            .session
            .recent(&SessionId::new("s"), 10)
            .await
            .expect("default session recent succeeds");

        assert!(recent.is_empty());
        agent
            .session
            .append(
                &SessionId::new("s"),
                SessionTurn {
                    role: Role::User,
                    content: "hi".to_owned(),
                    call_id: None,
                },
            )
            .await
            .expect("default session append succeeds");
        agent
            .journal
            .append(JournalEvent::Iteration { index: 0 })
            .await
            .expect("default journal append succeeds");
    }

    #[derive(Default)]
    struct RecordingSession {
        turns: Mutex<Vec<SessionTurn>>,
    }

    #[async_trait]
    impl SessionMemory for RecordingSession {
        async fn recent(
            &self,
            _session_id: &SessionId,
            limit: usize,
        ) -> Result<Vec<SessionTurn>, SessionError> {
            let turns = self.turns.lock().expect("recording session lock");
            Ok(turns.iter().rev().take(limit).cloned().collect())
        }

        async fn append(
            &self,
            _session_id: &SessionId,
            turn: SessionTurn,
        ) -> Result<(), SessionError> {
            self.turns
                .lock()
                .expect("recording session lock")
                .push(turn);
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_with_session_swaps_default() {
        let recording = Arc::new(RecordingSession::default());
        let agent = Agent::new("a", test_prompt()).with_session(recording.clone());

        agent
            .session
            .append(
                &SessionId::new("s"),
                SessionTurn {
                    role: Role::User,
                    content: "hi".to_owned(),
                    call_id: None,
                },
            )
            .await
            .expect("recording session append succeeds");

        assert_eq!(
            recording.turns.lock().expect("recorded turns lock").len(),
            1
        );
    }

    #[derive(Default)]
    struct RecordingJournal {
        events: Mutex<Vec<JournalEvent>>,
    }

    #[async_trait]
    impl Journal for RecordingJournal {
        async fn append(&self, event: JournalEvent) -> Result<(), JournalError> {
            self.events
                .lock()
                .expect("recording journal lock")
                .push(event);
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_with_journal_swaps_default() {
        let recording = Arc::new(RecordingJournal::default());
        let agent = Agent::new("a", test_prompt()).with_journal(recording.clone());

        agent
            .journal
            .append(JournalEvent::ToolResult {
                iteration: 0,
                call_id: "call_1".to_owned(),
                ok: true,
                output: json!({ "ok": true }),
            })
            .await
            .expect("recording journal append succeeds");

        assert_eq!(
            recording.events.lock().expect("recorded events lock").len(),
            1
        );
    }
}
