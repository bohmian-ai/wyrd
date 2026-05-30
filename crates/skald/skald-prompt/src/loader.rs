//! Filesystem loader and dumper for prompt builder values.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use wyrd_spec::{CardLoadFormat, PromptSpec, parse_spec_bytes, serialize_spec_bytes};

use crate::builder::{AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions};
use crate::error::{PromptBuilderError, PromptBuilderResult};
use crate::prompt::Prompt;

/// Load a bare `PromptSpec` file and return the prompt builder wrapper.
pub fn load_prompt(path: impl AsRef<Path>) -> PromptBuilderResult<Prompt> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    let format = format_from_path(path)?;
    match parse_spec_bytes(format, &bytes) {
        Ok(spec) => Ok(Prompt::from_native(spec.prompt)),
        Err(_) => load_authoring_prompt(format, &bytes),
    }
}

/// Dump a prompt builder as a bare `PromptSpec` file.
pub fn dump_prompt(prompt: &Prompt, path: impl AsRef<Path>) -> PromptBuilderResult<()> {
    let path = path.as_ref();
    let format = format_from_path(path)?;
    let spec = PromptSpec::new(prompt.native().clone())?;
    let bytes = serialize_spec_bytes(format, &spec)?;
    std::fs::write(path, bytes).map_err(|error| loader_io(path, &error))
}

fn format_from_path(path: &Path) -> PromptBuilderResult<CardLoadFormat> {
    CardLoadFormat::from_extension(path.extension().and_then(std::ffi::OsStr::to_str))
        .map_err(Into::into)
}

fn loader_io(path: &Path, error: &std::io::Error) -> PromptBuilderError {
    PromptBuilderError::Io {
        path: path_string(path),
        message: error.to_string(),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Debug, Deserialize)]
struct PromptAuthoring {
    provider: String,
    model: String,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    messages: Option<Value>,
    #[serde(default)]
    model_settings: Option<Value>,
    #[serde(default)]
    cache: Option<Value>,
    #[serde(default)]
    variables: Vec<String>,
    #[serde(default)]
    version: Option<String>,
}

fn load_authoring_prompt(format: CardLoadFormat, bytes: &[u8]) -> PromptBuilderResult<Prompt> {
    let value: Value = match format {
        CardLoadFormat::Json => serde_json::from_slice(bytes)
            .map_err(|error| PromptBuilderError::Validation(error.to_string()))?,
        CardLoadFormat::Yaml => serde_yaml::from_slice(bytes)
            .map_err(|error| PromptBuilderError::Validation(error.to_string()))?,
    };
    prompt_from_authoring_value(value)
}

/// Build a prompt from the concise authoring object used by YAML/JSON loaders.
pub fn prompt_from_authoring_value(value: Value) -> PromptBuilderResult<Prompt> {
    let authoring: PromptAuthoring = serde_json::from_value(value)
        .map_err(|error| PromptBuilderError::Validation(error.to_string()))?;
    let messages = authoring_messages(authoring.messages)?;
    let provider = crate::coerce::provider_name_from_str(&authoring.provider);
    match provider {
        skald_spec::ProviderName::OpenAi
            if authoring
                .operation
                .as_deref()
                .is_some_and(|operation| operation.eq_ignore_ascii_case("responses")) =>
        {
            crate::builder::openai_responses(
                authoring.model,
                OpenAiResponsesOptions {
                    instructions: authoring.system,
                    messages,
                    settings: settings_value(authoring.model_settings)?,
                    variables: authoring.variables,
                    version: authoring.version,
                    ..OpenAiResponsesOptions::default()
                },
            )
        }
        skald_spec::ProviderName::OpenAi => crate::builder::openai_chat(
            authoring.model,
            OpenAiChatOptions {
                system: authoring.system,
                messages,
                prompt_cache_key: authoring.cache.and_then(cache_prompt_key),
                settings: settings_value(authoring.model_settings)?,
                variables: authoring.variables,
                version: authoring.version,
                ..OpenAiChatOptions::default()
            },
        ),
        skald_spec::ProviderName::Anthropic => crate::builder::anthropic(
            authoring.model,
            AnthropicOptions {
                system: authoring.system,
                messages,
                settings: settings_value(authoring.model_settings)?,
                variables: authoring.variables,
                version: authoring.version,
                ..AnthropicOptions::default()
            },
        ),
        skald_spec::ProviderName::Google => crate::builder::gemini(
            authoring.model,
            GeminiOptions {
                system: authoring.system,
                messages,
                settings: settings_value(authoring.model_settings)?,
                variables: authoring.variables,
                version: authoring.version,
                ..GeminiOptions::default()
            },
        ),
        skald_spec::ProviderName::Vertex => crate::builder::vertex(
            authoring.model,
            GeminiOptions {
                system: authoring.system,
                messages,
                settings: settings_value(authoring.model_settings)?,
                variables: authoring.variables,
                version: authoring.version,
                ..GeminiOptions::default()
            },
        ),
        skald_spec::ProviderName::Custom(value) => Err(PromptBuilderError::InvalidProvider(value)),
    }
}

fn authoring_messages(value: Option<Value>) -> PromptBuilderResult<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::String(text) => Ok(vec![text]),
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::String(text) => Ok(text),
                other => Err(PromptBuilderError::Validation(format!(
                    "authoring messages must be strings, got {other}"
                ))),
            })
            .collect(),
        other => Err(PromptBuilderError::Validation(format!(
            "authoring messages must be a string or list of strings, got {other}"
        ))),
    }
}

fn settings_value<T>(value: Option<Value>) -> PromptBuilderResult<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    value
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| PromptBuilderError::Validation(error.to_string()))
        .map(Option::unwrap_or_default)
}

fn cache_prompt_key(value: Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value),
        Value::Object(mut map) => map
            .remove("prompt_cache_key")
            .or_else(|| map.remove("key"))
            .and_then(|value| value.as_str().map(ToOwned::to_owned)),
        _ => None,
    }
}
