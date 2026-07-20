//! Card references authored inside specs.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::envelope::{CardKind, Spec};
use crate::ids::{CardName, CardUid, SpaceName};
use crate::refs::{ReferenceSlotVisitor, SlotValue};
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
    ///
    /// Authors may omit it; the loader inherits the enclosing card's resolved
    /// metadata space before the reference crosses a wire boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceName>,
    /// Optional server-resolved durable UID.
    ///
    /// Authors identify a dependency with `(kind, space, name, version)` and
    /// may omit this field. The server resolves that reference to a [`CardUid`]
    /// before registration; the UID is then used for durable authorization and
    /// lineage without changing the authored reference identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<CardUid>,
}

/// A reference position that requires durable identity.
///
/// Loaders rewrite `Path` values to `Ref` values before submitting a request;
/// the server rejects unresolved paths at the wire boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", untagged)]
pub enum Ref {
    /// A direct reference to a registered Card.
    Ref(CardRef),
    /// A local authored path awaiting loader resolution.
    #[cfg_attr(feature = "server", schema(value_type = String))]
    Path(PathBuf),
}

impl Ref {
    /// Return the resolved [`CardRef`] when this ref carries durable identity.
    ///
    /// Returns `None` for [`Ref::Path`] values that a loader has not yet
    /// rewritten. Server code that observes `None` on the wire must reject
    /// with `WYRD_REGISTRY_400_UNRESOLVED_PATH_REF`.
    #[must_use]
    pub fn as_card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Ref(card_ref) => Some(card_ref),
            Self::Path(_) => None,
        }
    }

    /// Mutable variant of [`Ref::as_card_ref`].
    #[must_use]
    pub fn as_card_ref_mut(&mut self) -> Option<&mut CardRef> {
        match self {
            Self::Ref(card_ref) => Some(card_ref),
            Self::Path(_) => None,
        }
    }
}

impl From<CardRef> for Ref {
    fn from(card_ref: CardRef) -> Self {
        Self::Ref(card_ref)
    }
}

/// A reference position that may carry an embedded child spec.
///
/// Loaders rewrite `Path` values to `Ref` values before submitting a request;
/// the server rejects unresolved paths at the wire boundary. `Inline`
/// bodies are authored, not synthesized by the loader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", untagged)]
pub enum InlineableRef<T> {
    /// A direct reference to a registered Card.
    Ref(CardRef),
    /// An embedded child spec.
    Inline(Box<T>),
    /// A local authored path awaiting loader resolution.
    #[cfg_attr(feature = "server", schema(value_type = String))]
    Path(PathBuf),
}

impl<T> InlineableRef<T> {
    /// Return the resolved [`CardRef`] when this ref carries durable identity.
    ///
    /// Returns `None` for [`InlineableRef::Path`] (loader has not resolved
    /// it yet) and [`InlineableRef::Inline`] (the body is embedded, not
    /// referenced).
    #[must_use]
    pub fn as_card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Ref(card_ref) => Some(card_ref),
            Self::Inline(_) | Self::Path(_) => None,
        }
    }

    /// Mutable variant of [`InlineableRef::as_card_ref`].
    #[must_use]
    pub fn as_card_ref_mut(&mut self) -> Option<&mut CardRef> {
        match self {
            Self::Ref(card_ref) => Some(card_ref),
            Self::Inline(_) | Self::Path(_) => None,
        }
    }

    /// Return the embedded child body when authored inline.
    #[must_use]
    pub fn as_inline(&self) -> Option<&T> {
        match self {
            Self::Inline(body) => Some(body),
            Self::Ref(_) | Self::Path(_) => None,
        }
    }

    /// Return the embedded child body mutably when authored inline.
    #[must_use]
    pub fn as_inline_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Inline(body) => Some(body),
            Self::Ref(_) | Self::Path(_) => None,
        }
    }
}

impl<T> From<CardRef> for InlineableRef<T> {
    fn from(card_ref: CardRef) -> Self {
        Self::Ref(card_ref)
    }
}

impl From<skald_spec::Prompt> for InlineableRef<skald_spec::Prompt> {
    fn from(prompt: skald_spec::Prompt) -> Self {
        Self::Inline(Box::new(prompt))
    }
}

impl From<crate::card::agent::AgentSpec> for InlineableRef<crate::card::agent::AgentSpec> {
    fn from(spec: crate::card::agent::AgentSpec) -> Self {
        Self::Inline(Box::new(spec))
    }
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
    let mut spec = spec.clone();
    let mut references = Vec::new();
    ReferenceSlotVisitor::visit(&mut spec, |slot| match slot.value {
        SlotValue::Durable(reference) => {
            references.extend(reference.as_card_ref().cloned());
        }
        SlotValue::InlineablePrompt(reference) => {
            references.extend(reference.as_card_ref().cloned());
        }
        SlotValue::InlineableAgent(reference) => {
            references.extend(reference.as_card_ref().cloned());
        }
    });
    references
}

/// Bind server-resolved UIDs to the card references discovered by
/// [`scope_child_card_refs`]. The input pairs are authoritative; JSON values
/// are used only as the serialization boundary for the already typed spec and
/// are matched against the complete serialized `CardRef` identity shape. Any
/// existing `uid` is ignored because the server must replace authored values
/// with the UID it resolved.
pub fn bind_scoped_card_ref_uids(
    kind: &CardKind,
    spec: Spec,
    resolved: &[(CardRef, CardUid)],
) -> Result<Spec, crate::envelope::SpecDecodeError> {
    if resolved.is_empty() {
        return Ok(spec);
    }
    let mut value =
        serde_json::to_value(spec).map_err(|error| crate::envelope::SpecDecodeError {
            kind: kind.wire_name().to_owned(),
            message: error.to_string(),
        })?;
    for (card_ref, uid) in resolved {
        let mut target =
            serde_json::to_value(card_ref).expect("CardRef serialization is infallible");
        if let serde_json::Value::Object(object) = &mut target {
            object.remove("uid");
        }
        let uid_value = serde_json::to_value(uid).expect("CardUid serialization is infallible");
        bind_serialized_ref(&mut value, &target, &uid_value);
    }
    Spec::from_kind_and_value(kind, value)
}

/// Walk a serialized spec and add one resolved UID to exact `CardRef` objects.
///
/// Matching the complete serialized reference identity prevents a short generic
/// object with the same four identity fields from being rewritten accidentally.
/// The optional UID is excluded from the comparison so stale authored UIDs are
/// replaced. The traversal visits nested arrays and objects because references
/// can appear in several kind-specific spec shapes.
fn bind_serialized_ref(
    value: &mut serde_json::Value,
    target: &serde_json::Value,
    uid: &serde_json::Value,
) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                bind_serialized_ref(value, target, uid);
            }
        }
        serde_json::Value::Object(object) => {
            let mut identity = object.clone();
            identity.remove("uid");
            if serde_json::Value::Object(identity) == *target {
                object.insert("uid".to_owned(), uid.clone());
            }
            for value in object.values_mut() {
                bind_serialized_ref(value, target, uid);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

impl fmt::Display for CardRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}@{}",
            self.space
                .as_ref()
                .map_or("<missing-space>", SpaceName::as_str),
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
            space: Some(space),
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
    use crate::card::mcp::McpSpec;

    fn sample_ref() -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new("weights").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    #[test]
    fn card_ref_display_and_from_str_roundtrip() {
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("billing").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
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
            space: Some(SpaceName::new("default").expect("static space is valid")),
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
    fn binding_replaces_an_authored_uid_with_the_server_uid() {
        let authored_uid =
            CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11").expect("static uid is valid");
        let resolved_uid =
            CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b12").expect("static uid is valid");
        let authored_ref = CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("system-prompt").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("default").expect("static space is valid")),
            uid: Some(authored_uid),
        };
        let lookup_ref = CardRef {
            uid: None,
            ..authored_ref.clone()
        };
        let spec = Spec::Mcp(McpSpec {
            server_name: "tools".to_owned(),
            tool_refs: vec![authored_ref.into()],
            ..McpSpec::default()
        });

        let bound =
            bind_scoped_card_ref_uids(&CardKind::Mcp, spec, &[(lookup_ref, resolved_uid.clone())])
                .expect("MCP spec remains valid after UID binding");

        let Spec::Mcp(bound) = bound else {
            panic!("expected an MCP spec");
        };
        assert_eq!(
            bound.tool_refs[0]
                .as_card_ref()
                .and_then(|card_ref| card_ref.uid.clone()),
            Some(resolved_uid)
        );
    }

    #[test]
    fn card_ref_accepts_missing_authored_space() {
        let json = r#"{"kind":"Model","name":"churn","version":"1.0.0"}"#;
        let card_ref =
            serde_json::from_str::<CardRef>(json).expect("space is optional while authored");
        assert!(card_ref.space.is_none());
    }
}
