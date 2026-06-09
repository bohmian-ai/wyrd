//! PSI threshold strategies. Body lands in Commit 6.

use wyrd_spec::card::drift::PsiThreshold;

use crate::error::DriftScoreError;

pub fn compute_psi_threshold(
    _strategy: &PsiThreshold,
    _n_bins: usize,
    _target_sample_size: u64,
) -> Result<f64, DriftScoreError> {
    unimplemented!("Commit 6")
}
