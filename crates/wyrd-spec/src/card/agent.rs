//! Agent Card durable contract.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use thiserror::Error;

use crate::api_version::ApiVersion;
use crate::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use crate::error::WyrdError;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::reference::{CardRef, PromptRef};
use wyrd_semver::VersionBlock;

/// Pure-serde mirror of the Skald agent run configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentRunConfigSpec {
    /// Maximum loop iterations before the agent stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// Maximum concurrent runtime-local tool calls per model iteration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_concurrency_cap: Option<usize>,
    /// Maximum recent session turns loaded at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_recent_limit: Option<usize>,
    /// Overall agent run timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Agent authoring body inside a Wyrd Agent Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentSpec {
    /// Prompt reference preserved on disk and resolved by `skald-agent`.
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub prompt: PromptRef,
    /// Runtime-local tool names resolved from a local Skald tool registry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    /// Agent run configuration.
    #[serde(default, skip_serializing_if = "is_default_run_config")]
    pub run_config: AgentRunConfigSpec,
}

fn is_default_run_config(config: &AgentRunConfigSpec) -> bool {
    config == &AgentRunConfigSpec::default()
}

/// Local typed holder for a Wyrd Agent Card envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentCard {
    /// Logical card space.
    pub space: String,
    /// User-authored card name.
    pub name: String,
    /// Exact card version.
    pub version: String,
    /// Server-assigned card UID, empty before registration.
    pub uid: String,
    /// Queryable labels.
    pub labels: Labels,
    /// Free-form annotations.
    pub annotations: Annotations,
    /// Agent Card spec body.
    pub spec: AgentSpec,
    /// Derived prompt cascade children.
    pub cascade_children: Vec<CardRef>,
    /// Local creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl AgentCard {
    /// Convert this typed holder into the shared Wyrd `Card` envelope.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn to_envelope(&self) -> Result<Card, WyrdError> {
        Ok(Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Agent,
            metadata: EnvelopeMetadata {
                name: card_name("metadata.name", &self.name)?,
                version: Some(version_block("metadata.version", &self.version)?.into()),
                bump: None,
                space: Some(space_name(&self.space)?),
                uid: optional_card_uid(&self.uid)?,
                labels: self.labels.clone(),
                annotations: self.annotations.clone(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Agent(self.spec.clone()),
            relationships: Relationships::default(),
            status: None,
        })
    }

    /// Convert a shared Wyrd `Card` envelope into a typed Agent Card holder.
    ///
    /// # Errors
    /// Returns validation errors when the envelope is not an Agent Card.
    pub fn from_envelope(card: Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 {
            return Err(AgentCardError::validation(format!(
                "expected apiVersion wyrd/v1, got {}",
                card.api_version
            ))
            .into());
        }
        if card.kind != CardKind::Agent {
            return Err(AgentCardError::validation(format!(
                "expected kind Agent, got {}",
                card.kind.wire_name()
            ))
            .into());
        }

        let Spec::Agent(spec) = card.spec else {
            return Err(AgentCardError::validation("Agent Card spec must be an Agent spec").into());
        };

        let cascade_children = derive_cascade_children(&spec);
        Ok(Self {
            space: card
                .metadata
                .space
                .as_ref()
                .map_or_else(|| "default".to_owned(), ToString::to_string),
            name: card.metadata.name.to_string(),
            version: card
                .metadata
                .resolved_pin()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    AgentCardError::validation("Agent Card envelope missing resolved version pin")
                })?,
            uid: card
                .metadata
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            labels: card.metadata.labels,
            annotations: card.metadata.annotations,
            spec,
            cascade_children,
            created_at: Utc::now(),
        })
    }

    /// Convert this Agent Card identity into a `CardRef`.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Agent,
            name: card_name("metadata.name", &self.name)?,
            version: version_block("metadata.version", &self.version)?,
            space: space_name(&self.space)?,
            uid: optional_card_uid(&self.uid)?,
        })
    }
}

impl Serialize for AgentCard {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value =
            serde_yaml::to_value(self.to_envelope().map_err(serde::ser::Error::custom)?)
                .map_err(serde::ser::Error::custom)?;
        if let serde_yaml::Value::Mapping(mapping) = &mut value {
            mapping.insert(
                serde_yaml::Value::String("relationships".to_owned()),
                serde_yaml::Value::Mapping({
                    let mut relationships = serde_yaml::Mapping::new();
                    relationships.insert(
                        serde_yaml::Value::String("outbound".to_owned()),
                        serde_yaml::Value::Sequence(Vec::new()),
                    );
                    relationships
                }),
            );
            mapping.insert(
                serde_yaml::Value::String("status".to_owned()),
                serde_yaml::Value::Null,
            );
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentCard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let card = Card::deserialize(deserializer)?;
        Self::from_envelope(card).map_err(serde::de::Error::custom)
    }
}

/// Agent Card local boundary errors.
#[derive(Debug, Error)]
pub enum AgentCardError {
    /// Agent Card validation failed.
    #[error("AgentCard validation failed: {detail}")]
    Validation {
        /// Validation detail.
        detail: String,
    },
    /// Agent Card name is required.
    #[error("AgentCard name is required before saving")]
    MissingName,
    /// Agent Card version is required.
    #[error("AgentCard version is required before saving")]
    MissingVersion,
    /// Referenced prompt card is missing.
    #[error("Prompt Card not found: {card_ref:?}")]
    PromptCardNotFound {
        /// Missing Prompt Card reference.
        card_ref: CardRef,
    },
    /// Runtime-local tool name was not resolved.
    #[error("runtime-local tool `{name}` was not found")]
    RuntimeLocalToolNotFound {
        /// Requested tool name.
        name: String,
        /// Available tool names, when known.
        available: Vec<String>,
    },
    /// Runtime-local tools cannot be registered durably.
    #[error("runtime-local tool names are not registrable: {tool_names:?}")]
    RuntimeLocalToolsNotRegistrable {
        /// Runtime-local tool names.
        tool_names: Vec<String>,
    },
    /// Agent Card filesystem IO failed.
    #[error("AgentCard IO failed at {path}: {message}")]
    Io {
        /// Path being read or written.
        path: String,
        /// IO detail.
        message: String,
    },
    /// Agent Card YAML codec failed.
    #[error("AgentCard YAML codec failed: {message}")]
    Yaml {
        /// YAML detail.
        message: String,
    },
}

impl AgentCardError {
    /// Build an Agent Card validation error.
    pub fn validation(detail: impl Into<String>) -> Self {
        Self::Validation {
            detail: detail.into(),
        }
    }

    /// Build an Agent Card IO error.
    pub fn io(path: impl Into<String>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            message: error.to_string(),
        }
    }

    /// Build an Agent Card YAML error.
    pub fn yaml(error: &serde_yaml::Error) -> Self {
        Self::Yaml {
            message: error.to_string(),
        }
    }
}

impl From<AgentCardError> for WyrdError {
    fn from(error: AgentCardError) -> Self {
        match error {
            AgentCardError::Validation { detail } => WyrdError::AgentValidation {
                message: detail.clone(),
                details: json!({ "detail": detail }),
            },
            AgentCardError::MissingName => WyrdError::AgentMissingName {
                message: "AgentCard name is required before saving".to_owned(),
                details: json!({ "field": "metadata.name" }),
            },
            AgentCardError::MissingVersion => WyrdError::AgentMissingVersion {
                message: "AgentCard version is required before saving".to_owned(),
                details: json!({ "field": "metadata.version" }),
            },
            AgentCardError::PromptCardNotFound { card_ref } => WyrdError::AgentPromptCardNotFound {
                message: format!("Prompt Card not found: {}", card_ref_display(&card_ref)),
                details: json!({ "card_ref": card_ref }),
            },
            AgentCardError::RuntimeLocalToolNotFound { name, available } => {
                WyrdError::AgentRuntimeLocalToolNotFound {
                    message: format!("runtime-local tool `{name}` was not found"),
                    details: json!({ "name": name, "available": available }),
                }
            }
            AgentCardError::RuntimeLocalToolsNotRegistrable { tool_names } => {
                WyrdError::AgentRuntimeLocalToolsNotRegistrable {
                    message: "runtime-local tool names are not registrable".to_owned(),
                    details: json!({ "tool_names": tool_names }),
                }
            }
            AgentCardError::Io { path, message } => WyrdError::AgentValidation {
                message: format!("AgentCard IO failed at {path}: {message}"),
                details: json!({ "path": path, "source": message }),
            },
            AgentCardError::Yaml { message } => WyrdError::AgentValidation {
                message: format!("AgentCard YAML codec failed: {message}"),
                details: json!({ "source": message }),
            },
        }
    }
}

/// Return card refs declared by an agent for cascade and scope traversal.
fn derive_cascade_children(spec: &AgentSpec) -> Vec<CardRef> {
    match &spec.prompt {
        PromptRef::Card(card_ref) => vec![card_ref.clone()],
        PromptRef::Inline(_) => Vec::new(),
    }
}

/// Return card refs declared by an agent for card-ref scope traversal.
pub(crate) fn scope_child_card_refs(spec: &AgentSpec) -> Vec<CardRef> {
    derive_cascade_children(spec)
}

fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value).map_err(|error| {
        AgentCardError::validation(format!("{field} must be a valid CardName: {error}")).into()
    })
}

fn version_block(field: &str, value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value).map_err(|error| {
        AgentCardError::validation(format!("{field} must be a semantic version: {error}")).into()
    })
}

fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(
            AgentCardError::validation("metadata.space is required and cannot be empty").into(),
        );
    }
    SpaceName::new(value).map_err(|error| {
        AgentCardError::validation(format!("metadata.space is invalid: {error}")).into()
    })
}

fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        return Ok(None);
    }
    CardUid::new(value).map(Some).map_err(|error| {
        AgentCardError::validation(format!("metadata.uid is invalid: {error}")).into()
    })
}

fn card_ref_display(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}@{}",
        card_ref.space,
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}

#[cfg(test)]
mod agent_spec_tests {
    use crate::card::agent::{AgentRunConfigSpec, AgentSpec};
    use crate::envelope::CardKind;
    use crate::reference::{CardRef, PromptRef};

    fn prompt() -> skald_spec::Prompt {
        skald_spec::Prompt::new(
            skald_spec::ProviderRequest::OpenAiChatCompletion(skald_spec::OpenAiChatRequest {
                model: "gpt-4o-mini".to_owned(),
                messages: vec![skald_spec::OpenAiChatMessage {
                    role: "user".to_owned(),
                    content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                        "plan".to_owned(),
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
            }),
            "gpt-4o-mini",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static prompt is valid")
    }

    #[test]
    fn agent_spec_inline_prompt_roundtrips() {
        let spec = AgentSpec {
            prompt: PromptRef::from(prompt()),
            tool_names: vec!["search_docs".to_owned()],
            run_config: AgentRunConfigSpec {
                max_iterations: Some(7),
                tool_concurrency_cap: Some(2),
                session_recent_limit: Some(5),
                timeout_ms: Some(1_500),
            },
        };

        let json = serde_json::to_string(&spec).expect("serialize");
        let decoded: AgentSpec = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, spec);
        assert!(json.contains("session_recent_limit"));
        assert!(!json.contains(r#""kind":"inline""#));
        assert!(!json.contains(r#""value":"#));
    }

    #[test]
    fn agent_spec_card_prompt_uses_single_version_field() {
        let spec = AgentSpec {
            prompt: PromptRef::from(CardRef {
                kind: CardKind::Prompt,
                name: "planner-prompt".parse().expect("valid card name"),
                version: "0.3.0".parse().expect("valid version"),
                space: "research".parse().expect("valid space"),
                uid: None,
            }),
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec::default(),
        };

        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let decoded: AgentSpec = serde_yaml::from_str(&yaml).expect("deserialize");

        assert_eq!(decoded, spec);
        assert!(yaml.contains("version: 0.3.0"));
        assert!(yaml.contains("kind: Prompt"));
        assert!(!yaml.contains("kind: card"));
        assert!(!yaml.contains("value:"));
        assert!(!yaml.contains(&format!("{}{}", "version", "_req")));
        assert!(!yaml.contains(&format!("{}{}", "version: ", "^")));
    }
}
