//! Universal Card envelope.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{
    InstanceType, Metadata as SchemaMetadata, Schema, SchemaObject, SubschemaValidation,
};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;

use crate::api_version::ApiVersion;
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
use crate::card::source::SourceSpec;
use crate::card::trigger::TriggerSpec;
use crate::card::workflow::WorkflowSpec;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};

/// Canonical hash of a [`Spec`] over its RFC 8785 (JCS) canonicalized JSON.
///
/// Two specs that differ only in field ordering, whitespace, or unicode escape
/// representation produce the same hash. Field deletion, addition, or value
/// change produces a different hash. This is the registry's drift-detection
/// primitive.
///
/// Wire form: 64 lowercase hex characters (BLAKE3-256 digest).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct SpecHash(String);

impl SpecHash {
    /// Compute a [`SpecHash`] from canonicalized JSON bytes.
    #[must_use]
    pub fn from_canonical_bytes(bytes: &[u8]) -> Self {
        let digest = blake3::hash(bytes);
        Self(hex::encode(digest.as_bytes()))
    }

    /// Return the underlying hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the hash and return its hex form.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SpecHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for SpecHash {
    type Err = SpecHashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(SpecHashParseError::InvalidLength { len: s.len() });
        }
        if let Some(pos) = s
            .bytes()
            .position(|b| !matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(SpecHashParseError::InvalidChar { pos });
        }
        Ok(Self(s.to_owned()))
    }
}

/// Error returned when parsing a [`SpecHash`] from its hex string form.
#[derive(Debug, thiserror::Error)]
pub enum SpecHashParseError {
    /// The string was not exactly 64 characters.
    #[error("spec_hash must be 64 lowercase hex chars; got {len}")]
    InvalidLength {
        /// Length of the offending string.
        len: usize,
    },
    /// The string contained a character outside `[0-9a-f]`.
    #[error("spec_hash contains non-hex char at position {pos}")]
    InvalidChar {
        /// Zero-based position of the offending byte.
        pos: usize,
    },
}

/// Error produced by [`Spec::from_kind_and_value`].
#[derive(Debug, thiserror::Error)]
#[error("spec decode failed for kind {kind}: {message}")]
pub struct SpecDecodeError {
    /// Card kind that failed to decode.
    pub kind: String,
    /// Decode error detail.
    pub message: String,
}

/// Error produced by [`Spec::canonical_hash`] / [`Spec::canonical_bytes`].
#[derive(Debug, thiserror::Error)]
pub enum SpecCanonicalizationError {
    /// `serde_json` failed to serialize the spec to a JSON value, including
    /// non-finite floats which `serde_json` rejects before JCS runs.
    #[error("spec serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
}

/// Universal registered Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Card {
    /// API version. Must be `wyrd/v1` for v1 Cards.
    #[serde(rename = "apiVersion")]
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
///
/// `version` and `bump` capture the author's declarative intent on the
/// register path. The server resolves both to a concrete pin before the
/// envelope is returned to the caller:
///
/// - Pre-register: `version` may be `None` (server bumps absolute latest or
///   seeds `0.1.0`), `Some(VersionSpec::Scope("1.0"))` (server bumps within
///   the prefix line), or `Some(VersionSpec::Pin("1.4.2"))` (exact pin).
///   `bump` selects the bump level applied on the auto-bump paths (defaults
///   to `Patch` if unset).
/// - Post-register: `version` is always `Some(VersionSpec::Pin(resolved))`;
///   readers may use [`Metadata::resolved_pin`] to extract the
///   [`VersionBlock`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Metadata {
    /// Card name.
    pub name: CardName,
    /// Authored version: `None` (auto-bump from absolute latest), a bare
    /// partial scope (`"1"` / `"1.0"` — bump within the prefix line), or a
    /// full triple pin (`"1.4.2"`). Always populated and always a pin
    /// post-register.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionSpec>,
    /// Bump level applied on the auto-bump paths (`None` or `Scope`
    /// `version`). Defaults to `Patch` when unset. Ignored on the exact-pin
    /// path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<VersionBump>,
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
    pub spec_hash: Option<SpecHash>,
    /// Artifact content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    /// Code-origin provenance: the repository commit this card was declared
    /// from. Populated on the register path for cards authored in a repo;
    /// `None` for cards with no code origin (e.g. UI-authored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<crate::origin::Origin>,
}

impl Metadata {
    /// Extract the resolved version pin from a post-register envelope.
    ///
    /// Returns `Some(&VersionBlock)` when `version` is
    /// `Some(VersionSpec::Pin(_))`. Returns `None` when the envelope is
    /// pre-register (`version` is `None` or `Some(VersionSpec::Scope(_))`).
    #[must_use]
    pub fn resolved_pin(&self) -> Option<&VersionBlock> {
        match &self.version {
            Some(VersionSpec::Pin(block)) => Some(block),
            _ => None,
        }
    }
}

/// Kind-specific Card spec payload.
#[derive(Debug, Clone, PartialEq)]
// justification: public Card contract used by every kind; boxing variants would break call sites throughout the codebase and defeats the flat pattern-match shape that this contract exposes to consumers
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
    /// Source card spec.
    Source(SourceSpec),
}

#[cfg(feature = "server")]
fn eval_spec_openapi_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};

    let opaque_object = Schema::Object(ObjectBuilder::new().schema_type(Type::Object).build());
    utoipa::openapi::RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .description(Some("Evaluation Card spec."))
            .property("dataset", opaque_object.clone())
            .property("tasks", opaque_object.clone())
            .property("workflow", opaque_object.clone())
            .property("sampling", opaque_object.clone())
            .property("pass_gate", opaque_object.clone())
            .property("context_capture", opaque_object)
            .required("tasks")
            .schema_type(Type::Object)
            .build(),
    ))
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for Spec {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::OneOfBuilder::new()
            .item(<DataSpec as utoipa::PartialSchema>::schema())
            .item(<ModelSpec as utoipa::PartialSchema>::schema())
            .item(<ExperimentSpec as utoipa::PartialSchema>::schema())
            .item(<PromptSpec as utoipa::PartialSchema>::schema())
            .item(<AgentSpec as utoipa::PartialSchema>::schema())
            .item(<WorkflowSpec as utoipa::PartialSchema>::schema())
            .item(eval_spec_openapi_schema())
            .item(<DriftSpec as utoipa::PartialSchema>::schema())
            .item(<ServiceSpec as utoipa::PartialSchema>::schema())
            .item(<PolicySpec as utoipa::PartialSchema>::schema())
            .item(<McpSpec as utoipa::PartialSchema>::schema())
            .item(<AuditSpec as utoipa::PartialSchema>::schema())
            .item(<ArtifactSpec as utoipa::PartialSchema>::schema())
            .item(<TriggerSpec as utoipa::PartialSchema>::schema())
            .item(<OperatorSpec as utoipa::PartialSchema>::schema())
            .item(<SourceSpec as utoipa::PartialSchema>::schema())
            .title(Some("Spec"))
            .description(Some("One typed Wyrd Card spec variant."))
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for Spec {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Spec")
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        macro_rules! add_schema {
            ($schema:ty) => {
                schemas.push((
                    <$schema as utoipa::ToSchema>::name().into(),
                    <$schema as utoipa::PartialSchema>::schema(),
                ));
            };
        }

        add_schema!(DataSpec);
        add_schema!(ModelSpec);
        add_schema!(ExperimentSpec);
        add_schema!(PromptSpec);
        add_schema!(AgentSpec);
        add_schema!(WorkflowSpec);
        add_schema!(DriftSpec);
        add_schema!(ServiceSpec);
        add_schema!(PolicySpec);
        add_schema!(McpSpec);
        add_schema!(AuditSpec);
        add_schema!(ArtifactSpec);
        add_schema!(TriggerSpec);
        add_schema!(OperatorSpec);
        add_schema!(SourceSpec);
    }
}

impl schemars::JsonSchema for Spec {
    fn schema_name() -> String {
        "Spec".to_owned()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let variants = vec![
            generator.subschema_for::<DataSpec>(),
            generator.subschema_for::<ModelSpec>(),
            generator.subschema_for::<ExperimentSpec>(),
            generator.subschema_for::<PromptSpec>(),
            generator.subschema_for::<AgentSpec>(),
            generator.subschema_for::<WorkflowSpec>(),
            generator.subschema_for::<EvalSpec>(),
            generator.subschema_for::<DriftSpec>(),
            generator.subschema_for::<ServiceSpec>(),
            generator.subschema_for::<PolicySpec>(),
            generator.subschema_for::<McpSpec>(),
            generator.subschema_for::<AuditSpec>(),
            generator.subschema_for::<ArtifactSpec>(),
            generator.subschema_for::<TriggerSpec>(),
            generator.subschema_for::<OperatorSpec>(),
            generator.subschema_for::<SourceSpec>(),
        ];
        SchemaObject {
            metadata: Some(Box::new(SchemaMetadata {
                title: Some(Self::schema_name()),
                description: Some("One typed Wyrd Card spec variant.".to_owned()),
                ..SchemaMetadata::default()
            })),
            subschemas: Some(Box::new(SubschemaValidation {
                one_of: Some(variants),
                ..SubschemaValidation::default()
            })),
            ..SchemaObject::default()
        }
        .into()
    }
}

impl Spec {
    /// The [`CardKind`] this spec variant corresponds to.
    #[must_use]
    pub const fn kind(&self) -> CardKind {
        match self {
            Self::Data(_) => CardKind::Data,
            Self::Model(_) => CardKind::Model,
            Self::Experiment(_) => CardKind::Experiment,
            Self::Prompt(_) => CardKind::Prompt,
            Self::Agent(_) => CardKind::Agent,
            Self::Workflow(_) => CardKind::Workflow,
            Self::Eval(_) => CardKind::Eval,
            Self::Drift(_) => CardKind::Drift,
            Self::Service(_) => CardKind::Service,
            Self::Policy(_) => CardKind::Policy,
            Self::Mcp(_) => CardKind::Mcp,
            Self::Audit(_) => CardKind::Audit,
            Self::Artifact(_) => CardKind::Artifact,
            Self::Trigger(_) => CardKind::Trigger,
            Self::Operator(_) => CardKind::Operator,
            Self::Source(_) => CardKind::Source,
        }
    }

    /// Compute the canonical hash of this spec.
    ///
    /// Pipeline: serialize to JSON → RFC 8785 (JCS) canonicalize → BLAKE3-256 → lowercase hex.
    /// Returns [`SpecCanonicalizationError::Serialize`] when the spec fails JSON encoding,
    /// including non-finite floats.
    pub fn canonical_hash(&self) -> Result<SpecHash, SpecCanonicalizationError> {
        let canon = self.canonical_bytes()?;
        Ok(SpecHash::from_canonical_bytes(&canon))
    }

    /// Compute both the canonical hash and the underlying canonical bytes in one pass.
    pub fn canonical_hash_with_bytes(
        &self,
    ) -> Result<(SpecHash, Vec<u8>), SpecCanonicalizationError> {
        let canon = self.canonical_bytes()?;
        let hash = SpecHash::from_canonical_bytes(&canon);
        Ok((hash, canon))
    }

    /// Decode a `serde_json::Value` into a kind-specific [`Spec`] variant.
    ///
    /// Calls the private `spec_from_kind_value` and wraps any error in
    /// [`SpecDecodeError`] with the kind name attached.
    pub fn from_kind_and_value(
        kind: &CardKind,
        value: serde_json::Value,
    ) -> Result<Self, SpecDecodeError> {
        spec_from_kind_value(kind, value).map_err(|message| SpecDecodeError {
            kind: kind.wire_name().to_owned(),
            message,
        })
    }

    /// Return the JCS-canonicalized JSON bytes for this spec.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SpecCanonicalizationError> {
        let value = serde_json::to_value(self).map_err(SpecCanonicalizationError::Serialize)?;
        serde_jcs::to_vec(&value).map_err(SpecCanonicalizationError::Serialize)
    }
}

/// Server-derived relationship summary for graph, UI, policy, imports, and diff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Relationships {
    /// Outbound references as normalized display strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound: Vec<String>,
    /// Exact outbound references with optional user-facing aliases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound_refs: Vec<CardRelationship>,
    /// Inbound references as normalized display strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbound: Vec<String>,
    /// Exact inbound references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbound_refs: Vec<CardRelationship>,
}

/// One server-derived relationship edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CardRelationship {
    /// Exact related Card reference.
    #[serde(rename = "ref")]
    pub card_ref: crate::reference::CardRef,
    /// Optional user-facing alias assigned by a parent composition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// Source Card.
    Source,
    /// Unknown/external card kind.
    External,
}

impl CardKind {
    /// Total v1 Card kind count including `External`.
    pub const NATIVE_COUNT: usize = 17;
    /// Registrable v1 Card kind count (excludes `External`).
    pub const REGISTRABLE_COUNT: usize = 16;

    /// Return every native kind including [`CardKind::External`].
    #[must_use]
    pub fn native() -> [Self; Self::NATIVE_COUNT] {
        [
            Self::Data,
            Self::Model,
            Self::Experiment,
            Self::Prompt,
            Self::Agent,
            Self::Workflow,
            Self::Eval,
            Self::Drift,
            Self::Service,
            Self::Policy,
            Self::Mcp,
            Self::Audit,
            Self::Artifact,
            Self::Trigger,
            Self::Operator,
            Self::Source,
            Self::External,
        ]
    }

    /// Return every registrable kind (excludes [`CardKind::External`]).
    #[must_use]
    pub fn registrable() -> [Self; Self::REGISTRABLE_COUNT] {
        [
            Self::Data,
            Self::Model,
            Self::Experiment,
            Self::Prompt,
            Self::Agent,
            Self::Workflow,
            Self::Eval,
            Self::Drift,
            Self::Service,
            Self::Policy,
            Self::Mcp,
            Self::Audit,
            Self::Artifact,
            Self::Trigger,
            Self::Operator,
            Self::Source,
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
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
            Self::Source => "Source",
            Self::External => "External",
        })
    }

    /// Parse a wire-name string (`"Data"`, `"Model"`, …) into a [`CardKind`].
    ///
    /// Returns `None` when the string does not match a known kind.
    pub fn from_wire_name(value: &str) -> Option<Self> {
        native_from_str(value)
    }

    /// Public wire name for this kind.
    #[must_use]
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Model => "Model",
            Self::Experiment => "Experiment",
            Self::Prompt => "Prompt",
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
            Self::Source => "Source",
            Self::External => "External",
        }
    }

    /// True when observations may be attributed to this card kind.
    #[must_use]
    pub const fn is_observation_target(&self) -> bool {
        match self {
            Self::Data
            | Self::Model
            | Self::Experiment
            | Self::Prompt
            | Self::Agent
            | Self::Workflow
            | Self::Eval
            | Self::Drift
            | Self::Service
            | Self::Mcp
            | Self::Artifact
            | Self::Source => true,
            Self::Policy | Self::Audit | Self::Operator | Self::Trigger | Self::External => false,
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
        "Agent" => CardKind::Agent,
        "Workflow" => CardKind::Workflow,
        "Eval" => CardKind::Eval,
        "Drift" => CardKind::Drift,
        "Service" => CardKind::Service,
        "Policy" => CardKind::Policy,
        "Mcp" => CardKind::Mcp,
        "Audit" => CardKind::Audit,
        "Artifact" => CardKind::Artifact,
        "Trigger" => CardKind::Trigger,
        "Operator" => CardKind::Operator,
        "Source" => CardKind::Source,
        "External" => CardKind::External,
        _ => return None,
    })
}

impl JsonSchema for CardKind {
    fn schema_name() -> String {
        "CardKind".to_string()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        let enum_values = Self::registrable()
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
            Self::Agent(s) => s.serialize(serializer),
            Self::Workflow(s) => s.serialize(serializer),
            Self::Eval(s) => s.serialize(serializer),
            Self::Drift(s) => s.serialize(serializer),
            Self::Service(s) => s.serialize(serializer),
            Self::Policy(s) => s.serialize(serializer),
            Self::Mcp(s) => s.serialize(serializer),
            Self::Audit(s) => s.serialize(serializer),
            Self::Artifact(s) => s.serialize(serializer),
            Self::Trigger(s) => s.serialize(serializer),
            Self::Operator(s) => s.serialize(serializer),
            Self::Source(s) => s.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Card {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct CardHelper {
            #[serde(rename = "apiVersion")]
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
        if helper.api_version.as_str() != ApiVersion::V1 {
            return Err(D::Error::custom(format!(
                "expected apiVersion {}, got {}",
                ApiVersion::V1,
                helper.api_version
            )));
        }
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
        CardKind::Source => serde_json::from_value(value)
            .map(Spec::Source)
            .map_err(|e| e.to_string()),
        CardKind::External => {
            Err("External is a forward-compatibility variant; use a named card kind".to_string())
        }
    }
}
