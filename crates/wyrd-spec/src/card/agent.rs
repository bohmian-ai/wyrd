//! Agent Card durable contract.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::api_version::ApiVersion;
use crate::card::verifier::VerificationBinding;
use crate::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use crate::error::WyrdError;
use crate::ids::{CardName, CardUid, ProviderDeploymentName, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::reference::{CardRef, InlineableRef};
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

/// Where an Agent's model calls go.
///
/// Required on every Agent and replaceable whole for one invocation; it is
/// never inferred from the Prompt provider or model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionRoute {
    /// The executing host's provider configuration named by the Prompt
    /// provider; refused in server-controlled execution. An empty struct
    /// variant so `deny_unknown_fields` refuses a stray `deployment`.
    Native {},
    /// The tenant's Wyrd gateway, addressing `<deployment>/<prompt model>`.
    WyrdGateway {
        /// Named gateway deployment, not an adapter or model address.
        #[cfg_attr(feature = "server", schema(value_type = String))]
        deployment: ProviderDeploymentName,
    },
}

/// Agent authoring body inside a Wyrd Agent Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    /// Prompt reference preserved on disk and resolved by `skald-agent`.
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub prompt: InlineableRef<skald_spec::Prompt>,
    /// Declared execution route; invocations may replace it whole.
    pub execution_route: ExecutionRoute,
    /// Runtime-local tool names resolved from a local Skald tool registry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    /// Agent run configuration.
    #[serde(default, skip_serializing_if = "is_default_run_config")]
    pub run_config: AgentRunConfigSpec,
    /// Continuous verification bindings owned by this standalone Agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_by: Vec<VerificationBinding>,
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
            return Err(validation_error(format!(
                "expected apiVersion wyrd/v1, got {}",
                card.api_version
            )));
        }
        if card.kind != CardKind::Agent {
            return Err(validation_error(format!(
                "expected kind Agent, got {}",
                card.kind.wire_name()
            )));
        }

        let Spec::Agent(spec) = card.spec else {
            return Err(validation_error("Agent Card spec must be an Agent spec"));
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
                    validation_error("Agent Card envelope missing resolved version pin")
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
            space: Some(space_name(&self.space)?),
            uid: optional_card_uid(&self.uid)?,
        })
    }
}

impl Serialize for AgentCard {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_envelope()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentCard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let card = Card::deserialize(deserializer)?;
        Self::from_envelope(card).map_err(serde::de::Error::custom)
    }
}

/// Return card refs declared by an agent for cascade and scope traversal.
fn derive_cascade_children(spec: &AgentSpec) -> Vec<CardRef> {
    spec.prompt.as_card_ref().cloned().into_iter().collect()
}

fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value)
        .map_err(|error| validation_error(format!("{field} must be a valid CardName: {error}")))
}

fn version_block(field: &str, value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value)
        .map_err(|error| validation_error(format!("{field} must be a semantic version: {error}")))
}

fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(validation_error(
            "metadata.space is required and cannot be empty",
        ));
    }
    SpaceName::new(value)
        .map_err(|error| validation_error(format!("metadata.space is invalid: {error}")))
}

fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        return Ok(None);
    }
    CardUid::new(value)
        .map(Some)
        .map_err(|error| validation_error(format!("metadata.uid is invalid: {error}")))
}

fn validation_error(detail: impl Into<String>) -> WyrdError {
    let detail = detail.into();
    WyrdError::AgentValidation {
        message: detail.clone(),
        details: serde_json::json!({ "detail": detail }),
    }
}

#[cfg(test)]
mod agent_spec_tests {
    use std::collections::BTreeMap;

    use chrono::Utc;

    use crate::card::agent::{AgentCard, AgentRunConfigSpec, AgentSpec};
    use crate::envelope::CardKind;
    use crate::reference::{CardRef, InlineableRef};

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
        )
        .expect("static prompt is valid")
    }

    /// An inline Prompt and the run config round-trip through Agent spec JSON
    /// as the bare authored Prompt, with no `inline` tag or `value` wrapper.
    ///
    /// # Panics
    /// Panics when the spec does not round-trip or serializes a wrapper.
    #[test]
    fn agent_spec_inline_prompt_roundtrips() {
        let spec = AgentSpec {
            prompt: InlineableRef::from(prompt()),
            execution_route: crate::card::agent::ExecutionRoute::Native {},
            tool_names: vec!["search_docs".to_owned()],
            run_config: AgentRunConfigSpec {
                max_iterations: Some(7),
                tool_concurrency_cap: Some(2),
                session_recent_limit: Some(5),
                timeout_ms: Some(1_500),
            },
            verified_by: Vec::new(),
        };

        let json = serde_json::to_string(&spec).expect("serialize");
        let decoded: AgentSpec = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, spec);
        assert!(json.contains("session_recent_limit"));
        assert!(!json.contains(r#""kind":"inline""#));
        assert!(!json.contains(r#""value":"#));
    }

    /// A Prompt CardRef in an Agent spec serializes with one `version` field
    /// and its real `kind`, never a `card` tag, `value` wrapper, or version
    /// requirement.
    ///
    /// # Panics
    /// Panics when an identity field is invalid, the spec does not round-trip,
    /// or the YAML carries a removed spelling.
    #[test]
    fn agent_spec_card_prompt_uses_single_version_field() {
        let spec = AgentSpec {
            prompt: InlineableRef::from(CardRef {
                kind: CardKind::Prompt,
                name: "planner-prompt".parse().expect("valid card name"),
                version: "0.3.0".parse().expect("valid version"),
                space: Some("research".parse().expect("valid space")),
                uid: None,
            }),
            execution_route: crate::card::agent::ExecutionRoute::Native {},
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec::default(),
            verified_by: Vec::new(),
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

    /// A local Agent Card projects to the shared envelope with no spec hash,
    /// relationships, or status, which only the server derives, and its JSON
    /// round-trips the spec.
    ///
    /// # Panics
    /// Panics when the envelope or JSON carries server-derived state or the
    /// Card does not round-trip.
    #[test]
    fn agent_card_uses_the_shared_envelope_without_client_derived_state() {
        let card = AgentCard {
            space: "research".to_owned(),
            name: "planner".to_owned(),
            version: "0.3.0".to_owned(),
            uid: "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2".to_owned(),
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec: AgentSpec {
                prompt: InlineableRef::from(prompt()),
                execution_route: crate::card::agent::ExecutionRoute::Native {},
                tool_names: Vec::new(),
                run_config: AgentRunConfigSpec::default(),
                verified_by: Vec::new(),
            },
            cascade_children: Vec::new(),
            created_at: Utc::now(),
        };

        let envelope = card.to_envelope().expect("agent identity is valid");
        assert!(envelope.metadata.spec_hash.is_none());
        assert!(envelope.relationships.outbound.is_empty());
        assert!(envelope.status.is_none());

        let serialized = serde_json::to_value(&card).expect("agent card serializes");
        assert_eq!(serialized["relationships"], serde_json::json!({}));
        assert!(serialized.get("status").is_none());
        assert!(serialized["metadata"].get("specHash").is_none());

        let restored: AgentCard =
            serde_json::from_value(serialized).expect("agent card round trips");
        assert_eq!(restored.name, card.name);
        assert_eq!(restored.spec, card.spec);
    }

    /// T01: an Agent carries a Prompt in either authored form, inline or by
    /// reference, plus one required execution route of exactly two shapes;
    /// removed route spellings, route fields on a Prompt, and partial routes
    /// are refused.
    ///
    /// # Panics
    /// Panics when a valid Agent shape is refused or round-trips differently,
    /// or a refused shape decodes.
    #[test]
    fn agent_prompt_execution_contract() {
        use serde_json::{Value, json};

        use crate::card::agent::ExecutionRoute;

        let portable = json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "{{question}}"}],
            "response_format": {"type": "object", "properties": {"answer": {"type": "string"}}},
        });
        let native = json!({
            "provider": "anthropic",
            "model": "claude-sonnet-5",
            "format": "messages",
            "body": {"max_tokens": 64, "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]},
        });
        let reference = json!({"kind": "Prompt", "name": "support-answer", "version": "1.0.0", "space": "default"});
        let gateway = json!({"kind": "wyrd_gateway", "deployment": "openai-primary"});
        let agent = |prompt: &Value, route: Value| json!({"prompt": prompt, "execution_route": route, "run_config": {"max_iterations": 5}});

        for (prompt, route) in [
            (&portable, gateway.clone()),
            (&native, json!({"kind": "native"})),
            (&reference, gateway.clone()),
        ] {
            let authored = agent(prompt, route);
            let spec: AgentSpec = serde_json::from_value(authored.clone()).expect("agent decodes");
            assert_eq!(
                serde_json::to_value(&spec).expect("agent encodes"),
                authored,
                "the agent serializes exactly as authored"
            );
        }
        let spec: AgentSpec =
            serde_json::from_value(agent(&portable, gateway.clone())).expect("agent decodes");
        assert_eq!(
            spec.execution_route,
            ExecutionRoute::WyrdGateway {
                deployment: "openai-primary".parse().expect("deployment name")
            }
        );
        let InlineableRef::Inline(prompt) = &spec.prompt else {
            panic!("an inline prompt stays inline");
        };
        assert!(matches!(
            prompt.response_type,
            skald_spec::ResponseType::JsonSchema { .. }
        ));
        assert_eq!(prompt.variables, vec!["question"]);

        let refused = |authored: Value, why: &str| {
            assert!(
                serde_json::from_value::<AgentSpec>(authored).is_err(),
                "{why} is refused"
            );
        };
        refused(
            json!({"prompt": portable, "run_config": {}}),
            "a missing execution_route",
        );
        refused(
            json!({"prompt": portable, "execution_route": {"kind": "native"}, "llm_route": {"kind": "native"}}),
            "an llm_route field",
        );
        refused(
            agent(
                &portable,
                json!({"kind": "ext_gateway", "base_url": "https://gw.example.com"}),
            ),
            "an ext_gateway route",
        );
        refused(
            agent(
                &portable,
                json!({"kind": "native", "deployment": "openai-primary"}),
            ),
            "a native route with a deployment",
        );
        refused(
            agent(&portable, json!({"kind": "wyrd_gateway"})),
            "a gateway route without a deployment",
        );
        refused(
            agent(
                &portable,
                json!({"kind": "wyrd_gateway", "deployment": "openai/gpt-4o"}),
            ),
            "a model address as a deployment",
        );
        let mut routed_prompt = portable.clone();
        routed_prompt["deployment"] = json!("openai-primary");
        refused(
            agent(&routed_prompt, gateway.clone()),
            "a Prompt deployment field",
        );
        let mut mixed = portable;
        mixed["format"] = json!("chat_completion");
        mixed["body"] = json!({"messages": []});
        refused(agent(&mixed, gateway), "a mixed portable/native Prompt");
    }
}
