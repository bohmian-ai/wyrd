//! SPC control limits: c4 bias correction, adaptive sample-size table,
//! and fit_control_limits (X-bar/S chart per NIST section 3.2.1).

use statrs::function::gamma::ln_gamma;

use crate::error::DriftFitError;

/// Seven control limits per feature, ordered:
/// `three_lcl < two_lcl < one_lcl < center < one_ucl < two_ucl < three_ucl`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlLimits {
    pub center: f64,
    pub one_lcl: f64,
    pub one_ucl: f64,
    pub two_lcl: f64,
    pub two_ucl: f64,
    pub three_lcl: f64,
    pub three_ucl: f64,
}

/// c4 unbiased-stddev correction factor (exact Gamma-based formula).
///
/// `c4(n) = sqrt(2/(n-1)) · Γ(n/2) / Γ((n-1)/2)`
///
/// Computed in log-space via `ln_gamma` for numerical stability across all
/// chunk sizes. References: NIST e-Handbook §6.3.2; Montgomery "Introduction
/// to Statistical Quality Control" Table VI.
#[must_use]
pub fn c4(n: u32) -> f64 {
    let nf = f64::from(n);
    (0.5 * (2.0 / (nf - 1.0)).ln() + ln_gamma(nf / 2.0) - ln_gamma((nf - 1.0) / 2.0)).exp()
}

/// Adaptive sample chunk size from total baseline row count.
///
/// | row_count range  | chunk_size |
/// |------------------|------------|
/// | < 1_000          | 25         |
/// | < 10_000         | 100        |
/// | < 100_000        | 1_000      |
/// | < 1_000_000      | 10_000     |
/// | >= 1_000_000     | 100_000    |
#[must_use]
pub fn adaptive_sample_size(row_count: usize) -> u32 {
    if row_count < 1_000 {
        25
    } else if row_count < 10_000 {
        100
    } else if row_count < 100_000 {
        1_000
    } else if row_count < 1_000_000 {
        10_000
    } else {
        100_000
    }
}

/// Compute seven control limits via the X-bar/S chart fit.
///
/// Values are partitioned into chunks of size `chunk_size`. Trailing partial
/// chunks are included for chunk means. The stddev pass also includes trailing
/// partial chunks when they have at least two values; len-1 chunks are skipped
/// because sample stddev uses `ddof = 1`.
///
/// # Errors
///
/// Returns [`DriftFitError::InsufficientSamplesForChunk`] when there are not
/// enough values or at least two stddev-bearing chunks are unavailable. Returns
/// [`DriftFitError::SpcInternal`] for malformed chunk sizes or non-finite
/// inputs/results.
pub fn fit_control_limits(values: &[f64], chunk_size: u32) -> Result<ControlLimits, DriftFitError> {
    let cs = chunk_size as usize;
    if cs < 2 {
        return Err(DriftFitError::SpcInternal {
            message: "chunk_size must be >= 2".into(),
        });
    }
    if values.len() < cs {
        return Err(DriftFitError::InsufficientSamplesForChunk {
            feature: String::new(),
            rows: values.len(),
            chunk_size: cs,
        });
    }
    if !values.iter().all(|value| value.is_finite()) {
        return Err(DriftFitError::SpcInternal {
            message: "non-finite values in column".into(),
        });
    }

    let mut chunk_means = Vec::new();
    let mut chunk_stddevs = Vec::new();
    for chunk in values.chunks(cs) {
        let n = chunk.len();
        let mean = chunk.iter().sum::<f64>() / n as f64;
        chunk_means.push(mean);
        if n >= 2 {
            let variance = chunk
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (n as f64 - 1.0);
            chunk_stddevs.push(variance.sqrt());
        }
    }
    if chunk_means.len() < 2 || chunk_stddevs.len() < 2 {
        return Err(DriftFitError::InsufficientSamplesForChunk {
            feature: String::new(),
            rows: values.len(),
            chunk_size: cs,
        });
    }

    let center = chunk_means.iter().sum::<f64>() / chunk_means.len() as f64;
    let s_bar = chunk_stddevs.iter().sum::<f64>() / chunk_stddevs.len() as f64;
    let c4_value = c4(chunk_size);
    if c4_value == 0.0 {
        return Err(DriftFitError::SpcInternal {
            message: "c4 evaluated to zero".into(),
        });
    }
    let stddev_adj = s_bar / c4_value;

    let limits = ControlLimits {
        center,
        one_lcl: center - stddev_adj,
        one_ucl: center + stddev_adj,
        two_lcl: center - 2.0 * stddev_adj,
        two_ucl: center + 2.0 * stddev_adj,
        three_lcl: center - 3.0 * stddev_adj,
        three_ucl: center + 3.0 * stddev_adj,
    };
    if [
        limits.center,
        limits.one_lcl,
        limits.one_ucl,
        limits.two_lcl,
        limits.two_ucl,
        limits.three_lcl,
        limits.three_ucl,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(DriftFitError::SpcInternal {
            message: "non-finite control limit".into(),
        });
    }
    Ok(limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c4_n2_equals_sqrt_2_over_pi() {
        // Exact closed form: sqrt(2) * Γ(1) / Γ(1/2) = sqrt(2/π)
        let expected = (2.0_f64 / std::f64::consts::PI).sqrt();
        assert!((c4(2) - expected).abs() < 1e-12);
    }

    #[test]
    fn c4_n3_equals_sqrt_pi_over_2() {
        // Exact closed form: Γ(3/2) / Γ(1) = (sqrt(π)/2) / 1 = sqrt(π)/2
        let expected = std::f64::consts::PI.sqrt() / 2.0;
        assert!((c4(3) - expected).abs() < 1e-12);
    }

    #[test]
    fn c4_nist_table_regression() {
        // NIST e-Handbook §6.3.2 / Montgomery Table VI — 4 sig figs
        let cases: &[(u32, f64)] = &[(5, 0.9400), (10, 0.9727), (25, 0.9896)];
        for &(n, expected) in cases {
            assert!((c4(n) - expected).abs() < 5e-5, "n={n}: got {}", c4(n));
        }
    }

    #[test]
    fn c4_grows_monotonically_toward_one() {
        let ns: &[u32] = &[2, 5, 10, 25, 100, 1_000, 100_000];
        for pair in ns.windows(2) {
            let (a, b) = (c4(pair[0]), c4(pair[1]));
            assert!(b > a && b < 1.0, "n={}: c4={}", pair[1], b);
        }
    }

    #[test]
    fn adaptive_sizes_match_table() {
        assert_eq!(adaptive_sample_size(0), 25);
        assert_eq!(adaptive_sample_size(999), 25);
        assert_eq!(adaptive_sample_size(1_000), 100);
        assert_eq!(adaptive_sample_size(9_999), 100);
        assert_eq!(adaptive_sample_size(10_000), 1_000);
        assert_eq!(adaptive_sample_size(99_999), 1_000);
        assert_eq!(adaptive_sample_size(100_000), 10_000);
        assert_eq!(adaptive_sample_size(999_999), 10_000);
        assert_eq!(adaptive_sample_size(1_000_000), 100_000);
        assert_eq!(adaptive_sample_size(10_000_000), 100_000);
    }

    #[test]
    fn limits_ordered() {
        let values: Vec<f64> = (0..1_000).map(|value| (value as f64) % 100.0).collect();
        let limits = fit_control_limits(&values, 25).expect("control limits");
        assert!(limits.three_lcl < limits.two_lcl);
        assert!(limits.two_lcl < limits.one_lcl);
        assert!(limits.one_lcl < limits.center);
        assert!(limits.center < limits.one_ucl);
        assert!(limits.one_ucl < limits.two_ucl);
        assert!(limits.two_ucl < limits.three_ucl);
    }

    #[test]
    fn x_bar_s_formula_matches_hand_computation() {
        let values = vec![1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 4.0, 6.0];
        let limits = fit_control_limits(&values, 2).expect("control limits");
        let expected_stddev_adj = (2.0_f64).sqrt() / (2.0_f64 / std::f64::consts::PI).sqrt();
        assert!((limits.center - 3.5).abs() < 1e-12);
        assert!((limits.one_ucl - (3.5 + expected_stddev_adj)).abs() < 1e-9);
        assert!((limits.three_lcl - (3.5 - 3.0 * expected_stddev_adj)).abs() < 1e-9);
    }

    #[test]
    fn trailing_partial_chunk_contributes_to_center() {
        let values = vec![0.0, 2.0, 2.0, 4.0, 100.0, 104.0];
        let limits = fit_control_limits(&values, 4).expect("control limits");
        assert!((limits.center - 52.0).abs() < 1e-12);
    }

    #[test]
    fn rejects_insufficient_samples() {
        let values = vec![1.0, 2.0, 3.0];
        let err = fit_control_limits(&values, 25).expect_err("fit error");
        assert!(matches!(
            err,
            DriftFitError::InsufficientSamplesForChunk { .. }
        ));
    }

    #[test]
    fn rejects_only_one_chunk() {
        let values: Vec<f64> = (0..25).map(|value| value as f64).collect();
        let err = fit_control_limits(&values, 25).expect_err("fit error");
        assert!(matches!(
            err,
            DriftFitError::InsufficientSamplesForChunk { .. }
        ));
    }
}
