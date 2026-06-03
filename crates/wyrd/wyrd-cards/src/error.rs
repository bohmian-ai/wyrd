//! Agent Card boundary errors.

use serde_json::json;
use thiserror::Error;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

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
    /// `AgentBuilder` prompt is required.
    #[error("AgentBuilder requires a prompt")]
    MissingPrompt,
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
            AgentCardError::MissingPrompt => WyrdError::AgentMissingPrompt {
                message: "AgentBuilder requires a prompt".to_owned(),
                details: json!({ "field": "spec.prompt" }),
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

fn card_ref_display(card_ref: &CardRef) -> String {
    let space = card_ref
        .space
        .as_ref()
        .map_or_else(|| "default".to_owned(), ToString::to_string);
    format!(
        "{}/{}/{}@{}",
        space,
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}
