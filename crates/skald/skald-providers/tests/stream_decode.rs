use skald_providers::stream::{JsonlDecoder, decode_jsonl_events, decode_sse_events};
use skald_spec::{GoogleGenerateContentResponse, OpenAiChatStreamChunk};

#[test]
fn sse_parses_data_lines_and_skips_keepalive() {
    let payload = br#": keepalive
data: {"id":"chunk","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[]}
data: [DONE]
"#;

    let events: Vec<OpenAiChatStreamChunk> =
        decode_sse_events("openai", payload).expect("sse decodes");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "chunk");
}

#[test]
fn jsonl_handles_partial_line_buffer() {
    let mut decoder = JsonlDecoder::default();
    let first: Vec<GoogleGenerateContentResponse> = decoder
        .push("google", br#"{"candidates":[]"#)
        .expect("partial line accepted");
    let second: Vec<GoogleGenerateContentResponse> = decoder
        .push("google", br#","usage_metadata":{"total_token_count":1}}"#)
        .expect("line still partial");
    let third: Vec<GoogleGenerateContentResponse> =
        decoder.push("google", b"\n").expect("line decodes");

    assert!(first.is_empty());
    assert!(second.is_empty());
    assert_eq!(third.len(), 1);
}

#[test]
fn jsonl_decodes_complete_payload() {
    let events: Vec<GoogleGenerateContentResponse> =
        decode_jsonl_events("google", br#"{"candidates":[]}"#).expect("jsonl decodes");

    assert_eq!(events.len(), 1);
}
