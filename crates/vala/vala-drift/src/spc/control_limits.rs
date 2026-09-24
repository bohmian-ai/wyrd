//! NIST two-sided, three-sigma X-bar/S control limits.
//!
//! For subgroup size `n`, with grand mean `x_bar_bar`, mean subgroup sample
//! standard deviation `s_bar`, and bias correction `c4(n)`
//! ([NIST 6.3.2.1](https://itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm)):
//!
//! ```text
//! X-bar: x_bar_bar ± 3 * s_bar / (c4 * sqrt(n))
//! S:     max(0, s_bar * (1 - 3 * sqrt(1 - c4²) / c4)) .. s_bar * (1 + 3 * sqrt(1 - c4²) / c4)
//! ```

use serde::{Deserialize, Serialize};
use statrs::function::gamma::ln_gamma;

use crate::error::DriftFitError;

/// One chart's frozen center line and control limits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChartLimits {
    /// Center line.
    pub center: f64,
    /// Lower control limit.
    pub lower: f64,
    /// Upper control limit.
    pub upper: f64,
}

impl ChartLimits {
    /// Whether `value` lies strictly outside the limits; equality is in control.
    #[must_use]
    pub fn signals(&self, value: f64) -> bool {
        value < self.lower || value > self.upper
    }
}

/// c4 unbiased-stddev correction factor (exact Gamma-based formula).
///
/// `c4(n) = sqrt(2/(n-1)) · Γ(n/2) / Γ((n-1)/2)`
///
/// Computed in log-space via `ln_gamma` for numerical stability across all
/// subgroup sizes. References: NIST e-Handbook §6.3.2; Montgomery
/// "Introduction to Statistical Quality Control" Table VI.
#[must_use]
pub fn c4(n: u32) -> f64 {
    let nf = f64::from(n);
    (0.5 * (2.0 / (nf - 1.0)).ln() + ln_gamma(nf / 2.0) - ln_gamma((nf - 1.0) / 2.0)).exp()
}

/// Mean and sample standard deviation (`n - 1` denominator) of one subgroup.
///
/// A subgroup of one row has a NaN standard deviation; complete subgroups
/// always hold at least two rows.
#[must_use]
pub fn subgroup_stats(rows: &[f64]) -> (f64, f64) {
    let n = rows.len() as f64;
    let mean = rows.iter().sum::<f64>() / n;
    let variance = rows.iter().map(|row| (row - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, variance.sqrt())
}

/// Fit X-bar and S chart limits from complete subgroup statistics.
///
/// `stats` holds each baseline subgroup's `(mean, sample standard deviation)`
/// for subgroups of exactly `n` rows. Returns `(x_bar, s)` limits.
///
/// # Errors
/// Returns [`DriftFitError::SpcInternal`] when `n < 2`, `stats` is empty, or
/// a limit is not finite.
pub fn fit_x_bar_s(
    stats: &[(f64, f64)],
    n: u32,
) -> Result<(ChartLimits, ChartLimits), DriftFitError> {
    if n < 2 || stats.is_empty() {
        return Err(DriftFitError::SpcInternal {
            message: "an X-bar/S fit needs subgroups of at least two rows".into(),
        });
    }
    let count = stats.len() as f64;
    let x_bar_bar = stats.iter().map(|(mean, _)| mean).sum::<f64>() / count;
    let s_bar = stats.iter().map(|(_, sd)| sd).sum::<f64>() / count;
    let c4 = c4(n);
    let x_width = 3.0 * s_bar / (c4 * f64::from(n).sqrt());
    let s_width = 3.0 * (1.0 - c4 * c4).sqrt() / c4;
    let x_bar = ChartLimits {
        center: x_bar_bar,
        lower: x_bar_bar - x_width,
        upper: x_bar_bar + x_width,
    };
    let s = ChartLimits {
        center: s_bar,
        lower: (s_bar * (1.0 - s_width)).max(0.0),
        upper: s_bar * (1.0 + s_width),
    };
    let limits = [x_bar, s];
    if limits
        .iter()
        .flat_map(|chart| [chart.center, chart.lower, chart.upper])
        .any(|value| !value.is_finite())
    {
        return Err(DriftFitError::SpcInternal {
            message: "non-finite control limit".into(),
        });
    }
    Ok((x_bar, s))
}

#[cfg(test)]
mod tests {
    //! c4, subgroup statistics, and X-bar/S limits against closed forms and
    //! independently computed NIST fixtures.

    use super::*;

    /// Assert `got` equals `want` to 1e-9.
    ///
    /// # Panics
    /// Panics when the values differ.
    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    /// c4(2) is exactly `sqrt(2/π)`.
    #[test]
    fn c4_n2_equals_sqrt_2_over_pi() {
        close(c4(2), (2.0_f64 / std::f64::consts::PI).sqrt());
    }

    /// c4(3) is exactly `sqrt(π)/2`.
    #[test]
    fn c4_n3_equals_sqrt_pi_over_2() {
        close(c4(3), std::f64::consts::PI.sqrt() / 2.0);
    }

    /// c4 matches the NIST/Montgomery table to four significant figures.
    #[test]
    fn c4_nist_table_regression() {
        let cases: &[(u32, f64)] = &[(5, 0.9400), (10, 0.9727), (25, 0.9896)];
        for &(n, expected) in cases {
            assert!((c4(n) - expected).abs() < 5e-5, "n={n}: got {}", c4(n));
        }
    }

    /// The published A3, B3, and B4 constants follow from c4: A3 = 3/(c4·√n),
    /// B3/B4 = 1 ∓ 3·√(1-c4²)/c4 (NIST/Montgomery Table VI, three decimals).
    #[test]
    fn limits_reproduce_published_a3_b3_b4_constants() {
        let table: &[(u32, f64, f64, f64)] = &[
            (2, 2.659, 0.0, 3.267),
            (5, 1.427, 0.0, 2.089),
            (10, 0.975, 0.284, 1.716),
            (25, 0.606, 0.565, 1.435),
        ];
        for &(n, a3, b3, b4) in table {
            let stats = vec![(0.0, 1.0); 20];
            let (x_bar, s) = fit_x_bar_s(&stats, n).expect("limits");
            assert!((x_bar.upper - a3).abs() < 5e-4, "A3 n={n}: {}", x_bar.upper);
            assert!((s.lower - b3).abs() < 5e-4, "B3 n={n}: {}", s.lower);
            assert!((s.upper - b4).abs() < 5e-4, "B4 n={n}: {}", s.upper);
        }
    }

    /// Hand-computed fixture for n = 4 pins both charts and the `sqrt(n)`
    /// factor: the old limits omitted it and were twice as wide here.
    ///
    /// Subgroups `[1,2,3,4]` and `[3,4,5,6]` give means 2.5 and 4.5 and equal
    /// sample SDs `sqrt(5/3)`; `c4(4) = sqrt(2/3)·Γ(2)/Γ(3/2) = 0.9213177`.
    #[test]
    fn x_bar_s_fixture_includes_sqrt_n() {
        let stats = [
            subgroup_stats(&[1.0, 2.0, 3.0, 4.0]),
            subgroup_stats(&[3.0, 4.0, 5.0, 6.0]),
        ];
        close(stats[0].0, 2.5);
        close(stats[0].1, (5.0_f64 / 3.0).sqrt());
        let (x_bar, s) = fit_x_bar_s(&stats, 4).expect("limits");
        let s_bar = (5.0_f64 / 3.0).sqrt();
        let c4_4 = (2.0_f64 / 3.0).sqrt() / (std::f64::consts::PI.sqrt() / 2.0);
        close(c4(4), c4_4);
        assert!((c4_4 - 0.921_317_7).abs() < 1e-7, "{c4_4}");
        let half_width = 3.0 * s_bar / (c4_4 * 2.0);
        close(x_bar.center, 3.5);
        close(x_bar.lower, 3.5 - half_width);
        close(x_bar.upper, 3.5 + half_width);
        assert!(
            (x_bar.upper - (3.5 + 3.0 * s_bar / c4_4)).abs() > 1.0,
            "limits without sqrt(n) are the regression"
        );
        let b = 3.0 * (1.0 - c4_4 * c4_4).sqrt() / c4_4;
        close(s.center, s_bar);
        close(s.lower, 0.0_f64.max(s_bar * (1.0 - b)));
        close(s.upper, s_bar * (1.0 + b));
    }

    /// A value equal to a limit is in control; beyond either limit signals.
    #[test]
    fn equality_at_the_limit_does_not_signal() {
        let limits = ChartLimits {
            center: 0.0,
            lower: -1.0,
            upper: 1.0,
        };
        assert!(!limits.signals(1.0) && !limits.signals(-1.0) && !limits.signals(0.0));
        assert!(limits.signals(1.0 + 1e-12) && limits.signals(-1.0 - 1e-12));
    }

    /// Subgroups below two rows and an empty fit are refused.
    #[test]
    fn rejects_degenerate_fits() {
        assert!(fit_x_bar_s(&[(0.0, 1.0)], 1).is_err());
        assert!(fit_x_bar_s(&[], 5).is_err());
        assert!(fit_x_bar_s(&[(f64::NAN, 1.0)], 5).is_err());
    }
}
