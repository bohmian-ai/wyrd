//! The typed outcome of one queued Verifier execution.
//!
//! [`VerifierRunner`](super::runner::VerifierRunner) runs every claimed
//! execution through the one building-block engine and maps its result to an
//! [`EngineOutcome`]; the runner turns every outcome into exactly one fenced
//! lifecycle transition. The helpers here derive the bounded Operator failure
//! context from a completed [`VerificationOutcome`].

use wyrd_spec::card::operator::DriftCounts;
use wyrd_spec::verification::execution::{
    RequirementDetail, RequirementOutcome, VerificationOutcome,
};
use wyrd_spec::verification::{DriftVerdict, VerificationError};
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
/// or step output. Settlement clips it to the Operator summary bound.
#[must_use]
pub fn summary(outcome: &VerificationOutcome) -> String {
    let failed = outcome
        .requirements
        .iter()
        .filter(|requirement| requirement.outcome == RequirementOutcome::Failed)
        .count();
    format!(
        "Verdict {}: {failed} of {} requirements failed.",
        <&str>::from(outcome.verdict),
        outcome.requirements.len()
    )
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
}
