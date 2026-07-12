//! Workflow Card spec.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use thiserror::Error;

use crate::api_version::ApiVersion;
use crate::card::common::{Governance, NonSecretValue, ObservationHooks, ParameterValue};
use crate::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use crate::error::WyrdError;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::reference::{AgentRef, CardRef, PromptRef};
use wyrd_semver::VersionBlock;

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
            version: card
                .metadata
                .resolved_pin()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    WorkflowCardError::validation(
                        "Workflow Card envelope missing resolved version pin",
                    )
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

    /// Convert this Workflow Card identity into a `CardRef`.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Workflow,
            name: card_name("metadata.name", &self.name)?,
            version: version_block("metadata.version", &self.version)?,
            space: space_name(&self.space)?,
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
            a.space.as_str(),
            a.name.as_str(),
            a.version.to_string(),
        );
        let b_key = (
            b.kind.wire_name(),
            b.space.as_str(),
            b.name.as_str(),
            b.version.to_string(),
        );
        a_key.cmp(&b_key)
    });
    out.dedup();
    out
}

/// Return card refs declared by a workflow for card-ref scope traversal.
pub(crate) fn scope_child_card_refs(spec: &WorkflowSpec) -> Vec<CardRef> {
    derive_cascade_children(spec)
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

fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(WorkflowCardError::validation(
            "metadata.space is required and cannot be empty",
        )
        .into());
    }
    SpaceName::new(value).map_err(|error| {
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

#[cfg(test)]
mod workflow_spec_tests {
    use std::collections::BTreeMap;

    use crate::card::agent::{AgentRunConfigSpec, AgentSpec};
    use crate::card::workflow::{
        WorkflowAction, WorkflowCard, WorkflowSpec, WorkflowStep, WorkflowValidationError,
    };
    use crate::envelope::CardKind;
    use crate::error::WyrdError;
    use crate::ids::SpaceName;
    use crate::metadata::{Annotations, Labels};
    use crate::reference::{AgentRef, CardRef, PromptRef};

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

    fn inline_agent_step(id: &str) -> WorkflowStep {
        WorkflowStep {
            id: id.to_owned(),
            action: WorkflowAction::Agent(AgentRef::from(AgentSpec {
                prompt: PromptRef::from(prompt()),
                tool_names: vec![],
                run_config: AgentRunConfigSpec::default(),
            })),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            condition: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        }
    }

    fn card_ref_agent_step(id: &str, agent_name: &str) -> WorkflowStep {
        WorkflowStep {
            id: id.to_owned(),
            action: WorkflowAction::Agent(AgentRef::from(CardRef {
                kind: CardKind::Agent,
                name: agent_name.parse().expect("valid card name"),
                version: "0.1.0".parse().expect("valid version"),
                space: SpaceName::new("default").expect("static space is valid"),
                uid: None,
            })),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            condition: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        }
    }

    #[test]
    fn workflow_spec_inline_agent_roundtrips() {
        let spec = WorkflowSpec {
            steps: vec![inline_agent_step("planner")],
            ..WorkflowSpec::default()
        };

        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn workflow_spec_card_ref_agent_roundtrips() {
        let spec = WorkflowSpec {
            steps: vec![card_ref_agent_step("planner", "research-planner")],
            ..WorkflowSpec::default()
        };

        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(decoded, spec);
        assert!(
            yaml.contains("research-planner"),
            "expected card ref name in YAML, got: {yaml}"
        );
    }

    #[test]
    fn workflow_card_envelope_roundtrips() {
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![card_ref_agent_step("planner", "research-planner")],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.spec, card.spec);
        assert_eq!(reloaded.name, card.name);
        assert_eq!(reloaded.version, card.version);
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
    }

    #[test]
    fn workflow_card_cascade_inline_agent_with_card_prompt() {
        let inline_with_prompt_ref = AgentSpec {
            prompt: PromptRef::from(CardRef {
                kind: CardKind::Prompt,
                name: "planner-prompt".parse().expect("valid card name"),
                version: "0.3.0".parse().expect("valid version"),
                space: SpaceName::new("default").expect("static space is valid"),
                uid: None,
            }),
            tool_names: vec![],
            run_config: AgentRunConfigSpec::default(),
        };
        let step = WorkflowStep {
            id: "planner".to_owned(),
            action: WorkflowAction::Agent(AgentRef::from(inline_with_prompt_ref)),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            condition: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        };
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![step],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Prompt);
        assert_eq!(reloaded.cascade_children[0].name.as_str(), "planner-prompt");
    }

    #[test]
    fn workflow_card_cascade_card_ref_agent() {
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![card_ref_agent_step("planner", "research-planner")],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
        assert_eq!(
            reloaded.cascade_children[0].name.as_str(),
            "research-planner"
        );
    }

    #[test]
    fn workflow_card_cascade_dedups_repeated_refs() {
        let spec = WorkflowSpec {
            steps: vec![
                card_ref_agent_step("planner-a", "shared-planner"),
                card_ref_agent_step("planner-b", "shared-planner"),
            ],
            ..WorkflowSpec::default()
        };
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec,
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(
            reloaded.cascade_children.len(),
            1,
            "expected dedup, got: {:?}",
            reloaded.cascade_children
        );
    }

    #[test]
    fn workflow_validate_dag_duplicate_step() {
        let spec = WorkflowSpec {
            steps: vec![inline_agent_step("planner"), inline_agent_step("planner")],
            ..WorkflowSpec::default()
        };
        assert_eq!(
            spec.validate_dag().unwrap_err(),
            WorkflowValidationError::DuplicateStep
        );

        let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
        assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID");
    }

    #[test]
    fn workflow_validate_dag_missing_dependency() {
        let mut planner = inline_agent_step("planner");
        planner.depends_on = vec!["never-defined".to_owned()];
        let spec = WorkflowSpec {
            steps: vec![planner],
            ..WorkflowSpec::default()
        };
        assert_eq!(
            spec.validate_dag().unwrap_err(),
            WorkflowValidationError::MissingDependency
        );

        let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
        assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
    }

    #[test]
    fn workflow_validate_dag_cycle() {
        let mut a = inline_agent_step("a");
        let mut b = inline_agent_step("b");
        a.depends_on = vec!["b".to_owned()];
        b.depends_on = vec!["a".to_owned()];
        let spec = WorkflowSpec {
            steps: vec![a, b],
            ..WorkflowSpec::default()
        };
        assert_eq!(
            spec.validate_dag().unwrap_err(),
            WorkflowValidationError::Cycle
        );

        let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
        assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_CYCLE");
    }
}
