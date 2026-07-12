use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{SkaldError, SkaldResult};
use crate::media::{MediaKind, MediaRef, MediaSource};
use crate::request::{ProviderName, ProviderRequest};
use crate::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicDocumentSource, AnthropicImageSource, AnthropicMessagesRequest,
    AnthropicMessagesSettings, AnthropicSystem, AnthropicSystemBlock,
};
use crate::wire::google_generate::{
    GoogleContent, GoogleFileData, GoogleGenerateContentRequest, GoogleGenerateSettings,
    GoogleInlineData, GooglePart,
};
use crate::wire::openai_chat::{
    OpenAiChatRequest, OpenAiChatSettings, OpenAiContentPart, OpenAiFilePart, OpenAiImageUrl,
    OpenAiMessageContent,
};
use crate::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesSettings,
};

/// Authored native prompt request plus render metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Prompt {
    /// Native provider-shaped request authored with `${variable}` or `{{variable}}` placeholders.
    pub request: ProviderRequest,
    /// Model identifier used for indexing and selection.
    pub model: String,
    /// Optional author-assigned prompt version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Declared text placeholder names expected by `render`.
    #[serde(default)]
    pub variables: Vec<String>,
    /// Declared media placeholder names expected by `bind_media`.
    #[serde(default)]
    pub media_variables: Vec<String>,
    /// Expected response shape for runtime validation.
    #[serde(default)]
    pub response_type: ResponseType,
}

/// Borrowed view of native generation settings for the active provider.
pub enum ProviderSettingsRef<'a> {
    /// OpenAI Chat settings.
    OpenAiChat(&'a OpenAiChatSettings),
    /// OpenAI Responses settings.
    OpenAiResponses(&'a OpenAiResponsesSettings),
    /// Anthropic Messages settings.
    Anthropic(&'a AnthropicMessagesSettings),
    /// Google Gemini or Vertex GenerateContent settings.
    Google(&'a GoogleGenerateSettings),
}

/// Expected response shape for a rendered prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    /// Plain text response.
    #[default]
    Text,
    /// JSON response validated against a named schema by the runtime.
    JsonSchema {
        /// Schema name used in diagnostics and provider-native structured output fields.
        name: String,
        /// JSON Schema object.
        schema: serde_json::Value,
    },
}

/// A segment produced by splitting text around `${media:name}` tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextSegment {
    /// Plain text between media placeholders.
    Text(String),
    /// A captured media placeholder name.
    Placeholder(String),
}

/// Returns the compiled text placeholder regex.
///
/// Accepts both `${name}` and `{{name}}`. Names must match
/// `[a-zA-Z_][a-zA-Z0-9_]*`. The `${media:name}` form is reserved for the
/// media-bind path and is not matched here.
pub fn text_placeholder_regex() -> &'static Regex {
    static TEXT_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    TEXT_PLACEHOLDER_RE.get_or_init(|| {
        match Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}|\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}") {
            Ok(regex) => regex,
            Err(error) => panic!("text placeholder regex is static and valid: {error}"),
        }
    })
}

/// Returns the compiled `${media:name}` media placeholder regex.
pub fn media_placeholder_regex() -> &'static Regex {
    static MEDIA_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    MEDIA_PLACEHOLDER_RE.get_or_init(
        || match Regex::new(r"\$\{media:([a-zA-Z_][a-zA-Z0-9_]*)\}") {
            Ok(regex) => regex,
            Err(error) => panic!("media placeholder regex is static and valid: {error}"),
        },
    )
}

/// Split a text leaf into plain text and `${media:name}` placeholder segments.
pub fn split_text_on_media(text: &str) -> Vec<TextSegment> {
    let mut out = Vec::new();
    let mut last = 0usize;
    for caps in media_placeholder_regex().captures_iter(text) {
        let Some(full) = caps.get(0) else {
            continue;
        };
        if full.start() > last {
            out.push(TextSegment::Text(text[last..full.start()].to_owned()));
        }
        if let Some(name) = caps.get(1) {
            out.push(TextSegment::Placeholder(name.as_str().to_owned()));
        }
        last = full.end();
    }
    if last < text.len() {
        out.push(TextSegment::Text(text[last..].to_owned()));
    }
    out
}

impl Prompt {
    /// Create a prompt, split media placeholders into isolated native text
    /// parts, and populate `variables` / `media_variables` from the request.
    pub fn new(
        request: ProviderRequest,
        model: impl Into<String>,
        version: Option<String>,
        response_type: ResponseType,
    ) -> SkaldResult<Self> {
        let mut prompt = Self {
            request,
            model: model.into(),
            version,
            variables: Vec::new(),
            media_variables: Vec::new(),
            response_type,
        };
        prompt.normalize_media_placeholders_mut()?;
        prompt.variables = extract_text_variables(&prompt.request)?;
        Ok(prompt)
    }

    /// Borrow native generation settings for the request's active provider.
    pub fn settings_ref(&self) -> Option<ProviderSettingsRef<'_>> {
        match &self.request {
            ProviderRequest::OpenAiChatCompletion(request)
            | ProviderRequest::OpenAiChatCompatible { request, .. } => {
                Some(ProviderSettingsRef::OpenAiChat(&request.settings))
            }
            ProviderRequest::OpenAiResponses(request) => {
                Some(ProviderSettingsRef::OpenAiResponses(&request.settings))
            }
            ProviderRequest::AnthropicMessage(request) => {
                Some(ProviderSettingsRef::Anthropic(&request.settings))
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                Some(ProviderSettingsRef::Google(&request.settings))
            }
            ProviderRequest::Vertex(request) => {
                Some(ProviderSettingsRef::Google(&request.0.settings))
            }
            ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_)
            | ProviderRequest::RawV1 { .. } => None,
        }
    }

    /// Split `${media:name}` tokens into isolated provider-native text parts.
    ///
    /// This is idempotent and rejects media placeholders in system content.
    pub fn normalize_media_placeholders_mut(&mut self) -> SkaldResult<()> {
        scan_system_for_media(&self.request)?;
        let mut names = split_request_text_parts(&mut self.request);
        names.extend(self.media_variables.iter().cloned());
        self.media_variables = dedupe_preserve_order(names);
        Ok(())
    }

    /// Return a copy with the supplied `${name}` or `{{name}}` placeholders bound.
    ///
    /// This is the reusable primitive behind higher-level prompt binding. It performs
    /// replacement inside JSON string leaves after serializing the native
    /// request, then deserializes back into the same provider-native enum.
    pub fn bind(&self, vars: &[(&str, &str)]) -> SkaldResult<Self> {
        let mut prompt = self.clone();
        prompt.bind_mut(vars)?;
        Ok(prompt)
    }

    /// Bind supplied `${name}` or `{{name}}` placeholders into this prompt in place.
    ///
    /// Unlike `render`, this may bind only a subset of declared variables. Any
    /// names still present in `variables` remain required for a later render.
    pub fn bind_mut(&mut self, vars: &[(&str, &str)]) -> SkaldResult<()> {
        let mut request = serde_json::to_value(&self.request).map_err(SkaldError::serialize)?;
        replace_string_leaves(&mut request, vars);
        self.request =
            deserialize_request_like(&self.request, request).map_err(SkaldError::deserialize)?;
        self.variables
            .retain(|name| !vars.iter().any(|(key, _)| key == name));
        Ok(())
    }

    /// Bind a `${media:name}` placeholder to a provider-native media block.
    pub fn bind_media(&self, name: &str, media: &MediaRef) -> SkaldResult<Self> {
        let mut prompt = self.clone();
        prompt.bind_media_mut(name, media)?;
        Ok(prompt)
    }

    /// Bind a `${media:name}` placeholder in place.
    ///
    /// The replacement preserves the request provider shape and inserts native
    /// media blocks such as `OpenAiContentPart::ImageUrl`,
    /// `AnthropicContentBlock::Document`, or `GooglePart::InlineData`.
    pub fn bind_media_mut(&mut self, name: &str, media: &MediaRef) -> SkaldResult<()> {
        self.normalize_media_placeholders_mut()?;
        match &mut self.request {
            ProviderRequest::OpenAiChatCompletion(request)
            | ProviderRequest::OpenAiChatCompatible { request, .. } => {
                bind_media_openai_chat(request, name, media)?;
            }
            ProviderRequest::OpenAiResponses(request) => {
                bind_media_openai_responses(request, name, media)?;
            }
            ProviderRequest::AnthropicMessage(request) => {
                bind_media_anthropic(request, name, media)?;
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                bind_media_google(request, name, media, ProviderName::Google)?;
            }
            ProviderRequest::Vertex(request) => {
                bind_media_google(&mut request.0, name, media, ProviderName::Vertex)?;
            }
            ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_)
            | ProviderRequest::RawV1 { .. } => {
                return Err(SkaldError::MediaPlaceholderNotFound {
                    name: name.to_owned(),
                });
            }
        }
        self.media_variables.retain(|declared| declared != name);
        Ok(())
    }

    /// Render declared `${name}` or `{{name}}` placeholders into the native request.
    ///
    /// Values are escaped as JSON string content before substitution, so a
    /// variable value cannot inject new fields into the serialized provider
    /// request. The returned request remains in the same provider-native shape.
    pub fn render(&self, vars: &[(&str, &str)]) -> SkaldResult<ProviderRequest> {
        let mut prompt = self.clone();
        prompt.normalize_media_placeholders_mut()?;
        if let Some(name) = prompt.media_variables.first() {
            return Err(SkaldError::MissingMediaVariable { name: name.clone() });
        }
        for name in &prompt.variables {
            vars.iter()
                .find(|(key, _)| key == name)
                .ok_or_else(|| SkaldError::missing_variable(name))?;
        }
        prompt.bind_mut(vars)?;
        Ok(prompt.request)
    }
}

fn replace_string_leaves(value: &mut Value, vars: &[(&str, &str)]) {
    match value {
        Value::String(text) => {
            for (name, replacement) in vars {
                *text = text.replace(&format!("${{{name}}}"), replacement);
                *text = text.replace(&format!("{{{{{name}}}}}"), replacement);
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_string_leaves(value, vars);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_string_leaves(value, vars);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn deserialize_request_like(
    original: &ProviderRequest,
    value: Value,
) -> serde_json::Result<ProviderRequest> {
    Ok(match original {
        ProviderRequest::OpenAiChatCompletion(_) => {
            ProviderRequest::OpenAiChatCompletion(serde_json::from_value(value)?)
        }
        ProviderRequest::OpenAiChatCompatible { .. } => serde_json::from_value(value)?,
        ProviderRequest::OpenAiResponses(_) => {
            ProviderRequest::OpenAiResponses(serde_json::from_value(value)?)
        }
        ProviderRequest::OpenAiEmbeddings(_) => {
            ProviderRequest::OpenAiEmbeddings(serde_json::from_value(value)?)
        }
        ProviderRequest::AnthropicMessage(_) => {
            ProviderRequest::AnthropicMessage(serde_json::from_value(value)?)
        }
        ProviderRequest::GeminiGenerateContent(_) => {
            ProviderRequest::GeminiGenerateContent(serde_json::from_value(value)?)
        }
        ProviderRequest::GoogleBatchEmbed(_) => {
            ProviderRequest::GoogleBatchEmbed(serde_json::from_value(value)?)
        }
        ProviderRequest::Vertex(_) => ProviderRequest::Vertex(serde_json::from_value(value)?),
        ProviderRequest::VertexPredict(_) => {
            ProviderRequest::VertexPredict(serde_json::from_value(value)?)
        }
        ProviderRequest::RawV1 { .. } => serde_json::from_value(value)?,
    })
}

fn extract_text_variables(request: &ProviderRequest) -> SkaldResult<Vec<String>> {
    let json = serde_json::to_string(request).map_err(SkaldError::serialize)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for capture in text_placeholder_regex().captures_iter(&json) {
        let Some(name_match) = capture.get(1).or_else(|| capture.get(2)) else {
            continue;
        };
        let name = name_match.as_str().to_owned();
        if seen.insert(name.clone()) {
            out.push(name);
        }
    }
    Ok(out)
}

fn dedupe_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

fn placeholder_token(name: &str) -> String {
    format!("${{media:{name}}}")
}

fn scan_system_for_media(request: &ProviderRequest) -> SkaldResult<()> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => {
            for message in &request.messages {
                if message.role == "system" {
                    scan_openai_chat_content(message.content.as_ref())?;
                }
            }
        }
        ProviderRequest::OpenAiResponses(request) => {
            if let Some(instructions) = &request.instructions {
                scan_text_for_system_media(instructions)?;
            }
        }
        ProviderRequest::AnthropicMessage(request) => {
            if let Some(system) = &request.system {
                match system {
                    AnthropicSystem::Text(text) => scan_text_for_system_media(text)?,
                    AnthropicSystem::Blocks(blocks) => {
                        for block in blocks {
                            let AnthropicSystemBlock::Text { text, .. } = block;
                            scan_text_for_system_media(text)?;
                        }
                    }
                }
            }
        }
        ProviderRequest::GeminiGenerateContent(request) => {
            scan_google_system(request.system_instruction.as_ref())?;
        }
        ProviderRequest::Vertex(request) => {
            scan_google_system(request.0.system_instruction.as_ref())?;
        }
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => {}
    }
    Ok(())
}

fn scan_openai_chat_content(content: Option<&OpenAiMessageContent>) -> SkaldResult<()> {
    match content {
        Some(OpenAiMessageContent::Text(text)) => scan_text_for_system_media(text),
        Some(OpenAiMessageContent::Parts(parts)) => {
            for part in parts {
                if let OpenAiContentPart::Text { text } = part {
                    scan_text_for_system_media(text)?;
                }
            }
            Ok(())
        }
        None => Ok(()),
    }
}

fn scan_google_system(content: Option<&GoogleContent>) -> SkaldResult<()> {
    if let Some(content) = content {
        for part in &content.parts {
            if let GooglePart::Text { text } = part {
                scan_text_for_system_media(text)?;
            }
        }
    }
    Ok(())
}

fn scan_text_for_system_media(text: &str) -> SkaldResult<()> {
    if let Some(captures) = media_placeholder_regex().captures(text) {
        return Err(SkaldError::MediaInSystemMessage {
            name: captures[1].to_owned(),
        });
    }
    Ok(())
}

fn split_request_text_parts(request: &mut ProviderRequest) -> Vec<String> {
    let mut names = Vec::new();
    match request {
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => {
            split_openai_chat(request, &mut names);
        }
        ProviderRequest::OpenAiResponses(request) => split_openai_responses(request, &mut names),
        ProviderRequest::AnthropicMessage(request) => split_anthropic(request, &mut names),
        ProviderRequest::GeminiGenerateContent(request) => split_google(request, &mut names),
        ProviderRequest::Vertex(request) => split_google(&mut request.0, &mut names),
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => {}
    }
    names
}

fn split_openai_chat(request: &mut OpenAiChatRequest, names: &mut Vec<String>) {
    for message in &mut request.messages {
        if message.role == "system" {
            continue;
        }
        message.content = split_openai_chat_content(message.content.take(), names);
    }
}

fn split_openai_chat_content(
    content: Option<OpenAiMessageContent>,
    names: &mut Vec<String>,
) -> Option<OpenAiMessageContent> {
    match content {
        Some(OpenAiMessageContent::Text(text)) => {
            let segments = split_text_on_media(&text);
            if has_placeholder(&segments) {
                Some(OpenAiMessageContent::Parts(openai_text_segments(
                    segments, names,
                )))
            } else {
                Some(OpenAiMessageContent::Text(text))
            }
        }
        Some(OpenAiMessageContent::Parts(parts)) => {
            let mut out = Vec::new();
            for part in parts {
                match part {
                    OpenAiContentPart::Text { text } => {
                        out.extend(openai_text_segments(split_text_on_media(&text), names));
                    }
                    other => out.push(other),
                }
            }
            Some(OpenAiMessageContent::Parts(out))
        }
        None => None,
    }
}

fn openai_text_segments(
    segments: Vec<TextSegment>,
    names: &mut Vec<String>,
) -> Vec<OpenAiContentPart> {
    segments
        .into_iter()
        .filter_map(|segment| match segment {
            TextSegment::Text(text) if text.is_empty() => None,
            TextSegment::Text(text) => Some(OpenAiContentPart::Text { text }),
            TextSegment::Placeholder(name) => {
                names.push(name.clone());
                Some(OpenAiContentPart::Text {
                    text: placeholder_token(&name),
                })
            }
        })
        .collect()
}

fn split_openai_responses(request: &mut OpenAiResponsesRequest, names: &mut Vec<String>) {
    for item in &mut request.input {
        if let OpenAiResponseItem::Message { content, .. } = item {
            let mut out = Vec::new();
            for part in std::mem::take(content) {
                match part {
                    OpenAiResponseContentPart::InputText { text } => {
                        out.extend(openai_response_text_segments(
                            split_text_on_media(&text),
                            names,
                            false,
                        ));
                    }
                    OpenAiResponseContentPart::OutputText { text } => {
                        out.extend(openai_response_text_segments(
                            split_text_on_media(&text),
                            names,
                            true,
                        ));
                    }
                    other => out.push(other),
                }
            }
            *content = out;
        }
    }
}

fn openai_response_text_segments(
    segments: Vec<TextSegment>,
    names: &mut Vec<String>,
    output: bool,
) -> Vec<OpenAiResponseContentPart> {
    segments
        .into_iter()
        .filter_map(|segment| match segment {
            TextSegment::Text(text) if text.is_empty() => None,
            TextSegment::Text(text) if output => {
                Some(OpenAiResponseContentPart::OutputText { text })
            }
            TextSegment::Text(text) => Some(OpenAiResponseContentPart::InputText { text }),
            TextSegment::Placeholder(name) => {
                names.push(name.clone());
                let text = placeholder_token(&name);
                if output {
                    Some(OpenAiResponseContentPart::OutputText { text })
                } else {
                    Some(OpenAiResponseContentPart::InputText { text })
                }
            }
        })
        .collect()
}

fn split_anthropic(request: &mut AnthropicMessagesRequest, names: &mut Vec<String>) {
    for message in &mut request.messages {
        let mut out = Vec::new();
        for block in std::mem::take(&mut message.content) {
            match block {
                AnthropicContentBlock::Text {
                    text,
                    cache_control,
                    citations,
                } => out.extend(anthropic_text_segments(
                    split_text_on_media(&text),
                    names,
                    cache_control,
                    citations,
                )),
                other => out.push(other),
            }
        }
        message.content = out;
    }
}

fn anthropic_text_segments(
    segments: Vec<TextSegment>,
    names: &mut Vec<String>,
    cache_control: Option<crate::wire::anthropic_messages::AnthropicCacheControl>,
    citations: Option<Vec<crate::wire::anthropic_citation::AnthropicCitationV1>>,
) -> Vec<AnthropicContentBlock> {
    segments
        .into_iter()
        .filter_map(|segment| match segment {
            TextSegment::Text(text) if text.is_empty() => None,
            TextSegment::Text(text) => Some(AnthropicContentBlock::Text {
                text,
                cache_control: cache_control.clone(),
                citations: citations.clone(),
            }),
            TextSegment::Placeholder(name) => {
                names.push(name.clone());
                Some(AnthropicContentBlock::Text {
                    text: placeholder_token(&name),
                    cache_control: None,
                    citations: None,
                })
            }
        })
        .collect()
}

fn split_google(request: &mut GoogleGenerateContentRequest, names: &mut Vec<String>) {
    for content in &mut request.contents {
        let mut out = Vec::new();
        for part in std::mem::take(&mut content.parts) {
            match part {
                GooglePart::Text { text } => {
                    out.extend(google_text_segments(split_text_on_media(&text), names));
                }
                other => out.push(other),
            }
        }
        content.parts = out;
    }
}

fn google_text_segments(segments: Vec<TextSegment>, names: &mut Vec<String>) -> Vec<GooglePart> {
    segments
        .into_iter()
        .filter_map(|segment| match segment {
            TextSegment::Text(text) if text.is_empty() => None,
            TextSegment::Text(text) => Some(GooglePart::Text { text }),
            TextSegment::Placeholder(name) => {
                names.push(name.clone());
                Some(GooglePart::Text {
                    text: placeholder_token(&name),
                })
            }
        })
        .collect()
}

fn has_placeholder(segments: &[TextSegment]) -> bool {
    segments
        .iter()
        .any(|segment| matches!(segment, TextSegment::Placeholder(_)))
}

fn bind_media_openai_chat(
    request: &mut OpenAiChatRequest,
    name: &str,
    media: &MediaRef,
) -> SkaldResult<()> {
    let sentinel = placeholder_token(name);
    let mut found = false;
    for message in &mut request.messages {
        if message.role == "system" {
            continue;
        }
        let Some(OpenAiMessageContent::Parts(parts)) = &mut message.content else {
            continue;
        };
        for part in parts {
            let is_match =
                matches!(part, OpenAiContentPart::Text { text } if text.trim() == sentinel);
            if !is_match {
                continue;
            }
            if matches!(part, OpenAiContentPart::Text { text } if text != &sentinel) {
                return Err(SkaldError::MediaPlaceholderNotIsolated {
                    name: name.to_owned(),
                });
            }
            *part = build_openai_chat_part(media)?;
            found = true;
        }
    }
    found_or_missing(found, name)
}

fn bind_media_openai_responses(
    request: &mut OpenAiResponsesRequest,
    name: &str,
    media: &MediaRef,
) -> SkaldResult<()> {
    let sentinel = placeholder_token(name);
    let mut found = false;
    for item in &mut request.input {
        let OpenAiResponseItem::Message { content, .. } = item else {
            continue;
        };
        for part in content {
            let text = match part {
                OpenAiResponseContentPart::InputText { text }
                | OpenAiResponseContentPart::OutputText { text } => text,
                _ => continue,
            };
            if text.trim() != sentinel {
                continue;
            }
            if text != &sentinel {
                return Err(SkaldError::MediaPlaceholderNotIsolated {
                    name: name.to_owned(),
                });
            }
            *part = build_openai_response_part(media)?;
            found = true;
        }
    }
    found_or_missing(found, name)
}

fn bind_media_anthropic(
    request: &mut AnthropicMessagesRequest,
    name: &str,
    media: &MediaRef,
) -> SkaldResult<()> {
    let sentinel = placeholder_token(name);
    let mut found = false;
    for message in &mut request.messages {
        for block in &mut message.content {
            let is_match = matches!(block, AnthropicContentBlock::Text { text, .. } if text.trim() == sentinel);
            if !is_match {
                continue;
            }
            if matches!(block, AnthropicContentBlock::Text { text, .. } if text != &sentinel) {
                return Err(SkaldError::MediaPlaceholderNotIsolated {
                    name: name.to_owned(),
                });
            }
            *block = build_anthropic_block(media);
            found = true;
        }
    }
    found_or_missing(found, name)
}

fn bind_media_google(
    request: &mut GoogleGenerateContentRequest,
    name: &str,
    media: &MediaRef,
    provider: ProviderName,
) -> SkaldResult<()> {
    let sentinel = placeholder_token(name);
    let mut found = false;
    for content in &mut request.contents {
        for part in &mut content.parts {
            let is_match = matches!(part, GooglePart::Text { text } if text.trim() == sentinel);
            if !is_match {
                continue;
            }
            if matches!(part, GooglePart::Text { text } if text != &sentinel) {
                return Err(SkaldError::MediaPlaceholderNotIsolated {
                    name: name.to_owned(),
                });
            }
            *part = build_google_part(media, provider.clone())?;
            found = true;
        }
    }
    found_or_missing(found, name)
}

fn found_or_missing(found: bool, name: &str) -> SkaldResult<()> {
    if found {
        Ok(())
    } else {
        Err(SkaldError::MediaPlaceholderNotFound {
            name: name.to_owned(),
        })
    }
}

fn build_openai_chat_part(media: &MediaRef) -> SkaldResult<OpenAiContentPart> {
    Ok(match (media.kind, &media.source) {
        (MediaKind::Image, MediaSource::Url { url, .. }) => OpenAiContentPart::ImageUrl {
            image_url: OpenAiImageUrl {
                url: url.clone(),
                detail: None,
            },
        },
        (MediaKind::Image, MediaSource::Base64 { mime_type, data }) => {
            validate_mime(mime_type)?;
            OpenAiContentPart::ImageUrl {
                image_url: OpenAiImageUrl {
                    url: data_url(mime_type, data),
                    detail: None,
                },
            }
        }
        (MediaKind::Image, MediaSource::File { uri, .. }) => OpenAiContentPart::File {
            file: OpenAiFilePart {
                file_id: Some(uri.clone()),
                file_data: None,
                filename: None,
            },
        },
        (MediaKind::Document, MediaSource::Url { .. }) => {
            return Err(SkaldError::UnsupportedMediaForProvider {
                provider: ProviderName::OpenAi,
                kind: MediaKind::Document,
            });
        }
        (MediaKind::Document, MediaSource::Base64 { mime_type, data }) => {
            validate_mime(mime_type)?;
            OpenAiContentPart::File {
                file: OpenAiFilePart {
                    file_id: None,
                    file_data: Some(data_url(mime_type, data)),
                    filename: None,
                },
            }
        }
        (MediaKind::Document, MediaSource::File { uri, .. }) => OpenAiContentPart::File {
            file: OpenAiFilePart {
                file_id: Some(uri.clone()),
                file_data: None,
                filename: None,
            },
        },
    })
}

fn build_openai_response_part(media: &MediaRef) -> SkaldResult<OpenAiResponseContentPart> {
    Ok(match (media.kind, &media.source) {
        (MediaKind::Image, MediaSource::Url { url, .. }) => OpenAiResponseContentPart::InputImage {
            image_url: url.clone(),
            detail: None,
        },
        (MediaKind::Image, MediaSource::Base64 { mime_type, data }) => {
            validate_mime(mime_type)?;
            OpenAiResponseContentPart::InputImage {
                image_url: data_url(mime_type, data),
                detail: None,
            }
        }
        (_, MediaSource::File { uri, .. }) => OpenAiResponseContentPart::InputFile {
            file_id: uri.clone(),
        },
        (MediaKind::Document, MediaSource::Url { .. } | MediaSource::Base64 { .. }) => {
            return Err(SkaldError::UnsupportedMediaForProvider {
                provider: ProviderName::OpenAi,
                kind: MediaKind::Document,
            });
        }
    })
}

fn build_anthropic_block(media: &MediaRef) -> AnthropicContentBlock {
    match (media.kind, &media.source) {
        (MediaKind::Image, MediaSource::Url { url, .. }) => AnthropicContentBlock::Image {
            source: AnthropicImageSource::Url { url: url.clone() },
            cache_control: None,
        },
        (MediaKind::Image, MediaSource::Base64 { mime_type, data }) => {
            AnthropicContentBlock::Image {
                source: AnthropicImageSource::Base64 {
                    media_type: mime_type.clone(),
                    data: data.clone(),
                },
                cache_control: None,
            }
        }
        (MediaKind::Image, MediaSource::File { uri, .. }) => AnthropicContentBlock::Image {
            source: AnthropicImageSource::FileId {
                file_id: uri.clone(),
            },
            cache_control: None,
        },
        (MediaKind::Document, MediaSource::Url { url, .. }) => AnthropicContentBlock::Document {
            source: AnthropicDocumentSource::Url { url: url.clone() },
            cache_control: None,
            title: None,
            context: None,
            citations: None,
        },
        (MediaKind::Document, MediaSource::Base64 { mime_type, data }) => {
            AnthropicContentBlock::Document {
                source: AnthropicDocumentSource::Base64 {
                    media_type: mime_type.clone(),
                    data: data.clone(),
                },
                cache_control: None,
                title: None,
                context: None,
                citations: None,
            }
        }
        (MediaKind::Document, MediaSource::File { uri, .. }) => AnthropicContentBlock::Document {
            source: AnthropicDocumentSource::FileId {
                file_id: uri.clone(),
            },
            cache_control: None,
            title: None,
            context: None,
            citations: None,
        },
    }
}

fn build_google_part(media: &MediaRef, provider: ProviderName) -> SkaldResult<GooglePart> {
    match &media.source {
        MediaSource::Base64 { mime_type, data } => {
            validate_mime(mime_type)?;
            Ok(GooglePart::InlineData {
                inline_data: GoogleInlineData {
                    mime_type: mime_type.clone(),
                    data: data.clone(),
                },
            })
        }
        MediaSource::Url { url, mime_type } => {
            let mime_type =
                required_mime(mime_type, "Gemini URL media requires explicit mime_type")?;
            if url.starts_with("gs://") || is_gemini_file_api_url(url) {
                Ok(GooglePart::FileData {
                    file_data: GoogleFileData {
                        mime_type,
                        file_uri: url.clone(),
                    },
                })
            } else {
                Err(SkaldError::UnsupportedMediaForProvider {
                    provider,
                    kind: media.kind,
                })
            }
        }
        MediaSource::File { uri, mime_type } => {
            let mime_type =
                required_mime(mime_type, "Gemini file URI requires explicit mime_type")?;
            Ok(GooglePart::FileData {
                file_data: GoogleFileData {
                    mime_type,
                    file_uri: uri.clone(),
                },
            })
        }
    }
}

fn validate_mime(mime_type: &str) -> SkaldResult<()> {
    if mime_type.trim().is_empty() {
        Err(SkaldError::InvalidMediaType(
            "media source requires a non-empty mime_type".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn required_mime(mime_type: &Option<String>, message: &str) -> SkaldResult<String> {
    let Some(mime_type) = mime_type else {
        return Err(SkaldError::InvalidMediaType(message.to_owned()));
    };
    validate_mime(mime_type)?;
    Ok(mime_type.clone())
}

fn data_url(mime_type: &str, data: &str) -> String {
    format!("data:{mime_type};base64,{data}")
}

fn is_gemini_file_api_url(url: &str) -> bool {
    url.starts_with("https://generativelanguage.googleapis.com/v1/")
        || url.starts_with("https://generativelanguage.googleapis.com/v1beta/")
}

#[cfg(test)]
mod placeholder_syntax_tests {
    use super::*;
    use crate::request::ProviderRequest;
    use crate::wire::openai_chat::{OpenAiChatMessage, OpenAiChatRequest, OpenAiMessageContent};

    fn openai_request_with_user(text: &str) -> ProviderRequest {
        ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-test".into(),
            messages: vec![OpenAiChatMessage {
                role: "user".into(),
                content: Some(OpenAiMessageContent::Text(text.into())),
                ..Default::default()
            }],
            response_format: None,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings: Default::default(),
        })
    }

    #[test]
    fn extract_finds_dollar_brace() {
        let request = openai_request_with_user("hello ${name}");
        let vars = extract_text_variables(&request).expect("extracts");
        assert_eq!(vars, vec!["name".to_string()]);
    }

    #[test]
    fn extract_finds_mustache() {
        let request = openai_request_with_user("hello {{name}}");
        let vars = extract_text_variables(&request).expect("extracts");
        assert_eq!(vars, vec!["name".to_string()]);
    }

    #[test]
    fn extract_finds_mixed() {
        let request = openai_request_with_user("${a} and {{b}} and ${a}");
        let vars = extract_text_variables(&request).expect("extracts");
        assert_eq!(vars, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn extract_does_not_capture_media_prefix() {
        let request = openai_request_with_user("hello ${media:img}");
        let vars = extract_text_variables(&request).expect("extracts");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_does_not_capture_single_brace() {
        let request = openai_request_with_user("hello {name}");
        let vars = extract_text_variables(&request).expect("extracts");
        assert!(vars.is_empty());
    }

    #[test]
    fn render_binds_dollar_brace() {
        let prompt = Prompt::new(
            openai_request_with_user("hello ${name}"),
            "gpt-test",
            None,
            ResponseType::Text,
        )
        .expect("prompt builds");
        let rendered = prompt.render(&[("name", "world")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("hello world"));
    }

    #[test]
    fn render_binds_mustache() {
        let prompt = Prompt::new(
            openai_request_with_user("hello {{name}}"),
            "gpt-test",
            None,
            ResponseType::Text,
        )
        .expect("prompt builds");
        let rendered = prompt.render(&[("name", "world")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("hello world"));
    }

    #[test]
    fn render_binds_mixed_syntax_in_one_pass() {
        let prompt = Prompt::new(
            openai_request_with_user("${a} or {{a}}"),
            "gpt-test",
            None,
            ResponseType::Text,
        )
        .expect("prompt builds");
        let rendered = prompt.render(&[("a", "X")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("X or X"));
    }
}

#[cfg(test)]
mod prompt_media {
    use serde_json::json;

    use crate::common;
    use crate::wire::anthropic_messages::{AnthropicContentBlock, AnthropicDocumentSource};
    use crate::wire::google_generate::GooglePart;
    use crate::wire::openai_chat::{OpenAiContentPart, OpenAiMessageContent};
    use crate::{
        MediaKind, MediaRef, Prompt, ProviderName, ProviderRequest, ResponseType, SkaldError,
    };

    fn prompt(request: ProviderRequest) -> Prompt {
        Prompt::new(request, "model", None, ResponseType::Text).expect("prompt is valid")
    }

    fn openai_prompt(text: &str) -> Prompt {
        let mut request = common::openai_chat_request();
        request.messages.truncate(2);
        request.messages[0].content = Some(OpenAiMessageContent::Text("system".to_owned()));
        request.messages[1].content = Some(OpenAiMessageContent::Text(text.to_owned()));
        prompt(ProviderRequest::OpenAiChatCompletion(request))
    }

    fn anthropic_prompt(text: &str) -> Prompt {
        let mut request = common::anthropic_request();
        request.system = None;
        request.messages[0].content = vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }];
        prompt(ProviderRequest::AnthropicMessage(request))
    }

    fn google_prompt(text: &str) -> Prompt {
        let mut request = common::google_request();
        request.contents[0].parts = vec![GooglePart::Text {
            text: text.to_owned(),
        }];
        prompt(ProviderRequest::GeminiGenerateContent(request))
    }

    #[test]
    fn media_variables_populated_and_split_at_construction() {
        let prompt = openai_prompt("before ${media:logo} after ${media:logo}");
        assert_eq!(prompt.media_variables, vec!["logo"]);

        let ProviderRequest::OpenAiChatCompletion(request) = prompt.request else {
            panic!("expected OpenAI request");
        };
        let Some(OpenAiMessageContent::Parts(parts)) = &request.messages[1].content else {
            panic!("media placeholder should force native parts");
        };
        assert!(matches!(parts[0], OpenAiContentPart::Text { ref text } if text == "before "));
        assert!(
            matches!(parts[1], OpenAiContentPart::Text { ref text } if text == "${media:logo}")
        );
        assert!(matches!(parts[2], OpenAiContentPart::Text { ref text } if text == " after "));
        assert!(
            matches!(parts[3], OpenAiContentPart::Text { ref text } if text == "${media:logo}")
        );
    }

    #[test]
    fn media_placeholder_in_system_message_is_rejected() {
        let mut request = common::openai_chat_request();
        request.messages[0].content =
            Some(OpenAiMessageContent::Text("system ${media:x}".to_owned()));
        let err = Prompt::new(
            ProviderRequest::OpenAiChatCompletion(request),
            "model",
            None,
            ResponseType::Text,
        )
        .unwrap_err();
        assert_eq!(
            err,
            SkaldError::MediaInSystemMessage {
                name: "x".to_owned()
            }
        );
        assert_eq!(err.code(), "SKALD_SPEC_400_MEDIA_IN_SYSTEM_MESSAGE");
    }

    #[test]
    fn bind_media_replaces_all_openai_placeholders_and_preserves_text_variables() {
        let bound = openai_prompt("Hello {{name}} ${media:logo} ${media:logo}")
            .bind_media(
                "logo",
                &MediaRef::image_url("https://example.com/logo.png", None),
            )
            .unwrap();
        assert_eq!(bound.variables, vec!["name"]);
        assert!(bound.media_variables.is_empty());
        let ProviderRequest::OpenAiChatCompletion(request) = bound.request else {
            panic!("expected OpenAI request");
        };
        let Some(OpenAiMessageContent::Parts(parts)) = &request.messages[1].content else {
            panic!("expected parts");
        };
        assert!(matches!(parts[1], OpenAiContentPart::ImageUrl { .. }));
        assert!(matches!(parts[3], OpenAiContentPart::ImageUrl { .. }));
    }

    #[test]
    fn render_fails_when_media_variable_is_unbound() {
        let err = openai_prompt("${media:logo}").render(&[]).unwrap_err();
        assert_eq!(
            err,
            SkaldError::MissingMediaVariable {
                name: "logo".to_owned()
            }
        );
        assert_eq!(err.code(), "SKALD_SPEC_422_MISSING_MEDIA_VARIABLE");
    }

    #[test]
    fn missing_media_placeholder_returns_stable_error() {
        let err = openai_prompt("plain")
            .bind_media(
                "logo",
                &MediaRef::image_url("https://example.com/logo.png", None),
            )
            .unwrap_err();
        assert_eq!(
            err,
            SkaldError::MediaPlaceholderNotFound {
                name: "logo".to_owned()
            }
        );
        assert_eq!(err.code(), "SKALD_SPEC_422_MEDIA_PLACEHOLDER_NOT_FOUND");
    }

    #[test]
    fn openai_media_matrix() {
        let image = openai_prompt("${media:image}")
            .bind_media("image", &MediaRef::image_base64("image/png", "AAAA"))
            .unwrap();
        let ProviderRequest::OpenAiChatCompletion(image) = image.request else {
            panic!("expected OpenAI request");
        };
        let Some(OpenAiMessageContent::Parts(parts)) = &image.messages[1].content else {
            panic!("expected parts");
        };
        assert!(matches!(parts[0], OpenAiContentPart::ImageUrl { .. }));

        let document = openai_prompt("${media:doc}")
            .bind_media("doc", &MediaRef::document_base64("application/pdf", "BBBB"))
            .unwrap();
        let ProviderRequest::OpenAiChatCompletion(document) = document.request else {
            panic!("expected OpenAI request");
        };
        let Some(OpenAiMessageContent::Parts(parts)) = &document.messages[1].content else {
            panic!("expected parts");
        };
        assert!(matches!(parts[0], OpenAiContentPart::File { .. }));

        let err = openai_prompt("${media:doc}")
            .bind_media(
                "doc",
                &MediaRef::document_url("https://example.com/doc.pdf", None),
            )
            .unwrap_err();
        assert_eq!(
            err,
            SkaldError::UnsupportedMediaForProvider {
                provider: ProviderName::OpenAi,
                kind: MediaKind::Document,
            }
        );
    }

    #[test]
    fn anthropic_accepts_image_and_document_url_and_base64() {
        let image = anthropic_prompt("${media:image}")
            .bind_media(
                "image",
                &MediaRef::image_url("https://example.com/image.png", None),
            )
            .unwrap();
        let ProviderRequest::AnthropicMessage(image) = image.request else {
            panic!("expected Anthropic request");
        };
        assert!(matches!(
            image.messages[0].content[0],
            AnthropicContentBlock::Image { .. }
        ));

        let document = anthropic_prompt("${media:doc}")
            .bind_media("doc", &MediaRef::document_base64("application/pdf", "AAAA"))
            .unwrap();
        let ProviderRequest::AnthropicMessage(document) = document.request else {
            panic!("expected Anthropic request");
        };
        assert!(matches!(
            document.messages[0].content[0],
            AnthropicContentBlock::Document {
                source: AnthropicDocumentSource::Base64 { .. },
                ..
            }
        ));
    }

    #[test]
    fn google_media_matrix_and_rejections() {
        let inline = google_prompt("${media:image}")
            .bind_media("image", &MediaRef::image_base64("image/png", "AAAA"))
            .unwrap();
        let ProviderRequest::GeminiGenerateContent(inline) = inline.request else {
            panic!("expected Google request");
        };
        assert!(matches!(
            inline.contents[0].parts[0],
            GooglePart::InlineData { .. }
        ));

        let file = google_prompt("${media:image}")
            .bind_media(
                "image",
                &MediaRef::image_url("gs://bucket/image.png", Some("image/png".to_owned())),
            )
            .unwrap();
        let ProviderRequest::GeminiGenerateContent(file) = file.request else {
            panic!("expected Google request");
        };
        assert!(matches!(
            file.contents[0].parts[0],
            GooglePart::FileData { .. }
        ));

        let https = google_prompt("${media:image}")
            .bind_media(
                "image",
                &MediaRef::image_url(
                    "https://example.com/image.png",
                    Some("image/png".to_owned()),
                ),
            )
            .unwrap_err();
        assert_eq!(
            https,
            SkaldError::UnsupportedMediaForProvider {
                provider: ProviderName::Google,
                kind: MediaKind::Image,
            }
        );

        let missing_mime = google_prompt("${media:image}")
            .bind_media("image", &MediaRef::image_file("files/abc", None))
            .unwrap_err();
        assert!(matches!(missing_mime, SkaldError::InvalidMediaType(_)));
    }

    #[test]
    fn text_binding_does_not_touch_media_and_media_binding_does_not_touch_text() {
        let after_text = openai_prompt("Hi {{name}} ${media:logo}")
            .bind(&[("name", "Ada")])
            .unwrap();
        assert!(
            serde_json::to_value(&after_text.request)
                .unwrap()
                .to_string()
                .contains("${media:logo}")
        );

        let after_media = after_text
            .bind_media(
                "logo",
                &MediaRef::image_url("https://example.com/logo.png", None),
            )
            .unwrap();
        assert!(
            serde_json::to_value(&after_media.request)
                .unwrap()
                .to_string()
                .contains("Ada")
        );
    }

    #[test]
    fn serde_default_accepts_prompts_without_media_variables() {
        let value = json!({
            "request": common::openai_chat_request(),
            "model": "model",
            "variables": ["name"],
            "response_type": "text"
        });
        let prompt: Prompt = serde_json::from_value(value).unwrap();
        assert!(prompt.media_variables.is_empty());
    }
}

#[cfg(test)]
mod prompt_render {
    use serde_json::{Value, json};

    use crate::common;
    use crate::wire::anthropic_messages::AnthropicContentBlock;
    use crate::{Prompt, ProviderRequest, ResponseType, SkaldError};

    fn prompt(request: ProviderRequest, variables: Vec<&str>) -> Prompt {
        Prompt {
            request,
            model: "model".to_string(),
            version: None,
            variables: variables.into_iter().map(str::to_string).collect(),
            media_variables: Vec::new(),
            response_type: ResponseType::Text,
        }
    }

    #[test]
    fn render_substitutes_anthropic_variables() {
        let request = ProviderRequest::AnthropicMessage(common::anthropic_request());
        let rendered = prompt(request, vec!["topic", "name"])
            .render(&[("topic", "rules"), ("name", "world")])
            .unwrap();

        let ProviderRequest::AnthropicMessage(rendered) = rendered else {
            panic!("render should preserve provider variant");
        };
        let AnthropicContentBlock::Text { text, .. } = &rendered.messages[0].content[0] else {
            panic!("expected text block");
        };
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn render_repeated_reference_substitutes_all_occurrences() {
        let mut request = common::openai_chat_request();
        request.messages[0].content = Some(crate::wire::openai_chat::OpenAiMessageContent::Text(
            "{{name}} {{name}} {{name}}".to_string(),
        ));
        let rendered = prompt(ProviderRequest::OpenAiChatCompletion(request), vec!["name"])
            .render(&[("name", "Ada")])
            .unwrap();
        let json = serde_json::to_string(&rendered).unwrap();
        assert!(json.contains("Ada Ada Ada"));
    }

    #[test]
    fn render_no_variables_returns_request_unchanged() {
        let request = ProviderRequest::GeminiGenerateContent(common::google_request());
        let rendered = prompt(request.clone(), Vec::new()).render(&[]).unwrap();
        assert_eq!(rendered, request);
    }

    #[test]
    fn render_missing_variable_value_returns_skald_error() {
        let err = prompt(
            ProviderRequest::AnthropicMessage(common::anthropic_request()),
            vec!["name"],
        )
        .render(&[])
        .unwrap_err();
        assert_eq!(err, SkaldError::MissingVariable("name".to_string()));
        assert_eq!(err.code(), "SKALD_SPEC_422_MISSING_VARIABLE");
    }

    #[test]
    fn bind_returns_new_prompt_and_tracks_remaining_variables() {
        let request = ProviderRequest::AnthropicMessage(common::anthropic_request());
        let original = prompt(request, vec!["name", "topic"]);
        let bound = original.bind(&[("name", "Ada")]).unwrap();

        assert_eq!(original.variables, vec!["name", "topic"]);
        assert_eq!(bound.variables, vec!["topic"]);
        let ProviderRequest::AnthropicMessage(request) = &bound.request else {
            panic!("bind should preserve provider variant");
        };
        let AnthropicContentBlock::Text { text, .. } = &request.messages[0].content[0] else {
            panic!("expected text block");
        };
        assert_eq!(text, "Hello Ada");
    }

    #[test]
    fn bind_mut_supports_incremental_parameter_injection() {
        let request = ProviderRequest::OpenAiChatCompletion(common::openai_chat_request());
        let mut prompt = prompt(request, vec!["name"]);
        prompt.bind_mut(&[("name", "Grace")]).unwrap();

        assert!(prompt.variables.is_empty());
        let rendered = prompt.render(&[]).unwrap();
        assert!(serde_json::to_string(&rendered).unwrap().contains("Grace"));
    }

    #[test]
    fn render_escapes_json_special_characters_and_blocks_injection() {
        let request = ProviderRequest::OpenAiChatCompletion(common::openai_chat_request());
        let rendered = prompt(request, vec!["name"])
            .render(&[("name", "\"quoted\", \"new_field\": \"x {{still_text}}")])
            .unwrap();
        let value = serde_json::to_value(&rendered).unwrap();
        assert_eq!(value.get("new_field"), None);
        assert!(
            serde_json::to_string(&rendered)
                .unwrap()
                .contains("\\\"quoted\\\", \\\"new_field\\\": \\\"x {{still_text}}")
        );
    }

    #[test]
    fn dollar_syntax_is_bound() {
        let mut request = common::google_request();
        if let crate::wire::google_generate::GooglePart::Text { text } =
            &mut request.contents[0].parts[0]
        {
            *text = format!("Hello ${{{}}}", "name");
        }
        let rendered = prompt(
            ProviderRequest::GeminiGenerateContent(request),
            vec!["name"],
        )
        .render(&[("name", "Ada")])
        .unwrap();
        let value: Value = serde_json::to_value(rendered).unwrap();
        assert!(value.to_string().contains("Hello Ada"));
    }

    #[test]
    fn render_same_type_in_same_type_out_for_all_live_requests() {
        assert!(matches!(
            prompt(
                ProviderRequest::OpenAiChatCompletion(common::openai_chat_request()),
                vec!["name"]
            )
            .render(&[("name", "Ada")])
            .unwrap(),
            ProviderRequest::OpenAiChatCompletion(_)
        ));
        assert!(matches!(
            prompt(
                ProviderRequest::OpenAiResponses(common::openai_responses_request()),
                vec!["name"]
            )
            .render(&[("name", "Ada")])
            .unwrap(),
            ProviderRequest::OpenAiResponses(_)
        ));
        assert!(matches!(
            prompt(
                ProviderRequest::GeminiGenerateContent(common::google_request()),
                vec!["name"]
            )
            .render(&[("name", "Ada")])
            .unwrap(),
            ProviderRequest::GeminiGenerateContent(_)
        ));
    }

    #[test]
    fn response_type_json_schema_roundtrips() {
        let response_type = ResponseType::JsonSchema {
            name: "answer".to_string(),
            schema: json!({"type": "object"}),
        };
        let json = serde_json::to_string(&response_type).unwrap();
        assert_eq!(
            serde_json::from_str::<ResponseType>(&json).unwrap(),
            response_type
        );
    }
}
