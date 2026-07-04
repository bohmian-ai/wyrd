//! Card references authored inside specs.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::card::agent::AgentSpec;
use crate::card::drift::DriftSignal;
use crate::envelope::{CardKind, Spec};
use crate::ids::{CardName, CardUid, SpaceName};
use wyrd_semver::VersionBlock;

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

impl CardRef {
    /// True when two card refs share the authorization identity tuple.
    ///
    /// The optional `uid` is ignored — authorization is identity-based on
    /// `(kind, name, version, space)` only.
    #[must_use]
    pub fn same_identity(&self, other: &CardRef) -> bool {
        self.kind == other.kind
            && self.name == other.name
            && self.version == other.version
            && self.space == other.space
    }
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

/// A principal's card authorization set.
///
/// Authorization is identity-based on `(kind, name, version, space)`. The
/// optional resolved `uid` is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardRefScope(Vec<CardRef>);

impl CardRefScope {
    /// Borrow scope members.
    #[must_use]
    pub fn as_slice(&self) -> &[CardRef] {
        &self.0
    }

    /// Whether the scope has no members.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of scope members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Build a scope and guarantee that `root` is the first member.
    ///
    /// Duplicate identity (same `(kind, name, version, space)`, ignoring `uid`)
    /// is removed. Root is always retained as the first element.
    #[must_use]
    pub fn from_root_and_members(
        root: &CardRef,
        members: impl IntoIterator<Item = CardRef>,
    ) -> Self {
        let mut scope = vec![root.clone()];
        for member in members {
            if !scope.iter().any(|existing| existing.same_identity(&member)) {
                scope.push(member);
            }
        }
        Self(scope)
    }

    /// Build an own-card-only scope.
    ///
    /// Passing this as `card_ref_scope` on issuance is the canonical way to
    /// express "this token is scoped only to its own card." An empty
    /// `card_ref_scope` on a received wire token carries the same meaning and
    /// is upgraded to `own` by the verifier's `seed_scope` call.
    #[must_use]
    pub fn own(root: &CardRef) -> Self {
        Self::from_root_and_members(root, std::iter::empty())
    }

    /// True when the scope is empty (vacuously) or contains the root card.
    ///
    /// An empty scope is permitted because the wire format omits the scope
    /// field for own-scoped tokens and the verifier upgrades it to `own(root)`
    /// via `seed_scope`. If you need a stricter guarantee that the root was
    /// explicitly present at construction time, use `authorizes` on a
    /// non-empty scope.
    #[must_use]
    pub fn permits_root(&self, root: &CardRef) -> bool {
        self.is_empty() || self.authorizes(root)
    }

    /// Return true when `card` is authorized by identity.
    #[must_use]
    pub fn authorizes(&self, card: &CardRef) -> bool {
        self.0.iter().any(|member| member.same_identity(card))
    }
}

/// Serializes as a JSON array of `Display` strings (`"space/Kind/name@version"`).
///
/// This is the canonical wire encoding for `card_ref_scope` members in JWT claims
/// and HTTP payloads. Individual `card_ref` fields use an object shape instead —
/// this asymmetry exists because scope members are positional (root-first) and
/// compact by design. An empty array means "own-card scope"; the verifier upgrades
/// it to `own(root)` via `seed_scope`.
impl Serialize for CardRefScope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(ToString::to_string))
    }
}

impl<'de> Deserialize<'de> for CardRefScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let members: Vec<CardRef> = Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|s| s.parse::<CardRef>().map_err(serde::de::Error::custom))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match members.as_slice() {
            [] => Self::default(),
            [root, rest @ ..] => Self::from_root_and_members(root, rest.iter().cloned()),
        })
    }
}

/// Every card ref a spec declares, for auth-scope traversal.
#[must_use]
pub fn scope_child_card_refs(spec: &Spec) -> Vec<CardRef> {
    match spec {
        Spec::Data(data) => data
            .card_refs()
            .chain(data.materialized_split_refs())
            .chain(data.interface.manifest_ref())
            .cloned()
            .collect(),
        Spec::Model(model) => model.card_refs().cloned().collect(),
        Spec::Experiment(experiment) => experiment
            .target_refs
            .iter()
            .chain(experiment.card_refs.iter())
            .cloned()
            .collect(),
        Spec::Prompt(_) => Vec::new(),
        Spec::Agent(agent) => crate::card::agent::scope_child_card_refs(agent),
        Spec::Workflow(workflow) => crate::card::workflow::scope_child_card_refs(workflow),
        Spec::Eval(eval) => eval
            .subject_ref
            .iter()
            .chain(eval.dataset.iter().map(|dataset| dataset.as_card_ref()))
            .cloned()
            .collect(),
        Spec::Drift(drift) => {
            let mut out = vec![drift.subject_ref.clone()];
            match &drift.signal {
                DriftSignal::Distribution { baseline_ref, .. } => out.push(baseline_ref.clone()),
                DriftSignal::EvalScore { eval_ref } => out.push(eval_ref.clone()),
                DriftSignal::External { source_ref } => out.push(source_ref.clone()),
                DriftSignal::Metric { .. } => {}
            }
            out
        }
        Spec::Service(service) => service
            .components
            .iter()
            .map(|component| component.card_ref.clone())
            .collect(),
        Spec::Policy(_) => Vec::new(),
        Spec::Mcp(mcp) => mcp.tool_refs.clone(),
        Spec::Audit(audit) => audit
            .subject_refs
            .iter()
            .chain(audit.policy_refs.iter())
            .chain(audit.evidence_refs.iter())
            .cloned()
            .collect(),
        Spec::Artifact(artifact) => artifact.schema_ref.iter().cloned().collect(),
        Spec::Trigger(trigger) => {
            let source = match &trigger.source {
                crate::card::trigger::TriggerSource::DriftObservation { card }
                | crate::card::trigger::TriggerSource::EvalObservation { card } => {
                    Some(card.clone())
                }
                crate::card::trigger::TriggerSource::Schedule { .. } => None,
            };
            source.into_iter().chain([trigger.target.clone()]).collect()
        }
        Spec::Operator(operator) => operator
            .pre_invoke
            .iter()
            .chain(operator.post_invoke.iter())
            .cloned()
            .collect(),
        Spec::Source(_) => Vec::new(),
    }
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
    Version(wyrd_semver::VersionError),
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
