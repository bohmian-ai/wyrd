//! Sync tests for the attribute-key catalogs.

use wyrd_spec::vala::trace::attributes::*;

#[test]
fn wyrd_keys_in_sync_with_constants() {
    let declared = [
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

    for key in &declared {
        assert!(
            WYRD_KEYS.contains(key),
            "WYRD_KEYS missing {key}; add it to attributes.rs WYRD_KEYS array"
        );
    }
    assert_eq!(
        WYRD_KEYS.len(),
        declared.len(),
        "WYRD_KEYS has {} entries but {} constants are declared above",
        WYRD_KEYS.len(),
        declared.len()
    );
}

#[test]
fn gen_ai_keys_in_sync_with_constants() {
    let declared = [
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

    for key in &declared {
        assert!(
            GEN_AI_KEYS.contains(key),
            "GEN_AI_KEYS missing {key}; add it to attributes.rs GEN_AI_KEYS array"
        );
    }
    assert_eq!(
        GEN_AI_KEYS.len(),
        declared.len(),
        "GEN_AI_KEYS has {} entries but {} typed-column constants are declared above. \
         If you added a typed column to GenAiSpanRecord, also add the constant to \
         GEN_AI_KEYS.",
        GEN_AI_KEYS.len(),
        declared.len()
    );
}

#[test]
fn otel_referenced_keys_in_sync() {
    let declared = [SERVER_ADDRESS, SERVER_PORT, ERROR_TYPE];

    for key in &declared {
        assert!(
            OTEL_REFERENCED_KEYS.contains(key),
            "OTEL_REFERENCED_KEYS missing {key}"
        );
    }
    assert_eq!(OTEL_REFERENCED_KEYS.len(), declared.len());
}

#[test]
fn gen_ai_key_exact_strings_match_otel_spec() {
    assert_eq!(GEN_AI_PROVIDER_NAME, "gen_ai.provider.name");
    assert_eq!(GEN_AI_OPERATION_NAME, "gen_ai.operation.name");
    assert_eq!(GEN_AI_OUTPUT_TYPE, "gen_ai.output.type");
    assert_eq!(GEN_AI_CONVERSATION_ID, "gen_ai.conversation.id");
    assert_eq!(GEN_AI_SYSTEM_INSTRUCTIONS, "gen_ai.system_instructions");
    assert_eq!(
        GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
        "gen_ai.usage.cache_creation.input_tokens"
    );
    assert_eq!(
        GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
        "gen_ai.usage.cache_read.input_tokens"
    );
    assert_eq!(
        GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
        "gen_ai.usage.reasoning.output_tokens"
    );
    assert_eq!(GEN_AI_TOOL_DESCRIPTION, "gen_ai.tool.description");
    assert_eq!(
        GEN_AI_RESPONSE_TIME_TO_FIRST_CHUNK,
        "gen_ai.response.time_to_first_chunk"
    );
    assert_eq!(SERVER_ADDRESS, "server.address");
    assert_eq!(SERVER_PORT, "server.port");
    assert_eq!(ERROR_TYPE, "error.type");
}

#[test]
fn evaluation_event_constants_are_not_in_gen_ai_keys() {
    for key in [
        GEN_AI_EVALUATION_NAME,
        GEN_AI_EVALUATION_SCORE_LABEL,
        GEN_AI_EVALUATION_SCORE_VALUE,
        GEN_AI_EVALUATION_EXPLANATION,
    ] {
        assert!(
            !GEN_AI_KEYS.contains(&key),
            "{key} should NOT be in GEN_AI_KEYS (event attribute, not span attribute)"
        );
    }
}
