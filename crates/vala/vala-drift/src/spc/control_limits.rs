//! Control limits + c4 + adaptive sample size. Bodies land in Commit 7.

use crate::error::DriftFitError;

#[derive(Debug, Clone, Copy)]
pub struct ControlLimits {
    pub center: f64,
    pub one_lcl: f64,
    pub one_ucl: f64,
    pub two_lcl: f64,
    pub two_ucl: f64,
    pub three_lcl: f64,
    pub three_ucl: f64,
}

pub fn c4(_n: u32) -> f64 {
    unimplemented!("Commit 7")
}

pub fn adaptive_sample_size(_row_count: usize) -> u32 {
    unimplemented!("Commit 7")
}

pub fn fit_control_limits(
    _values: &[f64],
    _chunk_size: u32,
) -> Result<ControlLimits, DriftFitError> {
    unimplemented!("Commit 7")
}
