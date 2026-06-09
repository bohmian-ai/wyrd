//! SPC control limits: c4 bias correction, adaptive sample-size table,
//! and fit_control_limits (X-bar/S chart per NIST section 3.2.1).

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

/// c4 unbiased-stddev correction factor.
///
/// `c4(n) = (4n - 4) / (4n - 3)`. Used to scale the average of per-chunk
/// stddevs before computing control limits.
#[must_use]
pub fn c4(n: u32) -> f64 {
    let n4 = 4.0 * f64::from(n);
    (n4 - 4.0) / (n4 - 3.0)
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
    fn c4_at_n_25_matches_formula() {
        let value = c4(25);
        assert!((value - 96.0 / 97.0).abs() < 1e-12);
    }

    #[test]
    fn c4_grows_toward_one() {
        assert!(c4(1_000) > c4(25));
        assert!(c4(1_000) < 1.0);
        assert!(c4(1_000_000) > 0.999);
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
        let expected_stddev_adj = (2.0_f64).sqrt() / 0.8;
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
