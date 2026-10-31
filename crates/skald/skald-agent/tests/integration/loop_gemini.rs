use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use skald_agent::{Agent, FinishReason, RunConfig, WyrdGatewayCall, WyrdGatewayCaller};
use skald_providers::ProviderError;
use tokio_util::sync::CancellationToken;
use wyrd_spec::ids::ProviderDeploymentName;

use crate::callbacks::RecordingTool;
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::google_generate::{
    GoogleAnswerContent, GoogleCandidate, GoogleContent, GoogleFinishReason,
    GoogleGenerateContentRequest, GoogleGenerateContentResponse, GoogleGenerateSettings,
    GooglePart,
};
use skald_spec::{Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse};

/// A model-role Gemini answer carrying one text part, used to script the
/// mock provider's responses.
fn google_answer(text: &str) -> GoogleAnswerContent {
    GoogleAnswerContent {
        role: Some("model".to_owned()),
        parts: vec![GooglePart::Text {
            text: text.to_owned(),
        }],
    }
}

fn google_content(role: &str, text: &str) -> GoogleContent {
    GoogleContent {
        role: role.to_owned(),
        parts: vec![GooglePart::Text {
            text: text.to_owned(),
        }],
    }
}

fn gemini_request(contents: Vec<GoogleContent>) -> ProviderRequest {
    ProviderRequest::GeminiGenerateContent(GoogleGenerateContentRequest {
        contents,
        system_instruction: Some(google_content("system", "be helpful")),
        tools: None,
        tool_config: None,
        settings: GoogleGenerateSettings::default(),
    })
}

/// A Gemini answer with one candidate holding `text`, shared by tests that
/// script Gemini replies.
pub(crate) fn gemini_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::GeminiGenerateContent(GoogleGenerateContentResponse {
        candidates: vec![GoogleCandidate {
            content: google_answer(text),
            finish_reason: Some(GoogleFinishReason::Stop),
            index: Some(0),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            grounding_metadata: None,
            avg_logprobs: None,
        }],
        usage_metadata: None,
        model_version: Some("gemini-1.5-pro".to_owned()),
        prompt_feedback: None,
        response_id: None,
        create_time: None,
    })
}

/// A Gemini Prompt runs the Agent loop against the Google provider: the user
/// input is appended to the native contents and a text answer stops the run
/// after one iteration.
///
/// # Panics
/// Panics when the Prompt does not build, the mock sees another request, or
/// the run fails or finishes otherwise.
#[tokio::test]
async fn agent_run_executes_gemini_loop_against_mock_provider() {
    let expected_request = gemini_request(vec![google_content("user", "hello")]);
    let mock = MockProvider::new(ProviderName::Google)
        .expect_request(expected_request)
        .respond_with(gemini_text_response("hi"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));

    let prompt = Prompt::from_native(
        SpecPrompt::new(gemini_request(Vec::new()), "gemini-1.5-pro")
            .expect("Gemini prompt should build"),
    );
    let agent = Agent::new(prompt).with_id("g").with_run_config(RunConfig {
        max_iterations: 3,
        ..Default::default()
    });

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("Gemini loop must run");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 1);
    assert_eq!(run.output, "hi");
    assert!(run.final_response.is_some());
}

/// Gateway caller that decodes scripted raw Google answers as typed responses,
/// as the hosted gateway caller does, and records every native request.
struct RecordingGoogleGateway {
    /// Raw JSON answers returned in order.
    replies: Mutex<VecDeque<serde_json::Value>>,
    /// Serialized native request of every call, in order.
    requests: Mutex<Vec<serde_json::Value>>,
}

#[async_trait]
impl WyrdGatewayCaller for RecordingGoogleGateway {
    /// Record the request and decode the next scripted raw answer.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when a lock is poisoned, the script is empty, or a scripted
    /// answer is not a Google `GenerateContent` response.
    async fn call(
        &self,
        call: WyrdGatewayCall,
        _cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let ProviderRequest::GeminiGenerateContent(request) = call.request else {
            panic!("expected a Google request");
        };
        self.requests
            .lock()
            .expect("requests lock")
            .push(serde_json::to_value(&request).expect("request serializes"));
        let raw = self
            .replies
            .lock()
            .expect("replies lock")
            .pop_front()
            .expect("a scripted reply remains");
        Ok(ProviderResponse::GeminiGenerateContent(
            serde_json::from_str(&raw.to_string()).expect("scripted Google answer decodes"),
        ))
    }
}

/// A signed Google function call crosses typed decoding, tool execution and
/// assistant history: the next request replays the opaque signature on the
/// original call part, in order, followed by the matching tool result. Both
/// decoded spellings and an unsigned call are covered.
///
/// # Panics
/// Panics when the Prompt does not build, a run fails, the tool is not
/// called, or the continuation request loses, moves, or invents a signature.
#[tokio::test]
async fn signed_function_call_replays_its_signature_on_continuation() {
    for (signature_key, signature) in [
        (Some("thoughtSignature"), Some("c2lnLWNhbWVs")),
        (Some("thought_signature"), Some("c2lnLXNuYWtl")),
        (None, None),
    ] {
        let mut call_part = serde_json::json!({
            "functionCall": {"name": "lookup", "args": {"q": "order"}}
        });
        if let (Some(key), Some(value)) = (signature_key, signature) {
            call_part[key] = serde_json::json!(value);
        }
        let gateway = Arc::new(RecordingGoogleGateway {
            replies: Mutex::new(VecDeque::from([
                serde_json::json!({"candidates": [{
                    "content": {"role": "model", "parts": [{"text": "checking"}, call_part]},
                    "finishReason": "STOP"
                }]}),
                serde_json::json!({"candidates": [{
                    "content": {"role": "model", "parts": [{"text": "done"}]},
                    "finishReason": "STOP"
                }]}),
            ])),
            requests: Mutex::new(Vec::new()),
        });
        let tool = Arc::new(RecordingTool::new(
            "lookup",
            serde_json::json!({"found": true}),
        ));
        let prompt = Prompt::from_native(
            SpecPrompt::new(gemini_request(Vec::new()), "gemini-2.5-pro")
                .expect("Gemini prompt should build"),
        );
        let run = Agent::new(prompt)
            .with_tool(tool.clone())
            .with_run_config(RunConfig {
                max_iterations: 3,
                ..Default::default()
            })
            .with_execution_route(wyrd_spec::ExecutionRoute::WyrdGateway {
                deployment: ProviderDeploymentName::new("primary").expect("deployment name"),
            })
            .with_gateway_caller(gateway.clone())
            .run_with(&ProviderRegistry::new(), None, "where is my order?")
            .await
            .expect("Gemini tool loop runs");

        assert_eq!(run.output, "done");
        assert_eq!(tool.calls().len(), 1);
        let requests = gateway.requests.lock().expect("requests lock");
        assert_eq!(requests.len(), 2);
        let contents = requests[1]["contents"].as_array().expect("contents");
        let model_turn = &contents[contents.len() - 2]["parts"];
        assert_eq!(model_turn[0]["text"], "checking");
        assert_eq!(model_turn[1]["function_call"]["name"], "lookup");
        match signature {
            Some(value) => assert_eq!(model_turn[1]["thought_signature"], value),
            None => assert!(model_turn[1].get("thought_signature").is_none()),
        }
        let result_turn = &contents[contents.len() - 1]["parts"][0];
        assert_eq!(result_turn["function_response"]["name"], "lookup");
    }
}
