//! Top-level baseline fit and score dispatch.

use wyrd_spec::card::drift::{DriftMethod, DriftProfile, DriftSignal, DriftSpec};
use wyrd_spec::ids::FeatureName;

use crate::custom::score_custom;
use crate::error::{DriftFitError, DriftScoreError};
use crate::psi::PsiBaseline;
use crate::psi::{fit_psi_baseline, score_psi};
use crate::report::DriftReport;
use crate::spc::SpcBaseline;
use crate::spc::{fit_spc_baseline, score_spc};

/// Fitted baseline state produced by `fit_baseline`.
#[derive(Debug, Clone)]
pub enum FittedBaseline {
    /// PSI fitted baseline state.
    Psi(PsiBaseline),
    /// SPC fitted baseline state.
    Spc(SpcBaseline),
    /// Custom carries no state because there is no fit phase.
    Custom,
}

/// Fit a method-specific baseline from an Arrow `RecordBatch`.
///
/// # Errors
/// Returns a [`DriftFitError`] when the `DriftSpec` does not carry the signal
/// and profile shape required by the selected method, or when the method-level
/// fit fails.
pub fn fit_baseline(
    batch: &arrow::record_batch::RecordBatch,
    spec: &DriftSpec,
) -> Result<FittedBaseline, crate::DriftFitError> {
    match spec.method {
        DriftMethod::Psi => {
            let features = match &spec.signal {
                DriftSignal::Distribution { features, .. } => features.as_slice(),
                other => {
                    return Err(DriftFitError::SignalShapeMismatch {
                        got: signal_variant(other),
                    });
                }
            };
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Psi(profile)) => profile,
                _ => {
                    return Err(DriftFitError::PsiInternal {
                        message: "PSI method requires DriftProfile::Psi".to_string(),
                    });
                }
            };
            fit_psi_baseline(batch, profile, features).map(FittedBaseline::Psi)
        }
        DriftMethod::Spc => {
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Spc(profile)) => profile,
                _ => {
                    return Err(DriftFitError::SpcInternal {
                        message: "SPC method requires DriftProfile::Spc".to_string(),
                    });
                }
            };
            let metric_feature;
            let features = match &spec.signal {
                DriftSignal::Distribution { features, .. } => features.as_slice(),
                DriftSignal::Metric { name } => {
                    metric_feature = FeatureName::new(name.as_str()).map_err(|_| {
                        DriftFitError::SpcInternal {
                            message: format!("metric name {name} is not a valid FeatureName"),
                        }
                    })?;
                    std::slice::from_ref(&metric_feature)
                }
                other => {
                    return Err(DriftFitError::SignalShapeMismatch {
                        got: signal_variant(other),
                    });
                }
            };
            fit_spc_baseline(batch, profile, features).map(FittedBaseline::Spc)
        }
        DriftMethod::Custom => Ok(FittedBaseline::Custom),
        DriftMethod::External => Err(DriftFitError::ExternalMethodHasNoBaseline),
    }
}

/// Score a target Arrow `RecordBatch` against a fitted baseline.
///
/// # Errors
/// Returns a [`DriftScoreError`] when the fitted baseline does not match the
/// selected method, the method profile is missing or mismatched, scoring fails,
/// or the spec selects the out-of-phase External method.
pub fn score(
    baseline: &FittedBaseline,
    target: &arrow::record_batch::RecordBatch,
    spec: &DriftSpec,
) -> Result<DriftReport, crate::DriftScoreError> {
    match spec.method {
        DriftMethod::Psi => {
            let FittedBaseline::Psi(baseline) = baseline else {
                return Err(DriftScoreError::PsiInternal {
                    message: "PSI method requires FittedBaseline::Psi".to_string(),
                });
            };
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Psi(profile)) => profile,
                _ => {
                    return Err(DriftScoreError::PsiInternal {
                        message: "PSI method requires DriftProfile::Psi".to_string(),
                    });
                }
            };
            score_psi(baseline, target, profile)
        }
        DriftMethod::Spc => {
            let FittedBaseline::Spc(baseline) = baseline else {
                return Err(DriftScoreError::SpcInternal {
                    message: "SPC method requires FittedBaseline::Spc".to_string(),
                });
            };
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Spc(profile)) => profile,
                _ => {
                    return Err(DriftScoreError::SpcInternal {
                        message: "SPC method requires DriftProfile::Spc".to_string(),
                    });
                }
            };
            score_spc(baseline, target, profile)
        }
        DriftMethod::Custom => {
            if !matches!(baseline, FittedBaseline::Custom) {
                return Err(DriftScoreError::CustomInternal {
                    message: "Custom method requires FittedBaseline::Custom".to_string(),
                });
            }
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Custom(profile)) => profile,
                _ => {
                    return Err(DriftScoreError::CustomInternal {
                        message: "Custom method requires DriftProfile::Custom".to_string(),
                    });
                }
            };
            score_custom(target, profile)
        }
        DriftMethod::External => Err(DriftScoreError::ExternalMethodNotInPhase),
    }
}

fn signal_variant(signal: &DriftSignal) -> &'static str {
    match signal {
        DriftSignal::Distribution { .. } => "Distribution",
        DriftSignal::Metric { .. } => "Metric",
        DriftSignal::EvalScore { .. } => "EvalScore",
        DriftSignal::External { .. } => "External",
    }
}
