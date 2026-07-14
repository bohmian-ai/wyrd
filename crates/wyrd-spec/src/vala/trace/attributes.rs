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
/// Opt-in captured GenAI system instructions. Canonical in the current OTel
/// GenAI semantic conventions.
pub const GEN_AI_SYSTEM_INSTRUCTIONS: &str = "gen_ai.system_instructions";

/// Retrieved documents used by a GenAI call.
pub const GEN_AI_RETRIEVAL_DOCUMENTS: &str = "gen_ai.retrieval.documents";
/// Retrieval query text.
pub const GEN_AI_RETRIEVAL_QUERY_TEXT: &str = "gen_ai.retrieval.query.text";
/// Retrieval top-k count.
pub const GEN_AI_RETRIEVAL_TOP_K: &str = "gen_ai.retrieval.top_k";

/// Requested model reasoning effort level, such as `low`, `medium`, or `high`.
pub const GEN_AI_REQUEST_REASONING_LEVEL: &str = "gen_ai.request.reasoning.level";
/// Whether the conversation history was compacted before this call.
pub const GEN_AI_CONVERSATION_COMPACTED: &str = "gen_ai.conversation.compacted";
/// Version of the prompt used for this call.
pub const GEN_AI_PROMPT_VERSION: &str = "gen_ai.prompt.version";

/// Memory store identifier.
pub const GEN_AI_MEMORY_STORE_ID: &str = "gen_ai.memory.store.id";
/// Memory record identifier.
pub const GEN_AI_MEMORY_RECORD_ID: &str = "gen_ai.memory.record.id";
/// Number of memory records touched by the operation.
pub const GEN_AI_MEMORY_RECORD_COUNT: &str = "gen_ai.memory.record.count";
/// Memory query text.
pub const GEN_AI_MEMORY_QUERY_TEXT: &str = "gen_ai.memory.query.text";
/// Memory records payload (JSON).
pub const GEN_AI_MEMORY_RECORDS: &str = "gen_ai.memory.records";

/// OpenAI API type, such as `chat_completions` or `responses`. General OTel
/// semantic-convention key in the `openai.*` namespace, without a `gen_ai.`
/// prefix.
pub const OPENAI_API_TYPE: &str = "openai.api.type";
/// OpenAI requested service tier, such as `auto` or `default`. `openai.*`
/// namespace.
pub const OPENAI_REQUEST_SERVICE_TIER: &str = "openai.request.service_tier";
/// OpenAI response service tier. `openai.*` namespace.
pub const OPENAI_RESPONSE_SERVICE_TIER: &str = "openai.response.service_tier";
/// OpenAI response system fingerprint. `openai.*` namespace.
pub const OPENAI_RESPONSE_SYSTEM_FINGERPRINT: &str = "openai.response.system_fingerprint";

/// MCP session identifier. `mcp.*` correlation namespace, without a `gen_ai.`
/// prefix.
pub const MCP_SESSION_ID: &str = "mcp.session.id";
/// MCP method name. `mcp.*` namespace.
pub const MCP_METHOD_NAME: &str = "mcp.method.name";
/// MCP protocol version. `mcp.*` namespace.
pub const MCP_PROTOCOL_VERSION: &str = "mcp.protocol.version";
/// MCP resource URI. `mcp.*` namespace.
pub const MCP_RESOURCE_URI: &str = "mcp.resource.uri";

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
    GEN_AI_RETRIEVAL_TOP_K,
    GEN_AI_REQUEST_REASONING_LEVEL,
    GEN_AI_CONVERSATION_COMPACTED,
    GEN_AI_PROMPT_VERSION,
    GEN_AI_MEMORY_STORE_ID,
    GEN_AI_MEMORY_RECORD_ID,
    GEN_AI_MEMORY_RECORD_COUNT,
    GEN_AI_MEMORY_QUERY_TEXT,
    GEN_AI_MEMORY_RECORDS,
];

/// General OTel semantic convention keys that GenAI spans borrow from the
/// OTel references section.
pub const OTEL_REFERENCED_KEYS: &[&str] = &[SERVER_ADDRESS, SERVER_PORT, ERROR_TYPE];

/// `openai.*` semantic convention keys tracked in typed columns by the trace
/// surface. These live in OpenAI's own namespace, not under `gen_ai.`.
pub const OPENAI_KEYS: &[&str] = &[
    OPENAI_API_TYPE,
    OPENAI_REQUEST_SERVICE_TIER,
    OPENAI_RESPONSE_SERVICE_TIER,
    OPENAI_RESPONSE_SYSTEM_FINGERPRINT,
];

/// `mcp.*` correlation keys tracked in typed columns by the trace surface.
pub const MCP_KEYS: &[&str] = &[
    MCP_SESSION_ID,
    MCP_METHOD_NAME,
    MCP_PROTOCOL_VERSION,
    MCP_RESOURCE_URI,
];
