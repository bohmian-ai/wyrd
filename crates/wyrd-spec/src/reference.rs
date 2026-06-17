//! Card references authored inside specs.

use std::fmt;
use std::str::FromStr;

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

impl fmt::Display for CardRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}@{}",
            self.space,
            self.kind.wire_name(),
            self.name,
            self.version
        )?;
        if let Some(uid) = &self.uid {
            write!(f, "#{uid}")?;
        }
        Ok(())
    }
}

impl FromStr for CardRef {
    type Err = CardRefParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (head, uid) = match value.split_once('#') {
            Some((head, uid)) => (head, Some(uid.parse().map_err(CardRefParseError::Uid)?)),
            None => (value, None),
        };
        let (before_version, version) = head
            .rsplit_once('@')
            .ok_or(CardRefParseError::MissingVersion)?;
        let mut parts = before_version.split('/');
        let space = parts
            .next()
            .ok_or(CardRefParseError::MissingSpace)?
            .parse()
            .map_err(CardRefParseError::Space)?;
        let kind = parts
            .next()
            .ok_or(CardRefParseError::MissingKind)
            .and_then(parse_card_kind)?;
        let name = parts
            .next()
            .ok_or(CardRefParseError::MissingName)?
            .parse()
            .map_err(CardRefParseError::Name)?;
        if parts.next().is_some() {
            return Err(CardRefParseError::TooManySegments);
        }

        Ok(Self {
            kind,
            name,
            version: version.parse().map_err(CardRefParseError::Version)?,
            space,
            uid,
        })
    }
}

fn parse_card_kind(value: &str) -> Result<CardKind, CardRefParseError> {
    CardKind::native()
        .into_iter()
        .find(|kind| kind.wire_name() == value)
        .ok_or_else(|| CardRefParseError::Kind(value.to_owned()))
}

/// Card reference text parse error.
#[derive(Debug, thiserror::Error)]
pub enum CardRefParseError {
    /// Missing space segment.
    #[error("card ref is missing space")]
    MissingSpace,
    /// Missing kind segment.
    #[error("card ref is missing kind")]
    MissingKind,
    /// Missing name segment.
    #[error("card ref is missing name")]
    MissingName,
    /// Missing version suffix.
    #[error("card ref is missing @version")]
    MissingVersion,
    /// Too many slash-delimited segments.
    #[error("card ref must be space/kind/name@version[#uid]")]
    TooManySegments,
    /// Unknown card kind.
    #[error("unknown card kind: {0}")]
    Kind(String),
    /// Space failed validation.
    #[error("invalid space: {0}")]
    Space(crate::ids::IdError),
    /// Name failed validation.
    #[error("invalid name: {0}")]
    Name(crate::ids::IdError),
    /// Version failed validation.
    #[error("invalid version: {0}")]
    Version(crate::version::VersionError),
    /// UID failed validation.
    #[error("invalid uid: {0}")]
    Uid(crate::ids::IdError),
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
    fn card_ref_display_and_from_str_roundtrip() {
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("billing").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        };

        let text = card_ref.to_string();
        let parsed: CardRef = text.parse().expect("card ref parses");

        assert_eq!(text, "prod/Service/billing@1.0.0");
        assert_eq!(parsed, card_ref);
    }

    #[test]
    fn card_ref_canonical_round_trip() {
        let card_ref = sample_ref();

        let parsed: CardRef = card_ref.to_string().parse().expect("card ref parses");

        assert_eq!(parsed, card_ref);
    }

    #[test]
    fn card_ref_with_uid_round_trip() {
        let card_ref = CardRef {
            uid: Some(
                CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11").expect("static uid is valid"),
            ),
            ..sample_ref()
        };

        let text = card_ref.to_string();
        let parsed: CardRef = text.parse().expect("card ref parses");

        assert_eq!(
            text,
            "prod/Artifact/weights@1.0.0#01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11"
        );
        assert_eq!(parsed, card_ref);
    }

    #[test]
    fn card_ref_display_fromstr_inverse() {
        let card_ref = sample_ref();

        let parsed: CardRef = card_ref.to_string().parse().expect("card ref parses");

        assert_eq!(parsed.to_string(), card_ref.to_string());
    }

    #[test]
    fn card_ref_canonical_bytes_pinned() {
        let card_ref = CardRef {
            uid: Some(
                CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11").expect("static uid is valid"),
            ),
            ..sample_ref()
        };

        let bytes = serde_json::to_vec(&card_ref).expect("card ref serializes");

        assert_eq!(
            std::str::from_utf8(&bytes).expect("json is utf8"),
            r#"{"kind":"Artifact","name":"weights","version":"1.0.0","space":"prod","uid":"01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11"}"#
        );
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
