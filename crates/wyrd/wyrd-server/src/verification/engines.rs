//! The typed outcome of one queued Verifier execution.
//!
//! [`VerifierRunner`](super::runner::VerifierRunner) runs every claimed
//! execution through the one building-block engine and maps its result to an
//! [`EngineOutcome`]; the runner turns every outcome into exactly one fenced
//! lifecycle transition. The helpers here derive the bounded Operator failure
//! context from a completed [`VerificationOutcome`].

use std::collections::BTreeMap;

use wyrd_spec::card::operator::DriftCounts;
use wyrd_spec::card::verifier::{Requirement, SourceText, VerifierSpec};
use wyrd_spec::verification::execution::{
    RequirementDetail, RequirementOutcome, RequirementResult, StepResult, StepStatus,
    VerificationOutcome,
};
use wyrd_spec::verification::{DriftVerdict, TaskId, VerificationError, VerificationVerdict};
use wyrd_sql::queries::verifier_executions::TerminalStatus;

/// The typed result of one engine execution.
///
/// The runner maps each variant to exactly one transition: `Completed`
/// stages the result and completes, `Retry` reschedules the same execution
/// and input within its attempt budget, `Deferred` requeues after one poll
/// interval without charging an attempt, and `Terminal` settles without a
/// verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineOutcome {
    /// The engine produced a verdict and its breakdown.
    Completed(VerificationOutcome),
    /// A transient failure; the same execution and frozen input may be
    /// attempted again.
    Retry(VerificationError),
    /// Bifrost refused an input read, or the process refused a hosted block
    /// a Python worker or workspace, at admission. That is backpressure from
    /// shared capacity, not a failure of this execution, so it waits one poll
    /// interval and tries again without spending an attempt.
    Deferred(VerificationError),
    /// A terminal failure that carries no verdict.
    Terminal(TerminalStatus, VerificationError),
}

/// The bounded human-readable summary frozen into each Operator failure
/// context.
///
/// Only the verdict and requirement counts: never actual values, citations,
/// or step output. A group judgment refused by the source-execution ceiling
/// also carries that reason, which names only the authored source and the
/// ceiling. Settlement clips it to the Operator summary bound.
#[must_use]
pub fn summary(outcome: &VerificationOutcome) -> String {
    let failed = outcome
        .requirements
        .iter()
        .filter(|requirement| requirement.outcome == RequirementOutcome::Failed)
        .count();
    let mut summary = format!(
        "Verdict {}: {failed} of {} requirements failed.",
        <&str>::from(outcome.verdict),
        outcome.requirements.len()
    );
    if let Some(limit) = outcome.requirements.iter().find(|requirement| {
        requirement
            .reason
            .starts_with(SOURCE_EXECUTION_LIMIT_EXCEEDED)
    }) {
        summary.push(' ');
        summary.push_str(&limit.reason);
    }
    summary
}

/// Requirement reason of a group judgment whose source selection exceeded
/// the deployment source-execution ceiling.
pub const SOURCE_EXECUTION_LIMIT_EXCEEDED: &str = "source_execution_limit_exceeded";

/// The inconclusive outcome of a group judgment whose `reference` selected
/// more than `ceiling` source executions.
///
/// Nothing was read for the snapshot, so no truncated list is judged: every
/// requirement is inconclusive with a reason naming the reference and the
/// ceiling, and every step is skipped with the same reason.
#[must_use]
pub fn source_limit_exceeded(
    spec: &VerifierSpec,
    reference: &SourceText,
    ceiling: u32,
) -> VerificationOutcome {
    let reason = format!(
        "{SOURCE_EXECUTION_LIMIT_EXCEEDED}: {} selected more than {ceiling} source executions",
        reference.as_str()
    );
    let requirement = |step: Option<&TaskId>, requirement: &Requirement| RequirementResult {
        id: requirement.id.clone(),
        step: step.cloned(),
        statement: requirement.statement.clone(),
        outcome: RequirementOutcome::Inconclusive,
        reason: reason.clone(),
        actual: None,
        citations: Vec::new(),
        detail: None,
        check: requirement.check.clone(),
        provenance: None,
    };
    VerificationOutcome {
        verdict: VerificationVerdict::Inconclusive,
        requirements: spec
            .steps
            .iter()
            .flat_map(|step| {
                step.requirements
                    .iter()
                    .map(|req| requirement(Some(&step.id), req))
            })
            .chain(spec.requirements.iter().map(|req| requirement(None, req)))
            .collect(),
        steps: spec
            .steps
            .iter()
            .map(|step| StepResult {
                id: step.id.clone(),
                status: StepStatus::Skipped,
                output: None,
                attempts: 0,
                duration_ms: 0,
                reason: Some(reason.clone()),
                provenance: None,
                tool_calls: Vec::new(),
            })
            .collect(),
        evidence: BTreeMap::new(),
    }
}

/// The feature counts of the outcome's one drift requirement, frozen beside
/// [`summary`] into each Operator failure context.
///
/// `None` unless exactly one requirement carries drift detail, since the
/// `drift.*` template fields name a single check.
#[must_use]
pub fn drift_counts(outcome: &VerificationOutcome) -> Option<DriftCounts> {
    let mut reports = outcome.requirements.iter().filter_map(|requirement| {
        requirement
            .detail
            .as_ref()
            .map(|RequirementDetail::Drift(report)| report)
    });
    let report = reports.next()?;
    if reports.next().is_some() {
        return None;
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Some(DriftCounts {
        drifted_features: count(
            report
                .features
                .values()
                .filter(|feature| feature.verdict == DriftVerdict::Drift)
                .count(),
        ),
        total_features: count(report.features.len()),
    })
}

#[cfg(test)]
mod tests {
    //! Operator failure context derived from an outcome.

    use std::collections::BTreeMap;

    use wyrd_spec::card::drift::DriftMethod;
    use wyrd_spec::card::verifier::Check;
    use wyrd_spec::verification::TaskId;
    use wyrd_spec::verification::execution::RequirementResult;
    use wyrd_spec::verification::{DriftReport, VerificationVerdict};

    use super::*;

    /// One failed requirement with optional drift detail.
    ///
    /// # Panics
    /// Panics when `id` is not a valid task id; every caller passes a static
    /// identifier literal.
    fn requirement(id: &str, detail: Option<RequirementDetail>) -> RequirementResult {
        RequirementResult {
            id: TaskId::new(id).expect("valid task id"),
            step: None,
            statement: id.to_owned(),
            outcome: RequirementOutcome::Failed,
            reason: "failed".to_owned(),
            actual: None,
            citations: Vec::new(),
            detail,
            check: serde_json::from_value::<Check>(serde_json::json!({
                "kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true
            }))
            .expect("assert check decodes"),
            provenance: None,
        }
    }

    /// An empty drift report detail.
    fn drift() -> RequirementDetail {
        RequirementDetail::Drift(DriftReport {
            method: DriftMethod::Custom,
            features: BTreeMap::new(),
            verdict: DriftVerdict::Drift,
        })
    }

    /// The summary counts failed requirements, and drift counts exist only
    /// for exactly one drift requirement.
    ///
    /// # Panics
    /// Panics when the derived context differs.
    #[test]
    fn failure_context_counts_requirements_and_the_one_drift_check() {
        let mut outcome = VerificationOutcome {
            verdict: VerificationVerdict::Failed,
            requirements: vec![requirement("a", Some(drift())), requirement("b", None)],
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        };
        assert_eq!(
            summary(&outcome),
            "Verdict failed: 2 of 2 requirements failed."
        );
        assert_eq!(
            drift_counts(&outcome),
            Some(DriftCounts {
                drifted_features: 0,
                total_features: 0
            })
        );
        outcome.requirements.push(requirement("c", Some(drift())));
        assert_eq!(drift_counts(&outcome), None);
    }

    /// A source over the ceiling judges nothing: every requirement is
    /// inconclusive and every step skipped with a reason naming the source
    /// and ceiling, which the summary carries too.
    ///
    /// # Panics
    /// Panics when the outcome judges anything or the reason is not exposed.
    #[test]
    fn source_ceiling_is_inconclusive_and_named() {
        let spec: VerifierSpec = serde_json::from_value(serde_json::json!({
            "inputs": { "frames": "verifiers.frame.requirements.safe.outcome" },
            "requirements": [{
                "id": "all_safe", "statement": "every frame is safe",
                "check": { "kind": "assert", "path": "inputs.frames[*]",
                           "operator": "equals", "expected": "passed" }
            }]
        }))
        .expect("group verifier decodes");
        let reference: SourceText = serde_json::from_value(serde_json::json!(
            "verifiers.frame.requirements.safe.outcome"
        ))
        .expect("source decodes");
        let outcome = source_limit_exceeded(&spec, &reference, 20);
        assert_eq!(outcome.verdict, VerificationVerdict::Inconclusive);
        let reason = "source_execution_limit_exceeded: verifiers.frame.requirements.safe.outcome \
                      selected more than 20 source executions";
        assert!(outcome.requirements.iter().all(|requirement| {
            requirement.outcome == RequirementOutcome::Inconclusive && requirement.reason == reason
        }));
        assert_eq!(outcome.requirements.len(), 1);
        assert!(summary(&outcome).ends_with(reason));
    }
}
