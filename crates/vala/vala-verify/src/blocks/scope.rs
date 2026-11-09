//! The execution scope Verifier references resolve against.
//!
//! The scope reads the original evidence declarations, the shared
//! [`RunLedger`], which holds each settled step's status, output, and
//! requirement verdicts, and the [`Frame`] of the invocation being bound:
//! its fanout item, repeat iteration, and its own unsettled step. The driver
//! resolves every binding a block declares here before handing the block to
//! an executor, so executors only ever see values. A value that cannot be
//! supplied resolves to [`Missing`], which keeps the cause of the gap so an
//! intentional skip is never confused with a failure.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value;

use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::card::verifier::{PathSegment, Reference, ReferenceRoot, Requirement, VerdictField};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    InputDeclaration, Producer, ProducerInvocation, PublicationCollection, RequirementOutcome,
    RequirementResult, Speculation, StepResult, StepStatus,
};

use super::{CallInput, CallValue};
use crate::walk::RunLedger;

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

    /// The status of a step whose own control value is this gap: an
    /// intentional skip upstream stays a skip, and anything else is the
    /// step's evaluation error, never an implicit false.
    pub(super) const fn control_status(&self) -> StepStatus {
        match self.cause {
            Cause::Skipped => StepStatus::Skipped,
            Cause::Failed => StepStatus::Failed,
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

/// One settled step, or one settled invocation of a step, as later blocks
/// read it.
#[derive(Debug, Clone)]
pub(super) struct StepDone {
    /// The reported result.
    pub result: StepResult,
    /// The state `steps.<id>` references read.
    pub state: Settled,
    /// Every judged instance of the step's requirements, in item, iteration,
    /// then declaration order.
    pub requirements: Vec<RequirementResult>,
    /// Every invocation attempt, ordered by item, iteration, then attempt;
    /// the collection manifest reports them.
    pub invocations: Vec<ProducerInvocation>,
    /// The successful invocation attempt whose named publications
    /// `steps.<id>.publications.<name>` selects: a plain step's final attempt
    /// or a repeat's final successful iteration; `None` for fanout.
    pub member: Option<Producer>,
    /// The speculative disposition; anything but `none` and `admitted`
    /// withholds the contribution.
    pub speculation: Speculation,
}

/// One settled Verifier block in the shared [`RunLedger`].
///
/// Settlements are boxed because they are an order of magnitude larger than
/// the enum, and the ledger holds every block.
#[derive(Debug, Clone)]
pub(super) enum Done {
    /// A step with its requirement instances.
    Step(Box<StepDone>),
    /// A judged whole-Verifier requirement, as reported.
    Requirement(Box<RequirementResult>),
}

/// The ledger the engine shares with the blocks it starts, which each take a
/// snapshot of it.
pub(super) type Ledger = RunLedger<Arc<Done>>;

/// The invocation a binding resolves for: its fanout item, its repeat
/// iteration, and its own step's unsettled invocation once the action
/// settled, which that invocation's requirements and `until` read.
#[derive(Debug, Clone, Default)]
pub(super) struct Frame {
    /// The zero-based item and, for an `items` fanout, its value.
    pub item: Option<(u32, Option<CallInput>)>,
    /// The repeat iteration.
    pub iteration: Option<Iteration>,
    /// The own step's current invocation, once its action settled.
    pub own: Option<StepDone>,
}

impl Frame {
    /// The invocation this frame binds: its item and iteration.
    pub(super) fn instance(&self) -> (Option<u32>, Option<u32>) {
        (
            self.item.as_ref().map(|(at, _)| *at),
            self.iteration.as_ref().map(|iteration| iteration.index),
        )
    }
}

/// One repeat iteration's values.
#[derive(Debug, Clone)]
pub(super) struct Iteration {
    /// Zero-based iteration.
    pub index: u32,
    /// The previous iteration's output; `null` in the first.
    pub previous: Value,
    /// The just-completed output, which only `until` reads.
    pub output: Option<CallInput>,
}

/// The scope document of one invocation: the original evidence, every block
/// already settled, and the invocation's own frame. Read-only; the engine
/// owns the ledger.
pub(super) struct Scope<'a> {
    /// The executing Verifier, for slot declarations.
    spec: &'a VerifierSpec,
    /// The pinned original evidence by slot.
    evidence: &'a BTreeMap<SlotName, InputDeclaration>,
    /// Blocks already settled.
    ledger: &'a Ledger,
    /// The invocation being bound.
    frame: Frame,
}

impl<'a> Scope<'a> {
    /// The scope over the original evidence and the blocks in `ledger`.
    pub(super) fn new(
        spec: &'a VerifierSpec,
        evidence: &'a BTreeMap<SlotName, InputDeclaration>,
        ledger: &'a Ledger,
    ) -> Self {
        Self {
            spec,
            evidence,
            ledger,
            frame: Frame::default(),
        }
    }

    /// The same scope bound for the invocation `frame`.
    pub(super) fn framed(&self, frame: Frame) -> Self {
        Self {
            spec: self.spec,
            evidence: self.evidence,
            ledger: self.ledger,
            frame,
        }
    }

    /// Returns a settled step's state.
    pub(super) fn step(&self, step: &TaskId) -> Option<&Settled> {
        self.settled(step).map(|done| &done.state)
    }

    /// Returns a settled step, or the frame's own invocation of it.
    pub(super) fn settled(&self, step: &TaskId) -> Option<&StepDone> {
        if let Some(own) = &self.frame.own
            && &own.result.id == step
        {
            return Some(own);
        }
        match self.ledger.get(step).map(AsRef::as_ref) {
            Some(Done::Step(done)) => Some(done),
            _ => None,
        }
    }

    /// Returns the verdict of `step`'s requirement: the frame's own instance,
    /// or the settled step's last instance, which is a repeat's final
    /// iteration.
    fn verdict(&self, step: &TaskId, requirement: &TaskId) -> Option<&RequirementResult> {
        self.settled(step)?
            .requirements
            .iter()
            .rev()
            .find(|result| &result.id == requirement)
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
                step,
                requirement: id,
                field,
            } => {
                let verdict = self.verdict(step, id).ok_or_else(unresolved)?;
                let value = match field {
                    VerdictField::Outcome => wire(&verdict.outcome),
                    VerdictField::Reason => Value::String(verdict.reason.clone()),
                };
                Ok(json_input(value, BTreeSet::new()))
            }
            ReferenceRoot::StepPublications { step, name } => {
                let done = self.settled(step).ok_or_else(unresolved)?;
                match (name, &done.state, &done.member) {
                    (Some(name), Settled::Succeeded { sources, .. }, Some(producer)) => {
                        Ok(CallInput {
                            value: CallValue::Publication {
                                producer: producer.clone(),
                                name: name.clone(),
                            },
                            sources: sources.clone(),
                        })
                    }
                    (Some(_), Settled::Succeeded { .. }, None) => Err(Missing::failed(format!(
                        "step `{}` has one member per fanout item",
                        step.as_str()
                    ))),
                    (Some(_), Settled::NoOutput { gap, .. }, _) => Err(Missing {
                        cause: gap.cause,
                        reason: format!(
                            "step `{}` has no successful attempt to publish: {}",
                            step.as_str(),
                            gap.reason
                        ),
                    }),
                    (None, settled, _) => Ok(CallInput {
                        value: CallValue::Publications {
                            step: step.clone(),
                            collection: collection(done),
                        },
                        // Partial work of a failed step derives from no
                        // recorded sources, so it can cite none.
                        sources: match settled {
                            Settled::Succeeded { sources, .. } => sources.clone(),
                            Settled::NoOutput { .. } => BTreeSet::new(),
                        },
                    }),
                }
            }
            ReferenceRoot::RequirementId => {
                requirement.map_or_else(|| Err(unresolved()), |req| constant(req.id.as_str()))
            }
            ReferenceRoot::RequirementStatement => {
                requirement.map_or_else(|| Err(unresolved()), |req| constant(&req.statement))
            }
            ReferenceRoot::Item => match &self.frame.item {
                Some((_, Some(item))) => match &item.value {
                    CallValue::Json(value) => select(value, &reference.path)
                        .map(|value| json_input(value, item.sources.clone()))
                        .ok_or_else(unresolved),
                    _ => Err(unresolved()),
                },
                _ => Err(unresolved()),
            },
            ReferenceRoot::ItemIndex => self
                .frame
                .item
                .as_ref()
                .map(|(index, _)| json_input(Value::from(*index), BTreeSet::new()))
                .ok_or_else(unresolved),
            ReferenceRoot::IterationIndex => self
                .frame
                .iteration
                .as_ref()
                .map(|iteration| json_input(Value::from(iteration.index), BTreeSet::new()))
                .ok_or_else(unresolved),
            ReferenceRoot::IterationPrevious => {
                let iteration = self.frame.iteration.as_ref().ok_or_else(unresolved)?;
                select(&iteration.previous, &reference.path)
                    .map(|value| json_input(value, BTreeSet::new()))
                    .ok_or_else(unresolved)
            }
            ReferenceRoot::IterationOutput => {
                let output = self
                    .frame
                    .iteration
                    .as_ref()
                    .and_then(|iteration| iteration.output.as_ref())
                    .ok_or_else(unresolved)?;
                match &output.value {
                    CallValue::Json(value) => select(value, &reference.path)
                        .map(|value| json_input(value, output.sources.clone()))
                        .ok_or_else(unresolved),
                    _ => Err(unresolved()),
                }
            }
        }
    }
}

/// The reason a collection reports for an attempt that failed and was
/// retried; the retried failure itself is not recorded.
pub(super) const RETRIED: &str = "the attempt failed and was retried";

/// One record per attempt of one invocation that made `attempts` attempts
/// and ended as `status` for `reason`, each attempt before the last having
/// failed and been retried.
pub(super) fn attempt_records(
    item_index: Option<u32>,
    iteration_index: Option<u32>,
    attempts: u32,
    status: StepStatus,
    reason: Option<&str>,
) -> impl Iterator<Item = ProducerInvocation> + '_ {
    (1..=attempts).map(move |attempt| {
        let last = attempt == attempts;
        ProducerInvocation {
            item_index,
            iteration_index,
            attempt,
            status: if last { status } else { StepStatus::Failed },
            reason: if last {
                reason.map(str::to_owned)
            } else {
                Some(RETRIED.to_owned())
            },
        }
    })
}

/// The collection of the settled step `done`, with no entries: its status,
/// disposition, and every invocation attempt. The host fills the entries of
/// an admitted contribution only.
fn collection(done: &StepDone) -> PublicationCollection {
    PublicationCollection {
        status: done.result.status,
        reason: done.result.reason.clone(),
        speculation: done.speculation,
        invocations: done.invocations.clone(),
        entries: Vec::new(),
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
