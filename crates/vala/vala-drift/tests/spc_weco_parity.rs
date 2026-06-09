//! WECO scan fixtures for signed-zone rule behavior.

use vala_drift::spc::weco::{WecoViolation, evaluate, parse_rule};
use wyrd_spec::card::drift::SpcAlertThreshold;

#[test]
fn zone4_single_breach_fires_consecutive() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![1, 1, 4];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 4, sign: 1 }))
    );
}

#[test]
fn zone2_consecutive_needs_full_window_same_side() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![2, 2, 2, -2];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone2);
    assert!(
        !violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 2, .. }))
    );
}

#[test]
fn zone2_consecutive_fires_when_all_same_side_at_threshold() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![2, 2, 2, 2];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone2);
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 2, sign: 1 }))
    );
}

#[test]
fn alternating_zero_resets_run() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![3, -3, 0, 3, -3];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone3);
    assert!(
        !violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Alternating { zone: 3 }))
    );
}

#[test]
fn per_zone_rule_only_evaluates_at_matching_zone_index() {
    let rule = parse_rule("8 16 2 4 2 4 1 1").expect("valid rule");
    let drift = vec![3, 4];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone1);
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 4, .. }))
    );
    assert!(
        !violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Consecutive { zone: 3, .. }))
    );
}

#[test]
fn trend_rule_parity_six_of_seven_increasing_zones() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![-3, -2, -1, 0, 1, 2, 3];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
    let trend_hits = violations
        .iter()
        .filter(|violation| matches!(violation, WecoViolation::Trend { sign: 1 }))
        .count();
    assert_eq!(trend_hits, 1);
}

#[test]
fn trend_rule_parity_decreasing_zones_fires_negative_sign() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![3, 2, 1, 0, -1, -2, -3];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Trend { sign: -1 }))
    );
}

#[test]
fn trend_rule_parity_flat_zone_does_not_fire() {
    let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
    let drift = vec![1; 7];
    let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
    assert!(
        !violations
            .iter()
            .any(|violation| matches!(violation, WecoViolation::Trend { .. }))
    );
}
