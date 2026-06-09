//! PSI numeric binning.

use crate::error::DriftFitError;

/// Ordered bin edges.
///
/// Edges start with `-inf` and end with `+inf`. The number of bins is
/// `edges.len() - 1`.
#[derive(Debug, Clone, PartialEq)]
pub struct BinEdges {
    pub edges: Vec<f64>,
}

/// Compute equal-width PSI edges over finite baseline values.
///
/// Degenerate all-equal input returns one infinite bin, `[-inf, +inf]`.
pub fn compute_edges_equal_width(values: &[f64], n_bins: u32) -> Result<BinEdges, DriftFitError> {
    if values.is_empty() {
        return Err(DriftFitError::PsiInternal {
            message: "compute_edges_equal_width: empty values".to_string(),
        });
    }

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &value in values {
        if !value.is_finite() {
            return Err(DriftFitError::PsiInternal {
                message: "compute_edges_equal_width: non-finite input".to_string(),
            });
        }
        min = min.min(value);
        max = max.max(value);
    }

    let n = n_bins as usize;
    let mut edges = Vec::with_capacity(n + 1);
    edges.push(f64::NEG_INFINITY);

    if (max - min).abs() <= f64::EPSILON {
        edges.push(f64::INFINITY);
        return Ok(BinEdges { edges });
    }

    let width = (max - min) / n as f64;
    for i in 1..n {
        edges.push(min + width * i as f64);
    }
    edges.push(f64::INFINITY);

    Ok(BinEdges { edges })
}

/// Compute quantile PSI edges with the R-7 Hyndman-Fan estimator.
pub fn compute_edges_quantile(values: &[f64], n_bins: u32) -> Result<BinEdges, DriftFitError> {
    if values.is_empty() {
        return Err(DriftFitError::PsiInternal {
            message: "compute_edges_quantile: empty values".to_string(),
        });
    }

    let n = n_bins as usize;
    if n < 2 {
        return Err(DriftFitError::PsiInternal {
            message: "compute_edges_quantile: n_bins must be >= 2".to_string(),
        });
    }

    let mut sorted = Vec::with_capacity(values.len());
    for &value in values {
        if !value.is_finite() {
            return Err(DriftFitError::PsiInternal {
                message: "compute_edges_quantile: non-finite input".to_string(),
            });
        }
        sorted.push(value);
    }
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let count = sorted.len();
    let mut edges = Vec::with_capacity(n + 1);
    edges.push(f64::NEG_INFINITY);

    for i in 1..n {
        let p = i as f64 / n as f64;
        let m = 1.0 - p;
        let np_plus_m = count as f64 * p + m;
        let j = np_plus_m.floor() as usize;
        let h = np_plus_m - j as f64;
        let j_zero = j.saturating_sub(1);
        let j_zero_next = (j_zero + 1).min(count - 1);
        edges.push((1.0 - h) * sorted[j_zero] + h * sorted[j_zero_next]);
    }

    edges.push(f64::INFINITY);
    dedup_finite_edges(&mut edges);

    Ok(BinEdges { edges })
}

fn dedup_finite_edges(edges: &mut Vec<f64>) {
    let mut index = 1;
    while index < edges.len() {
        let previous = edges[index - 1];
        let current = edges[index];
        if previous.is_finite() && current.is_finite() && (previous - current).abs() <= f64::EPSILON
        {
            edges.remove(index);
        } else {
            index += 1;
        }
    }
}

/// Assign a finite value to the `(lower, upper]` interval it belongs to.
pub fn assign_bin(value: f64, edges: &[f64]) -> usize {
    debug_assert!(edges.len() >= 2, "edges must have at least two entries");
    let last_bin = edges.len().saturating_sub(2);
    for index in 0..edges.len() - 1 {
        if value > edges[index] && value <= edges[index + 1] {
            return index;
        }
    }
    last_bin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_width_simple_uniform() {
        let values: Vec<f64> = (0..100).map(f64::from).collect();
        let edges = compute_edges_equal_width(&values, 10).expect("edges");
        assert_eq!(edges.edges.len(), 11);
        assert!(edges.edges[0].is_infinite() && edges.edges[0].is_sign_negative());
        assert!(edges.edges[10].is_infinite() && edges.edges[10].is_sign_positive());
        assert!((edges.edges[1] - 9.9).abs() < 1e-6);
    }

    #[test]
    fn equal_width_degenerate_all_equal() {
        let values = vec![5.0; 50];
        let edges = compute_edges_equal_width(&values, 10).expect("edges");
        assert_eq!(edges.edges, vec![f64::NEG_INFINITY, f64::INFINITY]);
    }

    #[test]
    fn quantile_r7_quartiles_eight_values() {
        let values: Vec<f64> = (1..=8).map(f64::from).collect();
        let edges = compute_edges_quantile(&values, 4).expect("edges");
        assert_eq!(edges.edges.len(), 5);
        assert!((edges.edges[1] - 2.75).abs() < 1e-10);
        assert!((edges.edges[2] - 4.5).abs() < 1e-10);
        assert!((edges.edges[3] - 6.25).abs() < 1e-10);
    }

    #[test]
    fn quantile_dedups_skewed_input() {
        let mut values = vec![0.0; 95];
        values.extend([1.0, 2.0, 3.0, 4.0, 5.0]);
        let edges = compute_edges_quantile(&values, 5).expect("edges");
        assert!(edges.edges.len() < 6);
    }

    #[test]
    fn assign_bin_uses_left_open_right_closed_intervals() {
        let edges = vec![f64::NEG_INFINITY, 10.0, 20.0, 30.0, f64::INFINITY];
        assert_eq!(assign_bin(-5.0, &edges), 0);
        assert_eq!(assign_bin(10.0, &edges), 0);
        assert_eq!(assign_bin(15.0, &edges), 1);
        assert_eq!(assign_bin(20.0, &edges), 1);
        assert_eq!(assign_bin(30.0, &edges), 2);
        assert_eq!(assign_bin(30.0001, &edges), 3);
    }
}
