//! Public native mock provider for runtime and agent tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use skald_providers::{ProviderError, ProviderStream};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiUsage,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};

use crate::provider::Provider;

/// One scripted native provider exchange.
#[derive(Debug, Clone, PartialEq)]
pub struct MockExchange {
    /// Expected native request. `None` matches any request.
    pub expected: Option<ProviderRequest>,
    /// Native response or provider error to return.
    pub response: Result<ProviderResponse, ProviderError>,
}

/// Scripted expectation builder returned by [`MockProvider::expect_request`].
pub struct MockExpectation {
    provider: MockProvider,
    expected: ProviderRequest,
}

/// Public provider mock backed by queued native responses and streams.
#[derive(Clone)]
pub struct MockProvider {
    name: ProviderName,
    responses: Arc<Mutex<VecDeque<MockExchange>>>,
    streams: Arc<Mutex<VecDeque<Result<ProviderStream, ProviderError>>>>,
    echo: bool,
}

impl MockProvider {
    /// Creates an empty mock for one provider name.
    pub fn new(name: ProviderName) -> Self {
        Self {
            name,
            responses: Arc::new(Mutex::new(VecDeque::new())),
            streams: Arc::new(Mutex::new(VecDeque::new())),
            echo: false,
        }
    }

    /// Creates a mock provider that echoes the last user message.
    pub fn echo() -> Self {
        Self {
            echo: true,
            ..Self::new(ProviderName::Custom("mock".to_owned()))
        }
    }

    /// Starts an expected-request builder.
    pub fn expect_request(self, expected: ProviderRequest) -> MockExpectation {
        MockExpectation {
            provider: self,
            expected,
        }
    }

    /// Queues a wildcard native response that accepts any request.
    pub fn push_response(&self, response: ProviderResponse) {
        self.push_exchange(MockExchange {
            expected: None,
            response: Ok(response),
        });
    }

    /// Queues a wildcard provider error that accepts any request.
    pub fn push_error(&self, error: ProviderError) {
        self.push_exchange(MockExchange {
            expected: None,
            response: Err(error),
        });
    }

    /// Queues a provider-native stream result.
    pub fn push_stream(&self, stream: ProviderStream) {
        self.stream_queue().push_back(Ok(stream));
    }

    /// Queues a provider-native stream error.
    pub fn push_stream_error(&self, error: ProviderError) {
        self.stream_queue().push_back(Err(error));
    }

    /// Returns the number of pending response exchanges.
    pub fn remaining(&self) -> usize {
        self.response_queue().len()
    }

    fn push_exchange(&self, exchange: MockExchange) {
        self.response_queue().push_back(exchange);
    }

    fn response_queue(&self) -> MutexGuard<'_, VecDeque<MockExchange>> {
        match self.responses.lock() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn stream_queue(&self) -> MutexGuard<'_, VecDeque<Result<ProviderStream, ProviderError>>> {
        match self.streams.lock() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl MockExpectation {
    /// Queues the expected request with a canned response.
    pub fn respond_with(self, response: ProviderResponse) -> MockProvider {
        self.provider.push_exchange(MockExchange {
            expected: Some(self.expected),
            response: Ok(response),
        });
        self.provider
    }

    /// Queues the expected request with a provider error.
    pub fn respond_with_error(self, error: ProviderError) -> MockProvider {
        self.provider.push_exchange(MockExchange {
            expected: Some(self.expected),
            response: Err(error),
        });
        self.provider
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let exchange = self.response_queue().pop_front();
        let Some(exchange) = exchange else {
            return if self.echo {
                Ok(openai_text_response(&last_user_text(&request)))
            } else {
                Err(ProviderError::bad_request(
                    "mock",
                    "mock response queue is empty",
                ))
            };
        };

        if let Some(expected) = &exchange.expected {
            let expected = serde_json::to_value(expected)
                .map_err(|error| ProviderError::decode("mock", error))?;
            let actual = serde_json::to_value(&request)
                .map_err(|error| ProviderError::decode("mock", error))?;
            if expected != actual {
                return Err(ProviderError::bad_request(
                    "mock",
                    format!("mock request mismatch: expected {expected}, got {actual}"),
                ));
            }
        }

        exchange.response
    }

    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        self.stream_queue()
            .pop_front()
            .ok_or_else(|| ProviderError::bad_request("mock", "mock stream queue is empty"))?
    }

    fn name(&self) -> ProviderName {
        self.name.clone()
    }
}

fn last_user_text(request: &ProviderRequest) -> String {
    let messages = match request {
        ProviderRequest::OpenAiChatCompletion(request) => &request.messages,
        ProviderRequest::OpenAiChatCompatible { request, .. } => &request.messages,
        _ => return String::new(),
    };
    messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(|message| message.content.as_ref())
        .map(openai_content_text)
        .unwrap_or_default()
}

fn openai_content_text(content: &OpenAiMessageContent) -> String {
    match content {
        OpenAiMessageContent::Text(text) => text.clone(),
        OpenAiMessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                skald_spec::wire::openai_chat::OpenAiContentPart::Text { text } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect(),
    }
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "mock_response".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "mock-model".to_owned(),
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
        usage: Some(OpenAiUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
        system_fingerprint: None,
        service_tier: None,
    })
}
