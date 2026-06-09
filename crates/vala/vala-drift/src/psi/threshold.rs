//! PSI threshold strategies.

use statrs::distribution::{ChiSquared, ContinuousCDF, Normal};
use wyrd_spec::card::drift::PsiThreshold;

use crate::error::DriftScoreError;

/// Compute the per-feature PSI threshold for a threshold strategy.
pub fn compute_psi_threshold(
    strategy: &PsiThreshold,
    n_bins: usize,
    target_sample_size: u64,
) -> Result<f64, DriftScoreError> {
    if n_bins < 2 {
        return Err(DriftScoreError::ThresholdFailure {
            message: "n_bins must be >= 2".to_string(),
        });
    }
    if target_sample_size == 0 {
        return Err(DriftScoreError::ThresholdFailure {
            message: "target_sample_size must be > 0".to_string(),
        });
    }

    let m = target_sample_size as f64;
    let df = (n_bins - 1) as f64;
    match strategy {
        PsiThreshold::Fixed { value } => Ok(*value),
        PsiThreshold::ChiSquare { alpha } => {
            let dist = ChiSquared::new(df).map_err(|err| DriftScoreError::ThresholdFailure {
                message: format!("ChiSquared::new({df}): {err}"),
            })?;
            Ok(dist.inverse_cdf(1.0 - alpha) / m)
        }
        PsiThreshold::Normal { alpha } => {
            let dist = Normal::new(0.0, 1.0).map_err(|err| DriftScoreError::ThresholdFailure {
                message: format!("Normal::new(0, 1): {err}"),
            })?;
            let z = dist.inverse_cdf(1.0 - alpha);
            Ok(df / m + z * (2.0 * df).sqrt() / m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_returns_value() {
        let threshold = compute_psi_threshold(&PsiThreshold::Fixed { value: 0.25 }, 10, 1_000)
            .expect("fixed threshold");
        assert_eq!(threshold, 0.25);
    }

    #[test]
    fn chisquare_default_alpha_is_positive_finite() {
        let threshold = compute_psi_threshold(&PsiThreshold::ChiSquare { alpha: 0.05 }, 10, 1_000)
            .expect("chi-square threshold");
        assert!(threshold.is_finite() && threshold > 0.0 && threshold < 1.0);
    }

    #[test]
    fn normal_default_alpha_is_positive_finite() {
        let threshold = compute_psi_threshold(&PsiThreshold::Normal { alpha: 0.05 }, 10, 1_000)
            .expect("normal threshold");
        assert!(threshold.is_finite() && threshold > 0.0);
    }

    #[test]
    fn rejects_n_bins_below_two() {
        let err = compute_psi_threshold(&PsiThreshold::ChiSquare { alpha: 0.05 }, 1, 1_000)
            .expect_err("threshold error");
        assert!(matches!(err, DriftScoreError::ThresholdFailure { .. }));
    }

    #[test]
    fn rejects_zero_target_sample() {
        let err = compute_psi_threshold(&PsiThreshold::ChiSquare { alpha: 0.05 }, 5, 0)
            .expect_err("threshold error");
        assert!(matches!(err, DriftScoreError::ThresholdFailure { .. }));
    }
}
