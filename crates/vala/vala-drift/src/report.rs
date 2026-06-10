use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::ids::FeatureName;

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
    /// PSI score, SPC violation count, or absolute Custom delta. NaN for Inconclusive.
    pub score: f64,
    /// Threshold the score was compared against. NaN for Inconclusive or method-driven verdicts.
    pub threshold: f64,
    /// Feature-level verdict.
    pub verdict: DriftVerdict,
}

impl DriftReport {
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
