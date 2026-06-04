//! Prompt content hash computation.

use serde::Serialize;
use sha2::{Digest, Sha256};
use skald_spec::{Prompt, ProviderRequest, ResponseType};

use crate::card::prompt::PromptSpec;

/// Computes `sha256:<hex64>` over native prompt JSON excluding `prompt.version`.
pub fn compute(spec: &PromptSpec) -> String {
    let projection = HashProjection::from(&spec.prompt);
    let bytes = match serde_json::to_vec(&projection) {
        Ok(bytes) => bytes,
        Err(error) => {
            panic!(
                "HashProjection always serializes because all fields implement Serialize correctly: {error}"
            )
        }
    };
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

#[derive(Serialize)]
struct HashProjection<'a> {
    request: &'a ProviderRequest,
    model: &'a str,
    variables: &'a [String],
    media_variables: &'a [String],
    response_type: &'a ResponseType,
}

impl<'a> From<&'a Prompt> for HashProjection<'a> {
    fn from(prompt: &'a Prompt) -> Self {
        Self {
            request: &prompt.request,
            model: &prompt.model,
            variables: &prompt.variables,
            media_variables: &prompt.media_variables,
            response_type: &prompt.response_type,
        }
    }
}
