//! Prompt content hash computation.

use sha2::{Digest, Sha256};

use crate::card::prompt::PromptSpec;

/// Computes `sha256:<hex64>` over the JCS-canonical authored Prompt JSON.
///
/// The authored form is the durable identity: derived request, variables,
/// and response type never contribute, and key order never matters.
///
/// # Panics
///
/// Panics if the spec cannot be serialized, which compiling it on
/// construction rules out.
pub fn compute(spec: &PromptSpec) -> String {
    let bytes = serde_json::to_value(spec)
        .and_then(|value| serde_jcs::to_vec(&value))
        .unwrap_or_else(|error| {
            panic!("a compiled PromptSpec always serializes its authored form: {error}")
        });
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
