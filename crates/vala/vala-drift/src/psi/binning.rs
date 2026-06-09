//! PSI binning. Bodies land in Commit 5.

use crate::error::DriftFitError;

#[derive(Debug, Clone)]
pub struct BinEdges {
    pub edges: Vec<f64>,
}

pub fn compute_edges_equal_width(_values: &[f64], _n_bins: u32) -> Result<BinEdges, DriftFitError> {
    unimplemented!("Commit 5")
}

pub fn compute_edges_quantile(_values: &[f64], _n_bins: u32) -> Result<BinEdges, DriftFitError> {
    unimplemented!("Commit 5")
}

pub fn assign_bin(_value: f64, _edges: &[f64]) -> usize {
    unimplemented!("Commit 5")
}
