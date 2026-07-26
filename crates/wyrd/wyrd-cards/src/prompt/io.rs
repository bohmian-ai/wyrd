//! Filesystem IO helpers for local `PromptCard` materialization.

use std::path::Path;

use wyrd_spec::card::prompt::validate::PromptError;
use wyrd_spec::card::prompt::{
    CardLoadFormat, PromptSpec, parse_card_bytes, parse_spec_bytes, serialize_card,
};
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;

/// Read a full Prompt Card envelope from a local JSON or YAML file.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported, the
/// file cannot be read, or the bytes do not decode as a Prompt Card envelope.
pub fn read_card_file(path: &Path) -> Result<Card, WyrdError> {
    let format = format_from_path(path)?;
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    parse_card_bytes(format, &bytes)
}

/// Read a local Prompt Card authoring file and fill local-only identity gaps.
///
/// This helper is intentionally separate from `read_card_file`: server hydration
/// and `model_validate_json` require a complete persisted envelope, while local
/// declarative files may omit API version, space, or UID.
pub fn read_local_card_file(path: &Path) -> Result<Card, WyrdError> {
    let format = format_from_path(path)?;
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    let mut value: serde_json::Value = match format {
        CardLoadFormat::Json => {
            serde_json::from_slice(&bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt card JSON".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })?
        }
        CardLoadFormat::Yaml => {
            serde_yaml::from_slice(&bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt card YAML".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })?
        }
    };
    let object = value.as_object_mut().ok_or_else(|| WyrdError::Validation {
        message: "prompt card authoring file must be an object".to_owned(),
        details: serde_json::Value::Null,
    })?;
    object
        .entry("apiVersion")
        .or_insert_with(|| serde_json::json!("wyrd/v1"));
    object
        .entry("kind")
        .or_insert_with(|| serde_json::json!("Prompt"));
    let metadata = object
        .entry("metadata")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| WyrdError::Validation {
            message: "prompt card metadata must be an object".to_owned(),
            details: serde_json::Value::Null,
        })?;
    metadata
        .entry("space")
        .or_insert_with(|| serde_json::json!("default"));
    metadata
        .entry("name")
        .or_insert_with(|| serde_json::json!("prompt"));
    metadata
        .entry("version")
        .or_insert_with(|| serde_json::json!("0.1.0"));
    metadata
        .entry("uid")
        .or_insert_with(|| serde_json::json!(wyrd_utils::uuid7()));
    serde_json::from_value(value).map_err(|error| WyrdError::Validation {
        message: "failed to parse local prompt card".to_owned(),
        details: serde_json::json!({ "source": error.to_string() }),
    })
}

/// Write a full Prompt Card envelope to a local JSON or YAML file.
///
/// Parent directories are created when needed. The file format is selected from
/// the target extension.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported,
/// serialization fails, or the file cannot be written.
pub fn write_card_file(card: &Card, path: &Path) -> Result<(), WyrdError> {
    let format = format_from_path(path)?;
    let bytes = serialize_card(format, card)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| loader_io(parent, &error))?;
    }
    std::fs::write(path, bytes).map_err(|error| loader_io(path, &error))
}

/// Read a bare `PromptSpec` body from a local JSON or YAML file.
///
/// # Errors
/// Returns a Wyrd prompt loader error when the extension is unsupported, the
/// file cannot be read, or the bytes do not decode as a `PromptSpec`.
pub fn read_spec_file(path: &Path) -> Result<PromptSpec, WyrdError> {
    let format = format_from_path(path)?;
    let bytes = std::fs::read(path).map_err(|error| loader_io(path, &error))?;
    parse_spec_bytes(format, &bytes)
}

fn format_from_path(path: &Path) -> Result<CardLoadFormat, WyrdError> {
    CardLoadFormat::from_extension(path.extension().and_then(std::ffi::OsStr::to_str))
}

fn loader_io(path: &Path, error: &std::io::Error) -> WyrdError {
    PromptError::LoaderIo {
        path: path.display().to_string(),
        message: error.to_string(),
    }
    .into()
}

#[cfg(test)]
mod prompt_io {
    use std::path::PathBuf;

    use crate::prompt::PromptCard;
    use crate::prompt::io::{read_card_file, write_card_file};
    use wyrd_spec::envelope::Spec;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wyrd_prompt_card_s11_{}_{}",
            std::process::id(),
            name
        ))
    }

    fn prompt_card() -> PromptCard {
        let prompt = skald_spec::Prompt::new(
            skald_spec::ProviderRequest::RawV1 {
                provider: skald_spec::ProviderName::Custom("unit".to_owned()),
                body: serde_json::value::RawValue::from_string(
                    r#"{"messages":["hello"]}"#.to_owned(),
                )
                .expect("static raw JSON is valid"),
            },
            "unit-model",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static prompt is valid");
        let mut card = PromptCard::from_native_prompt(prompt);
        "growth".clone_into(&mut card.space);
        "lead-scoring".clone_into(&mut card.name);
        "1.2.3".clone_into(&mut card.version);
        card
    }

    fn typed_prompt_card() -> PromptCard {
        let request = skald_spec::OpenAiChatRequest {
            model: "gpt-4o-mini".to_owned(),
            messages: vec![skald_spec::OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                    "hello".to_owned(),
                )),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
                annotations: Vec::new(),
                audio: None,
            }],
            response_format: None,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings: skald_spec::OpenAiChatSettings::default(),
        };
        let prompt = skald_spec::Prompt::new(
            skald_spec::ProviderRequest::OpenAiChatCompletion(request),
            "gpt-4o-mini",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static typed prompt is valid");
        PromptCard::from_native_prompt(prompt)
    }

    #[test]
    fn json_write_read_round_trip() {
        let path = temp_path("round_trip.json");
        let card = prompt_card().to_card().expect("card is valid");

        write_card_file(&card, &path).expect("write succeeds");
        let loaded = read_card_file(&path).expect("read succeeds");

        assert_eq!(loaded, card);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn yaml_write_read_round_trip() {
        let path = temp_path("round_trip.yaml");
        let card = typed_prompt_card().to_card().expect("card is valid");

        write_card_file(&card, &path).expect("write succeeds");
        let loaded = read_card_file(&path).expect("read succeeds");

        assert_eq!(loaded, card);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn yaml_prompt_card_envelope_with_model_settings_loads() {
        let path = temp_path("authored_card.yaml");
        std::fs::write(
            &path,
            r"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  space: test
  name: yaml-prompt
  version: 0.1.0
  uid: 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00
spec:
  provider: openai
  model: gpt-4o
  messages:
    - hello
  model_settings:
    seed: 123
    future_knob:
      enabled: true
",
        )
        .expect("write fixture");

        let loaded = read_card_file(&path).expect("yaml prompt card loads");
        let card = PromptCard::from_card(loaded).expect("prompt card holder loads");
        let skald_spec::ProviderRequest::OpenAiChatCompletion(request) =
            card.metadata.prompt.request
        else {
            panic!("loaded prompt should be OpenAI chat");
        };

        assert_eq!(request.settings.seed, Some(123));
        assert_eq!(
            request.settings.extra["future_knob"],
            serde_json::json!({ "enabled": true })
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn txt_extension_returns_loader_bad_extension() {
        let path = temp_path("bad.txt");
        let card = prompt_card().to_card().expect("card is valid");

        let error = write_card_file(&card, &path).expect_err("bad extension fails");

        assert_eq!(error.code(), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION");
    }

    #[test]
    fn missing_file_returns_loader_io() {
        let path = temp_path("missing.json");
        let _ = std::fs::remove_file(&path);

        let error = read_card_file(&path).expect_err("missing file fails");

        assert_eq!(error.code(), "WYRD_PROMPT_500_LOADER_IO");
    }

    #[test]
    fn raw_v1_prompt_body_preserves_provider_bytes() {
        let path = temp_path("raw_v1.json");
        let raw = r#"{"a":[true,{"nested":"value"}],"z":2}"#;
        let prompt = skald_spec::Prompt::new(
            skald_spec::ProviderRequest::RawV1 {
                provider: skald_spec::ProviderName::Custom("raw-provider".to_owned()),
                body: serde_json::value::RawValue::from_string(raw.to_owned())
                    .expect("static raw JSON is valid"),
            },
            "raw-model",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static prompt is valid");
        let card = PromptCard::from_native_prompt(prompt)
            .to_card()
            .expect("card is valid");

        write_card_file(&card, &path).expect("write succeeds");
        let loaded = read_card_file(&path).expect("read succeeds");

        let Spec::Prompt(spec) = loaded.spec else {
            panic!("loaded card is a Prompt spec");
        };
        let skald_spec::ProviderRequest::RawV1 { body, .. } = spec.prompt.request else {
            panic!("loaded prompt is RawV1");
        };
        assert_eq!(body.get(), raw);
        let _ = std::fs::remove_file(path);
    }
}
