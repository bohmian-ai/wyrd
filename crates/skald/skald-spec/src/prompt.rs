use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring::{AuthoredPrompt, invalid};
use crate::error::{SkaldError, SkaldResult};
use crate::media::{MAX_MEDIA_FILE_BYTES, MediaKind, MediaRef, MediaSource};
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
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesInput, OpenAiResponsesRequest,
    OpenAiResponsesSettings,
};

/// Runtime Prompt: the compiled provider request plus derived render metadata.
///
/// A Prompt serializes as, and deserializes from, its [`AuthoredPrompt`]
/// form; decoding compiles that form. The request, variables, media
/// variables, and response type are derived and never stored.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Native provider-shaped request with `${variable}` or `{{variable}}` placeholders.
    pub request: ProviderRequest,
    /// Provider model identifier.
    pub model: String,
    /// Text placeholder names `render` requires, derived from the request.
    pub variables: Vec<String>,
    /// Media placeholder names `bind_media` fills, derived from the request.
    pub media_variables: Vec<String>,
    /// Expected response shape, derived from the request's output field.
    pub response_type: ResponseType,
    /// Native dispatch target, such as Vertex for a GenerateContent body.
    ///
    /// When absent, native dispatch uses the request dialect's default
    /// provider. Gateway routes select their own upstream and ignore it.
    pub provider: Option<ProviderName>,
    /// The authored form this Prompt was compiled from, when it still
    /// describes the request; absent after construction from a request or a
    /// binding, when the native form is derived from the request instead.
    pub(crate) authored: Option<Box<AuthoredPrompt>>,
    /// Media kind each portable `${media:name}` binding requires, derived
    /// from its image or document block at compile time. Native placeholders
    /// carry no kind and are absent; the list survives text and media binds.
    pub(crate) media_kinds: Vec<(String, MediaKind)>,
}

/// Prompts are equal when they serialize to the same authored form; a
/// request with no authored form compares by request, model, and provider.
impl PartialEq for Prompt {
    /// Compares authored forms when both derive one, falling back to the
    /// request, model, and dispatch provider.
    fn eq(&self, other: &Self) -> bool {
        match (self.authored(), other.authored()) {
            (Ok(mine), Ok(theirs)) => mine == theirs,
            _ => {
                self.request == other.request
                    && self.model == other.model
                    && self.provider() == other.provider()
            }
        }
    }
}

impl Serialize for Prompt {
    /// Serializes the authored form from [`Prompt::authored`].
    ///
    /// # Errors
    ///
    /// Returns the serializer's custom error when the request kind has no
    /// Prompt format, or the serializer's own error.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.authored()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Prompt {
    /// Decodes an [`AuthoredPrompt`] and compiles it.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error for an undecodable authored form and
    /// a custom error carrying the compiler refusal otherwise.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        AuthoredPrompt::deserialize(deserializer)?
            .compile()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for Prompt {
    /// Names the schema `Prompt`.
    fn schema_name() -> String {
        "Prompt".to_owned()
    }

    /// Publishes the authored form's schema, which is the wire form.
    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        AuthoredPrompt::json_schema(generator)
    }
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
///
/// # Panics
///
/// Panics if the static text placeholder pattern fails to compile.
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
///
/// # Panics
///
/// Panics if the static media placeholder pattern fails to compile.
pub fn media_placeholder_regex() -> &'static Regex {
    static MEDIA_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    MEDIA_PLACEHOLDER_RE.get_or_init(
        || match Regex::new(r"\$\{media:([a-zA-Z_][a-zA-Z0-9_]*)\}") {
            Ok(regex) => regex,
            Err(error) => panic!("media placeholder regex is static and valid: {error}"),
        },
    )
}

/// Refuses a model identifier that is not literal configuration.
///
/// The model names the dispatch target, so it must never be a template:
/// a `${...}` or `{{...}}` opener would make it a render variable that
/// [`Prompt::bind_mut`] rewrites. Every other nonempty, control-free
/// spelling, including slash-qualified ids, is accepted verbatim; no model
/// catalog is consulted.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for field `model` when the model is
/// blank, holds a control character, or holds placeholder syntax.
pub fn check_model(model: &str) -> SkaldResult<()> {
    let message = if model.trim().is_empty() {
        "must be a nonempty provider model id"
    } else if model.chars().any(char::is_control) {
        "must not contain control characters"
    } else if model.contains("${") || model.contains("{{") {
        "is a literal model id; `${name}` and `{{name}}` placeholders are not allowed"
    } else {
        return Ok(());
    };
    Err(invalid("model", message))
}

/// JSON Schema primitive type names an output schema may declare.
const SCHEMA_TYPES: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];

/// Refuses an output schema that is not structurally valid JSON Schema or
/// that uses a feature its destination statically cannot carry.
///
/// The walk is pure and never resolves references or fetches anything. It
/// checks `type` names, `required` string arrays, `properties`/`$defs`
/// object maps, and recurses through every nested subschema keyword. When
/// `object_root` is set (OpenAI strict structured output, which every
/// portable OpenAI schema uses) the root must declare `type: object`. Valid
/// schemas are never rewritten.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] whose field is `field` extended with
/// the dotted path to the first offending keyword.
pub(crate) fn check_output_schema(
    schema: &serde_json::Map<String, Value>,
    field: &str,
    object_root: bool,
) -> SkaldResult<()> {
    if object_root && schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(invalid(
            &format!("{field}.type"),
            "strict OpenAI structured output requires a root `type: object` schema",
        ));
    }
    check_schema(schema, field)
}

/// Walks one object schema for [`check_output_schema`].
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] at the dotted path of the first
/// invalid `type`, `required`, schema map, schema list, subschema, or `enum`.
fn check_schema(schema: &serde_json::Map<String, Value>, field: &str) -> SkaldResult<()> {
    // ponytail: structural checks only; swap in a JSON Schema meta-validator
    // if skald-spec ever earns that dependency.
    for (keyword, value) in schema {
        let at = format!("{field}.{keyword}");
        match keyword.as_str() {
            "type" => {
                let valid = |name: &Value| {
                    name.as_str()
                        .is_some_and(|name| SCHEMA_TYPES.contains(&name))
                };
                let ok = match value {
                    Value::Array(names) => !names.is_empty() && names.iter().all(valid),
                    name => valid(name),
                };
                if !ok {
                    return Err(invalid(
                        &at,
                        "type is one JSON Schema type name, or a nonempty array of them",
                    ));
                }
            }
            "required" => {
                if !value
                    .as_array()
                    .is_some_and(|names| names.iter().all(Value::is_string))
                {
                    return Err(invalid(&at, "required is an array of property names"));
                }
            }
            "properties" | "$defs" | "definitions" | "patternProperties" => {
                let Some(map) = value.as_object() else {
                    return Err(invalid(&at, "expected an object mapping names to schemas"));
                };
                for (name, subschema) in map {
                    check_subschema(subschema, &format!("{at}.{name}"))?;
                }
            }
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                let Some(list) = value.as_array().filter(|list| !list.is_empty()) else {
                    return Err(invalid(&at, "expected a nonempty array of schemas"));
                };
                for (index, subschema) in list.iter().enumerate() {
                    check_subschema(subschema, &format!("{at}.{index}"))?;
                }
            }
            "items" | "additionalProperties" | "not" | "contains" | "propertyNames" => {
                check_subschema(value, &at)?;
            }
            "enum" if !value.is_array() => {
                return Err(invalid(&at, "enum is an array of values"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Checks one nested subschema: a boolean schema, or an object schema walked
/// by [`check_schema`].
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] at `field` for any other JSON value,
/// and the nested refusal of an invalid object schema.
fn check_subschema(schema: &Value, field: &str) -> SkaldResult<()> {
    match schema {
        Value::Bool(_) => Ok(()),
        Value::Object(schema) => check_schema(schema, field),
        _ => Err(invalid(field, "expected a JSON Schema object or boolean")),
    }
}

/// Native body path of a request's output schema, and whether its
/// destination requires an object root (OpenAI with `strict: true`).
///
/// Only the four kinds [`response_type_of`] derives schemas from reach here.
fn output_schema_field(request: &ProviderRequest) -> (&'static str, bool) {
    use crate::wire::openai_chat::OpenAiResponseFormat;
    use crate::wire::openai_responses::OpenAiTextResponseFormat;

    match request {
        ProviderRequest::OpenAiChatCompletion(request) => (
            "body.response_format.json_schema.schema",
            matches!(&request.response_format, Some(OpenAiResponseFormat::JsonSchema { json_schema }) if json_schema.strict == Some(true)),
        ),
        ProviderRequest::OpenAiResponses(request) => (
            "body.text.format.schema",
            matches!(
                request.text.as_ref().and_then(|text| text.format.as_ref()),
                Some(OpenAiTextResponseFormat::JsonSchema {
                    strict: Some(true),
                    ..
                })
            ),
        ),
        ProviderRequest::AnthropicMessage(_) => ("body.output_config.format.schema", false),
        _ => ("body.generation_config.response_schema", false),
    }
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
    /// Create a prompt from a request, split media placeholders into
    /// isolated native text parts, and derive `variables`, `media_variables`,
    /// and `response_type` from the request.
    ///
    /// The request carries only the body schema, so the created Prompt has no
    /// destination (`provider` is `None`) and native dispatch uses the
    /// schema's default provider. A caller that targets a different
    /// destination, such as Vertex for a shared GenerateContent body, sets
    /// `provider` after construction.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] for field `model` when the model
    /// is not literal configuration (see [`check_model`]) and at the output
    /// schema's native body path when that schema is invalid for its
    /// destination (see [`check_output_schema`]),
    /// [`SkaldError::MediaInSystemMessage`] when system content holds a
    /// `${media:name}` placeholder, and a serialization error when the request
    /// cannot be serialized to scan its `${name}` / `{{name}}` variables.
    pub fn new(request: ProviderRequest, model: impl Into<String>) -> SkaldResult<Self> {
        let model = model.into();
        check_model(&model)?;
        let response_type = response_type_of(&request);
        if let ResponseType::JsonSchema { schema, .. } = &response_type {
            let (field, object_root) = output_schema_field(&request);
            let schema = schema
                .as_object()
                .ok_or_else(|| invalid(field, "an output schema is a JSON object"))?;
            check_output_schema(schema, field, object_root)?;
        }
        let mut prompt = Self {
            request,
            model,
            variables: Vec::new(),
            media_variables: Vec::new(),
            response_type,
            provider: None,
            authored: None,
            media_kinds: Vec::new(),
        };
        prompt.normalize_media_placeholders_mut()?;
        prompt.variables = extract_text_variables(&prompt.request)?;
        Ok(prompt)
    }

    /// Return the authored form this Prompt serializes as.
    ///
    /// This is the form it was compiled from while that form still compiles
    /// to the current request, model, and provider; otherwise, such as after a
    /// caller mutated the public fields, it is the native form derived from
    /// the request.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] when the request kind is not a
    /// Prompt format, such as embeddings or a raw body.
    pub fn authored(&self) -> SkaldResult<std::borrow::Cow<'_, AuthoredPrompt>> {
        // ponytail: recompiles to detect public-field mutation; make the
        // fields private if Prompt serialization ever shows up in a profile.
        match self.authored.as_deref() {
            Some(authored)
                if authored.compile().is_ok_and(|compiled| {
                    compiled.request == self.request
                        && compiled.model == self.model
                        && compiled.provider == self.provider
                }) =>
            {
                Ok(std::borrow::Cow::Borrowed(authored))
            }
            _ => AuthoredPrompt::native(self.provider(), &self.model, &self.request)
                .map(std::borrow::Cow::Owned),
        }
    }

    /// Return the provider native dispatch sends this Prompt to.
    ///
    /// This is [`Prompt::provider`](Self::provider) when set, otherwise the
    /// request dialect's default from [`ProviderRequest::provider`].
    pub fn provider(&self) -> ProviderName {
        self.provider
            .clone()
            .unwrap_or_else(|| self.request.provider())
    }

    /// The media kind a portable `${media:name}` binding requires, from its
    /// image or document block; `None` for an unknown name or a native
    /// placeholder, which carries no kind.
    #[must_use]
    pub fn media_kind(&self, name: &str) -> Option<MediaKind> {
        self.media_kinds
            .iter()
            .find(|(bound, _)| bound == name)
            .map(|(_, kind)| *kind)
    }

    /// Whether every answer to this Prompt is a structured JSON object.
    ///
    /// True when the Prompt declares a JSON-schema response, or when its
    /// request dialect answers with a typed object by protocol
    /// ([`ProviderRequest::answers_structured`]). Callers that require a
    /// structured judgment check this instead of naming a provider.
    pub const fn yields_structured_output(&self) -> bool {
        matches!(self.response_type, ResponseType::JsonSchema { .. })
            || self.request.answers_structured()
    }

    /// Borrow native generation settings for the request's active provider.
    pub fn settings_ref(&self) -> Option<ProviderSettingsRef<'_>> {
        match &self.request {
            ProviderRequest::OpenAiChatCompletion(request) => {
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
            ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_)
            | ProviderRequest::JevSystemOne(_)
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
    /// request, then deserializes back into the same provider-tagged variant.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Prompt::bind_mut`].
    pub fn bind(&self, vars: &[(&str, &str)]) -> SkaldResult<Self> {
        let mut prompt = self.clone();
        prompt.bind_mut(vars)?;
        Ok(prompt)
    }

    /// Bind supplied `${name}` or `{{name}}` placeholders into this prompt in place.
    ///
    /// Unlike `render`, this may bind only a subset of declared variables. Any
    /// names still present in `variables` remain required for a later render.
    /// The model is literal (see [`check_model`]), so binding never rewrites
    /// the dispatch target.
    ///
    /// # Errors
    ///
    /// Returns a serialization error when the request cannot be serialized
    /// and a deserialization error when the substituted request no longer
    /// decodes as its provider variant.
    pub fn bind_mut(&mut self, vars: &[(&str, &str)]) -> SkaldResult<()> {
        let mut request = serde_json::to_value(&self.request).map_err(SkaldError::serialize)?;
        replace_string_leaves(&mut request, vars);
        self.request = serde_json::from_value(request).map_err(SkaldError::deserialize)?;
        self.authored = None;
        self.variables
            .retain(|name| !vars.iter().any(|(key, _)| key == name));
        Ok(())
    }

    /// Bind a `${media:name}` placeholder to a provider-native media block.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Prompt::bind_media_mut`].
    pub fn bind_media(&self, name: &str, media: &MediaRef) -> SkaldResult<Self> {
        let mut prompt = self.clone();
        prompt.bind_media_mut(name, media)?;
        Ok(prompt)
    }

    /// Bind a `${media:name}` placeholder in place.
    ///
    /// The replacement preserves the request provider shape and inserts native
    /// media blocks such as `OpenAiContentPart::ImageUrl`,
    /// `AnthropicContentBlock::Document`, or `GooglePart::InlineData`. A
    /// portable binding's declared kind is checked first, so a refusal leaves
    /// the request unchanged; native placeholders accept either kind.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::InvalidMediaType`] when `media` is not the kind
    /// the portable block for `name` declares,
    /// [`SkaldError::MediaPlaceholderNotFound`] when the request has no such
    /// placeholder or no media-capable shape, and the source refusals of
    /// [`check_media_source`] (checked before any part is replaced) plus the
    /// remaining per-provider media errors.
    pub fn bind_media_mut(&mut self, name: &str, media: &MediaRef) -> SkaldResult<()> {
        if let Some((_, expected)) = self.media_kinds.iter().find(|(bound, _)| bound == name)
            && *expected != media.kind
        {
            return Err(SkaldError::InvalidMediaType(format!(
                "media `{name}` is declared as {expected:?} but was bound as {:?}",
                media.kind
            )));
        }
        let provider = self.provider();
        check_media_source(&self.request, provider.clone(), name, media)?;
        self.normalize_media_placeholders_mut()?;
        match &mut self.request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                bind_media_openai_chat(request, name, media)?;
            }
            ProviderRequest::OpenAiResponses(request) => {
                bind_media_openai_responses(request, name, media)?;
            }
            ProviderRequest::AnthropicMessage(request) => {
                bind_media_anthropic(request, name, media)?;
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                bind_media_google(request, name, media, provider)?;
            }
            ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_)
            | ProviderRequest::JevSystemOne(_)
            | ProviderRequest::RawV1 { .. } => {
                return Err(SkaldError::MediaPlaceholderNotFound {
                    name: name.to_owned(),
                });
            }
        }
        self.media_variables.retain(|declared| declared != name);
        self.authored = None;
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

/// Derives the expected response shape from a request's structured-output field.
///
/// Portable schemas carry the name `response`; Anthropic and GenerateContent
/// declare no schema name, so they use the same one.
fn response_type_of(request: &ProviderRequest) -> ResponseType {
    use crate::wire::anthropic_messages::AnthropicOutputFormat;
    use crate::wire::openai_chat::OpenAiResponseFormat;
    use crate::wire::openai_responses::OpenAiTextResponseFormat;

    let named = |name: &str, schema: Value| ResponseType::JsonSchema {
        name: name.to_owned(),
        schema,
    };
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => match &request.response_format {
            Some(OpenAiResponseFormat::JsonSchema { json_schema }) => json_schema
                .schema
                .clone()
                .map_or(ResponseType::Text, |schema| {
                    named(&json_schema.name, Value::Object(schema))
                }),
            _ => ResponseType::Text,
        },
        ProviderRequest::OpenAiResponses(request) => {
            match request.text.as_ref().and_then(|text| text.format.as_ref()) {
                Some(OpenAiTextResponseFormat::JsonSchema {
                    name,
                    schema: Some(schema),
                    ..
                }) => named(name, Value::Object(schema.clone())),
                _ => ResponseType::Text,
            }
        }
        ProviderRequest::AnthropicMessage(request) => match &request.output_config {
            Some(config) => match &config.format {
                AnthropicOutputFormat::JsonSchema { schema } => named("response", schema.clone()),
            },
            None => ResponseType::Text,
        },
        ProviderRequest::GeminiGenerateContent(request) => request
            .settings
            .generation_config
            .as_ref()
            .and_then(|config| config.response_schema.clone())
            .map_or(ResponseType::Text, |schema| named("response", schema)),
        _ => ResponseType::Text,
    }
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
        ProviderRequest::OpenAiChatCompletion(request) => {
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
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::JevSystemOne(_)
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
        ProviderRequest::OpenAiChatCompletion(request) => {
            split_openai_chat(request, &mut names);
        }
        ProviderRequest::OpenAiResponses(request) => split_openai_responses(request, &mut names),
        ProviderRequest::AnthropicMessage(request) => split_anthropic(request, &mut names),
        ProviderRequest::GeminiGenerateContent(request) => split_google(request, &mut names),
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::JevSystemOne(_)
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

/// Splits media placeholders out of every Responses text part.
///
/// Text shorthand input is expanded into its user message only when it holds a
/// placeholder, so input without media keeps the form it was authored in.
fn split_openai_responses(request: &mut OpenAiResponsesRequest, names: &mut Vec<String>) {
    if matches!(&request.input, OpenAiResponsesInput::Text(text) if !media_placeholder_regex().is_match(text))
    {
        return;
    }
    for item in request.input.items_mut() {
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

/// Replaces the isolated `name` placeholder part in Responses message items
/// with `media`.
///
/// Text shorthand input has no split-out placeholder part, so it binds
/// nothing and reports the placeholder missing.
///
/// # Errors
///
/// Returns [`SkaldError::MediaPlaceholderNotIsolated`] when the placeholder
/// shares its part with other text, a media build error, or the missing
/// placeholder error when no part names `name`.
fn bind_media_openai_responses(
    request: &mut OpenAiResponsesRequest,
    name: &str,
    media: &MediaRef,
) -> SkaldResult<()> {
    let sentinel = placeholder_token(name);
    let mut found = false;
    // Text shorthand holds no split-out placeholder part to bind.
    let OpenAiResponsesInput::Items(items) = &mut request.input else {
        return found_or_missing(found, name);
    };
    for item in items {
        let OpenAiResponseItem::Message { content, .. } = item else {
            continue;
        };
        for part in content {
            let (OpenAiResponseContentPart::InputText { text }
            | OpenAiResponseContentPart::OutputText { text }) = part
            else {
                continue;
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
        (MediaKind::Image, MediaSource::File { uri, .. })
        | (MediaKind::Document, MediaSource::File { uri, .. }) => OpenAiContentPart::File {
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
            let mime_type = required_mime(
                mime_type.as_deref(),
                "Gemini URL media requires explicit mime_type",
            )?;
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
            let mime_type = required_mime(
                mime_type.as_deref(),
                "Gemini file URI requires explicit mime_type",
            )?;
            Ok(GooglePart::FileData {
                file_data: GoogleFileData {
                    mime_type,
                    file_uri: uri.clone(),
                },
            })
        }
    }
}

/// Image MIME types OpenAI and Anthropic accept as image input.
const PORTABLE_IMAGE_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

/// Refuses a media source its destination cannot carry, before any request
/// part is replaced.
///
/// Inline base64 must decode and stay within [`MAX_MEDIA_FILE_BYTES`]; any
/// declared MIME type must be `type/subtype` and match the kind (`image/*`
/// for images, anything else for documents). OpenAI and Anthropic images
/// must be GIF, JPEG, PNG, or WebP; Anthropic inline documents must be PDF;
/// GenerateContent file URIs must be `gs://` or Gemini File API URIs.
/// Provider file ids are otherwise passed through unexamined. Pure: nothing
/// is fetched or uploaded.
///
/// # Errors
///
/// Returns [`SkaldError::InvalidMediaType`] for undecodable base64 or an
/// invalid, mismatched, or provider-unsupported MIME type,
/// [`SkaldError::MediaTooLarge`] for an over-budget payload, and
/// [`SkaldError::UnsupportedMediaForProvider`] for a refused Google file URI.
fn check_media_source(
    request: &ProviderRequest,
    provider: ProviderName,
    name: &str,
    media: &MediaRef,
) -> SkaldResult<()> {
    use base64::Engine;

    let invalid_type = |message: String| Err(SkaldError::InvalidMediaType(message));
    let mime_type = match &media.source {
        MediaSource::Base64 { mime_type, data } => {
            // Text longer than any in-budget encoding is refused undecoded.
            let encoded = data.len() as u64;
            let size = if encoded > MAX_MEDIA_FILE_BYTES.div_ceil(3) * 4 {
                encoded / 4 * 3
            } else {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| {
                        SkaldError::InvalidMediaType(format!(
                            "media `{name}` data is not valid base64"
                        ))
                    })?
                    .len() as u64
            };
            if size > MAX_MEDIA_FILE_BYTES {
                return Err(SkaldError::MediaTooLarge {
                    path: placeholder_token(name),
                    size,
                    limit: MAX_MEDIA_FILE_BYTES,
                });
            }
            Some(mime_type.as_str())
        }
        MediaSource::Url { mime_type, .. } | MediaSource::File { mime_type, .. } => {
            mime_type.as_deref()
        }
    };
    if let Some(mime_type) = mime_type {
        let mime = mime_type.to_ascii_lowercase();
        let well_formed = mime.split_once('/').is_some_and(|(top, sub)| {
            !top.is_empty()
                && !sub.is_empty()
                && !mime.contains(|c: char| c.is_whitespace() || c.is_control())
        });
        if !well_formed {
            return invalid_type(format!(
                "media `{name}` mime_type `{mime_type}` is not type/subtype"
            ));
        }
        if mime.starts_with("image/") != (media.kind == MediaKind::Image) {
            return invalid_type(format!(
                "media `{name}` mime_type `{mime_type}` does not match kind {:?}",
                media.kind
            ));
        }
        let anthropic = matches!(request, ProviderRequest::AnthropicMessage(_));
        let openai = matches!(
            request,
            ProviderRequest::OpenAiChatCompletion(_) | ProviderRequest::OpenAiResponses(_)
        );
        if media.kind == MediaKind::Image
            && (anthropic || openai)
            && !PORTABLE_IMAGE_TYPES.contains(&mime.as_str())
        {
            return invalid_type(format!(
                "media `{name}` image type `{mime_type}` is not GIF, JPEG, PNG, or WebP"
            ));
        }
        if anthropic
            && media.kind == MediaKind::Document
            && matches!(media.source, MediaSource::Base64 { .. })
            && mime != "application/pdf"
        {
            return invalid_type(format!(
                "media `{name}` inline Anthropic documents must be application/pdf, not `{mime_type}`"
            ));
        }
    }
    let google = matches!(request, ProviderRequest::GeminiGenerateContent(_));
    if google && mime_type.is_none() {
        return invalid_type(format!(
            "media `{name}` needs an explicit mime_type for Gemini"
        ));
    }
    if let (true, MediaSource::File { uri, .. }) = (google, &media.source)
        && !(uri.starts_with("gs://") || is_gemini_file_api_url(uri))
    {
        return Err(SkaldError::UnsupportedMediaForProvider {
            provider,
            kind: media.kind,
        });
    }
    Ok(())
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

fn required_mime(mime_type: Option<&str>, message: &str) -> SkaldResult<String> {
    let Some(mime_type) = mime_type else {
        return Err(SkaldError::InvalidMediaType(message.to_owned()));
    };
    validate_mime(mime_type)?;
    Ok(mime_type.to_owned())
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
            settings: OpenAiChatSettings::default(),
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

    /// Proves `${name}` placeholders render.
    #[test]
    fn render_binds_dollar_brace() {
        let prompt = Prompt::new(openai_request_with_user("hello ${name}"), "gpt-test")
            .expect("prompt builds");
        let rendered = prompt.render(&[("name", "world")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("hello world"));
    }

    /// Proves `{{name}}` placeholders render.
    #[test]
    fn render_binds_mustache() {
        let prompt = Prompt::new(openai_request_with_user("hello {{name}}"), "gpt-test")
            .expect("prompt builds");
        let rendered = prompt.render(&[("name", "world")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("hello world"));
    }

    /// Proves both placeholder grammars render together in one pass.
    #[test]
    fn render_binds_mixed_syntax_in_one_pass() {
        let prompt = Prompt::new(openai_request_with_user("${a} or {{a}}"), "gpt-test")
            .expect("prompt builds");
        let rendered = prompt.render(&[("a", "X")]).expect("renders");
        let json = serde_json::to_string(&rendered).expect("serializes");
        assert!(json.contains("X or X"));
    }
}

#[cfg(test)]
mod prompt_media {
    use crate::common;
    use crate::wire::anthropic_messages::{AnthropicContentBlock, AnthropicDocumentSource};
    use crate::wire::google_generate::GooglePart;
    use crate::wire::openai_chat::{OpenAiContentPart, OpenAiMessageContent};
    use crate::{MediaKind, MediaRef, Prompt, ProviderName, ProviderRequest, SkaldError};

    /// Builds a Prompt over `request` with a fixed literal model.
    ///
    /// # Panics
    ///
    /// Panics when the request is not a valid Prompt.
    fn prompt(request: ProviderRequest) -> Prompt {
        Prompt::new(request, "model").expect("prompt is valid")
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

    /// Proves a media placeholder in system content is refused at construction.
    #[test]
    fn media_placeholder_in_system_message_is_rejected() {
        let mut request = common::openai_chat_request();
        request.messages[0].content =
            Some(OpenAiMessageContent::Text("system ${media:x}".to_owned()));
        let err = Prompt::new(ProviderRequest::OpenAiChatCompletion(request), "model").unwrap_err();
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

        let mut vertex = google_prompt("${media:image}");
        vertex.provider = Some(ProviderName::Vertex);
        let vertex_https = vertex
            .bind_media(
                "image",
                &MediaRef::image_url(
                    "https://example.com/image.png",
                    Some("image/png".to_owned()),
                ),
            )
            .unwrap_err();
        assert_eq!(
            vertex_https,
            SkaldError::UnsupportedMediaForProvider {
                provider: ProviderName::Vertex,
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
            variables: variables.into_iter().map(str::to_string).collect(),
            media_variables: Vec::new(),
            response_type: ResponseType::Text,
            provider: None,
            authored: None,
            media_kinds: Vec::new(),
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

    /// A Jev Prompt discovers its `${reply}` state variable, and a value with
    /// quotes and newlines renders into the typed `state` text verbatim.
    #[test]
    fn jev_state_renders_quotes_and_newlines_verbatim() {
        let prompt = Prompt::new(
            ProviderRequest::JevSystemOne(common::jev_request()),
            "jev-latest",
        )
        .unwrap();
        assert_eq!(prompt.variables, ["reply"]);
        assert_eq!(prompt.provider(), crate::ProviderName::Jev);

        let reply = "He said \"no\".\n\"questions\": {}";
        let ProviderRequest::JevSystemOne(rendered) = prompt.render(&[("reply", reply)]).unwrap()
        else {
            panic!("a Jev Prompt renders a Jev request");
        };
        assert_eq!(rendered.state, reply);
        assert_eq!(rendered.questions.len(), 3);
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
