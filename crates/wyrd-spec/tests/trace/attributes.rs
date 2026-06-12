use wyrd_spec::vala::trace::attributes::*;

#[test]
fn wyrd_namespace_constants_have_wyrd_prefix() {
    for key in WYRD_KEYS {
        assert!(key.starts_with("wyrd."), "{key} does not have wyrd. prefix");
    }
}

#[test]
fn gen_ai_namespace_constants_have_gen_ai_prefix() {
    for key in GEN_AI_KEYS {
        assert!(
            key.starts_with("gen_ai."),
            "{key} does not have gen_ai. prefix"
        );
    }
}

#[test]
fn tag_prefix_ends_with_dot() {
    assert!(TAG_PREFIX.ends_with('.'));
    assert!(TAG_PREFIX.starts_with("wyrd.tracing.tag."));
}

#[test]
fn baggage_prefix_matches_w3c() {
    assert_eq!(BAGGAGE_PREFIX, "baggage.");
}

#[test]
fn gen_ai_evaluation_event_name_is_canonical() {
    assert_eq!(GEN_AI_EVALUATION_EVENT, "gen_ai.evaluation.result");
}

#[test]
fn gen_ai_provider_name_matches_otel_spec() {
    assert_eq!(GEN_AI_PROVIDER_NAME, "gen_ai.provider.name");
}

#[test]
fn gen_ai_system_instructions_uses_underscore_not_dot() {
    assert_eq!(GEN_AI_SYSTEM_INSTRUCTIONS, "gen_ai.system_instructions");
}

#[test]
fn gen_ai_cache_token_keys_use_dot_subnamespace() {
    assert_eq!(
        GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
        "gen_ai.usage.cache_creation.input_tokens"
    );
    assert_eq!(
        GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
        "gen_ai.usage.cache_read.input_tokens"
    );
}

#[test]
fn gen_ai_reasoning_output_tokens_key_matches_otel_spec() {
    assert_eq!(
        GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
        "gen_ai.usage.reasoning.output_tokens"
    );
}

#[test]
fn server_and_error_keys_have_no_gen_ai_prefix() {
    assert_eq!(SERVER_ADDRESS, "server.address");
    assert_eq!(SERVER_PORT, "server.port");
    assert_eq!(ERROR_TYPE, "error.type");
}

#[test]
fn wyrd_keys_count_locked() {
    assert_eq!(WYRD_KEYS.len(), 9);
}

#[test]
fn gen_ai_keys_count_locked() {
    assert_eq!(GEN_AI_KEYS.len(), 47);
}

#[test]
fn otel_referenced_keys_count_locked() {
    assert_eq!(OTEL_REFERENCED_KEYS.len(), 3);
}

#[test]
fn otel_referenced_keys_do_not_overlap_gen_ai_keys() {
    for key in OTEL_REFERENCED_KEYS {
        assert!(
            !GEN_AI_KEYS.contains(key),
            "{key} is in OTEL_REFERENCED_KEYS; it must NOT also appear in GEN_AI_KEYS"
        );
        assert!(
            !key.starts_with("gen_ai."),
            "{key} is OTEL_REFERENCED_KEYS but starts with gen_ai."
        );
    }
}
