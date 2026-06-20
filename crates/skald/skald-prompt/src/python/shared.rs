//! Items shared across more than one provider's Python bindings.

use std::sync::Arc;

use skald_spec::wire::openai_chat::OpenAiChatMessage;
use skald_spec::{ProviderRequest, ProviderResponse};

/// The owning source of a typed chat message exposed to Python.
///
/// Lets `PyOpenAiChatMessage` (and its part wrappers) live behind a single
/// `Arc` whether they were produced from a request or from a response choice.
#[derive(Clone)]
pub(crate) enum ChatMessageSource {
    Request {
        inner: Arc<ProviderRequest>,
        index: usize,
    },
    Choice {
        inner: Arc<ProviderResponse>,
        choice_index: usize,
    },
}

impl ChatMessageSource {
    pub(crate) fn msg(&self) -> &OpenAiChatMessage {
        match self {
            Self::Request { inner, index } => match inner.as_ref() {
                ProviderRequest::OpenAiChatCompletion(r) => &r.messages[*index],
                ProviderRequest::OpenAiChatCompatible { request, .. } => &request.messages[*index],
                _ => unreachable!(),
            },
            Self::Choice {
                inner,
                choice_index,
            } => match inner.as_ref() {
                ProviderResponse::OpenAiChatCompletion(r) => &r.choices[*choice_index].message,
                _ => unreachable!(),
            },
        }
    }
}
