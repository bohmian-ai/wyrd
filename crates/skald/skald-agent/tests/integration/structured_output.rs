//! Structured-output Agent runs validate each final answer against the
//! Prompt's schema and repair invalid answers inside the iteration budget.

use std::sync::Arc;

use serde_json::{Value, json};
use skald_agent::{Agent, AgentError, CallbackOutcome, JournalEvent, RunConfig};
use skald_prompt::{
    AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, ResponseFormat,
    anthropic, gemini, openai_chat, openai_responses,
};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessagesResponse, AnthropicStopReason, AnthropicUsage,
};
use skald_spec::wire::openai_chat::OpenAiResponseFormat;
use skald_spec::wire::openai_responses::OpenAiResponseContentPart;
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use wyrd_spec::error::WyrdError;

use crate::callbacks::{RecordingProvider, RecordingTool};
use crate::loop_basic::openai_tool_call_response;
use crate::loop_gemini::gemini_text_response;
use crate::loop_responses::{message, responses_answer};
use crate::session_journal::RecordingJournal;

/// Opening sentence of every correction turn.
const CORRECTION: &str = "Your previous reply was not accepted";

/// Most UTF-8 bytes one correction turn may take.
const CORRECTION_LIMIT: usize = 16 * 1024;

/// A registry whose OpenAI mock answers one Chat reply holding `text`.
fn registry_returning_text(text: &str) -> ProviderRegistry {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(ProviderResponse::text(text));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    providers
}

/// A registry holding `provider` under its own name.
fn registry(provider: &RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider.clone()));
    providers
}

/// The closed two-string schema most cases use.
fn foo_bar() -> Value {
    json!({
        "type": "object",
        "properties": {"foo": {"type": "string"}, "bar": {"type": "string"}},
        "required": ["foo", "bar"],
        "additionalProperties": false
    })
}

/// A Verdict-shaped schema with an enum and a closed-object union.
fn verdict_like() -> Value {
    let closed = |key: &str, value: Value| json!({"type": "object", "properties": {key: value}, "required": [key], "additionalProperties": false});
    json!({
        "type": "object",
        "properties": {
            "outcome": {"$ref": "#/$defs/Outcome"},
            "reason": {"type": "string"},
            "citations": {"type": "array", "items": {
                "type": "object",
                "properties": {"slot": {"type": "string"}, "locator": {"$ref": "#/$defs/Locator"}},
                "required": ["slot", "locator"],
                "additionalProperties": false
            }}
        },
        "required": ["outcome", "reason", "citations"],
        "additionalProperties": false,
        "$defs": {
            "Outcome": {"type": "string", "enum": ["passed", "failed", "inconclusive"]},
            "Locator": {"anyOf": [
                closed("page", json!({"type": "integer"})),
                closed("json_pointer", json!({"type": "string"})),
            ]}
        }
    })
}

/// A named JSON-schema output format for `schema`.
///
/// # Panics
/// Panics when `schema` is not a JSON object.
fn format(schema: Value) -> ResponseFormat {
    ResponseFormat::json_schema("test", schema).expect("schema is an object")
}

/// An OpenAI Chat Prompt asking for output satisfying `schema`.
///
/// # Panics
/// Panics when `schema` is not an object or the Prompt does not build.
fn chat_prompt(schema: Value) -> skald_prompt::Prompt {
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Say hi.".into()],
            output: Some(format(schema)),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("chat prompt builds")
}

/// An Anthropic answer with one text block.
fn anthropic_text(text: &str) -> ProviderResponse {
    ProviderResponse::AnthropicMessage(AnthropicMessagesResponse {
        id: "msg".to_owned(),
        r#type: "message".to_owned(),
        role: "assistant".to_owned(),
        model: "claude-test".to_owned(),
        content: vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }],
        stop_reason: Some(AnthropicStopReason::EndTurn),
        stop_sequence: None,
        usage: AnthropicUsage::default(),
    })
}

/// A Responses answer with one assistant output message.
fn responses_text(text: &str) -> ProviderResponse {
    responses_answer(vec![message(
        "assistant",
        OpenAiResponseContentPart::OutputText {
            text: text.to_owned(),
        },
    )])
}

/// One conversation encoding: its name, a structured Prompt, its provider,
/// and a builder for that provider's text answer.
type Encoding = (
    &'static str,
    skald_prompt::Prompt,
    ProviderName,
    fn(&str) -> ProviderResponse,
);

/// A structured Prompt and an answer builder for each conversation encoding.
///
/// # Panics
/// Panics when a Prompt does not build.
fn encodings() -> Vec<Encoding> {
    vec![
        (
            "chat",
            chat_prompt(foo_bar()),
            ProviderName::OpenAi,
            |text| ProviderResponse::text(text),
        ),
        (
            "responses",
            openai_responses(
                "gpt-test",
                OpenAiResponsesOptions {
                    messages: vec!["Say hi.".into()],
                    output: Some(format(foo_bar())),
                    ..OpenAiResponsesOptions::default()
                },
            )
            .expect("responses prompt builds"),
            ProviderName::OpenAi,
            responses_text,
        ),
        (
            "anthropic",
            anthropic(
                "claude-test",
                AnthropicOptions {
                    messages: vec!["Say hi.".into()],
                    output: Some(format(foo_bar())),
                    ..AnthropicOptions::default()
                },
            )
            .expect("anthropic prompt builds"),
            ProviderName::Anthropic,
            anthropic_text,
        ),
        (
            "gemini",
            gemini(
                "gemini-test",
                GeminiOptions {
                    messages: vec!["Say hi.".into()],
                    output: Some(format(foo_bar())),
                    ..GeminiOptions::default()
                },
            )
            .expect("gemini prompt builds"),
            ProviderName::Google,
            gemini_text_response,
        ),
    ]
}

/// The request serialized as JSON text, for content assertions.
///
/// # Panics
/// Panics when the request does not serialize.
fn text_of(request: &ProviderRequest) -> String {
    serde_json::to_string(request).expect("request serializes")
}

/// The last user text of an OpenAI Chat request.
///
/// # Panics
/// Panics when the request is not Chat, has no user turn, or that turn is
/// not plain text.
fn last_user(request: &ProviderRequest) -> String {
    let ProviderRequest::OpenAiChatCompletion(request) = request else {
        panic!("an OpenAI Chat request");
    };
    let message = request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .expect("a user turn");
    match &message.content {
        Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(text)) => text.clone(),
        other => panic!("text user content, got {other:?}"),
    }
}

/// An Agent running `prompt` with `cap` iterations and a recording journal.
fn agent(prompt: skald_prompt::Prompt, cap: u32, journal: &RecordingJournal) -> Agent {
    Agent::new(prompt)
        .with_journal(Arc::new(journal.clone()))
        .with_run_config(RunConfig {
            max_iterations: cap,
            ..RunConfig::default()
        })
}

/// A valid first answer is accepted after one iteration and parsed into
/// the structured output map.
///
/// # Panics
/// Panics when the run fails, takes more iterations, or the map differs.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_parses_when_schema_declared() {
    let prompt = chat_prompt(foo_bar());
    let agent = Agent::new(prompt);
    let providers = registry_returning_text(r#"{"foo":"hello","bar":"world"}"#);

    let run = agent
        .run_prompt(&providers, &agent.prompt, &[])
        .await
        .unwrap();

    let map = run.structured_output.expect("structured output present");
    assert_eq!(map.get("foo"), Some(&Value::String("hello".to_owned())));
    assert_eq!(map.get("bar"), Some(&Value::String("world".to_owned())));
    assert_eq!(run.iterations, 1);
}

/// Every conversation encoding repairs malformed JSON, a non-object, and a
/// schema-invalid object in one run: each rejected reply stays in the
/// conversation, the next request carries a correction turn, and the
/// corrected answer ends the run with one journaled iteration per call.
///
/// # Panics
/// Panics when a run fails, a request lacks its correction, or the
/// iteration accounting differs.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_repairs_every_encoding() {
    for (encoding, prompt, name, answer) in encodings() {
        let provider = RecordingProvider::named(
            name,
            vec![
                answer("not json"),
                answer("[1, 2]"),
                answer(r#"{"foo": "x"}"#),
                answer(r#"{"foo": "hello", "bar": "world"}"#),
            ],
        );
        let journal = RecordingJournal::new();
        let agent = agent(prompt, 4, &journal);

        let run = agent
            .run_prompt(&registry(&provider), &agent.prompt, &[])
            .await
            .unwrap_or_else(|error| panic!("{encoding}: {error}"));

        assert_eq!(run.iterations, 4, "{encoding}");
        assert_eq!(
            run.structured_output.expect("structured")["bar"],
            "world",
            "{encoding}"
        );
        let requests = provider.requests();
        assert_eq!(requests.len(), 4, "{encoding}");
        assert!(!text_of(&requests[0]).contains(CORRECTION), "{encoding}");
        for (index, (rejected, said)) in [
            ("not json", "not valid JSON"),
            ("[1, 2]", "not a JSON object"),
            (r#"{\"foo\": \"x\"}"#, "/required"),
        ]
        .into_iter()
        .enumerate()
        {
            let next = text_of(&requests[index + 1]);
            assert!(next.contains(rejected), "{encoding}: reply kept: {next}");
            assert!(next.contains(CORRECTION), "{encoding}: {next}");
            assert!(next.contains(said), "{encoding}: {said}: {next}");
        }
        let iterations = journal
            .events()
            .iter()
            .filter(|event| matches!(event, JournalEvent::Iteration { .. }))
            .count();
        let calls = journal
            .events()
            .iter()
            .filter(|event| matches!(event, JournalEvent::ModelCall { .. }))
            .count();
        assert_eq!((iterations, calls), (4, 4), "{encoding}");
    }
}

/// One correction lists every violation with its location and rule, shows
/// the permitted union shapes and enum values, and stays within its bound.
///
/// # Panics
/// Panics when a violation, union shape, or bound is missing.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_correction_explains_violations() {
    let invalid = json!({
        "outcome": "FAIL",
        "reason": "r",
        "citations": [{"slot": "order", "locator": {"page": 1, "slide": 2}}],
        "extra": true
    });
    let provider = RecordingProvider::named(
        ProviderName::OpenAi,
        vec![
            ProviderResponse::text(invalid.to_string()),
            ProviderResponse::text(
                json!({"outcome": "failed", "reason": "r", "citations": []}).to_string(),
            ),
        ],
    );
    let journal = RecordingJournal::new();
    let agent = agent(chat_prompt(verdict_like()), 2, &journal);

    let run = agent
        .run_prompt(&registry(&provider), &agent.prompt, &[])
        .await
        .expect("the corrected answer is accepted");

    assert_eq!(
        run.structured_output.expect("structured")["outcome"],
        "failed"
    );
    let correction = last_user(&provider.requests()[1]);
    assert!(correction.len() <= CORRECTION_LIMIT, "{}", correction.len());
    for expected in [
        "/outcome",
        "/$defs/Outcome/enum",
        "inconclusive",
        "/citations/0/locator",
        "/$defs/Locator/anyOf",
        "json_pointer",
        "additionalProperties",
    ] {
        assert!(correction.contains(expected), "{expected}: {correction}");
    }
}

/// Correction text stays within 16 KiB and says what it omitted when a
/// reply has more violations, or a schema larger, than fit.
///
/// # Panics
/// Panics when the correction exceeds its bound or omits silently.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_correction_is_bounded() {
    let values: Vec<String> = (0..4_000).map(|index| format!("value-{index}")).collect();
    let mut properties = serde_json::Map::new();
    for index in 0..30 {
        properties.insert(format!("p{index}"), json!({"enum": values}));
    }
    let schema = json!({"type": "object", "properties": properties, "additionalProperties": false});
    let reply: serde_json::Map<String, Value> = (0..30)
        .map(|index| (format!("p{index}"), json!("nope")))
        .collect();
    let provider = RecordingProvider::named(
        ProviderName::OpenAi,
        vec![
            ProviderResponse::text(Value::Object(reply).to_string()),
            ProviderResponse::text("not json"),
            ProviderResponse::text("{}"),
        ],
    );
    let journal = RecordingJournal::new();
    let agent = agent(chat_prompt(schema), 3, &journal);

    agent
        .run_prompt(&registry(&provider), &agent.prompt, &[])
        .await
        .expect("the empty object is valid");

    let requests = provider.requests();
    for request in &requests[1..=2] {
        let correction = last_user(request);
        assert!(correction.len() <= CORRECTION_LIMIT, "{}", correction.len());
        assert!(correction.contains("omitted"), "{correction}");
    }
    assert!(last_user(&requests[1]).contains("20 further violations omitted"));
}

/// A reply that never satisfies the schema ends the run at the cap with the
/// structured-invalid code, carrying schema locations but never reply
/// values, returned keys, or parser text, in the error and the journal.
///
/// # Panics
/// Panics when the run succeeds, makes another number of calls, or a
/// reply-derived marker leaks.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_exhaustion_is_safe() {
    let marker = "MARKER-7f3a";
    let replies = [
        json!({"outcome": marker, "reason": marker, "citations": [{"slot": marker, "locator": {"page": marker}}], "key-7f3a": marker}).to_string(),
        format!("{{\"{marker}\": "),
        json!({"outcome": "passed", "reason": 1, "citations": [], "data": {"key-7f3a": marker}}).to_string(),
    ];
    let provider = RecordingProvider::named(
        ProviderName::OpenAi,
        replies.iter().map(ProviderResponse::text).collect(),
    );
    let journal = RecordingJournal::new();
    let agent = agent(chat_prompt(verdict_like()), 3, &journal);

    let error = agent
        .run_prompt(&registry(&provider), &agent.prompt, &[])
        .await
        .expect_err("persistent invalid output fails");

    assert_eq!(provider.requests().len(), 3);
    assert_eq!(error.code(), "SKALD_AGENT_422_STRUCTURED_INVALID");
    let AgentError::StructuredOutputInvalid { violations, .. } = &error else {
        panic!("structured-invalid, got {error:?}");
    };
    assert!(
        !violations.is_empty() && violations.len() <= 10,
        "{violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("/properties/reason/type")),
        "{violations:?}"
    );
    let public = WyrdError::from(&error);
    assert_eq!(public.code(), "WYRD_AGENT_422_STRUCTURED_INVALID");
    assert_eq!(public.status(), 422);
    let problem = serde_json::to_string(&public.as_problem_json()).expect("problem json");
    let journaled = format!("{:?}", journal.events());
    for surface in [
        error.to_string(),
        format!("{violations:?}"),
        problem,
        journaled,
    ] {
        for leak in [marker, "key-7f3a", "EOF", "line 1"] {
            assert!(!surface.contains(leak), "{leak} leaked: {surface}");
        }
    }
}

/// Zero iterations makes no call and keeps the max-iterations error; one
/// iteration ends an invalid answer as structured-invalid after one call.
///
/// # Panics
/// Panics when a cap yields another error or call count.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_caps_zero_and_one() {
    for (cap, code, calls) in [
        (0, "SKALD_AGENT_500_MAX_ITERATIONS", 0),
        (1, "SKALD_AGENT_422_STRUCTURED_INVALID", 1),
    ] {
        let provider = RecordingProvider::named(
            ProviderName::OpenAi,
            vec![ProviderResponse::text("not json")],
        );
        let journal = RecordingJournal::new();
        let agent = agent(chat_prompt(foo_bar()), cap, &journal);
        let error = agent
            .run_prompt(&registry(&provider), &agent.prompt, &[])
            .await
            .expect_err("no valid answer");
        assert_eq!(error.code(), code, "cap {cap}");
        assert_eq!(provider.requests().len(), calls, "cap {cap}");
    }
}

/// Tool turns and repairs share one budget: a tool call, an invalid
/// answer, and a corrected answer fit three iterations; with two the
/// invalid answer is final; tool calls alone keep max-iterations.
///
/// # Panics
/// Panics when a budget yields another outcome or the tool does not run.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_repair_shares_tool_budget() {
    let valid = r#"{"foo": "a", "bar": "b"}"#;
    for (cap, script, expected) in [
        (3, vec!["tool", "{}", valid], Ok(3)),
        (
            2,
            vec!["tool", "{}", valid],
            Err("SKALD_AGENT_422_STRUCTURED_INVALID"),
        ),
        (
            2,
            vec!["tool", "tool", valid],
            Err("SKALD_AGENT_500_MAX_ITERATIONS"),
        ),
    ] {
        let responses = script
            .iter()
            .map(|step| match *step {
                "tool" => openai_tool_call_response("lookup", json!({})),
                text => ProviderResponse::text(text),
            })
            .collect();
        let provider = RecordingProvider::named(ProviderName::OpenAi, responses);
        let tool = Arc::new(RecordingTool::new("lookup", json!({"ok": true})));
        let journal = RecordingJournal::new();
        let agent = agent(chat_prompt(foo_bar()), cap, &journal).with_tool(tool.clone());
        let result = agent
            .run_prompt(&registry(&provider), &agent.prompt, &[])
            .await;
        match (result, expected) {
            (Ok(run), Ok(iterations)) => assert_eq!(run.iterations, iterations),
            (Err(error), Err(code)) => assert_eq!(error.code(), code, "cap {cap}: {error}"),
            (other, expected) => panic!("cap {cap}: {other:?} != {expected:?}"),
        }
        assert!(!tool.calls().is_empty(), "cap {cap}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn text_prompt_leaves_structured_output_none() {
    let prompt = openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Hi".into()],
            ..OpenAiChatOptions::default()
        },
    )
    .unwrap();
    let agent = Agent::new(prompt);
    let providers = registry_returning_text("hello");

    let run = agent
        .run_prompt(&providers, &agent.prompt, &[])
        .await
        .unwrap();

    assert!(run.structured_output.is_none());
    assert_eq!(run.output, "hello");
}

/// The JSON schema an OpenAI Chat request sends.
///
/// # Panics
/// Panics when the request is not Chat or sends no JSON schema.
fn sent_schema(request: &ProviderRequest) -> Value {
    let body = serde_json::to_value(request).expect("request serializes");
    let schema = body["body"]["response_format"]["json_schema"]["schema"].clone();
    assert!(schema.is_object(), "a JSON schema: {body}");
    schema
}

/// A schema whose enum value is bound from a Prompt variable is validated
/// as sent: the bound value succeeds first time, and the literal
/// placeholder is repaired.
///
/// # Panics
/// Panics when the sent schema keeps the placeholder, the bound value is
/// refused, or the placeholder is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_validates_the_bound_schema() {
    let schema = json!({
        "type": "object",
        "properties": {"answer": {"enum": ["${expected}"]}},
        "required": ["answer"],
        "additionalProperties": false
    });
    let ready = r#"{"answer": "ready"}"#;
    for (replies, calls) in [
        (vec![ready], 1),
        (vec![r#"{"answer": "${expected}"}"#, ready], 2),
    ] {
        let provider = RecordingProvider::named(
            ProviderName::OpenAi,
            replies.into_iter().map(ProviderResponse::text).collect(),
        );
        let journal = RecordingJournal::new();
        let agent = agent(chat_prompt(schema.clone()), 2, &journal);
        let run = agent
            .run_prompt(
                &registry(&provider),
                &agent.prompt,
                &[("expected", "ready")],
            )
            .await
            .unwrap_or_else(|error| panic!("{calls} calls: {error}"));
        assert_eq!(
            run.structured_output.expect("structured")["answer"],
            "ready"
        );
        let requests = provider.requests();
        assert_eq!(requests.len(), calls);
        assert_eq!(
            sent_schema(&requests[0])["properties"]["answer"]["enum"],
            json!(["ready"])
        );
    }
}

/// A `before_model` hook that replaces the response schema moves validation
/// to the replacement: an answer satisfying only the Prompt's schema is
/// repaired, and the Prompt itself is unchanged.
///
/// # Panics
/// Panics when the stale answer is accepted, the replacement answer is
/// refused, or the Prompt changes.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_validates_a_replaced_schema() {
    let replacement = json!({
        "type": "object",
        "properties": {"baz": {"type": "string"}},
        "required": ["baz"],
        "additionalProperties": false
    });
    let swapped = replacement.clone();
    let provider = RecordingProvider::named(
        ProviderName::OpenAi,
        vec![
            ProviderResponse::text(r#"{"foo": "a", "bar": "b"}"#),
            ProviderResponse::text(r#"{"baz": "z"}"#),
        ],
    );
    let journal = RecordingJournal::new();
    let agent =
        agent(chat_prompt(foo_bar()), 2, &journal).before_model(Arc::new(move |_ctx, request| {
            let mut request = request.clone();
            let ProviderRequest::OpenAiChatCompletion(chat) = &mut request else {
                panic!("an OpenAI Chat request");
            };
            if let Some(OpenAiResponseFormat::JsonSchema { json_schema }) =
                &mut chat.response_format
            {
                json_schema.schema = swapped.as_object().cloned();
            }
            CallbackOutcome::ReplaceWith(request)
        }));
    let card = format!("{:?}", agent.prompt.native().response_type);

    let run = agent
        .run_with(&registry(&provider), None, "hello")
        .await
        .expect("the replacement answer is accepted");

    assert_eq!(run.structured_output.expect("structured")["baz"], "z");
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(sent_schema(&requests[0]), replacement);
    assert!(text_of(&requests[1]).contains(CORRECTION));
    assert_eq!(format!("{:?}", agent.prompt.native().response_type), card);
}

/// A native refusal without an answer ends a structured run after one call
/// with the structured-invalid code and a fixed diagnostic, though budget
/// and a valid answer remain.
///
/// # Panics
/// Panics when a refusal is repaired, succeeds, or yields another error.
#[tokio::test(flavor = "multi_thread")]
async fn structured_output_does_not_repair_refusals() {
    let mut chat = ProviderResponse::text("");
    if let ProviderResponse::OpenAiChatCompletion(response) = &mut chat {
        response.choices[0].message.content = None;
        response.choices[0].message.refusal = Some("s3cr3t refusal".to_owned());
    }
    let ProviderResponse::AnthropicMessage(mut refused) = anthropic_text("") else {
        unreachable!("an Anthropic answer");
    };
    refused.content.clear();
    refused.stop_reason = Some(AnthropicStopReason::Refusal);
    let anthropic_prompt = encodings()
        .into_iter()
        .find(|(name, ..)| *name == "anthropic")
        .expect("anthropic encoding");
    for (prompt, name, refusal, valid) in [
        (
            chat_prompt(foo_bar()),
            ProviderName::OpenAi,
            chat,
            ProviderResponse::text(r#"{"foo": "a", "bar": "b"}"#),
        ),
        (
            anthropic_prompt.1,
            ProviderName::Anthropic,
            ProviderResponse::AnthropicMessage(refused),
            anthropic_text(r#"{"foo": "a", "bar": "b"}"#),
        ),
    ] {
        let provider = RecordingProvider::named(name.clone(), vec![refusal, valid]);
        let journal = RecordingJournal::new();
        let agent = agent(prompt, 3, &journal);
        let error = agent
            .run_prompt(&registry(&provider), &agent.prompt, &[])
            .await
            .expect_err("a refusal is terminal");
        assert_eq!(provider.requests().len(), 1, "{name:?}");
        assert_eq!(
            error.code(),
            "SKALD_AGENT_422_STRUCTURED_INVALID",
            "{name:?}"
        );
        let AgentError::StructuredOutputInvalid { violations, .. } = &error else {
            panic!("structured-invalid, got {error:?}");
        };
        assert_eq!(violations, &["provider refused without an answer"]);
        assert!(!error.to_string().contains("s3cr3t"), "{error}");
    }
}
