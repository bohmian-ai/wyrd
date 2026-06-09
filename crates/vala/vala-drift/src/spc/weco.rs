//! WECO rule parsing + zone assignment + rule evaluation. Bodies land in Commit 8.

use wyrd_spec::card::drift::SpcAlertThreshold;

use crate::error::DriftScoreError;
use crate::spc::control_limits::ControlLimits;

#[derive(Debug, Clone, Copy)]
pub struct WecoRule {
    pub zone1_consec: u32,
    pub zone1_alt: u32,
    pub zone2_consec: u32,
    pub zone2_alt: u32,
    pub zone3_consec: u32,
    pub zone3_alt: u32,
    pub zone4_consec: u32,
    pub zone4_alt: u32,
}

/// Mirrors the final shape in Commit 8.
#[derive(Debug, Clone, PartialEq)]
pub enum WecoViolation {
    Consecutive { zone: u8, sign: i8 },
    Alternating { zone: u8 },
    Trend { sign: i8 },
}

pub const SPC_TREND_WINDOW: usize = 7;
pub const SPC_TREND_MIN_MONOTONIC: u32 = 6;

pub fn parse_rule(_s: &str) -> Result<WecoRule, DriftScoreError> {
    unimplemented!("Commit 8")
}

pub fn assign_zone(_value: f64, _limits: &ControlLimits) -> i8 {
    unimplemented!("Commit 8")
}

/// Final shape: takes only the signed-zone drift array.
pub fn evaluate(
    _drift_array: &[i8],
    _rule: &WecoRule,
    _threshold: SpcAlertThreshold,
) -> Vec<WecoViolation> {
    unimplemented!("Commit 8")
}
