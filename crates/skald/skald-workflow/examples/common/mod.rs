use serde_json::json;
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};

pub fn plan_prompt() -> Prompt {
    openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["Plan research on: ${topic}".to_owned()],
            output: Some(
                ResponseFormat::json_schema(
                    "plan",
                    json!({
                        "type": "object",
                        "properties": {
                            "summary": {"type": "string"},
                            "steps": {"type": "array", "items": {"type": "string"}}
                        },
                        "required": ["summary", "steps"],
                        "additionalProperties": false
                    }),
                )
                .expect("static schema is valid"),
            ),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

pub fn write_prompt() -> Prompt {
    openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["Draft a brief on: ${summary}".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}
