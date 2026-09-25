//! In-memory drift baseline fit and scoring for Wyrd's drift Verifier implementation.
//!
//! This crate consumes Arrow `RecordBatch`es directly. Parquet I/O, DataCard
//! resolution, persistence, scheduling, and HTTP wiring live in other crates.
//!
//! ## Public surface
//!
//! - `fit_psi_baseline` / `score_psi` - Population Stability Index over a feature distribution.
//! - `fit_spc_baseline` / `score_spc` - Shewhart X-bar/S control charts over fixed rational subgroups.
//! - `score_custom` - User-defined scalar drift against an author-supplied baseline value.
//! - `fit_baseline` / `score` - Method-dispatching helpers that read a `DriftSpec`.
//!
//! ## Error model
//!
//! Two phase-specific error enums: [`DriftFitError`] for baseline construction and
//! [`DriftScoreError`] for target scoring. Both are crate-local `thiserror` enums
//! and do not participate in the `WyrdError` derive (no HTTP surface this phase).

pub mod baseline;
pub(crate) mod custom;
pub mod error;
pub(crate) mod feature;
pub mod psi;
pub mod report;
pub mod spc;

pub use baseline::{FITTED_FORMAT, FittedBaseline, fit_baseline, fit_baseline_until, score_drift};
pub use custom::{score_custom, score_custom_mean};
pub use error::{DriftFitError, DriftScoreError};
pub use psi::{
    FittedPsiFeature, PSI_MIN_TARGET_SAMPLE, PsiBaseline, PsiBinEvidence, PsiEvidence,
    fit_psi_baseline, score_psi, score_psi_counts,
};
pub use report::{DriftReport, DriftVerdict, FeatureDriftReport, FeatureEvidence};
pub use spc::{
    ChartLimits, FittedSpcFeature, MIN_BASELINE_SUBGROUPS, SpcBaseline, SpcChartEvidence,
    SpcEvidence, SpcScorer, fit_spc_baseline, score_spc,
};

#[cfg(test)]
mod smoke {
    //! Confirms the crate compiles and exposes its declared public surface.

    use crate::{
        DriftFitError, DriftReport, DriftScoreError, DriftVerdict, FeatureDriftReport,
        FittedBaseline, FittedPsiFeature, FittedSpcFeature, PsiBaseline, SpcBaseline, fit_baseline,
        fit_psi_baseline, fit_spc_baseline, score_custom, score_drift, score_psi, score_spc,
    };
    use arrow::record_batch::RecordBatch;
    use wyrd_spec::card::drift::{CustomProfile, DriftSpec, PsiProfile, SpcProfile};
    use wyrd_spec::ids::FeatureName;

    /// Every public re-export stays nameable from the crate root.
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
        let _fit: fn(
            &RecordBatch,
            &PsiProfile,
            &[FeatureName],
        ) -> Result<PsiBaseline, DriftFitError> = fit_psi_baseline;
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
        let _score_spc: fn(&SpcBaseline, &RecordBatch) -> Result<DriftReport, DriftScoreError> =
            score_spc;
        let _score_custom_fn: fn(
            &RecordBatch,
            &CustomProfile,
        ) -> Result<DriftReport, DriftScoreError> = score_custom;
        let _fit_dispatch: fn(&RecordBatch, &DriftSpec) -> Result<FittedBaseline, DriftFitError> =
            fit_baseline;
        let _score_dispatch: fn(
            &FittedBaseline,
            &RecordBatch,
            &DriftSpec,
        ) -> Result<DriftReport, DriftScoreError> = score_drift;
    }
}
