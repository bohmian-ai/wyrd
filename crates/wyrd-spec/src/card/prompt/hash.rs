//! Prompt content hash computation.

use serde::Serialize;
use sha2::{Digest, Sha256};
use skald_spec::{Prompt, ProviderRequest, ResponseType};

use crate::card::prompt::PromptSpec;

/// Computes `sha256:<hex64>` over native prompt JSON excluding `prompt.version`.
pub fn compute(spec: &PromptSpec) -> String {
    let projection = HashProjection::from(&spec.prompt);
    let bytes = serde_json::to_vec(&projection).expect("native skald Prompt projection serializes");
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

#[derive(Serialize)]
struct HashProjection<'a> {
    request: &'a ProviderRequest,
    model: &'a str,
    variables: &'a [String],
    response_type: &'a ResponseType,
}

impl<'a> From<&'a Prompt> for HashProjection<'a> {
    fn from(prompt: &'a Prompt) -> Self {
        Self {
            request: &prompt.request,
            model: &prompt.model,
            variables: &prompt.variables,
            response_type: &prompt.response_type,
        }
    }
}
