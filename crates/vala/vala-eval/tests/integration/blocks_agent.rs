//! Agent actions and checks on the shared Skald Agent loop.

use crate::orchestrator_support;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchestrator_support::{openai_text_response, structured_prompt, tid};
use serde_json::{Value, json};
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{MediaRef, ProviderName, ProviderResponse};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use tokio::time::Instant;
use vala_eval::blocks::{
    ActionExecutor, AgentCardResolver, AgentExecutor, AgentToolset, BlockCall, BlockError,
    CallInput, CallValue, CheckExecutor, EvidenceReader, Position, PromptCardResolver, ToolSession,
};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec, BuiltinTool, ExecutionRoute};
use wyrd_spec::card::blocks::{Action, AgentCall, Check};
use wyrd_spec::ids::SlotName;
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};

/// Refuses every registry lookup; the tests author Agents inline.
struct NoRegistry;

#[async_trait]
impl AgentCardResolver for NoRegistry {
    /// Always refuses.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn resolve(&self, _agent_ref: &CardRef) -> Result<AgentSpec, BlockError> {
        Err(BlockError::Terminal {
            reason: "no registry".to_owned(),
        })
    }
}

#[async_trait]
impl PromptCardResolver for NoRegistry {
    /// Always refuses.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn resolve(&self, _prompt_ref: &CardRef) -> Result<Prompt, BlockError> {
        Err(BlockError::Terminal {
            reason: "no registry".to_owned(),
        })
    }
}

/// A `fs.read` double answering every call with fixed text.
struct FixedRead;

#[async_trait]
impl AgentTool for FixedRead {
    /// The built-in wire name.
    fn name(&self) -> &str {
        BuiltinTool::FsRead.as_str()
    }

    /// A fixed description.
    fn description(&self) -> &'static str {
        "read a workspace file"
    }

    /// Any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Any object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Return fixed file text.
    ///
    /// # Errors
    /// Never fails.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Ok(json!({"text": "revenue is 42"}))
    }
}

/// A toolset binding [`FixedRead`] for any call.
struct FixedToolset;

#[async_trait]
impl AgentToolset for FixedToolset {
    /// Open a session holding only [`FixedRead`].
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses its one tool.
    async fn open(
        &self,
        _call: &BlockCall<'_, Action>,
        _tools: &[BuiltinTool],
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(FixedRead))
            .expect("one tool registers");
        Ok(ToolSession::new(registry, 1024, Box::new(())))
    }
}

/// An OpenAI text Prompt over `{{document}}`.
///
/// # Panics
/// Panics when the static Prompt is invalid.
fn text_prompt() -> Prompt {
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Summarize: {{document}}".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

/// An OpenAI Prompt over `{{response}}` answering with any JSON object.
///
/// # Panics
/// Panics when the static Prompt or schema is invalid.
fn verdict_prompt() -> Prompt {
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Judge: {{response}}".to_owned()],
            output: Some(
                ResponseFormat::json_schema("verdict", json!({"type": "object"})).expect("schema"),
            ),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

/// An inline native-route Agent over `prompt` declaring `tools`.
fn agent(prompt: Prompt, tools: &[&str]) -> AgentCall {
    AgentCall {
        agent: InlineableRef::Inline(Box::new(AgentSpec {
            prompt: InlineableRef::Inline(Box::new(prompt.into_native())),
            execution_route: ExecutionRoute::Native {},
            tool_names: tools.iter().map(|&name| name.to_owned()).collect(),
            run_config: AgentRunConfigSpec::default(),
            verified_by: Vec::new(),
        })),
        inputs: BTreeMap::new(),
        workspace: BTreeMap::new(),
    }
}

/// A provider registry answering with `responses` in order.
fn providers(responses: Vec<ProviderResponse>) -> Arc<ProviderRegistry> {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for response in responses {
        mock.push_response(response);
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(providers)
}

/// An executor with no registry over `responses`.
fn executor(responses: Vec<ProviderResponse>) -> AgentExecutor {
    AgentExecutor::new(
        providers(responses),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
}

/// One JSON input named `name`.
fn input(name: &str, value: Value) -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        name.to_owned(),
        CallInput {
            value: CallValue::Json(value),
            sources: BTreeSet::new(),
        },
    )])
}

/// An OpenAI answer calling `fs_read` once.
fn read_call() -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_tool".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_1".to_owned(),
                    kind: "function".to_owned(),
                    function: OpenAiToolFunctionCall {
                        name: "fs_read".to_owned(),
                        arguments: json!({"path": "/inputs/report.txt"}).to_string(),
                    },
                }]),
                ..Default::default()
            },
            finish_reason: Some("tool_calls".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Run `call` as a step action with `inputs`.
///
/// # Errors
/// Returns the executor's [`BlockError`] for the step.
async fn step(
    executor: &AgentExecutor,
    call: &AgentCall,
    inputs: BTreeMap<String, CallInput>,
) -> Result<vala_eval::blocks::ActionOutput, BlockError> {
    let id = tid("summarize");
    let action = Action::Agent(call.clone());
    ActionExecutor::execute(
        executor,
        BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs,
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
        },
    )
    .await
}

/// Run `call` as a Verifier requirement check with `inputs`.
///
/// # Errors
/// Returns the executor's [`BlockError`] for the check.
async fn check(
    executor: &AgentExecutor,
    call: &AgentCall,
    inputs: BTreeMap<String, CallInput>,
) -> Result<vala_eval::blocks::CheckOutput, BlockError> {
    let id = tid("grounded");
    let block = Check::Agent(call.clone());
    CheckExecutor::execute(
        executor,
        BlockCall {
            id: &id,
            position: Position::VerifierCheck,
            block: &block,
            inputs,
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
        },
    )
    .await
}

/// A step Agent with a structured Prompt returns its schema-valid answer and
/// names its effective route.
///
/// # Panics
/// Panics when the output or provenance differs.
#[tokio::test(flavor = "multi_thread")]
async fn step_agent_returns_structured_output() {
    let executor = executor(vec![openai_text_response(r#"{"passed": true}"#)]);
    let output = step(
        &executor,
        &agent(structured_prompt(), &[]),
        input("response", json!("ok")),
    )
    .await
    .expect("the step runs");
    assert_eq!(output.output, json!({"passed": true}));
    assert_eq!(
        output.provenance,
        Some(CallProvenance::Agent {
            agent: None,
            prompt: None,
            route: ExecutionRoute::Native {},
        })
    );
}

/// A structured answer violating the Prompt's response schema is terminal.
///
/// # Panics
/// Panics when the answer is accepted or retryable.
#[tokio::test(flavor = "multi_thread")]
async fn step_agent_rejects_schema_violation() {
    let executor = executor(vec![openai_text_response(r#"{"passed": "yes"}"#)]);
    let error = step(
        &executor,
        &agent(structured_prompt(), &[]),
        input("response", json!("ok")),
    )
    .await
    .expect_err("the answer violates the schema");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}

/// Inputs must equal the Prompt placeholders.
///
/// # Panics
/// Panics when a mismatched input reaches the provider.
#[tokio::test(flavor = "multi_thread")]
async fn agent_inputs_must_match_placeholders() {
    let executor = executor(Vec::new());
    let error = step(
        &executor,
        &agent(text_prompt(), &[]),
        input("other", json!("x")),
    )
    .await
    .expect_err("the input names no placeholder");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}

/// A step Agent calls its declared built-in, and the call is recorded with
/// the returned bytes' digest.
///
/// # Panics
/// Panics when the tool call or final answer is missing.
#[tokio::test(flavor = "multi_thread")]
async fn step_agent_records_declared_tool_calls() {
    let executor = executor(vec![read_call(), openai_text_response("revenue is 42")])
        .with_toolset(Arc::new(FixedToolset));
    let output = step(
        &executor,
        &agent(text_prompt(), &["fs.read"]),
        input("document", json!("report")),
    )
    .await
    .expect("the step runs");
    assert_eq!(output.output, json!("revenue is 42"));
    let returned = json!({"text": "revenue is 42"}).to_string();
    assert_eq!(output.tool_calls.len(), 1);
    let record = &output.tool_calls[0];
    assert_eq!(record.tool, BuiltinTool::FsRead);
    assert_eq!(record.arguments, json!({"path": "/inputs/report.txt"}));
    assert_eq!(
        record.output_sha256,
        Some(Sha256Hex::digest(returned.as_bytes()))
    );
    assert_eq!(record.returned_bytes, returned.len() as u64);
}

/// An undeclared built-in name is refused before any model call.
///
/// # Panics
/// Panics when the unknown tool is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn step_agent_refuses_unknown_tool() {
    let executor = executor(Vec::new()).with_toolset(Arc::new(FixedToolset));
    let error = step(
        &executor,
        &agent(text_prompt(), &["shell.exec"]),
        input("document", json!("x")),
    )
    .await
    .expect_err("the tool is not a built-in");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}

/// A pinned five-page `deck` slot: page citations into it are valid when
/// the slot is reachable.
struct PinnedDeck;

#[async_trait]
impl EvidenceReader for PinnedDeck {
    /// No native media is bound in these tests.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn media(&self, _evidence: &InputDeclaration) -> Result<MediaRef, BlockError> {
        Err(BlockError::Terminal {
            reason: "no media".to_owned(),
        })
    }

    /// Keep reachable `deck` page citations within its five pages.
    ///
    /// # Errors
    /// Never fails.
    async fn cited(
        &self,
        reachable: &BTreeSet<SlotName>,
        citations: Vec<Citation>,
    ) -> Result<Vec<Citation>, BlockError> {
        Ok(citations
            .into_iter()
            .filter(|citation| {
                citation.slot.as_str() == "deck"
                    && reachable.contains(&citation.slot)
                    && matches!(citation.locator, Locator::Page(1..=5))
            })
            .collect())
    }
}

/// One JSON input derived from the `deck` slot.
///
/// # Panics
/// Never; the slot name is valid.
fn from_deck(value: Value) -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        "response".to_owned(),
        CallInput {
            value: CallValue::Json(value),
            sources: BTreeSet::from([SlotName::new("deck").expect("slot")]),
        },
    )])
}

/// A step's cited findings bound into a tool-less check keep their valid
/// citations; fabricated and unreachable ones are dropped, and a pass left
/// with none, or with no evidence reader, is inconclusive.
///
/// # Panics
/// Panics when an invalid citation survives or an uncited pass stands.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_keeps_only_valid_citations() {
    let findings = json!({
        "finding": "refunds take 30 days",
        "citations": [{"slot": "deck", "locator": {"page": 3}}]
    });
    let answer = |citations: Value| {
        openai_text_response(
            &json!({"outcome": "passed", "reason": "grounded", "citations": citations}).to_string(),
        )
    };
    let pinned = executor(vec![
        openai_text_response(&findings.to_string()),
        answer(json!([
            {"slot": "deck", "locator": {"page": 3}},
            {"slot": "deck", "locator": {"page": 9}},
            {"slot": "policy", "locator": {"page": 1}}
        ])),
        answer(json!([{"slot": "deck", "locator": {"page": 9}}])),
    ])
    .with_evidence(Arc::new(PinnedDeck));
    let judge = agent(verdict_prompt(), &[]);

    let step = step(&pinned, &judge, from_deck(json!("deck text")))
        .await
        .expect("the step runs");
    let output = check(&pinned, &judge, from_deck(step.output.clone()))
        .await
        .expect("the check runs");
    assert_eq!(output.verdict.outcome, VerificationVerdict::Passed);
    assert_eq!(
        serde_json::to_value(&output.verdict.citations).expect("citations"),
        step.output["citations"]
    );

    let fabricated = check(&pinned, &judge, from_deck(step.output))
        .await
        .expect("the check runs");
    assert_eq!(
        fabricated.verdict.outcome,
        VerificationVerdict::Inconclusive
    );
    assert!(fabricated.verdict.citations.is_empty());

    let unread = executor(vec![answer(
        json!([{"slot": "deck", "locator": {"page": 3}}]),
    )]);
    let output = check(&unread, &judge, from_deck(json!("x")))
        .await
        .expect("the check runs");
    assert_eq!(output.verdict.outcome, VerificationVerdict::Inconclusive);
}

/// A check Agent answer outside the Verdict schema is terminal.
///
/// # Panics
/// Panics when the answer is accepted or retryable.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_rejects_non_verdict() {
    let executor = executor(vec![openai_text_response(r#"{"outcome": "passed"}"#)]);
    let error = check(
        &executor,
        &agent(verdict_prompt(), &[]),
        input("response", json!("ok")),
    )
    .await
    .expect_err("the answer is not a Verdict");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}

/// A check Agent declaring tools is refused before any model call.
///
/// # Panics
/// Panics when the check runs.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_refuses_tools() {
    let executor = executor(Vec::new()).with_toolset(Arc::new(FixedToolset));
    let error = check(
        &executor,
        &agent(verdict_prompt(), &["fs.read"]),
        input("response", json!("ok")),
    )
    .await
    .expect_err("check Agents have no tools");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}

/// A per-call route override replaces the declared route: a gateway override
/// on a host without a gateway is refused rather than run natively.
///
/// # Panics
/// Panics when the declared native route runs.
#[tokio::test(flavor = "multi_thread")]
async fn route_override_replaces_declared_route() {
    let executor =
        executor(vec![openai_text_response("ignored")]).with_route_overrides(BTreeMap::from([(
            tid("summarize"),
            ExecutionRoute::WyrdGateway {
                deployment: "prod".parse().expect("static deployment"),
            },
        )]));
    let error = step(
        &executor,
        &agent(text_prompt(), &[]),
        input("document", json!("x")),
    )
    .await
    .expect_err("the override needs a gateway");
    assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
}
