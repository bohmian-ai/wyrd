//! Workflow Card spec.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use thiserror::Error;

use crate::card::common::{Governance, NonSecretValue, ObservationHooks, ParameterValue};
use crate::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use crate::error::WyrdError;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::reference::{AgentRef, CardRef, PromptRef};
use crate::version::{ApiVersion, VersionBlock};

/// Declarative workflow definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowSpec {
    /// Workflow description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Typed workflow inputs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Workflow steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<WorkflowStep>,
    /// Output descriptors.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, serde_json::Value>,
    /// Governance and audit controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Observation hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_hooks: Option<ObservationHooks>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

impl WorkflowSpec {
    /// Validate duplicate step IDs, missing dependencies, and cycles.
    ///
    /// # Errors
    /// Returns a validation error when the workflow is not a DAG.
    pub fn validate_dag(&self) -> Result<(), WorkflowValidationError> {
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if !ids.insert(step.id.clone()) {
                return Err(WorkflowValidationError::DuplicateStep);
            }
        }
        for step in &self.steps {
            for dependency in &step.depends_on {
                if !ids.contains(dependency) {
                    return Err(WorkflowValidationError::MissingDependency);
                }
            }
        }
        for step in &self.steps {
            let mut visiting = BTreeSet::new();
            if self.has_cycle(&step.id, &mut visiting, &mut BTreeSet::new()) {
                return Err(WorkflowValidationError::Cycle);
            }
        }
        Ok(())
    }

    fn has_cycle(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        if visited.contains(id) {
            return false;
        }
        if !visiting.insert(id.to_string()) {
            return true;
        }
        let Some(step) = self.steps.iter().find(|step| step.id == id) else {
            return false;
        };
        for dependency in &step.depends_on {
            if self.has_cycle(dependency, visiting, visited) {
                return true;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        false
    }
}

/// One workflow step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowStep {
    /// Stable step ID.
    pub id: String,
    /// Step action kind.
    pub action: WorkflowAction,
    /// Dependency step IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Step inputs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Optional condition expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<WorkflowRetryPolicy>,
    /// Display metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: BTreeMap<String, NonSecretValue>,
}

/// Workflow retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowRetryPolicy {
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Initial backoff in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,
}

/// Workflow action target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "target", rename_all = "snake_case")]
pub enum WorkflowAction {
    /// Agent action — inline body or Agent Card reference.
    Agent(AgentRef),
    /// MCP server action.
    Mcp(CardRef),
    /// Prompt action.
    Prompt(CardRef),
}

/// Workflow validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowValidationError {
    /// Duplicate step ID.
    #[error("workflow contains a duplicate step id")]
    DuplicateStep,
    /// Missing dependency.
    #[error("workflow references a missing dependency")]
    MissingDependency,
    /// Workflow graph contains a cycle.
    #[error("workflow contains a dependency cycle")]
    Cycle,
}

/// Local typed holder for a Wyrd Workflow Card envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowCard {
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
    /// Workflow Card spec body.
    pub spec: WorkflowSpec,
    /// Derived cascade children (Agent / Prompt / Mcp refs).
    pub cascade_children: Vec<CardRef>,
    /// Local creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl WorkflowCard {
    /// Convert this typed holder into the shared Wyrd `Card` envelope.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn to_envelope(&self) -> Result<Card, WyrdError> {
        Ok(Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Workflow,
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
            spec: Spec::Workflow(self.spec.clone()),
            relationships: Relationships::default(),
            status: None,
        })
    }

    /// Convert a shared Wyrd `Card` envelope into a typed Workflow Card holder.
    ///
    /// # Errors
    /// Returns validation errors when the envelope is not a Workflow Card.
    pub fn from_envelope(card: Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 {
            return Err(WorkflowCardError::validation(format!(
                "expected apiVersion wyrd/v1, got {}",
                card.api_version
            ))
            .into());
        }
        if card.kind != CardKind::Workflow {
            return Err(WorkflowCardError::validation(format!(
                "expected kind Workflow, got {}",
                card.kind.wire_name()
            ))
            .into());
        }

        let Spec::Workflow(spec) = card.spec else {
            return Err(WorkflowCardError::validation(
                "Workflow Card spec must be a Workflow spec",
            )
            .into());
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

    /// Convert this Workflow Card identity into a `CardRef`.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Workflow,
            name: card_name("metadata.name", &self.name)?,
            version: version_block("metadata.version", &self.version)?,
            space: optional_space_name(&self.space)?,
            uid: optional_card_uid(&self.uid)?,
        })
    }
}

impl Serialize for WorkflowCard {
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

impl<'de> Deserialize<'de> for WorkflowCard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let card = Card::deserialize(deserializer)?;
        Self::from_envelope(card).map_err(serde::de::Error::custom)
    }
}

/// Workflow Card local boundary errors.
#[derive(Debug, Error)]
pub enum WorkflowCardError {
    /// Workflow Card validation failed.
    #[error("WorkflowCard validation failed: {detail}")]
    Validation {
        /// Validation detail.
        detail: String,
    },
    /// Workflow Card name is required.
    #[error("WorkflowCard name is required before saving")]
    MissingName,
    /// Workflow Card version is required.
    #[error("WorkflowCard version is required before saving")]
    MissingVersion,
    /// Workflow Card filesystem IO failed.
    #[error("WorkflowCard IO failed at {path}: {message}")]
    Io {
        /// Path being read or written.
        path: String,
        /// IO detail.
        message: String,
    },
    /// Workflow Card YAML codec failed.
    #[error("WorkflowCard YAML codec failed: {message}")]
    Yaml {
        /// YAML detail.
        message: String,
    },
}

impl WorkflowCardError {
    /// Build a Workflow Card validation error.
    pub fn validation(detail: impl Into<String>) -> Self {
        Self::Validation {
            detail: detail.into(),
        }
    }

    /// Build a Workflow Card IO error.
    pub fn io(path: impl Into<String>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            message: error.to_string(),
        }
    }

    /// Build a Workflow Card YAML error.
    pub fn yaml(error: &serde_yaml::Error) -> Self {
        Self::Yaml {
            message: error.to_string(),
        }
    }
}

impl From<WorkflowCardError> for WyrdError {
    fn from(error: WorkflowCardError) -> Self {
        match error {
            WorkflowCardError::Validation { detail } => WyrdError::WorkflowValidation {
                message: detail.clone(),
                details: json!({ "detail": detail }),
            },
            WorkflowCardError::MissingName => WyrdError::WorkflowMissingName {
                message: "WorkflowCard name is required before saving".to_owned(),
                details: json!({ "field": "metadata.name" }),
            },
            WorkflowCardError::MissingVersion => WyrdError::WorkflowMissingVersion {
                message: "WorkflowCard version is required before saving".to_owned(),
                details: json!({ "field": "metadata.version" }),
            },
            WorkflowCardError::Io { path, message } => WyrdError::WorkflowValidation {
                message: format!("WorkflowCard IO failed at {path}: {message}"),
                details: json!({ "path": path, "source": message }),
            },
            WorkflowCardError::Yaml { message } => WyrdError::WorkflowValidation {
                message: format!("WorkflowCard YAML codec failed: {message}"),
                details: json!({ "source": message }),
            },
        }
    }
}

impl From<WorkflowValidationError> for WyrdError {
    fn from(error: WorkflowValidationError) -> Self {
        match error {
            WorkflowValidationError::DuplicateStep => WyrdError::WorkflowDuplicateStepId {
                message: "workflow contains a duplicate step id".to_owned(),
                details: json!({}),
            },
            WorkflowValidationError::MissingDependency => WyrdError::WorkflowMissingDependency {
                message: "workflow references a missing dependency".to_owned(),
                details: json!({}),
            },
            WorkflowValidationError::Cycle => WyrdError::WorkflowCycle {
                message: "workflow contains a dependency cycle".to_owned(),
                details: json!({}),
            },
        }
    }
}

fn derive_cascade_children(spec: &WorkflowSpec) -> Vec<CardRef> {
    let mut out: Vec<CardRef> = Vec::new();
    for step in &spec.steps {
        match &step.action {
            WorkflowAction::Mcp(card_ref) | WorkflowAction::Prompt(card_ref) => {
                out.push(card_ref.clone());
            }
            WorkflowAction::Agent(AgentRef::Card(card_ref)) => {
                out.push(card_ref.clone());
            }
            WorkflowAction::Agent(AgentRef::Inline(agent_spec)) => {
                if let PromptRef::Card(prompt_ref) = &agent_spec.prompt {
                    out.push(prompt_ref.clone());
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let a_key = (
            a.kind.wire_name(),
            a.space.as_ref().map_or("", |s| s.as_str()),
            a.name.as_str(),
            a.version.to_string(),
        );
        let b_key = (
            b.kind.wire_name(),
            b.space.as_ref().map_or("", |s| s.as_str()),
            b.name.as_str(),
            b.version.to_string(),
        );
        a_key.cmp(&b_key)
    });
    out.dedup();
    out
}

fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value).map_err(|error| {
        WorkflowCardError::validation(format!("{field} must be a valid CardName: {error}")).into()
    })
}

fn version_block(field: &str, value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value).map_err(|error| {
        WorkflowCardError::validation(format!("{field} must be a semantic version: {error}")).into()
    })
}

fn optional_space_name(value: &str) -> Result<Option<SpaceName>, WyrdError> {
    if value.is_empty() || value == "default" {
        return Ok(None);
    }
    SpaceName::new(value).map(Some).map_err(|error| {
        WorkflowCardError::validation(format!("metadata.space is invalid: {error}")).into()
    })
}

fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        return Ok(None);
    }
    CardUid::new(value).map(Some).map_err(|error| {
        WorkflowCardError::validation(format!("metadata.uid is invalid: {error}")).into()
    })
}
