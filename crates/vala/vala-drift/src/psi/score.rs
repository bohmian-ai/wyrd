//! PSI formula and smoothing constant. Body lands in Commit 6.

pub const PSI_EPSILON: f64 = 1e-10;
pub const PSI_MIN_TARGET_SAMPLE: u64 = 100;

pub fn psi(_baseline_proportions: &[f64], _target_proportions: &[f64]) -> f64 {
    unimplemented!("Commit 6")
}
