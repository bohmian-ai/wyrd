mod common;

#[test]
fn openai_chat_request_matches_fixture() {
    insta::assert_json_snapshot!(common::openai_chat_request());
}

#[test]
fn openai_chat_response_matches_fixture() {
    insta::assert_json_snapshot!(common::openai_chat_response());
}

#[test]
fn openai_responses_request_matches_fixture() {
    insta::assert_json_snapshot!(common::openai_responses_request());
}

#[test]
fn openai_responses_response_matches_fixture() {
    insta::assert_json_snapshot!(common::openai_responses_response());
}

#[test]
fn anthropic_messages_request_matches_fixture() {
    insta::assert_json_snapshot!(common::anthropic_request());
}

#[test]
fn anthropic_messages_response_matches_fixture() {
    insta::assert_json_snapshot!(common::anthropic_response(
        skald_spec::wire::anthropic_messages::AnthropicStopReason::EndTurn
    ));
}

#[test]
fn anthropic_with_cache_control_matches_fixture() {
    insta::assert_json_snapshot!(common::anthropic_request().system);
}

#[test]
fn google_generate_content_request_matches_fixture() {
    insta::assert_json_snapshot!(common::google_request());
}

#[test]
fn google_generate_content_response_matches_fixture() {
    insta::assert_json_snapshot!(common::google_response(
        skald_spec::wire::google_generate::GoogleFinishReason::Stop
    ));
}

#[test]
fn vertex_generate_request_matches_fixture() {
    insta::assert_json_snapshot!(common::vertex_generate_request());
}

#[test]
fn vertex_predict_request_matches_fixture() {
    insta::assert_json_snapshot!(common::vertex_predict_request());
}
