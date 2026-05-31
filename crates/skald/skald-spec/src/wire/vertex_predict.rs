use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct VertexPredictRequest {
    pub instances: Vec<VertexEmbedInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<VertexEmbedParameters>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct VertexEmbedInstance {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VertexEmbedParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_truncate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VertexPredictResponse {
    pub predictions: Vec<VertexPrediction>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VertexPrediction {
    pub embeddings: VertexEmbeddingPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VertexEmbeddingPayload {
    pub values: Vec<f32>,
    #[serde(default)]
    pub statistics: Option<Value>,
}
