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

use crate::ids::SlotName;
use crate::vala::eval::TaskId;

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

/// Cursor over reference text.
struct Tokens<'a> {
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
    type Err = ReferenceError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl Serialize for Reference {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Reference {
    fn schema_name() -> String {
        "Reference".to_owned()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
