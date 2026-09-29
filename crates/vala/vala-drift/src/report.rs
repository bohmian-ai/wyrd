use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::ids::FeatureName;

use crate::psi::PsiEvidence;
use crate::spc::SpcEvidence;

/// Overall verdict for a feature or for the whole report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftVerdict {
    /// Score did not exceed threshold.
    NoDrift,
    /// Score exceeded threshold.
    Drift,
    /// Insufficient target data, so the verdict cannot be computed.
    Inconclusive,
}

/// One drift report per call to `score` or per method-dispatched score function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriftReport {
    /// Drift method used for the report.
    pub method: DriftMethod,
    /// Per-feature drift reports.
    pub features: BTreeMap<FeatureName, FeatureDriftReport>,
    /// Aggregated top-level verdict.
    pub verdict: DriftVerdict,
}

/// Per-feature drift report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureDriftReport {
    /// Feature this report describes.
    pub feature: FeatureName,
    /// PSI score, total SPC chart signals, or absolute Custom delta. NaN for Inconclusive.
    pub score: f64,
    /// Threshold the score was compared against: the PSI threshold, zero for
    /// SPC, or the Custom alert threshold. NaN for Inconclusive.
    pub threshold: f64,
    /// Feature-level verdict.
    pub verdict: DriftVerdict,
    /// Method evidence explaining a scored PSI or SPC feature.
    ///
    /// Absent for Custom, for an inconclusive feature, and in reports
    /// persisted before this evidence existed, which still deserialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<FeatureEvidence>,
}

/// Typed evidence behind one scored feature, by method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureEvidence {
    /// The frozen bins with baseline and target counts behind a PSI score.
    Psi(PsiEvidence),
    /// The X-bar and S chart limits and signals behind an SPC score.
    Spc(SpcEvidence),
}

impl DriftReport {
    /// The report of a target that could not be scored: inconclusive with no
    /// feature details.
    ///
    /// Returned when a selected observation lacks a configured feature or
    /// holds a null or non-finite value, so no value is dropped or imputed to
    /// make the window scorable.
    #[must_use]
    pub fn unscored(method: DriftMethod) -> Self {
        Self {
            method,
            features: BTreeMap::new(),
            verdict: DriftVerdict::Inconclusive,
        }
    }

    /// Aggregate a per-feature verdict map into the report-level verdict.
    ///
    /// Returns `Drift` if any feature is `Drift`, else `Inconclusive` if any
    /// feature is `Inconclusive`, else `NoDrift`.
    pub fn aggregate_verdict(features: &BTreeMap<FeatureName, FeatureDriftReport>) -> DriftVerdict {
        let mut any_drift = false;
        let mut any_inconclusive = false;
        for r in features.values() {
            match r.verdict {
                DriftVerdict::Drift => any_drift = true,
                DriftVerdict::Inconclusive => any_inconclusive = true,
                DriftVerdict::NoDrift => {}
            }
        }
        if any_drift {
            DriftVerdict::Drift
        } else if any_inconclusive {
            DriftVerdict::Inconclusive
        } else {
            DriftVerdict::NoDrift
        }
    }
}
