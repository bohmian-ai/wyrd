//! `OpenAI`-compatible public operation contracts of the gateway.
//!
//! Chat Completions, Responses, and Embeddings requests reuse the exact Skald
//! wire shapes, which the translating adapters already decode, through
//! transparent newtypes so this crate owns their public schema. Media, file,
//! batch, model, and error envelopes that Skald does not model are declared
//! here. Every envelope the provider may extend keeps unmodeled members in a
//! flattened `extra` map, so supported extension fields survive a decode and
//! re-encode. Numbers in `multipart/form-data` forms travel as text, so form
//! contracts publish the fields a form carries rather than decode one.
//!
//! With the `server` feature each contract also implements
//! [`utoipa::ToSchema`] by converting its generated JSON Schema, so the
//! published OpenAPI document shows the same concrete fields as the schema.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use skald_spec::wire::openai_chat::{OpenAiChatRequest, OpenAiChatResponse};
use skald_spec::wire::openai_embeddings::{OpenAiEmbeddingUsage, OpenAiEmbeddingsRequest};
use skald_spec::wire::openai_responses::{OpenAiResponsesRequest, OpenAiResponsesResponse};

/// `POST /v1/chat/completions` request: the `OpenAI` Chat Completions body
/// whose `model` is an exact `<provider>/<model>` projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GatewayChatCompletionsRequest(
    /// The Chat Completions body, decoded and relayed as the native `OpenAI` shape.
    pub OpenAiChatRequest,
);

/// Buffered `POST /v1/chat/completions` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GatewayChatCompletion(
    /// The provider's Chat Completions answer in the native `OpenAI` shape.
    pub OpenAiChatResponse,
);

/// `POST /v1/responses` request: the `OpenAI` Responses body whose `model` is
/// an exact `<provider>/<model>` projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GatewayResponsesRequest(
    /// The Responses body, decoded and relayed as the native `OpenAI` shape.
    pub OpenAiResponsesRequest,
);

/// Buffered `POST /v1/responses` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GatewayResponse(
    /// The provider's Responses answer in the native `OpenAI` shape.
    pub OpenAiResponsesResponse,
);

/// `POST /v1/embeddings` request: the `OpenAI` Embeddings body whose `model`
/// is an exact `<provider>/<model>` projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GatewayEmbeddingsRequest(
    /// The Embeddings body, decoded and relayed as the native `OpenAI` shape.
    pub OpenAiEmbeddingsRequest,
);

/// `POST /v1/embeddings` answer: one embedding per input, in input order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayEmbeddings {
    /// Always `list`.
    pub object: String,
    /// Model that produced the embeddings.
    pub model: String,
    /// Embeddings whose `index` equals their position.
    pub data: Vec<GatewayEmbedding>,
    /// Token usage of the call.
    pub usage: OpenAiEmbeddingUsage,
    /// Unmodeled provider members.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One embedding of a [`GatewayEmbeddings`] answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayEmbedding {
    /// Always `embedding`.
    pub object: String,
    /// Position of the input this embedding represents.
    pub index: u32,
    /// Vector in the requested `encoding_format`.
    pub embedding: GatewayEmbeddingVector,
}

/// An embedding vector in its requested representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum GatewayEmbeddingVector {
    /// `float` (or omitted) encoding: the components.
    Float(Vec<f32>),
    /// `base64` encoding: little-endian `f32` components.
    Base64(String),
}

/// `POST /v1/images/generations` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayImageGenerationRequest {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// Description of the images to generate.
    pub prompt: String,
    /// Number of images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Image size, such as `1024x1024`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Image quality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// `url` or `b64_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// End-user identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Binary content published as an `OpenAPI` binary string: one uploaded file
/// of a `multipart/form-data` form, or a raw `application/octet-stream` body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayFormFile;

impl JsonSchema for GatewayFormFile {
    /// Names the shared binary part schema.
    fn schema_name() -> String {
        "GatewayFormFile".to_owned()
    }

    /// Publishes a binary string, the `OpenAPI` shape of a file part.
    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            format: Some("binary".to_owned()),
            ..Default::default()
        }
        .into()
    }
}

/// `POST /v1/images/edits` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayImageEditForm {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// One to sixteen images, sent as `image` or `image[]` parts.
    pub image: Vec<GatewayFormFile>,
    /// Optional mask image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<GatewayFormFile>,
    /// Description of the edit.
    pub prompt: String,
    /// Number of images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Image size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// `url` or `b64_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// End-user identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `POST /v1/images/variations` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayImageVariationForm {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// The one source image.
    pub image: GatewayFormFile,
    /// Number of images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Image size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// `url` or `b64_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// End-user identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Images answer of generations, edits, and variations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayImages {
    /// Unix time the images were created.
    pub created: u64,
    /// Generated images.
    pub data: Vec<GatewayImage>,
    /// Unmodeled provider members, such as `usage`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One image of a [`GatewayImages`] answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayImage {
    /// Base64 image data, for `b64_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b64_json: Option<String>,
    /// Image URL, for `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Prompt the provider actually used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    /// Unmodeled provider members.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `POST /v1/audio/speech` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewaySpeechRequest {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// Text to speak.
    pub input: String,
    /// Voice name.
    pub voice: String,
    /// Delivery instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Audio format, such as `mp3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// Playback speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `POST /v1/audio/transcriptions` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTranscriptionForm {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// The one audio file.
    pub file: GatewayFormFile,
    /// ISO-639-1 input language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Style or context prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// `json`, `text`, `srt`, `verbose_json`, or `vtt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Supported provider extensions, such as `timestamp_granularities[]`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `POST /v1/audio/translations` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTranslationForm {
    /// Exact `<provider>/<model>` projection.
    pub model: String,
    /// The one audio file.
    pub file: GatewayFormFile,
    /// Style or context prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// `json`, `text`, `srt`, `verbose_json`, or `vtt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// JSON transcript or translation answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTranscription {
    /// The text.
    pub text: String,
    /// Unmodeled provider members, such as `segments` or `usage`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `GET /v1/models` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayModelList {
    /// Always `list`.
    pub object: String,
    /// Invocable model projections.
    pub data: Vec<GatewayModel>,
}

/// One model of a [`GatewayModelList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayModel {
    /// Exact `<provider>/<model>` projection.
    pub id: String,
    /// Always `model`.
    pub object: String,
    /// Always `0`: the gateway records no provider release time.
    pub created: u64,
    /// The provider.
    pub owned_by: String,
}

/// `POST /v1/files` form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayFileUploadForm {
    /// Always `batch`.
    pub purpose: String,
    /// Batch JSONL whose request lines target one endpoint and one model.
    pub file: GatewayFormFile,
}

/// `OpenAI` file object of an uploaded batch input file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayFile {
    /// Wyrd file id.
    pub id: String,
    /// Always `file`.
    pub object: String,
    /// Size in bytes.
    pub bytes: i64,
    /// Unix upload time.
    pub created_at: i64,
    /// Uploaded file name.
    pub filename: String,
    /// Always `batch`.
    pub purpose: String,
}

/// `DELETE /v1/files/{file_id}` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayFileDeleted {
    /// Deleted Wyrd file id.
    pub id: String,
    /// Always `file`.
    pub object: String,
    /// Always `true`.
    pub deleted: bool,
}

/// `POST /v1/batches` request; an identical request converges on one batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayBatchCreateRequest {
    /// Wyrd id of the uploaded input file.
    pub input_file_id: String,
    /// The endpoint every input file request targets.
    pub endpoint: String,
    /// Completion window, such as `24h`.
    pub completion_window: String,
    /// Caller metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
    /// Supported provider extensions, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `OpenAI` batch object with Wyrd batch and file ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayBatch {
    /// Wyrd batch id.
    pub id: String,
    /// Always `batch`.
    pub object: String,
    /// Endpoint of the batch requests.
    pub endpoint: String,
    /// Wyrd id of the input file.
    pub input_file_id: String,
    /// Completion window.
    pub completion_window: String,
    /// Last observed provider status.
    pub status: String,
    /// Wyrd id of the output file, once observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_file_id: Option<String>,
    /// Wyrd id of the error file, once observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_file_id: Option<String>,
    /// Unix creation time.
    pub created_at: i64,
    /// Caller metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
    /// Unmodeled provider members, such as `request_counts`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `GET /v1/batches` answer, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayBatchList {
    /// Always `list`.
    pub object: String,
    /// Visible batches.
    pub data: Vec<GatewayBatch>,
    /// Id of the first listed batch.
    pub first_id: Option<String>,
    /// Id of the last listed batch, the next page's `after`.
    pub last_id: Option<String>,
    /// Whether a later page exists.
    pub has_more: bool,
}

/// `GET /v1/batches` query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
pub struct GatewayBatchListQuery {
    /// Last batch id of the previous page.
    #[serde(default)]
    pub after: Option<String>,
    /// Page size between 1 and 100; default 20.
    #[serde(default)]
    #[schemars(range(min = 1, max = 100))]
    #[cfg_attr(feature = "server", param(minimum = 1, maximum = 100))]
    pub limit: Option<u32>,
}

/// `OpenAI` error envelope of every gateway-originated refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenAiErrorEnvelope {
    /// The error.
    pub error: OpenAiError,
}

/// Body of an [`OpenAiErrorEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenAiError {
    /// Human-readable message.
    pub message: String,
    /// `OpenAI` error type, such as `invalid_request_error`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Offending request member.
    pub param: Option<String>,
    /// Stable Wyrd error code.
    pub code: Option<String>,
}

/// Implements [`utoipa::ToSchema`] for contracts from their JSON Schema.
#[cfg(feature = "server")]
macro_rules! openapi_schema {
    ($($name:ident),+ $(,)?) => {$(
        impl utoipa::PartialSchema for $name {
            /// Converts the contract's root JSON Schema.
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                $crate::gateway::openai::openapi::component::<Self>().0
            }
        }

        impl utoipa::ToSchema for $name {
            /// The contract's type name.
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }

            /// Registers every named JSON Schema definition the contract uses.
            fn schemas(
                schemas: &mut Vec<(String, utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>)>,
            ) {
                schemas.extend($crate::gateway::openai::openapi::component::<Self>().1);
            }
        }
    )+};
}

#[cfg(feature = "server")]
pub(crate) use openapi_schema;

#[cfg(feature = "server")]
openapi_schema!(
    GatewayChatCompletionsRequest,
    GatewayChatCompletion,
    GatewayResponsesRequest,
    GatewayResponse,
    GatewayEmbeddingsRequest,
    GatewayEmbeddings,
    GatewayImageGenerationRequest,
    GatewayImageEditForm,
    GatewayImageVariationForm,
    GatewayImages,
    GatewaySpeechRequest,
    GatewayTranscriptionForm,
    GatewayTranslationForm,
    GatewayTranscription,
    GatewayModelList,
    GatewayFileUploadForm,
    GatewayFile,
    GatewayFileDeleted,
    GatewayFormFile,
    GatewayBatchCreateRequest,
    GatewayBatch,
    GatewayBatchList,
    OpenAiErrorEnvelope,
);

/// Structural JSON Schema to `OpenAPI` schema conversion.
///
/// Converting through the typed models, rather than deserializing JSON into
/// `utoipa`'s untagged schema enum, keeps composite schemas: that enum would
/// accept an `anyOf` as an empty object and silently drop its alternatives.
#[cfg(feature = "server")]
pub(crate) mod openapi {
    use schemars::JsonSchema;
    use schemars::r#gen::SchemaSettings;
    use schemars::schema::{InstanceType, Schema as Json, SchemaObject, SingleOrVec};
    use utoipa::openapi::schema::{
        AdditionalProperties, AllOfBuilder, AnyOfBuilder, ArrayBuilder, ObjectBuilder,
        OneOfBuilder, Schema, SchemaFormat, SchemaType, Type,
    };
    use utoipa::openapi::{Ref, RefOr};

    /// Named `OpenAPI` components of one converted definition set.
    pub(crate) type Components = Vec<(String, RefOr<Schema>)>;

    /// Root schema of `T` and its named definitions, referenced as components.
    pub(crate) fn component<T: JsonSchema>() -> (RefOr<Schema>, Components) {
        let mut settings = SchemaSettings::draft2019_09();
        settings.definitions_path = "#/components/schemas/".to_owned();
        settings.meta_schema = None;
        let root = settings.into_generator().into_root_schema_for::<T>();
        let definitions = root
            .definitions
            .iter()
            .map(|(name, schema)| (name.clone(), convert(schema)))
            .collect();
        (object(&root.schema), definitions)
    }

    /// Converts one schema; a boolean schema accepts any value.
    fn convert(schema: &Json) -> RefOr<Schema> {
        match schema {
            Json::Bool(_) => ObjectBuilder::new()
                .schema_type(SchemaType::AnyValue)
                .build()
                .into(),
            Json::Object(schema) => object(schema),
        }
    }

    /// Converts a schema object: a reference, a composite joined with its own
    /// constraints, or a plain schema.
    fn object(schema: &SchemaObject) -> RefOr<Schema> {
        if let Some(reference) = &schema.reference {
            return Ref::new(reference.clone()).into();
        }
        let description = schema.metadata.as_ref().and_then(|m| m.description.clone());
        let composite: Option<Schema> = schema.subschemas.as_ref().and_then(|sub| {
            let items = |schemas: &Vec<Json>| schemas.iter().map(convert).collect::<Vec<_>>();
            if let Some(one_of) = &sub.one_of {
                Some(
                    items(one_of)
                        .into_iter()
                        .fold(OneOfBuilder::new(), OneOfBuilder::item)
                        .into(),
                )
            } else if let Some(any_of) = &sub.any_of {
                Some(
                    items(any_of)
                        .into_iter()
                        .fold(AnyOfBuilder::new(), AnyOfBuilder::item)
                        .into(),
                )
            } else {
                sub.all_of.as_ref().map(|all_of| {
                    items(all_of)
                        .into_iter()
                        .fold(AllOfBuilder::new(), AllOfBuilder::item)
                        .into()
                })
            }
        });
        let constrained = schema.instance_type.is_some()
            || schema.object.is_some()
            || schema.array.is_some()
            || schema.enum_values.is_some()
            || schema.const_value.is_some();
        match composite {
            Some(composite) if constrained => AllOfBuilder::new()
                .item(plain(schema, None))
                .item(composite)
                .description(description)
                .into(),
            Some(composite) => wrap_description(composite, description),
            None => plain(schema, description).into(),
        }
    }

    /// Attaches `description` to a composite schema.
    fn wrap_description(composite: Schema, description: Option<String>) -> RefOr<Schema> {
        match (composite, description) {
            (Schema::OneOf(mut one_of), description) => {
                one_of.description = description;
                Schema::OneOf(one_of).into()
            }
            (Schema::AnyOf(mut any_of), description) => {
                any_of.description = description;
                Schema::AnyOf(any_of).into()
            }
            (Schema::AllOf(mut all_of), description) => {
                all_of.description = description;
                Schema::AllOf(all_of).into()
            }
            (other, _) => other.into(),
        }
    }

    /// Converts a schema's type, format, values, and array or object members.
    fn plain(schema: &SchemaObject, description: Option<String>) -> Schema {
        let schema_type = match &schema.instance_type {
            None => SchemaType::AnyValue,
            Some(SingleOrVec::Single(kind)) => SchemaType::Type(type_of(kind)),
            Some(SingleOrVec::Vec(kinds)) => SchemaType::Array(kinds.iter().map(type_of).collect()),
        };
        if let Some(array) = &schema.array {
            let items = match &array.items {
                Some(SingleOrVec::Single(item)) => convert(item),
                _ => convert(&Json::Bool(true)),
            };
            return ArrayBuilder::new()
                .schema_type(schema_type)
                .items(items)
                .min_items(array.min_items.map(|n| n as usize))
                .max_items(array.max_items.map(|n| n as usize))
                .description(description)
                .into();
        }
        let values = schema
            .enum_values
            .clone()
            .or_else(|| schema.const_value.clone().map(|value| vec![value]));
        let mut builder = ObjectBuilder::new()
            .schema_type(schema_type)
            .format(schema.format.clone().map(SchemaFormat::Custom))
            .description(description)
            .enum_values(values);
        if let Some(number) = &schema.number {
            builder = builder.minimum(number.minimum).maximum(number.maximum);
        }
        if let Some(object) = &schema.object {
            for (name, property) in &object.properties {
                builder = builder.property(name, convert(property));
            }
            for name in &object.required {
                builder = builder.required(name);
            }
            builder = builder.additional_properties(object.additional_properties.as_deref().map(
                |extra| match extra {
                    Json::Bool(allowed) => AdditionalProperties::FreeForm(*allowed),
                    Json::Object(_) => AdditionalProperties::RefOr(convert(extra)),
                },
            ));
        }
        builder.into()
    }

    /// The `OpenAPI` type of a JSON Schema instance type.
    fn type_of(kind: &InstanceType) -> Type {
        match kind {
            InstanceType::Null => Type::Null,
            InstanceType::Boolean => Type::Boolean,
            InstanceType::Object => Type::Object,
            InstanceType::Array => Type::Array,
            InstanceType::Number => Type::Number,
            InstanceType::String => Type::String,
            InstanceType::Integer => Type::Integer,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Every extensible envelope decodes and re-encodes a body with supported
    /// extension fields unchanged.
    ///
    /// # Panics
    ///
    /// Panics when a decode fails or an extension member is lost or altered.
    #[test]
    fn supported_extensions_round_trip() {
        fn round_trip<T: serde::de::DeserializeOwned + Serialize>(body: &Value) {
            let decoded: T = serde_json::from_value(body.clone()).expect("contract decodes");
            assert_eq!(&serde_json::to_value(decoded).expect("encodes"), body);
        }
        round_trip::<GatewayChatCompletionsRequest>(&json!({
            "model": "acme/m", "messages": [{"role": "user", "content": "hi"}],
            "vendor_hint": {"cache": true}
        }));
        round_trip::<GatewayResponsesRequest>(&json!({
            "model": "acme/m", "input": [], "vendor_hint": 1
        }));
        round_trip::<GatewayImageGenerationRequest>(&json!({
            "model": "acme/i", "prompt": "a fox", "n": 2, "background": "transparent"
        }));
        round_trip::<GatewaySpeechRequest>(&json!({
            "model": "acme/t", "input": "hi", "voice": "alloy", "stream_format": "audio"
        }));
        round_trip::<GatewayBatchCreateRequest>(&json!({
            "input_file_id": "file-1", "endpoint": "/v1/chat/completions",
            "completion_window": "24h", "metadata": {"k": "v"}, "output_expires_after": {"seconds": 3600}
        }));
        round_trip::<GatewayEmbeddings>(&json!({
            "object": "list", "model": "acme/e",
            "data": [{"object": "embedding", "index": 0, "embedding": "AAAAAA=="}],
            "usage": {"prompt_tokens": 1, "total_tokens": 1}, "vendor": "x"
        }));
        round_trip::<GatewayBatch>(&json!({
            "id": "batch_1", "object": "batch", "endpoint": "/v1/chat/completions",
            "input_file_id": "file-1", "completion_window": "24h", "status": "completed",
            "output_file_id": "file-1-output", "created_at": 1,
            "request_counts": {"total": 1, "completed": 1, "failed": 0}
        }));
    }
}
