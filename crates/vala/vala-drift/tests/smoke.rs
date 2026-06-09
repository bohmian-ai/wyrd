//! Confirms the crate compiles and exposes its declared public surface.

use arrow::record_batch::RecordBatch;
use vala_drift::{
    DriftFitError, DriftReport, DriftScoreError, DriftVerdict, FeatureDriftReport, FittedBaseline,
    FittedPsiFeature, FittedSpcFeature, PsiBaseline, SpcBaseline, fit_baseline, fit_psi_baseline,
    fit_spc_baseline, score, score_custom, score_psi, score_spc,
};
use wyrd_spec::card::drift::{CustomProfile, DriftSpec, PsiProfile, SpcProfile};
use wyrd_spec::ids::FeatureName;

#[test]
fn public_surface_is_visible() {
    // No-op: this test exists to force a link of every public re-export. If
    // any identifier above moves or disappears, this file fails to compile.
    let _ = std::any::type_name::<DriftReport>();
    let _ = std::any::type_name::<DriftVerdict>();
    let _ = std::any::type_name::<FeatureDriftReport>();
    let _ = std::any::type_name::<PsiBaseline>();
    let _ = std::any::type_name::<FittedPsiFeature>();
    let _ = std::any::type_name::<SpcBaseline>();
    let _ = std::any::type_name::<FittedSpcFeature>();
    let _ = std::any::type_name::<FittedBaseline>();
    let _ = std::any::type_name::<DriftFitError>();
    let _ = std::any::type_name::<DriftScoreError>();
    let _fit: fn(&RecordBatch, &PsiProfile, &[FeatureName]) -> Result<PsiBaseline, DriftFitError> =
        fit_psi_baseline;
    let _score: fn(
        &PsiBaseline,
        &RecordBatch,
        &PsiProfile,
    ) -> Result<DriftReport, DriftScoreError> = score_psi;
    let _fit_spc: fn(
        &RecordBatch,
        &SpcProfile,
        &[FeatureName],
    ) -> Result<SpcBaseline, DriftFitError> = fit_spc_baseline;
    let _score_spc: fn(
        &SpcBaseline,
        &RecordBatch,
        &SpcProfile,
    ) -> Result<DriftReport, DriftScoreError> = score_spc;
    let _score_custom_fn: fn(&RecordBatch, &CustomProfile) -> Result<DriftReport, DriftScoreError> =
        score_custom;
    let _fit_dispatch: fn(&RecordBatch, &DriftSpec) -> Result<FittedBaseline, DriftFitError> =
        fit_baseline;
    let _score_dispatch: fn(
        &FittedBaseline,
        &RecordBatch,
        &DriftSpec,
    ) -> Result<DriftReport, DriftScoreError> = score;
}
