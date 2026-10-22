//! Agent actions and checks on the shared Skald Agent loop.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall,
};
use skald_spec::{MediaRef, ProviderName, ProviderResponse};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use tokio::time::Instant;
use vala_verify::blocks::{
    ActionExecutor, AgentCardResolver, AgentExecutor, AgentToolset, BlockCall, BlockError,
    CallInput, CallValue, CheckExecutor, EvidenceReader, Position, PromptCardResolver, ToolSession,
};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec, BuiltinTool, ExecutionRoute};
use wyrd_spec::card::verifier::{Action, AgentCall, Check};
use wyrd_spec::ids::SlotName;
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};

/// Build the [`TaskId`] used for a requirement or step in these fixtures.
///
/// # Panics
/// Panics when `value` fails [`TaskId`] validation, which means the static
/// test literal itself is wrong.
fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

/// A structured judge prompt returning `{"passed": bool}`.
///
/// Builds an OpenAI chat Prompt whose response format is a strict JSON
/// schema requiring one boolean `passed` field, so Agent checks receive a
/// structured verdict from the mocked provider.
///
/// # Panics
/// Panics when the static JSON schema is rejected as a response format or
/// the static prompt fails construction.
fn structured_prompt() -> Prompt {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["passed"],
        "properties": { "passed": { "type": "boolean" } }
    });
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Judge: {{response}}".to_owned()],
            output: Some(ResponseFormat::json_schema("judge_result", schema).expect("schema")),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

/// One mocked OpenAI chat completion answering `text`.
fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                ..Default::default()
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

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
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        _deadline: Instant,
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
) -> Result<vala_verify::blocks::ActionOutput, BlockError> {
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
) -> Result<vala_verify::blocks::CheckOutput, BlockError> {
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

/// An Agent Card whose inline portable Prompt over `{{response}}` declares
/// `format` as its `response_format`, decoded the way Cards are.
///
/// # Panics
/// Panics when the Agent does not decode.
fn named_judge(format: Value) -> AgentCall {
    let spec: AgentSpec = serde_json::from_value(json!({
        "prompt": {"provider": "openai", "model": "gpt-test",
            "messages": [{"role": "user", "content": "Judge: {{response}}"}],
            "response_format": format},
        "execution_route": {"kind": "native"},
    }))
    .expect("the named-format Agent decodes");
    AgentCall {
        agent: InlineableRef::Inline(Box::new(spec)),
        inputs: BTreeMap::new(),
        workspace: BTreeMap::new(),
    }
}

/// A check Agent's answer must satisfy its whole declared `verdict` format:
/// valid custom data, including exact size and depth boundaries and failed
/// judgments, is kept; missing, wrong, null, undeclared, oversized, or too
/// deep data and unknown envelope members are terminal without echoing the
/// answer; the plain format refuses any data; and a citation-less pass is
/// still inconclusive.
///
/// # Panics
/// Panics when an invalid answer is accepted, a valid one is refused, or a
/// refusal echoes the answer.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_validates_custom_data() {
    let data = json!({"type": "object", "properties": {"score": {"type": "number"}, "note": {"type": "string"}, "tree": {}},
        "required": ["score", "note"], "additionalProperties": false});
    let custom = named_judge(json!({"kind": "verdict", "data": data}));
    let cited = json!([{"slot": "deck", "locator": {"page": 3}}]);
    let answer = |outcome: &str, extra: Value| {
        let mut body = json!({"outcome": outcome, "reason": "judged", "citations": cited});
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        openai_text_response(&body.to_string())
    };
    let deep = |containers: usize| {
        let mut tree = json!(1);
        for _ in 1..containers {
            tree = json!([tree]);
        }
        tree
    };
    let note = |bytes: usize| "é".repeat(bytes / 2);
    let exact = 65_536 - json!({"score": 1, "note": ""}).to_string().len();
    let accepted = [
        (
            "passed",
            json!({"data": {"score": 0.92, "note": "information_disclosure"}}),
        ),
        (
            "failed",
            json!({"data": {"score": 0.1, "note": "\u{1}\n\"quoted\""}}),
        ),
        ("failed", json!({"data": {"score": 1, "note": note(exact)}})),
        (
            "failed",
            json!({"data": {"score": 1, "note": "", "tree": deep(16)}}),
        ),
    ];
    for (outcome, extra) in accepted {
        let executor =
            executor(vec![answer(outcome, extra.clone())]).with_evidence(Arc::new(PinnedDeck));
        let output = check(&executor, &custom, from_deck(json!("x")))
            .await
            .unwrap_or_else(|error| panic!("{outcome}: {error}"));
        assert_eq!(
            serde_json::to_value(&output.verdict.data).expect("data"),
            extra["data"]
        );
    }

    let secret = "s3cr3t-payload";
    let refused = [
        json!({}),
        json!({"data": null}),
        json!({"data": {"score": secret, "note": secret}}),
        json!({"data": {"score": 1}}),
        json!({"data": {"score": 1, "note": secret, "extra": secret}}),
        json!({"data": {"score": 1, "note": secret}, "confidence": secret}),
        json!({"data": {"score": 1, "note": format!("{secret}{}", note(exact))}}),
        json!({"data": {"score": 1, "note": secret, "tree": deep(17)}}),
    ];
    for extra in refused {
        let executor =
            executor(vec![answer("passed", extra.clone())]).with_evidence(Arc::new(PinnedDeck));
        match check(&executor, &custom, from_deck(json!("x"))).await {
            Err(BlockError::Terminal { reason }) => {
                assert!(
                    !reason.contains(secret),
                    "{extra}: the refusal echoes the answer: {reason}"
                );
            }
            other => panic!("{extra}: expected a terminal refusal, got {other:?}"),
        }
    }

    for format in [json!("verdict"), json!({"kind": "verdict"})] {
        let plain = named_judge(format.clone());
        let executor = executor(vec![
            answer("failed", json!({})),
            answer("failed", json!({"data": {"score": 1}})),
            answer("passed", json!({"citations": []})),
        ])
        .with_evidence(Arc::new(PinnedDeck));
        let output = check(&executor, &plain, from_deck(json!("x")))
            .await
            .expect("plain Verdict");
        assert_eq!(output.verdict.outcome, VerificationVerdict::Failed);
        assert_eq!(output.verdict.data, None);
        assert!(
            matches!(
                check(&executor, &plain, from_deck(json!("x"))).await,
                Err(BlockError::Terminal { .. })
            ),
            "{format}: the plain format refuses data"
        );
        let uncited = check(&executor, &plain, from_deck(json!("x")))
            .await
            .expect("uncited pass");
        assert_eq!(
            uncited.verdict.outcome,
            VerificationVerdict::Inconclusive,
            "{format}"
        );
    }
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

/// A check Agent declaring a tool other than `bifrost.query` is refused
/// before any model call.
///
/// # Panics
/// Panics when the check runs or the refusal does not name the allowed tool.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_refuses_tools_other_than_bifrost_query() {
    let executor = executor(Vec::new()).with_toolset(Arc::new(FixedToolset));
    let error = check(
        &executor,
        &agent(verdict_prompt(), &["fs.read"]),
        input("response", json!("ok")),
    )
    .await
    .expect_err("check Agents may declare only bifrost.query");
    assert!(
        matches!(&error, BlockError::Terminal { reason } if reason.contains("bifrost.query")),
        "{error}"
    );
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

/// A `code.run` double refused process capacity, as the hosted sandbox
/// refuses it.
struct BusyCode;

#[async_trait]
impl AgentTool for BusyCode {
    /// The built-in wire name.
    fn name(&self) -> &str {
        BuiltinTool::CodeRun.as_str()
    }

    /// A fixed description.
    fn description(&self) -> &'static str {
        "run code"
    }

    /// Any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Any object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Fail with a [`BlockError::Busy`] cause.
    ///
    /// # Errors
    /// Always [`ToolError::Invocation`] caused by [`BlockError::Busy`].
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let busy = BlockError::Busy {
            reason: "no worker".to_owned(),
        };
        Err(ToolError::Invocation {
            detail: busy.to_string(),
            cause: Some(Box::new(busy)),
        })
    }
}

/// A `bifrost.query` double that never answers and raises `dropped` when
/// its pending call is dropped.
struct PendingQuery {
    /// Set once a pending call's future is dropped.
    dropped: Arc<std::sync::atomic::AtomicBool>,
}

/// Raises its flag when dropped with the pending call that owns it.
struct DropFlag(Arc<std::sync::atomic::AtomicBool>);

impl Drop for DropFlag {
    /// Record that the owning future was dropped.
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl AgentTool for PendingQuery {
    /// The built-in wire name.
    fn name(&self) -> &str {
        BuiltinTool::BifrostQuery.as_str()
    }

    /// A fixed description.
    fn description(&self) -> &'static str {
        "query"
    }

    /// Any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Any object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Wait forever while holding a [`DropFlag`].
    ///
    /// # Errors
    /// Never returns.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let _flag = DropFlag(Arc::clone(&self.dropped));
        std::future::pending().await
    }
}

/// A toolset binding [`BusyCode`] and a [`PendingQuery`] sharing `dropped`.
struct BusyToolset {
    /// Flag handed to the pending query.
    dropped: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl AgentToolset for BusyToolset {
    /// Open a session holding [`BusyCode`] and [`PendingQuery`].
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses either tool.
    async fn open(
        &self,
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        _deadline: Instant,
        _tools: &[BuiltinTool],
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(BusyCode))
            .expect("code registers");
        registry
            .register(Arc::new(PendingQuery {
                dropped: Arc::clone(&self.dropped),
            }))
            .expect("query registers");
        Ok(ToolSession::new(registry, 1024, Box::new(())))
    }
}

/// An OpenAI answer calling `code_run` and `bifrost_query` in parallel.
fn busy_and_pending_calls() -> ProviderResponse {
    let call = |id: &str, name: &str| OpenAiToolCall {
        id: id.to_owned(),
        kind: "function".to_owned(),
        function: OpenAiToolFunctionCall {
            name: name.to_owned(),
            arguments: "{}".to_owned(),
        },
    };
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_tools".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                tool_calls: Some(vec![
                    call("call_code", "code_run"),
                    call("call_query", "bifrost_query"),
                ]),
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

/// A `code.run` refused process capacity while a parallel sibling tool is
/// still pending ends the engine's timed Agent step busy at once: the step
/// deadline is not reached, no authored retry or further model call runs,
/// and the pending sibling's future is dropped.
///
/// # Panics
/// Panics when the execution is not [`vala_verify::blocks::EngineError::Busy`],
/// waits for the step deadline, retries, calls the model again, or leaves
/// the sibling pending.
#[tokio::test(start_paused = true)]
async fn busy_tool_ends_the_agent_step_before_a_pending_sibling() {
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(busy_and_pending_calls());
    mock.push_response(busy_and_pending_calls());
    mock.push_response(openai_text_response("unreachable"));
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock.clone()));
    let executor = Arc::new(
        AgentExecutor::new(
            Arc::new(registry),
            Arc::new(NoRegistry),
            Arc::new(NoRegistry),
        )
        .with_toolset(Arc::new(BusyToolset {
            dropped: Arc::clone(&dropped),
        })),
    );
    let mut action = serde_json::to_value(Action::Agent(agent(
        text_prompt(),
        &["code.run", "bifrost.query"],
    )))
    .expect("action serializes");
    action["inputs"] = json!({"document": "inputs.document"});
    let spec: wyrd_spec::card::verifier::VerifierSpec = serde_json::from_value(json!({
        "inputs": {"document": {"type": "json"}},
        "steps": [{
            "id": "draft",
            "action": action,
            "timeout_seconds": 30,
            "retries": 1
        }],
        "requirements": [{"id": "drafted", "statement": "Drafted", "check": {
            "kind": "assert", "path": "steps.draft.output", "operator": "is_not_null"
        }}]
    }))
    .expect("Verifier");
    spec.validate().expect("valid Verifier");
    let engine = vala_verify::blocks::VerificationEngine::new(
        Arc::clone(&executor) as Arc<dyn ActionExecutor>,
        executor as Arc<dyn CheckExecutor>,
        Arc::new(vala_verify::InMemoryTraceSource::new()),
        std::num::NonZeroUsize::new(1).expect("positive"),
    );
    let started = Instant::now();
    let refused = engine
        .run(
            Arc::new(spec),
            BTreeMap::from([(
                SlotName::new("document").expect("slot"),
                InputDeclaration::Json {
                    value: json!("report"),
                },
            )]),
            started + Duration::from_secs(60),
        )
        .await;

    assert!(
        matches!(refused, Err(vala_verify::blocks::EngineError::Busy(ref reason)) if reason == "no worker"),
        "{refused:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the refusal waited for the step deadline: {:?}",
        started.elapsed()
    );
    assert_eq!(
        mock.remaining(),
        2,
        "neither a retry nor a second model call ran"
    );
    assert!(
        dropped.load(std::sync::atomic::Ordering::SeqCst),
        "the pending sibling was dropped"
    );
}
