use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiEmbeddingsRequest {
    pub model: String,
    pub input: OpenAiEmbeddingsInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiEmbeddingsInput {
    One(String),
    Many(Vec<String>),
    Tokens(Vec<u32>),
    TokenBatches(Vec<Vec<u32>>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiEmbeddingsResponse {
    pub object: String,
    pub model: String,
    pub data: Vec<OpenAiEmbeddingVector>,
    pub usage: OpenAiEmbeddingUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiEmbeddingVector {
    pub object: String,
    pub index: u32,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiEmbeddingUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}
