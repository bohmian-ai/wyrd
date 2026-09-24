//! WECO rule parsing, zone assignment, and rule evaluation.

use std::collections::VecDeque;

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
        return Err(DriftScoreError::WecoMalformed {
            got: s.chars().take(200).collect::<String>(),
        });
    }

    let mut vals = [0u32; 8];
    for (idx, part) in parts.iter().enumerate() {
        let value = part
            .parse::<u32>()
            .map_err(|_| DriftScoreError::WecoMalformed {
                got: s.chars().take(200).collect::<String>(),
            })?;
        if value == 0 {
            return Err(DriftScoreError::WecoMalformed {
                got: s.chars().take(200).collect::<String>(),
            });
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

/// One alert-gated zone rule: the zone magnitude and its authored
/// consecutive and alternating run lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZoneCheck {
    /// Zone magnitude in `1..=4`.
    zone: u8,
    /// Trailing run length that must sit entirely on one side at `>= zone`.
    consec: usize,
    /// Trailing run length scanned for an alternating-sign run.
    alt: usize,
}

/// The WECO checks one profile applies, resolved once from a parsed rule and
/// its alert threshold.
///
/// Owns the alert-threshold gating and the lookback cap that bounds every
/// [`WecoScan`] history, so per-series scans only carry trailing zones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WecoChecks {
    /// Zone rules gated in by the alert threshold, lowest zone first.
    zones: Vec<ZoneCheck>,
    /// Longest trailing window any rule reads: every authored threshold and
    /// the trend window. A scan never retains more zones than this.
    lookback: usize,
}

impl WecoChecks {
    /// Resolve the gated zone rules and lookback cap for `rule` at `threshold`.
    ///
    /// `threshold` admits its own zone and every higher zone. The lookback cap
    /// is the maximum of all eight authored thresholds (gated or not) and
    /// [`SPC_TREND_WINDOW`].
    #[must_use]
    pub(crate) fn new(rule: &WecoRule, threshold: SpcAlertThreshold) -> Self {
        let all = [
            (1, rule.zone1_consec, rule.zone1_alt),
            (2, rule.zone2_consec, rule.zone2_alt),
            (3, rule.zone3_consec, rule.zone3_alt),
            (4, rule.zone4_consec, rule.zone4_alt),
        ];
        let lowest = match threshold {
            SpcAlertThreshold::Zone1 => 1,
            SpcAlertThreshold::Zone2 => 2,
            SpcAlertThreshold::Zone3 => 3,
            SpcAlertThreshold::Zone4 => 4,
        };
        let zones = all
            .iter()
            .filter(|(zone, ..)| *zone >= lowest)
            .map(|&(zone, consec, alt)| ZoneCheck {
                zone,
                consec: consec as usize,
                alt: alt as usize,
            })
            .collect();
        let lookback = all
            .iter()
            .flat_map(|&(_, consec, alt)| [consec as usize, alt as usize])
            .fold(SPC_TREND_WINDOW, usize::max);
        Self { zones, lookback }
    }
}

/// Incremental WECO rule scan over one ordered signed-zone series.
///
/// Each [`WecoScan::push`] appends one zone and reports every rule that fires
/// at that index, reading only the trailing zones it retains. The history is
/// capped at [`WecoChecks`]'s lookback, so memory is bounded by the largest
/// rule window rather than the series length.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WecoScan {
    /// Trailing zones, oldest first, never longer than the lookback cap.
    history: VecDeque<i8>,
    /// Whether the alternating rule already fired for zone `index + 1`; each
    /// zone contributes at most one alternating violation per series.
    alt_fired: [bool; 4],
}

impl WecoScan {
    /// Append the next zone of the series and emit each rule firing at it.
    ///
    /// When `value` is `±zone` for a gated zone, the consecutive rule fires
    /// if the trailing `consec` zones are all `>= zone` or all `<= -zone`, and
    /// the alternating rule fires (once per zone) if the trailing `alt` zones
    /// hold an alternating run of `alt`. The trend rule fires for every full
    /// trailing [`SPC_TREND_WINDOW`] with at least
    /// [`SPC_TREND_MIN_MONOTONIC`] increasing or decreasing transitions.
    /// Because every threshold is at most the lookback cap, a trailing window
    /// is complete exactly when the capped history is at least that long.
    pub(crate) fn push(
        &mut self,
        checks: &WecoChecks,
        value: i8,
        mut emit: impl FnMut(WecoViolation),
    ) {
        if self.history.len() == checks.lookback {
            self.history.pop_front();
        }
        self.history.push_back(value);

        for check in &checks.zones {
            let z = check.zone as i8;
            if value != z && value != -z {
                continue;
            }
            if let Some(sign) = self.consecutive_sign(check.consec, z) {
                emit(WecoViolation::Consecutive {
                    zone: check.zone,
                    sign,
                });
            }
            let fired = &mut self.alt_fired[usize::from(check.zone - 1)];
            if !*fired && alternating_hit(trailing(&self.history, check.alt), z, check.alt) {
                *fired = true;
                emit(WecoViolation::Alternating { zone: check.zone });
            }
        }

        if let Some(window) = trailing(&self.history, SPC_TREND_WINDOW) {
            let steps = window.clone().zip(window.skip(1));
            let increasing = steps.clone().filter(|(prev, next)| next > prev).count();
            let decreasing = steps.filter(|(prev, next)| next < prev).count();
            if increasing >= SPC_TREND_MIN_MONOTONIC as usize {
                emit(WecoViolation::Trend { sign: 1 });
            } else if decreasing >= SPC_TREND_MIN_MONOTONIC as usize {
                emit(WecoViolation::Trend { sign: -1 });
            }
        }
    }

    /// Sign of a full trailing `len`-zone run entirely at `>= z` (`1`) or
    /// entirely at `<= -z` (`-1`); `None` when the run is incomplete or mixed.
    fn consecutive_sign(&self, len: usize, z: i8) -> Option<i8> {
        let mut window = trailing(&self.history, len)?;
        if window.clone().all(|value| value >= z) {
            Some(1)
        } else if window.all(|value| value <= -z) {
            Some(-1)
        } else {
            None
        }
    }

    /// Number of zones currently retained; bounded by the lookback cap.
    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.history.len()
    }
}

/// The last `len` zones of `history`, or `None` when fewer are retained.
fn trailing(history: &VecDeque<i8>, len: usize) -> Option<impl Iterator<Item = i8> + Clone + '_> {
    let start = history.len().checked_sub(len)?;
    Some(history.range(start..).copied())
}

/// Whether `window` holds a run of `alt_threshold` nonzero zones at magnitude
/// `>= z`, each differing from the previous; a zero or a repeat resets the
/// run. `None` (an incomplete window) never hits.
fn alternating_hit(window: Option<impl Iterator<Item = i8>>, z: i8, alt_threshold: usize) -> bool {
    let Some(window) = window else {
        return false;
    };
    let mut last_value = 0;
    let mut alt_count = 0;

    for value in window {
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

/// Evaluate WECO rules over a whole signed-zone drift array.
///
/// Feeds the array through one [`WecoScan`] and collects every violation, so
/// it shares the production incremental algorithm. Zone rules are gated by
/// `threshold`; the trend rule is always evaluated.
#[cfg(test)]
pub(crate) fn evaluate(
    drift_array: &[i8],
    rule: &WecoRule,
    threshold: SpcAlertThreshold,
) -> Vec<WecoViolation> {
    let checks = WecoChecks::new(rule, threshold);
    let mut scan = WecoScan::default();
    let mut violations = Vec::new();
    for &value in drift_array {
        scan.push(&checks, value, |violation| violations.push(violation));
    }
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
    fn assign_zone_covers_all_regions() {
        let limits = limits_basic();
        // Negative side
        assert_eq!(assign_zone(-3.5, &limits), -4); // beyond 3σ
        assert_eq!(assign_zone(-2.5, &limits), -3); // 2σ–3σ
        assert_eq!(assign_zone(-1.5, &limits), -2); // 1σ–2σ
        assert_eq!(assign_zone(-0.5, &limits), -1); // center–1σ
        // Center
        assert_eq!(assign_zone(0.0, &limits), 0);
        // Positive side
        assert_eq!(assign_zone(0.5, &limits), 1); // center–1σ
        assert_eq!(assign_zone(1.5, &limits), 2); // 1σ–2σ
        assert_eq!(assign_zone(2.5, &limits), 3); // 2σ–3σ
        assert_eq!(assign_zone(3.5, &limits), 4); // beyond 3σ
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
    fn zone2_consecutive_needs_full_window_same_side() {
        let drift = vec![2i8, 2, 2, -2];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone2);
        assert!(
            !violations
                .iter()
                .any(|v| matches!(v, WecoViolation::Consecutive { zone: 2, .. }))
        );
    }

    #[test]
    fn zone2_consecutive_fires_when_all_same_side_at_threshold() {
        let drift = vec![2i8, 2, 2, 2];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone2);
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, WecoViolation::Consecutive { zone: 2, sign: 1 }))
        );
    }

    #[test]
    fn alternating_zero_resets_run() {
        let drift = vec![3i8, -3, 0, 3, -3];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone3);
        assert!(
            !violations
                .iter()
                .any(|v| matches!(v, WecoViolation::Alternating { zone: 3 }))
        );
    }

    #[test]
    fn per_zone_rule_only_evaluates_at_matching_zone_index() {
        let drift = vec![3i8, 4];
        let rule = parse_rule("8 16 2 4 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone1);
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, WecoViolation::Consecutive { zone: 4, .. }))
        );
        assert!(
            !violations
                .iter()
                .any(|v| matches!(v, WecoViolation::Consecutive { zone: 3, .. }))
        );
    }

    #[test]
    fn two_non_overlapping_zone4_windows_produce_score_two() {
        // Two isolated zone-4 points with zone4_consec=1 should each fire
        // independently, yielding two Consecutive violations (score == 2.0).
        let drift = vec![4i8, 0, 0, 0, 4];
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let violations = evaluate(&drift, &rule, SpcAlertThreshold::Zone4);
        let consec_count = violations
            .iter()
            .filter(|v| matches!(v, WecoViolation::Consecutive { zone: 4, sign: 1 }))
            .count();
        assert_eq!(
            consec_count, 2,
            "expected 2 zone-4 violations, got {violations:?}"
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

    /// Pre-streaming slice-rescanning WECO evaluation, kept verbatim as the
    /// parity oracle for the incremental [`WecoScan`].
    mod reference {
        use super::super::{SPC_TREND_MIN_MONOTONIC, SPC_TREND_WINDOW, WecoRule, WecoViolation};
        use wyrd_spec::card::drift::SpcAlertThreshold;

        /// Old consecutive check over a full trailing slice.
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

        /// Old alternating check over a full trailing slice.
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

        /// Old per-zone scan: consecutive hits and whether alternating fired.
        fn scan_zone(drift: &[i8], zone: u8, consec: u32, alt: u32) -> (Vec<i8>, bool) {
            let z = zone as i8;
            let mut consec_hits = Vec::new();
            let mut alt_hit = false;
            for (idx, &value) in drift.iter().enumerate() {
                if value != z && value != -z {
                    continue;
                }
                let consec_len = consec as usize;
                if idx + 1 >= consec_len {
                    let start = idx + 1 - consec_len;
                    if let Some(sign) = slice_consecutive_hit(&drift[start..=idx], zone) {
                        consec_hits.push(sign);
                    }
                }
                let alt_len = alt as usize;
                if idx + 1 >= alt_len
                    && slice_alternating_hit(&drift[idx + 1 - alt_len..=idx], zone, alt)
                {
                    alt_hit = true;
                }
            }
            (consec_hits, alt_hit)
        }

        /// Old trend scan over every full window.
        fn scan_trend(drift: &[i8]) -> Vec<WecoViolation> {
            let mut hits = Vec::new();
            if drift.len() < SPC_TREND_WINDOW {
                return hits;
            }
            for window in drift.windows(SPC_TREND_WINDOW) {
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

        /// Old whole-array evaluation.
        pub(super) fn evaluate(
            drift: &[i8],
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
            for &(zone, consec, alt) in zones {
                let (consec_hits, alt_hit) = scan_zone(drift, zone, consec, alt);
                for sign in consec_hits {
                    violations.push(WecoViolation::Consecutive { zone, sign });
                }
                if alt_hit {
                    violations.push(WecoViolation::Alternating { zone });
                }
            }
            violations.extend(scan_trend(drift));
            violations
        }
    }

    /// Order-independent rendering of a violation multiset for comparison.
    fn sorted(violations: &[WecoViolation]) -> Vec<String> {
        let mut rendered: Vec<String> = violations.iter().map(|v| format!("{v:?}")).collect();
        rendered.sort();
        rendered
    }

    /// The incremental scan fires exactly the violations the old
    /// slice-rescanning evaluation fired, over long pseudo-random zone
    /// sequences, several rules (including authored thresholds above 8), and
    /// every alert threshold.
    ///
    /// # Panics
    /// Panics when any sequence's violation multiset differs from the oracle.
    #[test]
    fn incremental_scan_matches_reference_on_random_sequences() {
        let rules = [
            "8 16 4 8 2 4 1 1",
            "1 1 1 1 1 1 1 1",
            "3 2 2 3 2 2 1 2",
            "20 12 9 10 3 5 2 3",
        ];
        let thresholds = [
            SpcAlertThreshold::Zone1,
            SpcAlertThreshold::Zone2,
            SpcAlertThreshold::Zone3,
            SpcAlertThreshold::Zone4,
        ];
        // Deterministic LCG so failures reproduce without a rand dependency.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u32
        };
        for rule_string in rules {
            let rule = parse_rule(rule_string).expect("valid rule");
            for threshold in thresholds {
                for trial in 0..40 {
                    let len = 1 + (next() % 400) as usize;
                    // Alternate between the full zone range and a narrow band
                    // that produces long alternating and same-side runs.
                    let drift: Vec<i8> = (0..len)
                        .map(|_| {
                            if trial % 2 == 0 {
                                (next() % 9) as i8 - 4
                            } else {
                                [-1i8, 1, 1, -1, 2, -2][(next() % 6) as usize]
                            }
                        })
                        .collect();
                    assert_eq!(
                        sorted(&evaluate(&drift, &rule, threshold)),
                        sorted(&reference::evaluate(&drift, &rule, threshold)),
                        "rule {rule_string} threshold {threshold:?} drift {drift:?}"
                    );
                }
            }
        }
    }

    /// Retained history never exceeds the lookback cap (the largest authored
    /// threshold, here 16) nor the number of zones seen, across 100_000 pushes.
    ///
    /// # Panics
    /// Panics when the history outgrows the cap or the number of zones seen.
    #[test]
    fn scan_history_is_capped_at_lookback() {
        let rule = parse_rule("8 16 4 8 2 4 1 1").expect("valid rule");
        let checks = WecoChecks::new(&rule, SpcAlertThreshold::Zone1);
        let mut scan = WecoScan::default();
        let mut max_retained = 0;
        for idx in 0..100_000usize {
            scan.push(&checks, (idx % 9) as i8 - 4, |_| {});
            assert!(scan.retained() <= idx + 1);
            max_retained = max_retained.max(scan.retained());
        }
        assert_eq!(max_retained, 16);
    }
}
