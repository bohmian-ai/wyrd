//! Agent actions and checks on the shared Skald Agent loop.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{MockProvider, Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall,
};
use skald_spec::{MediaRef, ProviderName, ProviderRequest, ProviderResponse};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use tokio::time::Instant;
use vala_verify::blocks::{
    AGENT_PHASE_DURATION_SECONDS, ActionExecutor, AgentCardResolver, AgentExecutor, AgentToolset,
    BlockCall, BlockError, CallInput, CallValue, CheckExecutor, EvidenceReader, Position,
    PromptCardResolver, ToolCallsOwner, ToolLimits, ToolSession,
};
use wyrd_benchmark::MetricValue;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec, BuiltinTool, ExecutionRoute};
use wyrd_spec::card::verifier::{Action, AgentCall, Check};
use wyrd_spec::ids::SlotName;
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{
    CallProvenance, Citation, InputDeclaration, Locator, Producer, RequirementOutcome,
    ToolRecipient,
};

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

/// Session limits small enough to reach in a test.
const fn limits() -> ToolLimits {
    ToolLimits {
        max_result_bytes: 1024,
        max_argument_bytes: 1024,
        max_calls: 16,
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
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(FixedRead))
            .expect("one tool registers");
        Ok(ToolSession::new(registry, limits(), Box::new(())))
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

/// `call` with a one-iteration budget, so its first invalid structured
/// answer exhausts the Agent instead of being repaired.
fn once(mut call: AgentCall) -> AgentCall {
    if let InlineableRef::Inline(spec) = &mut call.agent {
        spec.run_config.max_iterations = Some(1);
    }
    call
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
            value: CallValue::Json(value.into()),
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
    step_recording(executor, call, inputs, &ToolCallsOwner::default()).await
}

/// Run `call` as a step action with `inputs`, recording into `calls`, and
/// return once the host work it started has settled.
///
/// # Errors
/// Returns the executor's [`BlockError`] for the step.
async fn step_recording(
    executor: &AgentExecutor,
    call: &AgentCall,
    inputs: BTreeMap<String, CallInput>,
    calls: &ToolCallsOwner,
) -> Result<vala_verify::blocks::ActionOutput, BlockError> {
    let id = tid("summarize");
    let action = Action::Agent(call.clone());
    let output = ActionExecutor::execute(
        executor,
        BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs,
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            attempt: 1,
            item_index: None,
            iteration_index: None,
            tool_calls: calls.handle(),
        },
    )
    .await;
    calls.settle().await;
    output
}

/// Run `call` as a Verifier requirement check with `inputs`, returning once
/// the host work it started has settled.
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
    let calls = ToolCallsOwner::default();
    let output = CheckExecutor::execute(
        executor,
        BlockCall {
            id: &id,
            position: Position::VerifierCheck,
            block: &block,
            inputs,
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            attempt: 1,
            item_index: None,
            iteration_index: None,
            tool_calls: calls.handle(),
        },
    )
    .await;
    calls.settle().await;
    output
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

/// A structured answer still violating the Prompt's response schema when
/// the budget runs out is terminal with the stable code and no answer
/// content; within the budget it is repaired.
///
/// # Panics
/// Panics when the invalid answer is accepted, retryable, or echoed, or the
/// repaired answer is refused.
#[tokio::test(flavor = "multi_thread")]
async fn step_agent_rejects_schema_violation() {
    let invalid = || openai_text_response(r#"{"passed": "s3cr3t-value", "s3cr3t-key": 1}"#);
    let exhausted = executor(vec![invalid()]);
    let error = step(
        &exhausted,
        &once(agent(structured_prompt(), &[])),
        input("response", json!("ok")),
    )
    .await
    .expect_err("the answer violates the schema");
    let BlockError::Terminal { reason } = &error else {
        panic!("expected terminal, got {error}");
    };
    assert!(
        reason.starts_with("WYRD_AGENT_422_STRUCTURED_INVALID: "),
        "{reason}"
    );
    assert!(!reason.contains("s3cr3t"), "{reason}");

    let repaired = executor(vec![invalid(), openai_text_response(r#"{"passed": true}"#)]);
    let output = step(
        &repaired,
        &agent(structured_prompt(), &[]),
        input("response", json!("ok")),
    )
    .await
    .expect("the repaired answer is accepted");
    assert_eq!(output.output, json!({"passed": true}));
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
    let calls = ToolCallsOwner::default();
    let output = step_recording(
        &executor,
        &agent(text_prompt(), &["fs.read"]),
        input("document", json!("report")),
        &calls,
    )
    .await
    .expect("the step runs");
    assert_eq!(output.output, json!("revenue is 42"));
    let returned = json!({"text": "revenue is 42"}).to_string();
    let records = calls.settled();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.tool, BuiltinTool::FsRead);
    assert_eq!(record.arguments, json!({"path": "/inputs/report.txt"}));
    assert_eq!(record.recipient, ToolRecipient::Model);
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
    async fn media(
        &self,
        _evidence: &InputDeclaration,
    ) -> Result<(skald_spec::MediaRef, Box<dyn std::any::Any + Send + Sync>), BlockError> {
        Err(BlockError::Terminal {
            reason: "no media".to_owned(),
        })
    }

    /// Accept `deck` page citations within its five pages.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for any other citation.
    async fn validate(&self, citations: &[&Citation]) -> Result<(), BlockError> {
        match citations.iter().find(|citation| {
            citation.slot.as_str() != "deck" || !matches!(citation.locator, Locator::Page(1..=5))
        }) {
            Some(citation) => Err(BlockError::Terminal {
                reason: format!(
                    "invalid citation: slot {} has no such location",
                    citation.slot
                ),
            }),
            None => Ok(()),
        }
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
            value: CallValue::Json(value.into()),
            sources: BTreeSet::from([SlotName::new("deck").expect("slot")]),
        },
    )])
}

/// The Agent executor returns its check's Verdict as the Agent authored it,
/// citations unfiltered. Through the engine every valid verdict keeps its
/// outcome, an uncited pass included, while a citation the check cannot
/// reach, which its narrowed format already refuses at the provider, or one
/// with no location in its pinned evidence, is an explicit inconclusive error
/// rather than a dropped citation.
///
/// # Panics
/// Panics when a verdict is rewritten, a citation dropped, or an invalid
/// citation accepted.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_verdicts_are_kept_and_citations_validated() {
    let answer = |outcome: &str, citations: &Value| {
        openai_text_response(
            &json!({"outcome": outcome, "reason": "judged", "citations": citations}).to_string(),
        )
    };
    let unreachable = json!([{"slot": "policy", "locator": {"page": 1}}]);
    let authored =
        executor(vec![answer("passed", &unreachable)]).with_evidence(Arc::new(PinnedDeck));
    let output = check(
        &authored,
        &agent(verdict_prompt(), &[]),
        from_deck(json!("x")),
    )
    .await
    .expect("the check runs");
    assert_eq!(output.verdict.outcome, VerificationVerdict::Passed);
    assert_eq!(
        serde_json::to_value(&output.verdict.citations).expect("citations"),
        unreachable
    );

    let page = |page: u32| json!([{"slot": "deck", "locator": {"page": page}}]);
    let cases = [
        ("passed", json!([]), RequirementOutcome::Passed, "judged"),
        ("passed", page(3), RequirementOutcome::Passed, "judged"),
        ("failed", json!([]), RequirementOutcome::Failed, "judged"),
        (
            "inconclusive",
            json!([]),
            RequirementOutcome::Inconclusive,
            "judged",
        ),
        (
            "passed",
            unreachable,
            RequirementOutcome::Inconclusive,
            "provider failure SKALD_PROVIDERS_400_BAD_REQUEST",
        ),
        (
            "passed",
            page(9),
            RequirementOutcome::Inconclusive,
            "invalid citation: slot deck has no such location",
        ),
    ];
    for (outcome, citations, expected, reason) in cases {
        let spec: wyrd_spec::card::verifier::VerifierSpec = serde_json::from_value(json!({
            "inputs": {"deck": {"type": "json"}, "policy": {"type": "json"}},
            "requirements": [{"id": "grounded", "statement": "Grounded", "check": {
                "kind": "agent", "agent": named_judge(json!("verdict")).agent,
                "inputs": {"response": "inputs.deck"}}}]
        }))
        .expect("Verifier");
        spec.validate().expect("valid Verifier");
        let executor = Arc::new(executor(vec![answer(outcome, &citations)]));
        let engine = vala_verify::blocks::VerificationEngine::new(
            Arc::clone(&executor) as Arc<dyn ActionExecutor>,
            executor as Arc<dyn CheckExecutor>,
            Arc::new(vala_verify::InMemoryTraceSource::new()),
            std::num::NonZeroUsize::new(1).expect("positive"),
        )
        .with_evidence(Arc::new(PinnedDeck));
        let json = |value: Value| InputDeclaration::Json { value };
        let executed = engine
            .run(
                Arc::new(spec),
                BTreeMap::from([
                    (
                        SlotName::new("deck").expect("slot"),
                        json(json!("deck text")),
                    ),
                    (SlotName::new("policy").expect("slot"), json(json!({}))),
                ]),
                Instant::now() + Duration::from_secs(60),
            )
            .await
            .expect("the execution settles");
        let result = &executed.requirements[0].result;
        assert_eq!(result.outcome, expected, "{outcome} {citations}");
        assert_eq!(result.reason, reason, "{outcome} {citations}");
        if expected == RequirementOutcome::Passed {
            assert_eq!(
                serde_json::to_value(&result.citations).expect("citations"),
                citations
            );
        }
    }
}

/// A benchmark-recorded Agent check times each of its phases once into
/// [`AGENT_PHASE_DURATION_SECONDS`].
///
/// # Panics
/// Panics when the check fails or a phase is missing or counted twice.
#[tokio::test(flavor = "multi_thread")]
async fn benchmark_check_agent_times_each_phase() {
    wyrd_benchmark::install().expect("the only recorder");
    let pinned = executor(vec![openai_text_response(
        &json!({"outcome": "passed", "reason": "grounded", "citations": [{"slot": "deck", "locator": {"page": 3}}]})
            .to_string(),
    )])
    .with_evidence(Arc::new(PinnedDeck));
    check(
        &pinned,
        &agent(verdict_prompt(), &[]),
        from_deck(json!("deck text")),
    )
    .await
    .expect("the check runs");
    let metrics = wyrd_benchmark::snapshot().metrics;
    for phase in ["resolve", "bind", "run", "verdict"] {
        let series = metrics
            .iter()
            .find(|metric| metric.matches(AGENT_PHASE_DURATION_SECONDS, &[("phase", phase)]))
            .unwrap_or_else(|| panic!("no {phase} phase: {metrics:?}"));
        assert!(
            matches!(series.value, MetricValue::Histogram { count: 1, .. }),
            "{phase}: {series:?}"
        );
    }
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
/// valid custom data, including exactly 65,536 compact bytes of escaped
/// UTF-8 text or of a wide array, 16 nested containers, and failed
/// judgments, is kept; missing, wrong, null, or undeclared data, one byte
/// past either size boundary, 17 containers, and unknown envelope members
/// are terminal without echoing the answer on a one-iteration budget; the
/// plain format refuses any data; and a citation-less pass stays passed.
///
/// # Panics
/// Panics when an invalid answer is accepted, a valid one is refused, or a
/// refusal echoes the answer.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_validates_custom_data() {
    let data = json!({"type": "object", "properties": {"score": {"type": "number"}, "note": {"type": "string"}, "tree": {}},
        "required": ["score", "note"], "additionalProperties": false});
    let custom = once(named_judge(json!({"kind": "verdict", "data": data})));
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
    // {"score":1,"note":""} is 21 compact bytes; 32,000 two-byte characters,
    // the escaped newline and quote (2 bytes each), and the escaped U+0001
    // (6 bytes) leave 1,505. {"score":1,"note":"","tree":[]} is 31 bytes, and
    // 32,753 one-digit items with their commas take the remaining 65,505.
    let utf8 = format!("{}\n\"\u{1}", "é".repeat(32_000));
    let text =
        |pad: usize| json!({"data": {"score": 1, "note": format!("{utf8}{}", "x".repeat(pad))}});
    let wide = |last: u8| {
        let mut tree = vec![0; 32_752];
        tree.push(last);
        json!({"data": {"score": 1, "note": "", "tree": tree}})
    };
    let bytes = |extra: &Value| serde_json::to_vec(&extra["data"]).expect("json").len();
    let (exact_text, over_text) = (text(1_505), text(1_506));
    let (exact_wide, over_wide) = (wide(0), wide(10));
    for (extra, expected) in [
        (&exact_text, 65_536),
        (&over_text, 65_537),
        (&exact_wide, 65_536),
        (&over_wide, 65_537),
    ] {
        assert_eq!(bytes(extra), expected, "boundary fixture size");
    }
    let accepted = [
        (
            "passed",
            json!({"data": {"score": 0.92, "note": "information_disclosure"}}),
        ),
        (
            "failed",
            json!({"data": {"score": 0.1, "note": "\u{1}\n\"quoted\""}}),
        ),
        ("failed", exact_text),
        ("failed", exact_wide),
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
        over_text,
        over_wide,
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
        let plain = once(named_judge(format.clone()));
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
            VerificationVerdict::Passed,
            "{format}"
        );
    }

    let rooted = once(named_judge(
        json!({"kind": "verdict", "data": {"type": "object",
        "required": ["reason", "copy", "pair"],
        "properties": {"reason": {"type": "integer"}, "copy": {"$ref": "#/properties/reason"},
            "pair": {"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]}}}}),
    ));
    let good = json!({"reason": 1, "copy": 2, "pair": ["a", 1]});
    let executor = executor(vec![
        answer("passed", json!({"data": good})),
        answer(
            "passed",
            json!({"data": {"reason": 1, "copy": "wrong", "pair": ["a", 1]}}),
        ),
        answer(
            "passed",
            json!({"data": {"reason": 1, "copy": 2, "pair": [1, "a"]}}),
        ),
    ])
    .with_evidence(Arc::new(PinnedDeck));
    let output = check(&executor, &rooted, from_deck(json!("x")))
        .await
        .expect("data refs resolve against the data schema");
    assert_eq!(
        serde_json::to_value(&output.verdict.data).expect("data"),
        good
    );
    for _ in 0..2 {
        assert!(
            matches!(
                check(&executor, &rooted, from_deck(json!("x"))).await,
                Err(BlockError::Terminal { .. })
            ),
            "data violating its own refs or prefixItems is refused"
        );
    }
}

/// A check Agent answer outside the Verdict schema exhausts a one-iteration
/// budget terminally, with the stable code and no answer content.
///
/// # Panics
/// Panics when the answer is accepted, retryable, or echoed.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_rejects_non_verdict() {
    let executor = executor(vec![openai_text_response(
        r#"{"outcome": "s3cr3t", "s3cr3t-key": {"nested": "s3cr3t"}}"#,
    )]);
    let error = check(
        &executor,
        &once(named_judge(json!("verdict"))),
        input("response", json!("ok")),
    )
    .await
    .expect_err("the answer is not a Verdict");
    let BlockError::Terminal { reason } = &error else {
        panic!("expected terminal, got {error}");
    };
    assert!(
        reason.starts_with("WYRD_AGENT_422_STRUCTURED_INVALID: "),
        "{reason}"
    );
    assert!(!reason.contains("s3cr3t"), "{reason}");
}

/// A check Agent whose provider refuses without an answer is terminal with
/// the stable code after one call, though budget and a valid Verdict remain.
///
/// # Panics
/// Panics when the refusal is repaired, accepted, or echoed.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_refusal_is_not_repaired() {
    let mut refusal = openai_text_response("");
    if let ProviderResponse::OpenAiChatCompletion(response) = &mut refusal {
        response.choices[0].message.content = None;
        response.choices[0].message.refusal = Some("s3cr3t".to_owned());
    }
    let valid = openai_text_response(
        &json!({"outcome": "passed", "reason": "r", "citations": []}).to_string(),
    );
    let (executor, provider) = recording(vec![refusal, valid]);
    let error = check(
        &executor,
        &named_judge(json!("verdict")),
        input("response", json!("ok")),
    )
    .await
    .expect_err("a refusal is inconclusive");
    let BlockError::Terminal { reason } = &error else {
        panic!("expected terminal, got {error}");
    };
    assert!(
        reason.starts_with("WYRD_AGENT_422_STRUCTURED_INVALID: "),
        "{reason}"
    );
    assert!(!reason.contains("s3cr3t"), "{reason}");
    assert_eq!(provider.requests.lock().expect("unpoisoned").len(), 1);
}

/// A provider that records each native request before answering from a
/// [`MockProvider`] queue.
#[derive(Clone)]
struct Recording {
    /// The scripted answers.
    mock: MockProvider,
    /// Requests in send order.
    requests: Arc<std::sync::Mutex<Vec<ProviderRequest>>>,
}

#[async_trait]
impl Provider for Recording {
    /// Records `request` and answers from the queue.
    ///
    /// # Errors
    /// Returns the mock's error for an empty queue.
    ///
    /// # Panics
    /// Panics when the request log's lock is poisoned.
    async fn send(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests
            .lock()
            .expect("unpoisoned")
            .push(request.clone());
        self.mock.send(request).await
    }

    /// Streams are not used.
    ///
    /// # Errors
    /// Always the mock's empty-stream error.
    async fn stream(&self, request: &ProviderRequest) -> Result<ProviderStream, ProviderError> {
        self.mock.stream(request).await
    }

    /// The mocked provider name.
    fn name(&self) -> ProviderName {
        self.mock.name()
    }
}

/// An executor over a [`Recording`] provider answering `responses`.
fn recording(responses: Vec<ProviderResponse>) -> (AgentExecutor, Recording) {
    let provider = Recording {
        mock: MockProvider::new(ProviderName::OpenAi),
        requests: Arc::default(),
    };
    for response in responses {
        provider.mock.push_response(response);
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider.clone()));
    let executor = AgentExecutor::new(
        Arc::new(providers),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_evidence(Arc::new(PinnedDeck));
    (executor, provider)
}

/// The citation-slot `enum` one recorded OpenAI chat request sends, null
/// when the slots are unrestricted.
///
/// # Panics
/// Panics when the request carries no JSON schema response format.
fn sent_slots(request: &ProviderRequest) -> Value {
    let body = serde_json::to_value(request).expect("request serializes");
    body["body"]["response_format"]["json_schema"]["schema"]["$defs"]["SlotName"]["enum"].clone()
}

/// One JSON `response` input whose provenance is `slots`.
///
/// # Panics
/// Panics when a slot name is invalid.
fn sourced(value: &str, slots: &[&str]) -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        "response".to_owned(),
        CallInput {
            value: CallValue::Json(json!(value).into()),
            sources: slots
                .iter()
                .map(|&slot| SlotName::new(slot).expect("slot"))
                .collect(),
        },
    )])
}

/// A check's provider request admits exactly the invocation's reachable
/// slots, so a prefixed slot is repaired before judgment; concurrent calls
/// keep distinct restrictions, the Agent Card is untouched, and a call with
/// no reachable source sends the unrestricted slot definition.
///
/// # Panics
/// Panics when a request's slots differ from its call's, the wrong slot is
/// not repaired, or the Card changes.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_narrows_citation_slots_per_call() {
    let verdict = |slot: &str| {
        openai_text_response(
            &json!({"outcome": "passed", "reason": "r", "citations": [{"slot": slot, "locator": {"page": 3}}]})
                .to_string(),
        )
    };
    let judge = named_judge(json!("verdict"));
    let card = serde_json::to_value(&judge.agent).expect("card");
    let (executor, provider) = recording(vec![verdict("inputs.deck"), verdict("deck")]);
    let output = check(&executor, &judge, sourced("x", &["deck"]))
        .await
        .expect("the repaired check runs");
    assert_eq!(output.verdict.outcome, VerificationVerdict::Passed);
    let requests = provider.requests.lock().expect("unpoisoned").clone();
    assert_eq!(requests.len(), 2, "one repair");
    for request in &requests {
        assert_eq!(sent_slots(request), json!(["deck"]));
    }
    assert!(
        serde_json::to_string(&requests[1])
            .expect("serializes")
            .contains("was not accepted"),
        "the repair turn reaches the provider"
    );

    let (executor, provider) = recording(vec![verdict("deck"), verdict("deck"), verdict("deck")]);
    let (first, second) = tokio::join!(
        check(&executor, &judge, sourced("call-a", &["deck"])),
        check(&executor, &judge, sourced("call-b", &["deck", "policy"])),
    );
    first.expect("first check");
    second.expect("second check");
    check(&executor, &judge, sourced("call-c", &[]))
        .await
        .expect("unsourced check");
    for request in provider.requests.lock().expect("unpoisoned").iter() {
        let text = serde_json::to_string(request).expect("serializes");
        let expected = if text.contains("call-a") {
            json!(["deck"])
        } else if text.contains("call-b") {
            json!(["deck", "policy"])
        } else {
            Value::Null
        };
        assert_eq!(sent_slots(request), expected, "{text}");
    }
    assert_eq!(serde_json::to_value(&judge.agent).expect("card"), card);
}

/// A check Agent reads its workspace through its declared tools, and a
/// citation into a slot reachable only through that workspace survives.
///
/// # Panics
/// Panics when the check is refused or the workspace citation is dropped.
#[tokio::test(flavor = "multi_thread")]
async fn check_agent_reads_its_workspace_through_declared_tools() {
    let executor = executor(vec![
        read_call(),
        openai_text_response(
            &json!({"outcome": "passed", "reason": "read", "citations": [{"slot": "deck", "locator": {"page": 3}}]})
                .to_string(),
        ),
    ])
    .with_toolset(Arc::new(FixedToolset))
    .with_evidence(Arc::new(PinnedDeck));
    let id = tid("grounded");
    let block = Check::Agent(agent(verdict_prompt(), &["fs.read"]));
    let calls = ToolCallsOwner::default();
    let output = CheckExecutor::execute(
        &executor,
        BlockCall {
            id: &id,
            position: Position::VerifierCheck,
            block: &block,
            inputs: input("response", json!("ok")),
            workspace: BTreeMap::from([(
                "/inputs/report.txt".to_owned(),
                CallInput {
                    value: CallValue::Json(json!("report").into()),
                    sources: BTreeSet::from([SlotName::new("deck").expect("slot")]),
                },
            )]),
            deadline: Instant::now() + Duration::from_secs(30),
            attempt: 1,
            item_index: None,
            iteration_index: None,
            tool_calls: calls.handle(),
        },
    )
    .await;
    calls.settle().await;
    let output = output.expect("the check runs");
    assert_eq!(output.verdict.outcome, VerificationVerdict::Passed);
    assert_eq!(output.verdict.citations.len(), 1);
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

    /// Fail with a [`BlockError::Busy`] cause after yielding once, so a
    /// sibling call is already in flight when the session stops.
    ///
    /// # Errors
    /// Always [`ToolError::Invocation`] caused by [`BlockError::Busy`].
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        tokio::task::yield_now().await;
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
        _producer: Option<&Producer>,
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
        Ok(ToolSession::new(registry, limits(), Box::new(())))
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

/// `call` limited to `iterations` model calls.
fn iterations(mut call: AgentCall, iterations: u32) -> AgentCall {
    if let InlineableRef::Inline(spec) = &mut call.agent {
        spec.run_config.max_iterations = Some(iterations);
    }
    call
}

/// Run `spec` once over a JSON `document` on `executor`.
///
/// # Errors
/// Returns the engine's error for the execution.
///
/// # Panics
/// Panics when `spec` is not a valid Verifier.
async fn run_engine(
    executor: AgentExecutor,
    spec: Value,
) -> Result<wyrd_spec::verification::execution::ExecutedOutcome, vala_verify::blocks::EngineError> {
    let spec: wyrd_spec::card::verifier::VerifierSpec =
        serde_json::from_value(spec).expect("Verifier");
    spec.validate().expect("valid Verifier");
    let executor = Arc::new(executor);
    let engine = vala_verify::blocks::VerificationEngine::new(
        Arc::clone(&executor) as Arc<dyn ActionExecutor>,
        executor as Arc<dyn CheckExecutor>,
        Arc::new(vala_verify::InMemoryTraceSource::new()),
        std::num::NonZeroUsize::new(1).expect("positive"),
    );
    engine
        .run(
            Arc::new(spec),
            BTreeMap::from([(
                SlotName::new("document").expect("slot"),
                InputDeclaration::Json {
                    value: json!("report"),
                },
            )]),
            Instant::now() + Duration::from_secs(60),
        )
        .await
}

/// `block` bound to `inputs.document` through `{{response}}` and as its
/// workspace file.
///
/// # Panics
/// Panics when `block` does not serialize to a JSON object.
fn bound<B: serde::Serialize>(block: B) -> Value {
    let mut block = serde_json::to_value(block).expect("block serializes");
    block["inputs"] = json!({"response": "inputs.document"});
    block["workspace"] = json!({"/inputs/report.txt": "inputs.document"});
    block
}

/// Tool calls an Agent made before its answer was refused stay on the
/// produced result: a failed step and an inconclusive check each carry
/// their completed call with its digest.
///
/// # Panics
/// Panics when the step does not fail, the check is not inconclusive, or
/// either result loses its recorded call.
#[tokio::test(flavor = "multi_thread")]
async fn agent_records_survive_failed_steps_and_inconclusive_checks() {
    let executor = executor(vec![
        read_call(),
        openai_text_response(r#"{"passed": "not a boolean"}"#),
        read_call(),
        openai_text_response(r#"{"not": "a verdict"}"#),
    ])
    .with_toolset(Arc::new(FixedToolset));
    let step = bound(Action::Agent(iterations(
        agent(structured_prompt(), &["fs.read"]),
        2,
    )));
    let mut judge = named_judge(json!("verdict"));
    if let InlineableRef::Inline(spec) = &mut judge.agent {
        spec.tool_names = vec!["fs.read".to_owned()];
    }
    let check = bound(Check::Agent(iterations(judge, 2)));
    let outcome = run_engine(
        executor,
        json!({
            "inputs": {"document": {"type": "json"}},
            "steps": [{"id": "draft", "action": step}],
            "requirements": [{"id": "grounded", "statement": "Grounded", "check": check}]
        }),
    )
    .await
    .expect("the execution settles");
    let returned = json!({"text": "revenue is 42"}).to_string();
    let step = &outcome.steps[0];
    assert_eq!(
        step.status,
        wyrd_spec::verification::execution::StepStatus::Failed,
        "{step:?}"
    );
    assert_eq!(step.tool_calls.len(), 1, "failed step records: {step:?}");
    assert_eq!(
        step.tool_calls[0].output_sha256,
        Some(Sha256Hex::digest(returned.as_bytes()))
    );
    let requirement = &outcome.requirements[0];
    assert_eq!(
        requirement.result.outcome,
        wyrd_spec::verification::execution::RequirementOutcome::Inconclusive,
        "{requirement:?}"
    );
    assert_eq!(
        requirement.tool_calls.len(),
        1,
        "inconclusive check records: {requirement:?}"
    );
    assert_eq!(requirement.tool_calls[0].tool, BuiltinTool::FsRead);
}

/// A toolset binding [`FixedRead`] and a [`PendingQuery`] within a
/// `max_calls` allowance.
struct BudgetToolset {
    /// Requests each logical invocation may make.
    max_calls: u32,
    /// Flag handed to the pending query.
    dropped: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl AgentToolset for BudgetToolset {
    /// Open a session holding [`FixedRead`] and [`PendingQuery`].
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
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(FixedRead))
            .expect("read registers");
        registry
            .register(Arc::new(PendingQuery {
                dropped: Arc::clone(&self.dropped),
            }))
            .expect("query registers");
        Ok(ToolSession::new(
            registry,
            ToolLimits {
                max_calls: self.max_calls,
                ..limits()
            },
            Box::new(()),
        ))
    }
}

/// An OpenAI answer calling `bifrost_query` once.
///
/// # Panics
/// Panics when [`read_call`] carries no tool call.
fn query_call() -> ProviderResponse {
    let ProviderResponse::OpenAiChatCompletion(mut response) = read_call() else {
        unreachable!("read_call is an OpenAI completion");
    };
    let call = &mut response.choices[0]
        .message
        .tool_calls
        .as_mut()
        .expect("calls")[0];
    "bifrost_query".clone_into(&mut call.function.name);
    "{}".clone_into(&mut call.function.arguments);
    ProviderResponse::OpenAiChatCompletion(response)
}

/// One step's request allowance spans its retries without refund: a call
/// cancelled by the attempt timeout still spent its request and is recorded
/// with no digest, zero bytes, and a cancellation error; the retry's next
/// request fails the step terminally with the budget code and no further
/// model call. Spending exactly the allowance still lets the Agent answer.
///
/// # Panics
/// Panics when the allowance is refunded, exceeded, or renewed by the
/// retry, the cancelled call is lost or reports a result, or the final
/// allowance cannot answer.
#[tokio::test(start_paused = true)]
async fn agent_tool_budget_spans_attempts_without_refund() {
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mock = MockProvider::new(ProviderName::OpenAi);
    for response in [
        read_call(),
        query_call(),
        read_call(),
        openai_text_response("unreachable"),
    ] {
        mock.push_response(response);
    }
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock.clone()));
    let executor = AgentExecutor::new(
        Arc::new(registry),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_toolset(Arc::new(BudgetToolset {
        max_calls: 2,
        dropped: Arc::clone(&dropped),
    }));
    let mut action = bound(Action::Agent(agent(
        structured_prompt(),
        &["fs.read", "bifrost.query"],
    )));
    action["workspace"] = json!({"/inputs/report.txt": "inputs.document"});
    let outcome = run_engine(
        executor,
        json!({
            "inputs": {"document": {"type": "json"}},
            "steps": [{"id": "draft", "action": action, "timeout_seconds": 5, "retries": 1}],
            "requirements": [{"id": "drafted", "statement": "Drafted", "check": {
                "kind": "assert", "path": "steps.draft.output", "operator": "is_not_null"
            }}]
        }),
    )
    .await
    .expect("the execution settles");
    let step = &outcome.steps[0];
    assert_eq!(
        step.attempts, 2,
        "the timed-out attempt was retried: {step:?}"
    );
    let reason = step.reason.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED: "),
        "{reason}"
    );
    assert_eq!(mock.remaining(), 1, "no model call followed the refusal");
    assert!(
        dropped.load(std::sync::atomic::Ordering::SeqCst),
        "the timed-out query was dropped"
    );
    let tools: Vec<_> = step.tool_calls.iter().map(|record| record.tool).collect();
    assert_eq!(tools, [BuiltinTool::FsRead, BuiltinTool::BifrostQuery]);
    let cancelled = &step.tool_calls[1];
    assert_eq!(cancelled.output_sha256, None);
    assert_eq!(cancelled.returned_bytes, 0);
    assert!(
        cancelled
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("cancelled")),
        "{cancelled:?}"
    );

    let last = executor_with_budget(vec![
        read_call(),
        read_call(),
        openai_text_response(r#"{"passed": true}"#),
    ]);
    let calls = ToolCallsOwner::default();
    let output = step_recording(
        &last,
        &agent(structured_prompt(), &["fs.read", "bifrost.query"]),
        input("response", json!("ok")),
        &calls,
    )
    .await
    .expect("the final allowance still answers");
    assert_eq!(output.output, json!({"passed": true}));
    assert_eq!(calls.settled().len(), 2);
}

/// An executor over `responses` whose tools allow two requests.
fn executor_with_budget(responses: Vec<ProviderResponse>) -> AgentExecutor {
    executor(responses).with_toolset(Arc::new(BudgetToolset {
        max_calls: 2,
        dropped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }))
}

/// A `fs.read` double that yields while active and keeps the peak number
/// of concurrent calls.
#[derive(Default)]
struct CountingRead {
    /// Calls running now.
    active: std::sync::atomic::AtomicUsize,
    /// Most calls ever running at once.
    peak: std::sync::atomic::AtomicUsize,
}

/// A registry entry over one shared [`CountingRead`].
struct SharedRead(Arc<CountingRead>);

#[async_trait]
impl AgentTool for SharedRead {
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

    /// Count this call as active across a few yields, then return its
    /// `path` back.
    ///
    /// # Errors
    /// Never fails.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        use std::sync::atomic::Ordering::SeqCst;
        let now = self.0.active.fetch_add(1, SeqCst) + 1;
        self.0.peak.fetch_max(now, SeqCst);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        self.0.active.fetch_sub(1, SeqCst);
        Ok(json!({"text": args["path"]}))
    }
}

/// The registry face of `code.run`; the session runs [`Script`] instead.
struct CodeFace;

#[async_trait]
impl AgentTool for CodeFace {
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

    /// Refuse: a session with a [`CodeTool`] never reaches this.
    ///
    /// # Errors
    /// Always [`ToolError::Invocation`].
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::Invocation {
            detail: "ran without a bridge".to_owned(),
            cause: None,
        })
    }
}

/// A `code.run` program making two concurrent reads, then each refused
/// request kind, then a read it returns before receiving, and returning
/// every received outcome's value or stable code.
struct Script;

#[async_trait]
impl vala_verify::blocks::CodeTool for Script {
    /// Run the fixed program through `bridge`.
    ///
    /// # Errors
    /// Never fails; refusals are part of the result.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let outcome = |result: Result<vala_verify::blocks::Delivery, ToolError>| match result {
            Ok(delivery) => delivery.deliver(),
            Err(error) => json!(wyrd_spec::error::WyrdError::from(&error).code()),
        };
        let (first, second) = tokio::join!(
            bridge.call("fs.read", json!({"path": "/inputs/a.txt"})),
            bridge.call("fs.read", json!({"path": "/inputs/b.txt"})),
        );
        let mut outcomes = vec![outcome(first), outcome(second)];
        for (name, args) in [
            ("cards.get", json!({})),
            ("code.run", json!({"source": "1"})),
            ("fs_read", json!({})),
            ("fs.read", json!(["/inputs/a.txt"])),
            ("fs.read", json!({"path": "x".repeat(2048)})),
        ] {
            outcomes.push(outcome(bridge.call(name, args).await));
        }
        drop(
            bridge
                .call("fs.read", json!({"path": "/inputs/unreceived.txt"}))
                .await,
        );
        Ok(json!({"outcomes": outcomes, "halted": bridge.halted()}))
    }
}

/// A toolset binding `code.run` through [`Script`] and a shared
/// [`CountingRead`].
struct ScriptToolset {
    /// The read every session shares.
    read: Arc<CountingRead>,
}

#[async_trait]
impl AgentToolset for ScriptToolset {
    /// Open a session running [`Script`] with nested reads.
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
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(SharedRead(Arc::clone(&self.read))))
            .expect("read registers");
        registry
            .register(Arc::new(CodeFace))
            .expect("code registers");
        Ok(ToolSession::new(registry, limits(), Box::new(())).with_code(Arc::new(Script)))
    }
}

/// An OpenAI answer calling `code_run` once.
///
/// # Panics
/// Panics when [`read_call`] carries no tool call.
fn code_call() -> ProviderResponse {
    let ProviderResponse::OpenAiChatCompletion(mut response) = read_call() else {
        unreachable!("read_call is an OpenAI completion");
    };
    let call = &mut response.choices[0]
        .message
        .tool_calls
        .as_mut()
        .expect("calls")[0];
    "code_run".clone_into(&mut call.function.name);
    call.function.arguments = json!({"source": "..."}).to_string();
    ProviderResponse::OpenAiChatCompletion(response)
}

/// Nested requests take the direct path: a declared read returns its value
/// to the program and is recorded once with recipient `code` after the
/// outer `code.run` recorded for the model; undeclared, recursive, and
/// provider-spelled names are not in the Agent and malformed or oversized
/// arguments are invalid input, none of them recorded or halting; and at
/// concurrency cap 1 the outer `code.run` holds no permit, so its two
/// concurrent reads complete one at a time, while cap 2 runs them together.
/// A read that settles but is never received by the program keeps no
/// digest or size and settles as cancelled, while the received reads keep
/// theirs.
///
/// # Panics
/// Panics when a refusal has another code, is recorded, or halts, a read
/// is lost, an unreceived value is recorded as returned, or the shared cap
/// is passed or deadlocks.
#[tokio::test(flavor = "multi_thread")]
async fn code_bridge_shares_the_recorded_path() {
    for cap in [1, 2] {
        let read = Arc::new(CountingRead::default());
        let executor = executor(vec![code_call(), openai_text_response("done")]).with_toolset(
            Arc::new(ScriptToolset {
                read: Arc::clone(&read),
            }),
        );
        let mut call = agent(text_prompt(), &["fs.read", "code.run"]);
        if let InlineableRef::Inline(spec) = &mut call.agent {
            spec.run_config.tool_concurrency_cap = Some(cap);
        }
        let calls = ToolCallsOwner::default();
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            step_recording(&executor, &call, input("document", json!("x")), &calls),
        )
        .await
        .unwrap_or_else(|_| panic!("cap {cap} deadlocked"))
        .expect("the step runs");
        assert_eq!(output.output, json!("done"));
        assert_eq!(
            read.peak.load(std::sync::atomic::Ordering::SeqCst),
            cap,
            "peak concurrent reads at cap {cap}"
        );
        let records = calls.settled();
        let shape: Vec<_> = records
            .iter()
            .map(|record| (record.tool, record.recipient))
            .collect();
        assert_eq!(
            shape,
            [
                (BuiltinTool::CodeRun, ToolRecipient::Model),
                (BuiltinTool::FsRead, ToolRecipient::Code),
                (BuiltinTool::FsRead, ToolRecipient::Code),
                (BuiltinTool::FsRead, ToolRecipient::Code),
            ]
        );
        for received in &records[1..3] {
            assert!(
                received.error.is_none() && received.output_sha256.is_some(),
                "{received:?}"
            );
        }
        let unreceived = &records[3];
        assert_eq!(
            (unreceived.output_sha256, unreceived.returned_bytes),
            (None, 0),
            "an unreceived value is not recorded as returned: {unreceived:?}"
        );
        assert!(
            unreceived
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("cancelled")),
            "{unreceived:?}"
        );
        let result = json!({
                "outcomes": [
                    {"text": "/inputs/a.txt"},
                    {"text": "/inputs/b.txt"},
                    "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                    "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                    "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                    "WYRD_TOOL_422_INPUT",
                    "WYRD_TOOL_422_INPUT",
                ],
            "halted": false
        });
        assert_eq!(
            records[0].output_sha256,
            Some(Sha256Hex::digest(result.to_string().as_bytes())),
            "the outer record digests the program's result"
        );
        assert!(records[0].error.is_none(), "{records:?}");
    }
}

/// A `fs.read` double recording each invoked path; a read of `/held`
/// waits for one permit of `gate`.
struct GatedRead {
    /// Paths invoked, in order.
    invoked: Arc<std::sync::Mutex<Vec<String>>>,
    /// Released by the program to let the held read finish.
    gate: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl AgentTool for GatedRead {
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

    /// Record the path, wait on the gate for `/held`, and return the path.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the record lock is poisoned or the gate is closed.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let path = args["path"].as_str().unwrap_or_default().to_owned();
        self.invoked.lock().expect("invoked").push(path.clone());
        if path == "/held" {
            self.gate.acquire().await.expect("gate").forget();
        }
        Ok(json!({"text": path}))
    }
}

/// A `code.run` program that starts a read holding the only permit and a
/// second read waiting for it, then makes a request past the allowance and
/// only afterwards releases the first read and awaits both, all within one
/// poll of the program.
struct HaltScript {
    /// The gate of the held read.
    gate: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl vala_verify::blocks::CodeTool for HaltScript {
    /// Run the fixed program through `bridge`.
    ///
    /// # Errors
    /// Never fails; outcomes are part of the result.
    ///
    /// # Panics
    /// Panics when a read finishes before the refusal.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let outcome = |result: Result<vala_verify::blocks::Delivery, ToolError>| match result {
            Ok(delivery) => delivery.deliver(),
            Err(error) => json!(wyrd_spec::error::WyrdError::from(&error).code()),
        };
        let mut held = std::pin::pin!(bridge.call("fs.read", json!({"path": "/held"})));
        let mut waiting = std::pin::pin!(bridge.call("fs.read", json!({"path": "/waiting"})));
        for read in [held.as_mut(), waiting.as_mut()] {
            assert!(futures_poll_once(read).is_none(), "a read finished early");
        }
        let refused = outcome(bridge.call("fs.read", json!({"path": "/refused"})).await);
        self.gate.add_permits(1);
        let held = outcome(held.await);
        let waiting = outcome(waiting.await);
        Ok(json!({"outcomes": [held, waiting, refused], "halted": bridge.halted()}))
    }
}

/// Poll `future` once, returning its output when it is ready.
fn futures_poll_once<F: Future>(future: std::pin::Pin<&mut F>) -> Option<F::Output> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.poll(&mut context) {
        std::task::Poll::Ready(output) => Some(output),
        std::task::Poll::Pending => None,
    }
}

/// A toolset binding [`HaltScript`] and a [`GatedRead`] with three
/// requests.
struct HaltToolset {
    /// Paths the read invoked.
    invoked: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentToolset for HaltToolset {
    /// Open a session running [`HaltScript`] with a gated read.
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
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(GatedRead {
                invoked: Arc::clone(&self.invoked),
                gate: Arc::clone(&gate),
            }))
            .expect("read registers");
        registry
            .register(Arc::new(CodeFace))
            .expect("code registers");
        Ok(ToolSession::new(
            registry,
            ToolLimits {
                max_calls: 3,
                ..limits()
            },
            Box::new(()),
        )
        .with_code(Arc::new(HaltScript { gate })))
    }
}

/// At concurrency cap 1 with three requests, `code.run` spends the first,
/// a held read the second, and a read waiting for the permit the third;
/// the program's fourth request stops the session before the held read
/// finishes in that same poll. The waiting read is never invoked, though
/// the permit frees before the Agent executor regains control, and stays
/// recorded in admission order as cancelled, with no digest or bytes; the
/// step fails with the budget code and no model call follows.
///
/// # Panics
/// Panics when the waiting read runs, its record is lost or reports a
/// value, the spent count passes three, or the model is called again.
#[tokio::test(flavor = "multi_thread")]
async fn admitted_waiters_survive_halt_without_dispatch() {
    let invoked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(code_call());
    mock.push_response(openai_text_response("unreachable"));
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock.clone()));
    let executor = AgentExecutor::new(
        Arc::new(registry),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_toolset(Arc::new(HaltToolset {
        invoked: Arc::clone(&invoked),
    }));
    let mut call = agent(text_prompt(), &["fs.read", "code.run"]);
    if let InlineableRef::Inline(spec) = &mut call.agent {
        spec.run_config.tool_concurrency_cap = Some(1);
    }
    let calls = ToolCallsOwner::default();
    let error = step_recording(&executor, &call, input("document", json!("x")), &calls)
        .await
        .expect_err("the spent allowance stops the step");
    assert!(
        matches!(&error, BlockError::Terminal { reason }
            if reason.starts_with("WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED: ")),
        "{error}"
    );
    assert_eq!(
        *invoked.lock().expect("invoked"),
        ["/held"],
        "nothing was invoked after the stop"
    );
    assert_eq!(mock.remaining(), 1, "no model call followed the stop");
    let records = calls.settled();
    let shape: Vec<_> = records
        .iter()
        .map(|record| (record.tool, record.arguments["path"].clone()))
        .collect();
    assert_eq!(
        shape,
        [
            (BuiltinTool::CodeRun, Value::Null),
            (BuiltinTool::FsRead, json!("/held")),
            (BuiltinTool::FsRead, json!("/waiting")),
        ],
        "admitted calls in start order; the refused request has no record"
    );
    let waiting = &records[2];
    assert_eq!(waiting.recipient, ToolRecipient::Code);
    assert_eq!((waiting.output_sha256, waiting.returned_bytes), (None, 0));
    assert!(
        waiting
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("cancelled")),
        "{waiting:?}"
    );
}

/// A `bifrost.query` double failing with `error`'s structured catalog
/// problem, exactly as the hosted query tool projects an Oracle refusal.
struct RefusedQuery {
    /// The Oracle error every call fails with.
    error: wyrd_spec::vala::error::BifrostError,
}

#[async_trait]
impl AgentTool for RefusedQuery {
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

    /// Fail with the structured problem of the configured Oracle error.
    ///
    /// # Errors
    /// Always that problem.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Err(vala_verify::blocks::tool_error(
            &wyrd_spec::error::WyrdError::from(self.error.clone()),
        ))
    }
}

/// A `code.run` program that catches every failure of a nested query and
/// then reads a file, returning what each call gave it.
struct CatchAll;

#[async_trait]
impl vala_verify::blocks::CodeTool for CatchAll {
    /// Run the fixed program through `bridge`.
    ///
    /// # Errors
    /// Never fails; the query's failure is caught.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let outcome = |result: Result<vala_verify::blocks::Delivery, ToolError>| match result {
            Ok(delivery) => delivery.deliver(),
            Err(error) => json!(wyrd_spec::error::WyrdError::from(&error).code()),
        };
        let query = outcome(
            bridge
                .call("bifrost.query", json!({"sql": "SELECT 1"}))
                .await,
        );
        let read = outcome(bridge.call("fs.read", json!({"path": "/after"})).await);
        Ok(json!({"query": query, "read": read}))
    }
}

/// A toolset of a [`RefusedQuery`], a recording [`GatedRead`], and
/// [`CatchAll`] as `code.run`.
struct RefusedToolset {
    /// The Oracle error of every query.
    error: wyrd_spec::vala::error::BifrostError,
    /// Paths the read invoked.
    invoked: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentToolset for RefusedToolset {
    /// Open a session over the refused query, the read, and [`CatchAll`].
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses a tool.
    async fn open(
        &self,
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        _deadline: Instant,
        _tools: &[BuiltinTool],
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(RefusedQuery {
                error: self.error.clone(),
            }))
            .expect("query registers");
        registry
            .register(Arc::new(GatedRead {
                invoked: Arc::clone(&self.invoked),
                gate: Arc::new(tokio::sync::Semaphore::new(0)),
            }))
            .expect("read registers");
        registry
            .register(Arc::new(CodeFace))
            .expect("code registers");
        Ok(ToolSession::new(registry, limits(), Box::new(())).with_code(Arc::new(CatchAll)))
    }
}

/// An Oracle queue refusal is the process's capacity, not the program's
/// failure: whether the model calls `bifrost.query` directly or a program
/// calls it and catches every error, the invocation stops busy, nothing
/// the program asks for afterwards runs, no model call follows, and the
/// engine stops the execution busy so its host defers it unjudged. An
/// admitted query that ran out of its own resources stays an ordinary
/// failure the program catches and continues past, and a fresh invocation
/// after the refusal answers normally.
///
/// # Panics
/// Panics when a queue refusal is caught or judged, work follows it, or the
/// ordinary failure stops the step.
#[tokio::test(flavor = "multi_thread")]
async fn oracle_queue_refusal_is_busy_at_every_origin() {
    use wyrd_spec::vala::error::BifrostError;
    let run = |error: BifrostError, responses: Vec<ProviderResponse>| {
        let invoked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock = MockProvider::new(ProviderName::OpenAi);
        for response in responses {
            mock.push_response(response);
        }
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(mock.clone()));
        let executor = AgentExecutor::new(
            Arc::new(registry),
            Arc::new(NoRegistry),
            Arc::new(NoRegistry),
        )
        .with_toolset(Arc::new(RefusedToolset {
            error,
            invoked: Arc::clone(&invoked),
        }));
        (executor, mock, invoked)
    };
    let tools = ["bifrost.query", "fs.read", "code.run"];
    for (origin, first) in [("direct", query_call()), ("nested", code_call())] {
        let (executor, mock, invoked) = run(
            BifrostError::QueryQueueFull,
            vec![first.clone(), openai_text_response("unreachable")],
        );
        let calls = ToolCallsOwner::default();
        let error = step_recording(
            &executor,
            &agent(text_prompt(), &tools),
            input("document", json!("x")),
            &calls,
        )
        .await
        .expect_err("a queue refusal stops the step");
        assert!(
            matches!(&error, BlockError::Busy { reason }
                if reason.starts_with("WYRD_VALA_429_QUERY_QUEUE_FULL: ")),
            "{origin}: {error}"
        );
        assert!(invoked.lock().expect("invoked").is_empty(), "{origin}");
        assert_eq!(mock.remaining(), 1, "{origin}: no model call followed");
        let query = calls
            .settled()
            .into_iter()
            .find(|record| record.tool == BuiltinTool::BifrostQuery)
            .expect("the refused query is recorded");
        assert!(
            query
                .error
                .as_deref()
                .is_some_and(|error| error.ends_with("query queue full")),
            "{origin}: {query:?}"
        );

        let (executor, _, _) = run(BifrostError::QueryQueueFull, vec![first]);
        let mut action = bound(Action::Agent(agent(text_prompt(), &tools)));
        action["inputs"] = json!({"document": "inputs.document"});
        let refused = run_engine(
            executor,
            json!({
                "inputs": {"document": {"type": "json"}},
                "steps": [{"id": "draft", "action": action, "retries": 1}],
                "requirements": [{"id": "drafted", "statement": "Drafted", "check": {
                    "kind": "assert", "path": "steps.draft.output", "operator": "is_not_null"
                }}]
            }),
        )
        .await;
        assert!(
            matches!(&refused, Err(vala_verify::blocks::EngineError::Busy(reason))
                if reason.starts_with("WYRD_VALA_429_QUERY_QUEUE_FULL: ")),
            "{origin}: {refused:?}"
        );
    }

    let (executor, mock, invoked) = run(
        BifrostError::QueryResourcesExhausted,
        vec![code_call(), openai_text_response("done")],
    );
    let output = step(
        &executor,
        &agent(text_prompt(), &tools),
        input("document", json!("x")),
    )
    .await
    .expect("an exhausted query is the program's to handle");
    assert_eq!(output.output, json!("done"));
    assert_eq!(*invoked.lock().expect("invoked"), ["/after"]);
    assert_eq!(mock.remaining(), 0);
}

/// A `fs.extract` double whose blocking work takes the caller's permit with
/// [`vala_verify::blocks::hold_permit`], reports that it started, runs
/// until released, and reports that it finished.
struct HeldWork {
    /// Gains one permit when the work starts.
    started: Arc<tokio::sync::Semaphore>,
    /// Gains one permit when the work ends.
    finished: Arc<tokio::sync::Semaphore>,
    /// Ends the work when sent to or dropped.
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    /// Whether the work received the caller's permit.
    held: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl AgentTool for HeldWork {
    /// The built-in wire name.
    fn name(&self) -> &str {
        BuiltinTool::FsExtract.as_str()
    }

    /// A fixed description.
    fn description(&self) -> &'static str {
        "extract a workspace document"
    }

    /// Any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Any object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Run the held work on the blocking pool, keeping the caller's permit
    /// until it ends.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the release lock is poisoned or the blocking task
    /// panics.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let permit = vala_verify::blocks::hold_permit();
        self.held
            .store(permit.is_some(), std::sync::atomic::Ordering::SeqCst);
        let started = Arc::clone(&self.started);
        let finished = Arc::clone(&self.finished);
        let release = Arc::clone(&self.release);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            started.add_permits(1);
            release.lock().expect("release").recv().ok();
            finished.add_permits(1);
        })
        .await
        .expect("the work runs");
        Ok(json!({}))
    }
}

/// A `code.run` program that drops an extraction once its blocking work
/// has started, checks that a following read is not dispatched, then
/// releases the work and awaits the read.
struct InterruptScript {
    /// Gains one permit when the extraction's work starts.
    started: Arc<tokio::sync::Semaphore>,
    /// Releases the extraction's work.
    release: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
    /// Paths the read invoked.
    invoked: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl vala_verify::blocks::CodeTool for InterruptScript {
    /// Run the fixed program through `bridge`.
    ///
    /// # Errors
    /// Returns the read's error.
    ///
    /// # Panics
    /// Panics when the extraction ends while held, or a lock or channel
    /// fails.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let mut extract = Box::pin(bridge.call("fs.extract", json!({})));
        tokio::select! {
            ended = &mut extract => panic!("the extraction ended while held: {:?}", ended.map(vala_verify::blocks::Delivery::deliver)),
            started = self.started.acquire() => started.expect("started").forget(),
        }
        drop(extract);
        let mut read = Box::pin(bridge.call("fs.read", json!({"path": "/after"})));
        let polled = futures_poll_once(read.as_mut());
        let pending = polled.is_none() && self.invoked.lock().expect("invoked").is_empty();
        self.release
            .lock()
            .expect("release")
            .send(())
            .expect("the work waits");
        let read = match polled {
            Some(read) => read,
            None => read.await,
        }?
        .deliver();
        Ok(json!({"pending": pending, "read": read}))
    }
}

/// A toolset binding [`InterruptScript`] over [`HeldWork`] and a
/// [`GatedRead`].
struct InterruptToolset {
    /// Paths the read invoked.
    invoked: Arc<std::sync::Mutex<Vec<String>>>,
    /// Whether the held work received the caller's permit.
    held: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl AgentToolset for InterruptToolset {
    /// Open a session running [`InterruptScript`].
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses a tool.
    async fn open(
        &self,
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        _deadline: Instant,
        _tools: &[BuiltinTool],
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let started = Arc::new(tokio::sync::Semaphore::new(0));
        let (release, gate) = std::sync::mpsc::channel();
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(HeldWork {
                started: Arc::clone(&started),
                finished: Arc::new(tokio::sync::Semaphore::new(0)),
                release: Arc::new(std::sync::Mutex::new(gate)),
                held: Arc::clone(&self.held),
            }))
            .expect("extract registers");
        registry
            .register(Arc::new(GatedRead {
                invoked: Arc::clone(&self.invoked),
                gate: Arc::new(tokio::sync::Semaphore::new(0)),
            }))
            .expect("read registers");
        registry
            .register(Arc::new(CodeFace))
            .expect("code registers");
        Ok(
            ToolSession::new(registry, limits(), Box::new(())).with_code(Arc::new(
                InterruptScript {
                    started,
                    release: std::sync::Mutex::new(release),
                    invoked: Arc::clone(&self.invoked),
                },
            )),
        )
    }
}

/// At concurrency cap 1, blocking work keeps its call's permit after the
/// call is dropped: a following read is not dispatched until the work is
/// released, then runs. The dropped call is recorded as cancelled with no
/// digest.
///
/// # Panics
/// Panics when the permit frees before the work ends, the work never
/// receives it, or a record is lost or misreported.
#[tokio::test(flavor = "multi_thread")]
async fn interrupted_blocking_work_keeps_its_permit() {
    let invoked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let held = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executor = executor(vec![code_call(), openai_text_response("done")]).with_toolset(
        Arc::new(InterruptToolset {
            invoked: Arc::clone(&invoked),
            held: Arc::clone(&held),
        }),
    );
    let mut call = agent(text_prompt(), &["fs.extract", "fs.read", "code.run"]);
    if let InlineableRef::Inline(spec) = &mut call.agent {
        spec.run_config.tool_concurrency_cap = Some(1);
    }
    let calls = ToolCallsOwner::default();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        step_recording(&executor, &call, input("document", json!("x")), &calls),
    )
    .await
    .expect("the step ends")
    .expect("the step runs");
    assert_eq!(output.output, json!("done"));
    assert!(
        held.load(std::sync::atomic::Ordering::SeqCst),
        "the work holds the permit"
    );
    let records = calls.settled();
    assert_eq!(
        records[0].output_sha256,
        Some(Sha256Hex::digest(
            json!({"pending": true, "read": {"text": "/after"}})
                .to_string()
                .as_bytes()
        )),
        "the read waited for the work's permit: {records:?}"
    );
    let shape: Vec<_> = records.iter().map(|record| record.tool).collect();
    assert_eq!(
        shape,
        [
            BuiltinTool::CodeRun,
            BuiltinTool::FsExtract,
            BuiltinTool::FsRead
        ]
    );
    assert_eq!(
        (records[1].output_sha256, records[1].returned_bytes),
        (None, 0)
    );
    assert!(
        records[1]
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("cancelled")),
        "{records:?}"
    );
    assert_eq!(*invoked.lock().expect("invoked"), ["/after"]);
}

/// An OpenAI answer calling `fs_extract` once.
///
/// # Panics
/// Panics when [`read_call`] carries no tool call.
fn extract_call() -> ProviderResponse {
    let ProviderResponse::OpenAiChatCompletion(mut response) = read_call() else {
        unreachable!("read_call is an OpenAI completion");
    };
    let call = &mut response.choices[0]
        .message
        .tool_calls
        .as_mut()
        .expect("calls")[0];
    "fs_extract".clone_into(&mut call.function.name);
    call.function.arguments = json!({"path": "/inputs/report.txt"}).to_string();
    ProviderResponse::OpenAiChatCompletion(response)
}

/// How a [`SettlingToolset`]'s held extraction starts and how the Agent ends
/// while it runs.
#[derive(Debug, Clone)]
enum Held {
    /// The model calls `fs_extract` directly and waits on it.
    Direct,
    /// A [`DropHeld`] program returns once the extraction starts.
    Returned,
    /// A [`DropHeld`] program stops the session with this host error once
    /// the extraction starts.
    Stopped(BlockError),
}

/// A `code.run` program that starts a nested [`HeldWork`] extraction, drops
/// it once its work has started, and returns or stops as its [`Held`] says.
struct DropHeld {
    /// Has a permit once the extraction's work has started; only peeked.
    started: Arc<tokio::sync::Semaphore>,
    /// Gains one permit as the program ends.
    ended: Arc<tokio::sync::Semaphore>,
    /// How the program ends.
    held: Held,
}

#[async_trait]
impl vala_verify::blocks::CodeTool for DropHeld {
    /// Run the fixed program through `bridge`.
    ///
    /// # Errors
    /// Returns [`ToolError::Invocation`] caused by the [`Held::Stopped`]
    /// error.
    ///
    /// # Panics
    /// Panics when the extraction ends while held.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let mut extract = Box::pin(bridge.call("fs.extract", json!({})));
        tokio::select! {
            ended = &mut extract => panic!("the extraction ended while held: {:?}", ended.map(vala_verify::blocks::Delivery::deliver)),
            started = self.started.acquire() => drop(started.expect("started")),
        }
        drop(extract);
        self.ended.add_permits(1);
        match &self.held {
            Held::Stopped(error) => Err(ToolError::Invocation {
                detail: error.to_string(),
                cause: Some(Box::new(error.clone())),
            }),
            Held::Direct | Held::Returned => Ok(json!({})),
        }
    }
}

/// A toolset whose first session runs one [`HeldWork`] extraction, started
/// as its [`Held`] says, and settles only once that work ends; later
/// sessions start no work.
struct SettlingToolset {
    /// How the held extraction starts and the Agent ends.
    held: Held,
    /// Gains one permit when the held work starts; only peeked.
    started: Arc<tokio::sync::Semaphore>,
    /// Gains one permit when the held work ends.
    finished: Arc<tokio::sync::Semaphore>,
    /// Gains one permit when a settlement is first polled.
    awaited: Arc<tokio::sync::Semaphore>,
    /// Gains one permit when a [`DropHeld`] program ends.
    ended: Arc<tokio::sync::Semaphore>,
    /// Releases the held work.
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    /// The first session's deadline.
    deadline: std::sync::Mutex<Option<Instant>>,
    /// Resolves once the first session's settlement completes, or fails
    /// once it is dropped before completing.
    settled: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    /// For each session after the first, whether the held work had ended
    /// when it opened.
    reopened: Arc<std::sync::Mutex<Vec<bool>>>,
    /// Sessions opened.
    opens: std::sync::atomic::AtomicUsize,
}

impl SettlingToolset {
    /// A toolset holding its first extraction until `release` sends.
    fn new(held: Held, release: std::sync::mpsc::Receiver<()>) -> Self {
        Self {
            held,
            started: Arc::new(tokio::sync::Semaphore::new(0)),
            finished: Arc::new(tokio::sync::Semaphore::new(0)),
            awaited: Arc::new(tokio::sync::Semaphore::new(0)),
            ended: Arc::new(tokio::sync::Semaphore::new(0)),
            release: Arc::new(std::sync::Mutex::new(release)),
            deadline: std::sync::Mutex::new(None),
            settled: std::sync::Mutex::new(None),
            reopened: Arc::new(std::sync::Mutex::new(Vec::new())),
            opens: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The first session's deadline.
    ///
    /// # Panics
    /// Panics before the first session opens or when the lock is poisoned.
    fn deadline(&self) -> Instant {
        self.deadline
            .lock()
            .expect("deadline")
            .expect("the first session opened")
    }

    /// Take the first session's settlement signal.
    ///
    /// # Panics
    /// Panics before the first session opens, once taken, or when the lock
    /// is poisoned.
    fn settled(&self) -> tokio::sync::oneshot::Receiver<()> {
        self.settled
            .lock()
            .expect("settled")
            .take()
            .expect("the first session opened")
    }
}

#[async_trait]
impl AgentToolset for SettlingToolset {
    /// Open a session over the held extraction, recording whether earlier
    /// work had ended.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses a tool or a lock is poisoned.
    async fn open(
        &self,
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        deadline: Instant,
        _tools: &[BuiltinTool],
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let first = self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        let done = if first {
            *self.deadline.lock().expect("deadline") = Some(deadline);
            let (done, settled) = tokio::sync::oneshot::channel();
            *self.settled.lock().expect("settled") = Some(settled);
            Some(done)
        } else {
            self.reopened
                .lock()
                .expect("reopened")
                .push(self.finished.available_permits() > 0);
            None
        };
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(HeldWork {
                started: Arc::clone(&self.started),
                finished: Arc::clone(&self.finished),
                release: Arc::clone(&self.release),
                held: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }))
            .expect("extract registers");
        if !matches!(self.held, Held::Direct) {
            registry
                .register(Arc::new(CodeFace))
                .expect("code registers");
        }
        let awaited = Arc::clone(&self.awaited);
        let finished = Arc::clone(&self.finished);
        let session =
            ToolSession::new(registry, limits(), Box::new(())).with_settlement(async move {
                awaited.add_permits(1);
                if let Some(done) = done {
                    drop(finished.acquire().await.expect("finished"));
                    done.send(()).ok();
                }
            });
        Ok(match self.held {
            Held::Direct => session,
            Held::Returned | Held::Stopped(_) => session.with_code(Arc::new(DropHeld {
                started: Arc::clone(&self.started),
                ended: Arc::clone(&self.ended),
                held: self.held.clone(),
            })),
        })
    }
}

/// An executor over the `responses` script whose built-ins are `toolset`.
fn settling_executor(
    responses: Vec<ProviderResponse>,
    toolset: &Arc<SettlingToolset>,
) -> (AgentExecutor, MockProvider) {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for response in responses {
        mock.push_response(response);
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock.clone()));
    let executor = AgentExecutor::new(
        Arc::new(providers),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_toolset(Arc::clone(toolset) as Arc<dyn AgentToolset>);
    (executor, mock)
}

/// The one cancelled `fs.extract` record among `records`.
///
/// # Panics
/// Panics when there is not exactly one extraction, or it returned a value
/// or is not recorded as cancelled.
fn cancelled_extraction(
    label: &str,
    records: &[wyrd_spec::verification::execution::ToolCallRecord],
) {
    let extracts: Vec<_> = records
        .iter()
        .filter(|record| record.tool == BuiltinTool::FsExtract)
        .collect();
    let [extract] = extracts.as_slice() else {
        panic!("{label}: one extraction: {records:?}");
    };
    assert_eq!(
        (extract.output_sha256, extract.returned_bytes),
        (None, 0),
        "{label}"
    );
    assert!(
        extract
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("cancelled")),
        "{label}: {extract:?}"
    );
}

/// An attempt's host work settles before the engine retries the step or
/// produces its result, whether the step's `timeout_seconds` ends the
/// attempt while the Agent is running or after a host stop ended it early.
///
/// Each case starts an extraction held past the one-second attempt timeout.
/// Timed out while the model waits on it, the engine must await the
/// attempt's settlement while the work is held; once released, with one
/// retry a new session opens only after the old work ended and the step
/// succeeds, and with none the step fails as timed out with no further
/// model call. Stopped earlier by a `code.run` host error with the work
/// still running, the settlement must survive the attempt timeout instant
/// and the engine must not advance; once released, the stop keeps its own
/// classification, a failed step or a busy execution, and is not retried as
/// a timeout. Every result keeps the interrupted call as cancelled with no
/// digest.
///
/// # Panics
/// Panics when the engine retries or produces a result while the work is
/// held, the settlement is dropped or ends early, a new session overlaps
/// the old work, or the outcome or records differ.
#[tokio::test(flavor = "multi_thread")]
async fn timed_out_attempt_settles_before_retry_or_result() {
    use wyrd_spec::verification::execution::StepStatus;

    let limit = BlockError::Terminal {
        reason: "code.run exceeded its operation deadline".to_owned(),
    };
    let busy = BlockError::Busy {
        reason: "no worker".to_owned(),
    };
    for (retries, held) in [
        (1, Held::Direct),
        (0, Held::Direct),
        (1, Held::Stopped(limit)),
        (1, Held::Stopped(busy)),
    ] {
        let label = format!("retries {retries}, {held:?}");
        let (release, gate) = std::sync::mpsc::channel();
        let toolset = Arc::new(SettlingToolset::new(held.clone(), gate));
        let mut responses = vec![match held {
            Held::Direct => extract_call(),
            Held::Returned | Held::Stopped(_) => code_call(),
        }];
        if retries > 0 && matches!(held, Held::Direct) {
            responses.push(openai_text_response("done"));
        }
        let (executor, mock) = settling_executor(responses, &toolset);
        let tools: &[&str] = match held {
            Held::Direct => &["fs.extract"],
            Held::Returned | Held::Stopped(_) => &["fs.extract", "code.run"],
        };
        let mut action = bound(Action::Agent(agent(text_prompt(), tools)));
        action["inputs"] = json!({"document": "inputs.document"});
        let mut engine = tokio::spawn(run_engine(
            executor,
            json!({
                "inputs": {"document": {"type": "json"}},
                "steps": [{"id": "draft", "action": action, "retries": retries, "timeout_seconds": 1}],
                "requirements": [{"id": "drafted", "statement": "Drafted", "check": {
                    "kind": "assert", "path": "steps.draft.output", "operator": "is_not_null"
                }}]
            }),
        ));
        tokio::select! {
            ended = &mut engine => panic!("{label}: ended before the work started: {ended:?}"),
            started = toolset.started.acquire() => drop(started.expect("the work starts")),
        }
        tokio::select! {
            ended = &mut engine => panic!("{label}: advanced while the work was held: {ended:?}"),
            awaited = toolset.awaited.acquire() => awaited.expect("awaited").forget(),
        }
        if let Held::Stopped(_) = held {
            let deadline = toolset.deadline();
            assert_eq!(
                toolset.ended.available_permits(),
                1,
                "{label}: the Agent stopped first"
            );
            assert!(
                Instant::now() < deadline,
                "{label}: stopped before the attempt timeout"
            );
            let mut settled = toolset.settled();
            tokio::select! {
                ended = &mut engine => panic!("{label}: advanced while the work was held: {ended:?}"),
                settled = &mut settled => panic!(
                    "{label}: the settlement {} while the work was held",
                    if settled.is_ok() { "ended" } else { "was dropped" }
                ),
                () = tokio::time::sleep_until(deadline + Duration::from_secs(1)) => {}
            }
        }
        assert_eq!(
            toolset.opens.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "{label}: no new session before settlement"
        );
        release.send(()).expect("the work waits for release");
        let outcome = engine.await.expect("the engine task runs");
        assert_eq!(mock.remaining(), 0, "{label}: the script is spent");
        if let Held::Stopped(BlockError::Busy { reason }) = &held {
            assert!(
                matches!(&outcome, Err(vala_verify::blocks::EngineError::Busy(busy)) if busy == reason),
                "{label}: {outcome:?}"
            );
            assert_eq!(toolset.opens.load(std::sync::atomic::Ordering::SeqCst), 1);
            continue;
        }
        let outcome = outcome.expect("the execution settles");
        let step = &outcome.steps[0];
        match &held {
            Held::Direct if retries > 0 => {
                assert_eq!(step.status, StepStatus::Succeeded, "{label}: {step:?}");
                assert_eq!(step.attempts, 2, "{label}: {step:?}");
                assert_eq!(
                    *toolset.reopened.lock().expect("reopened"),
                    [true],
                    "{label}: the retry opened only after the old work ended"
                );
            }
            Held::Direct => {
                assert_eq!(step.status, StepStatus::Failed, "{label}: {step:?}");
                assert!(
                    step.reason
                        .as_deref()
                        .is_some_and(|reason| reason.contains("timed out")),
                    "{label}: {step:?}"
                );
                assert_eq!(toolset.opens.load(std::sync::atomic::Ordering::SeqCst), 1);
            }
            Held::Returned | Held::Stopped(_) => {
                assert_eq!(step.status, StepStatus::Failed, "{label}: {step:?}");
                assert_eq!(step.attempts, 1, "{label}: a stop is not retried: {step:?}");
                assert_eq!(
                    step.reason.as_deref(),
                    Some("code.run exceeded its operation deadline"),
                    "{label}: the stop keeps its classification"
                );
                assert_eq!(toolset.opens.load(std::sync::atomic::Ordering::SeqCst), 1);
            }
        }
        cancelled_extraction(&label, &step.tool_calls);
    }
}

/// An Agent check's host work settles before the engine judges the
/// requirement, whether the check returns an answer or a host stop ends it.
///
/// A `code.run` program drops a held extraction once it starts; the check
/// then answers a failed judgment, or its session stops with a terminal host
/// error. Either way the engine must await the check's settlement while
/// the work is held and must not produce the requirement until release;
/// then the failure or the inconclusive stop is judged with the interrupted
/// call recorded as cancelled with no digest.
///
/// # Panics
/// Panics when the engine produces a result while the work is held, the
/// check returns only after the work ends, or the judgment or records
/// differ.
#[tokio::test(flavor = "multi_thread")]
async fn agent_check_settles_before_result() {
    use wyrd_spec::verification::execution::RequirementOutcome;

    let limit = BlockError::Terminal {
        reason: "code.run exceeded its operation deadline".to_owned(),
    };
    for held in [Held::Returned, Held::Stopped(limit)] {
        let label = format!("{held:?}");
        let (release, gate) = std::sync::mpsc::channel();
        let toolset = Arc::new(SettlingToolset::new(held.clone(), gate));
        let mut responses = vec![code_call()];
        if let Held::Returned = held {
            responses.push(openai_text_response(
                &json!({"outcome": "failed", "reason": "judged", "citations": []}).to_string(),
            ));
        }
        let (executor, mock) = settling_executor(responses, &toolset);
        let mut judge = named_judge(json!("verdict"));
        if let InlineableRef::Inline(spec) = &mut judge.agent {
            spec.tool_names = vec!["fs.extract".to_owned(), "code.run".to_owned()];
        }
        let mut engine = tokio::spawn(run_engine(
            executor,
            json!({
                "inputs": {"document": {"type": "json"}},
                "requirements": [{"id": "grounded", "statement": "Grounded", "check": bound(Check::Agent(judge))}]
            }),
        ));
        tokio::select! {
            ended = &mut engine => panic!("{label}: ended before the work started: {ended:?}"),
            started = toolset.started.acquire() => drop(started.expect("the work starts")),
        }
        tokio::select! {
            ended = &mut engine => panic!("{label}: judged while the work was held: {ended:?}"),
            awaited = toolset.awaited.acquire() => awaited.expect("awaited").forget(),
        }
        assert_eq!(
            toolset.ended.available_permits(),
            1,
            "{label}: the check returned while the work was held"
        );
        assert!(
            !engine.is_finished(),
            "{label}: judged while the work was held"
        );
        release.send(()).expect("the work waits for release");
        let outcome = engine
            .await
            .expect("the engine task runs")
            .expect("the execution settles");
        assert_eq!(mock.remaining(), 0, "{label}: the script is spent");
        let requirement = &outcome.requirements[0];
        if let Held::Stopped(BlockError::Terminal { reason }) = &held {
            assert_eq!(
                requirement.result.outcome,
                RequirementOutcome::Inconclusive,
                "{label}: {requirement:?}"
            );
            assert_eq!(&requirement.result.reason, reason, "{label}");
        } else {
            assert_eq!(
                requirement.result.outcome,
                RequirementOutcome::Failed,
                "{label}: {requirement:?}"
            );
            assert_eq!(requirement.result.reason, "judged", "{label}");
        }
        cancelled_extraction(&label, &requirement.tool_calls);
    }
}

/// A writable built-in double recording the provider call ID of each
/// invocation, `None` for a call outside any provider call.
struct WriteFace {
    /// The built-in it stands for.
    tool: BuiltinTool,
    /// Every invocation's call ID, in order.
    invoked: Arc<std::sync::Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl AgentTool for WriteFace {
    /// The built-in wire name.
    fn name(&self) -> &str {
        self.tool.as_str()
    }

    /// A fixed description.
    fn description(&self) -> &'static str {
        "write"
    }

    /// Any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Any object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Record a call without a provider call ID.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the record lock is poisoned.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        self.invoked.lock().expect("invoked").push(None);
        Ok(json!({}))
    }

    /// Record provider call `call_id`.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the record lock is poisoned.
    async fn invoke_call(&self, call_id: &str, _args: Value) -> Result<Value, ToolError> {
        self.invoked
            .lock()
            .expect("invoked")
            .push(Some(call_id.to_owned()));
        Ok(json!({}))
    }
}

/// A `code.run` program requesting `fs.write` and `publish` and returning
/// each outcome's stable code or value.
struct WritingScript;

#[async_trait]
impl vala_verify::blocks::CodeTool for WritingScript {
    /// Request both writable tools through `bridge`.
    ///
    /// # Errors
    /// Never fails; refusals are part of the result.
    async fn run(
        &self,
        _args: Value,
        bridge: vala_verify::blocks::ToolBridge,
    ) -> Result<Value, ToolError> {
        let mut outcomes = Vec::new();
        for name in ["fs.write", "publish"] {
            outcomes.push(match bridge.call(name, json!({"path": "/work/a"})).await {
                Ok(delivery) => delivery.deliver(),
                Err(error) => json!(wyrd_spec::error::WyrdError::from(&error).code()),
            });
        }
        Ok(json!(outcomes))
    }
}

/// A toolset declaring `fs.write`, `publish`, and `code.run` through
/// [`WritingScript`], recording the writable calls in `invoked`.
struct WritingToolset {
    /// Every writable invocation's call ID.
    invoked: Arc<std::sync::Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl AgentToolset for WritingToolset {
    /// Open a session over the writable doubles and [`WritingScript`].
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when the empty registry refuses a tool.
    async fn open(
        &self,
        _id: &TaskId,
        _workspace: &BTreeMap<String, CallInput>,
        _deadline: Instant,
        _tools: &[BuiltinTool],
        _producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        let registry = ToolRegistry::new();
        for tool in [BuiltinTool::FsWrite, BuiltinTool::Publish] {
            registry
                .register(Arc::new(WriteFace {
                    tool,
                    invoked: Arc::clone(&self.invoked),
                }))
                .expect("writable tool registers");
        }
        registry
            .register(Arc::new(CodeFace))
            .expect("code registers");
        Ok(ToolSession::new(registry, limits(), Box::new(())).with_code(Arc::new(WritingScript)))
    }
}

/// The model's `publish` call reaches its tool under the provider call ID
/// it was made with, while code requesting the declared `fs.write` and
/// `publish` is refused as not in the Agent and never invokes either, so a
/// nested call can neither write nor publish.
///
/// # Panics
/// Panics when the model call loses its call ID, a nested writable call is
/// invoked or refused with another code, or the records disagree.
#[tokio::test(flavor = "multi_thread")]
async fn nested_calls_refuse_writable_tools_and_model_calls_keep_their_call_id() {
    let ProviderResponse::OpenAiChatCompletion(mut publish) = read_call() else {
        unreachable!("read_call is an OpenAI completion");
    };
    let call = &mut publish.choices[0]
        .message
        .tool_calls
        .as_mut()
        .expect("calls")[0];
    "publish".clone_into(&mut call.function.name);
    let invoked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let executor = executor(vec![
        ProviderResponse::OpenAiChatCompletion(publish),
        code_call(),
        openai_text_response("done"),
    ])
    .with_toolset(Arc::new(WritingToolset {
        invoked: Arc::clone(&invoked),
    }));
    let calls = ToolCallsOwner::default();
    let output = step_recording(
        &executor,
        &agent(text_prompt(), &["fs.write", "publish", "code.run"]),
        input("document", json!("x")),
        &calls,
    )
    .await
    .expect("the step runs");
    assert_eq!(output.output, json!("done"));
    assert_eq!(
        *invoked.lock().expect("invoked"),
        [Some("call_1".to_owned())],
        "only the model's publish ran, under its provider call ID"
    );
    let records = calls.settled();
    let shape: Vec<_> = records
        .iter()
        .map(|record| (record.tool, record.recipient))
        .collect();
    assert_eq!(
        shape,
        [
            (BuiltinTool::Publish, ToolRecipient::Model),
            (BuiltinTool::CodeRun, ToolRecipient::Model),
        ],
        "refused nested requests are not recorded"
    );
    let refused = json!([
        "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
        "WYRD_AGENT_404_TOOL_NOT_IN_AGENT"
    ]);
    assert_eq!(
        records[1].output_sha256,
        Some(Sha256Hex::digest(refused.to_string().as_bytes())),
        "code saw both writable tools refused"
    );
}

/// Evidence whose media is one PNG image, guarded by a clone of `held` so
/// a test can see whether the guard is still alive.
struct HeldMedia {
    /// Cloned into every guard the reader returns.
    held: Arc<()>,
}

#[async_trait]
impl EvidenceReader for HeldMedia {
    /// One PNG image, guarded by a clone of `held`.
    ///
    /// # Errors
    /// Never fails.
    async fn media(
        &self,
        _evidence: &InputDeclaration,
    ) -> Result<(MediaRef, Box<dyn std::any::Any + Send + Sync>), BlockError> {
        Ok((
            MediaRef::image_base64("image/png", "iVBORw0KGgo="),
            Box::new(Arc::clone(&self.held)),
        ))
    }

    /// Accept every citation.
    ///
    /// # Errors
    /// Never fails.
    async fn validate(&self, _citations: &[&Citation]) -> Result<(), BlockError> {
        Ok(())
    }
}

/// A provider that records how many holders `held` has when a request
/// arrives, then answers from its queue or, when `pend` is set, never.
#[derive(Clone)]
struct WatchingMedia {
    /// The scripted answers.
    mock: MockProvider,
    /// The reader's guard target.
    held: Arc<()>,
    /// Holders of `held` seen at each request.
    seen: Arc<std::sync::Mutex<Vec<usize>>>,
    /// Never answer.
    pend: bool,
}

#[async_trait]
impl Provider for WatchingMedia {
    /// Record the holders of `held`, then answer or pend.
    ///
    /// # Errors
    /// Returns the mock's error for an empty queue.
    ///
    /// # Panics
    /// Panics when the log's lock is poisoned.
    async fn send(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.seen
            .lock()
            .expect("unpoisoned")
            .push(Arc::strong_count(&self.held));
        if self.pend {
            std::future::pending::<()>().await;
        }
        self.mock.send(request).await
    }

    /// Streams are not used.
    ///
    /// # Errors
    /// Always the mock's empty-stream error.
    async fn stream(&self, request: &ProviderRequest) -> Result<ProviderStream, ProviderError> {
        self.mock.stream(request).await
    }

    /// The mocked provider name.
    fn name(&self) -> ProviderName {
        self.mock.name()
    }
}

/// The media an Agent binds stays held by its evidence reader's guard
/// while the provider request carries it, and the guard is released once
/// the call completes or is cancelled mid-request.
///
/// # Panics
/// Panics when the provider sees the guard released, or the guard outlives
/// the finished or cancelled call.
#[tokio::test]
async fn bound_media_stays_held_until_the_agent_call_ends() {
    for pend in [false, true] {
        let held = Arc::new(());
        let provider = WatchingMedia {
            mock: MockProvider::new(ProviderName::OpenAi),
            held: Arc::clone(&held),
            seen: Arc::default(),
            pend,
        };
        provider.mock.push_response(openai_text_response("a cat"));
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(provider.clone()));
        let executor = AgentExecutor::new(
            Arc::new(providers),
            Arc::new(NoRegistry),
            Arc::new(NoRegistry),
        )
        .with_evidence(Arc::new(HeldMedia {
            held: Arc::clone(&held),
        }));
        let prompt = openai_chat(
            "gpt-test",
            OpenAiChatOptions {
                messages: vec!["Describe ${media:shot}".to_owned()],
                ..OpenAiChatOptions::default()
            },
        )
        .expect("static prompt is valid");
        let inputs = BTreeMap::from([(
            "shot".to_owned(),
            CallInput {
                value: CallValue::Evidence(InputDeclaration::File {
                    sha256: Sha256Hex::digest(b"png"),
                    size_bytes: 3,
                    format: skald_spec::MediaFormat::Png,
                    filename: None,
                }),
                sources: BTreeSet::new(),
            },
        )]);

        let call = agent(prompt, &[]);
        let idle = Arc::strong_count(&held);
        let run = step(&executor, &call, inputs);
        if pend {
            let cancelled = tokio::time::timeout(Duration::from_millis(200), run).await;
            assert!(cancelled.is_err(), "the pending provider never answers");
        } else {
            run.await.expect("the media step runs");
        }

        assert_eq!(
            *provider.seen.lock().expect("unpoisoned"),
            [idle + 1],
            "the provider request sees the guard held (pend: {pend})"
        );
        assert_eq!(
            Arc::strong_count(&held),
            idle,
            "the guard is released once the call ends (pend: {pend})"
        );
    }
}

/// A byte budget shared by a media reader and its sibling promises.
#[derive(Default)]
struct Memory {
    /// Most bytes it promises at once.
    capacity: u64,
    /// Bytes promised now.
    used: std::sync::atomic::AtomicU64,
}

impl Memory {
    /// Promise `bytes`, or `None` when they would pass the capacity.
    fn charge(self: &Arc<Self>, bytes: u64) -> Option<Promise> {
        self.used
            .try_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| {
                    used.checked_add(bytes)
                        .filter(|total| *total <= self.capacity)
                },
            )
            .ok()?;
        Some(Promise(Arc::clone(self), bytes))
    }

    /// Bytes promised now.
    fn used(&self) -> u64 {
        self.used.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Bytes promised from a [`Memory`] until dropped.
struct Promise(
    /// The budget the bytes were promised from.
    Arc<Memory>,
    /// The promised bytes.
    u64,
);

impl Drop for Promise {
    /// Returns the promised bytes.
    fn drop(&mut self) {
        self.0
            .used
            .fetch_sub(self.1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Evidence whose media is one `raw`-byte PNG, charged like the host's
/// reader: the returned guard holds the encoding, charged from `memory`
/// before the encoding exists.
struct ChargedMedia {
    /// The budget the media is charged to.
    memory: Arc<Memory>,
    /// Raw media bytes.
    raw: usize,
    /// How many times the media was read.
    reads: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl EvidenceReader for ChargedMedia {
    /// The PNG with its charge, or `Busy` when `memory` cannot hold it.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when the budget refuses the encoding.
    async fn media(
        &self,
        _evidence: &InputDeclaration,
    ) -> Result<(MediaRef, Box<dyn std::any::Any + Send + Sync>), BlockError> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let promise = self
            .memory
            .charge((self.raw.div_ceil(3) * 4) as u64)
            .ok_or_else(|| BlockError::Busy {
                reason: "no memory for the media".to_owned(),
            })?;
        let data = "A".repeat(self.raw.div_ceil(3) * 4);
        Ok((
            MediaRef::image_base64("image/png", &data),
            Box::new(promise),
        ))
    }

    /// Accept every citation.
    ///
    /// # Errors
    /// Never fails.
    async fn validate(&self, _citations: &[&Citation]) -> Result<(), BlockError> {
        Ok(())
    }
}

/// A provider that, while a request is pending, records the bytes
/// promised from `memory` and whether a sibling could still be promised
/// `sibling` bytes, then answers from its queue or, with `pend`, never.
#[derive(Clone)]
struct MeteredMedia {
    /// The scripted answers.
    mock: MockProvider,
    /// The shared budget.
    memory: Arc<Memory>,
    /// The sibling promise tried during the request.
    sibling: u64,
    /// `(promised, sibling admitted)` at each request.
    seen: Arc<std::sync::Mutex<Vec<(u64, bool)>>>,
    /// Never answer.
    pend: bool,
}

#[async_trait]
impl Provider for MeteredMedia {
    /// Record the promised bytes and the sibling's admission, then answer
    /// or pend.
    ///
    /// # Errors
    /// Returns the mock's error for an empty queue.
    ///
    /// # Panics
    /// Panics when the log's lock is poisoned.
    async fn send(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let sibling = self.memory.charge(self.sibling).is_some();
        self.seen
            .lock()
            .expect("unpoisoned")
            .push((self.memory.used(), sibling));
        if self.pend {
            std::future::pending::<()>().await;
        }
        self.mock.send(request).await
    }

    /// Streams are not used.
    ///
    /// # Errors
    /// Always the mock's empty-stream error.
    async fn stream(&self, request: &ProviderRequest) -> Result<ProviderStream, ProviderError> {
        self.mock.stream(request).await
    }

    /// The mocked provider name.
    fn name(&self) -> ProviderName {
        self.mock.name()
    }
}

/// Bound media is read once per input and charged only the guard its reader
/// returns, however often its placeholder occurs: no provider copies are
/// promised. Memory short of that guard refuses before the provider sees
/// anything and holds nothing, and with enough memory the held call keeps
/// the guard, refusing a sibling the read and encoding, until it completes
/// or is cancelled.
///
/// # Panics
/// Panics when a refusal reaches the provider or keeps a charge, binding
/// reads the media other than once, the held call is charged other than the
/// guard, a sibling reuses its bytes, or the charge outlives the call.
#[tokio::test]
async fn bound_media_holds_only_its_guard_through_the_call() {
    let raw = 48 * 1024;
    let encoded = (raw / 3 * 4) as u64;
    for (messages, occurrences) in [
        (vec!["Describe ${media:shot}".to_owned()], 1_u64),
        (
            vec![
                "Describe ${media:shot}".to_owned(),
                "Then compare ${media:shot}".to_owned(),
            ],
            2,
        ),
    ] {
        let prompt = || {
            openai_chat(
                "gpt-test",
                OpenAiChatOptions {
                    messages: messages.clone(),
                    ..OpenAiChatOptions::default()
                },
            )
            .expect("static prompt is valid")
        };
        let inputs = BTreeMap::from([(
            "shot".to_owned(),
            CallInput {
                value: CallValue::Evidence(InputDeclaration::File {
                    sha256: Sha256Hex::digest(b"png"),
                    size_bytes: raw as u64,
                    format: skald_spec::MediaFormat::Png,
                    filename: None,
                }),
                sources: BTreeSet::new(),
            },
        )]);
        let peak = encoded;

        for (capacity, pend) in [
            (peak - 1, false),
            (peak + raw as u64, false),
            (peak + raw as u64, true),
        ] {
            let memory = Arc::new(Memory {
                capacity,
                ..Memory::default()
            });
            let provider = MeteredMedia {
                mock: MockProvider::new(ProviderName::OpenAi),
                memory: Arc::clone(&memory),
                sibling: raw as u64 + encoded,
                seen: Arc::default(),
                pend,
            };
            provider.mock.push_response(openai_text_response("a cat"));
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider.clone()));
            let reader = Arc::new(ChargedMedia {
                memory: Arc::clone(&memory),
                raw,
                reads: std::sync::atomic::AtomicUsize::new(0),
            });
            let executor = AgentExecutor::new(
                Arc::new(providers),
                Arc::new(NoRegistry),
                Arc::new(NoRegistry),
            )
            .with_evidence(Arc::clone(&reader) as Arc<dyn EvidenceReader>);
            let call = agent(prompt(), &[]);
            let run = step(&executor, &call, inputs.clone());
            let result = if pend {
                let cancelled = tokio::time::timeout(Duration::from_millis(200), run).await;
                assert!(cancelled.is_err(), "the pending provider never answers");
                None
            } else {
                Some(run.await)
            };

            assert_eq!(
                reader.reads.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "binding reads the media once for {occurrences} occurrence(s)"
            );
            let seen = provider.seen.lock().expect("unpoisoned").clone();
            if capacity < peak {
                assert!(
                    matches!(result, Some(Err(BlockError::Busy { .. }))),
                    "{result:?}"
                );
                assert!(seen.is_empty(), "a refusal precedes the provider call");
            } else {
                if let Some(result) = result {
                    result.expect("the media step runs");
                }
                assert_eq!(
                    seen,
                    [(peak, false)],
                    "the held call keeps its peak and refuses the sibling (pend: {pend})"
                );
            }
            assert_eq!(memory.used(), 0, "the call's end returns the charge");
        }
    }
}
