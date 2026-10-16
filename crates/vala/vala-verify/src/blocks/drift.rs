//! Requirement-level adapter from Verifier `drift` checks to `vala-drift`.
//!
//! The statistical engines, fitted baselines, minimum-sample rules, and
//! insufficient-data behavior stay owned by `vala-drift`; this adapter only
//! selects the requirement's own fitted state and its slot's samples, scores
//! them with [`vala_drift::score_drift`], and projects the report onto the
//! requirement result. Reading samples and fitted state is IO and stays with
//! the caller, which builds one [`DriftChecks`] per execution.

use std::collections::BTreeMap;

use arrow::record_batch::RecordBatch;
use vala_drift::{DriftReport, DriftVerdict, FittedBaseline, score_drift};
use wyrd_spec::card::drift::{DriftMethod, DriftProfile};
use wyrd_spec::card::verifier::DriftCheck;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{CallProvenance, RequirementDetail, Verdict};
use wyrd_spec::verification::{DriftWindow, VerificationVerdict};

use super::CheckOutput;

/// The resolved scheduled evidence of one execution's drift checks.
#[derive(Debug, Clone, Default)]
pub struct DriftChecks {
    /// The scored sample window, recorded as each check's provenance.
    window: Option<DriftWindow>,
    /// Fitted state keyed by the globally unique requirement ID.
    baselines: BTreeMap<TaskId, FittedBaseline>,
    /// Window samples keyed by samples slot.
    samples: BTreeMap<SlotName, RecordBatch>,
    /// Reports a caller already scored from server-side aggregates, keyed by
    /// requirement; they take precedence over samples and fitted state.
    reports: BTreeMap<TaskId, DriftReport>,
}

impl DriftChecks {
    /// Evidence for one window: fitted state per requirement and samples per
    /// slot. A requirement without fitted state is unready, except Custom,
    /// which has no fit phase.
    #[must_use]
    pub fn new(
        window: Option<DriftWindow>,
        baselines: BTreeMap<TaskId, FittedBaseline>,
        samples: BTreeMap<SlotName, RecordBatch>,
    ) -> Self {
        Self {
            window,
            baselines,
            samples,
            reports: BTreeMap::new(),
        }
    }

    /// Judge `requirement` from `report`, scored by the caller from
    /// server-side window aggregates instead of raw samples.
    ///
    /// The caller owns reading, completeness, and scoring through
    /// `vala-drift`; an unscorable window is [`DriftReport::unscored`].
    #[must_use]
    pub fn with_report(mut self, requirement: TaskId, report: DriftReport) -> Self {
        self.reports.insert(requirement, report);
        self
    }

    /// Judge one drift requirement.
    ///
    /// A caller-scored report of the requirement maps directly.
    /// Missing samples, an unready baseline, and any scoring refusal are
    /// inconclusive with the cause as reason; a scored report maps `NoDrift`
    /// to passed, `Drift` to failed, and `Inconclusive` to inconclusive.
    /// A Custom window with a null measurement of its configured metric is
    /// the unscored report, since the Custom scorer would otherwise average
    /// only the non-null rows; nulls in other columns do not matter.
    #[must_use]
    pub fn judge(&self, requirement: &TaskId, check: &DriftCheck) -> CheckOutput {
        if let Some(report) = self.reports.get(requirement) {
            return self.scored(report);
        }
        let Some(samples) = check.on.slot().and_then(|slot| self.samples.get(slot)) else {
            return self.inconclusive(format!("no samples were recorded for `{}`", check.on));
        };
        if let Some(DriftProfile::Custom(profile)) = &check.spec.profile
            && samples
                .column_by_name(&profile.metric_name)
                .is_some_and(|column| column.null_count() > 0)
        {
            return self.scored(&DriftReport::unscored(check.spec.method));
        }
        let custom = FittedBaseline::Custom;
        let baseline = match check.spec.method {
            DriftMethod::Custom => &custom,
            DriftMethod::Psi | DriftMethod::Spc => match self.baselines.get(requirement) {
                Some(baseline) => baseline,
                None => {
                    return self.inconclusive(format!(
                        "the fitted baseline of `{}` is not ready",
                        requirement.as_str()
                    ));
                }
            },
        };
        match score_drift(baseline, samples, &check.spec) {
            Ok(report) => self.scored(&report),
            Err(error) => self.inconclusive(error.to_string()),
        }
    }

    /// Project a scored report, keeping its full statistics as detail.
    ///
    /// Callers that score outside [`DriftChecks::judge`], such as a
    /// pre-aggregated window read, use this to produce the same result.
    #[must_use]
    pub fn scored(&self, report: &DriftReport) -> CheckOutput {
        // The wire report mirrors the engine report; a non-finite score of an
        // inconclusive feature becomes an absent value.
        let Ok(wire) = serde_json::to_value(report).and_then(serde_json::from_value) else {
            return self.inconclusive("the drift report cannot be encoded".to_owned());
        };
        let drifted = report
            .features
            .values()
            .filter(|feature| feature.verdict == DriftVerdict::Drift)
            .count();
        let outcome = match report.verdict {
            DriftVerdict::NoDrift => VerificationVerdict::Passed,
            DriftVerdict::Drift => VerificationVerdict::Failed,
            DriftVerdict::Inconclusive => VerificationVerdict::Inconclusive,
        };
        CheckOutput {
            verdict: Verdict {
                outcome,
                reason: format!(
                    "{drifted} of {} features drifted ({:?})",
                    report.features.len(),
                    report.verdict
                ),
                citations: Vec::new(),
            },
            actual: None,
            detail: Some(RequirementDetail::Drift(wire)),
            provenance: Some(self.provenance()),
        }
    }

    /// An inconclusive drift result with `reason` and no statistics.
    fn inconclusive(&self, reason: String) -> CheckOutput {
        CheckOutput {
            verdict: Verdict {
                outcome: VerificationVerdict::Inconclusive,
                reason,
                citations: Vec::new(),
            },
            actual: None,
            detail: None,
            provenance: Some(self.provenance()),
        }
    }

    /// The drift provenance naming the scored window.
    fn provenance(&self) -> CallProvenance {
        CallProvenance::Drift {
            window: self.window,
        }
    }
}
