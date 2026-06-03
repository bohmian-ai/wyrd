//! Agent Card envelope holder.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, PromptRef};
use wyrd_spec::version::{ApiVersion, VersionBlock};

use crate::agent::derive_cascade_children;
use crate::error::AgentCardError;

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
                version: version_block("metadata.version", &self.version)?,
                space: optional_space_name(&self.space)?,
                uid: optional_card_uid(&self.uid)?,
                labels: self.labels.clone(),
                annotations: self.annotations.clone(),
                spec_hash: None,
                artifact_hash: None,
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
            version: card.metadata.version.to_string(),
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
            space: optional_space_name(&self.space)?,
            uid: optional_card_uid(&self.uid)?,
        })
    }

    /// Write this Agent Card YAML envelope to local disk.
    ///
    /// # Errors
    /// Returns IO, YAML, or validation errors.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), WyrdError> {
        let path = path.as_ref();
        let yaml = serde_yaml::to_string(self).map_err(|error| AgentCardError::yaml(&error))?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| AgentCardError::io(parent.display().to_string(), &error))?;
        }
        std::fs::write(path, yaml)
            .map_err(|error| AgentCardError::io(path.display().to_string(), &error))?;
        Ok(())
    }

    /// Load this Agent Card YAML envelope from local disk.
    ///
    /// # Errors
    /// Returns IO, YAML, or validation errors.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, WyrdError> {
        let path = path.as_ref();
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| AgentCardError::io(path.display().to_string(), &error))?;
        serde_yaml::from_str(&yaml).map_err(|error| WyrdError::from(AgentCardError::yaml(&error)))
    }
}

impl Serialize for AgentCard {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value =
            serde_yaml::to_value(self.to_envelope().map_err(serde::ser::Error::custom)?)
                .map_err(serde::ser::Error::custom)?;
        if let serde_yaml::Value::Mapping(mapping) = &mut value {
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

fn optional_space_name(value: &str) -> Result<Option<SpaceName>, WyrdError> {
    if value.is_empty() || value == "default" {
        return Ok(None);
    }
    SpaceName::new(value).map(Some).map_err(|error| {
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

#[allow(dead_code)]
fn _prompt_ref_keeps_card_ref(ref_: &PromptRef) -> Option<&CardRef> {
    match ref_ {
        PromptRef::Card(card_ref) => Some(card_ref),
        PromptRef::Inline(_) => None,
    }
}
