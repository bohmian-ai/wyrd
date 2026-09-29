//! The closed Verifier implementation arms and the typed outcomes they return.
//!
//! [`VerifierRunner`](super::runner::VerifierRunner) owns the one dispatch over
//! [`VerifierImplementation`]: each arm receives the claimed run and its typed
//! implementation spec and returns an [`EngineOutcome`]. Arms never read or
//! write run rows, claim work, publish results, or create dispatches; the
//! runner turns every outcome into exactly one fenced lifecycle transition.
//! Drift runs through [`DriftEngine`](super::drift::DriftEngine); Eval lands
//! by replacing the body of [`eval`] only.

use std::future::Future;

use vala_drift::{DriftReport, DriftVerdict};
use vala_eval::executor::EvalReport;
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::verification::{VerificationError, VerificationVerdict};
use wyrd_sql::queries::verifier_runs::{ClaimedRun, TerminalStatus};

/// Stable error code of an implementation whose engine has not shipped.
pub const IMPLEMENTATION_UNAVAILABLE: &str = "implementation_unavailable";

/// What one completed Verifier execution produced, before publication.
///
/// Carries the existing engine report types unchanged so result publication
/// maps them to the analytical tables without an intermediate copy.
#[derive(Debug, Clone, PartialEq)]
pub enum VerifierReport {
    /// A Drift execution; `None` when valid input could not be scored, which
    /// is recorded as an inconclusive result rather than a fabricated report.
    Drift(Option<DriftReport>),
    /// An Eval workflow execution over one committed record.
    Eval {
        /// Every task outcome, both executed and skipped; empty when sampled out.
        report: EvalReport,
        /// The workflow verdict the Eval engine's pass gate decided.
        verdict: VerificationVerdict,
    },
}

impl VerifierReport {
    /// The `implementation` discriminator the result row records.
    #[must_use]
    pub const fn implementation(&self) -> &'static str {
        match self {
            Self::Drift(_) => "drift",
            Self::Eval { .. } => "eval",
        }
    }

    /// The common verdict of this result.
    ///
    /// Drift maps its own verdict (`no_drift` passes, `drift` fails,
    /// `inconclusive` stays inconclusive) and an unscored execution is
    /// inconclusive; Eval reports the verdict its pass gate decided.
    #[must_use]
    pub const fn verdict(&self) -> VerificationVerdict {
        match self {
            Self::Drift(None) => VerificationVerdict::Inconclusive,
            Self::Drift(Some(report)) => match report.verdict {
                DriftVerdict::NoDrift => VerificationVerdict::Passed,
                DriftVerdict::Drift => VerificationVerdict::Failed,
                DriftVerdict::Inconclusive => VerificationVerdict::Inconclusive,
            },
            Self::Eval { verdict, .. } => *verdict,
        }
    }
}

/// The typed result of one engine execution.
///
/// The runner maps each variant to exactly one transition: `Completed`
/// publishes and then completes, `Retry` reschedules the same run and input
/// within its attempt budget, and `Terminal` settles without a verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineOutcome {
    /// The engine produced a verdict and its report.
    Completed(VerifierReport),
    /// A transient failure; the same run and frozen input may be attempted again.
    Retry(VerificationError),
    /// A terminal failure that carries no verdict.
    Terminal(TerminalStatus, VerificationError),
}

impl EngineOutcome {
    /// The terminal outcome of an implementation whose engine is not available.
    ///
    /// Never a verdict: the run settles `errored` with
    /// [`IMPLEMENTATION_UNAVAILABLE`] so nothing downstream mistakes it for a
    /// judgment.
    #[must_use]
    pub fn implementation_unavailable(implementation: &str) -> Self {
        Self::Terminal(
            TerminalStatus::Errored,
            VerificationError {
                code: IMPLEMENTATION_UNAVAILABLE.to_owned(),
                message: format!("the {implementation} Verifier engine is not available"),
            },
        )
    }
}

/// Eval arm of the closed Verifier dispatch.
///
/// Receives the claimed run (its frozen input record) and the exact
/// Verifier's typed Eval spec. Until the continuous Eval engine ships this
/// returns the terminal `implementation_unavailable` outcome; it never
/// fabricates a verdict.
pub fn eval(
    _run: &ClaimedRun,
    _spec: &EvalSpec,
) -> impl Future<Output = EngineOutcome> + Send + 'static {
    std::future::ready(EngineOutcome::implementation_unavailable("eval"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wyrd_spec::card::drift::DriftMethod;

    use super::*;

    /// Build a Drift report with `verdict` and no features.
    fn drift_report(verdict: DriftVerdict) -> DriftReport {
        DriftReport {
            method: DriftMethod::Custom,
            features: BTreeMap::new(),
            verdict,
        }
    }

    /// Drift verdicts map onto the common verdict and an unscored run is inconclusive.
    #[test]
    fn drift_verdicts_map_to_the_common_verdict() {
        let cases = [
            (Some(DriftVerdict::NoDrift), VerificationVerdict::Passed),
            (Some(DriftVerdict::Drift), VerificationVerdict::Failed),
            (
                Some(DriftVerdict::Inconclusive),
                VerificationVerdict::Inconclusive,
            ),
            (None, VerificationVerdict::Inconclusive),
        ];
        for (drift, expected) in cases {
            let report = VerifierReport::Drift(drift.map(drift_report));
            assert_eq!(report.verdict(), expected, "{drift:?}");
            assert_eq!(report.implementation(), "drift");
        }
    }

    /// The unavailable outcome is a terminal `errored` failure, never a verdict.
    #[test]
    fn unavailable_implementation_is_terminal_without_verdict() {
        let EngineOutcome::Terminal(status, error) =
            EngineOutcome::implementation_unavailable("drift")
        else {
            panic!("an unavailable implementation must be terminal");
        };
        assert_eq!(status, TerminalStatus::Errored);
        assert_eq!(error.code, IMPLEMENTATION_UNAVAILABLE);
    }
}
