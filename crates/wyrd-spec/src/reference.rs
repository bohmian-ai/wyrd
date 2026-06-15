//! Card references authored inside specs.

use serde::{Deserialize, Serialize};

use crate::card::agent::AgentSpec;
use crate::envelope::CardKind;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::version::VersionBlock;

/// Reference to a registered Card by kind, name, version, space, and optional UID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CardRef {
    /// Referenced Card kind.
    pub kind: CardKind,
    /// Referenced Card name.
    pub name: CardName,
    /// Exact referenced Card version.
    pub version: VersionBlock,
    /// Space pinning identity together with `name` and `version`.
    pub space: SpaceName,
    /// Optional resolved UID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<CardUid>,
}

/// Reference to an agent prompt, either inline or by Prompt Card reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PromptRef {
    /// Inline native prompt payload.
    Inline(Box<skald_spec::Prompt>),
    /// Reference to a registered Prompt Card.
    Card(CardRef),
}

impl From<skald_spec::Prompt> for PromptRef {
    fn from(prompt: skald_spec::Prompt) -> Self {
        Self::Inline(Box::new(prompt))
    }
}

impl From<CardRef> for PromptRef {
    fn from(card_ref: CardRef) -> Self {
        Self::Card(card_ref)
    }
}

/// Reference to a workflow step's agent, either inline or by Agent Card reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum AgentRef {
    /// Inline agent spec body.
    Inline(Box<AgentSpec>),
    /// Reference to a registered Agent Card.
    Card(CardRef),
}

impl From<AgentSpec> for AgentRef {
    fn from(spec: AgentSpec) -> Self {
        Self::Inline(Box::new(spec))
    }
}

impl From<CardRef> for AgentRef {
    fn from(card_ref: CardRef) -> Self {
        Self::Card(card_ref)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ref() -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new("weights").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    #[test]
    fn card_ref_serde_roundtrips() {
        let card_ref = sample_ref();
        let json = serde_json::to_string(&card_ref).expect("serialize");
        let parsed: CardRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(card_ref, parsed);
    }

    #[test]
    fn card_ref_skips_none_uid() {
        let card_ref = CardRef {
            kind: CardKind::Model,
            name: CardName::new("churn").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        };
        let json = serde_json::to_string(&card_ref).expect("serialize");
        assert!(
            json.contains(r#""space":"default""#),
            "space must serialize"
        );
        assert!(!json.contains("uid"), "uid=None must skip");
    }

    #[test]
    fn card_ref_rejects_missing_space() {
        let json = r#"{"kind":"Model","name":"churn","version":"1.0.0"}"#;
        let err = serde_json::from_str::<CardRef>(json).expect_err("space is required");
        assert!(
            err.to_string().contains("space"),
            "error must mention space: {err}"
        );
    }
}
