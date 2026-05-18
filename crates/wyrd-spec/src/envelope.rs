//! Universal Card envelope.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::card::agent::AgentSpec;
use crate::card::artifact::ArtifactSpec;
use crate::card::audit::AuditSpec;
use crate::card::data::DataSpec;
use crate::card::drift::DriftSpec;
use crate::card::eval::EvalSpec;
use crate::card::experiment::ExperimentSpec;
use crate::card::mcp::McpSpec;
use crate::card::model::ModelSpec;
use crate::card::policy::PolicySpec;
use crate::card::prompt::PromptSpec;
use crate::card::service::ServiceSpec;
use crate::card::skill::SkillSpec;
use crate::card::subagent::SubAgentSpec;
use crate::card::tool::ToolSpec;
use crate::card::workflow::WorkflowSpec;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::version::{ApiVersion, VersionBlock};

/// Universal registered Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Card {
    /// API version. Must be `wyrd/v1` for v1 Cards.
    pub api_version: ApiVersion,
    /// Card kind discriminator.
    pub kind: CardKind,
    /// Card metadata.
    pub metadata: Metadata,
    /// Kind-specific spec payload.
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
    pub labels: BTreeMap<String, String>,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
    /// Spec content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_hash: Option<String>,
    /// Artifact content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
}

/// Kind-specific Card spec payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, schemars::JsonSchema)]
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
    /// Unknown Card kind with schema hash.
    External {
        /// External kind name.
        name: String,
        /// 32-byte schema hash.
        schema_hash: [u8; 32],
    },
}

impl CardKind {
    /// Native v1 Card kind count.
    pub const NATIVE_COUNT: usize = 16;

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
        ]
    }

    /// Native wire name, if this is a native kind.
    #[must_use]
    pub fn native_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::Data => "data",
            Self::Model => "model",
            Self::Experiment => "experiment",
            Self::Prompt => "prompt",
            Self::Tool => "tool",
            Self::Agent => "agent",
            Self::Workflow => "workflow",
            Self::Eval => "eval",
            Self::Drift => "drift",
            Self::Service => "service",
            Self::Policy => "policy",
            Self::Mcp => "mcp",
            Self::Skill => "skill",
            Self::SubAgent => "sub_agent",
            Self::Audit => "audit",
            Self::Artifact => "artifact",
            Self::External { .. } => return None,
        })
    }
}

impl Serialize for CardKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(name) = self.native_name() {
            return serializer.serialize_str(name);
        }
        match self {
            Self::External { name, schema_hash } => {
                #[derive(Serialize)]
                struct External<'a> {
                    kind: &'a str,
                    schema_hash: String,
                }
                External {
                    kind: name,
                    schema_hash: hex::encode(schema_hash),
                }
                .serialize(serializer)
            }
            _ => unreachable!("native handled above"),
        }
    }
}

impl<'de> Deserialize<'de> for CardKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl<'de> Visitor<'de> for KindVisitor {
            type Value = CardKind;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a native kind string or external kind object")
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                native_from_str(value)
                    .ok_or_else(|| E::custom(format!("unknown native card kind: {value}")))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut name = None;
                let mut hash = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => name = Some(map.next_value::<String>()?),
                        "schema_hash" => hash = Some(map.next_value::<String>()?),
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let name = name.ok_or_else(|| A::Error::missing_field("kind"))?;
                let hash = hash.ok_or_else(|| A::Error::missing_field("schema_hash"))?;
                if let Some(native) = native_from_str(&name) {
                    return Ok(native);
                }
                let decoded = hex::decode(hash).map_err(A::Error::custom)?;
                let schema_hash: [u8; 32] = decoded
                    .try_into()
                    .map_err(|_| A::Error::custom("schema_hash must decode to 32 bytes"))?;
                Ok(CardKind::External { name, schema_hash })
            }
        }

        deserializer.deserialize_any(KindVisitor)
    }
}

fn native_from_str(value: &str) -> Option<CardKind> {
    Some(match value {
        "data" => CardKind::Data,
        "model" => CardKind::Model,
        "experiment" => CardKind::Experiment,
        "prompt" => CardKind::Prompt,
        "tool" => CardKind::Tool,
        "agent" => CardKind::Agent,
        "workflow" => CardKind::Workflow,
        "eval" => CardKind::Eval,
        "drift" => CardKind::Drift,
        "service" => CardKind::Service,
        "policy" => CardKind::Policy,
        "mcp" => CardKind::Mcp,
        "skill" => CardKind::Skill,
        "sub_agent" => CardKind::SubAgent,
        "audit" => CardKind::Audit,
        "artifact" => CardKind::Artifact,
        _ => return None,
    })
}
