//! Prompt Card spec.
//!
//! This module is a thin Wyrd Card envelope around [`skald_spec::Prompt`].
//! Provider-specific messages, tools, media, cache directives, response
//! formats, and provider knobs stay native inside `skald-spec`.

mod codec;
mod hash;
mod parameter;
mod promptref;
mod spec;
pub mod validate;

pub use codec::{
    CardLoadFormat, parse_card_bytes, parse_spec_bytes, serialize_card, serialize_spec_bytes,
};
pub use hash::compute as content_hash;
pub use parameter::{
    ParameterName, extract_media_placeholders, extract_text_placeholders, is_valid_parameter_name,
};
pub use promptref::PromptRef;
pub use spec::PromptSpec;
pub use validate::{PromptError, validate};
