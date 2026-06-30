//! Locked attribute conventions for the trace surface.
//!
//! Two namespaces:
//!
//! - `wyrd.*` — Wyrd-internal annotations. Renamed from the predecessor
//!   namespace.
//! - `gen_ai.*` — OpenTelemetry GenAI semantic conventions, verbatim from
//!   the spec with no Wyrd-prefixed alias. See
//!   <https://opentelemetry.io/docs/specs/semconv/gen-ai/>.
//!
//! All constants are `pub const &str` so they participate in trivial pattern
//! matches and stay zero-cost in attribute-key comparisons.

/// Function classification (instrumented vs decorated).
pub const FUNCTION_TYPE: &str = "wyrd.function.type";
/// Instrumented function name (free-form; `module.function`).
pub const FUNCTION_NAME: &str = "wyrd.function.name";
/// Input payload captured by trace instrumentation (JSON-shaped string).
pub const TRACING_INPUT: &str = "wyrd.tracing.input";
/// Output payload captured by trace instrumentation (JSON-shaped string).
pub const TRACING_OUTPUT: &str = "wyrd.tracing.output";
/// User-supplied label for ad-hoc grouping in the UI.
pub const TRACING_LABEL: &str = "wyrd.tracing.label";
/// Originating `EvalRecord.record_id` UUID.
pub const EVAL_RECORD_UID: &str = "wyrd.eval.record_uid";
/// Originating `EvalCard.profile_uid` UUID.
pub const EVAL_PROFILE_UID: &str = "wyrd.eval.profile_uid";
/// Originating `ServiceCard` UID. Set by observation instrumentation.
pub const SERVICE_CARD_UID: &str = "wyrd.service.card_uid";
/// Tenant partition key. Mirrors the typed `DataTenantId` column on the
/// owning record.
pub const DATA_TENANT_ID: &str = "wyrd.data_tenant_id";

/// Naming convention for user-supplied tag attributes. A tag `foo` becomes
/// attribute key `wyrd.tracing.tag.foo`.
pub const TAG_PREFIX: &str = "wyrd.tracing.tag.";

/// W3C Baggage propagation prefix. Producers project baggage entries into
/// span attributes under this prefix.
pub const BAGGAGE_PREFIX: &str = "baggage.";

/// Provider name, such as `anthropic`, `openai`, `aws.bedrock`,
/// `azure.ai.openai`, or `gcp.vertex_ai`.
pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
/// GenAI operation name, such as `chat`, `embeddings`, or `execute_tool`.
pub const GEN_AI_OPERATION_NAME: &str = "gen_ai.operation.name";
/// GenAI output type, such as `text`, `json`, `image`, or `speech`.
pub const GEN_AI_OUTPUT_TYPE: &str = "gen_ai.output.type";
/// Stable conversation identifier across turns.
pub const GEN_AI_CONVERSATION_ID: &str = "gen_ai.conversation.id";

/// Requested model name.
pub const GEN_AI_REQUEST_MODEL: &str = "gen_ai.request.model";
/// Requested sampling temperature.
pub const GEN_AI_REQUEST_TEMPERATURE: &str = "gen_ai.request.temperature";
/// Requested nucleus sampling probability.
pub const GEN_AI_REQUEST_TOP_P: &str = "gen_ai.request.top_p";
/// Requested top-k sampling count.
pub const GEN_AI_REQUEST_TOP_K: &str = "gen_ai.request.top_k";
/// Requested maximum output token count.
pub const GEN_AI_REQUEST_MAX_TOKENS: &str = "gen_ai.request.max_tokens";
/// Requested frequency penalty.
pub const GEN_AI_REQUEST_FREQUENCY_PENALTY: &str = "gen_ai.request.frequency_penalty";
/// Requested presence penalty.
pub const GEN_AI_REQUEST_PRESENCE_PENALTY: &str = "gen_ai.request.presence_penalty";
/// Requested random seed.
pub const GEN_AI_REQUEST_SEED: &str = "gen_ai.request.seed";
/// Requested number of choices.
pub const GEN_AI_REQUEST_CHOICE_COUNT: &str = "gen_ai.request.choice.count";
/// Requested stop sequences.
pub const GEN_AI_REQUEST_STOP_SEQUENCES: &str = "gen_ai.request.stop_sequences";
/// Whether the request was streamed.
pub const GEN_AI_REQUEST_STREAM: &str = "gen_ai.request.stream";
/// Requested embedding encoding formats.
pub const GEN_AI_REQUEST_ENCODING_FORMATS: &str = "gen_ai.request.encoding_formats";

/// Response model name.
pub const GEN_AI_RESPONSE_MODEL: &str = "gen_ai.response.model";
/// Provider response identifier.
pub const GEN_AI_RESPONSE_ID: &str = "gen_ai.response.id";
/// Provider finish reasons.
pub const GEN_AI_RESPONSE_FINISH_REASONS: &str = "gen_ai.response.finish_reasons";
/// Streaming time to first chunk in seconds.
pub const GEN_AI_RESPONSE_TIME_TO_FIRST_CHUNK: &str = "gen_ai.response.time_to_first_chunk";

/// Input token count.
pub const GEN_AI_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
/// Output token count.
pub const GEN_AI_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
/// Token count for cache creation input.
pub const GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS: &str =
    "gen_ai.usage.cache_creation.input_tokens";
/// Token count read from cache input.
pub const GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS: &str = "gen_ai.usage.cache_read.input_tokens";
/// Token count spent on model-internal reasoning.
pub const GEN_AI_USAGE_REASONING_OUTPUT_TOKENS: &str = "gen_ai.usage.reasoning.output_tokens";

/// Tool call identifier.
pub const GEN_AI_TOOL_CALL_ID: &str = "gen_ai.tool.call.id";
/// Tool name.
pub const GEN_AI_TOOL_NAME: &str = "gen_ai.tool.name";
/// Tool type.
pub const GEN_AI_TOOL_TYPE: &str = "gen_ai.tool.type";
/// Human-readable tool description supplied to the model.
pub const GEN_AI_TOOL_DESCRIPTION: &str = "gen_ai.tool.description";
/// Tool definitions supplied to the model.
pub const GEN_AI_TOOL_DEFINITIONS: &str = "gen_ai.tool.definitions";
/// Tool call arguments.
pub const GEN_AI_TOOL_CALL_ARGUMENTS: &str = "gen_ai.tool.call.arguments";
/// Tool call result.
pub const GEN_AI_TOOL_CALL_RESULT: &str = "gen_ai.tool.call.result";

/// Agent name.
pub const GEN_AI_AGENT_NAME: &str = "gen_ai.agent.name";
/// Agent identifier.
pub const GEN_AI_AGENT_ID: &str = "gen_ai.agent.id";
/// Agent description.
pub const GEN_AI_AGENT_DESCRIPTION: &str = "gen_ai.agent.description";
/// Agent version.
pub const GEN_AI_AGENT_VERSION: &str = "gen_ai.agent.version";

/// Prompt name.
pub const GEN_AI_PROMPT_NAME: &str = "gen_ai.prompt.name";
/// Workflow name.
pub const GEN_AI_WORKFLOW_NAME: &str = "gen_ai.workflow.name";
/// Data source identifier.
pub const GEN_AI_DATA_SOURCE_ID: &str = "gen_ai.data_source.id";

/// Embedding vector dimension count.
pub const GEN_AI_EMBEDDINGS_DIMENSION_COUNT: &str = "gen_ai.embeddings.dimension.count";

/// Provider endpoint host.
pub const SERVER_ADDRESS: &str = "server.address";
/// Provider endpoint port.
pub const SERVER_PORT: &str = "server.port";
/// Provider-side error classification.
pub const ERROR_TYPE: &str = "error.type";

/// Opt-in captured GenAI input messages.
pub const GEN_AI_INPUT_MESSAGES: &str = "gen_ai.input.messages";
/// Opt-in captured GenAI output messages.
pub const GEN_AI_OUTPUT_MESSAGES: &str = "gen_ai.output.messages";
/// Opt-in captured GenAI system instructions.
///
/// Single-segment suffix is intentional per the Wyrd GenAI semconv extension.
/// The upstream OTel GenAI semconv does not yet define a canonical key for
/// system instructions; track <https://github.com/open-telemetry/semantic-conventions>
/// and rename if the spec stabilizes on a different key.
pub const GEN_AI_SYSTEM_INSTRUCTIONS: &str = "gen_ai.system_instructions";

/// Retrieved documents used by a GenAI call.
pub const GEN_AI_RETRIEVAL_DOCUMENTS: &str = "gen_ai.retrieval.documents";
/// Retrieval query text.
pub const GEN_AI_RETRIEVAL_QUERY_TEXT: &str = "gen_ai.retrieval.query.text";

/// OpenAI API type.
pub const GEN_AI_OPENAI_API_TYPE: &str = "gen_ai.openai.api.type";
/// OpenAI service tier.
pub const GEN_AI_OPENAI_SERVICE_TIER: &str = "gen_ai.openai.service_tier";

/// Event name for a GenAI evaluation result.
pub const GEN_AI_EVALUATION_EVENT: &str = "gen_ai.evaluation.result";
/// GenAI evaluation name.
pub const GEN_AI_EVALUATION_NAME: &str = "gen_ai.evaluation.name";
/// GenAI evaluation score label.
pub const GEN_AI_EVALUATION_SCORE_LABEL: &str = "gen_ai.evaluation.score.label";
/// GenAI evaluation numeric score.
pub const GEN_AI_EVALUATION_SCORE_VALUE: &str = "gen_ai.evaluation.score.value";
/// GenAI evaluation explanation.
pub const GEN_AI_EVALUATION_EXPLANATION: &str = "gen_ai.evaluation.explanation";

/// Every Wyrd-namespaced attribute key tracked by this module.
pub const WYRD_KEYS: &[&str] = &[
    FUNCTION_TYPE,
    FUNCTION_NAME,
    TRACING_INPUT,
    TRACING_OUTPUT,
    TRACING_LABEL,
    EVAL_RECORD_UID,
    EVAL_PROFILE_UID,
    SERVICE_CARD_UID,
    DATA_TENANT_ID,
];

/// Every GenAI semantic convention key tracked in typed columns by the trace
/// surface, excluding general OTel referenced keys.
pub const GEN_AI_KEYS: &[&str] = &[
    GEN_AI_PROVIDER_NAME,
    GEN_AI_OPERATION_NAME,
    GEN_AI_OUTPUT_TYPE,
    GEN_AI_CONVERSATION_ID,
    GEN_AI_REQUEST_MODEL,
    GEN_AI_REQUEST_TEMPERATURE,
    GEN_AI_REQUEST_TOP_P,
    GEN_AI_REQUEST_TOP_K,
    GEN_AI_REQUEST_MAX_TOKENS,
    GEN_AI_REQUEST_FREQUENCY_PENALTY,
    GEN_AI_REQUEST_PRESENCE_PENALTY,
    GEN_AI_REQUEST_SEED,
    GEN_AI_REQUEST_CHOICE_COUNT,
    GEN_AI_REQUEST_STOP_SEQUENCES,
    GEN_AI_REQUEST_STREAM,
    GEN_AI_REQUEST_ENCODING_FORMATS,
    GEN_AI_RESPONSE_MODEL,
    GEN_AI_RESPONSE_ID,
    GEN_AI_RESPONSE_FINISH_REASONS,
    GEN_AI_RESPONSE_TIME_TO_FIRST_CHUNK,
    GEN_AI_USAGE_INPUT_TOKENS,
    GEN_AI_USAGE_OUTPUT_TOKENS,
    GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
    GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
    GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
    GEN_AI_TOOL_CALL_ID,
    GEN_AI_TOOL_NAME,
    GEN_AI_TOOL_TYPE,
    GEN_AI_TOOL_DESCRIPTION,
    GEN_AI_TOOL_DEFINITIONS,
    GEN_AI_TOOL_CALL_ARGUMENTS,
    GEN_AI_TOOL_CALL_RESULT,
    GEN_AI_AGENT_NAME,
    GEN_AI_AGENT_ID,
    GEN_AI_AGENT_DESCRIPTION,
    GEN_AI_AGENT_VERSION,
    GEN_AI_PROMPT_NAME,
    GEN_AI_WORKFLOW_NAME,
    GEN_AI_DATA_SOURCE_ID,
    GEN_AI_EMBEDDINGS_DIMENSION_COUNT,
    GEN_AI_INPUT_MESSAGES,
    GEN_AI_OUTPUT_MESSAGES,
    GEN_AI_SYSTEM_INSTRUCTIONS,
    GEN_AI_RETRIEVAL_DOCUMENTS,
    GEN_AI_RETRIEVAL_QUERY_TEXT,
    GEN_AI_OPENAI_API_TYPE,
    GEN_AI_OPENAI_SERVICE_TIER,
];

/// General OTel semantic convention keys that GenAI spans borrow from the
/// OTel references section.
pub const OTEL_REFERENCED_KEYS: &[&str] = &[SERVER_ADDRESS, SERVER_PORT, ERROR_TYPE];
