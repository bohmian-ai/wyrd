//! PSI formula with epsilon smoothing.
//!
//! ## Formula
//!
//! ```text
//! PSI = sum_i (p_i + epsilon - q_i - epsilon) * ln((p_i + epsilon) / (q_i + epsilon))
//! ```
//!
//! where `p_i` is the baseline proportion in bin `i`, `q_i` is the target
//! proportion in bin `i`, and `epsilon = 1e-10` is the smoothing constant.
//! Epsilon is applied symmetrically so the result is finite even when a bin has
//! zero count in either set.

/// Epsilon smoothing constant.
pub const PSI_EPSILON: f64 = 1e-10;
/// Minimum number of non-null target rows per feature for a stable PSI score.
pub const PSI_MIN_TARGET_SAMPLE: u64 = 100;

/// Compute PSI given two equal-length proportion vectors.
///
/// # Panics
/// In debug builds, panics if the vectors have different lengths. In release
/// builds, returns `NaN` instead; public scoring verifies length before calling.
pub fn psi(baseline_proportions: &[f64], target_proportions: &[f64]) -> f64 {
    debug_assert_eq!(
        baseline_proportions.len(),
        target_proportions.len(),
        "psi: proportion vectors must be equal length"
    );
    if baseline_proportions.len() != target_proportions.len() {
        return f64::NAN;
    }

    let mut sum = 0.0;
    for (p, q) in baseline_proportions.iter().zip(target_proportions.iter()) {
        let pe = p + PSI_EPSILON;
        let qe = q + PSI_EPSILON;
        sum += (pe - qe) * (pe / qe).ln();
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psi_identical_distributions_is_zero() {
        let proportions = vec![0.25, 0.25, 0.25, 0.25];
        assert!(psi(&proportions, &proportions).abs() < 1e-9);
    }

    #[test]
    fn psi_shifted_distribution_is_positive() {
        let baseline = vec![0.4, 0.3, 0.2, 0.1];
        let target = vec![0.1, 0.2, 0.3, 0.4];
        let score = psi(&baseline, &target);
        assert!(score > 0.1, "PSI = {score}");
    }

    #[test]
    fn psi_handles_zero_bin_with_epsilon() {
        let baseline = vec![0.5, 0.5];
        let target = vec![1.0, 0.0];
        let score = psi(&baseline, &target);
        assert!(score.is_finite() && score > 0.0);
    }
}
