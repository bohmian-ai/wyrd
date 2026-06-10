//! Universal Card envelope.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Metadata as SchemaMetadata, Schema, SchemaObject};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;

use crate::card::agent::AgentSpec;
use crate::card::artifact::ArtifactSpec;
use crate::card::audit::AuditSpec;
use crate::card::data::DataSpec;
use crate::card::drift::DriftSpec;
use crate::card::eval::EvalSpec;
use crate::card::experiment::ExperimentSpec;
use crate::card::mcp::McpSpec;
use crate::card::model::ModelSpec;
use crate::card::operator::OperatorSpec;
use crate::card::policy::PolicySpec;
use crate::card::prompt::PromptSpec;
use crate::card::service::ServiceSpec;
use crate::card::skill::SkillSpec;
use crate::card::subagent::SubAgentSpec;
use crate::card::tool::ToolSpec;
use crate::card::trigger::TriggerSpec;
use crate::card::workflow::WorkflowSpec;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::version::{ApiVersion, VersionBlock};

/// Universal registered Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Card {
    /// API version. Must be `wyrd/v1` for v1 Cards.
    #[serde(rename = "apiVersion", default)]
    pub api_version: ApiVersion,
    /// Card kind discriminator.
    pub kind: CardKind,
    /// Card metadata.
    pub metadata: Metadata,
    /// Kind-specific spec payload.
    #[schemars(with = "serde_json::Value")]
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub spec: Spec,
    /// Server-derived relationship summary.
    #[serde(default)]
    pub relationships: Relationships,
    /// Optional server-derived status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

/// Card metadata common to every kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Metadata {
    /// Card name.
    pub name: CardName,
    /// Exact Card version.
    pub version: VersionBlock,
    /// Optional space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceName>,
    /// Optional resolved UID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<CardUid>,
    /// Display labels.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: Labels,
    /// Free-form annotations for automation, UI, and integration metadata.
    ///
    /// As of 2026-05-18, keys under `wyrd.io/*` MUST remain reserved for
    /// Wyrd-owned runtime and registry metadata. User and vendor annotations
    /// MUST use another DNS-style prefix, such as `acme.com/cost-center`, to
    /// avoid collisions.
    // source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-6
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: Annotations,
    /// Spec content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_hash: Option<String>,
    /// Artifact content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
}

/// Kind-specific Card spec payload.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[allow(clippy::large_enum_variant)]
pub enum Spec {
    /// Data card spec.
    Data(DataSpec),
    /// Model card spec.
    Model(ModelSpec),
    /// Experiment card spec.
    Experiment(ExperimentSpec),
    /// Prompt card spec.
    Prompt(PromptSpec),
    /// Tool card spec.
    Tool(ToolSpec),
    /// Agent card spec.
    Agent(AgentSpec),
    /// Workflow card spec.
    Workflow(WorkflowSpec),
    /// Eval card spec.
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    Eval(EvalSpec),
    /// Drift card spec.
    Drift(DriftSpec),
    /// Service card spec.
    Service(ServiceSpec),
    /// Policy card spec.
    Policy(PolicySpec),
    /// MCP card spec.
    Mcp(McpSpec),
    /// Skill card spec.
    Skill(SkillSpec),
    /// Sub-agent card spec.
    SubAgent(SubAgentSpec),
    /// Audit card spec.
    Audit(AuditSpec),
    /// Artifact card spec.
    Artifact(ArtifactSpec),
    /// Trigger card spec.
    // source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-1
    Trigger(TriggerSpec),
    /// Operator card spec.
    // source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-2
    Operator(OperatorSpec),
}

/// Server-derived relationship summary for graph, UI, policy, imports, and diff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Relationships {
    /// Outbound references as normalized display strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound: Vec<String>,
    /// Inbound references as normalized display strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbound: Vec<String>,
}

/// Server-derived Card status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Status {
    /// Lifecycle phase such as `draft`, `active`, or `deprecated`.
    pub phase: String,
    /// Human-readable status message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Last status update timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Native Wyrd Card kind plus forward-compatible external catch-all.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum CardKind {
    /// Data Card.
    Data,
    /// Model Card.
    Model,
    /// Experiment Card.
    Experiment,
    /// Prompt Card.
    Prompt,
    /// Tool Card.
    Tool,
    /// Agent Card.
    Agent,
    /// Workflow Card.
    Workflow,
    /// Evaluation Card.
    Eval,
    /// Drift Card.
    Drift,
    /// Service Card.
    Service,
    /// Policy Card.
    Policy,
    /// MCP Card.
    Mcp,
    /// Skill Card.
    Skill,
    /// Sub-agent Card.
    SubAgent,
    /// Audit Card.
    Audit,
    /// Artifact Card.
    Artifact,
    /// Trigger Card.
    // source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-1
    Trigger,
    /// Operator Card.
    // source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-2
    Operator,
    /// Unknown/external card kind.
    External,
}

impl CardKind {
    /// Native v1 Card kind count.
    pub const NATIVE_COUNT: usize = 19;

    /// Return every native kind.
    #[must_use]
    pub fn native() -> [Self; Self::NATIVE_COUNT] {
        [
            Self::Data,
            Self::Model,
            Self::Experiment,
            Self::Prompt,
            Self::Tool,
            Self::Agent,
            Self::Workflow,
            Self::Eval,
            Self::Drift,
            Self::Service,
            Self::Policy,
            Self::Mcp,
            Self::Skill,
            Self::SubAgent,
            Self::Audit,
            Self::Artifact,
            Self::Trigger,
            Self::Operator,
            Self::External,
        ]
    }

    /// Native wire name, if this is a native kind.
    #[must_use]
    pub fn native_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::Data => "Data",
            Self::Model => "Model",
            Self::Experiment => "Experiment",
            Self::Prompt => "Prompt",
            Self::Tool => "Tool",
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Skill => "Skill",
            Self::SubAgent => "SubAgent",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
            Self::External => "External",
        })
    }

    /// Public wire name for this kind.
    #[must_use]
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Model => "Model",
            Self::Experiment => "Experiment",
            Self::Prompt => "Prompt",
            Self::Tool => "Tool",
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Skill => "Skill",
            Self::SubAgent => "SubAgent",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
            Self::External => "External",
        }
    }
}

impl Serialize for CardKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire_name())
    }
}

impl<'de> Deserialize<'de> for CardKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        native_from_str(&s).ok_or_else(|| D::Error::custom(format!("unknown card kind: {s}")))
    }
}

fn native_from_str(value: &str) -> Option<CardKind> {
    Some(match value {
        "Data" => CardKind::Data,
        "Model" => CardKind::Model,
        "Experiment" => CardKind::Experiment,
        "Prompt" => CardKind::Prompt,
        "Tool" => CardKind::Tool,
        "Agent" => CardKind::Agent,
        "Workflow" => CardKind::Workflow,
        "Eval" => CardKind::Eval,
        "Drift" => CardKind::Drift,
        "Service" => CardKind::Service,
        "Policy" => CardKind::Policy,
        "Mcp" => CardKind::Mcp,
        "Skill" => CardKind::Skill,
        "SubAgent" => CardKind::SubAgent,
        "Audit" => CardKind::Audit,
        "Artifact" => CardKind::Artifact,
        "Trigger" => CardKind::Trigger,
        "Operator" => CardKind::Operator,
        "External" => CardKind::External,
        _ => return None,
    })
}

impl JsonSchema for CardKind {
    fn schema_name() -> String {
        "CardKind".to_string()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        let enum_values = Self::native()
            .iter()
            .map(|kind| json!(kind.wire_name()))
            .collect();

        SchemaObject {
            metadata: Some(Box::new(SchemaMetadata {
                title: Some(Self::schema_name()),
                description: Some("Wyrd Card kind.".to_string()),
                ..SchemaMetadata::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(enum_values),
            ..SchemaObject::default()
        }
        .into()
    }
}

impl Serialize for Spec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Data(s) => s.serialize(serializer),
            Self::Model(s) => s.serialize(serializer),
            Self::Experiment(s) => s.serialize(serializer),
            Self::Prompt(s) => s.serialize(serializer),
            Self::Tool(s) => s.serialize(serializer),
            Self::Agent(s) => s.serialize(serializer),
            Self::Workflow(s) => s.serialize(serializer),
            Self::Eval(s) => s.serialize(serializer),
            Self::Drift(s) => s.serialize(serializer),
            Self::Service(s) => s.serialize(serializer),
            Self::Policy(s) => s.serialize(serializer),
            Self::Mcp(s) => s.serialize(serializer),
            Self::Skill(s) => s.serialize(serializer),
            Self::SubAgent(s) => s.serialize(serializer),
            Self::Audit(s) => s.serialize(serializer),
            Self::Artifact(s) => s.serialize(serializer),
            Self::Trigger(s) => s.serialize(serializer),
            Self::Operator(s) => s.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Card {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct CardHelper {
            #[serde(rename = "apiVersion", default)]
            api_version: ApiVersion,
            kind: CardKind,
            metadata: Metadata,
            spec: serde_json::Value,
            #[serde(default)]
            relationships: Relationships,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            status: Option<Status>,
        }

        let helper = CardHelper::deserialize(deserializer)?;
        let spec =
            spec_from_kind_value(&helper.kind, helper.spec).map_err(serde::de::Error::custom)?;

        Ok(Card {
            api_version: helper.api_version,
            kind: helper.kind,
            metadata: helper.metadata,
            spec,
            relationships: helper.relationships,
            status: helper.status,
        })
    }
}

fn spec_from_kind_value(kind: &CardKind, mut value: serde_json::Value) -> Result<Spec, String> {
    // Strip legacy `type` field — present in older serialized cards.
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("type");
    }
    match kind {
        CardKind::Data => serde_json::from_value(value)
            .map(Spec::Data)
            .map_err(|e| e.to_string()),
        CardKind::Model => serde_json::from_value(value)
            .map(Spec::Model)
            .map_err(|e| e.to_string()),
        CardKind::Experiment => serde_json::from_value(value)
            .map(Spec::Experiment)
            .map_err(|e| e.to_string()),
        CardKind::Prompt => serde_json::from_value(value)
            .map(Spec::Prompt)
            .map_err(|e| e.to_string()),
        CardKind::Tool => serde_json::from_value(value)
            .map(Spec::Tool)
            .map_err(|e| e.to_string()),
        CardKind::Agent => serde_json::from_value(value)
            .map(Spec::Agent)
            .map_err(|e| e.to_string()),
        CardKind::Workflow => serde_json::from_value(value)
            .map(Spec::Workflow)
            .map_err(|e| e.to_string()),
        CardKind::Eval => serde_json::from_value(value)
            .map(Spec::Eval)
            .map_err(|e| e.to_string()),
        CardKind::Drift => serde_json::from_value(value)
            .map(Spec::Drift)
            .map_err(|e| e.to_string()),
        CardKind::Service => serde_json::from_value(value)
            .map(Spec::Service)
            .map_err(|e| e.to_string()),
        CardKind::Policy => serde_json::from_value(value)
            .map(Spec::Policy)
            .map_err(|e| e.to_string()),
        CardKind::Mcp => serde_json::from_value(value)
            .map(Spec::Mcp)
            .map_err(|e| e.to_string()),
        CardKind::Skill => serde_json::from_value(value)
            .map(Spec::Skill)
            .map_err(|e| e.to_string()),
        CardKind::SubAgent => serde_json::from_value(value)
            .map(Spec::SubAgent)
            .map_err(|e| e.to_string()),
        CardKind::Audit => serde_json::from_value(value)
            .map(Spec::Audit)
            .map_err(|e| e.to_string()),
        CardKind::Artifact => serde_json::from_value(value)
            .map(Spec::Artifact)
            .map_err(|e| e.to_string()),
        CardKind::Trigger => serde_json::from_value(value)
            .map(Spec::Trigger)
            .map_err(|e| e.to_string()),
        CardKind::Operator => serde_json::from_value(value)
            .map(Spec::Operator)
            .map_err(|e| e.to_string()),
        CardKind::External => Err("unsupported external card kind".to_string()),
    }
}
