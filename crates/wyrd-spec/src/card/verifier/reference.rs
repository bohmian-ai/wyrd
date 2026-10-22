//! Scope references used by Verifier actions, conditions, and checks.
//!
//! A reference names one value in the execution scope document:
//! `inputs.<slot>`, `steps.<id>.output`, `steps.<id>.status`,
//! `steps.<id>.requirements.<requirement>.outcome|reason`, or
//! `requirement.id|statement`. Input and step-output roots accept dotted
//! child fields and `[n]` array indexes; `[*]` wildcards parse here and are
//! refused by validation everywhere except an assertion's `each`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::{CardName, SlotName};
use crate::verification::TaskId;

/// One parsed scope reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The scope value the reference starts from.
    pub root: ReferenceRoot,
    /// Child selectors applied below an input or step-output root.
    pub path: Vec<PathSegment>,
}

/// The scope value a [`Reference`] starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceRoot {
    /// `inputs.<slot>`: one declared input slot.
    Input(SlotName),
    /// `steps.<id>.output`: one step's action output.
    StepOutput(TaskId),
    /// `steps.<id>.status`: one step's final status.
    StepStatus(TaskId),
    /// `steps.<id>.requirements.<requirement>.<field>`: one step
    /// requirement's verdict field.
    StepVerdict {
        /// Step that owns the requirement.
        step: TaskId,
        /// Requirement whose verdict is read.
        requirement: TaskId,
        /// Verdict field read.
        field: VerdictField,
    },
    /// `requirement.id`: the current requirement's ID during its check.
    RequirementId,
    /// `requirement.statement`: the current requirement's statement during
    /// its check.
    RequirementStatement,
}

/// A readable field of a requirement verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictField {
    /// The requirement outcome.
    Outcome,
    /// The requirement reason.
    Reason,
}

/// One child selector below an input or step-output root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// `.name`: an object field.
    Field(String),
    /// `[n]`: a zero-based array index.
    Index(usize),
    /// `[*]`: every array element.
    Wildcard,
}

/// Error returned when a string is not a valid [`Reference`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid reference `{reference}`: {reason}")]
pub struct ReferenceError {
    /// The rejected reference text.
    pub reference: String,
    /// Why the reference was rejected.
    pub reason: &'static str,
}

impl Reference {
    /// Returns the step this reference reads, when it reads any step value.
    #[must_use]
    pub fn step(&self) -> Option<&TaskId> {
        match &self.root {
            ReferenceRoot::StepOutput(step)
            | ReferenceRoot::StepStatus(step)
            | ReferenceRoot::StepVerdict { step, .. } => Some(step),
            ReferenceRoot::Input(_)
            | ReferenceRoot::RequirementId
            | ReferenceRoot::RequirementStatement => None,
        }
    }

    /// Returns the input slot this reference reads, when it reads one.
    #[must_use]
    pub fn slot(&self) -> Option<&SlotName> {
        match &self.root {
            ReferenceRoot::Input(slot) => Some(slot),
            _ => None,
        }
    }

    /// Returns whether any path segment is a `[*]` wildcard.
    #[must_use]
    pub fn has_wildcard(&self) -> bool {
        self.path.contains(&PathSegment::Wildcard)
    }

    /// Parses a reference.
    ///
    /// # Errors
    /// Returns [`ReferenceError`] for an unknown root, an invalid slot or
    /// block ID, a malformed selector, or child selectors on a root that is
    /// not an input or step output.
    pub fn parse(text: &str) -> Result<Self, ReferenceError> {
        let fail = |reason| ReferenceError {
            reference: text.to_owned(),
            reason,
        };
        let mut tokens = Tokens::new(text);
        let head = tokens.name().ok_or_else(|| fail("expected a root name"))?;
        let root = match head {
            "inputs" => {
                let slot = tokens
                    .dotted()
                    .ok_or_else(|| fail("expected inputs.<slot>"))?;
                ReferenceRoot::Input(SlotName::new(slot).map_err(|_| fail("invalid slot name"))?)
            }
            "steps" => {
                let step = tokens.dotted().ok_or_else(|| fail("expected steps.<id>"))?;
                let step = TaskId::new(step).map_err(|_| fail("invalid step id"))?;
                match tokens.dotted() {
                    Some("output") => ReferenceRoot::StepOutput(step),
                    Some("status") => ReferenceRoot::StepStatus(step),
                    Some("requirements") => {
                        let requirement = tokens
                            .dotted()
                            .ok_or_else(|| fail("expected requirements.<id>"))?;
                        let requirement =
                            TaskId::new(requirement).map_err(|_| fail("invalid requirement id"))?;
                        let field = match tokens.dotted() {
                            Some("outcome") => VerdictField::Outcome,
                            Some("reason") => VerdictField::Reason,
                            _ => return Err(fail("expected .outcome or .reason")),
                        };
                        ReferenceRoot::StepVerdict {
                            step,
                            requirement,
                            field,
                        }
                    }
                    _ => return Err(fail("expected .output, .status, or .requirements")),
                }
            }
            "requirement" => match tokens.dotted() {
                Some("id") => ReferenceRoot::RequirementId,
                Some("statement") => ReferenceRoot::RequirementStatement,
                _ => return Err(fail("expected requirement.id or requirement.statement")),
            },
            _ => return Err(fail("root must be inputs, steps, or requirement")),
        };
        let path = tokens
            .segments()
            .ok_or_else(|| fail("malformed selector"))?;
        if !path.is_empty()
            && !matches!(root, ReferenceRoot::Input(_) | ReferenceRoot::StepOutput(_))
        {
            return Err(fail("only inputs and step outputs accept child selectors"));
        }
        Ok(Self { root, path })
    }
}

/// A group Verifier's source declaration as authored: the text of one
/// input slot declared as a string rather than a typed slot.
///
/// Decoding accepts any string so registration can refuse a malformed source
/// at its own `inputs.<slot>` field; [`SourceText::parse`] yields the typed
/// [`SourceReference`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceText(
    /// The authored `verifiers.<name>...` text, kept verbatim so coverage
    /// pins exactly what the author wrote.
    String,
);

impl SourceText {
    /// The authored text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse the authored text as a source reference.
    ///
    /// # Errors
    /// As [`SourceReference::parse`].
    pub fn parse(&self) -> Result<SourceReference, ReferenceError> {
        SourceReference::parse(&self.0)
    }
}

impl schemars::JsonSchema for SourceText {
    /// Names the source schema in generated definitions.
    fn schema_name() -> String {
        "SourceText".to_owned()
    }

    /// Describes a source declaration as a string naming its two forms.
    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Metadata, SchemaObject};

        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(SOURCE_DESCRIPTION.to_owned()),
                ..Metadata::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            ..SchemaObject::default()
        }
        .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for SourceText {
    /// Describes a source declaration as its authored string form.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(SOURCE_DESCRIPTION))
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for SourceText {}

/// Generated-schema description of a source declaration.
const SOURCE_DESCRIPTION: &str = "Group source reference: verifiers.<name>.inputs.<slot> or \
     verifiers.<name>.requirements.<requirement>.outcome, selecting one whole slot or \
     requirement outcome per source execution.";

/// One parsed group source reference: a whole input slot or one
/// requirement's outcome of the same-owner Verifier named `verifier`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceReference {
    /// The source Verifier Card's `metadata.name`.
    pub verifier: CardName,
    /// What each source execution contributes.
    pub field: SourceField,
}

/// The value one source execution contributes to a group input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceField {
    /// `inputs.<slot>`: the execution's whole supplied slot value.
    Input(SlotName),
    /// `requirements.<requirement>.outcome`: the requirement's outcome.
    Outcome(TaskId),
}

impl SourceReference {
    /// Parses `verifiers.<name>.inputs.<slot>` or
    /// `verifiers.<name>.requirements.<requirement>.outcome`.
    ///
    /// # Errors
    /// Returns [`ReferenceError`] for another root, an invalid Verifier name,
    /// slot or requirement ID, a `.reason` field, and any child field, index
    /// or wildcard after the whole slot or outcome.
    pub fn parse(text: &str) -> Result<Self, ReferenceError> {
        let fail = |reason| ReferenceError {
            reference: text.to_owned(),
            reason,
        };
        let mut tokens = Tokens::new(text);
        if tokens.name() != Some("verifiers") {
            return Err(fail(
                "a source must be verifiers.<name>.inputs.<slot> or verifiers.<name>.requirements.<id>.outcome",
            ));
        }
        let verifier = tokens
            .dotted()
            .and_then(|name| CardName::new(name).ok())
            .ok_or_else(|| fail("expected verifiers.<name> naming a Verifier Card"))?;
        let field = match tokens.dotted() {
            Some("inputs") => SourceField::Input(
                tokens
                    .dotted()
                    .and_then(|slot| SlotName::new(slot).ok())
                    .ok_or_else(|| fail("expected inputs.<slot>"))?,
            ),
            Some("requirements") => {
                let requirement = tokens
                    .dotted()
                    .and_then(|id| TaskId::new(id).ok())
                    .ok_or_else(|| fail("expected requirements.<id>"))?;
                match tokens.dotted() {
                    Some("outcome") => SourceField::Outcome(requirement),
                    Some("reason") => {
                        return Err(fail(
                            "a source selects a requirement outcome only; .reason is not selectable",
                        ));
                    }
                    _ => return Err(fail("expected requirements.<id>.outcome")),
                }
            }
            _ => {
                return Err(fail(
                    "expected .inputs.<slot> or .requirements.<id>.outcome",
                ));
            }
        };
        if !tokens.rest.is_empty() {
            return Err(fail(
                "a source selects a whole slot or outcome; child fields, indexes and wildcards are refused",
            ));
        }
        Ok(Self { verifier, field })
    }
}

impl fmt::Display for SourceReference {
    /// Writes the canonical source text, which [`SourceReference::parse`]
    /// reads back to an equal value.
    ///
    /// # Errors
    /// Returns [`fmt::Error`] when the formatter's underlying writer fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            SourceField::Input(slot) => write!(f, "verifiers.{}.inputs.{slot}", self.verifier),
            SourceField::Outcome(requirement) => write!(
                f,
                "verifiers.{}.requirements.{}.outcome",
                self.verifier,
                requirement.as_str()
            ),
        }
    }
}

/// Cursor over reference text.
struct Tokens<'a> {
    /// The text not yet consumed; each method advances it only on success
    /// of the token it reads.
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    /// Starts a cursor at the beginning of `text`.
    fn new(text: &'a str) -> Self {
        Self { rest: text }
    }

    /// Consumes one name made of ASCII alphanumerics, `_`, and `-`.
    fn name(&mut self) -> Option<&'a str> {
        let end = self
            .rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
            .unwrap_or(self.rest.len());
        if end == 0 {
            return None;
        }
        let (name, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(name)
    }

    /// Consumes `.name`.
    fn dotted(&mut self) -> Option<&'a str> {
        self.rest = self.rest.strip_prefix('.')?;
        self.name()
    }

    /// Consumes the remaining child selectors, or `None` when malformed.
    fn segments(&mut self) -> Option<Vec<PathSegment>> {
        let mut segments = Vec::new();
        while !self.rest.is_empty() {
            if self.rest.starts_with('.') {
                segments.push(PathSegment::Field(self.dotted()?.to_owned()));
            } else {
                let (inner, rest) = self.rest.strip_prefix('[')?.split_once(']')?;
                self.rest = rest;
                segments.push(if inner == "*" {
                    PathSegment::Wildcard
                } else if !inner.is_empty() && inner.bytes().all(|byte| byte.is_ascii_digit()) {
                    PathSegment::Index(inner.parse().ok()?)
                } else {
                    return None;
                });
            }
        }
        Some(segments)
    }
}

impl fmt::Display for Reference {
    /// Writes the canonical reference text, which [`Reference::parse`]
    /// reads back to an equal value.
    ///
    /// # Errors
    /// Returns [`fmt::Error`] when the formatter's underlying writer fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.root {
            ReferenceRoot::Input(slot) => write!(f, "inputs.{slot}")?,
            ReferenceRoot::StepOutput(step) => write!(f, "steps.{}.output", step.as_str())?,
            ReferenceRoot::StepStatus(step) => write!(f, "steps.{}.status", step.as_str())?,
            ReferenceRoot::StepVerdict {
                step,
                requirement,
                field,
            } => {
                let field = match field {
                    VerdictField::Outcome => "outcome",
                    VerdictField::Reason => "reason",
                };
                write!(
                    f,
                    "steps.{}.requirements.{}.{field}",
                    step.as_str(),
                    requirement.as_str()
                )?;
            }
            ReferenceRoot::RequirementId => f.write_str("requirement.id")?,
            ReferenceRoot::RequirementStatement => f.write_str("requirement.statement")?,
        }
        for segment in &self.path {
            match segment {
                PathSegment::Field(name) => write!(f, ".{name}")?,
                PathSegment::Index(index) => write!(f, "[{index}]")?,
                PathSegment::Wildcard => f.write_str("[*]")?,
            }
        }
        Ok(())
    }
}

impl FromStr for Reference {
    /// Why the text is not a reference.
    type Err = ReferenceError;

    /// Parses reference text; see [`Reference::parse`].
    ///
    /// # Errors
    /// Returns [`ReferenceError`] for an unknown root, malformed selector, or
    /// a child selector on a root that has no children.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl Serialize for Reference {
    /// Writes the reference as its canonical text.
    ///
    /// # Errors
    /// Returns the serializer's error.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Reference {
    /// Reads reference text and parses it.
    ///
    /// # Errors
    /// Returns a custom error when the value is not a string or the text is
    /// not a valid reference.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Reference {
    /// Names the reference schema in generated definitions.
    fn schema_name() -> String {
        "Reference".to_owned()
    }

    /// Describes a reference as a string, documenting the accepted roots
    /// and selectors.
    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Metadata, SchemaObject};

        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(
                    "Scope reference: inputs.<slot>, steps.<id>.output, steps.<id>.status, \
                     steps.<id>.requirements.<id>.outcome|reason, requirement.id, or \
                     requirement.statement, with .field, [n], and (in assert each) [*] \
                     selectors below inputs and step outputs."
                        .to_owned(),
                ),
                ..Metadata::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            ..SchemaObject::default()
        }
        .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for Reference {
    /// Describes a reference as its authored string form.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(
                "Scope reference: inputs.<slot>, steps.<id>.output, steps.<id>.status, \
                 steps.<id>.requirements.<id>.outcome|reason, requirement.id, or \
                 requirement.statement, with .field, [n], and (in assert each) [*] \
                 selectors below inputs and step outputs.",
            ))
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for Reference {}

/// A group Verifier's declared source-list order, authored as `order_by`.
///
/// Every list is ordered by the server-recorded input time, then execution
/// ID, unless an input key orders it first; that default also breaks ties of
/// an input key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderBy {
    /// `recorded_at`: the time the server recorded each source execution's
    /// inputs, then execution ID. The default when `order_by` is absent.
    RecordedAt,
    /// `inputs.<slot>...`: a key read from the value of a source slot that
    /// reads a source input; numbers sort before strings before anything
    /// else, ascending.
    Input(Reference),
}

/// Authored text of [`OrderBy::RecordedAt`].
const RECORDED_AT: &str = "recorded_at";

/// Generated-schema description of an `order_by` declaration.
const ORDER_DESCRIPTION: &str = "Group source-list order: recorded_at (server-recorded input \
     time, the default) or inputs.<slot> with .field and [n] selectors naming a sortable key \
     of a source slot that reads a source input. Ties order by recorded_at, then execution ID.";

impl OrderBy {
    /// Parses `recorded_at` or an input reference.
    ///
    /// # Errors
    /// Returns [`ReferenceError`] when the text is neither `recorded_at` nor
    /// a valid reference.
    pub fn parse(text: &str) -> Result<Self, ReferenceError> {
        if text == RECORDED_AT {
            return Ok(Self::RecordedAt);
        }
        Reference::parse(text).map(Self::Input)
    }
}

impl fmt::Display for OrderBy {
    /// Writes the authored text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordedAt => f.write_str(RECORDED_AT),
            Self::Input(reference) => reference.fmt(f),
        }
    }
}

impl Serialize for OrderBy {
    /// Writes the order as its authored text.
    ///
    /// # Errors
    /// Returns the serializer's error.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for OrderBy {
    /// Reads order text and parses it.
    ///
    /// # Errors
    /// Returns a custom error when the value is not a string or the text is
    /// neither `recorded_at` nor a valid reference.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for OrderBy {
    /// Names the order schema in generated definitions.
    fn schema_name() -> String {
        "OrderBy".to_owned()
    }

    /// Describes an order as a string naming its two forms.
    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Metadata, SchemaObject};

        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(ORDER_DESCRIPTION.to_owned()),
                ..Metadata::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            ..SchemaObject::default()
        }
        .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for OrderBy {
    /// Describes an order as its authored string form.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(ORDER_DESCRIPTION))
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for OrderBy {}

#[cfg(test)]
mod tests {
    //! Reference text parsing, printing, and serde refusals.

    use super::*;

    /// Both source forms parse and print back; child selectors, `.reason`,
    /// and other roots are refused.
    ///
    /// # Panics
    /// Panics when a source form is classified incorrectly.
    #[test]
    fn source_reference_accepts_only_whole_slots_and_outcomes() {
        for text in [
            "verifiers.turn-verifier.inputs.turn",
            "verifiers.refund-turn.requirements.valid-fact.outcome",
        ] {
            let parsed = SourceReference::parse(text).expect(text);
            assert_eq!(parsed.to_string(), text);
        }
        for text in [
            "verifiers.turn-verifier.inputs.turn.text",
            "verifiers.turn-verifier.inputs.turn[0]",
            "verifiers.turn-verifier.inputs.turn[*]",
            "verifiers.refund-turn.requirements.valid-fact.reason",
            "verifiers.refund-turn.requirements.valid-fact.outcome.x",
            "verifiers.refund-turn.steps.a.output",
            "verifiers.X.inputs.turn",
            "inputs.turn",
        ] {
            assert!(SourceReference::parse(text).is_err(), "{text}");
        }
    }

    /// Every approved root and selector form parses and prints back unchanged.
    ///
    /// # Panics
    /// Panics when a valid reference is refused or does not round-trip.
    #[test]
    fn reference_round_trips_every_root() {
        for text in [
            "inputs.answer",
            "inputs.reply.refund_amount",
            "steps.structure.output.slides[*].title",
            "steps.workbook.output.sheets[0].cells[1].value",
            "steps.scope.status",
            "steps.scope.requirements.reviewable-size.outcome",
            "steps.scope.requirements.reviewable-size.reason",
            "requirement.id",
            "requirement.statement",
        ] {
            let reference = Reference::parse(text).expect(text);
            assert_eq!(reference.to_string(), text);
        }
        let reference = Reference::parse("steps.a.output.x[2]").expect("parse");
        assert_eq!(reference.step().map(TaskId::as_str), Some("a"));
        assert_eq!(
            reference.path,
            vec![PathSegment::Field("x".to_owned()), PathSegment::Index(2)]
        );
    }

    /// Malformed roots, IDs, and selectors are refused at parse time.
    ///
    /// # Panics
    /// Panics when an invalid reference parses.
    #[test]
    fn reference_refuses_malformed_text() {
        for text in [
            "",
            "$.inputs.a",
            "output",
            "inputs",
            "inputs.a-b",
            "steps.a",
            "steps.a.result",
            "steps.a.requirements.b",
            "steps.a.requirements.b.verdict",
            "steps.a.status.x",
            "steps.a.requirements.b.outcome.x",
            "requirement.statement.x",
            "requirement.other",
            "inputs.a.",
            "inputs.a[]",
            "inputs.a[x]",
            "inputs.a[-1]",
            "inputs.a[0",
            "inputs.a..b",
            "inputs.a b",
        ] {
            assert!(Reference::parse(text).is_err(), "{text}");
        }
    }

    /// References serialize as their string form.
    ///
    /// # Panics
    /// Panics when serde does not use the string form.
    #[test]
    fn reference_serde_uses_string_form() {
        let reference: Reference =
            serde_json::from_str("\"inputs.reply.text\"").expect("deserialize");
        assert_eq!(
            serde_json::to_value(&reference).expect("serialize"),
            "inputs.reply.text"
        );
        assert!(serde_json::from_str::<Reference>("\"steps.x\"").is_err());
    }
}
