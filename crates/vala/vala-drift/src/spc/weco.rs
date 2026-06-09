//! WECO rule parsing, zone assignment, and rule evaluation.

use wyrd_spec::card::drift::SpcAlertThreshold;

use crate::error::DriftScoreError;
use crate::spc::control_limits::ControlLimits;

/// Parsed WECO rule thresholds: four zones times consecutive and alternating
/// checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// A single rule firing.
#[derive(Debug, Clone, PartialEq)]
pub enum WecoViolation {
    Consecutive { zone: u8, sign: i8 },
    Alternating { zone: u8 },
    Trend { sign: i8 },
}

/// Trend rule window length.
pub const SPC_TREND_WINDOW: usize = 7;
/// Minimum monotonic adjacent transitions inside a trend window before firing.
pub const SPC_TREND_MIN_MONOTONIC: u32 = 6;

/// Parse the WECO rule string.
///
/// # Errors
///
/// Returns [`DriftScoreError::WecoMalformed`] when the input is not eight
/// whitespace-separated positive `u32` values.
pub fn parse_rule(s: &str) -> Result<WecoRule, DriftScoreError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 8 {
        return Err(DriftScoreError::WecoMalformed { got: s.to_string() });
    }

    let mut vals = [0u32; 8];
    for (idx, part) in parts.iter().enumerate() {
        let value = part
            .parse::<u32>()
            .map_err(|_| DriftScoreError::WecoMalformed { got: s.to_string() })?;
        if value == 0 {
            return Err(DriftScoreError::WecoMalformed { got: s.to_string() });
        }
        vals[idx] = value;
    }

    Ok(WecoRule {
        zone1_consec: vals[0],
        zone1_alt: vals[1],
        zone2_consec: vals[2],
        zone2_alt: vals[3],
        zone3_consec: vals[4],
        zone3_alt: vals[5],
        zone4_consec: vals[6],
        zone4_alt: vals[7],
    })
}

/// Assign a signed zone in `{-4..=-1, 0, 1..=4}` for one chunk mean.
#[must_use]
pub fn assign_zone(value: f64, limits: &ControlLimits) -> i8 {
    if value > limits.three_ucl {
        4
    } else if value < limits.three_lcl {
        -4
    } else if value < limits.three_ucl && value >= limits.two_ucl {
        3
    } else if value < limits.two_ucl && value >= limits.one_ucl {
        2
    } else if value < limits.one_ucl && value > limits.center {
        1
    } else if value > limits.three_lcl && value <= limits.two_lcl {
        -3
    } else if value > limits.two_lcl && value <= limits.one_lcl {
        -2
    } else if value > limits.one_lcl && value < limits.center {
        -1
    } else {
        0
    }
}

fn slice_consecutive_hit(slice: &[i8], zone: u8) -> Option<i8> {
    let z = zone as i8;
    let len = slice.len();
    if len == 0 {
        return None;
    }

    if slice.iter().filter(|&&value| value >= z).count() == len {
        return Some(1);
    }
    if slice.iter().filter(|&&value| value <= -z).count() == len {
        return Some(-1);
    }
    None
}

fn slice_alternating_hit(slice: &[i8], zone: u8, alt_threshold: u32) -> bool {
    let z = zone as i8;
    let mut last_value = 0;
    let mut alt_count = 0;

    for &value in slice {
        if value == 0 {
            last_value = 0;
            alt_count = 0;
            continue;
        }

        if value != last_value && (value >= z || value <= -z) {
            alt_count += 1;
            if alt_count >= alt_threshold {
                return true;
            }
            last_value = value;
        } else {
            last_value = 0;
            alt_count = 0;
        }
    }

    false
}

fn scan_zone(
    drift_array: &[i8],
    zone: u8,
    consec_threshold: u32,
    alt_threshold: u32,
) -> (Option<i8>, bool) {
    let z = zone as i8;
    let mut consec_hit = None;
    let mut alt_hit = false;

    for (idx, &value) in drift_array.iter().enumerate() {
        if value != z && value != -z {
            continue;
        }

        let consec_len = consec_threshold as usize;
        if idx + 1 >= consec_len {
            let start = idx + 1 - consec_len;
            if let Some(sign) = slice_consecutive_hit(&drift_array[start..=idx], zone) {
                consec_hit = Some(sign);
            }
        }

        let alt_len = alt_threshold as usize;
        if idx + 1 >= alt_len
            && slice_alternating_hit(&drift_array[idx + 1 - alt_len..=idx], zone, alt_threshold)
        {
            alt_hit = true;
        }
    }

    (consec_hit, alt_hit)
}

fn scan_trend(drift_array: &[i8]) -> Vec<WecoViolation> {
    let mut hits = Vec::new();
    if drift_array.len() < SPC_TREND_WINDOW {
        return hits;
    }

    for window in drift_array.windows(SPC_TREND_WINDOW) {
        let mut increasing = 0;
        let mut decreasing = 0;
        for pair in window.windows(2) {
            if pair[1] > pair[0] {
                increasing += 1;
            } else if pair[1] < pair[0] {
                decreasing += 1;
            }
        }

        if increasing >= SPC_TREND_MIN_MONOTONIC {
            hits.push(WecoViolation::Trend { sign: 1 });
        } else if decreasing >= SPC_TREND_MIN_MONOTONIC {
            hits.push(WecoViolation::Trend { sign: -1 });
        }
    }

    hits
}

/// Evaluate WECO rules over a signed-zone drift array.
///
/// Zone-rule scans are gated by `threshold`; the trend rule is always
/// evaluated against the same signed-zone array.
pub fn evaluate(
    drift_array: &[i8],
    rule: &WecoRule,
    threshold: SpcAlertThreshold,
) -> Vec<WecoViolation> {
    let zones: &[(u8, u32, u32)] = match threshold {
        SpcAlertThreshold::Zone1 => &[
            (1, rule.zone1_consec, rule.zone1_alt),
            (2, rule.zone2_consec, rule.zone2_alt),
            (3, rule.zone3_consec, rule.zone3_alt),
            (4, rule.zone4_consec, rule.zone4_alt),
        ],
        SpcAlertThreshold::Zone2 => &[
            (2, rule.zone2_consec, rule.zone2_alt),
            (3, rule.zone3_consec, rule.zone3_alt),
            (4, rule.zone4_consec, rule.zone4_alt),
        ],
        SpcAlertThreshold::Zone3 => &[
            (3, rule.zone3_consec, rule.zone3_alt),
            (4, rule.zone4_consec, rule.zone4_alt),
        ],
        SpcAlertThreshold::Zone4 => &[(4, rule.zone4_consec, rule.zone4_alt)],
    };

    let mut violations = Vec::new();
    for &(zone, consec_threshold, alt_threshold) in zones {
        let (consec_hit, alt_hit) = scan_zone(drift_array, zone, consec_threshold, alt_threshold);
        if let Some(sign) = consec_hit {
            violations.push(WecoViolation::Consecutive { zone, sign });
        }
        if alt_hit {
            violations.push(WecoViolation::Alternating { zone });
        }
    }
    violations.extend(scan_trend(drift_array));
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits_basic() -> ControlLimits {
        ControlLimits {
            center: 0.0,
            one_ucl: 1.0,
            one_lcl: -1.0,
            two_ucl: 2.0,
            two_lcl: -2.0,
            three_ucl: 3.0,
            three_lcl: -3.0,
        }
    }

    #[test]
    fn parse_default_rule() {
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid default rule");
        assert_eq!(rule.zone1_consec, 8);
        assert_eq!(rule.zone4_alt, 1);
    }

    #[test]
    fn parse_rejects_seven_values() {
        assert!(matches!(
            parse_rule("8 16 4 8 2 4 1").expect_err("invalid rule"),
            DriftScoreError::WecoMalformed { .. }
        ));
    }

    #[test]
    fn parse_rejects_zero() {
        assert!(matches!(
            parse_rule("8 16 4 8 2 4 0 1").expect_err("invalid rule"),
            DriftScoreError::WecoMalformed { .. }
        ));
    }

    #[test]
    fn assign_zone_in_control() {
        let limits = limits_basic();
        assert_eq!(assign_zone(0.5, &limits), 1);
        assert_eq!(assign_zone(-0.5, &limits), -1);
    }

    #[test]
    fn assign_zone_exactly_center_is_zero() {
        let limits = limits_basic();
        assert_eq!(assign_zone(0.0, &limits), 0);
    }

    #[test]
    fn assign_zone_above_three_sigma() {
        let limits = limits_basic();
        assert_eq!(assign_zone(3.5, &limits), 4);
        assert_eq!(assign_zone(-3.5, &limits), -4);
    }

    #[test]
    fn consecutive_zone4_one_breach() {
        let drift = vec![1, 1, 4, 1];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        assert!(
            violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 4, .. }))
        );
    }

    #[test]
    fn consecutive_zone1_8_same_side() {
        let drift = vec![1; 8];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone1);
        assert!(
            violations.iter().any(|violation| matches!(
                violation,
                WecoViolation::Consecutive { zone: 1, sign: 1 }
            ))
        );
    }

    #[test]
    fn alternating_zone1_short_window() {
        let drift = vec![3, -3];
        let rule = parse_rule("4 8 2 4 1 2 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone1);
        assert!(
            violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Alternating { zone: 3 }))
        );
    }

    #[test]
    fn no_violation_for_in_control_alternating_zone1() {
        let drift = vec![1, -1, 1, -1, 1];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone2);
        assert!(violations.is_empty(), "got {violations:?}");
    }

    #[test]
    fn trend_rule_fires_on_strict_monotonic_zone_run_of_seven() {
        let drift = vec![-3, -2, -1, 0, 1, 2, 3];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        assert!(
            violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Trend { sign: 1 }))
        );
    }

    #[test]
    fn trend_rule_does_not_fire_on_flat_zone_array_even_if_chunk_means_rise() {
        let drift = vec![1; 7];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        assert!(
            !violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Trend { .. }))
        );
    }

    #[test]
    fn trend_rule_does_not_fire_on_six_value_run() {
        let drift = vec![-3, -2, -1, 0, 1, 2];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        assert!(
            !violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Trend { .. }))
        );
    }

    #[test]
    fn trend_rule_fires_independent_of_alert_threshold() {
        let drift = vec![3, 2, 1, 0, -1, -2, -3];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        assert!(
            violations
                .iter()
                .any(|violation| matches!(violation, WecoViolation::Trend { sign: -1 }))
        );
    }
}
