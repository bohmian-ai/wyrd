//! The execution scope Verifier references resolve against.
//!
//! The scope reads the original evidence declarations and the shared
//! [`RunLedger`] of the stage walk, which holds each settled step's status
//! and output and each settled step requirement's verdict. The driver
//! resolves every binding a block declares here before handing the block to
//! an executor, so executors only ever see values. A value that cannot be
//! supplied resolves to [`Missing`], which keeps the cause of the gap so an
//! intentional skip is never confused with a failure.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use wyrd_spec::card::blocks::VerifierSpec;
use wyrd_spec::card::blocks::{PathSegment, Reference, ReferenceRoot, Requirement, VerdictField};
use wyrd_spec::ids::SlotName;
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::verification::execution::{
    InputDeclaration, RequirementOutcome, RequirementResult, StepResult, StepStatus,
};

use super::{CallInput, CallValue};
use crate::executor::RunLedger;

/// Why a value is missing. Ordered so the stronger cause is the greater:
/// when causes mix, failure wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Cause {
    /// An intentional skip: a false `when`, or blocked only by one.
    Skipped,
    /// A failure, timeout, or otherwise missing required value.
    Failed,
}

impl Cause {
    /// The outcome of a requirement whose evidence is missing for this cause.
    pub(super) const fn outcome(self) -> RequirementOutcome {
        match self {
            Self::Skipped => RequirementOutcome::NotApplicable,
            Self::Failed => RequirementOutcome::Inconclusive,
        }
    }
}

/// A value that could not be supplied, with its cause and a bounded reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Missing {
    /// Why the value is missing.
    pub cause: Cause,
    /// Human-readable explanation recorded on the dependent block.
    pub reason: String,
}

impl Missing {
    /// A missing value caused by a failure or absent required evidence.
    pub(super) fn failed(reason: impl Into<String>) -> Self {
        Self {
            cause: Cause::Failed,
            reason: reason.into(),
        }
    }

    /// Fold several gaps into one: the strongest cause wins, keeping the
    /// first reason recorded for that cause.
    pub(super) fn strongest(gaps: impl IntoIterator<Item = Self>) -> Option<Self> {
        gaps.into_iter()
            .fold(None, |best: Option<Self>, gap| match best {
                Some(best) if best.cause >= gap.cause => Some(best),
                _ => Some(gap),
            })
    }
}

/// The final state of one step as later blocks read it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Settled {
    /// The action produced `output`, derived from the `sources` slots.
    Succeeded {
        /// The action output.
        output: Value,
        /// Original slots the action read.
        sources: BTreeSet<SlotName>,
    },
    /// The step failed or was skipped, so it has no output.
    NoOutput {
        /// `failed` or `skipped`, as `steps.<id>.status` reads it.
        status: StepStatus,
        /// Why there is no output; a skip blocked by a failure carries the
        /// failure cause.
        gap: Missing,
    },
}

impl Settled {
    /// The status `steps.<id>.status` reads.
    pub(super) const fn status(&self) -> StepStatus {
        match self {
            Self::Succeeded { .. } => StepStatus::Succeeded,
            Self::NoOutput { status, .. } => *status,
        }
    }
}

/// One settled Verifier block in the shared [`RunLedger`].
///
/// Results are boxed because they are an order of magnitude larger than the
/// settled state, and the ledger holds every block.
#[derive(Debug)]
pub(super) enum Done {
    /// A step: its reported result and the state later blocks read.
    Step {
        /// The reported step result.
        result: Box<StepResult>,
        /// The state `steps.<id>` references read.
        state: Settled,
    },
    /// A judged requirement, as reported.
    Requirement(Box<RequirementResult>),
}

/// The scope document of one stage: the original evidence plus every block
/// earlier stages settled. Read-only; the stage walk owns the ledger.
pub(super) struct Scope<'a> {
    /// The executing Verifier, for slot declarations.
    spec: &'a VerifierSpec,
    /// The pinned original evidence by slot.
    evidence: &'a BTreeMap<SlotName, InputDeclaration>,
    /// Blocks settled by earlier stages.
    ledger: &'a RunLedger<Done>,
}

impl<'a> Scope<'a> {
    /// The scope over the original evidence and the blocks in `ledger`.
    pub(super) fn new(
        spec: &'a VerifierSpec,
        evidence: &'a BTreeMap<SlotName, InputDeclaration>,
        ledger: &'a RunLedger<Done>,
    ) -> Self {
        Self {
            spec,
            evidence,
            ledger,
        }
    }

    /// Returns a settled step's state.
    pub(super) fn step(&self, step: &TaskId) -> Option<&Settled> {
        match self.ledger.get(step) {
            Some(Done::Step { state, .. }) => Some(state),
            _ => None,
        }
    }

    /// Returns a settled step requirement's verdict.
    fn verdict(&self, requirement: &TaskId) -> Option<&RequirementResult> {
        match self.ledger.get(requirement) {
            Some(Done::Requirement(result)) => Some(result),
            _ => None,
        }
    }

    /// Resolve every named binding, returning the values or the strongest
    /// gap among them.
    ///
    /// # Errors
    /// Returns the strongest [`Missing`] when any binding cannot be supplied.
    pub(super) fn resolve_all<'r, K: ToString + 'r>(
        &self,
        bindings: impl IntoIterator<Item = (K, &'r Reference)>,
        requirement: Option<&Requirement>,
    ) -> Result<BTreeMap<String, CallInput>, Missing> {
        let mut resolved = BTreeMap::new();
        let mut gaps = Vec::new();
        for (name, reference) in bindings {
            match self.resolve(reference, requirement) {
                Ok(input) => {
                    resolved.insert(name.to_string(), input);
                }
                Err(gap) => gaps.push(gap),
            }
        }
        Missing::strongest(gaps).map_or(Ok(resolved), Err)
    }

    /// Resolve one reference, with `requirement` as the current requirement
    /// during its check.
    ///
    /// A JSON input or step output is navigated by the reference's child
    /// selectors; a whole media or trace slot resolves to its pinned
    /// declaration. An omitted optional slot is `null`. Step statuses and
    /// verdict fields resolve to their wire strings.
    ///
    /// # Errors
    /// Returns [`Missing`] carrying the upstream cause for the output of a
    /// step that produced none, and a failure for an omitted required slot,
    /// an unsettled step or verdict, or a selector that matches nothing.
    pub(super) fn resolve(
        &self,
        reference: &Reference,
        requirement: Option<&Requirement>,
    ) -> Result<CallInput, Missing> {
        let unresolved = || Missing::failed(format!("`{reference}` has no value"));
        let constant = |text: &str| Ok(json_input(Value::String(text.to_owned()), BTreeSet::new()));
        match &reference.root {
            ReferenceRoot::Input(slot) => {
                let sources = BTreeSet::from([slot.clone()]);
                match self.evidence.get(slot) {
                    Some(InputDeclaration::Json { value }) => select(value, &reference.path)
                        .map(|value| json_input(value, sources))
                        .ok_or_else(unresolved),
                    Some(declaration) if reference.path.is_empty() => Ok(CallInput {
                        value: CallValue::Evidence(declaration.clone()),
                        sources,
                    }),
                    Some(_) => Err(unresolved()),
                    None if self
                        .spec
                        .inputs
                        .get(slot)
                        .is_some_and(|slot| !slot.required()) =>
                    {
                        select(&Value::Null, &reference.path)
                            .map(|value| json_input(value, sources))
                            .ok_or_else(unresolved)
                    }
                    None => Err(Missing::failed(format!(
                        "required input `{slot}` was not supplied"
                    ))),
                }
            }
            ReferenceRoot::StepOutput(step) => match self.step(step) {
                Some(Settled::Succeeded { output, sources }) => select(output, &reference.path)
                    .map(|value| json_input(value, sources.clone()))
                    .ok_or_else(unresolved),
                Some(Settled::NoOutput { gap, .. }) => Err(Missing {
                    cause: gap.cause,
                    reason: format!(
                        "step `{}` produced no output: {}",
                        step.as_str(),
                        gap.reason
                    ),
                }),
                None => Err(unresolved()),
            },
            ReferenceRoot::StepStatus(step) => {
                let settled = self.step(step).ok_or_else(unresolved)?;
                Ok(json_input(wire(&settled.status()), BTreeSet::new()))
            }
            ReferenceRoot::StepVerdict {
                requirement: id,
                field,
                ..
            } => {
                let verdict = self.verdict(id).ok_or_else(unresolved)?;
                let value = match field {
                    VerdictField::Outcome => wire(&verdict.outcome),
                    VerdictField::Reason => Value::String(verdict.reason.clone()),
                };
                Ok(json_input(value, BTreeSet::new()))
            }
            ReferenceRoot::RequirementId => {
                requirement.map_or_else(|| Err(unresolved()), |req| constant(req.id.as_str()))
            }
            ReferenceRoot::RequirementStatement => {
                requirement.map_or_else(|| Err(unresolved()), |req| constant(&req.statement))
            }
        }
    }
}

/// A JSON binding value.
fn json_input(value: Value, sources: BTreeSet<SlotName>) -> CallInput {
    CallInput {
        value: CallValue::Json(value),
        sources,
    }
}

/// The serde wire form of a status or outcome enum, such as `"succeeded"`.
fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Apply child selectors to `value`.
///
/// Fields and indexes select one child; a `[*]` wildcard maps the remaining
/// selectors over every array element and yields the array of results.
/// Returns `None` when any selected field, index, or wildcard target is
/// absent, so a missing value is never mistaken for `null`.
fn select(value: &Value, path: &[PathSegment]) -> Option<Value> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(value.clone());
    };
    match segment {
        PathSegment::Field(name) => select(value.get(name)?, rest),
        PathSegment::Index(index) => select(value.get(index)?, rest),
        PathSegment::Wildcard => {
            // A later wildcard already yields an array of matches; flatten
            // it so `a[*].b[*]` is one array.
            let nested = rest.contains(&PathSegment::Wildcard);
            let mut matches = Vec::new();
            for item in value.as_array()? {
                match select(item, rest)? {
                    Value::Array(inner) if nested => matches.extend(inner),
                    other => matches.push(other),
                }
            }
            Some(Value::Array(matches))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Reference selection over the scope document.

    use serde_json::json;

    use super::*;

    /// Parse a reference's child selectors.
    ///
    /// # Panics
    /// Panics when `inputs.x<text>` is not a valid reference.
    fn path(text: &str) -> Vec<PathSegment> {
        Reference::parse(&format!("inputs.x{text}"))
            .expect("reference")
            .path
    }

    /// Fields, indexes, and nested wildcards select values; absent targets
    /// are missing while an explicit null is a value.
    ///
    /// # Panics
    /// Panics when a selection result differs from the expected value.
    #[test]
    fn select_distinguishes_absent_from_null() {
        let doc = json!({"sheets": [
            {"cells": [{"error": null}, {"error": "#DIV/0!"}]},
            {"cells": [{"error": null}]}
        ], "none": null});
        assert_eq!(
            select(&doc, &path(".sheets[0].cells[1].error")),
            Some(json!("#DIV/0!"))
        );
        assert_eq!(
            select(&doc, &path(".sheets[*].cells[*].error")),
            Some(json!([null, "#DIV/0!", null]))
        );
        assert_eq!(select(&doc, &path(".none")), Some(Value::Null));
        assert_eq!(select(&doc, &path(".absent")), None);
        assert_eq!(select(&doc, &path(".sheets[5]")), None);
        assert_eq!(select(&doc, &path(".none.child")), None);
        assert_eq!(
            select(&json!({"rows": []}), &path(".rows[*]")),
            Some(json!([]))
        );
    }
}
