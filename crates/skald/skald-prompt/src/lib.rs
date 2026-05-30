//! Client-safe Prompt authoring builder for native Skald requests.
//!
//! This crate owns the Python-facing `Prompt` builder. The stored value is
//! always [`skald_spec::Prompt`]; helpers only make it easier to author the
//! provider-native wire structs.

#![deny(missing_docs)]

/// Provider-shaped Prompt constructors.
pub mod builder;
/// Python and JSON coercion helpers used by builder entry points.
pub mod coerce;
/// Prompt builder errors.
pub mod error;
/// Filesystem loader/dumper for the Prompt builder.
pub mod loader;
/// Media reference helpers for prompt media binding.
pub mod media;
/// Native message and content helper functions.
pub mod messages;
/// Prompt wrapper and rendered request wrapper.
pub mod prompt;
/// Response format helpers for provider-native structured output fields.
pub mod response_format;
/// Provider-native generation settings wrappers.
pub mod settings;

pub use builder::{
    AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, VertexOptions,
    anthropic, gemini, openai_chat, openai_responses, raw, vertex,
};
pub use error::{PromptBuilderError, PromptBuilderResult};
pub use media::{MAX_MEDIA_FILE_BYTES, PyMediaRef, document_path, image_path};
pub use prompt::{Prompt, PyProviderRequest};
pub use response_format::{ResponseFormat, ResponseFormatKind};
pub use settings::{
    PyAnthropicSettings, PyGoogleGenerateSettings, PyOpenAiChatSettings, PyOpenAiResponsesSettings,
};
#[cfg(feature = "python")]
pub use settings::{apply_model_settings, model_settings_py, prompt_py};

#[cfg(feature = "python")]
use pyo3::{prelude::*, types::PyModule};

/// Register prompt Python classes on a `wyrd.prompt` module.
#[cfg(feature = "python")]
pub fn register_prompt(module: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_interfaces::error::register_exceptions(module)?;
    module.add_class::<Prompt>()?;
    module.add_class::<PyProviderRequest>()?;
    module.add_class::<PyMediaRef>()?;
    module.add_class::<ResponseFormat>()?;
    module.add_class::<PyOpenAiChatSettings>()?;
    module.add_class::<PyOpenAiResponsesSettings>()?;
    module.add_class::<PyAnthropicSettings>()?;
    module.add_class::<PyGoogleGenerateSettings>()?;
    Ok(())
}
